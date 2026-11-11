//! KD-tree vs brute force, and morphology / mesh occupancy against 1.x
//! (`rust/tests/parity/voxel_golden.json`).

use std::path::PathBuf;

use serde_json::Value;
use stl_mesh::nearest::PointTree;
use stl_mesh::voxel::{Grid3, binary_dilation, binary_erosion, binary_fill_holes, mesh_occupancy};

fn tests_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests")
}

fn voxel_golden() -> Value {
    let text = std::fs::read_to_string(tests_dir().join("parity/voxel_golden.json")).expect("voxel_golden.json");
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
            v = [
                t1.wrapping_add(t2),
                v[0],
                v[1],
                v[2],
                v[3].wrapping_add(t1),
                v[4],
                v[5],
                v[6],
            ];
        }
        for (hi, vi) in h.iter_mut().zip(v) {
            *hi = hi.wrapping_add(vi);
        }
    }
    let mut out = String::with_capacity(64);
    for word in h {
        for b in word.to_be_bytes() {
            out.push_str(&format!("{b:02x}"));
        }
    }
    out
}

fn read_stl_soup(name: &str) -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
    let data = std::fs::read(tests_dir().join("models").join(name)).expect("model file");
    let n = u32::from_le_bytes(data[80..84].try_into().unwrap()) as usize;
    assert_eq!(data.len(), 84 + 50 * n, "{name} is not a binary STL");
    let mut verts = Vec::with_capacity(n * 3);
    let mut faces = Vec::with_capacity(n);
    for t in 0..n {
        let rec = &data[84 + 50 * t + 12..84 + 50 * t + 48];
        let base = verts.len() as u32;
        for c in rec.chunks_exact(12) {
            let f = |o: usize| f32::from_le_bytes(c[o..o + 4].try_into().unwrap()) as f64;
            verts.push([f(0), f(4), f(8)]);
        }
        faces.push([base, base + 1, base + 2]);
    }
    (verts, faces)
}

fn shape(v: &Value) -> [usize; 3] {
    let s = v.as_array().expect("shape");
    [s[0].as_u64().unwrap() as usize, s[1].as_u64().unwrap() as usize, s[2].as_u64().unwrap() as usize]
}

fn grid_from(v: &Value) -> Grid3<bool> {
    let data = v["data"].as_array().expect("inline grid").iter().map(|b| b.as_bool().unwrap()).collect();
    Grid3 { n: shape(&v["shape"]), data }
}

fn check_grid_against(case: &str, got: &Grid3<bool>, want: &Value) {
    assert_eq!(got.n, shape(&want["shape"]), "{case} shape");
    if want.get("data").is_some() {
        assert_eq!(got.data, grid_from(want).data, "{case} data");
    } else {
        let count = got.data.iter().filter(|&&b| b).count();
        assert_eq!(count, want["count"].as_u64().unwrap() as usize, "{case} count");
        let bytes: Vec<u8> = got.data.iter().map(|&b| u8::from(b)).collect();
        assert_eq!(sha256_hex(&bytes), want["sha256"].as_str().unwrap(), "{case} sha256");
    }
}

#[test]
fn morphology_matches_golden() {
    let golden = voxel_golden();
    let mut checked = 0;
    for (case, rec) in golden.as_object().unwrap() {
        let op = rec["op"].as_str().unwrap();
        if !matches!(op, "dilate" | "erode" | "fill_holes") {
            continue;
        }
        checked += 1;
        let input = grid_from(&rec["input"]["grid"]);
        let its = rec["input"]["iterations"].as_u64().unwrap_or(1) as usize;
        let got = match op {
            "dilate" => binary_dilation(&input, its).unwrap(),
            "erode" => binary_erosion(&input, its).unwrap(),
            _ => binary_fill_holes(&input).unwrap(),
        };
        check_grid_against(case, &got, &rec["output"]["grid"]);
    }
    assert_eq!(checked, 3);
}

#[test]
fn mesh_occupancy_matches_golden() {
    let golden = voxel_golden();
    let mut checked = 0;
    for (case, rec) in golden.as_object().unwrap() {
        if rec["op"].as_str() != Some("mesh_occupancy") {
            continue;
        }
        checked += 1;
        let (v, f) = read_stl_soup(rec["input"]["model"].as_str().unwrap());
        let pitch = rec["input"]["pitch"].as_f64().unwrap();
        let (occ, origin) = mesh_occupancy(&v, &f, pitch).expect("mesh_occupancy");
        let want_origin = rec["output"]["origin"].as_array().unwrap();
        for a in 0..3 {
            assert_eq!(origin[a], want_origin[a].as_f64().unwrap(), "{case} origin[{a}]");
        }
        check_grid_against(case, &occ, &rec["output"]["grid"]);
    }
    assert_eq!(checked, 2);
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
    }
    fn point(&mut self, s: f64) -> [f64; 3] {
        [self.next() * s, self.next() * s, self.next() * s]
    }
}

