//! Task 4: the viewport renderer. Camera numbers are worked out from 1.x `viewer.js`'s `fit`:
//! `dist = r / sin(halfFov) * 1.05`, `halfFov = min(17.5°, atan(tan(17.5°) * aspect))`, along the
//! camera's current direction from its target (start: (160, -220, 150) looking at the origin).
//!
//! Native only: the GPU and the worker threads these tests use do not exist on wasm32.
#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

use egui::Color32;
use egui_kittest::Harness;
use egui::PointerButton::{Middle, Primary, Secondary};
use stl_app::engine_link::BodyView;
use stl_app::viewport::camera::{drag_action, DragAction};
use stl_app::viewport::overlays::Depth;
use stl_app::viewport::renderer::{FlatBatch, Headless, ViewportCallback};
use stl_app::viewport::scene::{self, FlatVertex};
use stl_app::viewport::{MouseMode, PlaneFrame, Viewport};
use stl_kernel::{Aabb, MeshArrays};

#[path = "support/wgpu_renderer.rs"]
mod wgpu_renderer;

fn close(a: [f64; 3], b: [f64; 3], tol: f64) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= tol)
}

/// A 20 mm cube standing on the bed, centred on the origin in X and Y.
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

#[test]
fn camera_fit_matches_viewer_js() {
    let mut vp = Viewport::new();
    vp.set_view_size(800.0, 600.0);
    vp.fit(Aabb { min: [-10.0, -10.0, 0.0], max: [10.0, 10.0, 20.0] });
    let (target, pos, up) = vp.camera().goal();
    assert!(close(target, [0.0, 0.0, 10.0], 1e-9), "{target:?}");
    assert!(close(pos, [31.15045885313839, -42.831880923065285, 39.20355517481724], 1e-9), "{pos:?}");
    assert_eq!(up, [0.0, 0.0, 1.0]);

    // a tall, narrow view is limited by its width
    let mut vp = Viewport::new();
    vp.set_view_size(300.0, 600.0);
    vp.fit(Aabb { min: [0.0, 0.0, 0.0], max: [100.0, 40.0, 5.0] });
    let (target, pos, _) = vp.camera().goal();
    assert!(close(target, [50.0, 20.0, 2.5], 1e-9));
    assert!(close(pos, [237.21931931269407, -237.42656405495433, 178.0181118556507], 1e-9), "{pos:?}");

    // the flight takes 320 ms and lands exactly there
    let mut vp = Viewport::new();
    vp.set_view_size(800.0, 600.0);
    vp.fit(Aabb { min: [-10.0, -10.0, 0.0], max: [10.0, 10.0, 20.0] });
    assert!(vp.camera_mut().animate(0.16));
    assert!(!vp.camera_mut().animate(0.32));
    assert!(close(vp.camera().position, [31.15045885313839, -42.831880923065285, 39.20355517481724], 1e-9));
}

#[test]
fn empty_scene_fit() {
    // no bodies: viewer.js falls back to (-50, -50, 0)..(50, 50, 50)
    let mut vp = Viewport::new();
    vp.set_view_size(900.0, 600.0);
    assert!(vp.scene_box().is_none());
    vp.fit(stl_app::viewport::camera::empty_box());
    let (target, pos, _) = vp.camera().goal();
    assert!(close(target, [0.0, 0.0, 25.0], 1e-9));
    assert!(close(pos, [134.8854435317986, -185.46748485622305, 151.45510331106118], 1e-9), "{pos:?}");

    // every body hidden: the same
    let mut vp = Viewport::new();
    vp.set_view_size(900.0, 600.0);
    let mut b = cube(1);
    b.hidden = true;
    vp.sync_bodies(&[b]);
    assert!(vp.scene_box().is_none());
    vp.fit_all();
    let (_, pos, _) = vp.camera().goal();
    assert!(close(pos, [134.8854435317986, -185.46748485622305, 151.45510331106118], 1e-9));

    // a box of a single point, and a zero-sized view: no NaN
    let mut vp = Viewport::new();
    vp.set_view_size(0.0, 0.0);
    vp.fit(Aabb { min: [5.0; 3], max: [5.0; 3] });
    let (t, p, _) = vp.camera().goal();
    assert!(t.iter().chain(p.iter()).all(|v| v.is_finite()));
    // and the other views on an empty scene
    for v in ["top", "front", "right", "iso", "left", "back", "bottom", "fit"] {
        vp.set_view(v);
        vp.camera_mut().finish_flight();
        let c = vp.camera();
        assert!(c.position.iter().chain(c.target.iter()).all(|v| v.is_finite()), "{v}");
        let s = vp.world_to_screen([0.0, 0.0, 0.0]);
        assert!(s.x.is_finite() && s.y.is_finite(), "{v}");
    }
}

