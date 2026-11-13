//! Task 6: section view and smoothness.
//!
//! The plane follows `viewer.js` `setSection`. The cap is `#d06060`, drawn on the GPU (dimmed
//! bodies too, previews never). Idle is "no repaint, no render unless something changed".
//! A rebuild of the bench mesh must not stall a frame.
//!
//! Native only: the GPU and the worker threads these tests use do not exist on wasm32.
#![cfg(not(target_arch = "wasm32"))]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use stl_app::engine_link::BodyView;
use stl_app::viewport::renderer::Headless;
use stl_app::viewport::section::{self, SectionAxis, CAP_COLOR, CAP_WGSL, UPLOAD_BUDGET_BYTES};
use stl_app::viewport::{PreviewMesh, PreviewPack, Viewport};
use stl_kernel::MeshArrays;

#[path = "support/wgpu_renderer.rs"]
mod wgpu_renderer;

fn cube_mesh() -> Arc<MeshArrays> {
    let p = [
        [-10.0, -10.0, 0.0],
        [10.0, -10.0, 0.0],
        [10.0, 10.0, 0.0],
        [-10.0, 10.0, 0.0],
        [-10.0, -10.0, 20.0],
        [10.0, -10.0, 20.0],
        [10.0, 10.0, 20.0],
        [-10.0, 10.0, 20.0],
    ];
    let t: [u32; 36] = [0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 1, 2, 6, 1, 6, 5, 2, 3, 7, 2, 7, 6, 3, 0, 4, 3, 4, 7];
    Arc::new(MeshArrays { positions: p.iter().flatten().map(|&v| v as f32).collect(), triangles: t.to_vec() })
}

fn cube(revision: u64) -> BodyView {
    BodyView { id: "b1".into(), name: "Cube".into(), color: Some("#9aa7b4".into()), hidden: false, solid: true, mesh: cube_mesh(), revision }
}

fn close4(a: [f64; 4], b: [f64; 4]) -> bool {
    (0..4).all(|i| (a[i] - b[i]).abs() < 1e-9)
}

