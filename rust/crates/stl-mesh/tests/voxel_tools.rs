//! `hollow` and `voxel_rebuild` against the feature golden records of desktop 1.x
//! (`rust/tests/parity/features_golden/{hollow,repair}.json`), per spec 5.3.

use std::path::PathBuf;

use serde_json::Value;
use stl_kernel::{MeshArrays, Solid};
use stl_mesh::voxel::{auto_pitch, hollow, voxel_rebuild};
use stl_mesh::{local_repair, quick_repair};
use stl_parity::{Exactness, MeshStats, compare_mesh, mesh_stats};

fn tests_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests")
}

fn golden(file: &str, case: &str) -> MeshStats {
    let text = std::fs::read_to_string(tests_dir().join("parity/features_golden").join(file)).expect("golden");
    let v: Value = serde_json::from_str(&text).expect("json");
    serde_json::from_value(v[case]["bodies"][0]["stats"].clone()).expect("stats")
}

/// A binary STL as a triangle soup: f32 positions, three per triangle.
fn read_binary_stl(name: &str) -> Vec<f32> {
    let data = std::fs::read(tests_dir().join("models").join(name)).expect("model file");
    let n = u32::from_le_bytes(data[80..84].try_into().unwrap()) as usize;
    let mut out = Vec::with_capacity(n * 9);
    for t in 0..n {
        let rec = &data[84 + 50 * t + 12..84 + 50 * t + 48];
        out.extend(rec.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())));
    }
    out
}

fn soup(name: &str) -> (Vec<[f32; 3]>, Vec<[u32; 3]>) {
    let s = read_binary_stl(name);
    let v: Vec<[f32; 3]> = s.chunks_exact(3).map(|p| [p[0], p[1], p[2]]).collect();
    let f = (0..v.len() as u32 / 3).map(|t| [3 * t, 3 * t + 1, 3 * t + 2]).collect();
    (v, f)
}

fn stats(s: &Solid) -> MeshStats {
    let m = s.to_mesh();
    mesh_stats(&m.positions, &m.triangles)
}

fn pct(g: f64, v: f64) -> f64 {
    100.0 * (v - g) / g
}

fn check(case: &str, want: &MeshStats, got: &Solid, report: &mut Vec<String>) {
    let g = stats(got);
    let diffs = compare_mesh(want, &g, Exactness::Approx);
    println!(
        "{case}: volume {:+.4}% area {:+.4}% (tri {} vs {})",
        pct(want.volume, g.volume),
        pct(want.area, g.area),
        want.triangles,
        g.triangles
    );
    if !diffs.is_empty() {
        report.push(format!("{case}: {}", diffs.join("; ")));
    }
}

/// 1.x `_hollow_pitch`.
fn hollow_pitch(s: &Solid, t: f64, q: &str) -> f64 {
    let bb = s.bounding_box();
    let d: [f64; 3] = std::array::from_fn(|a| bb.max[a] - bb.min[a]);
    let diag = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let div = match q {
        "draft" => 80.0,
        "fine" => 260.0,
        _ => 150.0,
    };
    (t / 2.5).min(diag / div).max(diag / (div * 2.5))
}

fn box_30x30x20() -> Solid {
    Solid::cube([30.0, 30.0, 20.0], false).translate([-15.0, -15.0, 0.0])
}

fn err(r: Result<Solid, stl_mesh::MeshError>) -> String {
    match r {
        Ok(_) => "no error".into(),
        Err(e) => e.to_string(),
    }
}

