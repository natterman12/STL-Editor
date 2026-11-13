//! Task 5: picking. Rays and plane hits are the numbers from 1.x `viewer.js`
//! (`Raycaster.setFromCamera` and `Ray.intersectPlane` in three r170).
//!
//! Native only: the GPU and the worker threads these tests use do not exist on wasm32.
#![cfg(not(target_arch = "wasm32"))]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use stl_app::engine_link::{BodyView, PlaneView};
use stl_app::viewport::pick::Hit;
use stl_app::viewport::{PlaneFrame, PreviewMesh, PreviewPack, Viewport};
use stl_kernel::MeshArrays;
use stl_mesh::bvh::ray_triangle;

#[path = "support/gpu.rs"]
mod gpu;

fn close(a: [f64; 3], b: [f64; 3], tol: f64) -> bool {
    a.iter().zip(b).all(|(p, q)| (p - q).abs() <= tol)
}

fn body(id: &str, mesh: MeshArrays, hidden: bool) -> BodyView {
    BodyView { id: id.into(), name: id.into(), color: None, hidden, solid: true, mesh: Arc::new(mesh), revision: 1 }
}

/// 20 mm cube on the bed, centred in X and Y. Face 4 is the y = -10 side (triangles 0,1,5).
fn cube_mesh() -> MeshArrays {
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
    MeshArrays { positions: p.iter().flatten().copied().map(|v| v as f32).collect(), triangles: t.to_vec() }
}

fn quad_z(z: f64, half: f64) -> MeshArrays {
    let p = [[-half, -half, z], [half, -half, z], [half, half, z], [-half, half, z]];
    MeshArrays {
        positions: p.iter().flatten().map(|v| *v as f32).collect(),
        triangles: vec![0, 1, 2, 0, 2, 3],
    }
}

fn grid(n: usize, z: f64, half: f64) -> MeshArrays {
    let mut positions = Vec::new();
    let mut triangles = Vec::new();
    let step = (2.0 * half) / n as f64;
    for iy in 0..=n {
        for ix in 0..=n {
            positions.push((-half + ix as f64 * step) as f32);
            positions.push((-half + iy as f64 * step) as f32);
            positions.push(z as f32);
        }
    }
    let row = (n + 1) as u32;
    for iy in 0..n as u32 {
        for ix in 0..n as u32 {
            let i = iy * row + ix;
            triangles.extend_from_slice(&[i, i + 1, i + row + 1, i, i + row + 1, i + row]);
        }
    }
    MeshArrays { positions, triangles }
}

fn merge(mut a: MeshArrays, b: MeshArrays) -> MeshArrays {
    let offset = (a.positions.len() / 3) as u32;
    a.triangles.extend(b.triangles.into_iter().map(|i| i + offset));
    a.positions.extend(b.positions);
    a
}

fn view(mesh: MeshArrays) -> (Viewport, Arc<MeshArrays>) {
    let mesh = Arc::new(mesh);
    let mut vp = Viewport::new();
    vp.set_view_size(800.0, 600.0);
    vp.sync_bodies(&[BodyView {
        id: "part".into(),
        name: "Part".into(),
        color: None,
        hidden: false,
        solid: true,
        mesh: Arc::clone(&mesh),
        revision: 1,
    }]);
    vp.flush();
    (vp, mesh)
}

fn brute_front(m: &MeshArrays, origin: [f64; 3], dir: [f64; 3]) -> Option<u32> {
    let n = m.triangles.len() / 3;
    let mut best_t = f64::INFINITY;
    let mut best = None;
    for i in 0..n {
        let b = i * 3;
        let p = |v: u32| {
            let s = v as usize * 3;
            [m.positions[s] as f64, m.positions[s + 1] as f64, m.positions[s + 2] as f64]
        };
        let t = ray_triangle(origin, dir, p(m.triangles[b]), p(m.triangles[b + 1]), p(m.triangles[b + 2]), true);
        if let Some(t) = t {
            if t < best_t {
                best_t = t;
                best = Some(i as u32);
            }
        }
    }
    best
}

struct Lcg(u64);
impl Lcg {
    fn unit(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        ((self.0 >> 11) as f64) * (1.0 / ((1u64 << 53) as f64))
    }
}

