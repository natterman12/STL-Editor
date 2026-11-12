//! The fast Support Fins host: the engine runs in the system web engine (WebView2 on Windows,
//! WebKitGTK on Linux) inside a hidden `wry` window.
//!
//! - One process-wide UI thread owns the `tao` event loop and every hidden window
//!   (`with_any_thread(true)`, so it needn't be the main thread; GTK also allows only one
//!   such thread per process). Hosts talk to it through an `EventLoopProxy`.
//! - The engine file, the page and each call's inputs are served through the custom
//!   protocol `sf://localhost/...`, so big base64 inputs are never pasted into a script.
//!   A call only evaluates `__sfRun(<seq>)`; the page fetches `call/<seq>`, runs it and posts
//!   the result back through `window.ipc`.
//! - Calls are serialized by the host's own lock, as 1.x serializes with `_lock`.
//! - Each call has a 120 s limit. A timeout, or the WebView2 browser process exiting (checked
//!   every 200 ms while waiting, on Windows), gives `Err(STOPPED)` and throws the webview
//!   away; the next call builds a fresh one. Nothing here panics or waits without a limit.
//! - If the UI thread itself ends (a panic in the platform code outside the per-event guard),
//!   it is marked dead; the call in flight gets `Err(STOPPED)` and the next one starts a new
//!   UI thread and webview. tao allows a new event loop on a new thread on Windows. On Linux GTK
//!   can only ever run on the first thread that started it, so there a second UI thread fails
//!   to start: calls keep returning `Err(STOPPED)` and a new `WebviewHost::start` returns
//!   `Err`, which makes `start_fins_host` fall back to Boa.
//! - The page may only ever be at `sf://localhost/` (the engine can be a downloaded update),
//!   and IPC messages that don't have the expected shape are ignored.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use stl_engine::{FinsHost, HostKind};
use tao::event::Event;
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget};
use tao::platform::run_return::EventLoopExtRunReturn;
use tao::window::{Window, WindowBuilder};
use wry::http::{Request, Response};
use wry::{WebContext, WebView, WebViewBuilder};

use super::STOPPED;

/// The limit for one engine call (spec section 2).
const CALL_LIMIT: Duration = Duration::from_secs(120);
/// How long a new webview may take to load the engine page.
const START_LIMIT: Duration = Duration::from_secs(60);
/// How often a waiting call checks that the browser process is still there.
const POLL: Duration = Duration::from_millis(200);

const PAGE: &str = r#"<!doctype html><html><head><meta charset="utf-8"><title>Support Fins</title></head><body>
<script src="engine.js"></script>
<script>
(function () {
  function post(o) { window.ipc.postMessage(JSON.stringify(o)); }
  var E = typeof SupportFinsEngine === 'undefined' ? null : SupportFinsEngine;
  window.__sfRun = function (seq) {
    fetch('call/' + seq).then(function (r) { return r.json(); }).then(function (q) {
      var out;
      try {
        if (q.op === 'has') {
          out = { seq: seq, ok: true, v: String(!!E && typeof E[q.name] === 'function') };
        } else {
          var fn = E && E[q.name];
          if (typeof fn !== 'function') throw new TypeError('SupportFinsEngine.' + q.name + ' is not a function');
          // `args` are the values themselves; `sargs` (an argument that wasn't valid JSON) are
          // JSON text for the page to parse, as it always did.
          var args = q.sargs ? q.sargs.map(function (a) { return JSON.parse(a); }) : q.args;
          var v = fn.apply(E, args);
          out = { seq: seq, ok: true, v: typeof v === 'string' ? v : (v === undefined ? 'null' : JSON.stringify(v)) };
        }
      } catch (e) {
        out = { seq: seq, ok: false, e: String(e) };
      }
      post(out);
    }).catch(function (e) { post({ seq: seq, ok: false, e: String(e) }); });
  };
  post({ ready: !!E });
})();
</script></body></html>"#;

/// From a webview's page (or the UI thread) to the host waiting on it.
enum Msg {
    Ready(bool),
    Done(u64, Result<String, String>),
}

