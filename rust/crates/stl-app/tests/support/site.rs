//! What the website pictures share (`site_shots.rs`, the screenshots, and `site_videos.rs`, the
//! clips): the app on the headless GPU in the dark theme, the 1.x site's mounting bracket, and
//! cropping a frame to the window.
//!
//! Use from a test file that also declares `mod harness` (`ui/harness.rs`) and
//! `mod wgpu_renderer` (`support/wgpu_renderer.rs`), with `#[path = "support/site.rs"] mod site;`.
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;

use egui_kittest::Harness;
use serde_json::json;
use stl_app::app::StlApp;
use stl_app::engine_link::Command;
use stl_app::engine_link::native::NativeLink;
use stl_app::ui::ThemePref;
use stl_app::viewport::renderer::Headless;

use super::harness::{FakePlatform, SharedLink, repo, settle};
use super::wgpu_renderer;

pub type H = Harness<'static, StlApp>;
pub fn img_dir() -> PathBuf {
    repo().join("docs/img")
}

/// How sharp the pictures are: pixels per point, from `SITE_SCALE` (2 unless set), so a
/// 1600 × 900 point window is a 3200 × 1800 picture, sharp on a high-density screen.
pub fn scale() -> f32 {
    std::env::var("SITE_SCALE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2.0)
}

/// The app in the dark theme on the headless GPU, `size` points big, with an empty project.
/// `cursor` draws the mouse pointer (for the clips). `step_dt` is the app time one harness step
/// stands for: a clip's frame interval, so camera flights move as fast as in the app.
pub fn fresh(gpu: &Headless, size: egui::Vec2, cursor: bool, step_dt: Option<f32>) -> (H, Arc<NativeLink>) {
    let link = Arc::new(NativeLink::new());
    let ctx = egui::Context::default();
    let mut app = StlApp::new(
        &ctx,
        Box::new(FakePlatform::default()),
        Box::new(SharedLink { inner: link.clone() }),
    );
    app.viewport.init_gpu(&gpu.render_state);
    let renderer = wgpu_renderer::WgpuRenderer::new(gpu.render_state.clone());
    let mut init = false;
    let mut b = Harness::builder().with_size(size).with_pixels_per_point(scale());
    if let Some(dt) = step_dt {
        b = b.with_step_dt(dt);
    }
    let mut h = b
        .with_render_cursor(cursor)
        .with_accessibility_check(false)
        .renderer(renderer)
        .build_ui_state(
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

pub fn add(h: &mut H, link: &NativeLink, kind: &str, name: Option<&str>, params: serde_json::Value) {
    h.state_mut().link.send(Command::AddFeature {
        kind: kind.into(),
        params,
        index: None,
        name: name.map(str::to_owned),
    });
    settle(h, link);
}

/// The 1.x site's mounting bracket: a base plate, a round boss with a hole, an upright, and four
/// counterbored holes. Seen from the iso view.
pub fn build_bracket(h: &mut H, link: &NativeLink) {
    let prim = |shape: &str, extra: serde_json::Value, op: &str, z: f64, x: f64| {
        let mut p = json!({"shape": shape, "anchor": "base", "plane": "XY", "operation": op,
            "offset": [x, 0, z], "rotate": [0, 0, 0], "targets": []});
        for (k, v) in extra.as_object().unwrap() {
            p[k] = v.clone();
        }
        p
    };
    add(
        h,
        link,
        "primitive",
        Some("Bracket"),
        prim("box", json!({"length": 90, "width": 56, "height": 8}), "new", 0.0, 0.0),
    );
    add(
        h,
        link,
        "primitive",
        None,
        prim("cylinder", json!({"radius": 14, "height": 22}), "join", 8.0, 0.0),
    );
    add(
        h,
        link,
        "primitive",
        None,
        prim(
            "box",
            json!({"length": 10, "width": 56, "height": 18}),
            "join",
            8.0,
            -40.0,
        ),
    );
    add(
        h,
        link,
        "hole",
        None,
        json!({"points": [[0, 0, 30]], "normal": [0, 0, 1], "diameter": 12, "through": true, "kind": "simple"}),
    );
    add(
        h,
        link,
        "hole",
        None,
        json!({"points": [[33, 20, 8], [33, -20, 8], [-22, 20, 8], [-22, -20, 8]], "normal": [0, 0, 1],
            "diameter": 5.5, "through": true, "kind": "counterbore", "cb_diameter": 10, "cb_depth": 3}),
    );
    assert!(!h.state().bodies.is_empty(), "the bracket was built");
    let id = h.state().bodies[0].id.clone();
    h.state_mut().link.send(Command::BodyProps {
        id,
        name: Some("Bracket".into()),
        color: None,
        hidden: None,
    });
    settle(h, link);
    iso(h);
}

pub fn iso(h: &mut H) {
    h.state_mut().viewport.set_view("iso");
    h.state_mut().viewport.fit_all();
    h.state_mut().viewport.flush();
    h.run_steps(3);
}

pub fn wait_meshes(h: &mut H) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while std::time::Instant::now() < end {
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
        if !pending && ready {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// The app draws its window with a see-through shadow margin around it; keep only the window.
pub fn crop_to_window(img: &image::RgbaImage) -> image::RgbaImage {
    let (w, h) = img.dimensions();
    let opaque = |x: u32, y: u32| img.get_pixel(x, y)[3] == 255;
    let (cx, cy) = (w / 2, h / 2);
    let left = (0..w).find(|&x| opaque(x, cy)).unwrap_or(0);
    let right = (0..w).rev().find(|&x| opaque(x, cy)).unwrap_or(w - 1);
    let top = (0..h).find(|&y| opaque(cx, y)).unwrap_or(0);
    let bottom = (0..h).rev().find(|&y| opaque(cx, y)).unwrap_or(h - 1);
    image::imageops::crop_imm(img, left, top, right - left + 1, bottom - top + 1).to_image()
}