#[test]
fn hollow_and_rebuild_match_1x() {
    let mut report = Vec::new();

    for q in ["draft", "normal", "fine"] {
        let b = box_30x30x20();
        let out = hollow(&b, 2.0, Some(hollow_pitch(&b, 2.0, q)), false).expect("hollow");
        check(&format!("hollow_{q}"), &golden("hollow.json", &format!("hollow_{q}")), &out, &mut report);
    }

    // a hollow with a thick wall: the error text
    let cube = Solid::cube([10.0, 10.0, 10.0], false);
    let e = err(hollow(&cube, 6.0, Some(hollow_pitch(&cube, 6.0, "draft")), false));
    assert_eq!(e, "Wall thickness is too large for this body; nothing would be left hollow.");
    assert_eq!(
        err(hollow(&cube, 0.0, None, false)),
        "Wall thickness must be positive."
    );

    // two shells: the import repairs the model first
    let (v, f) = soup("two_blobs.stl");
    let rv: Vec<f64> = v.iter().flat_map(|p| p.map(|x| x as f64)).collect();
    let rf: Vec<u32> = f.iter().flatten().copied().collect();
    let two = local_repair(&rv, &rf).unwrap_or_else(|| {
        let (qv, qf) = quick_repair(&rv, &rf);
        let m = MeshArrays { positions: qv.iter().map(|&x| x as f32).collect(), triangles: qf };
        Solid::from_mesh(&m).expect("two_blobs solid")
    });
    let out = hollow(&two, 1.5, Some(hollow_pitch(&two, 1.5, "draft")), false).expect("hollow two shells");
    check("hollow_two_shells", &golden("hollow.json", "hollow_two_shells"), &out, &mut report);

    // voxel_rebuild on the broken models
    let cases: [(&str, &str, Option<f64>, f64); 5] = [
        ("repair_rebuild_voxel_2", "open_box.stl", Some(2.0), 0.0),
        ("repair_expr_voxel", "open_box.stl", Some(1.0), 0.0),
        ("repair_rebuild_draft", "open_box.stl", None, 120.0),
        ("repair_rebuild_normal", "open_box.stl", None, 220.0),
        ("repair_rebuild_fine", "holed_block.stl", None, 360.0),
    ];
    for (case, model, voxel, target) in cases {
        let (v, f) = soup(model);
        let pitch = voxel.unwrap_or_else(|| auto_pitch(&v, target));
        let v64: Vec<[f64; 3]> = v.iter().map(|p| p.map(|x| x as f64)).collect();
        let out = voxel_rebuild(&v64, &f, pitch, 0.6).expect("rebuild");
        check(case, &golden("repair.json", case), &out, &mut report);
    }

    // too fine a grid
    let (v, f) = soup("open_box.stl");
    let v64: Vec<[f64; 3]> = v.iter().map(|p| p.map(|x| x as f64)).collect();
    assert_eq!(
        err(voxel_rebuild(&v64, &f, 0.01, 0.6)),
        "Resolution too fine for this model size. Use a coarser setting."
    );
    // not a tiny mesh: nothing to rebuild
    assert!(voxel_rebuild(&[], &[], 1.0, 0.6).is_err());

    assert!(report.is_empty(), "{} mismatches:\n{}", report.len(), report.join("\n"));
}

/// Shell's path (`cavity_only`) and the default pitch (`None`), against numbers computed with
/// 1.x `G.hollow` on the same 30 x 30 x 20 box (scratch script, 2026-11-11).
#[test]
fn hollow_cavity_only_and_default_pitch_match_1x() {
    let b = box_30x30x20();
    let pitch = hollow_pitch(&b, 2.0, "normal");

    let cavity = hollow(&b, 2.0, Some(pitch), true).expect("cavity");
    let full = hollow(&b, 2.0, Some(pitch), false).expect("hollow");
    assert!((cavity.volume() + full.volume() - b.volume()).abs() < 1e-6 * b.volume());
    assert_eq!(cavity.num_tri(), 100);
    assert!(close(10811.016654935866, cavity.volume()), "cavity volume {}", cavity.volume());
    assert!(close(2986.3821097370587, cavity.surface_area()), "cavity area {}", cavity.surface_area());

    let d = hollow(&b, 2.0, None, false).expect("default pitch");
    assert_eq!(d.num_tri(), 112);
    assert!(close(7203.581114988085, d.volume()), "default volume {}", d.volume());
    assert!(close(7165.913931602535, d.surface_area()), "default area {}", d.surface_area());
}

