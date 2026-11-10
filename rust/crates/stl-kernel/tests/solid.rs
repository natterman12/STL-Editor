use stl_kernel::{KernelError, MeshArrays, Solid};

/// A 10 mm cube, 8 vertices and 12 triangles, counter-clockwise seen from outside.
fn cube_mesh() -> MeshArrays {
    #[rustfmt::skip]
    let positions = vec![
        0.0, 0.0, 0.0,   10.0, 0.0, 0.0,   10.0, 10.0, 0.0,   0.0, 10.0, 0.0,
        0.0, 0.0, 10.0,  10.0, 0.0, 10.0,  10.0, 10.0, 10.0,  0.0, 10.0, 10.0,
    ];
    #[rustfmt::skip]
    let triangles = vec![
        0, 2, 1,  0, 3, 2, // bottom
        0, 1, 5,  0, 5, 4, // front
        1, 2, 6,  1, 6, 5, // right
        2, 3, 7,  2, 7, 6, // back
        3, 0, 4,  3, 4, 7, // left
        4, 5, 6,  4, 6, 7, // top (last two triangles)
    ];
    MeshArrays { positions, triangles }
}

fn rel(got: f64, want: f64) -> f64 {
    ((got - want) / want).abs()
}

#[test]
fn cube_minus_sphere_matches_cpp() {
    // Values from 1.x manifold3d (C++ Manifold).
    let s = Solid::cube([10.0; 3], true).difference(&Solid::sphere(6.0, 64));
    let (volume, area, tris) = (s.volume(), s.surface_area(), s.num_tri());
    assert!(
        rel(volume, 204.79137941438853) <= 1e-9,
        "volume {volume:?} area {area:?} tris {tris}"
    );
    assert!(
        rel(area, 622.7683431285991) <= 1e-9,
        "volume {volume:?} area {area:?} tris {tris}"
    );
    assert_eq!(tris, 1640, "volume {volume:?} area {area:?}");
}

#[test]
fn round_trip_mesh() {
    let s = Solid::cube([10.0; 3], true).difference(&Solid::sphere(6.0, 64));
    let mesh = s.to_mesh();
    assert_eq!(mesh.positions.len() % 3, 0);
    assert_eq!(mesh.triangles.len(), s.num_tri() * 3);
    let back = Solid::from_mesh(&mesh).expect("a kernel mesh is closed");
    assert_eq!(back.num_tri(), s.num_tri());
}

#[test]
fn from_mesh_accepts_a_closed_cube() {
    let s = Solid::from_mesh(&cube_mesh()).expect("closed cube");
    assert!((s.volume() - 1000.0).abs() < 1e-6);
    assert_eq!(s.num_tri(), 12);
    let b = s.bounding_box();
    assert_eq!((b.min, b.max), ([0.0; 3], [10.0; 3]));
}

#[test]
fn from_mesh_rejects_bad_input() {
    let invalid = |m: &MeshArrays| matches!(Solid::from_mesh(m), Err(KernelError::Invalid(_)));

    // an open box: the cube without its top two triangles
    let mut open = cube_mesh();
    open.triangles.truncate(30);
    assert!(matches!(Solid::from_mesh(&open), Err(KernelError::NotClosed)));

    let mut nan = cube_mesh();
    nan.positions[4] = f32::NAN;
    assert!(invalid(&nan));

    let mut inf = cube_mesh();
    inf.positions[0] = f32::INFINITY;
    assert!(invalid(&inf));

    assert!(invalid(&MeshArrays { positions: vec![], triangles: vec![] }));

    let mut no_tris = cube_mesh();
    no_tris.triangles.clear();
    assert!(invalid(&no_tris));

    let mut index = cube_mesh();
    index.triangles[7] = 99;
    assert!(invalid(&index));

    let mut ragged_positions = cube_mesh();
    ragged_positions.positions.pop();
    assert!(invalid(&ragged_positions));

    let mut ragged_triangles = cube_mesh();
    ragged_triangles.triangles.pop();
    assert!(invalid(&ragged_triangles));
}

#[test]
fn split_volumes_add_up() {
    let s = Solid::cube([10.0; 3], true).difference(&Solid::sphere(6.0, 64));
    let (a, b) = s.split_by_plane([0.0, 0.0, 1.0], 0.0);
    assert!(a.volume() > 0.0 && b.volume() > 0.0);
    assert!(rel(a.volume() + b.volume(), s.volume()) < 1e-9);
    let trimmed = s.trim_by_plane([0.0, 0.0, 1.0], 0.0);
    assert!(rel(trimmed.volume(), a.volume()) < 1e-9);
    assert!(trimmed.bounding_box().min[2] >= -1e-9);
}