fn compare_rays(vp: &Viewport, mesh: &MeshArrays, n: usize, seed: u64) {
    let center = egui::pos2(vp.rect().width() * 0.5, vp.rect().height() * 0.5);
    assert!(vp.pick(center, true, false).is_some(), "centre ray missed the fixture");
    let mut rng = Lcg(seed);
    let mut hits = 0u32;
    let (w, h) = (vp.rect().width() as f64, vp.rect().height() as f64);
    for i in 0..n {
        let pos = egui::pos2((rng.unit() * w) as f32, (rng.unit() * h) as f32);
        let (origin, dir) = vp.ray(pos);
        let expect = brute_front(mesh, origin, dir);
        let got = match vp.pick(pos, true, false) {
            Some(Hit::Body { id, face, .. }) => {
                assert_eq!(id, "part");
                Some(face)
            }
            Some(other) => panic!("ray {i}: {other:?}"),
            None => None,
        };
        assert_eq!(got, expect, "ray {i} at {pos:?}");
        if got.is_some() {
            hits += 1;
        }
    }
    assert!(hits >= 10, "only {hits} of {n} rays hit");
}

#[test]
fn pick_matches_brute_force() {
    // Start camera (160, -220, 150) looks at the origin. The centre pixel of an 800×600 view
    // is that look direction, normalize(-160, 220, -150). It meets the cube's y = -10 face
    // (triangle 0,1,5, face 4) at x = 160/22 = 80/11, z = 150/22 = 75/11. The face normal
    // is -Y and the ray travels +Y, so the face is in front.
    let (vp, mesh) = view(cube_mesh());
    let pos = egui::pos2(400.0, 300.0);
    match vp.pick(pos, true, false) {
        Some(Hit::Body { id, face, point, normal }) => {
            assert_eq!(id, "part");
            assert_eq!(face, 4);
            assert!(close(point, [80.0 / 11.0, -10.0, 75.0 / 11.0], 1e-4), "{point:?}");
            assert!(close(normal, [0.0, -1.0, 0.0], 1e-6), "{normal:?}");
            let (o, d) = vp.ray(pos);
            assert_eq!(brute_front(&mesh, o, d), Some(4));
        }
        other => panic!("expected the cube's near side, got {other:?}"),
    }

    let (mut vp, mesh) = view(cube_mesh());
    vp.fit_all();
    vp.camera_mut().finish_flight();
    compare_rays(&vp, &mesh, 1000, 1);

    let (mut vp, mesh) = view(grid(10, 0.0, 40.0));
    vp.fit_all();
    vp.camera_mut().finish_flight();
    compare_rays(&vp, &mesh, 1000, 2);

    let (mut vp, mesh) = view(merge(grid(8, 0.0, 30.0), grid(8, 18.0, 30.0)));
    vp.fit_all();
    vp.camera_mut().finish_flight();
    compare_rays(&vp, &mesh, 1000, 3);
}