fn close(want: f64, got: f64) -> bool {
    ((got - want) / want).abs() < 1e-3
}

/// The streamed surface samples are the same set as `surface_points`, and the grid search
/// gives exactly the distances the KD-tree gives (including queries far outside the points).
#[test]
fn point_grid_matches_tree_and_sampler_matches_surface_points() {
    use stl_mesh::nearest::{PointGrid, PointTree};
    use stl_mesh::voxel::{SurfaceSampler, surface_points};

    let verts: Vec<[f64; 3]> = vec![
        [0.0, 0.0, 0.0],
        [10.0, 0.0, 0.0],
        [0.0, 7.0, 1.0],
        [3.0, 3.0, 9.0],
        [0.2, 0.1, 0.3],
    ];
    let faces: Vec<[u32; 3]> = vec![[0, 1, 2], [0, 1, 3], [1, 2, 3], [0, 2, 3], [0, 4, 4], [0, 1, 99]];
    let spacing = 0.7;
    let all = surface_points(&verts, &faces, spacing);
    let sampler = SurfaceSampler::new(&verts, &faces, spacing);
    let mut streamed: Vec<[f64; 3]> = Vec::new();
    sampler.each_vertex(|p| streamed.push(p));
    for fi in 0..sampler.face_count() {
        sampler.each_face_point(fi, |p| streamed.push(p));
    }
    let key = |p: &[f64; 3]| (p[0].to_bits(), p[1].to_bits(), p[2].to_bits());
    let mut a = all.clone();
    let mut b = streamed.clone();
    a.sort_by_key(key);
    b.sort_by_key(key);
    assert_eq!(a, b);

    let tree = PointTree::new(&all);
    let grid = PointGrid::build([0.0; 3], [10.0, 7.0, 9.0], 0.9, all.len(), |f| {
        sampler.each_vertex(|p| f(p));
        for fi in 0..sampler.face_count() {
            sampler.each_face_point(fi, |p| f(p));
        }
    })
    .expect("grid");
    let mut seed = 12345u64;
    let mut rnd = move || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 11) as f64 / (1u64 << 53) as f64
    };
    for _ in 0..500 {
        let q = [rnd() * 30.0 - 10.0, rnd() * 24.0 - 8.0, rnd() * 26.0 - 8.0];
        assert_eq!(tree.nearest_dist(q), grid.nearest_dist(q), "query {q:?}");
    }
}

/// A sample just outside the grid bounds lands in an edge cell; the search must still find it.
#[test]
fn point_grid_finds_samples_clamped_into_edge_cells() {
    use stl_mesh::nearest::{PointGrid, PointTree};
    let pts = vec![[0.0, 1.0 - 1e-12, 0.0], [-1e-15, 1.0, 0.0]];
    let tree = PointTree::new(&pts);
    let grid = PointGrid::build([0.0; 3], [4.0; 3], 1.0, 2, |f| {
        for &p in &pts {
            f(p);
        }
    })
    .expect("grid");
    for q in [[-10.0, 1.0 - 1e-12, 0.0], [10.0, 1.0, 0.0], [0.5, -3.0, 5.0], [-1e-15, 1.0, -7.0]] {
        assert_eq!(tree.nearest_dist(q), grid.nearest_dist(q), "query {q:?}");
    }
}