/// What the custom protocol serves for one webview.
struct Shared {
    engine: Arc<Vec<u8>>,
    /// The call the page should fetch next: (seq, JSON request).
    pending: Mutex<Option<(u64, String)>>,
}

enum Ui {
    Create { id: u64, shared: Arc<Shared>, tx: Sender<Msg>, reply: Sender<Result<Option<u32>, String>> },
    Run { id: u64, seq: u64 },
    Destroy(u64),
    /// Ends the UI thread (tests only).
    Exit,
}

/// One live webview, owned by the UI thread.
struct Slot {
    webview: WebView,
    _window: Window,
    _context: WebContext,
    tx: Sender<Msg>,
    data_dir: PathBuf,
}

/// One UI thread. `alive` turns false when the thread ends, however it ends.
struct UiThread {
    proxy: Mutex<EventLoopProxy<Ui>>,
    alive: Arc<AtomicBool>,
}

impl UiThread {
    fn alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    fn send(&self, ev: Ui) -> Result<(), String> {
        if !self.alive() {
            return Err("the Support Fins window thread has stopped".into());
        }
        let p = self.proxy.lock().unwrap_or_else(|e| e.into_inner());
        p.send_event(ev).map_err(|_| {
            self.alive.store(false, Ordering::SeqCst);
            "the Support Fins window thread has stopped".to_string()
        })
    }
}

/// Sets `alive` to false when the UI thread returns or unwinds.
struct AliveGuard(Arc<AtomicBool>);

impl Drop for AliveGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

static UI: Mutex<Option<Arc<UiThread>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// The live UI thread, starting a new one when there is none or the last one has ended.
fn ui() -> Result<Arc<UiThread>, String> {
    let mut cur = UI.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(t) = cur.as_ref().filter(|t| t.alive()) {
        return Ok(t.clone());
    }
    let t = Arc::new(start_ui_thread()?);
    *cur = Some(t.clone());
    Ok(t)
}

fn profiles_dir() -> PathBuf {
    std::env::temp_dir().join("stl-editor-support-fins")
}

