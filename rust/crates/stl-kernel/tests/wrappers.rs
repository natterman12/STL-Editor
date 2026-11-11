use std::collections::HashMap;
use std::path::PathBuf;

use stl_kernel::{Shape2d, Solid};
use stl_parity::{Exactness, MeshStats, compare_mesh, mesh_stats};

fn rel(got: f64, want: f64) -> f64 {
    ((got - want) / want).abs()
}

fn golden_stats() -> HashMap<String, Vec<MeshStats>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/kernel_golden.json");
    serde_json::from_slice(&std::fs::read(path).expect("kernel_golden.json")).expect("valid json")
}

fn assert_matches_golden(
    golden: &HashMap<String, Vec<MeshStats>>,
    name: &str,
    s: &Solid,
    ex: Exactness,
) {
    let want = &golden.get(name).unwrap_or_else(|| panic!("missing golden {name}"))[0];
    let m = s.to_mesh();
    let got = mesh_stats(&m.positions, &m.triangles);
    let diffs = compare_mesh(want, &got, ex);
    assert!(diffs.is_empty(), "{name}: {}", diffs.join("; "));
}

#[test]
fn wrappers_match_manifold3d() {
    let golden = golden_stats();

    // 1. Sphere r = 1, segments = 8: exact triangle count and 1.x volume/area.
    let sp8 = Solid::sphere(1.0, 8);
    assert_eq!(sp8.num_tri(), 32);
    assert_eq!(sp8.num_vert(), 18);
    assert!(rel(sp8.volume(), 2.9428090415820636) <= 1e-9, "vol {}", sp8.volume());
    assert!(rel(sp8.surface_area(), 10.41775152135782) <= 1e-9, "area {}", sp8.surface_area());
    assert_matches_golden(&golden, "sphere_r1_s8", &sp8, Exactness::Exact);

    // 2. Minkowski sum and difference of a 4 mm centred cube with that sphere.
    let cube = Solid::cube([4.0, 4.0, 4.0], true);
    let sum = cube.minkowski_sum(&sp8);
    assert_eq!(sum.num_tri(), 92);
    assert!(rel(sum.volume(), 196.88393453853635) <= 1e-9, "sum vol {}", sum.volume());
    assert!(rel(sum.surface_area(), 179.89297053545505) <= 1e-9, "sum area {}", sum.surface_area());
    let sb = sum.bounding_box();
    assert_eq!((sb.min, sb.max), ([-3.0; 3], [3.0; 3]));
    assert_matches_golden(&golden, "minkowski_sum_cube_sphere", &sum, Exactness::Exact);

    let diff = cube.minkowski_difference(&sp8);
    assert_eq!(diff.num_tri(), 12);
    assert!(rel(diff.volume(), 8.0) <= 1e-9, "diff vol {}", diff.volume());
    assert!(rel(diff.surface_area(), 24.0) <= 1e-9, "diff area {}", diff.surface_area());
    let db = diff.bounding_box();
    assert_eq!((db.min, db.max), ([-1.0; 3], [1.0; 3]));
    assert!(cube.minkowski_difference(&Solid::sphere(3.0, 8)).is_empty());
    assert_matches_golden(&golden, "minkowski_difference_cube_sphere", &diff, Exactness::Exact);

    // 3. Simplify of a sphere at tolerance 0.01.
    let sp96 = Solid::sphere(1.0, 96);
    assert_eq!(sp96.num_tri(), 4608);
    let simp = sp96.simplify(0.01);
    assert_eq!(simp.num_tri(), 2126);
    assert!(rel(simp.volume(), 4.152733474491687) <= 1e-9, "simp vol {}", simp.volume());
    assert!(rel(simp.surface_area(), 12.524381517730026) <= 1e-9, "simp area {}", simp.surface_area());
    assert!(
        (simp.bounding_box().max[2] - 0.9978589232386036).abs() <= 1e-12,
        "simp bbox {:?}",
        simp.bounding_box()
    );
    assert_matches_golden(&golden, "simplify_sphere", &simp, Exactness::Exact);

    // 4. Miter offset of an L-shape by -0.4 with miter limit 2.0.
    let l_contours = vec![vec![
        [0.0, 0.0],
        [10.0, 0.0],
        [10.0, 4.0],
        [4.0, 4.0],
        [4.0, 10.0],
        [0.0, 10.0],
    ]];
    let l_shape = Shape2d::from_even_odd(&l_contours);
    let l_miter = l_shape.offset_miter(-0.4, 2.0);
    assert!((l_miter.area() - 48.64).abs() <= 1e-6, "miter area {}", l_miter.area());
    let polys = l_miter.to_polygons();
    assert_eq!(polys.len(), 1);
    assert_eq!(polys[0].len(), 6);
    let (lo, hi) = l_miter.bounds().expect("non-empty bounds");
    assert!((lo[0] - 0.4).abs() <= 1e-7 && (lo[1] - 0.4).abs() <= 1e-7);
    assert!((hi[0] - 9.6).abs() <= 1e-7 && (hi[1] - 9.6).abs() <= 1e-7);
    // Round offset rounds the reflex corner (4, 4) instead of cutting to (3.6, 3.6), keeping more area.
    assert!(l_shape.offset_round(-0.4).area() > l_miter.area() + 0.03);
    assert_matches_golden(&golden, "offset_miter_l_shape", &l_miter.extrude(1.0), Exactness::Exact);

    // Acute reflex notch: miter_limit 2.0 squares off the spike, leaving more area than 12.0.
    let notch = Shape2d::from_even_odd(&[vec![
        [0.0, 0.0],
        [10.0, 0.0],
        [10.0, 10.0],
        [6.0, 10.0],
        [5.0, 1.0],
        [4.0, 10.0],
        [0.0, 10.0],
    ]]);
    let m2 = notch.offset_miter(-0.4, 2.0);
    let m12 = notch.offset_miter(-0.4, 12.0);
    assert!(rel(m2.area(), 69.19569193) <= 1e-6, "m2 area {}", m2.area());
    assert!(rel(m12.area(), 69.0569296) <= 1e-6, "m12 area {}", m12.area());

    // 5. Shape2d::intersection (`^` in 1.x, used by Shell) and to_polygons on slices.
    let sq = Shape2d::from_even_odd(&[vec![[2.0, 2.0], [8.0, 2.0], [8.0, 8.0], [2.0, 8.0]]]);
    let inter = l_shape.intersection(&sq);
    assert!(rel(inter.area(), 20.0) <= 1e-9, "inter area {}", inter.area());
    assert_eq!(inter.bounds(), Some(([2.0, 2.0], [8.0, 8.0])));
    assert_eq!(inter.to_polygons().len(), 1);
    assert_eq!(inter.to_polygons()[0].len(), 6);
    let far = Shape2d::from_even_odd(&[vec![[20.0, 20.0], [25.0, 20.0], [25.0, 25.0], [20.0, 25.0]]]);
    assert!(l_shape.intersection(&far).is_empty());

    let sliced = Solid::cube([2.0, 3.0, 4.0], false).slice(1.0);
    assert_eq!(sliced.to_polygons(), sliced.contours);
}
