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
    }

    impl eframe::App for WebShell {
        fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
            self.app.ui(ui, frame);
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
                        Ok(Box::new(WebShell { app, track }))
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