#[test]
fn mouse_modes_map_buttons() {
    use DragAction::*;
    assert_eq!(drag_action(MouseMode::Model, Primary), Some(Rotate));
    assert_eq!(drag_action(MouseMode::Model, Middle), Some(Pan));
    assert_eq!(drag_action(MouseMode::Model, Secondary), Some(Pan));
    for m in [MouseMode::Sketch, MouseMode::Sculpt] {
        assert_eq!(drag_action(m, Primary), None);
        assert_eq!(drag_action(m, Middle), Some(Pan));
        assert_eq!(drag_action(m, Secondary), Some(Rotate));
    }

    // OrbitControls: a drag the view's height long turns a full circle about Z, no damping
    let mut vp = Viewport::new();
    vp.set_view_size(800.0, 600.0);
    let start = vp.camera().position;
    vp.camera_mut().rotate(600.0, 0.0);
    assert!(close(vp.camera().position, start, 1e-6), "{:?}", vp.camera().position);
    vp.camera_mut().rotate(150.0, 0.0); // a quarter turn
    let p = vp.camera().position;
    assert!((p[2] - start[2]).abs() < 1e-9);
    let ang = |q: [f64; 3]| q[1].atan2(q[0]);
    let turn = (ang(start) - ang(p)).rem_euclid(std::f64::consts::TAU);
    assert!((turn - std::f64::consts::FRAC_PI_2).abs() < 1e-9, "{turn}");

    // screen-space pan moves the target and the camera together
    let mut vp = Viewport::new();
    vp.set_view_size(800.0, 600.0);
    let off0 = vp.camera().position.map(|v| v) ;
    vp.camera_mut().pan(100.0, 0.0);
    let c = vp.camera();
    let moved = [c.position[0] - off0[0], c.position[1] - off0[1], c.position[2] - off0[2]];
    assert!(close(moved, [c.target[0], c.target[1], c.target[2]], 1e-9));
    // 2 * dx * |offset| * tan(17.5°) / height
    let expect = 2.0 * 100.0 * 96500f64.sqrt() * 17.5f64.to_radians().tan() / 600.0;
    let len = (moved[0].powi(2) + moved[1].powi(2) + moved[2].powi(2)).sqrt();
    assert!((len - expect).abs() < 1e-9);

    // zoom to the cursor: the view centre keeps its point, distance times 0.95 per 100 of deltaY
    let mut vp = Viewport::new();
    vp.set_view_size(800.0, 600.0);
    vp.camera_mut().wheel(-100.0, [400.0, 300.0]);
    let c = vp.camera();
    let d = ((c.position[0] - c.target[0]).powi(2) + (c.position[1] - c.target[1]).powi(2) + (c.position[2] - c.target[2]).powi(2)).sqrt();
    assert!((d - 96500f64.sqrt() * 0.95).abs() < 1e-6, "{d}");
    assert!(close(c.target, [0.0; 3], 1e-6), "{:?}", c.target);
}

#[test]
fn sync_skips_unchanged_revision() {
    let Some(gpu) = Headless::new() else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let mut vp = Viewport::new();
    vp.sync_bodies(&[cube(1)]);
    vp.flush();
    gpu.render(&mut vp, 64, 48).expect("a picture");
    assert_eq!(vp.stats().uploads(), 1);

    // same revision, other colour and hidden state: no upload
    let mut b = cube(1);
    b.color = Some("#c9a66b".into());
    vp.sync_bodies(&[b.clone()]);
    vp.flush();
    gpu.render(&mut vp, 64, 48).unwrap();
    b.hidden = true;
    vp.sync_bodies(&[b]);
    vp.flush();
    gpu.render(&mut vp, 64, 48).unwrap();
    assert_eq!(vp.stats().uploads(), 1);

    // a new revision: one upload
    vp.sync_bodies(&[cube(2)]);
    vp.flush();
    gpu.render(&mut vp, 64, 48).unwrap();
    assert_eq!(vp.stats().uploads(), 2);

    // nothing changed: the last render is reused
    let renders = vp.stats().renders();
    gpu.render(&mut vp, 64, 48).unwrap();
    assert_eq!(vp.stats().renders(), renders);

    // a removed body leaves the GPU
    vp.sync_bodies(&[]);
    gpu.render(&mut vp, 64, 48).unwrap();
    assert_eq!(gpu.bodies_on_gpu(), 0);
}

