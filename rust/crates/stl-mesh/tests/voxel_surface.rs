//! Distance field, marching cubes and field to solid against 1.x
//! (`rust/tests/parity/voxel_golden.json`, written by `dev/parity/make_voxel_golden.py`).

use std::path::PathBuf;

use serde_json::Value;
use stl_mesh::voxel::{Grid3, distance_transform_edt, field_to_solid, marching_cubes};
use stl_parity::{Exactness, MeshStats, compare_mesh, mesh_stats};

const SAMPLE_STRIDE: usize = 97;

fn voxel_golden() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/voxel_golden.json");
    let text = std::fs::read_to_string(&path).expect("voxel_golden.json");
    serde_json::from_str(&text).expect("parse voxel_golden.json")
}

/// FIPS 180-4 SHA-256 for checking size-limited boolean grids in `voxel_golden.json`.
fn sha256_hex(message: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut data = message.to_vec();
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend(((message.len() as u64) * 8).to_be_bytes());
    for chunk in data.chunks(64) {
        let mut w = [0u32; 64];
        for (i, word) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for (hi, vi) in h.iter_mut().zip(v) {
            *hi = hi.wrapping_add(vi);
        }
    }
    h.iter().flat_map(|w| w.to_be_bytes()).map(|b| format!("{b:02x}")).collect()
}

fn parse_shape(v: &Value) -> [usize; 3] {
    let s = v.as_array().expect("shape array");
    [0, 1, 2].map(|a| s[a].as_u64().unwrap() as usize)
}

/// A golden `bool` grid. Size-limited grids are rebuilt from the case's construction in
/// `make_voxel_golden.py` and checked against the stored sha256 and count.
fn bool_grid(case: &str, g: &Value) -> Grid3<bool> {
    let n = parse_shape(&g["shape"]);
    if let Some(arr) = g.get("data").and_then(|d| d.as_array()) {
        return Grid3 { n, data: arr.iter().map(|b| b.as_bool().unwrap()).collect() };
    }
    let mut grid = Grid3 { n, data: vec![false; n[0] * n[1] * n[2]] };
    match case {
        "edt_box24" => {
            for i in 0..n[0] {
                for j in 0..n[1] {
                    for k in 0..n[2] {
                        let outer = [i, j, k].iter().all(|&c| (4..20).contains(&c));
                        let inner = [i, j, k].iter().all(|&c| (8..16).contains(&c));
                        let at = grid.idx(i, j, k);
                        grid.data[at] = outer && !inner;
                    }
                }
            }
        }
        other => panic!("{other}: no construction for a size-limited input grid"),
    }
    let bytes: Vec<u8> = grid.data.iter().map(|&b| u8::from(b)).collect();
    assert_eq!(sha256_hex(&bytes), g["sha256"].as_str().unwrap(), "{case} input sha256");
    assert_eq!(grid.data.iter().filter(|&&b| b).count() as u64, g["count"].as_u64().unwrap());
    grid
}

fn f32_field(g: &Value) -> Grid3<f32> {
    let n = parse_shape(&g["shape"]);
    let data: Vec<f32> = g["data"]
        .as_array()
        .expect("inline f32 field")
        .iter()
        .map(|v| v.as_f64().unwrap() as f32)
        .collect();
    assert_eq!(data.len(), n[0] * n[1] * n[2]);
    Grid3 { n, data }
}

fn cases<'a>(golden: &'a Value, op: &str) -> Vec<(&'a String, &'a Value)> {
    golden.as_object().expect("cases object").iter().filter(|(_, r)| r["op"].as_str() == Some(op)).collect()
}

#[test]
fn edt_matches_golden() {
    let golden = voxel_golden();
    let edt_cases = cases(&golden, "edt");
    assert!(edt_cases.len() >= 2, "expected at least 2 edt cases, got {}", edt_cases.len());
    for (case, rec) in edt_cases {
        let occ = bool_grid(case, &rec["input"]["grid"]);
        let dist = distance_transform_edt(&occ).unwrap();
        let want = &rec["output"]["field"];
        assert_eq!(dist.n, parse_shape(&want["shape"]), "{case} shape");
        let mut max_diff = 0.0f64;
        if let Some(arr) = want.get("data").and_then(|d| d.as_array()) {
            assert_eq!(dist.data.len(), arr.len(), "{case} len");
            for (idx, (&got, wv)) in dist.data.iter().zip(arr).enumerate() {
                let w = wv.as_f64().unwrap();
                let diff = (got - w).abs();
                max_diff = max_diff.max(diff);
                assert!(diff <= 1e-6, "{case} cell {idx}: got {got} want {w}");
            }
        } else {
            let sum: f64 = dist.data.iter().sum();
            let min = dist.data.iter().copied().fold(f64::INFINITY, f64::min);
            let max = dist.data.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            for (name, got, w) in [("sum", sum, &want["sum"]), ("min", min, &want["min"]), ("max", max, &want["max"])] {
                let w = w.as_f64().unwrap();
                let diff = (got - w).abs();
                if name != "sum" {
                    max_diff = max_diff.max(diff);
                }
                assert!(diff <= 1e-6 * if name == "sum" { dist.data.len() as f64 } else { 1.0 }, "{case} {name}: got {got} want {w}");
            }
            let sample = want["sample"].as_array().unwrap();
            assert_eq!(sample.len(), dist.data.len().div_ceil(SAMPLE_STRIDE), "{case} sample count");
            for (s, wv) in sample.iter().enumerate() {
                let got = dist.data[s * SAMPLE_STRIDE];
                let w = wv.as_f64().unwrap();
                let diff = (got - w).abs();
                max_diff = max_diff.max(diff);
                assert!(diff <= 1e-6, "{case} sample {s}: got {got} want {w}");
            }
        }
        println!("{case}: max diff = {max_diff:.3e}");
    }
}

