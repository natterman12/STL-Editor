#[cfg(not(target_arch = "wasm32"))]
use stl_app::app::StlApp;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    // Before eframe or the engine starts a thread. On Windows this thread runs above normal.
    // The engine thread builds Rayon's pool and runs below this one. See `engine_link::native`.
    stl_app::engine_link::native::reserve_core_for_the_window();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some((out, quick)) = stl_app::bench::parse_args(args.iter().cloned()) {
        let launched = std::time::Instant::now();
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_title("STL Editor (uibench)").with_inner_size([1280.0, 800.0]),
            ..Default::default()
        };
        return eframe::run_native(
            "STL Editor",
            options,
            Box::new(move |cc| Ok(Box::new(stl_app::bench::BenchApp::new(cc, out, quick, launched)))),
        );
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("STL Editor").with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    // `stl-app <file>`: a .stlx is opened, any other mesh is inserted.
    let launch_file = args.into_iter().find(|a| !a.starts_with('-'));
    eframe::run_native(
        "STL Editor",
        options,
        Box::new(move |cc| {
            // The GPU device comes up before the engine thread, so driver threads are not
            // part of the engine's pool.
            let mut viewport = stl_app::viewport::Viewport::new();
            if let Some(rs) = cc.wgpu_render_state.as_ref() {
                viewport.init_gpu(rs);
            }
            let ctx = cc.egui_ctx.clone();
            let link = stl_app::engine_link::native::NativeLink::new();
            let wake = ctx.clone();
            link.set_waker(move || wake.request_repaint());
            let wake = ctx.clone();
            let mut app = StlApp::new(
                &cc.egui_ctx,
                Box::new(stl_app::platform::native::NativePlatform::with_waker(move || wake.request_repaint())),
                Box::new(link),
            );
            app.viewport = viewport;
            if let Some(path) = launch_file {
                app.open_launch_file(std::path::Path::new(&path));
            }
            Ok(Box::new(app))
        }),
    )
}

#[cfg(target_arch = "wasm32")]
mod web_main {
    use std::cell::Cell;

    use stl_app::app::StlApp;
    use stl_app::engine_link::web::WebLink;
    use stl_app::engine_link::{Command, EngineLink, NullLink};
    use stl_app::platform::web::WebPlatform;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    thread_local! {
        /// Bodies in the egui browser tree. `-1` until an opened project has drawn.
        static BROWSER_BODIES: Cell<i32> = const { Cell::new(-1) };
    }

    /// How many bodies the running app's browser tree is showing.
    #[wasm_bindgen]
    pub fn stl_browser_body_count() -> i32 {
        BROWSER_BODIES.with(|c| c.get())
    }

    thread_local! {
        /// Backend, pick and section result for the headless check (`?report=1&full=1`).
        /// Empty until the check has run.
        static CHECK_STATUS: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
        /// What the page's `__stlGrab` returned for the last pixel readback.
        static GRAB: std::cell::RefCell<Option<Result<serde_json::Value, String>>> = const { std::cell::RefCell::new(None) };
    }

    /// `ok:<backend>:<details>` or `error:<why>`; empty while the check runs.
    #[wasm_bindgen]
    pub fn stl_check_status() -> String {
        CHECK_STATUS.with(|c| c.borrow().clone())
    }

    fn set_check_status(s: String) {
        CHECK_STATUS.with(|c| *c.borrow_mut() = s);
    }

    /// Read the viewport's pixels back through the page (`__stlGrab`). It runs as a microtask,
    /// after the frame that is being painted now has been drawn, while the canvas still holds it.
    fn request_grab(label: &'static str, ctx: &egui::Context, rect: egui::Rect) {
        let ppp = ctx.pixels_per_point();
        let (x, y) = ((rect.min.x * ppp).round().max(0.0), (rect.min.y * ppp).round().max(0.0));
        let (w, h) = ((rect.width() * ppp).round().max(1.0), (rect.height() * ppp).round().max(1.0));
        GRAB.with(|g| *g.borrow_mut() = None);
        wasm_bindgen_futures::spawn_local(async move {
            let result = (|| {
                let win = web_sys::window().ok_or("no window")?;
                let f = js_sys::Reflect::get(&win, &JsValue::from_str("__stlGrab")).map_err(|_| "the page has no __stlGrab".to_string())?;
                let f = f.dyn_into::<js_sys::Function>().map_err(|_| "the page has no __stlGrab".to_string())?;
                let args = js_sys::Array::of5(
                    &JsValue::from_str(label),
                    &JsValue::from_f64(x as f64),
                    &JsValue::from_f64(y as f64),
                    &JsValue::from_f64(w as f64),
                    &JsValue::from_f64(h as f64),
                );
                let out = f.apply(&win, &args).map_err(|e| format!("reading the canvas failed: {e:?}"))?;
                let text = out.as_string().ok_or("__stlGrab returned no text")?;
                serde_json::from_str::<serde_json::Value>(&text).map_err(|e| e.to_string())
            })();
            GRAB.with(|g| *g.borrow_mut() = Some(result));
        });
    }

