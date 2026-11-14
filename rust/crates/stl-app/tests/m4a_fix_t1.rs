//! 4a fix, task 1: the title after every open path, swatches, the Parameters list, About credits.
#![cfg(not(target_arch = "wasm32"))]

// keeps this test binary out of the real 2.0 folders (STLEDITOR2_CONFIG_DIR)
#[path = "support/test_env.rs"]
mod test_env;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use stl_app::app::{about_text, StlApp};
use stl_app::engine_link::native::NativeLink;
use stl_app::engine_link::{BodyView, Command, EngineLink, ParamView, Update};
use stl_app::platform::{Pending, PickedFile, Platform};
use stl_app::ui::browser::{swatch_rgb, Browser, BrowserView};
use stl_app::ui::{ThemePref, UiAction};
use stl_kernel::MeshArrays;

fn fixture(name: &str) -> Vec<u8> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/projects").join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// The real engine, shared with the app so the test can ask whether it is busy.
#[derive(Clone)]
struct SharedLink(Arc<NativeLink>);
impl EngineLink for SharedLink {
    fn send(&self, c: Command) {
        self.0.send(c);
    }
    fn poll(&self) -> Vec<Update> {
        self.0.poll()
    }
    fn restart(&self) {
        self.0.restart();
    }
}

/// An open dialog that hands out the queued files, one per dialog.
struct Picks(Mutex<Vec<PickedFile>>);
impl Platform for Picks {
    fn open_file(&self, _f: &[(&str, &[&str])]) -> Pending<Option<PickedFile>> {
        let mut q = self.0.lock().unwrap();
        Pending::ready((!q.is_empty()).then(|| q.remove(0)))
    }
    fn save_file(&self, _s: &str, _b: Vec<u8>) -> Pending<Result<bool, String>> {
        Pending::ready(Ok(false))
    }
    fn now_iso_utc(&self) -> String {
        String::new()
    }
    fn is_web(&self) -> bool {
        false
    }
}

struct Rig {
    h: Harness<'static, StlApp>,
    link: SharedLink,
}