/// Removes profile folders (`<pid>-<id>`) left by app processes that are no longer running.
/// Best effort: anything that can't be read or removed is left alone.
fn clean_stale_profiles() {
    let Ok(entries) = std::fs::read_dir(profiles_dir()) else { return };
    let me = std::process::id();
    for e in entries.flatten() {
        let name = e.file_name();
        let Some(pid) = name.to_str().and_then(|n| n.split('-').next()).and_then(|p| p.parse::<u32>().ok()) else {
            continue;
        };
        if pid != me && !process_running(pid) {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

#[cfg(windows)]
fn process_running(pid: u32) -> bool {
    // SAFETY: OpenProcess / WaitForSingleObject / CloseHandle on a handle owned by this block.
    unsafe {
        let h = win::OpenProcess(win::SYNCHRONIZE, 0, pid);
        if h.is_null() {
            return false;
        }
        let running = win::WaitForSingleObject(h, 0) == win::WAIT_TIMEOUT;
        win::CloseHandle(h);
        running
    }
}

#[cfg(not(windows))]
fn process_running(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

fn start_ui_thread() -> Result<UiThread, String> {
    clean_stale_profiles();
    let alive = Arc::new(AtomicBool::new(true));
    let guard = AliveGuard(alive.clone());
    let (tx, rx) = channel::<Result<EventLoopProxy<Ui>, String>>();
    let spawned = std::thread::Builder::new().name("support-fins-webview".into()).spawn(move || {
        let _guard = guard;
        let made = std::panic::catch_unwind(|| {
            let mut b = EventLoopBuilder::<Ui>::with_user_event();
            #[cfg(windows)]
            {
                use tao::platform::windows::EventLoopBuilderExtWindows;
                b.with_any_thread(true);
            }
            #[cfg(target_os = "linux")]
            {
                use tao::platform::unix::EventLoopBuilderExtUnix;
                b.with_any_thread(true);
            }
            b.build()
        });
        let mut event_loop = match made {
            Ok(el) => el,
            Err(_) => {
                let _ = tx.send(Err("could not start the window thread".into()));
                return;
            }
        };
        let _ = tx.send(Ok(event_loop.create_proxy()));
        let mut slots: HashMap<u64, Slot> = HashMap::new();
        // run_return, not run: tao's `run` exits the whole process when its loop ends.
        event_loop.run_return(move |event, target, flow| {
            *flow = ControlFlow::Wait;
            if let Event::UserEvent(Ui::Exit) = event {
                slots.clear();
                *flow = ControlFlow::Exit;
            } else if let Event::UserEvent(ev) = event {
                // A panic in the platform code must not take the thread (and every host) down.
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handle(ev, target, &mut slots)));
            }
        });
    });
    if let Err(e) = spawned {
        return Err(format!("could not start the window thread: {e}"));
    }
    match rx.recv_timeout(START_LIMIT) {
        Ok(Ok(p)) => Ok(UiThread { proxy: Mutex::new(p), alive }),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("the window thread did not start".into()),
    }
}

fn handle(ev: Ui, target: &EventLoopWindowTarget<Ui>, slots: &mut HashMap<u64, Slot>) {
    match ev {
        Ui::Create { id, shared, tx, reply } => {
            let made = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| build(id, shared, tx, target)));
            let r = match made {
                Ok(Ok(slot)) => {
                    let pid = browser_pid(&slot.webview);
                    slots.insert(id, slot);
                    Ok(pid)
                }
                Ok(Err(e)) => Err(e),
                Err(_) => Err("the web engine crashed while starting".into()),
            };
            let _ = reply.send(r);
        }
        Ui::Run { id, seq } => {
            if let Some(slot) = slots.get(&id) {
                if let Err(e) = slot.webview.evaluate_script(&format!("__sfRun({seq})")) {
                    let _ = slot.tx.send(Msg::Done(seq, Err(format!("Error: {e}"))));
                }
            }
        }
        Ui::Exit => {}
        Ui::Destroy(id) => {
            if let Some(slot) = slots.remove(&id) {
                let dir = slot.data_dir.clone();
                drop(slot);
                let _ = std::fs::remove_dir_all(dir);
            }
        }
    }
}

fn build(id: u64, shared: Arc<Shared>, tx: Sender<Msg>, target: &EventLoopWindowTarget<Ui>) -> Result<Slot, String> {
    let window = WindowBuilder::new()
        .with_title("Support Fins")
        .with_visible(false)
        .build(target)
        .map_err(|e| format!("could not make the hidden window: {e}"))?;
    // Each host gets its own profile folder, so it gets its own browser process: one host
    // crashing (or being killed) never takes another down.
    let data_dir = profiles_dir().join(format!("{}-{id}", std::process::id()));
    let mut context = WebContext::new(Some(data_dir.clone()));
    let ipc_tx = tx.clone();
    let builder = WebViewBuilder::new_with_web_context(&mut context)
        .with_visible(false)
        .with_custom_protocol("sf".into(), move |_id, req| serve(&shared, req))
        .with_navigation_handler(|url| page_url(&url))
        .with_ipc_handler(move |req: Request<String>| {
            if !page_url(&req.uri().to_string()) {
                return;
            }
            if let Some(m) = parse_ipc(req.body()) {
                let _ = ipc_tx.send(m);
            }
        })
        .with_url("sf://localhost/index.html");
    #[cfg(windows)]
    let webview = {
        use wry::WebViewBuilderExtWindows;
        // The window is hidden, so by default Chromium treats the page as backgrounded and
        // throttles its renderer (the engine ran ~1.7x slower). A call is CPU-bound and must
        // run at full speed, so turn the backgrounding off.
        builder
            .with_additional_browser_args(
                "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,CalculateNativeWinOcclusion                  --disable-background-timer-throttling --disable-renderer-backgrounding                  --disable-backgrounding-occluded-windows",
            )
            .build(&window)
    };
    #[cfg(target_os = "linux")]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        match window.default_vbox() {
            Some(vbox) => builder.build_gtk(vbox),
            None => return Err("the hidden window has no GTK box".into()),
        }
    };
    let webview = webview.map_err(|e| format!("could not start the web engine: {e}"))?;
    Ok(Slot { webview, _window: window, _context: context, tx, data_dir })
}