#[test]
fn pick_empty_scene() {
    let mut vp = Viewport::new();
    vp.set_view_size(800.0, 600.0);
    let center = egui::pos2(400.0, 300.0);
    assert!(vp.pick(center, true, true).is_none());
    assert!(vp.pick(center, true, false).is_none());
    assert!(vp.pick(center, false, true).is_none());

    vp.sync_bodies(&[body("hidden", cube_mesh(), true)]);
    assert!(vp.pick(center, true, true).is_none());

    // a construction plane that is not shown is not a target
    vp.sync_planes(&[PlaneView {
        id: "floor".into(),
        name: "Floor".into(),
        origin: [0.0; 3],
        x: [1.0, 0.0, 0.0],
        y: [0.0, 1.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        visible: false,
    }]);
    assert!(vp.pick(center, false, true).is_none());

    vp.sync_planes(&[PlaneView {
        id: "floor".into(),
        name: "Floor".into(),
        origin: [0.0; 3],
        x: [1.0, 0.0, 0.0],
        y: [0.0, 1.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        visible: true,
    }]);
    match vp.pick(center, false, true) {
        Some(Hit::Plane { id, point }) => {
            assert_eq!(id, "floor");
            assert!(close(point, [0.0; 3], 1e-4), "{point:?}");
        }
        other => panic!("expected the floor, got {other:?}"),
    }

    vp.sync_planes(&[]);
    vp.set_base_planes_visible(true);
    match vp.pick(center, true, true) {
        Some(Hit::Plane { id, .. }) => assert_eq!(id, "XY"),
        other => panic!("expected the XY base plane, got {other:?}"),
    }
    vp.set_base_planes_visible(false);
    assert!(vp.pick(center, true, true).is_none());
}

#[test]
fn pick_skips_hidden() {
    let mut vp = Viewport::new();
    vp.set_view_size(800.0, 600.0);
    // The start camera's centre ray hits z = 40 near (42.7, -58.7) and z = 0 at the origin.
    // The hidden sheet is closer and covers both; the shown sheet covers only the origin.
    vp.sync_bodies(&[body("near", quad_z(40.0, 100.0), true), body("far", quad_z(0.0, 50.0), false)]);
    vp.flush();
    let center = egui::pos2(400.0, 300.0);
    match vp.pick(center, true, true) {
        Some(Hit::Body { id, .. }) => assert_eq!(id, "far"),
        other => panic!("expected the shown sheet, got {other:?}"),
    }
    assert!(vp.pick(center, false, false).is_none());

    // A pixel whose ray meets (90, -90, 40): inside the hidden sheet, outside the shown one.
    let off = vp.world_to_screen([90.0, -90.0, 40.0]);
    assert!(vp.pick(off, true, true).is_none(), "hidden sheet was hit at {off:?}");

    vp.sync_bodies(&[body("near", quad_z(40.0, 100.0), true)]);
    vp.flush();
    assert!(vp.pick(center, true, true).is_none());
}

#[test]
fn ray_to_plane_matches_viewer_js() {
    // viewer.js: setFromCamera(ndc, camera) then Plane.setFromNormalAndCoplanarPoint(normal, origin)
    // and Ray.intersectPlane. NDC of pixel (x, y) in a view (w, h) is
    // (x/w*2 - 1, -(y/h)*2 + 1). The centre of the start view looks along
    // normalize(-160, 220, -150) and the XY plane contains the origin, so the hit is the origin.
    let mut vp = Viewport::new();
    vp.set_view_size(800.0, 600.0);
    let xy = PlaneFrame { origin: [0.0; 3], x: [1.0, 0.0, 0.0], y: [0.0, 1.0, 0.0], normal: [0.0, 0.0, 1.0] };
    let p = vp.ray_to_plane(egui::pos2(400.0, 300.0), &xy).unwrap();
    assert!(close(p, [0.0; 3], 1e-6), "{p:?}");

    // Same camera, plane z = 400. The ray travels toward -Z, so the hit is behind the camera.
    let behind = PlaneFrame { origin: [0.0, 0.0, 400.0], x: [1.0, 0.0, 0.0], y: [0.0, 1.0, 0.0], normal: [0.0, 0.0, 1.0] };
    assert!(vp.ray_to_plane(egui::pos2(400.0, 300.0), &behind).is_none());

    // Camera on the -Y axis, looking at the origin, up +Z, 35° fov, 800×600.
    // Pixel (200, 150) is NDC (-0.5, 0.5). Unproject (three's Vector3.unproject at GL depth 0.5)
    // gives direction about (-0.2032987314, 0.9671717998, 0.1524740486). The plane y = 0 then
    // meets the ray at (-21.0199192586, 0, 15.7649394439).
    {
        let c = vp.camera_mut();
        c.position = [0.0, -100.0, 0.0];
        c.target = [0.0, 0.0, 0.0];
        c.up = [0.0, 0.0, 1.0];
    }
    let ground = PlaneFrame { origin: [0.0; 3], x: [1.0, 0.0, 0.0], y: [0.0, 0.0, 1.0], normal: [0.0, 1.0, 0.0] };
    let p = vp.ray_to_plane(egui::pos2(200.0, 150.0), &ground).unwrap();
    assert!(close(p, [-21.019919258598833, 0.0, 15.764939443949126], 1e-6), "{p:?}");

    // Centre pixel: the ray is +Y and runs 100 mm onto y = 0.
    let p = vp.ray_to_plane(egui::pos2(400.0, 300.0), &ground).unwrap();
    assert!(close(p, [0.0; 3], 1e-6), "{p:?}");

    // y = -200 is behind the camera.
    let back = PlaneFrame { origin: [0.0, -200.0, 0.0], x: [1.0, 0.0, 0.0], y: [0.0, 0.0, 1.0], normal: [0.0, 1.0, 0.0] };
    assert!(vp.ray_to_plane(egui::pos2(400.0, 300.0), &back).is_none());

    // The centre ray is +Y, so a horizontal plane (normal +Z) is parallel and is missed.
    let flat = PlaneFrame { origin: [0.0, 0.0, 10.0], x: [1.0, 0.0, 0.0], y: [0.0, 1.0, 0.0], normal: [0.0, 0.0, 1.0] };
    assert!(vp.ray_to_plane(egui::pos2(400.0, 300.0), &flat).is_none());

    // Pixel (560, 420) is NDC (0.4, -0.4). Direction about (0.1645631439, 0.9786142719, -0.1234223579).
    // Plane y = 10: t = (10 - (-100)) / dir.y, point (18.4975289476, 10, -13.8731467107).
    let shelf = PlaneFrame { origin: [2.0, 10.0, -3.0], x: [1.0, 0.0, 0.0], y: [0.0, 0.0, 1.0], normal: [0.0, 1.0, 0.0] };
    let p = vp.ray_to_plane(egui::pos2(560.0, 420.0), &shelf).unwrap();
    assert!(close(p, [18.497528947566977, 10.0, -13.873146710675233], 1e-6), "{p:?}");
}

#[test]
fn pick_preview_returns_its_id() {
    let mut vp = Viewport::new();
    vp.set_view_size(800.0, 600.0);
    // The centre ray hits z = 30 near (32, -44), inside the near sheet and in front of z = 0.
    vp.show_preview(&PreviewPack {
        meshes: vec![
            PreviewMesh { id: "far".into(), mesh: Arc::new(quad_z(0.0, 40.0)) },
            PreviewMesh { id: "near".into(), mesh: Arc::new(quad_z(30.0, 100.0)) },
        ],
        ..PreviewPack::default()
    });
    assert_eq!(vp.pick_preview(egui::pos2(400.0, 300.0)).as_deref(), Some("near"));
    vp.clear_preview();
    assert!(vp.pick_preview(egui::pos2(400.0, 300.0)).is_none());
}

#[test]
fn pick_without_tree_matches_brute_force() {
    // No picking tree (as in the frames before the worker has built it): the direct test.
    let mesh = Arc::new(merge(grid(8, 0.0, 30.0), grid(8, 18.0, 30.0)));
    let mut vp = Viewport::new();
    vp.set_view_size(800.0, 600.0);
    vp.set_pick_trees(false);
    vp.sync_bodies(&[BodyView { id: "part".into(), name: "Part".into(), color: None, hidden: false, solid: true, mesh: mesh.clone(), revision: 1 }]);
    vp.flush();
    assert!(!vp.pick_tree_ready("part"));
    vp.fit_all();
    vp.camera_mut().finish_flight();
    compare_rays(&vp, &mesh, 500, 4);

    // with the tree, after flush (which waits for it)
    let (vp, _) = view(cube_mesh());
    assert!(vp.pick_tree_ready("part"));
}

#[test]
fn pick_with_section_cut() {
    // One body of two sheets: z = 40 (faces 0, 1) in front of z = 0 (faces 2, 3) along the
    // start camera's centre ray, a construction plane at z = 0 too.
    for trees in [true, false] {
        let mut vp = Viewport::new();
        vp.set_view_size(800.0, 600.0);
        vp.set_pick_trees(trees);
        vp.sync_bodies(&[body("part", merge(quad_z(40.0, 100.0), quad_z(0.0, 50.0)), false)]);
        vp.flush();
        let center = egui::pos2(400.0, 300.0);
        let face = |vp: &Viewport| match vp.pick(center, true, false) {
            Some(Hit::Body { face, point, .. }) => (face, point[2]),
            other => panic!("expected the part, got {other:?}"),
        };
        let (f, z) = face(&vp);
        assert!(f < 2 && (z - 40.0).abs() < 1e-3, "no cut: near sheet, got face {f} at z {z}");

        // keep z <= 20 (normal -Z): the near sheet is cut away, the far one is hit
        vp.set_section_plane(Some([0.0, 0.0, -1.0, 20.0]));
        vp.set_section(true);
        let (f, z) = face(&vp);
        assert!(f >= 2 && z.abs() < 1e-3, "cut on: far sheet, got face {f} at z {z}");

        // keep z >= 10: the far sheet and the plane at z = 0 are gone, the near sheet stays
        vp.set_section_plane(Some([0.0, 0.0, 1.0, -10.0]));
        vp.sync_planes(&[PlaneView { id: "floor".into(), name: "Floor".into(), origin: [0.0; 3], x: [1.0, 0.0, 0.0], y: [0.0, 1.0, 0.0], normal: [0.0, 0.0, 1.0], visible: true }]);
        assert!(vp.pick(center, false, true).is_none(), "the cut-away plane was hit");
        let (f, _) = face(&vp);
        assert!(f < 2);

        // off again: the plane is kept, the cut no longer applies
        vp.set_section(false);
        assert!(matches!(vp.pick(center, false, true), Some(Hit::Plane { .. })));
    }
}

#[test]
fn body_just_behind_a_plane_wins() {
    // viewer.js:562 — a body hit wins over a nearer plane when it is less than extent * 0.002
    // further along the ray (planes are see-through).
    let center = egui::pos2(400.0, 300.0);
    let floor = PlaneView { id: "floor".into(), name: "Floor".into(), origin: [0.0; 3], x: [1.0, 0.0, 0.0], y: [0.0, 1.0, 0.0], normal: [0.0, 0.0, 1.0], visible: true };
    let mut probe = Viewport::new();
    probe.set_view_size(800.0, 600.0);
    let (_, d) = probe.ray(center);
    for (gap_in_thresholds, want_body) in [(0.5, true), (0.99, true), (1.01, false), (3.0, false), (-1.0, true)] {
        let mut vp = Viewport::new();
        vp.set_view_size(800.0, 600.0);
        // a 100 mm sheet: extent = its diagonal, whatever its height
        let threshold = (100.0f64 * 100.0 * 2.0).sqrt() * 0.002;
        // the sheet sits below the plane by this much, measured along the ray
        let dz = gap_in_thresholds * threshold * -d[2];
        vp.sync_bodies(&[body("sheet", quad_z(-dz, 50.0), false)]);
        vp.flush();
        assert!((vp.extent() * 0.002 - threshold).abs() < 1e-9);
        vp.sync_planes(std::slice::from_ref(&floor));
        match vp.pick(center, true, true) {
            Some(Hit::Body { id, .. }) => assert!(want_body, "gap {gap_in_thresholds}: got the body {id}"),
            Some(Hit::Plane { id, .. }) => assert!(!want_body, "gap {gap_in_thresholds}: got the plane {id}"),
            None => panic!("gap {gap_in_thresholds}: nothing hit"),
        }
    }
}

#[test]
fn pick_uses_the_revision_on_screen() {
    let Some(gpu) = gpu::headless() else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    // the view is the size it is rendered at: the centre pixel is the start camera's look line
    let center = egui::pos2(40.0, 30.0);
    let mut vp = Viewport::new();
    vp.set_view_size(80.0, 60.0);
    let mut b = body("part", quad_z(0.0, 50.0), false);
    vp.sync_bodies(std::slice::from_ref(&b));
    vp.flush();
    gpu.render(&mut vp, 80, 60).expect("a picture");
    gpu.render(&mut vp, 80, 60).expect("a picture");
    assert_eq!(vp.shown_revision("part"), Some(1));
    let old_mesh = vp.body_mesh("part").unwrap().clone();

    // revision 2, a sheet higher up: prepared, but not on the GPU yet
    b.mesh = Arc::new(quad_z(30.0, 100.0));
    b.revision = 2;
    vp.sync_bodies(std::slice::from_ref(&b));
    vp.flush();
    let _ = vp.frame_for_size(80.0, 60.0, 1.0);
    assert_eq!(vp.shown_revision("part"), Some(1), "promoted before its upload");
    assert!(Arc::ptr_eq(vp.body_mesh("part").unwrap(), &old_mesh), "highlights would use the new mesh");
    assert!(vp.scene_box().unwrap().max[2].abs() < 1e-9, "the box moved before the triangles did");
    match vp.pick(center, true, false) {
        Some(Hit::Body { point, .. }) => assert!(point[2].abs() < 1e-3, "{point:?}"),
        other => panic!("{other:?}"),
    }

    // the upload lands; the next frame picks the new sheet
    gpu.render(&mut vp, 80, 60).expect("a picture");
    assert_eq!(vp.stats().on_gpu("part"), Some(2));
    gpu.render(&mut vp, 80, 60).expect("a picture");
    assert_eq!(vp.shown_revision("part"), Some(2));
    assert!((vp.scene_box().unwrap().max[2] - 30.0).abs() < 1e-6);
    match vp.pick(center, true, false) {
        Some(Hit::Body { point, .. }) => assert!((point[2] - 30.0).abs() < 1e-3, "{point:?}"),
        other => panic!("{other:?}"),
    }
}

/// Pick timing on the bench mesh (a timer only; the file is never compared against a golden
/// mesh). Three numbers:
/// - `direct`: the first pick once the mesh is on screen, while the worker may still be building
///   the picking tree (then every triangle is tested). Must fit in a frame.
/// - `cold`: the first pick with the tree built, nothing warmed up before it.
/// - `median`: nine picks after that. Cold and median must be under 2 ms in release.
///
/// The tree is built on the worker, so its build time is in none of these.
#[test]
fn pick_bench_mesh_under_2ms() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../dev/bench/test_mesh.stl");
    if !path.is_file() {
        eprintln!("skip pick_bench_mesh_under_2ms: {} is missing", path.display());
        return;
    }
    let bytes = std::fs::read(&path).expect("read bench mesh");
    let mut vp = Viewport::new();
    vp.set_view_size(1280.0, 720.0);
    vp.load_dev_mesh("test_mesh.stl", &bytes).expect("load bench mesh");
    let id = vp.bodies().next().expect("a body").id.clone();
    // wait (without blocking on the tree) until the mesh is prepared: what a frame would see
    let until = Instant::now() + std::time::Duration::from_secs(120);
    while vp.shown_revision(&id).is_none() && Instant::now() < until {
        let _ = vp.frame_for_size(1280.0, 720.0, 1.0);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(vp.shown_revision(&id).is_some(), "the bench mesh was never prepared");
    vp.camera_mut().finish_flight();
    let center = egui::pos2(640.0, 360.0);
    let tree_then = vp.pick_tree_ready(&id);
    let t0 = Instant::now();
    let direct_hit = vp.pick(center, true, false).is_some();
    let direct = t0.elapsed().as_secs_f64() * 1.0e3;

    vp.flush();
    assert!(vp.pick_tree_ready(&id));
    let t0 = Instant::now();
    let cold_hit = vp.pick(center, true, false).is_some();
    let cold = t0.elapsed().as_secs_f64() * 1.0e3;
    let mut ms = [0.0f64; 9];
    for sample in &mut ms {
        let t0 = Instant::now();
        assert_eq!(vp.pick(center, true, false).is_some(), cold_hit);
        *sample = t0.elapsed().as_secs_f64() * 1.0e3;
    }
    ms.sort_by(|a, b| a.total_cmp(b));
    let median = ms[4];
    eprintln!(
        "pick_bench_mesh_under_2ms: direct {direct:.3} ms (tree {} then, hit {direct_hit}), cold {cold:.4} ms, median {median:.4} ms (min {:.4}, max {:.4}), hit {cold_hit}",
        if tree_then { "ready" } else { "not ready" },
        ms[0],
        ms[8]
    );
    assert!(cold_hit, "the centre ray missed the bench mesh");
    assert_eq!(direct_hit, cold_hit);
    // The budgets are for a release build. Debug is several times slower and is not the target.
    if !cfg!(debug_assertions) {
        assert!(cold < 2.0, "cold pick {cold:.4} ms");
        assert!(median < 2.0, "median pick {median:.4} ms, samples {ms:?}");
        assert!(direct < 16.7, "pick without the tree {direct:.3} ms");
    }
}
