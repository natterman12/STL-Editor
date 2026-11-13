//! M4b side-by-side 2.0 screenshots (`dev/out/m4b-shots/2x/<theme>/<state>.png`).
//!
//! Ignored by default: needs a GPU. Run with:
//! `cargo test -p stl-app --test m4b_shots -- --ignored`
//!
//! 1.x pairs come from `node dev/m4b_shots/capture_m4b_shots.mjs`.
//! States not built yet in 2.0 are skipped and listed (not faked).
#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use stl_app::app::StlApp;
use stl_app::engine_link::native::NativeLink;
use stl_app::engine_link::{Command, EngineLink, Update};
use stl_app::platform::{Pending, PickedFile, Platform};
use stl_app::ui::ThemePref;
use stl_app::viewport::renderer::Headless;

#[path = "support/wgpu_renderer.rs"]
mod wgpu_renderer;

const SIZE: egui::Vec2 = egui::vec2(1280.0, 800.0);

/// Same state names as `dev/m4b_shots/capture_m4b_shots.mjs`.
const TOOL_STATES: &[&str] = &[
    "tool_extrude",
    "tool_revolve",
    "tool_loft",
    "tool_sweep",
    "tool_primitive_box",
    "tool_primitive_cylinder",
    "tool_primitive_sphere",
    "tool_primitive_cone",
    "tool_primitive_torus",
    "tool_snapfit",
    "tool_hinge",
    "tool_hole",
    "tool_combine",
    "tool_split",
    "tool_bedsplit",
    "tool_plate",
    "tool_layflat",
    "tool_thread",
    "tool_offset",
    "tool_thincheck",
    "tool_fittest",
    "tool_resizehole",
    "tool_pushpull",
    "tool_mirror",
    "tool_move",
    "tool_scale",
    "tool_pattern",
    "tool_hollow",
    "tool_lattice",
    "tool_shell",
    "tool_plane_offset",
    "tool_plane_angle",
    "tool_plane_3pt",
    "tool_plane_mid",
    "tool_import",
    "tool_sculpt",
    "tool_remesh",
    "tool_repair",
    "tool_simplify",
    "tool_separate",
    "tool_delete",
];

const EXTRA_STATES: &[&str] = &[
    "gizmo_move",
    "gizmo_rotate",
    "sculpt_brush",
    "sf_rotate",
    "sf_add",
    "sf_remove",
    "walls_line",
    "walls_curve",
];

const THEMES: &[&str] = &["dark", "light"];

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn out_dir() -> PathBuf {
    repo().join("dev/out/m4b-shots")
}

fn project_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/projects/basic.stlx")
}

fn fixture_stl(name: &str) -> PathBuf {
    repo().join("tests/ui/fixtures").join(name)
}

/// States the 2.0 panels can open for real (Tasks 2–6 landed).
fn state_ready(state: &str) -> bool {
    if state.starts_with("tool_") {
        let key = &state["tool_".len()..];
        return stl_app::tools::tool(key).is_some()
            || matches!(key, "supportfins" | "supportwall");
    }
    matches!(
        state,
        "gizmo_move" | "gizmo_rotate" | "sculpt_brush" | "sf_rotate" | "sf_add" | "sf_remove" | "walls_line" | "walls_curve"
    )
}

fn select_first(h: &mut Harness<'static, StlApp>) {
    if let Some(id) = h.state().bodies.first().map(|b| b.id.clone()) {
        h.state_mut().shell.selection = vec![id];
    }
}

