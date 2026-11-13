//! M4a side-by-side 2.0 screenshots (`dev/out/m4a-shots/*_2x.png`).
//!
//! Ignored by default: needs a GPU. Run with:
//! `cargo test -p stl-app --test m4a_shots -- --ignored`
//!
//! 1.x pairs come from `node dev/m4a_shots/capture_m4a_shots.mjs`.
#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use stl_app::app::StlApp;
use stl_app::engine_link::native::NativeLink;
use stl_app::engine_link::{Command, EngineLink, Update};
use stl_app::platform::{Pending, PickedFile, Platform};
use stl_app::ui::browser::Browser;
use stl_app::ui::ThemePref;
use stl_app::viewport::renderer::Headless;

#[path = "support/wgpu_renderer.rs"]
mod wgpu_renderer;

const SIZE: egui::Vec2 = egui::vec2(1280.0, 800.0);

const STATES: &[&str] = &["start", "project", "rename", "timeline", "parameters", "section"];
const THEMES: &[&str] = &["dark", "light"];

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn out_dir() -> PathBuf {
    repo().join("dev/out/m4a-shots")
}

fn project_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/projects/basic.stlx")
}

/// Same FakePlatform as the UI harness, kept here so we can own the NativeLink Arc for settle.
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
        "2026-11-12T12:00:00".to_string()
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
        let pending = h.state().viewport.stats().upload_pending.load(std::sync::atomic::Ordering::Relaxed);
        let ready = h.state().bodies.iter().all(|b| h.state().viewport.body_mesh(&b.id).is_some());
        if !pending && (h.state().bodies.is_empty() || ready) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn apply_state(h: &mut Harness<'static, StlApp>, link: &NativeLink, state: &str) {
    match state {
        "start" => {}
        "project" => {
            h.state_mut().viewport.fit_all();
        }
        "rename" => {
            if let Some(b) = h.state().bodies.first().cloned() {
                h.state_mut().browser = Browser::with_rename(b.id, b.name);
            }
            h.state_mut().viewport.fit_all();
        }
        "timeline" => {
            // basic.stlx already has rollback=2; force a visible marker if missing.
            if h.state().timeline_view.marker.is_none() && !h.state().timeline_view.items.is_empty() {
                let n = h.state().timeline_view.items.len().saturating_sub(1).max(1);
                link.send(Command::Rollback(Some(n)));
                settle(h, link);
            }
            h.state_mut().viewport.fit_all();
        }
        "parameters" => {
            h.state_mut().params_open = true;
            h.state_mut().params.set_open(true);
            h.state_mut().viewport.fit_all();
        }
        "section" => {
            h.state_mut().viewport.set_section_enabled(true);
            h.state_mut().viewport.fit_all();
        }
        other => panic!("unknown state {other}"),
    }
    h.run_steps(3);
    wait_meshes(h);
}

fn capture_one(gpu: &Headless, state: &str, theme: &str) -> PathBuf {
    let dark = theme != "light";
    let link = Arc::new(NativeLink::new());
    let platform = FakePlatform::default();
    let ctx = egui::Context::default();
    let mut app = StlApp::new(&ctx, Box::new(platform), Box::new(SharedLink { inner: link.clone() }));
    app.viewport.init_gpu(&gpu.render_state);

    let needs_project = state != "start";
    if needs_project {
        let path = project_path();
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        link.send(Command::Open { name: "basic.stlx".into(), bytes });
    }

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
                    app.set_theme(ui.ctx(), if dark { ThemePref::Dark } else { ThemePref::Light });
                }
                app.layout(ui);
            },
            app,
        );

    if needs_project {
        settle(&mut h, &link);
        assert!(!h.state().bodies.is_empty(), "{state}/{theme}: expected bodies after open");
    } else {
        h.run_steps(2);
    }
    apply_state(&mut h, &link, state);
    h.run();

    let img = h.render().unwrap_or_else(|e| panic!("render {state}/{theme}: {e}"));
    let out = out_dir().join(format!("{state}_{theme}_2x.png"));
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    img.save(&out).unwrap_or_else(|e| panic!("save {}: {e}", out.display()));
    eprintln!("wrote {} ({}x{})", out.display(), img.width(), img.height());
    out
}

fn write_index() {
    let out = out_dir();
    let mut rows = String::new();
    for state in STATES {
        for theme in THEMES {
            let a = format!("{state}_{theme}_1x.png");
            let b = format!("{state}_{theme}_2x.png");
            rows.push_str(&format!("| {state} / {theme} | [{a}]({a}) | [{b}]({b}) |\n"));
        }
    }
    let md = format!(
        "# M4a shell/viewport screenshots\n\n\
Window size: 1280×800. 1.x from `node dev/m4a_shots/capture_m4a_shots.mjs`; \
2.0 from `cargo test -p stl-app --test m4a_shots -- --ignored`.\n\n\
| State / theme | 1.x | 2.0 |\n| --- | --- | --- |\n{rows}"
    );
    std::fs::write(out.join("index.md"), &md).unwrap();
    std::fs::write(repo().join("dev/m4a_shots/index.md"), &md).unwrap();
}

#[test]
#[ignore = "writes GPU screenshots to dev/out/m4a-shots; run with --ignored"]
fn m4a_shots_2x() {
    let Some(gpu) = Headless::new() else {
        panic!("no GPU adapter for m4a_shots");
    };
    std::fs::create_dir_all(out_dir()).unwrap();
    for theme in THEMES {
        for state in STATES {
            capture_one(&gpu, state, theme);
        }
    }
    write_index();
}