fn serve(shared: &Shared, req: Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    let path = req.uri().path().trim_start_matches('/');
    let ok = |ty: &str, body: Cow<'static, [u8]>| {
        Response::builder()
            .header("Content-Type", ty)
            .header("Cache-Control", "no-store")
            .body(body)
            .unwrap_or_else(|_| Response::new(Cow::Borrowed(&[][..])))
    };
    match path {
        "" | "index.html" => ok("text/html; charset=utf-8", Cow::Borrowed(PAGE.as_bytes())),
        "engine.js" => ok("text/javascript; charset=utf-8", Cow::Owned(shared.engine.as_ref().clone())),
        p if p.starts_with("call/") => {
            let seq: Option<u64> = p["call/".len()..].parse().ok();
            let mut pending = shared.pending.lock().unwrap_or_else(|e| e.into_inner());
            match pending.take() {
                Some((s, body)) if Some(s) == seq => ok("application/json", Cow::Owned(body.into_bytes())),
                other => {
                    *pending = other;
                    not_found()
                }
            }
        }
        _ => not_found(),
    }
}

fn not_found() -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(404)
        .body(Cow::Borrowed(&[][..]))
        .unwrap_or_else(|_| Response::new(Cow::Borrowed(&[][..])))
}

/// Our own page: `sf://localhost/...` (Linux), shown as `http(s)://sf.localhost/...` by WebView2.
fn page_url(url: &str) -> bool {
    ["sf://localhost/", "http://sf.localhost/", "https://sf.localhost/"].iter().any(|p| url.starts_with(p))
}

/// The page's two messages, exactly: `{"ready": bool}`, or `{"seq": n, "ok": true, "v": str}` /
/// `{"seq": n, "ok": false, "e": str}`. Anything else is ignored.
fn parse_ipc(body: &str) -> Option<Msg> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let o = v.as_object()?;
    if o.len() == 1 {
        return Some(Msg::Ready(o.get("ready")?.as_bool()?));
    }
    if o.len() != 3 {
        return None;
    }
    let seq = o.get("seq")?.as_u64()?;
    let r = if o.get("ok")?.as_bool()? {
        Ok(o.get("v")?.as_str()?.to_string())
    } else {
        Err(o.get("e")?.as_str()?.to_string())
    };
    Some(Msg::Done(seq, r))
}

// ---- the browser process (Windows): its id, and whether it has exited

#[cfg(windows)]
fn browser_pid(webview: &WebView) -> Option<u32> {
    use wry::WebViewExtWindows;
    let mut pid = 0u32;
    // SAFETY: a plain COM getter writing one u32 on the thread that owns the webview.
    let ok = unsafe { webview.webview().BrowserProcessId(&mut pid) }.is_ok();
    (ok && pid != 0).then_some(pid)
}

#[cfg(not(windows))]
fn browser_pid(_webview: &WebView) -> Option<u32> {
    None
}

/// A handle on the browser process that says when it has gone. On Linux there is none, so
/// a dead WebKit process is caught by the call limit instead.
struct ProcWatch {
    #[cfg(windows)]
    handle: usize,
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        pub fn WaitForSingleObject(handle: *mut c_void, ms: u32) -> u32;
        pub fn CloseHandle(handle: *mut c_void) -> i32;
    }
    pub const SYNCHRONIZE: u32 = 0x0010_0000;
    pub const WAIT_TIMEOUT: u32 = 0x102;
}

impl ProcWatch {
    #[cfg(windows)]
    fn new(pid: Option<u32>) -> Self {
        // SAFETY: OpenProcess with a pid; a null result just means "can't watch".
        let h = pid.map(|p| unsafe { win::OpenProcess(win::SYNCHRONIZE, 0, p) } as usize).unwrap_or(0);
        ProcWatch { handle: h }
    }
    #[cfg(not(windows))]
    fn new(_pid: Option<u32>) -> Self {
        ProcWatch {}
    }
    #[cfg(windows)]
    fn exited(&self) -> bool {
        // SAFETY: the handle is ours until Drop; WaitForSingleObject(…, 0) never blocks.
        self.handle != 0 && unsafe { win::WaitForSingleObject(self.handle as *mut _, 0) } != win::WAIT_TIMEOUT
    }
    #[cfg(not(windows))]
    fn exited(&self) -> bool {
        false
    }
}