#[test]
fn surface_lost_recovers() {
    let Some(gpu) = Headless::new() else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let mut vp = Viewport::new();
    vp.sync_bodies(&[cube(1)]);
    vp.flush();
    vp.set_view_size(96.0, 64.0);
    vp.fit_all();
    vp.camera_mut().finish_flight();
    let before = gpu.render(&mut vp, 96, 64).expect("a picture");

    // minimised (zero size), then tiny, then the surface is lost
    assert!(gpu.render(&mut vp, 0, 0).is_none());
    assert!(gpu.render(&mut vp, 0, 64).is_none());
    assert!(gpu.render(&mut vp, 1, 1).is_some());
    gpu.surface_lost();
    assert!(gpu.render(&mut vp, 0, 0).is_none());
    gpu.surface_lost();

    // back to the old size: the same picture, and the body was not uploaded again
    let after = gpu.render(&mut vp, 96, 64).expect("a picture");
    assert_eq!(vp.stats().uploads(), 1);
    assert_eq!(before, after);
    assert_eq!(gpu.bodies_on_gpu(), 1);
}

/// Orbit timing on the bench mesh (never a golden input): run with
/// `cargo test --release -p stl-app --test viewport -- --ignored orbit_bench --nocapture`.
#[test]
#[ignore]
fn orbit_bench_mesh_frame_times() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../dev/bench/test_mesh.stl");
    let Ok(bytes) = std::fs::read(path) else { return eprintln!("no bench mesh: skipped") };
    let Some(gpu) = Headless::new() else { return eprintln!("no GPU adapter: skipped") };
    let mut vp = Viewport::new();
    vp.load_dev_mesh("test_mesh.stl", &bytes).unwrap();
    vp.flush();
    vp.set_view_size(1280.0, 800.0);
    vp.camera_mut().finish_flight();
    assert!(gpu.render_and_wait(&mut vp, 1280, 800));
    let mut times = Vec::new();
    for _ in 0..300 {
        vp.camera_mut().rotate(4.0, 1.0);
        let t = std::time::Instant::now();
        assert!(gpu.render_and_wait(&mut vp, 1280, 800));
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    let p = |q: f64| times[((times.len() - 1) as f64 * q) as usize];
    eprintln!("orbit, {} frames at 1280x800: p50 {:.2} ms, p99 {:.2} ms, max {:.2} ms", times.len(), p(0.5), p(0.99), p(1.0));
    assert_eq!(vp.stats().uploads(), 1);
    assert!(p(0.99) < 16.7);
}

// ---------------------------------------------------------------------- GPU loss and errors

#[test]
fn device_loss_recovers_on_a_new_device() {
    let Some(a) = Headless::new() else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let mut vp = Viewport::new();
    vp.sync_bodies(&[cube(1)]);
    vp.flush();
    vp.set_view_size(96.0, 64.0);
    vp.fit_all();
    vp.camera_mut().finish_flight();
    let before = a.render(&mut vp, 96, 64).expect("a picture");
    assert_eq!(vp.stats().uploads(), 1);
    assert_eq!(vp.stats().gpu_states(), 1);

    // the device goes away (a driver reset, a GPU unplugged)
    a.render_state.device.destroy();
    let _ = a.render_state.device.poll(wgpu::PollType::Poll);
    for _ in 0..3 {
        assert!(a.render(&mut vp, 96, 64).is_none(), "drew on a lost device");
    }
    assert!(vp.stats().device_lost());
    // picking still works on the kept mesh
    assert!(vp.pick(vp.world_to_screen([0.0, -10.0, 10.0]), true, false).is_some());

    // a new render state (what eframe makes for a new device): the viewport sets itself up
    // there and uploads the body again from the mesh it kept
    let Some(b) = Headless::new() else { return };
    let after = b.render(&mut vp, 96, 64).expect("a picture on the new device");
    assert_eq!(vp.stats().gpu_states(), 2);
    assert_eq!(vp.stats().uploads(), 2);
    assert!(!vp.stats().device_lost());
    assert_eq!(b.bodies_on_gpu(), 1);
    assert_eq!(before, after);
}

