//! The start-up flight to the iso view, and the sketch plane grid it shares the camera with.
//!
//! 1.x `viewer.js` `_flyTo`: 320 ms, eased with `1 - (1 - k)^3`, from the start pose
//! `(160, -220, 150)` looking at the origin. An empty scene's iso view sits `60 / sin(halfFov) * 1.05`
//! along `(1, -1, 0.85)`. A pointer press, drag or wheel calls [`Camera::cancel_flight`].
//! Desktop only: `preview_prep::bench_mesh` is left out of the web build.
#![cfg(not(target_arch = "wasm32"))]

use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use stl_app::viewport::camera::START_POSITION;
use stl_app::viewport::preview_prep::{self, Batch};
use stl_app::viewport::scene::{self, CreaseBuild};
use stl_app::viewport::{PlaneFrame, Viewport};
use stl_mesh::bvh::Bvh;

fn close(a: [f64; 3], b: [f64; 3], tol: f64) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= tol)
}

/// Feature-edge segments, sorted. `feature_edges` finishes with a HashMap walk, so the order
/// of open edges changes between calls even when the segments are the same.
fn edge_set(edges: &Option<Vec<[f32; 3]>>) -> Vec<[u32; 6]> {
    let Some(v) = edges else { return Vec::new() };
    let mut segs: Vec<[u32; 6]> = v
        .chunks(2)
        .map(|c| [c[0][0].to_bits(), c[0][1].to_bits(), c[0][2].to_bits(), c[1][0].to_bits(), c[1][1].to_bits(), c[1][2].to_bits()])
        .collect();
    segs.sort_unstable();
    segs
}