#[test]
fn marching_cubes_matches_golden() {
    let golden = voxel_golden();
    let mc_cases = cases(&golden, "marching_cubes");
    assert!(!mc_cases.is_empty(), "expected a marching_cubes case");
    for (case, rec) in mc_cases {
        let field = f32_field(&rec["input"]["field"]);
        let level = rec["input"]["level"].as_f64().unwrap() as f32;
        let spacing = rec["input"]["spacing"].as_f64().unwrap();
        let (verts, faces) = marching_cubes(&field, level, spacing);

        let want_v: Vec<[f64; 3]> = rec["output"]["verts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| [0, 1, 2].map(|a| p[a].as_f64().unwrap()))
            .collect();
        let want_f: Vec<[u32; 3]> = rec["output"]["faces"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| [0, 1, 2].map(|a| t[a].as_u64().unwrap() as u32))
            .collect();
        assert_eq!(faces.len(), want_f.len(), "{case} triangle count");
        assert_eq!(verts.len(), want_v.len(), "{case} vertex count");

        let sorted = |v: &[[f64; 3]]| {
            let mut s = v.to_vec();
            s.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])).then(a[2].total_cmp(&b[2])));
            s
        };
        let mut max_sorted = 0.0f64;
        for (g, w) in sorted(&verts).iter().zip(sorted(&want_v).iter()) {
            for a in 0..3 {
                max_sorted = max_sorted.max((g[a] - w[a]).abs());
            }
        }
        assert!(max_sorted <= 1e-5, "{case} sorted vertices differ by {max_sorted}");

        // skimage's own order: the same vertices in the same order, and the same faces
        let mut max_ordered = 0.0f64;
        for (g, w) in verts.iter().zip(&want_v) {
            for a in 0..3 {
                max_ordered = max_ordered.max((g[a] - w[a]).abs());
            }
        }
        assert!(max_ordered <= 1e-5, "{case} vertex order differs (max {max_ordered})");
        assert_eq!(faces, want_f, "{case} faces");
        println!("{case}: {} verts, {} faces, max vertex diff = {max_ordered:.3e}", verts.len(), faces.len());
    }
}

#[test]
fn field_to_solid_matches_golden() {
    let golden = voxel_golden();
    let f_cases = cases(&golden, "field_to_manifold");
    assert!(!f_cases.is_empty(), "expected a field_to_manifold case");
    for (case, rec) in f_cases {
        let input = &rec["input"];
        let field = f32_field(&input["field"]);
        let o = input["origin"].as_array().unwrap();
        let origin = [0, 1, 2].map(|a| o[a].as_f64().unwrap());
        let pitch = input["pitch"].as_f64().unwrap();
        let level = input["level"].as_f64().unwrap() as f32;
        let solid = field_to_solid(&field, origin, pitch, level).expect("field_to_solid");
        let m = solid.to_mesh();
        let got = mesh_stats(&m.positions, &m.triangles);
        let want: MeshStats = serde_json::from_value(rec["output"]["stats"].clone()).expect("MeshStats");
        let diffs = compare_mesh(&want, &got, Exactness::Exact);
        assert!(diffs.is_empty(), "{case}: {}", diffs.join("; "));
        println!(
            "{case}: volume {} (golden {}), area {} (golden {}), {} triangles",
            got.volume, want.volume, got.area, want.area, got.triangles
        );
    }
}

/// A negated field does not come out inside out: the padding is below the level, so the part
/// above the level is always enclosed and its surface faces outwards (here a block with a
/// ball-shaped cavity).
#[test]
fn negated_field_gives_a_block_with_a_cavity() {
    let golden = voxel_golden();
    let mut field = f32_field(&golden["field_to_manifold_sphere"]["input"]["field"]);
    for x in &mut field.data {
        *x = -*x;
    }
    let s = field_to_solid(&field, [0.0; 3], 1.0, 0.0).expect("field_to_solid");
    let want: MeshStats =
        serde_json::from_value(golden["field_to_manifold_sphere"]["output"]["stats"].clone()).unwrap();
    assert!(s.signed_volume() > 0.0, "signed volume {}", s.signed_volume());
    assert!((s.signed_volume() - s.volume()).abs() < 1e-9);
    assert!(s.volume() > 8.0f64.powi(3) - want.volume, "volume {}", s.volume());
}

#[test]
fn field_to_solid_without_a_surface_is_an_error() {
    let field = Grid3 { n: [3, 3, 3], data: vec![-1.0f32; 27] };
    assert!(field_to_solid(&field, [0.0; 3], 1.0, 0.0).is_err());
}