/// The same through the paint callback, as in the app: on the lost device it makes no commands;
/// on a new renderer (through egui) it finds no state, makes one and uploads the body again.
#[test]
fn device_loss_through_the_paint_callback() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let Some(a) = Headless::new() else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let vp = Rc::new(RefCell::new(Viewport::new()));
    {
        let mut v = vp.borrow_mut();
        v.init_gpu(&a.render_state);
        v.sync_bodies(&[cube(1)]);
        v.flush();
        v.set_view_size(160.0, 120.0);
        v.fit_all();
        v.camera_mut().finish_flight();
    }
    let harness_on = |rs: &egui_wgpu::RenderState| {
        let shown = vp.clone();
        Harness::builder()
            .with_size(egui::vec2(160.0, 120.0))
            .with_render_cursor(false)
            .renderer(wgpu_renderer::WgpuRenderer::new(rs.clone()))
            .build_ui(move |ui| {
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
                    shown.borrow_mut().show(ui);
                });
            })
    };
    let mut ha = harness_on(&a.render_state);
    ha.run();
    let before = ha.render().expect("a picture");
    assert_eq!(vp.borrow().stats().uploads(), 1);

    drop(ha);
    a.render_state.device.destroy();
    let _ = a.render_state.device.poll(wgpu::PollType::Poll);
    // egui-wgpu 0.37's own renderer panics when it fills its buffers on a destroyed device, so
    // the lost frame runs the viewport's paint callback by hand, as egui would call it
    {
        use egui_wgpu::CallbackTrait;
        let rs = &a.render_state;
        let cb = {
            let mut v = vp.borrow_mut();
            v.camera_mut().rotate(1.0, 0.0);
            ViewportCallback { frame: v.frame_for_size(160.0, 120.0, 1.0), ctx: egui::Context::default(), format: rs.target_format, stats: v.stats_handle() }
        };
        let screen = egui_wgpu::ScreenDescriptor { size_in_pixels: [160, 120], pixels_per_point: 1.0 };
        let mut enc = rs.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let mut renderer = rs.renderer.write();
        for _ in 0..2 {
            let bufs = cb.prepare(&rs.device, &rs.queue, &screen, &mut enc, &mut renderer.callback_resources);
            assert!(bufs.is_empty(), "commands for a lost device");
        }
    }
    assert!(vp.borrow().stats().device_lost());
    assert_eq!(vp.borrow().stats().uploads(), 1);

    let Some(b) = Headless::new() else { return };
    vp.borrow_mut().camera_mut().rotate(-1.0, 0.0);
    let mut hb = harness_on(&b.render_state);
    hb.run();
    let mut after = hb.render().expect("a picture on the new device");
    for _ in 0..4 {
        if !vp.borrow().stats().upload_pending.load(std::sync::atomic::Ordering::Relaxed) && vp.borrow().stats().uploads() == 2 {
            break;
        }
        hb.run();
        after = hb.render().expect("a picture on the new device");
    }
    let stats = vp.borrow().stats_handle();
    assert_eq!(stats.gpu_states(), 2, "the callback made no state on the new device");
    assert_eq!(stats.uploads(), 2);
    assert!(!stats.device_lost());
    // the camera turned and back (not bit for bit): the same cube within 1 %
    let (p0, p1) = (cube_pixels(&before), cube_pixels(&after));
    assert!(p1 > 500, "the cube is not drawn on the new device");
    assert!(p0.abs_diff(p1) * 100 < p0, "{p0} cube pixels before, {p1} after");
}

/// Pixels of the cube's grey-blue sides (not the background, grid or view cube).
fn cube_pixels(img: &image::RgbaImage) -> usize {
    img.pixels().filter(|p| p.0[2] > 120 && p.0[2] > p.0[0].saturating_add(10) && p.0[1] > 90).count()
}

#[test]
fn gpu_error_keeps_body_buffers() {
    let Some(gpu) = Headless::new() else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let mut vp = Viewport::new();
    vp.sync_bodies(&[cube(1)]);
    vp.flush();
    vp.set_view_size(96.0, 64.0);
    vp.fit_all();
    vp.camera_mut().finish_flight();
    let before = gpu.render(&mut vp, 96, 64).expect("a picture");

    // a frame the GPU rejects: a draw far past the end of the overlay buffer
    let mut bad = (*vp.frame_for_size(96.0, 64.0, 1.0)).clone();
    bad.id += 1_000;
    bad.batches.push(FlatBatch { order: 50, lines: false, depth: Depth::Off, range: 0..3_000_000 });
    let (r, submitted) = gpu.submit(&vp, &bad);
    assert!(r.error, "{r:?}");
    assert!(submitted.is_none(), "an invalid command buffer was submitted");
    assert_eq!(vp.stats().gpu_errors(), 1);
    assert_eq!(gpu.bodies_on_gpu(), 1, "the device survived: body buffers stay");

    // the next good frame draws the same picture without uploading again
    let after = gpu.render(&mut vp, 96, 64).expect("a picture");
    assert_eq!(vp.stats().uploads(), 1);
    assert_eq!(before, after);
}

