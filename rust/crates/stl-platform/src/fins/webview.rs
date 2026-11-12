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
use std::collections::hash_map::RandomState;
use std::collections::{HashMap, VecDeque};
use std::hash::{BuildHasher, Hasher};
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
/// An argument longer than this (the triangle soup of a part) is kept in the page.
const BIG: usize = 1_000_000;

const PAGE: &str = r#"<!doctype html><html><head><meta charset="utf-8"><title>Support Fins</title></head><body>
<script src="engine.js"></script>
<script>
(function () {
  function post(o) { window.ipc.postMessage(JSON.stringify(o)); }
  var E = typeof SupportFinsEngine === 'undefined' ? null : SupportFinsEngine;
  // Big arguments (a part's triangle soup) kept between calls, under the host's key for them.
  // The host says what to keep and drop; a reload empties it, and a missing key is reported
  // so the host sends the whole call again.
  var store = {};
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
          var st = q.store, i;
          if (st) {
            if (st.reset) store = {};
            st.drop.forEach(function (k) { delete store[k]; });
            for (i in st.refs) {
              if (!Object.prototype.hasOwnProperty.call(store, st.refs[i])) { post({ seq: seq, miss: st.refs[i] }); return; }
              args[i] = store[st.refs[i]];
            }
            for (i in st.keep) store[st.keep[i]] = args[i];
          }
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
    /// The page doesn't hold a kept argument the call refers to.
    Miss(u64),
}

/// What the custom protocol serves for one webview.
struct Shared {
    engine: Arc<Vec<u8>>,
    /// The call the page should fetch next: (seq, JSON request).
    pending: Mutex<Option<(u64, String)>>,
    /// How many call requests over [`BIG`] bytes the page has fetched (tests count them).
    big_sends: AtomicU64,
}

enum Ui {
    Create { id: u64, shared: Arc<Shared>, tx: Sender<Msg>, reply: Sender<Result<Option<u32>, String>> },
    Run { id: u64, seq: u64 },
    /// Reloads the page, as a renderer reset would (tests only).
    #[cfg(test)]
    Reload(u64),
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
        #[cfg(test)]
        Ui::Reload(id) => {
            if let Some(slot) = slots.get(&id) {
                let _ = slot.webview.evaluate_script("location.reload()");
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
                Some((s, body)) if Some(s) == seq => {
                    if body.len() > BIG {
                        shared.big_sends.fetch_add(1, Ordering::SeqCst);
                    }
                    ok("application/json", Cow::Owned(body.into_bytes()))
                }
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

/// The page's messages, exactly: `{"ready": bool}`, `{"seq": n, "miss": key}`, or `{"seq": n, "ok": true, "v": str}` /
/// `{"seq": n, "ok": false, "e": str}`. Anything else is ignored.
fn parse_ipc(body: &str) -> Option<Msg> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let o = v.as_object()?;
    if o.len() == 1 {
        return Some(Msg::Ready(o.get("ready")?.as_bool()?));
    }
    if o.len() == 2 {
        o.get("miss")?.as_str()?;
        return Some(Msg::Miss(o.get("seq")?.as_u64()?));
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
    /// The keys the page holds, least recently used first.
    store: PageStore,
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
    /// Keys the hash of a big argument (random per host).
    hasher: RandomState,
}

/// The arguments kept in the page: at most [`STORE_MAX`] keys, the least recently used first.
#[derive(Default)]
struct PageStore {
    keys: VecDeque<String>,
}

const STORE_MAX: usize = 6;

impl PageStore {
    fn holds(&self, k: &str) -> bool {
        self.keys.iter().any(|x| x == k)
    }

    /// Marks `k` (held) as just used.
    fn touch(&mut self, k: &str) {
        if let Some(i) = self.keys.iter().position(|x| x == k) {
            let k = self.keys.remove(i).expect("found");
            self.keys.push_back(k);
        }
    }

    /// Adds `k`, putting the keys dropped to make room into `dropped`.
    fn add(&mut self, k: &str, dropped: &mut Vec<String>) {
        self.keys.push_back(k.to_string());
        while self.keys.len() > STORE_MAX {
            dropped.extend(self.keys.pop_front());
        }
    }
}

/// One request, before it is turned into the JSON the page fetches.
enum Query<'a> {
    Has(&'a str),
    /// The name, the arguments, and each big argument's key.
    Call(&'a str, &'a [&'a str], Vec<Option<String>>),
}

impl WebviewHost {
    /// Starts a hidden webview with the engine loaded. Fails (never panics) when the system
    /// web engine is missing or can't load the file; the caller then uses Boa.
    pub fn start(engine_js: &Path) -> Result<Self, String> {
        let engine = std::fs::read(engine_js).map_err(|e| format!("could not read {}: {e}", engine_js.display()))?;
        let host = WebviewHost {
            engine: Arc::new(engine),
            state: Mutex::new(State { live: None, seq: 0 }),
            hasher: RandomState::new(),
        };
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

    /// How many requests over 1 MB the current page has fetched (tests only).
    #[cfg(test)]
    fn big_sends(&self) -> u64 {
        let st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        st.live.as_ref().map_or(0, |l| l.shared.big_sends.load(Ordering::SeqCst))
    }

    /// Reloads the page and waits until it is ready again (tests only). The host is not told,
    /// so the next call finds the page's store empty.
    #[cfg(test)]
    fn reload_for_tests(&self) {
        let st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let live = st.live.as_ref().expect("a live webview");
        live.ui.send(Ui::Reload(live.id)).unwrap();
        let deadline = Instant::now() + START_LIMIT;
        loop {
            match live.rx.recv_timeout(POLL) {
                Ok(Msg::Ready(true)) => return,
                _ => assert!(Instant::now() < deadline, "the page did not reload"),
            }
        }
    }

    fn spawn(&self) -> Result<Live, String> {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let shared = Arc::new(Shared { engine: self.engine.clone(), pending: Mutex::new(None), big_sends: AtomicU64::new(0) });
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
        let live = Live { id, ui, shared, rx, pid, watch: ProcWatch::new(pid), store: PageStore::default() };
        let deadline = Instant::now() + START_LIMIT;
        loop {
            match live.rx.recv_timeout(POLL) {
                Ok(Msg::Ready(true)) => return Ok(live),
                Ok(Msg::Ready(false)) => return Err("the web engine could not load the Support Fins engine".into()),
                Ok(Msg::Done(..) | Msg::Miss(_)) => {}
                Err(RecvTimeoutError::Disconnected) => return Err("the web engine stopped while starting".into()),
                Err(RecvTimeoutError::Timeout) => {
                    if live.watch.exited() || !live.ui.alive() || Instant::now() > deadline {
                        return Err("the web engine did not load the page in time".into());
                    }
                }
            }
        }
    }

    /// The key of a big argument: a 64-bit hash (keyed at random per host) and the length.
    fn key_of(&self, a: &str) -> String {
        let mut h = self.hasher.build_hasher();
        h.write(a.as_bytes());
        format!("{:016x}-{}", h.finish(), a.len())
    }

    fn request(&self, q: Query) -> Result<String, String> {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if st.live.is_none() {
            st.live = Some(self.spawn().map_err(|_| STOPPED.to_string())?);
        }
        // A call the page can't run (a kept argument is missing) is sent once more, whole.
        let mut reset = false;
        loop {
            st.seq += 1;
            let seq = st.seq;
            let live = st.live.as_mut().expect("just made");
            let body = match &q {
                Query::Has(b) => b.to_string(),
                Query::Call(name, args, keys) => call_body(name, args, keys, &mut live.store, reset),
            };
            *live.shared.pending.lock().unwrap_or_else(|e| e.into_inner()) = Some((seq, body));
            // Some(Some(result)), Some(None) for a miss, None for a dead or stuck page
            let outcome = if live.watch.exited() || live.ui.send(Ui::Run { id: live.id, seq }).is_err() {
                None
            } else {
                let deadline = Instant::now() + CALL_LIMIT;
                loop {
                    match live.rx.recv_timeout(POLL) {
                        Ok(Msg::Done(s, r)) if s == seq => break Some(Some(r)),
                        Ok(Msg::Miss(s)) if s == seq => break Some(None),
                        // the page loaded again: what it kept is gone
                        Ok(Msg::Ready(_)) => live.store = PageStore::default(),
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
                Some(Some(r)) => return r,
                Some(None) if !reset => {
                    live.store = PageStore::default();
                    reset = true;
                }
                // can't happen: a reset call refers to nothing kept
                Some(None) => return Err(STOPPED.to_string()),
                None => {
                    // Dead or stuck: throw this webview away; the next call builds a fresh one.
                    st.live = None;
                    return Err(STOPPED.to_string());
                }
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
///
/// A big argument (one with a key in `keys`) that the page already holds goes as `null` plus
/// its key in `store.refs`; one it doesn't hold goes whole and is kept (`store.keep`, dropping
/// the least recently used keys past [`STORE_MAX`] in `store.drop`). `reset` empties the page's
/// store first. `store` is updated to what the page will hold.
fn call_body(name: &str, args: &[&str], keys: &[Option<String>], store: &mut PageStore, reset: bool) -> String {
    let name = serde_json::to_string(name).unwrap_or_else(|_| "\"\"".into());
    let key = |i: usize| keys.get(i).and_then(Option::as_deref);
    // A held argument was valid JSON when it was kept (same key, same text).
    let held: Vec<bool> = (0..args.len()).map(|i| key(i).is_some_and(|k| store.holds(k))).collect();
    let mut out = String::new();
    if !args.iter().zip(&held).all(|(a, &h)| h || valid_json(a)) {
        out.push_str("{\"op\":\"call\",\"name\":");
        out.push_str(&name);
        out.push_str(",\"sargs\":");
        out.push_str(&serde_json::to_string(args).unwrap_or_else(|_| "[]".into()));
        out.push('}');
        return out;
    }
    let cap = 160 + name.len() + args.iter().zip(&held).map(|(a, &h)| if h { 40 } else { a.len() + 40 }).sum::<usize>();
    out.reserve(cap);
    out.push_str("{\"op\":\"call\",\"name\":");
    out.push_str(&name);
    out.push_str(",\"args\":[");
    let (mut refs, mut keep, mut drop) = (Vec::new(), Vec::new(), Vec::new());
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        match key(i) {
            Some(k) if held[i] => {
                store.touch(k);
                out.push_str("null");
                refs.push(format!("\"{i}\":\"{k}\""));
            }
            Some(k) => {
                out.push_str(a);
                store.add(k, &mut drop);
                keep.push(format!("\"{i}\":\"{k}\""));
            }
            None => out.push_str(a),
        }
    }
    out.push(']');
    if reset || !refs.is_empty() || !keep.is_empty() {
        // keys are hex digits, a dash and digits: no escaping needed
        let drop: Vec<String> = drop.iter().map(|k| format!("\"{k}\"")).collect();
        out.push_str(&format!(
            ",\"store\":{{\"reset\":{reset},\"drop\":[{}],\"refs\":{{{}}},\"keep\":{{{}}}}}",
            drop.join(","),
            refs.join(","),
            keep.join(",")
        ));
    }
    out.push('}');
    out
}

impl FinsHost for WebviewHost {
    fn call(&self, name: &str, args: &[&str]) -> Result<String, String> {
        let keys = args.iter().map(|a| (a.len() > BIG).then(|| self.key_of(a))).collect();
        self.request(Query::Call(name, args, keys))
    }

    fn has(&self, name: &str) -> bool {
        let q = serde_json::json!({ "op": "has", "name": name }).to_string();
        matches!(self.request(Query::Has(&q)).as_deref(), Ok("true"))
    }

    fn host_kind(&self) -> HostKind {
        HostKind::Webview
    }
}

#[cfg(test)]
mod body_tests {
    use super::*;

    fn call_body(name: &str, args: &[&str]) -> String {
        super::call_body(name, args, &vec![None; args.len()], &mut PageStore::default(), false)
    }

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

#[cfg(test)]
mod store_tests {
    use super::*;

    /// A stand-in engine: `sum(s)` answers the length of `s` and a checksum of its characters.
    const FAKE: &str = "var SupportFinsEngine = { sum: function (s, k) { var h = 0;         for (var i = 0; i < s.length; i++) { h = (h * 31 + s.charCodeAt(i)) % 1000003; }         return s.length + ':' + h + ':' + k; } };";

    fn host(name: &str) -> WebviewHost {
        let p = std::env::temp_dir().join(format!("stl-fins-fake-{}-{name}.js", std::process::id()));
        std::fs::write(&p, FAKE).unwrap();
        WebviewHost::start(&p).expect("the webview host should start on this machine")
    }

    /// A 2 MB JSON string argument, not all the same character.
    fn big(seed: u8) -> String {
        let a = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut s = String::with_capacity(2_000_002);
        s.push('"');
        for i in 0..2_000_000usize {
            s.push(a[(i * 7 + i / 64 + seed as usize) % 64] as char);
        }
        s.push('"');
        s
    }

    fn want(arg: &str, k: &str) -> String {
        let mut h: u64 = 0;
        for c in arg[1..arg.len() - 1].bytes() {
            h = (h * 31 + c as u64) % 1000003;
        }
        format!("{}:{h}:{k}", arg.len() - 2)
    }

    #[test]
    fn large_arg_sent_once() {
        let h = host("once");
        let s = big(0);
        let t = Instant::now();
        assert_eq!(h.call("sum", &[&s, "\"a\""]).unwrap(), want(&s, "a"));
        let first = t.elapsed();
        let t = Instant::now();
        assert_eq!(h.call("sum", &[&s, "\"b\""]).unwrap(), want(&s, "b"));
        println!("large_arg_sent_once: first call {first:?}, second {:?}", t.elapsed());
        assert_eq!(h.big_sends(), 1, "the 2 MB argument should cross sf:// once");
        // a different big argument is sent, and both stay usable
        let s2 = big(1);
        assert_eq!(h.call("sum", &[&s2, "\"c\""]).unwrap(), want(&s2, "c"));
        assert_eq!(h.call("sum", &[&s, "\"d\""]).unwrap(), want(&s, "d"));
        assert_eq!(h.big_sends(), 2);
    }

    #[test]
    fn large_arg_after_reload() {
        let h = host("reload");
        let s = big(2);
        assert_eq!(h.call("sum", &[&s, "1"]).unwrap(), want(&s, "1"));
        assert_eq!(h.big_sends(), 1);
        h.reload_for_tests();
        assert_eq!(h.call("sum", &[&s, "2"]).unwrap(), want(&s, "2"));
        assert_eq!(h.big_sends(), 2, "after a reload the argument is sent again");
        assert_eq!(h.call("sum", &[&s, "3"]).unwrap(), want(&s, "3"));
        assert_eq!(h.big_sends(), 2, "and then kept again");
    }

    #[test]
    fn store_keeps_six() {
        let h = host("six");
        let args: Vec<String> = (0..7).map(|i| big(10 + i)).collect();
        for a in &args {
            assert_eq!(h.call("sum", &[a, "0"]).unwrap(), want(a, "0"));
        }
        assert_eq!(h.big_sends(), 7);
        // the newest six are kept; the oldest was dropped and is sent again
        assert_eq!(h.call("sum", &[&args[6], "0"]).unwrap(), want(&args[6], "0"));
        assert_eq!(h.big_sends(), 7);
        assert_eq!(h.call("sum", &[&args[0], "0"]).unwrap(), want(&args[0], "0"));
        assert_eq!(h.big_sends(), 8);
    }

    /// The real engine, with a part whose soup is over 1 MB: kept arguments give the same answer.
    #[test]
    fn large_arg_real_engine() {
        let engine = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../web/vendor/support-fins/engine.iife.js");
        let h = WebviewHost::start(&engine).expect("the webview host should start on this machine");
        // a 100 x 100 mm plate, 5 mm thick, 10 mm above the bed: its underside is an overhang
        let n = 90usize;
        let mut bytes = Vec::new();
        let mut tri = |p: [[f64; 3]; 3]| {
            for v in p {
                for x in v {
                    bytes.extend_from_slice(&x.to_le_bytes());
                }
            }
        };
        let s = 100.0 / n as f64;
        for i in 0..n {
            for k in 0..n {
                let (x0, y0, x1, y1) = (i as f64 * s - 50.0, k as f64 * s - 50.0, (i + 1) as f64 * s - 50.0, (k + 1) as f64 * s - 50.0);
                for z in [10.0, 15.0] {
                    let (a, b, c, d) = ([x0, y0, z], [x1, y0, z], [x1, y1, z], [x0, y1, z]);
                    if z > 12.0 {
                        tri([a, b, c]);
                        tri([a, c, d]);
                    } else {
                        tri([a, c, b]);
                        tri([a, d, c]);
                    }
                }
            }
        }
        // a small foot at the far corner, so the plate sits above the bed
        tri([[45.0, 45.0, 0.0], [45.0, 47.0, 0.0], [47.0, 47.0, 0.0]]);
        tri([[45.0, 45.0, 0.0], [47.0, 47.0, 0.0], [47.0, 45.0, 0.0]]);
        let soup = format!("\"{}\"", stl_engine::features::common::fins::b64_encode(&bytes));
        assert!(soup.len() > BIG);
        let opts = serde_json::to_string(&h.call("optionsFromDialogJson", &["\"{}\""]).unwrap()).unwrap();
        let wall = |a: &str, b: &str| {
            let t = Instant::now();
            let r = h.call("drawWallB64", &[&soup, &serde_json::to_string(a).unwrap(), &serde_json::to_string(b).unwrap(), &opts]).unwrap();
            (r, t.elapsed())
        };
        let (r1, t1) = wall("[-20,0,10]", "[20,0,10]");
        let (r2, t2) = wall("[0,-20,10]", "[0,20,10]");
        let (r3, t3) = wall("[-20,0,10]", "[20,0,10]");
        println!("large_arg_real_engine: soup {} bytes; cold {t1:?}, warm {t2:?}, {t3:?}", soup.len());
        assert_eq!(r1, r3, "the same arguments give the same answer");
        assert_ne!(r1, r2);
        assert_eq!(h.big_sends(), 1, "the soup crosses sf:// once");
    }
}