fn lerp(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

#[test]
fn start_flies_to_iso() {
    let mut vp = Viewport::new();
    vp.set_view_size(800.0, 600.0);
    vp.camera_mut().set_time(0.0);
    assert!(close(vp.camera().position, START_POSITION, 1e-12));
    assert!(close(vp.camera().target, [0.0; 3], 1e-12));

    vp.start_fly_in();
    let (goal_t, goal_p, goal_u) = vp.camera().goal();
    // empty scene: centre on the origin, distance 60 / sin(17.5°) * 1.05 along the iso direction
    assert!(close(goal_t, [0.0, 0.0, 0.0], 1e-9), "{goal_t:?}");
    assert!(close(goal_p, [126.97399998524338, -126.97399998524338, 107.92789998745688], 1e-6), "{goal_p:?}");
    assert!(close(goal_u, [0.0, 0.0, 1.0], 1e-12));

    // 0 ms: still at the start
    assert!(vp.camera_mut().animate(0.0));
    assert!(close(vp.camera().position, START_POSITION, 1e-9));
    assert!(close(vp.camera().target, [0.0; 3], 1e-12));
    assert!(vp.camera().is_flying());

    // 160 ms: k = 0.5, ease = 1 - 0.5^3 = 0.875
    assert!(vp.camera_mut().animate(0.16));
    let mid = lerp(START_POSITION, goal_p, 0.875);
    assert!(close(vp.camera().position, mid, 1e-6), "{:?} vs {mid:?}", vp.camera().position);
    assert!(close(vp.camera().target, [0.0; 3], 1e-9));
    assert!(close(vp.camera().up, [0.0, 0.0, 1.0], 1e-9));
    assert!(close(mid, [131.10224998708796, -138.60224998708796, 113.18691248902476], 1e-6));

    // 320 ms: landed, flight over
    assert!(!vp.camera_mut().animate(0.32));
    assert!(close(vp.camera().position, goal_p, 1e-9), "{:?}", vp.camera().position);
    assert!(close(vp.camera().up, goal_u, 1e-12));
    assert!(!vp.camera().is_flying());

    // user input stops the flight where it is; it does not run on to the goal
    {
        let cam = vp.camera_mut();
        cam.position = START_POSITION;
        cam.target = [0.0; 3];
        cam.up = [0.0, 0.0, 1.0];
        cam.set_time(1.0);
    }
    vp.start_fly_in();
    assert!(vp.camera_mut().animate(1.16));
    let held = vp.camera().position;
    assert!(!close(held, goal_p, 1.0), "still at the start of the ease, not the goal");
    vp.camera_mut().cancel_flight();
    assert!(!vp.camera().is_flying());
    assert!(!vp.camera_mut().animate(1.32));
    assert!(close(vp.camera().position, held, 1e-12));

    // a drag does the same (rotate clears the flight before it moves the camera)
    {
        let cam = vp.camera_mut();
        cam.position = START_POSITION;
        cam.target = [0.0; 3];
        cam.up = [0.0, 0.0, 1.0];
        cam.set_time(2.0);
    }
    vp.start_fly_in();
    assert!(vp.camera_mut().animate(2.10));
    vp.camera_mut().rotate(0.0, 0.0);
    assert!(!vp.camera().is_flying());
}

#[test]
fn sketch_plane_grid_matches_sketcher() {
    let mut vp = Viewport::new();
    let xy = PlaneFrame { origin: [0.0; 3], x: [1.0, 0.0, 0.0], y: [0.0, 1.0, 0.0], normal: [0.0, 0.0, 1.0] };
    vp.set_sketch_plane_grid(Some(xy));
    assert!(vp.sketch_plane_grid_visible());
    // empty scene: R = 100, step 5, half 100. 41 stations, 9 of them major.
    let (sheet, lines, axes) = scene::sketch_plane_grid(&xy.as_frame(), None);
    assert_eq!(sheet.len(), 6);
    assert_eq!(lines.len(), 41 * 4);
    assert_eq!(axes.len(), 4);
    assert!(close([axes[0].pos[0] as f64, axes[0].pos[1] as f64, axes[0].pos[2] as f64], [-100.0, 0.0, 0.0], 1e-6));
    assert_eq!(axes[0].c0, [0xe2, 0x55, 0x55, 255]);
    assert_eq!(axes[2].c0, [0x22, 0xa3, 0x5a, 255]);
    // sheet 0xfff1e8 at 0.18 → alpha 46
    assert_eq!(sheet[0].c0, [0xff, 0xf1, 0xe8, 46]);
    // a major station (v = 0) is the stronger rust colour
    assert!(lines.iter().any(|v| v.c0[3] == (0.32_f32 * 255.0).round() as u8 && v.c0[0] == 0xc5));
    vp.set_sketch_plane_grid(None);
    assert!(!vp.sketch_plane_grid_visible());
}

#[test]
fn sliced_preview_matches_prepare_mesh() {
    let mesh = Arc::new(preview_prep::bench_mesh(2000));
    let mut crease = CreaseBuild::new(&mesh, scene::CREASE_DEG);
    let mut steps = 0;
    let sliced = loop {
        steps += 1;
        if let Some(v) = crease.step(&mesh, Some(Duration::ZERO)) {
            break v;
        }
        assert!(steps < 100, "crease did not yield");
    };
    assert!(steps > 1, "a 2000-triangle mesh should take more than one slice");
    assert_eq!(sliced, scene::creased_normals(&mesh, scene::CREASE_DEG));

    let mut batch = Batch::new(vec![mesh.clone()], vec![Arc::new(OnceLock::<Bvh>::new())], Arc::new(AtomicUsize::new(0)));
    let got = batch.poll(None).expect("budget None finishes");
    let direct = scene::prepare_mesh(&mesh);
    assert_eq!(got[0].vertices, direct.vertices);
    // Open edges are appended from a HashMap, so their order is not part of the mesh.
    assert_eq!(edge_set(&got[0].edges), edge_set(&direct.edges));
    assert_eq!(got[0].triangles, direct.triangles);
    assert_eq!(got[0].flat, direct.flat);
    assert_eq!(got[0].bbox, direct.bbox);
}

/// A handle drag posts a new preview every frame. On wasm the picking tree used to wait for
/// the next frame, so a small mesh that was replaced every frame never appeared.
#[test]
fn small_preview_replaced_every_frame_still_shows() {
    for _ in 0..5 {
        let mesh = Arc::new(preview_prep::bench_mesh(32));
        let tree = Arc::new(OnceLock::new());
        let mut batch = Batch::new(vec![mesh], vec![tree.clone()], Arc::new(AtomicUsize::new(0)));
        let got = batch.poll(Some(preview_prep::SLICE)).expect("a 32-triangle preview finishes in one slice");
        assert_eq!(got[0].triangles, 32);
        assert!(tree.get().is_some(), "the picking tree is built in that same slice");
    }
}

/// The mesh step can outlast a quarter of the slice. Forced here with a zero budget, so the
/// old clock check treats the step as already slow and leaves the tree for a frame a drag
/// never gives it. A mesh at [`preview_prep::ONE_SLICE_TRIS`] still finishes; a larger one stays sliced.
#[test]
fn small_preview_slow_step_finishes_in_its_frame() {
    let budget = Duration::ZERO;
    let n = preview_prep::ONE_SLICE_TRIS;
    let mesh = Arc::new(preview_prep::bench_mesh(n));
    let tree = Arc::new(OnceLock::new());
    let mut batch = Batch::new(vec![mesh], vec![tree.clone()], Arc::new(AtomicUsize::new(0)));
    let got = batch.poll(Some(budget)).expect("a small preview finishes in the frame it arrives in");
    assert_eq!(got[0].triangles, n);
    assert!(tree.get().is_some(), "the picking tree is built in that same slice");

    let big = Arc::new(preview_prep::bench_mesh(n + 1));
    let big_tree = Arc::new(OnceLock::new());
    let mut batch = Batch::new(vec![big], vec![big_tree.clone()], Arc::new(AtomicUsize::new(0)));
    assert!(batch.poll(Some(budget)).is_none(), "a larger preview stays sliced");
    assert!(big_tree.get().is_none(), "its picking tree waits for a later slice");
}