// ---------------------------------------------------------------------- bodies off the UI thread

#[test]
fn new_body_box_comes_from_the_worker() {
    let mut vp = Viewport::new();
    vp.set_view_size(800.0, 600.0);
    let start = vp.camera().goal();
    vp.sync_bodies(&[cube(1)]);
    // sync walks no vertex: no box until the prepared mesh comes back, and a fit waits for it
    assert!(vp.scene_box().is_none());
    vp.fit_all();
    assert_eq!(vp.camera().goal(), start);
    vp.flush();
    let b = vp.scene_box().expect("box");
    assert_eq!((b.min, b.max), ([-10.0, -10.0, 0.0], [10.0, 10.0, 20.0]));
    let (target, pos, _) = vp.camera().goal();
    assert!(close(target, [0.0, 0.0, 10.0], 1e-9), "{target:?}");
    assert!(close(pos, [31.15045885313839, -42.831880923065285, 39.20355517481724], 1e-9), "{pos:?}");
}

#[test]
fn non_solid_body_is_pink() {
    let mut vp = Viewport::new();
    let mut b = cube(1);
    b.solid = false;
    vp.sync_bodies(&[b]);
    vp.flush();
    let f = vp.frame_for_size(64.0, 48.0, 1.0);
    let c = f.bodies[0].uniform.color;
    let want = scene::linear(scene::rgb(scene::NOT_SOLID));
    assert!((0..3).all(|k| (c[k] - want[k]).abs() < 1e-6), "{c:?}");
    assert_eq!(scene::NOT_SOLID, 0xd9a0a0);
}

// ---------------------------------------------------------------------- overlays

fn overlay_view() -> Viewport {
    let mut vp = Viewport::new();
    vp.set_grid_visible(false);
    vp.sync_bodies(&[cube(1)]);
    vp.flush();
    vp
}

/// The overlay batches of one draw order, with their vertices.
fn batches(vp: &mut Viewport, order: i32) -> Vec<(FlatBatch, Vec<FlatVertex>)> {
    let f = vp.frame_for_size(64.0, 48.0, 1.0);
    f.batches
        .iter()
        .filter(|b| b.order == order)
        .map(|b| (b.clone(), f.flat[b.range.start as usize..b.range.end as usize].to_vec()))
        .collect()
}

fn alpha(a: f32) -> u8 {
    (a * 255.0).round() as u8
}

fn pos(v: &FlatVertex) -> [f64; 3] {
    v.pos.map(|c| c as f64)
}

#[test]
fn overlay_highlight_face() {
    let mut vp = overlay_view();
    vp.highlight_face("b1", &[4, 99], "face", Color32::from_rgb(0x2f, 0x8c, 0xff));
    let b = batches(&mut vp, 3);
    assert_eq!(b.len(), 1);
    let (batch, v) = &b[0];
    assert!(!batch.lines);
    assert_eq!(batch.depth, Depth::TestOffset);
    // triangle 4 is corners 0, 1, 5; triangle 99 does not exist
    assert_eq!(v.len(), 3);
    assert_eq!(pos(&v[0]), [-10.0, -10.0, 0.0]);
    assert_eq!(pos(&v[1]), [10.0, -10.0, 0.0]);
    assert_eq!(pos(&v[2]), [10.0, -10.0, 20.0]);
    assert!(v.iter().all(|x| x.c0 == [0x2f, 0x8c, 0xff, alpha(0.45)] && x.c1 == x.c0));
    // an unknown body draws nothing
    vp.highlight_face("nope", &[0], "other", Color32::RED);
    assert_eq!(batches(&mut vp, 3).len(), 1);
}