impl Drop for ProcWatch {
    fn drop(&mut self) {
        #[cfg(windows)]
        if self.handle != 0 {
            // SAFETY: closing the handle OpenProcess gave us, once.
            unsafe { win::CloseHandle(self.handle as *mut _) };
        }
    }
}

// ---- the host

struct Live {
    id: u64,
    ui: Arc<UiThread>,
    shared: Arc<Shared>,
    rx: Receiver<Msg>,
    pid: Option<u32>,
    watch: ProcWatch,
}

impl Drop for Live {
    fn drop(&mut self) {
        let _ = self.ui.send(Ui::Destroy(self.id));
    }
}

struct State {
    live: Option<Live>,
    seq: u64,
}

pub struct WebviewHost {
    engine: Arc<Vec<u8>>,
    state: Mutex<State>,
}

impl WebviewHost {
    /// Starts a hidden webview with the engine loaded. Fails (never panics) when the system
    /// web engine is missing or can't load the file; the caller then uses Boa.
    pub fn start(engine_js: &Path) -> Result<Self, String> {
        let engine = std::fs::read(engine_js).map_err(|e| format!("could not read {}: {e}", engine_js.display()))?;
        let host = WebviewHost { engine: Arc::new(engine), state: Mutex::new(State { live: None, seq: 0 }) };
        let live = host.spawn()?;
        host.state.lock().unwrap_or_else(|e| e.into_inner()).live = Some(live);
        Ok(host)
    }

    /// The WebView2 browser process of the current webview (Windows; `None` elsewhere).
    pub fn browser_pid(&self) -> Option<u32> {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).live.as_ref().and_then(|l| l.pid)
    }

    /// Ends the UI thread, as if it had crashed, and waits until it has gone (tests only).
    #[doc(hidden)]
    pub fn stop_ui_thread_for_tests() {
        let cur = UI.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(t) = cur {
            let _ = t.send(Ui::Exit);
            let deadline = Instant::now() + Duration::from_secs(10);
            while t.alive() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }

    fn spawn(&self) -> Result<Live, String> {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let shared = Arc::new(Shared { engine: self.engine.clone(), pending: Mutex::new(None) });
        let (tx, rx) = channel();
        let (reply_tx, reply_rx) = channel();
        let ui = ui()?;
        ui.send(Ui::Create { id, shared: shared.clone(), tx, reply: reply_tx })?;
        let pid = match reply_rx.recv_timeout(START_LIMIT) {
            Ok(Ok(pid)) => pid,
            Ok(Err(e)) => return Err(e),
            Err(_) => {
                let _ = ui.send(Ui::Destroy(id));
                return Err("the web engine did not start in time".into());
            }
        };
        let live = Live { id, ui, shared, rx, pid, watch: ProcWatch::new(pid) };
        let deadline = Instant::now() + START_LIMIT;
        loop {
            match live.rx.recv_timeout(POLL) {
                Ok(Msg::Ready(true)) => return Ok(live),
                Ok(Msg::Ready(false)) => return Err("the web engine could not load the Support Fins engine".into()),
                Ok(Msg::Done(..)) => {}
                Err(RecvTimeoutError::Disconnected) => return Err("the web engine stopped while starting".into()),
                Err(RecvTimeoutError::Timeout) => {
                    if live.watch.exited() || !live.ui.alive() || Instant::now() > deadline {
                        return Err("the web engine did not load the page in time".into());
                    }
                }
            }
        }
    }

    fn request(&self, body: String) -> Result<String, String> {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if st.live.is_none() {
            st.live = Some(self.spawn().map_err(|_| STOPPED.to_string())?);
        }
        st.seq += 1;
        let seq = st.seq;
        let live = st.live.as_ref().expect("just made");
        *live.shared.pending.lock().unwrap_or_else(|e| e.into_inner()) = Some((seq, body));
        let outcome = if live.watch.exited() || live.ui.send(Ui::Run { id: live.id, seq }).is_err() {
            None
        } else {
            let deadline = Instant::now() + CALL_LIMIT;
            loop {
                match live.rx.recv_timeout(POLL) {
                    Ok(Msg::Done(s, r)) if s == seq => break Some(r),
                    Ok(_) => {}
                    Err(RecvTimeoutError::Disconnected) => break None,
                    Err(RecvTimeoutError::Timeout) => {
                        if live.watch.exited() || !live.ui.alive() || Instant::now() > deadline {
                            break None;
                        }
                    }
                }
            }
        };
        match outcome {
            Some(r) => r,
            None => {
                // Dead or stuck: throw this webview away; the next call builds a fresh one.
                st.live = None;
                Err(STOPPED.to_string())
            }
        }
    }
}

