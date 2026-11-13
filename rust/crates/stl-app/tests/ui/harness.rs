//! `app_harness`: start `StlApp` with a fake `Platform` and a real `NativeLink`, load a
//! project, and wait until the engine is idle.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use stl_app::app::StlApp;
use stl_app::engine_link::native::NativeLink;
use stl_app::engine_link::{Command, EngineLink, Update};
use stl_app::platform::{Pending, PickedFile, Platform};
use stl_app::ui::ThemePref;

/// Hands out queued picks and records every save / open filter list.
#[derive(Clone, Default)]
pub struct FakePlatform {
    pub picks: Arc<Mutex<Vec<Option<PickedFile>>>>,
    pub saved: Arc<Mutex<Vec<(String, Vec<u8>)>>>,
    pub opens: Arc<Mutex<Vec<Vec<String>>>>,
}

impl Platform for FakePlatform {
    fn open_file(&self, filters: &[(&str, &[&str])]) -> Pending<Option<PickedFile>> {
        self.opens
            .lock()
            .unwrap()
            .push(filters.iter().flat_map(|(_, e)| e.iter().map(|s| s.to_string())).collect());
        let next = self.picks.lock().unwrap().pop().unwrap_or(None);
        Pending::ready(next)
    }

    fn save_file(&self, suggested: &str, bytes: Vec<u8>) -> Pending<Result<bool, String>> {
        self.saved.lock().unwrap().push((suggested.to_string(), bytes));
        Pending::ready(Ok(true))
    }

    fn now_iso_utc(&self) -> String {
        "2026-11-12T12:00:00".to_string()
    }

    fn is_web(&self) -> bool {
        false
    }
}

/// `EngineLink` that shares a `NativeLink` so the harness can call [`NativeLink::is_busy`].
struct SharedLink {
    inner: Arc<NativeLink>,
}

impl EngineLink for SharedLink {
    fn send(&self, c: Command) {
        self.inner.send(c);
    }
    fn poll(&self) -> Vec<Update> {
        self.inner.poll()
    }
    fn restart(&self) {
        self.inner.restart();
    }
}

/// Starts the app with a fake platform and a real native engine link. When `project` is set,
/// opens that file and steps until [`NativeLink::is_busy`] is false and every update has been
/// applied (no pending updates left in the link).
pub fn app_harness(project: Option<&Path>) -> Harness<'static, StlApp> {
    let platform = FakePlatform::default();
    let link = Arc::new(NativeLink::new());
    let ctx = egui::Context::default();
    let app = StlApp::new(&ctx, Box::new(platform), Box::new(SharedLink { inner: link.clone() }));

    if let Some(path) = project {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("project.stlx").to_owned();
        link.send(Command::Open { name, bytes });
    }

    let mut init = false;
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_accessibility_check(false).build_ui_state(
        move |ui, app: &mut StlApp| {
            if !init {
                init = true;
                stl_app::fonts::install(ui.ctx());
                app.set_theme(ui.ctx(), ThemePref::Dark);
            }
            app.layout(ui);
        },
        app,
    );

    settle(&mut h, &link);
    h
}

/// Step the harness until the engine is idle and a follow-up poll finds nothing new.
fn settle(h: &mut Harness<'static, StlApp>, link: &NativeLink) {
    let end = Instant::now() + Duration::from_secs(120);
    loop {
        h.run_steps(1);
        if !link.is_busy() {
            // Drain anything that landed as the last job finished.
            h.run_steps(1);
            if !link.is_busy() {
                // Confirm the link queue is empty without stealing from the next frame:
                // one more layout poll; if still idle and body state is stable, we're done.
                let before = h.state().bodies.len();
                h.run_steps(1);
                if !link.is_busy() && h.state().bodies.len() == before {
                    return;
                }
            }
        }
        assert!(Instant::now() < end, "engine never went idle");
        std::thread::sleep(Duration::from_millis(5));
    }
}