#[test]
fn point_tree_matches_brute_force() {
    let mut rng = Lcg(0x5EED_1234);
    let mut pts: Vec<[f64; 3]> = (0..2000).map(|_| rng.point(10.0)).collect();
    pts.push(pts[0]); // duplicate point
    let tree = PointTree::new(&pts);
    let d = |a: &[f64; 3], b: &[f64; 3]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
    for _ in 0..10_000 {
        let q = rng.point(12.0);
        let r = rng.next() * 2.0;
        let want = pts.iter().map(|p| d(p, &q)).fold(f64::INFINITY, f64::min);
        assert!((tree.nearest_dist(q) - want).abs() <= 1e-12, "nearest {q:?}");
        let want_in: Vec<usize> = (0..pts.len()).filter(|&i| d(&pts[i], &q) <= r).collect();
        assert_eq!(tree.within(q, r), want_in, "within {q:?} r={r}");
    }
    assert_eq!(PointTree::new(&[]).nearest_dist([0.0; 3]), f64::INFINITY);
    assert!(tree.within(pts[5], 0.0).contains(&5));
}

/// 10 mm box with a 2 mm square hole in the lid. 1.x only closes it by growing the surface
/// (it = 1 at pitch 1.0, it = 2 at pitch 0.5). Expected values come from 1.x `mesh_occupancy`
/// (computed in a scratch script).
#[test]
fn mesh_occupancy_closes_lid_hole() {
    let mut verts: Vec<[f64; 3]> = Vec::new();
    for z in [0.0, 10.0] {
        for y in [0.0, 10.0] {
            for x in [0.0, 10.0] {
                verts.push([x, y, z]);
            }
        }
    }
    verts.extend([[4.0, 4.0, 10.0], [6.0, 4.0, 10.0], [4.0, 6.0, 10.0], [6.0, 6.0, 10.0]]);
    let faces: Vec<[u32; 3]> = vec![
        [4, 5, 9], [4, 9, 8], [5, 7, 11], [5, 11, 9], [7, 6, 10], [7, 10, 11], [6, 4, 8], [6, 8, 10],
        [0, 2, 1], [1, 2, 3], [0, 1, 5], [0, 5, 4], [1, 3, 7], [1, 7, 5], [3, 2, 6], [3, 6, 7],
        [2, 0, 4], [2, 4, 6],
    ];
    let cases = [
        (1.0, 35usize, 1330usize, "57321b9493bfe0f4765ce845b35db511a483b18aa48df96c7c3eb281c8381a39", -12.0),
        (0.5, 45, 9251, "48deca0131b39f5cdbfa497c08d41f7a0ccbb20533a752331d35c2edc768b606", -6.0),
    ];
    for (pitch, n, count, sha, o) in cases {
        let (occ, origin) = mesh_occupancy(&verts, &faces, pitch).unwrap();
        assert_eq!(occ.n, [n, n, n], "pitch {pitch} shape");
        assert_eq!(origin, [o, o, o], "pitch {pitch} origin");
        assert_eq!(occ.data.iter().filter(|&&b| b).count(), count, "pitch {pitch} count");
        let bytes: Vec<u8> = occ.data.iter().map(|&b| u8::from(b)).collect();
        assert_eq!(sha256_hex(&bytes), sha, "pitch {pitch} sha256");
    }
}

#[test]
fn morphology_zero_iterations_runs_to_fixpoint() {
    // 1x1x5 line in a 5x1x5 grid: dilation to a fixpoint fills everything; erosion empties it.
    let mut g = Grid3 { n: [5, 1, 5], data: vec![false; 25] };
    let c = g.idx(2, 0, 2);
    g.data[c] = true;
    assert!(binary_dilation(&g, 0).unwrap().data.iter().all(|&b| b));
    assert!(binary_erosion(&g, 0).unwrap().data.iter().all(|&b| !b));
    assert!(mesh_occupancy(&[], &[], 1.0).is_err());
}