#[test]
fn overlay_marker() {
    let mut vp = overlay_view();
    // the cube's extent is its diagonal, 34.6: radius max(0.2, 0.3) = 0.3
    vp.add_marker([1.0, 2.0, 3.0], Color32::from_rgb(0xd9, 0x48, 0x0f), "marker", None);
    let b = batches(&mut vp, 20);
    assert_eq!(b.len(), 1);
    let (batch, v) = &b[0];
    assert_eq!((batch.lines, batch.depth), (false, Depth::Off));
    // SphereGeometry(r, 16, 12): 16 + 10 * 32 + 16 triangles
    assert_eq!(v.len(), 352 * 3);
    for x in v {
        let p = pos(x);
        let r = ((p[0] - 1.0).powi(2) + (p[1] - 2.0).powi(2) + (p[2] - 3.0).powi(2)).sqrt();
        assert!((r - 0.3).abs() < 1e-5, "{r}");
        assert_eq!(x.c0, [0xd9, 0x48, 0x0f, 255]);
    }
    vp.add_marker([0.0; 3], Color32::WHITE, "big", Some(2.0));
    let b = batches(&mut vp, 20);
    assert_eq!(b.len(), 2);
    let r = b[1].1.iter().map(|x| (pos(x)[0].powi(2) + pos(x)[1].powi(2) + pos(x)[2].powi(2)).sqrt()).fold(0.0, f64::max);
    assert!((r - 2.0).abs() < 1e-5, "{r}");
}

#[test]
fn overlay_arrow() {
    let mut vp = overlay_view();
    vp.add_arrow([0.0, 0.0, 5.0], [0.0, 0.0, 2.0], 10.0, Color32::from_rgb(0x1f, 0x6f, 0xd1), "arrow");
    let b = batches(&mut vp, 20);
    let lines: Vec<_> = b.iter().filter(|(bt, _)| bt.lines).collect();
    let tris: Vec<_> = b.iter().filter(|(bt, _)| !bt.lines).collect();
    assert_eq!((lines.len(), tris.len()), (1, 1));
    // ArrowHelper(dir, p, 10, c, 2.5, 1.2): the shaft to the head's base, a five-sided cone
    let shaft = &lines[0].1;
    assert_eq!(shaft.len(), 2);
    assert!(close(pos(&shaft[0]), [0.0, 0.0, 5.0], 1e-6) && close(pos(&shaft[1]), [0.0, 0.0, 12.5], 1e-6));
    let cone = &tris[0].1;
    assert_eq!(cone.len(), 30);
    assert!(cone.iter().any(|x| close(pos(x), [0.0, 0.0, 15.0], 1e-6)), "no tip");
    let rim = cone.iter().map(|x| pos(x)).filter(|p| (p[2] - 12.5).abs() < 1e-6).map(|p| (p[0] * p[0] + p[1] * p[1]).sqrt()).fold(0.0, f64::max);
    assert!((rim - 0.6).abs() < 1e-5, "{rim}");
    assert_eq!(lines[0].0.depth, Depth::Off);
}

#[test]
fn overlay_line() {
    let mut vp = overlay_view();
    vp.add_line([1.0, 2.0, 3.0], [4.0, 5.0, 6.0], Color32::from_rgb(0x1f, 0x6f, 0xd1), "line");
    let b = batches(&mut vp, 20);
    assert_eq!(b.len(), 1);
    let (batch, v) = &b[0];
    assert_eq!((batch.lines, batch.depth), (true, Depth::Off));
    assert_eq!(v.iter().map(pos).collect::<Vec<_>>(), vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
    assert_eq!(v[0].c0, [0x1f, 0x6f, 0xd1, 255]);
}

#[test]
fn overlay_polyline() {
    let mut vp = overlay_view();
    vp.add_polyline(&[[0.0; 3]], Color32::WHITE, "line");
    assert!(batches(&mut vp, 21).is_empty(), "one point draws nothing");
    vp.add_polyline(&[[0.0; 3], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]], Color32::WHITE, "line");
    let b = batches(&mut vp, 21);
    assert_eq!(b.len(), 1);
    let (batch, v) = &b[0];
    assert_eq!((batch.lines, batch.depth), (true, Depth::Off));
    assert_eq!(v.iter().map(pos).collect::<Vec<_>>(), vec![[0.0; 3], [1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]]);
}

#[test]
fn overlay_plane_ghost() {
    let mut vp = overlay_view();
    let f = PlaneFrame { origin: [0.0, 0.0, 5.0], x: [1.0, 0.0, 0.0], y: [0.0, 1.0, 0.0], normal: [0.0, 0.0, 1.0] };
    vp.add_plane_ghost(&f, "plane-ghost", Color32::from_rgb(0xe8, 0x76, 0x3a));
    let b = batches(&mut vp, 2);
    let fill = b.iter().find(|(bt, _)| !bt.lines).expect("fill");
    let edge = b.iter().find(|(bt, _)| bt.lines).expect("border");
    assert_eq!(fill.0.depth, Depth::Test);
    assert_eq!(fill.1.len(), 6);
    assert_eq!(edge.1.len(), 8);
    assert!(fill.1.iter().all(|x| x.c0 == [0xe8, 0x76, 0x3a, alpha(0.22)]));
    assert!(edge.1.iter().all(|x| x.c0 == [0xe8, 0x76, 0x3a, alpha(0.7)]));
    // max(extent * 0.35, 20) = 20 across
    assert!(fill.1.iter().all(|x| (pos(x)[0].abs() - 10.0).abs() < 1e-6 && (pos(x)[2] - 5.0).abs() < 1e-6));
}