    fn page_param(name: &str) -> Option<String> {
        let search = web_sys::window()?.location().search().ok()?;
        for pair in search.trim_start_matches('?').split('&') {
            let mut it = pair.splitn(2, '=');
            if it.next() == Some(name) {
                let raw = it.next().unwrap_or("");
                let decoded = js_sys::decode_uri_component(raw).ok().and_then(|s| s.as_string()).unwrap_or_else(|| raw.to_string());
                if !decoded.is_empty() {
                    return Some(decoded);
                }
            }
        }
        None
    }

    /// The headless check, once the project has drawn:
    /// 1. every body the fixture has is in the viewport, and a pick at the block's projected
    ///    centre hits the block;
    /// 2. read the viewport's pixels with the section view off, turn it on, read them again.
    ///    They must differ in the middle of the view and the section frame must not be blank;
    /// 3. keep drawing for a while and fail on any GPU error that shows up late.
    #[derive(Default)]
    enum Check {
        #[default]
        Waiting,
        /// Frames to wait before the "off" readback.
        PreOff(u32, String),
        AwaitOff(String),
        /// `renders` before the section was turned on; frames since it drew.
        Section { renders: u64, drawn: Option<u32>, frames: u32, details: String },
        AwaitOn(String),
        Settle { frames: u32, since: web_time::Instant, details: String },
        Done,
    }

    fn fail(check: &mut Check, backend: &str, why: &str) {
        set_check_status(format!("error:{why} ({backend})"));
        *check = Check::Done;
    }