fn rig(picks: Vec<PickedFile>) -> Rig {
    let link = SharedLink(Arc::new(NativeLink::new()));
    let app = StlApp::new(&egui::Context::default(), Box::new(Picks(Mutex::new(picks))), Box::new(link.clone()));
    let mut init = false;
    let h = Harness::builder().with_size(egui::vec2(1500.0, 700.0)).with_accessibility_check(false).build_ui_state(
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
    Rig { h, link }
}

impl Rig {
    /// Run frames until the engine has finished everything it was sent.
    fn settle(&mut self) {
        let end = Instant::now() + Duration::from_secs(60);
        loop {
            self.h.run_steps(2);
            if !self.link.0.is_busy() {
                self.h.run_steps(3);
                if !self.link.0.is_busy() {
                    return;
                }
            }
            assert!(Instant::now() < end, "the engine never finished");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn title(&self) -> String {
        self.h.state().shell.doc_name.clone()
    }

    fn assert_title_clean(&self, want: &str, what: &str) {
        assert_eq!(self.title(), want, "{what}: title");
        assert!(!self.h.state().shell.dirty, "{what}: the project is clean (no •)");
    }
}

fn basic() -> PickedFile {
    PickedFile { name: "basic.stlx".into(), bytes: fixture("basic.stlx") }
}

#[test]
fn title_after_each_open_path() {
    // File > Open (Ctrl+O): the file name, as 1.x shows it, and clean
    let mut r = rig(vec![basic()]);
    r.h.run_steps(3);
    r.h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::O);
    r.settle();
    assert!(!r.h.state().timeline_view.items.is_empty(), "the project opened");
    r.assert_title_clean("basic.stlx", "File > Open");

    // a drag and drop: 1.x refuses a project (the title stays) and inserts anything else
    let mut out = Default::default();
    r.h.state_mut().files.accept_dropped(&*r.link.0, "other.stlx".into(), fixture("extra_keys.stlx"), &mut out);
    r.settle();
    r.assert_title_clean("basic.stlx", "a dropped project is refused");

    // New, then Recent: the file name again, and clean
    r.h.state_mut().shell.file_requests.push(UiAction::New);
    r.settle();
    assert_eq!(r.title(), "Untitled");
    r.h.state_mut().shell.file_requests.push(UiAction::OpenRecent("basic.stlx".into()));
    r.settle();
    assert!(!r.h.state().timeline_view.items.is_empty());
    r.assert_title_clean("basic.stlx", "Recent");
}

#[test]
fn title_after_a_startup_argument() {
    let dir = std::env::temp_dir().join(format!("stl-fix-t1-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("launch.stlx");
    std::fs::write(&path, fixture("basic.stlx")).unwrap();
    let mut r = rig(Vec::new());
    r.h.run_steps(2);
    r.h.state_mut().open_launch_file(&path);
    r.settle();
    assert!(!r.h.state().timeline_view.items.is_empty());
    r.assert_title_clean("launch.stlx", "startup argument");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn failed_open_keeps_title() {
    let bad = PickedFile { name: "bad.stlx".into(), bytes: b"not a project".to_vec() };
    let mut r = rig(vec![basic(), bad]);
    r.h.run_steps(3);
    r.h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::O);
    r.settle();
    r.assert_title_clean("basic.stlx", "first open");
    // change something so the project is dirty, then fail an open (File > Open asks first)
    r.link.send(Command::AddFeature { kind: "primitive".into(), params: serde_json::json!({"shape": "box", "size": [3.0, 3.0, 3.0]}), index: None, name: None });
    r.settle();
    assert!(r.h.state().shell.dirty, "an added step makes the project dirty");
    r.h.state_mut().shell.file_requests.push(UiAction::Open);
    r.h.run_steps(3);
    r.h.get_by_label("OK").click();
    r.settle();
    assert_eq!(r.title(), "basic.stlx", "a failed open keeps the title");
    assert!(r.h.state().shell.dirty, "a failed open keeps the dirty state");
    assert_eq!(r.h.state().files.recent_names(), vec!["basic.stlx".to_string()], "and Recent");
    // a failed startup argument changes nothing either
    let dir = std::env::temp_dir().join(format!("stl-fix-t1-bad-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("broken.stlx");
    std::fs::write(&path, b"nope").unwrap();
    r.h.state_mut().open_launch_file(&path);
    r.settle();
    assert_eq!(r.title(), "basic.stlx");
    assert!(r.h.state().shell.dirty, "a failed startup argument keeps the dirty state");
    std::fs::remove_dir_all(&dir).ok();
}

fn body(id: &str, color: Option<&str>) -> BodyView {
    BodyView {
        id: id.into(),
        name: id.into(),
        color: color.map(str::to_owned),
        hidden: false,
        solid: true,
        mesh: Arc::new(MeshArrays { positions: Vec::new(), triangles: Vec::new() }),
        revision: 0,
    }
}

#[test]
fn swatch_matches_viewport_colour() {
    use stl_app::viewport::{body_color, scene};
    let mut cases: Vec<(BodyView, usize)> = Vec::new();
    for (i, c) in [Some("#112233"), Some("#C9A66B"), None, Some("red"), Some("#9aa7b4")].into_iter().enumerate() {
        cases.push((body("a", c), i));
    }
    for i in 0..scene::PALETTE.len() + 2 {
        cases.push((body("a", None), i));
    }
    let mut not_solid = body("a", Some("#112233"));
    not_solid.solid = false;
    cases.push((not_solid.clone(), 0));
    not_solid.color = None;
    cases.push((not_solid, 3));
    for (b, i) in &cases {
        assert_eq!(swatch_rgb(b, *i), body_color(b, *i), "{:?} at {i}", b.color);
    }
    // the cases the plan names, spelled out
    assert_eq!(swatch_rgb(&body("a", Some("#112233")), 0), [0x11, 0x22, 0x33]);
    assert_eq!(swatch_rgb(&body("a", None), 1), scene::rgb(scene::PALETTE[1]));
    assert_ne!(swatch_rgb(&body("a", None), 0), swatch_rgb(&body("a", None), 1));
    let mut bad = body("a", Some("#112233"));
    bad.solid = false;
    assert_eq!(swatch_rgb(&bad, 0), [0xd9, 0xa0, 0xa0], "a body that isn't watertight is pink, as in the viewport");
}

struct TreeUi {
    browser: Browser,
    params: Vec<ParamView>,
}

#[test]
fn params_listed_expanded() {
    let p = |name: &str, v: f64| ParamView { name: name.into(), expr: v.to_string(), note: String::new(), value: Some(v), error: None, used: false };
    let mut h = Harness::builder().with_size(egui::vec2(280.0, 320.0)).with_accessibility_check(false).build_ui_state(
        |ui, st: &mut TreeUi| {
            stl_app::fonts::install(ui.ctx());
            let tk = stl_app::theme::tokens(true);
            let ctx = ui.ctx().clone();
            let view = BrowserView { doc_name: "Doc", bodies: &[], selection: &[], origin_visible: true, planes: &[], sketches: &[], params: &st.params };
            stl_app::ui::browser::show(ui, &ctx, &tk, &mut st.browser, view, &mut Vec::new(), &mut Vec::new());
        },
        TreeUi { browser: Browser::default(), params: vec![p("dx", 5.0), p("spare", 1.0), p("half", 2.5)] },
    );
    h.run();
    // expanded by default, without anything being clicked
    h.get_by_label("dx = 5");
    h.get_by_label("spare = 1");
    h.get_by_label("half = 2.5");
    // a click on a row opens the table
    h.get_by_label("dx = 5").click();
    h.run();
    assert!(h.state_mut().browser.take_open_params());
}

/// The licence of each credit, as the crates' own `Cargo.toml` files in the registry say
/// (`license = ...`): the About text has to say exactly this.
const EXPECTED: &[(&str, &str)] = &[
    ("manifold-rust", "Apache-2.0"),
    ("egui and eframe", "MIT or Apache-2.0"),
    ("wgpu", "MIT or Apache-2.0"),
    ("Selawik", "SIL Open Font Licence 1.1"),
    ("Noto Emoji", "SIL Open Font Licence 1.1"),
    ("rayon, ttf-parser, serde", "MIT or Apache-2.0"),
    ("zip, quick-xml", "MIT"),
    ("usvg, resvg", "Apache-2.0 or MIT"),
];

#[test]
fn about_lists_rust_crates_only() {
    for web in [false, true] {
        let text = about_text(web);
        let t = text.to_lowercase();
        for gone in ["numpy", "trimesh", "pywebview", "manifold3d", "shapely", "scipy", "scikit", "python", "three.js", "opentype", "fflate", "licenses folder"] {
            assert!(!t.contains(gone), "web={web}: the About text still names {gone}");
        }
        assert!(t.contains("cad-style editing for stl, 3mf and obj meshes"));
        assert!(t.contains("you can buy me a coffee on ko-fi"));
        // each credit sits on its own line and ends with exactly its licence
        for (name, lic) in EXPECTED {
            let line = text.lines().find(|l| l.contains(name)).unwrap_or_else(|| panic!("web={web}: no credit line names {name}"));
            assert!(line.ends_with(&format!("({lic})")), "web={web}: {line:?} should end with ({lic})");
        }
    }
    // the table in the app is the table above, nothing more
    assert_eq!(stl_app::app::CREDITS.len(), EXPECTED.len());
    for ((what, lic), (name, want)) in stl_app::app::CREDITS.iter().zip(EXPECTED) {
        assert!(what.contains(name) && lic == want, "{what}: {lic}");
    }
    // every crate named is one the workspace really depends on
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let app = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    for dep in ["egui", "eframe", "wgpu"] {
        assert!(app.contains(&format!("
{dep} = ")), "{dep} is in stl-app's Cargo.toml");
    }
    let rd = |c: &str| std::fs::read_to_string(dir.join("..").join(c).join("Cargo.toml")).unwrap();
    assert!(rd("stl-kernel").contains("manifold-rust"));
    assert!(rd("stl-io").contains("quick-xml") && rd("stl-io").contains("zip"));
    assert!(rd("stl-sketch").contains("ttf-parser") && rd("stl-sketch").contains("usvg"));
    assert!(rd("stl-mesh").contains("rayon") && rd("stl-engine").contains("serde"));
}