#[test]
fn overlay_band() {
    let mut vp = overlay_view();
    let pts: Vec<[f64; 3]> = (0..200).map(|i| [i as f64, 0.0, 0.0]).collect();
    vp.set_band(&pts, Some([0.0, 0.0, 20.0]), Color32::from_rgb(0x59, 0xd9, 0x8e));
    let band = batches(&mut vp, 22);
    assert_eq!(band.len(), 1);
    // at most 130 points: 129 segments
    assert_eq!(band[0].1.len(), 258);
    assert!(band[0].0.lines && band[0].1[0].c0 == [0x59, 0xd9, 0x8e, 255]);
    let dot = batches(&mut vp, 23);
    assert_eq!(dot.len(), 1);
    assert!(!dot[0].0.lines && dot[0].1.iter().all(|x| x.c0 == [0xcf, 0xfb, 0xe4, alpha(0.75)]));
    // no points and no cursor hide both
    vp.set_band(&[], None, Color32::WHITE);
    assert!(batches(&mut vp, 22).is_empty() && batches(&mut vp, 23).is_empty());
}

#[test]
fn overlay_regions() {
    let mut vp = overlay_view();
    let f = PlaneFrame { origin: [0.0, 0.0, 2.0], x: [1.0, 0.0, 0.0], y: [0.0, 1.0, 0.0], normal: [0.0, 0.0, 1.0] };
    let outer = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
    let hole = vec![[4.0, 4.0], [4.0, 6.0], [6.0, 6.0], [6.0, 4.0]];
    vp.add_regions(&f, &[vec![outer, hole]], "regions");
    let b = batches(&mut vp, 12);
    assert_eq!(b.len(), 1);
    let (batch, v) = &b[0];
    assert_eq!((batch.lines, batch.depth), (false, Depth::Off));
    assert!(v.iter().all(|x| x.c0 == [0x2f, 0x8c, 0xff, alpha(0.4)] && (pos(x)[2] - 2.0).abs() < 1e-6));
    let area: f64 = v
        .chunks_exact(3)
        .map(|t| {
            let (a, b, c) = (pos(&t[0]), pos(&t[1]), pos(&t[2]));
            ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() / 2.0
        })
        .sum();
    assert!((area - 96.0).abs() < 1e-6, "{area}");
}

#[test]
fn overlay_preview() {
    use stl_app::viewport::{PreviewMesh, PreviewPack};
    let mut vp = overlay_view();
    vp.show_preview(&PreviewPack {
        meshes: vec![PreviewMesh { id: "b1".into(), mesh: cube_mesh() }, PreviewMesh { id: "new".into(), mesh: cube_mesh() }],
        ..PreviewPack::default()
    });
    let f = vp.frame_for_size(64.0, 48.0, 1.0);
    let body = f.bodies.iter().find(|d| d.key == "b1").unwrap();
    assert!(!body.visible, "the body the preview stands for is hidden");
    assert!(!vp.body_visible("b1"));
    let same = f.bodies.iter().find(|d| d.key.ends_with("preview:b1")).unwrap();
    let new = f.bodies.iter().find(|d| d.key.ends_with("preview:new")).unwrap();
    assert!(same.visible && new.visible && !same.cap && !new.cap);
    // the body's own colour (emissive 0.08); a new mesh takes the preview palette (0.2)
    let lin = |hex: u32| scene::linear(scene::rgb(hex));
    assert_eq!(&same.uniform.color[..3], &lin(0x9aa7b4)[..]);
    assert_eq!(&new.uniform.color[..3], &lin(0x7fb0e8)[..]);
    vp.clear_preview();
    let f = vp.frame_for_size(64.0, 48.0, 1.0);
    assert_eq!(f.bodies.len(), 1);
    assert!(f.bodies[0].visible && vp.body_visible("b1"));
}