#[test]
fn decompose_sorted() {
    let small = Solid::sphere(3.0, 32);
    let big = Solid::sphere(5.0, 32).translate([20.0, 0.0, 0.0]);
    // small first, so the order has to come from the sort
    let parts = Solid::compose(&[small.clone(), big.clone()]).decompose();
    assert_eq!(parts.len(), 2);
    assert!(rel(parts[0].volume(), big.volume()) < 1e-9);
    assert!(rel(parts[1].volume(), small.volume()) < 1e-9);
}

#[test]
fn transforms_and_measures() {
    let c = Solid::cube([2.0, 3.0, 4.0], false);
    assert!(rel(c.volume(), 24.0) < 1e-12);
    assert!(rel(c.surface_area(), 52.0) < 1e-12);
    assert_eq!((c.num_tri(), c.num_vert(), c.genus()), (12, 8, 0));
    assert!(!c.is_empty());
    assert!(Solid::cube([1.0; 3], false).intersection(&c.translate([5.0, 0.0, 0.0])).is_empty());

    let moved = c.translate([1.0, 0.0, 0.0]).scale([2.0, 1.0, 1.0]);
    let b = moved.bounding_box();
    assert_eq!((b.min, b.max), ([2.0, 0.0, 0.0], [6.0, 3.0, 4.0]));

    // row-major 3x4: the same move and stretch as one matrix
    let m = c.transform([[2.0, 0.0, 0.0, 2.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]]);
    let b = m.bounding_box();
    assert_eq!((b.min, b.max), ([2.0, 0.0, 0.0], [6.0, 3.0, 4.0]));

    let r = c.rotate_deg([0.0, 0.0, 90.0]).bounding_box();
    assert!((r.min[0] + 3.0).abs() < 1e-9 && (r.max[1] - 2.0).abs() < 1e-9);

    let mirrored = c.mirror([1.0, 0.0, 0.0]);
    assert!(rel(mirrored.volume(), 24.0) < 1e-12);
    assert_eq!(mirrored.bounding_box().min[0], -2.0);

    let all = Solid::union_all(&[c.clone(), c.translate([1.0, 0.0, 0.0]), c.translate([10.0, 0.0, 0.0])]);
    assert!(rel(all.volume(), 24.0 + 12.0 + 24.0) < 1e-9);
    assert!(rel(c.union(&c.translate([1.0, 0.0, 0.0])).volume(), 36.0) < 1e-9);

    assert!(c.refine_to_length(1.0).num_tri() > 12);
    assert!(rel(c.refine_to_length(1.0).volume(), 24.0) < 1e-9);

    let cyl = Solid::cylinder(10.0, 2.0, 2.0, 64, true);
    let b = cyl.bounding_box();
    assert!((b.min[2] + 5.0).abs() < 1e-9 && (b.max[2] - 5.0).abs() < 1e-9);
    assert!(rel(cyl.volume(), std::f64::consts::PI * 40.0) < 2e-3);
}

#[test]
fn hulls() {
    let two = Solid::compose(&[Solid::cube([1.0; 3], false), Solid::cube([1.0; 3], false).translate([3.0, 0.0, 0.0])]);
    assert!(rel(two.hull().volume(), 4.0) < 1e-9);
    let tet = Solid::hull_points(&[[0.0, 0.0, 0.0], [6.0, 0.0, 0.0], [0.0, 6.0, 0.0], [0.0, 0.0, 6.0]]);
    assert!(rel(tet.volume(), 36.0) < 1e-9);
}

#[test]
fn slice_extrude_revolve() {
    let c = Solid::cube([2.0, 3.0, 4.0], false);
    let shape = c.slice(1.0);
    assert_eq!(shape.contours.len(), 1);
    // 8, not 4: each side face is two triangles, and the cut crosses their shared diagonal.
    // 1.x manifold3d gives the same 8 points.
    assert_eq!(shape.contours[0].len(), 8);
    assert!(rel(shape.extrude(5.0).volume(), 30.0) < 1e-9);
    assert!(c.slice(99.0).contours.is_empty());
    assert!(c.slice(99.0).extrude(5.0).is_empty());

    // a 1 x 2 rectangle 1 mm from the axis, turned all the way round: pi * (2^2 - 1^2) * 2
    let ring = stl_kernel::Shape2d { contours: vec![vec![[1.0, 0.0], [2.0, 0.0], [2.0, 2.0], [1.0, 2.0]]] };
    let v = ring.revolve(128, 360.0).volume();
    assert!(rel(v, std::f64::consts::PI * 6.0) < 2e-3, "{v}");
}

#[test]
fn clone_is_cheap_and_shared() {
    let a = Solid::sphere(10.0, 512);
    let vol = a.volume();
    assert!(a.num_tri() > 100_000, "fully computed before timing");
    let t = std::time::Instant::now();
    let b = a.clone();
    let took = t.elapsed();
    assert!(took < std::time::Duration::from_millis(1), "clone took {took:?}");
    assert!(a.shares_with(&b));
    assert_eq!(a.volume(), vol);
    assert_eq!(b.volume(), vol);
    assert!(!a.shares_with(&a.translate([1.0, 0.0, 0.0])));
}
