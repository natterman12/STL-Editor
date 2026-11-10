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

#[test]
fn shape2d_offset_decompose_bounds() {
    use stl_kernel::Shape2d;
    let sq = |x0: f64, x1: f64| vec![[x0, 0.0], [x1, 0.0], [x1, 10.0], [x0, 10.0]];
    let s = Shape2d { contours: vec![sq(0.0, 10.0), sq(30.0, 40.0)] };
    assert!((s.area() - 200.0).abs() < 1e-9);
    assert_eq!(s.bounds(), Some(([0.0, 0.0], [40.0, 10.0])));
    let parts = s.decompose();
    assert_eq!(parts.len(), 2);
    assert!((parts[0].area() - 100.0).abs() < 1e-9);
    // shrinking by 2 leaves 6 x 6 squares; shrinking by 6 leaves nothing
    let inner = Shape2d { contours: vec![sq(0.0, 10.0)] }.offset_round(-2.0);
    assert!((inner.area() - 36.0).abs() < 1e-6);
    assert!(Shape2d { contours: vec![sq(0.0, 10.0)] }.offset_round(-6.0).is_empty());
    assert_eq!(Shape2d::default().bounds(), None);
}

/// Five cubes in one mesh. The expected order was printed by 1.x:
/// `[p.volume() for p in manifold3d.Manifold(mesh).decompose()]` (manifold3d 3.5.4).
#[test]
fn decompose_in_kernel_order_matches_1x() {
    let cube = cube_mesh();
    let specs: [(f32, [f32; 3]); 5] = [
        (0.5, [30.0, 0.0, 0.0]),
        (2.0, [50.0, -5.0, 3.0]),
        (1.0, [0.0, 0.0, 0.0]),
        (0.3, [-20.0, 40.0, -7.0]),
        (1.5, [12.0, 30.0, 25.0]),
    ];
    let mut m = MeshArrays::default();
    for (k, (scale, offset)) in specs.iter().enumerate() {
        for p in cube.positions.chunks_exact(3) {
            m.positions.extend((0..3).map(|i| p[i] * scale + offset[i]));
        }
        m.triangles.extend(cube.triangles.iter().map(|i| i + 8 * k as u32));
    }
    let s = Solid::from_mesh(&m).unwrap();
    let volumes: Vec<f64> = s.decompose_in_kernel_order().iter().map(|p| p.volume().round()).collect();
    assert_eq!(volumes, vec![1000.0, 27.0, 3375.0, 125.0, 8000.0]);
    // the sorted one is unchanged: largest first
    let sorted: Vec<f64> = s.decompose().iter().map(|p| p.volume().round()).collect();
    assert_eq!(sorted, vec![8000.0, 3375.0, 1000.0, 125.0, 27.0]);
}

#[test]
fn signed_volume_is_negative_for_an_inside_out_solid() {
    let mut m = cube_mesh();
    assert!((Solid::from_mesh(&m).unwrap().signed_volume() - 1000.0).abs() < 1e-9);
    for t in m.triangles.chunks_exact_mut(3) {
        t.swap(0, 2);
    }
    let inside_out = Solid::from_mesh(&m).unwrap();
    assert!((inside_out.signed_volume() + 1000.0).abs() < 1e-9);
    assert!((inside_out.volume() - 1000.0).abs() < 1e-9);
}