/// Is `a` valid JSON text? Big arguments (the base64 soup of a part is tens of MB) are checked
/// by one quick scan when they are a plain string; anything else gets a real parse.
fn valid_json(a: &str) -> bool {
    let b = a.as_bytes();
    if b.len() > 4096 && b.len() >= 2 && b[0] == b'"' && b[b.len() - 1] == b'"' {
        if b[1..b.len() - 1].iter().all(|&c| c != b'"' && c != 92 && c >= 0x20) {
            return true;
        }
    }
    serde_json::from_str::<serde_json::Value>(a).is_ok()
}

/// The request for `call`. Each argument is JSON text, so it is put into the request as it is
/// (the page then gets the value without parsing a copy of a tens-of-MB string again, and the
/// host doesn't escape one). An argument that isn't valid JSON is sent as text for the page to
/// parse, so it fails there as it always did.
fn call_body(name: &str, args: &[&str]) -> String {
    let name = serde_json::to_string(name).unwrap_or_else(|_| "\"\"".into());
    let cap = 64 + name.len() + args.iter().map(|a| a.len() + 2).sum::<usize>();
    let mut out = String::with_capacity(cap);
    out.push_str("{\"op\":\"call\",\"name\":");
    out.push_str(&name);
    if args.iter().all(|a| valid_json(a)) {
        out.push_str(",\"args\":[");
        for (i, a) in args.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(a);
        }
        out.push_str("]}");
    } else {
        out.push_str(",\"sargs\":");
        out.push_str(&serde_json::to_string(args).unwrap_or_else(|_| "[]".into()));
        out.push('}');
    }
    out
}

impl FinsHost for WebviewHost {
    fn call(&self, name: &str, args: &[&str]) -> Result<String, String> {
        self.request(call_body(name, args))
    }

    fn has(&self, name: &str) -> bool {
        matches!(self.request(serde_json::json!({ "op": "has", "name": name }).to_string()).as_deref(), Ok("true"))
    }

    fn host_kind(&self) -> HostKind {
        HostKind::Webview
    }
}

#[cfg(test)]
mod body_tests {
    use super::*;

    #[test]
    fn call_body_embeds_json_args_and_falls_back() {
        let b: serde_json::Value = serde_json::from_str(&call_body("f", &["\"a\\\"b\"", "12", "{\"x\":[1]}"])).unwrap();
        assert_eq!(b["args"], serde_json::json!(["a\"b", 12, {"x": [1]}]));
        let b: serde_json::Value = serde_json::from_str(&call_body("f", &["\"ok\"", "{bad"])).unwrap();
        assert_eq!(b["sargs"], serde_json::json!(["\"ok\"", "{bad"]));
        let big = format!("\"{}\"", "A".repeat(10_000));
        let b: serde_json::Value = serde_json::from_str(&call_body("f", &[&big])).unwrap();
        assert_eq!(b["args"][0].as_str().unwrap().len(), 10_000);
        let weird = format!("\"{}\u{1}\"", "A".repeat(10_000));
        let b: serde_json::Value = serde_json::from_str(&call_body("f", &[&weird])).unwrap();
        assert!(b.get("sargs").is_some());
    }
}
