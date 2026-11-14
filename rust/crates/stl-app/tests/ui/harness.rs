//! `app_harness`: start `StlApp` with a fake `Platform` and a real `NativeLink`, load a
//! project, and wait until the engine is idle.
// Shared by several test files (ui, projects, fins_in_app); each uses only some helpers.
#![allow(dead_code)]

// keeps every test binary that uses this harness out of the real 2.0 folders
#[path = "../support/test_env.rs"]
mod test_env;

use std::path::{Path, PathBuf};
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
    /// The small remembered settings (what the browser keeps in `localStorage`).
    pub settings: Arc<Mutex<std::collections::BTreeMap<String, String>>>,
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

    fn setting(&self, key: &str) -> Option<String> {
        self.settings.lock().unwrap().get(key).cloned()
    }

    fn set_setting(&self, key: &str, value: &str) {
        self.settings.lock().unwrap().insert(key.to_owned(), value.to_owned());
    }
}

/// `EngineLink` that shares a `NativeLink` so the harness can call [`NativeLink::is_busy`].
pub struct SharedLink {
    pub inner: Arc<NativeLink>,
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

pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

pub fn fixture_stl(name: &str) -> PathBuf {
    repo().join("tests/ui/fixtures").join(name)
}

pub fn project(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/projects").join(name)
}

/// Starts the app with a fake platform and a real native engine link. When `project` is set,
/// opens that file and steps until [`NativeLink::is_busy`] is false and every update has been
/// applied (no pending updates left in the link).
pub fn app_harness(project: Option<&Path>) -> Harness<'static, StlApp> {
    app_harness_ex(project, egui::vec2(1280.0, 800.0)).0
}

/// Like [`app_harness`], but returns the shared engine link so callers can [`settle`] again
/// after sending more commands.
pub fn app_harness_ex(project: Option<&Path>, size: egui::Vec2) -> (Harness<'static, StlApp>, Arc<NativeLink>) {
    app_harness_with(project, size, FakePlatform::default())
}

/// [`app_harness_ex`] on a platform the caller keeps a handle to (its settings, saved files).
pub fn app_harness_with(project: Option<&Path>, size: egui::Vec2, platform: FakePlatform) -> (Harness<'static, StlApp>, Arc<NativeLink>) {
    let link = Arc::new(NativeLink::new());
    let ctx = egui::Context::default();
    let app = StlApp::new(&ctx, Box::new(platform), Box::new(SharedLink { inner: link.clone() }));

    if let Some(path) = project {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("project.stlx").to_owned();
        link.send(Command::Open { name, bytes });
    }

    let mut init = false;
    let mut h = Harness::builder().with_size(size).with_accessibility_check(false).build_ui_state(
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
    (h, link)
}

/// Empty project (start page), then settle.
pub fn empty_harness() -> (Harness<'static, StlApp>, Arc<NativeLink>) {
    app_harness_ex(None, egui::vec2(1280.0, 800.0))
}

/// Import an STL through the real engine and wait until a body appears.
pub fn import_stl(h: &mut Harness<'static, StlApp>, link: &NativeLink, path: &Path) {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("part.stl").to_owned();
    h.state_mut().link.send(Command::Import { name, bytes });
    settle(h, link);
    assert!(!h.state().bodies.is_empty(), "import produced no body ({})", path.display());
}

/// Select the first body (if any).
pub fn select_first_body(h: &mut Harness<'static, StlApp>) {
    if let Some(id) = h.state().bodies.first().map(|b| b.id.clone()) {
        h.state_mut().shell.selection = vec![id];
    }
}

/// Open a tool and step a few frames so the panel paints.
pub fn open_tool(h: &mut Harness<'static, StlApp>, key: &str) -> bool {
    let ok = h.state_mut().open_tool(key, None);
    h.run_steps(4);
    ok
}

/// Wait until Support Fins schema is installed (and the tool panel opens if waiting).
pub fn wait_support_open(h: &mut Harness<'static, StlApp>, link: &NativeLink, key: &str) {
    let end = Instant::now() + Duration::from_secs(60);
    if !stl_app::supports::is_installed() {
        let _ = h.state_mut().open_tool(key, None);
    } else if h.state().tool().is_none() {
        let _ = h.state_mut().open_tool(key, None);
    }
    while Instant::now() < end {
        settle(h, link);
        h.run_steps(2);
        if stl_app::supports::is_installed() && h.state().tool().is_some_and(|t| t.def.key == key) {
            return;
        }
        if !stl_app::supports::is_installed() {
            let _ = h.state_mut().open_tool(key, None);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("Support tool {key} never opened (installed={})", stl_app::supports::is_installed());
}

/// Step the harness until the engine is idle and a follow-up poll finds nothing new.
pub fn settle(h: &mut Harness<'static, StlApp>, link: &NativeLink) {
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
                    // the viewport prepares bodies on worker threads: wait until they are on
                    // screen and pickable (a slow CI machine can otherwise pick nothing)
                    h.state_mut().viewport.flush();
                    h.run_steps(1);
                    return;
                }
            }
        }
        assert!(Instant::now() < end, "engine never went idle");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Step until `ready` is true (the engine's answers arrive on their own time).
pub fn wait_for(h: &mut Harness<'static, StlApp>, link: &NativeLink, what: &str, ready: impl Fn(&mut Harness<'static, StlApp>) -> bool) {
    let end = Instant::now() + Duration::from_secs(90);
    loop {
        settle(h, link);
        h.run_steps(2);
        if ready(h) {
            return;
        }
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(30));
    }
}