#[test]
fn section_plane_matches_viewer_js() {
    // camera on +Z of the target, axis Z, flip off: keep the far half (normal points away from the camera)
    let plane = section::cut_plane(SectionAxis::Z, 10.0, false, [160.0, -220.0, 150.0], [0.0, 0.0, 0.0]);
    assert!(close4(plane, [0.0, 0.0, -1.0, 10.0]), "{plane:?}");
    let flipped = section::cut_plane(SectionAxis::Z, 10.0, true, [160.0, -220.0, 150.0], [0.0, 0.0, 0.0]);
    assert!(close4(flipped, [0.0, 0.0, 1.0, -10.0]), "{flipped:?}");
    // camera on the negative X side: keep the positive half, the one away from the camera
    let x = section::cut_plane(SectionAxis::X, 5.0, false, [-10.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
    assert!(close4(x, [1.0, 0.0, 0.0, -5.0]), "{x:?}");

    let mut vp = Viewport::new();
    vp.set_view_size(800.0, 600.0);
    vp.sync_bodies(&[cube(1)]);
    vp.flush();
    vp.fit_all();
    vp.camera_mut().finish_flight();
    assert!(vp.section_plane().is_none());
    vp.set_section(true);
    let b = vp.scene_box().unwrap();
    let offset = (b.min[2] + b.max[2]) * 0.5;
    let cam = vp.camera();
    let expect = section::cut_plane(SectionAxis::Z, offset, false, cam.position, cam.target);
    assert!(close4(vp.section_plane().unwrap(), expect), "{:?}", vp.section_plane());
    assert!(vp.section_enabled());

    let frame = vp.frame_for_size(64.0, 48.0, 1.0);
    assert_eq!(frame.globals.flags[0], 1.0);
    for i in 0..4 {
        assert!((frame.globals.clip[i] - expect[i] as f32).abs() < 1e-5);
    }

    // off keeps the plane; on again does not recompute it after the camera moves
    vp.set_section(false);
    assert!(!vp.section_enabled());
    assert!(close4(vp.section_plane().unwrap(), expect));
    vp.camera_mut().rotate(80.0, 10.0);
    vp.set_section(true);
    assert!(close4(vp.section_plane().unwrap(), expect));
    vp.set_section(false);
    let off = vp.frame_for_size(64.0, 48.0, 1.0);
    assert_eq!(off.globals.flags[0], 0.0);

    // empty scene: Z through the origin (midpoint of [-50, 50])
    let mut empty = Viewport::new();
    empty.set_section(true);
    let cam = empty.camera();
    let expect = section::cut_plane(SectionAxis::Z, 0.0, false, cam.position, cam.target);
    assert!(close4(empty.section_plane().unwrap(), expect), "{:?}", empty.section_plane());

    assert!(CAP_WGSL.contains("fn fs_cap"));
    assert!(CAP_WGSL.contains("208.0 / 255.0"));
    assert!(CAP_WGSL.contains("discard"));
    assert_eq!(CAP_COLOR, 0xd06060);
}

#[test]
fn upload_span_splits_a_large_mesh() {
    let total = UPLOAD_BUDGET_BYTES * 2 + 100;
    let mut at = 0u64;
    let mut n = 0;
    let mut covered = 0u64;
    while at < total {
        let span = section::upload_span(at, total);
        assert!(span.end - span.start <= UPLOAD_BUDGET_BYTES);
        assert_eq!(span.start, at);
        assert!(span.end > span.start);
        covered += span.end - span.start;
        at = span.end;
        n += 1;
    }
    assert_eq!(n, 3);
    assert_eq!(covered, total);
    assert!(section::upload_span(total, total).is_empty());
}

#[test]
fn section_cap_on_dimmed_bodies_not_previews() {
    // viewer.js:252 (setBodiesDim) leaves `o.back` opaque, and :297 gives preview meshes no back.
    let mut vp = Viewport::new();
    vp.sync_bodies(&[cube(1)]);
    vp.flush();
    vp.set_dimmed(&["b1".to_owned()], true);
    vp.show_preview(&PreviewPack {
        meshes: vec![PreviewMesh { id: "fin".into(), mesh: cube_mesh() }],
        ..PreviewPack::default()
    });
    vp.set_section(true);
    let frame = vp.frame_for_size(64.0, 48.0, 1.0);
    let body = frame.bodies.iter().find(|d| d.key == "b1").expect("the body");
    assert!(body.transparent && body.cap, "a dimmed body keeps its cap");
    let fin = frame.bodies.iter().find(|d| d.key.ends_with("preview:fin")).expect("the preview");
    assert!(!fin.cap, "a preview mesh has no cap");
}

#[test]
fn section_toggle_always_repaints() {
    let vp = Rc::new(RefCell::new(Viewport::new()));
    vp.borrow_mut().sync_bodies(&[cube(1)]);
    vp.borrow_mut().flush();
    let shown = vp.clone();
    let mut harness = Harness::builder().with_size(egui::vec2(160.0, 120.0)).build_ui(move |ui| {
        shown.borrow_mut().show(ui);
    });
    harness.run();
    assert!(!harness.ctx.has_requested_repaint(), "idle before the toggle");
    for on in [true, true, false, false, true] {
        vp.borrow_mut().set_section_enabled(on);
        assert_eq!(vp.borrow().section_enabled(), on);
        assert!(harness.ctx.has_requested_repaint(), "set_section_enabled({on}) asked for no frame");
        harness.run();
        let flags = vp.borrow_mut().frame_for_size(160.0, 120.0, 1.0).globals.flags[0];
        assert_eq!(flags, if on { 1.0 } else { 0.0 });
    }
    // a new cut repaints too
    vp.borrow_mut().set_section_cut(Some((SectionAxis::X, 2.0, false)));
    assert!(harness.ctx.has_requested_repaint());
}

fn cap_pixels(img: &image::RgbaImage) -> usize {
    img.pixels()
        .filter(|p| {
            let [r, g, b, a] = p.0;
            a > 200 && r > 180 && (g as i16) < 140 && (b as i16) < 140 && r > g.saturating_add(40) && r > b.saturating_add(40)
        })
        .count()
}

/// The section view through egui's paint callback (`tests/support/wgpu_renderer.rs`): the cube
/// and a second cube beside it that is hidden (it must not show, cut or cap). With `name`, the
/// picture is also checked against `tests/snapshots/<name>.png`.
fn section_picture(gpu: &Headless, dark: bool, section: bool, name: Option<&str>) -> image::RgbaImage {
    let mut vp = Viewport::new();
    vp.init_gpu(&gpu.render_state);
    vp.set_theme(dark);
    let mut hidden = cube(1);
    hidden.id = "b2".into();
    hidden.hidden = true;
    hidden.mesh = Arc::new(MeshArrays {
        positions: cube_mesh().positions.chunks_exact(3).flat_map(|p| [p[0] + 30.0, p[1], p[2]]).collect(),
        triangles: cube_mesh().triangles.clone(),
    });
    vp.sync_bodies(&[cube(1), hidden]);
    vp.flush();
    vp.set_view_size(320.0, 240.0);
    vp.fit_all();
    vp.camera_mut().finish_flight();
    vp.set_section(section);
    let renderer = wgpu_renderer::WgpuRenderer::new(gpu.render_state.clone());
    let mut harness = Harness::builder().with_size(egui::vec2(320.0, 240.0)).with_render_cursor(false).renderer(renderer).build_ui(move |ui| {
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            vp.show(ui);
        });
    });
    harness.run();
    if let Some(name) = name {
        harness.snapshot(name);
    }
    let picture = harness.render().expect("section picture");
    // this test makes several harnesses: check each one's snapshot here
    let errors: Vec<String> = harness.take_snapshot_results().into_inner().iter().map(|e| e.to_string()).collect();
    assert!(errors.is_empty(), "{}", errors.join("
"));
    picture
}

#[test]
fn section_snapshot() {
    let Some(gpu) = Headless::new() else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let dark = section_picture(&gpu, true, true, Some("section_dark"));
    let light = section_picture(&gpu, false, true, Some("section_light"));
    let open = section_picture(&gpu, true, false, None);
    let dark_caps = cap_pixels(&dark);
    let light_caps = cap_pixels(&light);
    let open_caps = cap_pixels(&open);
    assert!(dark_caps >= 30, "dark section shows {dark_caps} cap pixels, image {}x{}", dark.width(), dark.height());
    assert!(light_caps >= 30, "light section shows {light_caps} cap pixels");
    assert!(open_caps * 4 < dark_caps, "section off has {open_caps} red pixels, on has {dark_caps}");
    assert_ne!(dark.as_raw(), light.as_raw(), "dark and light section views are the same picture");
}

fn repaint_delay(harness: &Harness<'_>) -> Duration {
    harness
        .output()
        .viewport_output
        .get(&egui::ViewportId::ROOT)
        .map(|v| v.repaint_delay)
        .unwrap_or(Duration::MAX)
}

/// Idle: nothing changes for five seconds, so the view asks for no frame and (with a GPU)
/// renders nothing new. Frames are still stepped and painted every second, as when the OS
/// repaints the window: the viewport reuses its last render instead of drawing the scene again.
#[test]
fn idle_draws_no_frames() {
    let gpu = Headless::new();
    if gpu.is_none() {
        eprintln!("no GPU adapter: only the repaint requests are checked");
    }
    let mut vp = Viewport::new();
    if let Some(g) = &gpu {
        vp.init_gpu(&g.render_state);
    }
    vp.sync_bodies(&[cube(1)]);
    vp.flush();
    vp.set_view_size(320.0, 240.0);
    vp.fit_all();
    vp.camera_mut().finish_flight();
    let stats = vp.stats_handle();
    let mut builder = Harness::builder().with_size(egui::vec2(320.0, 240.0));
    if let Some(g) = &gpu {
        builder = builder.renderer(wgpu_renderer::WgpuRenderer::new(g.render_state.clone()));
    }
    let mut harness = builder.build_ui(move |ui| {
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            vp.show(ui);
        });
    });
    harness.run();
    if gpu.is_some() {
        // the first painted frame uploads the cube and renders it once
        harness.render().expect("first frame");
        while stats.upload_pending.load(std::sync::atomic::Ordering::Relaxed) {
            harness.run();
            harness.render().expect("upload frame");
        }
        harness.run();
        harness.render().expect("settled frame");
        assert_eq!(stats.uploads(), 1);
        assert!(stats.renders() >= 1, "the cube was never rendered");
    }
    let renders = stats.renders();

    // Five seconds with nothing happening: the view asks for no further frame.
    let mut paints = 0u32;
    let mut now = harness.input().time.unwrap_or(0.0);
    let end = now + 5.0;
    while paints < 8 {
        let delay = repaint_delay(&harness);
        let left = end - now;
        if left <= 0.0 || delay > Duration::from_secs_f64(left) {
            break;
        }
        now += delay.as_secs_f64();
        paints += 1;
        harness.input_mut().time = Some(now);
        harness.step();
    }
    assert_eq!(paints, 0, "idle requested {paints} repaints within 5 s (next in {:?})", repaint_delay(&harness));

    // Forced frames (the OS repainting the window): painted, but the scene is not drawn again.
    for _ in 0..5 {
        now += 1.0;
        harness.input_mut().time = Some(now);
        harness.step();
        if gpu.is_some() {
            harness.render().expect("idle frame");
        }
    }
    assert_eq!(stats.renders(), renders, "idle frames rendered the scene again");
    assert_eq!(stats.uploads(), if gpu.is_some() { 1 } else { 0 });
}

