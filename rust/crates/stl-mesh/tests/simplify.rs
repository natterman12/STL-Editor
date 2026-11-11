//! The Fast Quadric port against `fast_simplification` 0.2.0: `rust/tests/parity/simplify_golden.json`
//! and the meshes in `rust/tests/parity/simplify/`, written by `dev/parity/make_simplify_golden.py`.

use std::path::PathBuf;

use serde_json::Value;
use stl_kernel::Solid;
use stl_mesh::simplify::{Simplified, fqms_simplify, simplify};
use stl_mesh::weld;

fn tests_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests")
}

/// The triangles of a binary STL, as f32 corners.
fn read_binary_stl(path: &PathBuf) -> Vec<[[f32; 3]; 3]> {
    let data = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let n = u32::from_le_bytes(data[80..84].try_into().unwrap()) as usize;
    assert_eq!(data.len(), 84 + 50 * n, "{} is not a binary STL", path.display());
    (0..n)
        .map(|t| {
            let rec = &data[84 + 50 * t + 12..84 + 50 * t + 48];
            let x: Vec<f32> = rec.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect();
            [[x[0], x[1], x[2]], [x[3], x[4], x[5]], [x[6], x[7], x[8]]]
        })
        .collect()
}

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Distance from `p` to the triangle `abc` (closest point by Voronoi region, Ericson 5.1.5).
fn point_triangle(p: V3, a: V3, b: V3, c: V3) -> f64 {
    let at = |s: f64, t: f64| {
        let q = [a[0] + s * (b[0] - a[0]) + t * (c[0] - a[0]), a[1] + s * (b[1] - a[1]) + t * (c[1] - a[1]), a[2] + s * (b[2] - a[2]) + t * (c[2] - a[2])];
        let d = sub(p, q);
        dot(d, d).sqrt()
    };
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return at(0.0, 0.0);
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return at(1.0, 0.0);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return at(d1 / (d1 - d3), 0.0);
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return at(0.0, 1.0);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return at(0.0, d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return at(1.0 - w, w);
    }
    let denom = va + vb + vc;
    if denom == 0.0 {
        // degenerate triangle: fall back to its edges
        let seg = |u: V3, v: V3| {
            let uv = sub(v, u);
            let l = dot(uv, uv);
            let t = if l > 0.0 { (dot(sub(p, u), uv) / l).clamp(0.0, 1.0) } else { 0.0 };
            let d = sub(p, [u[0] + t * uv[0], u[1] + t * uv[1], u[2] + t * uv[2]]);
            dot(d, d).sqrt()
        };
        return seg(a, b).min(seg(b, c)).min(seg(c, a));
    }
    at(vb / denom, vc / denom)
}

/// The largest distance from a corner of `from` to the surface of `to`.
fn one_way(from: &[[V3; 3]], to: &[[V3; 3]]) -> f64 {
    let mut worst = 0.0f64;
    for t in from {
        for &p in t {
            let d = to.iter().map(|u| point_triangle(p, u[0], u[1], u[2])).fold(f64::INFINITY, f64::min);
            worst = worst.max(d);
        }
    }
    worst
}

/// Symmetric vertex-to-surface Hausdorff distance. Two empty meshes are 0 apart; an empty mesh
/// and a non-empty one are infinitely far apart.
fn hausdorff(a: &[[V3; 3]], b: &[[V3; 3]]) -> f64 {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => 0.0,
        (true, false) | (false, true) => f64::INFINITY,
        _ => one_way(a, b).max(one_way(b, a)),
    }
}

#[test]
fn simplify_paths() {
    let ball = Solid::sphere(10.0, 48);
    let m = ball.to_mesh();
    let v: Vec<[f32; 3]> = m.positions.chunks_exact(3).map(|p| [p[0], p[1], p[2]]).collect();
    let f: Vec<[u32; 3]> = m.triangles.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();

    // a tolerance with no ratio goes straight to Manifold
    match simplify(&v, &f, Some(&ball), None, Some(0.01)) {
        Simplified::Solid(s) => assert_eq!(s.num_tri(), ball.simplify(0.01).num_tri()),
        Simplified::Raw(..) => panic!("tolerance path gave a raw mesh"),
    }
    // ratio 0 is clamped to 0.001, and a solid body always comes back solid
    for ratio in [None, Some(0.0), Some(0.1), Some(1.0)] {
        match simplify(&v, &f, Some(&ball), ratio, None) {
            Simplified::Solid(s) => assert!(s.num_tri() <= f.len(), "{ratio:?}"),
            Simplified::Raw(..) => panic!("{ratio:?}: solid body gave a raw mesh"),
        }
    }
    // a lone triangle simplifies to nothing, and without a solid that comes back raw
    let tri = [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    match simplify(&tri, &[[0, 1, 2]], None, Some(0.5), None) {
        Simplified::Raw(rv, rf) => assert!(rv.is_empty() && rf.is_empty()),
        Simplified::Solid(_) => panic!("a lone triangle became a solid"),
    }
}

#[test]
fn fqms_matches_golden() {
    let parity = tests_dir().join("parity");
    let golden: Value =
        serde_json::from_str(&std::fs::read_to_string(parity.join("simplify_golden.json")).unwrap()).unwrap();
    let cases = golden.as_object().unwrap();
    assert!(!cases.is_empty());
    let mut bad = Vec::new();
    for (name, rec) in cases {
        let soup = read_binary_stl(&tests_dir().join("models").join(rec["model"].as_str().unwrap()));
        let pos: Vec<f64> = soup.iter().flatten().flatten().map(|&x| x as f64).collect();
        let idx: Vec<u32> = (0..(soup.len() * 3) as u32).collect();
        let (wv, wf) = weld(&pos, &idx, 1e-5);
        let v32: Vec<[f32; 3]> = wv.chunks_exact(3).map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect();
        let f: Vec<[u32; 3]> = wf.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();
        let ratio = rec["ratio"].as_f64().unwrap();
        let (nv, nf) = fqms_simplify(&v32, &f, 1.0 - ratio);

        let ours: Vec<[V3; 3]> = nf.iter().map(|t| t.map(|i| nv[i as usize])).collect();
        let theirs: Vec<[V3; 3]> = read_binary_stl(&parity.join(rec["mesh"].as_str().unwrap()))
            .iter()
            .map(|t| t.map(|p| p.map(|x| x as f64)))
            .collect();
        let want = rec["tris"].as_u64().unwrap() as usize;
        let h = hausdorff(&ours, &theirs);
        println!("{name}: tris {} (golden {want}), hausdorff {h:.3e} mm", nf.len());
        if nf.len() != want || theirs.len() != want || !(h < 0.01) {
            bad.push(format!("{name}: tris {} against {want}, hausdorff {h}", nf.len()));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