/// Primary click at a screen position (after the viewport has flushed).
pub fn click_at(h: &mut Harness<'static, StlApp>, at: egui::Pos2) {
    use egui::{Event, PointerButton};
    h.event(Event::PointerMoved(at));
    h.step();
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Default::default() });
    h.step();
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Default::default() });
    h.run_steps(2);
}

/// Double-click at a screen position.
pub fn dblclick_at(h: &mut Harness<'static, StlApp>, at: egui::Pos2) {
    use egui::{Event, PointerButton};
    for _ in 0..2 {
        h.event(Event::PointerMoved(at));
        h.step();
        h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Default::default() });
        h.step();
        h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Default::default() });
        h.step();
    }
    h.run_steps(2);
}

/// Screen position of a sketch-plane UV point (needs an open sketch and a flushed viewport).
pub fn sketch_uv_screen(h: &Harness<'static, StlApp>, uv: [f64; 2]) -> egui::Pos2 {
    let s = h.state().sketch().expect("a sketch is open");
    h.state().viewport.world_to_screen(s.to_3(uv))
}

/// Click a sketch-plane UV after flushing the viewport.
pub fn sketch_click_uv(h: &mut Harness<'static, StlApp>, uv: [f64; 2]) {
    h.state_mut().viewport.flush();
    h.run_steps(1);
    let at = sketch_uv_screen(h, uv);
    click_at(h, at);
}

/// Enter a sketch on an origin plane (or face JSON) and wait for the ribbon.
pub fn enter_sketch_plane(h: &mut Harness<'static, StlApp>, link: &NativeLink, plane: serde_json::Value) {
    h.state_mut().enter_sketch(Some(plane), None);
    settle(h, link);
    h.state_mut().viewport.flush();
    h.run_steps(2);
    assert!(h.state().sketch().is_some(), "sketch opened");
}

/// Add a closed centre rectangle to the open sketch (entities only; no pointer path).
pub fn sketch_add_rect(h: &mut Harness<'static, StlApp>, id: &str, x: f64, y: f64, w: f64, hgt: f64) {
    let m = h.state_mut().sketch_mode_mut().expect("sketch open");
    m.s.push();
    m.s.entities.push(serde_json::from_value(serde_json::json!({
        "id": id, "type": "rect", "x": x, "y": y, "w": w, "h": hgt
    })).unwrap());
}

/// Finish the sketch (and optionally open Extrude); settle until the sketch closes.
pub fn finish_sketch_settle(h: &mut Harness<'static, StlApp>, link: &NativeLink, extrude: bool) {
    h.state_mut().finish_sketch(extrude);
    settle(h, link);
    wait_for(h, link, "sketch to close", |h| h.state().sketch().is_none());
}

/// The volume of a closed mesh (mm³), from signed tetrahedra.
pub fn mesh_volume(m: &stl_kernel::MeshArrays) -> f64 {
    let p = |i: u32| {
        let k = i as usize * 3;
        [m.positions[k] as f64, m.positions[k + 1] as f64, m.positions[k + 2] as f64]
    };
    m.triangles
        .chunks(3)
        .map(|t| {
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])) / 6.0
        })
        .sum::<f64>()
        .abs()
}