/// Two cubes that share one edge, as one mesh: four triangles use that edge. Turning it into a
/// solid has to separate the edge without moving a corner (Manifold 3.5.4 can move corners
/// here: `tests/EXCEPTIONS.md`), so every vertex of the solid is a vertex of the mesh.
#[test]
fn from_mesh_separates_a_shared_edge_without_moving_corners() {
    let cube = cube_mesh();
    let mut m = cube.clone();
    // the second cube stands 10 mm further along x and y; its vertices 0 and 4 are the first
    // cube's vertices 2 and 6
    for p in cube.positions.chunks_exact(3) {
        m.positions.extend_from_slice(&[p[0] + 10.0, p[1] + 10.0, p[2]]);
    }
    m.triangles.extend(cube.triangles.iter().map(|&i| match i {
        0 => 2,
        4 => 6,
        i => i + 8,
    }));
    let s = Solid::from_mesh(&m).expect("two cubes on one edge are closed");
    assert!((s.volume() - 2000.0).abs() < 1e-6, "volume {}", s.volume());
    assert!((s.surface_area() - 1200.0).abs() < 1e-6, "area {}", s.surface_area());
    assert_eq!(s.num_tri(), 24);
    let b = s.bounding_box();
    assert_eq!((b.min, b.max), ([0.0; 3], [20.0, 20.0, 10.0]));
    let out = s.to_mesh();
    for p in out.positions.chunks_exact(3) {
        assert!(m.positions.chunks_exact(3).any(|q| q == p), "{p:?} is not a vertex of the mesh");
    }
    assert_eq!(s.decompose().len(), 2);
}

/// A plane cut gives the vertices that 1.x (Manifold 3.5.4) gives, to the last bit: 3.5.4
/// applies the moves of its cutting block as one matrix. A pin that starts on the cut face
/// only unites as in 1.x when they are the same.
#[test]
fn tilted_plane_cut_has_the_vertices_of_1x() {
    let len = (1.0f64 + 0.09 + 0.04).sqrt();
    let n = [1.0 / len, 0.3 / len, 0.2 / len];
    let offset = n[0] * 20.0 + n[1] * 30.0 + n[2] * 15.0;
    let (above, below) = Solid::cube([40.0, 60.0, 30.0], false).split_by_plane(n, offset);
    // 1.x: Manifold.cube((40, 60, 30)).split_by_plane(n, offset), surface_area() of each half
    assert_eq!(above.surface_area(), 7313.426246292236);
    assert_eq!(below.surface_area(), 7313.426246292237);
    assert_eq!((above.num_tri(), below.num_tri()), (12, 12));
    assert!((above.volume() + below.volume() - 72000.0).abs() < 1e-6);
    // the trimmed half is the same solid
    let trimmed = Solid::cube([40.0, 60.0, 30.0], false).trim_by_plane(n, offset);
    assert_eq!(trimmed.to_mesh().positions, above.to_mesh().positions);
}

/// A solid with its matrix still to apply is the same solid; only a union treats it as 1.x
/// would. Three posts that do not touch: the union is the three together.
#[test]
fn pending_transform_is_the_same_solid() {
    let post = Solid::cylinder(10.0, 2.0, 2.0, 32, false);
    // a quarter turn about X (the post then lies along -Y), moved to x = 10 * k
    let m = |k: f64| [[1.0, 0.0, 0.0, 10.0 * k], [0.0, 0.0, 1.0, 0.0], [0.0, -1.0, 0.0, 0.0]];
    let applied = post.transform(m(1.0));
    let pending = post.transform_pending(m(1.0));
    assert_eq!(pending.to_mesh().positions, applied.to_mesh().positions);
    assert_eq!(pending.to_mesh().triangles, applied.to_mesh().triangles);

    let parts: Vec<Solid> = (0..3).map(|k| post.transform_pending(m(k as f64))).collect();
    let all = Solid::union_all(&parts);
    // a 32-sided post: area of the polygon times the height
    let one = 0.5 * 32.0 * 4.0 * (std::f64::consts::TAU / 32.0).sin() * 10.0;
    assert!((all.volume() - 3.0 * one).abs() < 1e-9, "{}", all.volume());
    assert_eq!(all.decompose().len(), 3);
    assert_eq!(all.num_tri(), 3 * post.num_tri());
    // a second matrix on a pending solid is applied together with the first
    let twice = post.transform_pending(m(0.0)).transform_pending([[1.0, 0.0, 0.0, 10.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]]);
    assert_eq!(twice.to_mesh().positions, applied.to_mesh().positions);
}