#[test]
fn overlay_clear_by_tag() {
    let mut vp = overlay_view();
    vp.add_line([0.0; 3], [1.0, 0.0, 0.0], Color32::WHITE, "a");
    vp.add_marker([0.0; 3], Color32::WHITE, "b", None);
    vp.add_polyline(&[[0.0; 3], [0.0, 1.0, 0.0]], Color32::WHITE, "a");
    vp.clear_overlay("a");
    let b = batches(&mut vp, 20);
    assert!(b.len() == 1 && !b[0].0.lines, "only the marker is left");
    assert!(batches(&mut vp, 21).is_empty());
    vp.clear_overlay("");
    assert!(batches(&mut vp, 20).is_empty());
    // the axes are not overlays
    assert!(!batches(&mut vp, 5).is_empty());
}

#[test]
fn sketches_draw_outside_sketch_mode() {
    use stl_app::engine_link::{SketchOutline, SketchView};
    let square = SketchOutline { pts: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]], closed: true, construction: false };
    let guide = SketchOutline { pts: vec![[0.0, 0.0], [5.0, 0.0]], closed: false, construction: true };
    let frame = [[0.0, 0.0, 3.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let sk = |id: &str, hidden: bool| SketchView { id: id.into(), name: id.into(), plane: "XY".into(), hidden, frame, outlines: vec![square.clone(), guide.clone()].into() };
    let mut vp = overlay_view();
    vp.sync_sketches(&[sk("s1", false), sk("s2", true), sk("s3", false)], Some("s3"));
    let b = batches(&mut vp, 10);
    assert_eq!(b.len(), 1, "one sketch drawn: s2 is hidden, s3 is being edited");
    let (batch, v) = &b[0];
    assert_eq!((batch.lines, batch.depth), (true, Depth::Off));
    let solid: Vec<_> = v.iter().filter(|x| x.c0 == [0xd9, 0x48, 0x0f, alpha(0.9)]).collect();
    let dashed: Vec<_> = v.iter().filter(|x| x.c0 == [0x8a, 0x6a, 0x55, alpha(0.9)]).collect();
    // the closed square: 4 segments on the sketch plane; the construction line dashed 1.5 / 1
    assert_eq!(solid.len(), 8);
    assert!(solid.iter().all(|x| (pos(x)[2] - 3.0).abs() < 1e-6));
    assert_eq!(dashed.len(), 2 * 2, "dashes at 0-1.5 and 2.5-4");
    assert_eq!(solid.len() + dashed.len(), v.len());
    vp.sync_sketches(&[], None);
    assert!(batches(&mut vp, 10).is_empty());
}

// ---------------------------------------------------------------------- snapshots

mod snapshots {
    use super::*;

    /// The cube in a 480 x 320 window (kittest adds an 8-point margin), through egui, the app's
    /// theme and the paint callback.
    fn snapshot(name: &str, dark: bool, setup: impl FnOnce(&mut Viewport)) {
        let Some(gpu) = Headless::new() else {
            eprintln!("no GPU adapter: skipped");
            return;
        };
        let mut vp = Viewport::new();
        vp.init_gpu(&gpu.render_state);
        vp.set_theme(dark);
        vp.sync_bodies(&[cube(1)]);
        vp.flush();
        vp.set_view_size(464.0, 304.0);
        setup(&mut vp);
        vp.fit_all();
        vp.camera_mut().finish_flight();
        let renderer = super::wgpu_renderer::WgpuRenderer::new(gpu.render_state.clone());
        let mut harness = Harness::builder().with_size(egui::vec2(480.0, 320.0)).renderer(renderer).build_ui(move |ui| {
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
                vp.show(ui);
            });
        });
        stl_app::theme::apply(&harness.ctx, &stl_app::theme::tokens(dark));
        harness.run();
        harness.snapshot(name);
    }

    #[test]
    fn viewport_cube_shaded() {
        snapshot("viewport_cube_shaded", true, |vp| vp.set_edges_visible(false));
    }

    #[test]
    fn viewport_cube_edges() {
        snapshot("viewport_cube_edges", true, |vp| vp.set_edges_visible(true));
    }

    #[test]
    fn viewport_cube_selected() {
        snapshot("viewport_cube_selected", true, |vp| vp.set_selection(&["b1".to_owned()]));
    }

    #[test]
    fn viewport_cube_hidden() {
        snapshot("viewport_cube_hidden", true, |vp| {
            let mut b = cube(1);
            b.hidden = true;
            vp.sync_bodies(&[b]);
        });
    }

    #[test]
    fn viewport_cube_dark() {
        snapshot("viewport_cube_dark", true, |vp| vp.set_base_planes_visible(true));
    }

    #[test]
    fn viewport_cube_light() {
        snapshot("viewport_cube_light", false, |vp| vp.set_base_planes_visible(true));
    }
}