/// Samples a few ulps off their cell's nominal slab must not be pruned (two found in review).
#[test]
fn point_grid_is_exact_for_samples_at_cell_edges() {
    use stl_mesh::nearest::{PointGrid, PointTree};
    let cases: [(Vec<[f64; 3]>, [f64; 3], [f64; 3], [f64; 3]); 2] = [
        (
            vec![[3689.0999999999995, 0.0, 0.0], [3685.4999999999995, 3.6, 0.0]],
            [0.0; 3],
            [3690.0, 1.0, 0.0],
            [3685.4999999999995, 0.0, 0.0],
        ),
        (
            vec![[8192.699999999999, 0.0, 0.0], [-8192.7, 0.0, 0.0]],
            [0.0; 3],
            [8193.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
        ),
    ];
    for (pts, lo, hi, q) in cases {
        let tree = PointTree::new(&pts);
        let grid = PointGrid::build(lo, hi, 0.9, pts.len(), |f| {
            for &p in &pts {
                f(p);
            }
        })
        .expect("grid");
        assert_eq!(tree.nearest_dist(q), grid.nearest_dist(q), "points {pts:?}");
    }
}

fn lcg(seed: u64) -> impl FnMut() -> f64 {
    let mut s = seed;
    move || {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (s >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// The triangle-based search (no sample list) gives bit-for-bit the distance a KD-tree over all
/// the samples gives, on several separate pieces far apart (a query between two pieces has its
/// nearest sample in the neighbouring one), skinny and collapsed triangles, one-sample triangles,
/// triangles clamped at 400 subdivisions and faces with bad indices.
#[test]
fn sample_index_matches_tree_over_all_samples() {
    use stl_mesh::nearest::PointTree;
    use stl_mesh::voxel::{SampleIndex, SurfaceSampler, surface_points};

    let mut rnd = lcg(777);
    let mut verts: Vec<[f64; 3]> = Vec::new();
    let mut faces: Vec<[u32; 3]> = Vec::new();
    // four pieces on a row, 30 mm apart: each a few random triangles of mixed sizes
    for piece in 0..4 {
        let ox = piece as f64 * 30.0;
        let base = verts.len() as u32;
        for _ in 0..9 {
            verts.push([ox + rnd() * 6.0, rnd() * 6.0, rnd() * 6.0]);
        }
        for f in 0..8u32 {
            faces.push([base + f, base + (f + 3) % 9, base + (f + 5) % 9]);
        }
    }
    let n = verts.len() as u32;
    // a skinny sliver 40 mm long (hits the 400 clamp at fine spacing), a collapsed edge, a point
    // triangle, a tiny triangle (one sample), a face with a bad index
    verts.extend([[0.0, 0.0, 0.0], [40.0, 0.2, 0.1], [40.0, 0.2, 0.3], [5.0, 5.0, 5.0], [5.0, 5.0, 5.0], [9.0, 9.0, 9.0]]);
    faces.extend([[n, n + 1, n + 2], [n + 3, n + 4, n + 5], [n + 3, n + 3, n + 3], [n + 5, n + 5, n + 4], [0, 1, 999]]);
    for spacing in [0.4, 1.3, 0.07] {
        let all = surface_points(&verts, &faces, spacing);
        let tree = PointTree::new(&all);
        let sampler = SurfaceSampler::new(&verts, &faces, spacing);
        let index = SampleIndex::new(&sampler).expect("index");
        for _ in 0..400 {
            let q = [rnd() * 110.0 - 10.0, rnd() * 20.0 - 7.0, rnd() * 20.0 - 7.0];
            assert_eq!(tree.nearest_dist(q), index.nearest_dist(q), "spacing {spacing} query {q:?}");
        }
        // queries right on and next to samples
        for k in (0..all.len()).step_by(all.len() / 60 + 1) {
            let mut q = all[k];
            assert_eq!(tree.nearest_dist(q), index.nearest_dist(q));
            q[1] += 0.013;
            assert_eq!(tree.nearest_dist(q), index.nearest_dist(q));
        }
        // a query halfway between two pieces: the nearest sample is in the neighbouring piece
        for q in [[15.0, 3.0, 3.0], [45.0, 3.0, 3.0], [75.0, 3.0, 3.0]] {
            assert_eq!(tree.nearest_dist(q), index.nearest_dist(q), "between pieces {q:?}");
        }
    }
    // no faces at all: only the vertices
    let sampler = SurfaceSampler::new(&verts, &[], 0.5);
    let index = SampleIndex::new(&sampler).expect("index");
    let tree = PointTree::new(&verts);
    assert_eq!(tree.nearest_dist([1.0, 2.0, 3.0]), index.nearest_dist([1.0, 2.0, 3.0]));
}