    /// Pick at the projected centre of `id`'s box; if that misses (a hole, or another body in
    /// front), try a grid inside the box. `Some("centre" | "scan")` when `id` was hit.
    fn pick_body(app: &StlApp, id: &str) -> Option<&'static str> {
        use stl_app::viewport::pick::Hit;
        let mesh = app.viewport.body_mesh(id)?;
        let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
        for p in mesh.positions.chunks_exact(3) {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k] as f64);
                hi[k] = hi[k].max(p[k] as f64);
            }
        }
        let hits = |pos: egui::Pos2| matches!(app.viewport.pick(pos, true, false), Some(Hit::Body { id: got, .. }) if got == id);
        let mid = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
        if hits(app.viewport.world_to_screen(mid)) {
            return Some("centre");
        }
        let mut min = egui::pos2(f32::MAX, f32::MAX);
        let mut max = egui::pos2(f32::MIN, f32::MIN);
        for i in 0..8 {
            let c = [if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { lo[2] } else { hi[2] }];
            let s = app.viewport.world_to_screen(c);
            min = min.min(s);
            max = max.max(s);
        }
        for iy in 1..24 {
            for ix in 1..24 {
                let pos = egui::pos2(min.x + (max.x - min.x) * ix as f32 / 24.0, min.y + (max.y - min.y) * iy as f32 / 24.0);
                if hits(pos) {
                    return Some("scan");
                }
            }
        }
        None
    }

    fn run_check(check: &mut Check, app: &mut StlApp, backend: &str, ctx: &egui::Context) {
        let stats = app.viewport.stats_handle();
        if !matches!(check, Check::Done) {
            ctx.request_repaint();
            if stats.gpu_errors() > 0 || stats.device_lost() {
                return fail(check, backend, "the GPU reported an error");
            }
        }
        let rect = app.viewport.rect();
        match std::mem::take(check) {
            Check::Done => *check = Check::Done,
            Check::Waiting => {
                let ready = !app.bodies.is_empty()
                    && stats.renders() >= 2
                    && app.bodies.iter().all(|b| app.viewport.shown_revision(&b.id).is_some());
                if !ready {
                    *check = Check::Waiting;
                    return;
                }
                let expect: Vec<String> = page_param("bodies").unwrap_or_default().split(',').filter(|s| !s.is_empty()).map(str::to_owned).collect();
                let block = page_param("block").unwrap_or_default();
                let shown: Vec<String> = app.bodies.iter().map(|b| b.id.clone()).collect();
                if expect.is_empty() || block.is_empty() || !expect.contains(&block) {
                    return fail(check, backend, "the page URL names no fixture bodies (bodies=, block=)");
                }
                if let Some(missing) = expect.iter().find(|e| !shown.contains(e)) {
                    return fail(check, backend, &format!("the fixture body {missing} is not in the viewport (viewport has {})", shown.join(",")));
                }
                // Only a pick at the block's projected centre counts. A hit found by scanning the
                // box (or no hit) means the projection or the picking is off.
                let how = match pick_body(app, &block) {
                    Some("centre") => "centre",
                    Some(_) => return fail(check, backend, &format!("a pick at the centre of the block {block} missed it; only a scan of its box hit it")),
                    None => return fail(check, backend, &format!("no pick hit the block {block}, not even a scan of its box")),
                };
                let others: Vec<String> = expect
                    .iter()
                    .filter(|e| **e != block)
                    .map(|e| format!("{e}:{}", pick_body(app, e).unwrap_or("miss")))
                    .collect();
                let details = format!(
                    "bodies={};block={block}@{how}{}",
                    expect.join(","),
                    if others.is_empty() { String::new() } else { format!(";others={}", others.join(",")) }
                );
                *check = Check::PreOff(4, details);
            }
            Check::PreOff(n, details) => {
                if n > 0 {
                    *check = Check::PreOff(n - 1, details);
                } else {
                    request_grab("off", ctx, rect);
                    *check = Check::AwaitOff(details);
                }
            }
            Check::AwaitOff(details) => match GRAB.with(|g| g.borrow_mut().take()) {
                None => *check = Check::AwaitOff(details),
                Some(Err(e)) => fail(check, backend, &e),
                Some(Ok(off)) => {
                    let mode = off["mode"].as_f64().unwrap_or(1.0);
                    if mode > 0.995 || off["distinct"].as_u64().unwrap_or(0) < 4 {
                        return fail(check, backend, "the viewport read back blank before the section view was turned on");
                    }
                    app.viewport.set_section_enabled(true);
                    *check = Check::Section { renders: stats.renders(), drawn: None, frames: 0, details };
                }
            },
            Check::Section { renders, mut drawn, mut frames, details } => {
                frames += 1;
                if drawn.is_none() && stats.renders() > renders && app.viewport.active_section().is_some() {
                    drawn = Some(0);
                }
                if let Some(d) = drawn {
                    // A few frames after the section render, so the picture on the canvas is the cut one.
                    if d >= 4 {
                        request_grab("on", ctx, rect);
                        *check = Check::AwaitOn(details);
                        return;
                    }
                    drawn = Some(d + 1);
                }
                if frames > 900 {
                    return fail(check, backend, "the section view did not draw");
                }
                *check = Check::Section { renders, drawn, frames, details };
            }
            Check::AwaitOn(details) => match GRAB.with(|g| g.borrow_mut().take()) {
                None => *check = Check::AwaitOn(details),
                Some(Err(e)) => fail(check, backend, &e),
                Some(Ok(on)) => {
                    let mode = on["mode"].as_f64().unwrap_or(1.0);
                    let distinct = on["distinct"].as_u64().unwrap_or(0);
                    let differ = on["differ"].as_i64().unwrap_or(-1);
                    if mode > 0.995 || distinct < 4 {
                        return fail(check, backend, "the section view read back blank");
                    }
                    if differ < 200 {
                        return fail(
                            check,
                            backend,
                            &format!("the section view looks the same as the view without it ({differ} pixels differ in the middle of the view)"),
                        );
                    }
                    let details = format!("{details};section differs in {differ} px of the view, {distinct} colours");
                    *check = Check::Settle { frames: 0, since: web_time::Instant::now(), details };
                }
            },
            Check::Settle { frames, since, details } => {
                // Keep drawing with the section view on: WebGPU reports some errors a few frames late.
                if frames >= 60 && since.elapsed().as_millis() >= 1000 {
                    set_check_status(format!("ok:{backend}:{details}"));
                    *check = Check::Done;
                } else {
                    *check = Check::Settle { frames: frames + 1, since, details };
                }
            }
        }
    }

    fn set_check_error(msg: &str) {
        if let Some(win) = web_sys::window() {
            let _ = js_sys::Reflect::set(&win, &JsValue::from_str("__STL_CHECK_ERROR__"), &JsValue::from_str(msg));
        }
    }

    fn page_project() -> Option<String> {
        let search = web_sys::window()?.location().search().ok()?;
        for pair in search.trim_start_matches('?').split('&') {
            let mut it = pair.splitn(2, '=');
            if it.next() == Some("project") {
                let raw = it.next().unwrap_or("");
                let decoded = js_sys::decode_uri_component(raw).ok().and_then(|s| s.as_string()).unwrap_or_else(|| raw.to_string());
                if !decoded.is_empty() {
                    return Some(decoded);
                }
            }
        }
        None
    }

    fn file_name(url: &str) -> String {
        url.split('?')
            .next()
            .unwrap_or(url)
            .split('/')
            .next_back()
            .filter(|s| !s.is_empty())
            .unwrap_or("project.stlx")
            .to_string()
    }

    async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
        let win = web_sys::window().ok_or_else(|| "This page has no window.".to_string())?;
        let fetch = js_sys::Reflect::get(&win, &JsValue::from_str("fetch")).map_err(|_| "This page has no fetch.".to_string())?;
        let fetch = fetch.dyn_into::<js_sys::Function>().map_err(|_| "This page has no fetch.".to_string())?;
        let prom = fetch.call1(&win, &JsValue::from_str(url)).map_err(|_| format!("Could not fetch {url}."))?;
        let resp = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(prom))
            .await
            .map_err(|_| format!("Could not fetch {url}."))?;
        let ok = js_sys::Reflect::get(&resp, &JsValue::from_str("ok")).ok().and_then(|v| v.as_bool()).unwrap_or(false);
        if !ok {
            return Err(format!("Could not fetch {url}."));
        }
        let ab = js_sys::Reflect::get(&resp, &JsValue::from_str("arrayBuffer")).map_err(|_| format!("Could not read {url}."))?;
        let ab = ab.dyn_into::<js_sys::Function>().map_err(|_| format!("Could not read {url}."))?;
        let buf_prom = ab.call0(&resp).map_err(|_| format!("Could not read {url}."))?;
        let buf = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(buf_prom))
            .await
            .map_err(|_| format!("Could not read {url}."))?;
        let bytes_js = js_sys::Uint8Array::new(&buf);
        let mut bytes = vec![0u8; bytes_js.length() as usize];
        bytes_js.copy_to(&mut bytes);
        Ok(bytes)
    }

    struct WebShell {
        app: StlApp,
        /// Publish the browser-tree count once a project opened from the page URL has bodies.
        track: bool,
        backend: String,
        check: Check,
    }

    impl eframe::App for WebShell {
        fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
            self.app.ui(ui, frame);
            if self.track {
                run_check(&mut self.check, &mut self.app, &self.backend, ui.ctx());
            }
            // The browser panel draws `app.bodies`. Publish that length, not a second counter.
            if self.track && !self.app.bodies.is_empty() {
                let n = self.app.bodies.len();
                BROWSER_BODIES.with(|c| c.set(n as i32));
            }
        }
    }

    pub fn start() {
        wasm_bindgen_futures::spawn_local(async {
            let project = page_project();
            let opened = match &project {
                Some(url) => match fetch_bytes(url).await {
                    Ok(bytes) => Some((file_name(url), bytes)),
                    Err(e) => {
                        set_check_error(&e);
                        None
                    }
                },
                None => None,
            };
            let Some(canvas) = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.get_element_by_id("stl-canvas"))
                .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
            else {
                set_check_error("the page needs a canvas with id stl-canvas");
                return;
            };
            let result = eframe::WebRunner::new()
                .start(
                    canvas,
                    eframe::WebOptions::default(),
                    Box::new(move |cc| {
                        let ctx = cc.egui_ctx.clone();
                        let (link, track): (Box<dyn EngineLink>, bool) = match WebLink::new() {
                            Ok(link) => {
                                link.set_waker(move || ctx.request_repaint());
                                if let Some((name, bytes)) = opened.clone() {
                                    link.send(Command::Open { name, bytes });
                                }
                                (Box::new(link), opened.is_some())
                            }
                            Err(e) => {
                                set_check_error(&e);
                                (Box::new(NullLink), false)
                            }
                        };
                        let mut app = StlApp::new(&cc.egui_ctx, Box::new(WebPlatform::new()), link);
                        if let Some(rs) = cc.wgpu_render_state.as_ref() {
                            app.viewport.init_gpu(rs);
                        }
                        let backend = cc
                            .wgpu_render_state
                            .as_ref()
                            .map(|rs| format!("{:?}", rs.adapter.get_info().backend))
                            .unwrap_or_else(|| "none".into());
                        Ok(Box::new(WebShell { app, track, backend, check: Check::default() }))
                    }),
                )
                .await;
            if let Err(e) = result {
                set_check_error(&format!("{e:?}"));
            }
        });
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {
    web_main::start();
}