#[test]
fn rebuild_never_stalls() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../dev/bench/test_mesh.stl");
    let Ok(bytes) = std::fs::read(path) else {
        eprintln!("bench mesh missing at {path}: skipped");
        return;
    };
    let Some(gpu) = Headless::new() else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let mut vp = Viewport::new();
    vp.load_dev_mesh("test_mesh.stl", &bytes).expect("bench mesh");
    vp.flush();
    vp.set_view_size(1280.0, 800.0);
    vp.camera_mut().finish_flight();
    assert!(gpu.render_and_wait(&mut vp, 1280, 800), "first frame");
    // The bench mesh is copied 2 MiB a frame. Finish that first upload before timing a rebuild,
    // or the timed loop measures the initial copy instead of the revision bump.
    let warmup = Instant::now() + Duration::from_secs(60);
    while vp.stats().uploads() == 0 && Instant::now() < warmup {
        assert!(gpu.render_and_wait(&mut vp, 1280, 800));
    }
    assert!(vp.stats().uploads() >= 1, "bench mesh never uploaded");
    let uploads_before = vp.stats().uploads();

    // There is no mesh Fillet in the engine yet. What a Fillet does to the UI is: the worker
    // returns a new mesh, and this frame uploads it. Same triangles, new revision.
    let body = vp.bodies().next().expect("bench body").clone();
    let mut rebuilt = body.clone();
    rebuilt.revision = body.revision + 1;

    let mut times = Vec::new();
    let apply = Instant::now();
    vp.sync_bodies(std::slice::from_ref(&rebuilt));
    assert!(gpu.render_and_wait(&mut vp, 1280, 800));
    times.push(apply.elapsed().as_secs_f64() * 1000.0);

    let deadline = Instant::now() + Duration::from_secs(180);
    while vp.stats().uploads() == uploads_before && Instant::now() < deadline {
        let t = Instant::now();
        assert!(gpu.render_and_wait(&mut vp, 1280, 800));
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let t = Instant::now();
    assert!(gpu.render_and_wait(&mut vp, 1280, 800));
    times.push(t.elapsed().as_secs_f64() * 1000.0);

    let longest = times.iter().copied().fold(0.0_f64, f64::max);
    eprintln!(
        "fillet rebuild: {} frames, longest {longest:.2} ms, uploads {uploads_before} -> {}",
        times.len(),
        vp.stats().uploads()
    );
    assert!(vp.stats().uploads() > uploads_before, "the rebuilt mesh was never uploaded");
    assert!(longest < 16.7, "longest frame {longest:.2} ms");
}