fn import_fixture(h: &mut Harness<'static, StlApp>, link: &NativeLink, name: &str) {
    let path = fixture_stl(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    h.state_mut().link.send(Command::Import {
        name: name.to_owned(),
        bytes,
    });
    settle(h, link);
    select_first(h);
}

fn wait_support(h: &mut Harness<'static, StlApp>, link: &NativeLink, key: &str) {
    let end = Instant::now() + Duration::from_secs(60);
    let _ = h.state_mut().open_tool(key, None);
    while Instant::now() < end {
        settle(h, link);
        h.run_steps(2);
        if stl_app::supports::is_installed() && h.state().tool().is_some_and(|t| t.def.key == key || (key == "supportwall" && t.def.key == "supportfins")) {
            return;
        }
        let _ = h.state_mut().open_tool(key, None);
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("support tool {key} never opened");
}

/// The Held card says "Checking…" until the coverage answer comes back: wait for it (and for
/// the plastic read-out from the preview) so the shot shows the settled panel.
fn wait_cover(h: &mut Harness<'static, StlApp>, link: &NativeLink) {
    let end = Instant::now() + Duration::from_secs(90);
    while Instant::now() < end {
        settle(h, link);
        h.run_steps(2);
        let done = h
            .state()
            .tool()
            .and_then(|t| t.sf.as_ref())
            .is_some_and(|s| s.cover.as_ref().is_some_and(|c| !c.busy) && s.plastic.is_some());
        if done {
            h.run_steps(2);
            return;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    eprintln!("m4b_shots: the coverage answer never arrived; the Held card stays as it is");
}

fn apply_state(h: &mut Harness<'static, StlApp>, link: &NativeLink, state: &str) {
    use serde_json::json;
    if let Some(key) = state.strip_prefix("tool_") {
        select_first(h);
        assert!(h.state_mut().open_tool(key, None), "open {key}");
        h.run_steps(4);
        return;
    }
    match state {
        "gizmo_move" => {
            select_first(h);
            assert!(h.state_mut().open_tool("move", None));
            h.state_mut().tool_set_value("_gizmo", json!("move"));
            h.state_mut().tool_set_value("translate", json!([12.0, 0.0, 0.0]));
            h.state_mut().viewport.set_view("front");
            h.run_steps(4);
        }
        "gizmo_rotate" => {
            select_first(h);
            let mut v = serde_json::Map::new();
            v.insert("_gizmo".into(), json!("rotate"));
            v.insert("rotate".into(), json!([0.0, 25.0, 0.0]));
            assert!(h.state_mut().open_tool("move", Some(v)));
            h.state_mut().viewport.set_view("front");
            h.run_steps(4);
        }
        "sculpt_brush" => {
            select_first(h);
            assert!(h.state_mut().open_tool("sculpt", None));
            // Differ from tool_sculpt: front view + a painted stroke so the brush trail shows.
            h.state_mut().viewport.set_view("front");
            h.state_mut().tool_set_value(
                "strokes",
                json!([[
                    { "x": -5.0, "y": -10.0, "z": 20.0 },
                    { "x": 0.0, "y": -5.0, "z": 20.0 },
                    { "x": 5.0, "y": 0.0, "z": 20.0 }
                ]]),
            );
            h.state_mut().tool_set_value("radius", json!(8.0));
            h.run_steps(6);
        }
        "sf_rotate" | "sf_add" | "sf_remove" => {
            import_fixture(h, link, "tipped.stl");
            wait_support(h, link, "supportfins");
            let mode = &state["sf_".len()..];
            h.state_mut().tool_set_value("_mode", json!(mode));
            h.run_steps(6);
            wait_cover(h, link);
        }
        "walls_line" | "walls_curve" => {
            import_fixture(h, link, "ell.stl");
            wait_support(h, link, "supportwall");
            let shape = if state == "walls_line" { "line" } else { "curve" };
            let walls = if shape == "line" {
                json!([{ "shape": "line", "points": [[2.0, 2.0, 0.0], [28.0, 2.0, 0.0]] }])
            } else {
                json!([{ "shape": "curve", "points": [[4.0, 2.0, 0.0], [4.0, 18.0, 0.0], [15.0, 10.0, 0.0]] }])
            };
            h.state_mut().tool_set_value("walls", walls);
            h.run_steps(6);
        }
        other => panic!("unknown state {other}"),
    }
}

#[derive(Clone, Default)]
struct FakePlatform {
    picks: Arc<std::sync::Mutex<Vec<Option<PickedFile>>>>,
    saved: Arc<std::sync::Mutex<Vec<(String, Vec<u8>)>>>,
}

impl Platform for FakePlatform {
    fn open_file(&self, _filters: &[(&str, &[&str])]) -> Pending<Option<PickedFile>> {
        let next = self.picks.lock().unwrap().pop().unwrap_or(None);
        Pending::ready(next)
    }
    fn save_file(&self, suggested: &str, bytes: Vec<u8>) -> Pending<Result<bool, String>> {
        self.saved.lock().unwrap().push((suggested.to_string(), bytes));
        Pending::ready(Ok(true))
    }
    fn now_iso_utc(&self) -> String {
        "2026-11-13T12:00:00".to_string()
    }
    fn is_web(&self) -> bool {
        false
    }
}

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

fn settle(h: &mut Harness<'static, StlApp>, link: &NativeLink) {
    let end = Instant::now() + Duration::from_secs(120);
    loop {
        h.run_steps(1);
        if !link.is_busy() {
            h.run_steps(1);
            if !link.is_busy() {
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

fn wait_meshes(h: &mut Harness<'static, StlApp>) {
    let end = Instant::now() + Duration::from_secs(60);
    while Instant::now() < end {
        h.state_mut().viewport.flush();
        h.run_steps(2);
        let pending = h
            .state()
            .viewport
            .stats()
            .upload_pending
            .load(std::sync::atomic::Ordering::Relaxed);
        let ready = h
            .state()
            .bodies
            .iter()
            .all(|b| h.state().viewport.body_mesh(&b.id).is_some());
        if !pending && (h.state().bodies.is_empty() || ready) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn all_states() -> Vec<&'static str> {
    let mut v = Vec::with_capacity(TOOL_STATES.len() + EXTRA_STATES.len());
    v.extend_from_slice(TOOL_STATES);
    v.extend_from_slice(EXTRA_STATES);
    v
}

fn capture_one(gpu: &Headless, state: &str, theme: &str) -> (PathBuf, u64) {
    let dark = theme != "light";
    let link = Arc::new(NativeLink::new());
    let platform = FakePlatform::default();
    let ctx = egui::Context::default();
    let mut app = StlApp::new(
        &ctx,
        Box::new(platform),
        Box::new(SharedLink {
            inner: link.clone(),
        }),
    );
    app.viewport.init_gpu(&gpu.render_state);

    let renderer = wgpu_renderer::WgpuRenderer::new(gpu.render_state.clone());
    let mut init = false;
    let mut h = Harness::builder()
        .with_size(SIZE)
        .with_render_cursor(false)
        .with_accessibility_check(false)
        .renderer(renderer)
        .build_ui_state(
            move |ui, app: &mut StlApp| {
                if !init {
                    init = true;
                    stl_app::fonts::install(ui.ctx());
                    app.set_theme(
                        ui.ctx(),
                        if dark {
                            ThemePref::Dark
                        } else {
                            ThemePref::Light
                        },
                    );
                }
                app.layout(ui);
            },
            app,
        );

    if state.starts_with("sf_") || state.starts_with("walls_") {
        // Support Fins / Draw Walls shots start from a fresh STL import inside apply_state.
    } else {
        h.state_mut().open_launch_file(&project_path());
        settle(&mut h, &link);
        assert!(
            !h.state().bodies.is_empty(),
            "{state}/{theme}: expected bodies after open"
        );
    }
    apply_state(&mut h, &link, state);
    assert!(h.state().tool().is_some(), "{state}/{theme}: expected an open tool");
    h.state_mut().viewport.fit_all();
    h.run_steps(3);
    wait_meshes(&mut h);
    h.run();

    let img = h
        .render()
        .unwrap_or_else(|e| panic!("render {state}/{theme}: {e}"));
    let out = out_dir().join("2x").join(theme).join(format!("{state}.png"));
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    img.save(&out)
        .unwrap_or_else(|e| panic!("save {}: {e}", out.display()));
    eprintln!("wrote {} ({}x{})", out.display(), img.width(), img.height());
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    img.as_raw().hash(&mut hasher);
    (out, hasher.finish())
}

/// Compiles and documents any remaining skip list. GPU capture is behind `--ignored`.
#[test]
fn m4b_shots_skip_list_documents_unbuilt_states() {
    let skipped: Vec<&str> = all_states()
        .into_iter()
        .filter(|s| !state_ready(s))
        .collect();
    let ready: Vec<&str> = all_states().into_iter().filter(|s| state_ready(s)).collect();
    assert!(!ready.is_empty(), "expected ready 2.0 states after Tasks 2–6");
    eprintln!(
        "m4b_shots: ready {} state(s); skipping {} unbuilt: {}",
        ready.len(),
        skipped.len(),
        skipped.join(", ")
    );
}

#[test]
#[ignore = "writes GPU screenshots to dev/out/m4b-shots/2x; run with --ignored"]
fn m4b_shots_2x() {
    let Some(gpu) = Headless::new() else {
        panic!("no GPU adapter for m4b_shots");
    };
    std::fs::create_dir_all(out_dir()).unwrap();
    let mut skipped = Vec::new();
    let mut identical = Vec::new();
    let states = all_states();
    for theme in THEMES {
        let mut seen: Vec<(&str, u64)> = Vec::new();
        for state in &states {
            if !state_ready(state) {
                if *theme == "dark" {
                    skipped.push(*state);
                }
                continue;
            }
            let (_, hash) = capture_one(&gpu, state, theme);
            if let Some((other, _)) = seen.iter().find(|(_, h)| *h == hash) {
                identical.push(format!("{theme}: {other} and {state} are identical"));
            }
            seen.push((state, hash));
        }
    }
    if !skipped.is_empty() {
        eprintln!(
            "m4b_shots skipped (not built yet): {}",
            skipped.join(", ")
        );
    }
    assert!(identical.is_empty(), "states_differ FAILED: {identical:?}");
}
