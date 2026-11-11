//! Voxel grid, surface-point sampling, and Gaussian smoothing against 1.x
//! (`rust/tests/parity/voxel_golden.json` and `holed_block.stl`).

use std::path::PathBuf;

use serde_json::Value;
use stl_kernel::{MeshArrays, Solid};
use stl_mesh::voxel::{
    Grid3, check_grid, gaussian_filter, grid_for, raster_evenodd, solid_occupancy, surface_points,
};
use stl_mesh::{MeshError, weld};

const CAP_MSG: &str = "Resolution too fine for this model size. Use a coarser setting.";
const SAMPLE_STRIDE: usize = 97;

fn tests_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests")
}

fn voxel_golden() -> Value {
    let path = tests_dir().join("parity/voxel_golden.json");
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
            let x = f32::from_le_bytes(c[0..4].try_into().unwrap()) as f64;
            let y = f32::from_le_bytes(c[4..8].try_into().unwrap()) as f64;
            let z = f32::from_le_bytes(c[8..12].try_into().unwrap()) as f64;
            verts.push([x, y, z]);
        }
        faces.push([base, base + 1, base + 2]);
    }
    (verts, faces)
}

fn load_model_solid(model: &str) -> Solid {
    match model {
        "primitive:cube10" => Solid::cube([10.0, 10.0, 10.0], false),
        "primitive:cube40" => Solid::cube([40.0, 40.0, 40.0], false),
        other => {
            let (raw_v, raw_f) = read_stl_soup(other);
            let flat_v: Vec<f64> = raw_v.iter().flatten().copied().collect();
            let flat_f: Vec<u32> = raw_f.iter().flatten().copied().collect();
            let (wv, wf) = weld(&flat_v, &flat_f, 1e-5);
            let m = MeshArrays {
                positions: wv.iter().map(|&x| x as f32).collect(),
                triangles: wf,
            };
            Solid::from_mesh(&m).expect("watertight solid")
        }
    }
}

fn parse_shape(v: &Value) -> [usize; 3] {
    let s = v.as_array().expect("shape array");
    [
        s[0].as_u64().unwrap() as usize,
        s[1].as_u64().unwrap() as usize,
        s[2].as_u64().unwrap() as usize,
    ]
}

#[test]
fn solid_occupancy_matches_golden() {
    let golden = voxel_golden();
    let cases = golden.as_object().expect("cases object");
    let mut checked = 0usize;
    for (case, rec) in cases {
        if rec["op"].as_str() != Some("solid_occupancy") {
            continue;
        }
        checked += 1;
        let model = rec["input"]["model"].as_str().unwrap();
        let pitch = rec["input"]["pitch"].as_f64().unwrap();
        let solid = load_model_solid(model);
        let (occ, origin) = solid_occupancy(&solid, pitch).expect("solid_occupancy");

        let want_origin = rec["output"]["origin"].as_array().unwrap();
        for a in 0..3 {
            let w = want_origin[a].as_f64().unwrap();
            assert!((origin[a] - w).abs() <= 1e-12, "{case} origin[{a}]: got {} want {w}", origin[a]);
        }

        let want_grid = &rec["output"]["grid"];
        assert_eq!(occ.n, parse_shape(&want_grid["shape"]), "{case} shape");
        if let Some(arr) = want_grid.get("data").and_then(|d| d.as_array()) {
            let want_data: Vec<bool> = arr.iter().map(|b| b.as_bool().unwrap()).collect();
            assert_eq!(occ.data, want_data, "{case} grid data");
        } else {
            let want_count = want_grid["count"].as_u64().unwrap() as usize;
            let got_count = occ.data.iter().filter(|&&b| b).count();
            assert_eq!(got_count, want_count, "{case} true count");
            let bytes: Vec<u8> = occ.data.iter().map(|&b| u8::from(b)).collect();
            let got_sha = sha256_hex(&bytes);
            assert_eq!(got_sha, want_grid["sha256"].as_str().unwrap(), "{case} sha256");
        }
    }
    assert!(checked >= 2, "expected at least 2 solid_occupancy cases, got {checked}");

    // Direct sanity check on grid_for, raster_evenodd, and non-cubic holed_block occupancy.
    let (o, n) = grid_for([0.0, 0.0, 0.0], [10.0, 10.0, 10.0], 2.0, 2);
    assert_eq!(o, [-4.0, -4.0, -4.0]);
    assert_eq!(n, [10, 10, 10]);
    let empty_plane = raster_evenodd(&[], 0.0, 0.0, 1.0, 4, 4);
    assert_eq!(empty_plane, vec![false; 16]);

    let holed = load_model_solid("holed_block.stl");
    let (hocc, horigin) = solid_occupancy(&holed, 1.5).expect("holed_block solid_occupancy");
    assert_eq!(hocc.n, [25, 19, 12]);
    assert_eq!(horigin, [-3.0, -3.0, -3.0]);
    assert_eq!(hocc.data.iter().filter(|&&b| b).count(), 1524);
    let hbytes: Vec<u8> = hocc.data.iter().map(|&b| u8::from(b)).collect();
    assert_eq!(
        sha256_hex(&hbytes),
        "20d15716a2f496898d71b9ce39e4fc1e20f549aa4b1c1a0cb115f53242f8d707"
    );
}

#[test]
fn gaussian_matches_golden() {
    let golden = voxel_golden();
    let cases = golden.as_object().expect("cases object");
    let mut checked = 0usize;
    for (case, rec) in cases {
        if rec["op"].as_str() != Some("gaussian") {
            continue;
        }
        checked += 1;
        let in_field = &rec["input"]["field"];
        let shape = parse_shape(&in_field["shape"]);
        let data: Vec<f32> = in_field["data"]
            .as_array()
            .expect("inline gaussian input")
            .iter()
            .map(|v| v.as_f64().unwrap() as f32)
            .collect();
        let sigma = rec["input"]["sigma"].as_f64().unwrap();
        let mut grid = Grid3 { n: shape, data };
        gaussian_filter(&mut grid, sigma);

        let want = &rec["output"]["field"];
        assert_eq!(grid.n, parse_shape(&want["shape"]), "{case} shape");
        if let Some(arr) = want.get("data").and_then(|d| d.as_array()) {
            assert_eq!(grid.data.len(), arr.len(), "{case} len");
            let mut max_diff = 0.0f64;
            for (idx, (&got, wv)) in grid.data.iter().zip(arr.iter()).enumerate() {
                let w = wv.as_f64().unwrap();
                let diff = (got as f64 - w).abs();
                max_diff = max_diff.max(diff);
                assert!(diff <= 1e-6, "{case} cell {idx}: got {got} want {w} (diff {diff})");
            }
            println!("{case}: max diff = {max_diff:.3e}");
        } else {
            let sum: f64 = grid.data.iter().map(|&x| x as f64).sum();
            let min = grid.data.iter().copied().fold(f32::INFINITY, f32::min) as f64;
            let max = grid.data.iter().copied().fold(f32::NEG_INFINITY, f32::max) as f64;
            assert!((sum - want["sum"].as_f64().unwrap()).abs() <= 1e-6 * grid.data.len() as f64);
            assert!((min - want["min"].as_f64().unwrap()).abs() <= 1e-6);
            assert!((max - want["max"].as_f64().unwrap()).abs() <= 1e-6);
            let sample = want["sample"].as_array().unwrap();
            for (s_idx, wv) in sample.iter().enumerate() {
                let got = grid.data[s_idx * SAMPLE_STRIDE] as f64;
                let w = wv.as_f64().unwrap();
                assert!((got - w).abs() <= 1e-6, "{case} sample {s_idx}: got {got} want {w}");
            }
        }
    }
    assert!(checked >= 1, "expected at least 1 gaussian case, got {checked}");

    // Non-cubic grid (3x5x2) with sigma = 2.0 (radius 8 > all dimensions: tests multi-period
    // reflect boundary extension and non-cubic axis strides) and sigma = 0.0 no-op.
    let raw: Vec<f32> = (0..30).map(|i| (i as f32) * 0.37f32 - 4.1f32).collect();
    let mut noop = Grid3 { n: [3, 5, 2], data: raw.clone() };
    gaussian_filter(&mut noop, 0.0);
    assert_eq!(noop.data, raw);

    let mut small = Grid3 { n: [3, 5, 2], data: raw };
    gaussian_filter(&mut small, 2.0);
    const WANT_3_5_2: [f64; 30] = [
        0.21428610384464264, 0.21694770455360413, 0.45757582783699036, 0.46023741364479065,
        0.8508502244949341, 0.8535118103027344, 1.2441245317459106, 1.246786117553711,
        1.4874142408370972, 1.4900758266448975, 0.6271052956581116, 0.6297668814659119,
        0.8703950643539429, 0.8730566501617432, 1.2636693716049194, 1.2663309574127197,
        1.6569437980651855, 1.6596053838729858, 1.9002333879470825, 1.9028950929641724,
        1.0399245023727417, 1.042586088180542, 1.2832142114639282, 1.2858757972717285,
        1.6764885187149048, 1.6791502237319946, 2.069762945175171, 2.0724246501922607,
        2.3130526542663574, 2.3157143592834473,
    ];
    for (idx, (&got, &want)) in small.data.iter().zip(WANT_3_5_2.iter()).enumerate() {
        let diff = (got as f64 - want).abs();
        assert!(diff <= 1e-6, "3x5x2 cell {idx}: got {got} want {want} (diff {diff})");
    }
}

#[test]
fn surface_points_match_1x() {
    let (raw_v, raw_f) = read_stl_soup("holed_block.stl");
    let flat_v: Vec<f64> = raw_v.iter().flatten().copied().collect();
    let flat_f: Vec<u32> = raw_f.iter().flatten().copied().collect();
    let (wv_flat, wf_flat) = weld(&flat_v, &flat_f, 1e-5);
    let wv: Vec<[f64; 3]> = wv_flat.chunks_exact(3).map(|p| [p[0], p[1], p[2]]).collect();
    let wf: Vec<[u32; 3]> = wf_flat.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();

    // 1. Welded holed_block.stl at spacing = 15.0 exercises verts + k = 1, 2, 3 in np.unique(k) order.
    let pts = surface_points(&wv, &wf, 15.0);
    assert_eq!(pts.len(), 528, "welded holed_block spacing=15 count");

    const SAMPLES_15: &[(usize, [f64; 3])] = &[
        (0, [0.0, 0.0, 0.0]),
        (17, [7.50559139251709, 11.66671085357666, 10.0]),
        (34, [10.0, 7.0, 0.0]),
        (51, [12.121320724487305, 7.8786797523498535, 10.0]),
        (68, [17.0, 10.0, 0.0]),
        (85, [17.878679275512695, 12.121320724487305, 10.0]),
        (102, [20.58527183532715, 7.057644367218018, 0.0]),
        (119, [22.494409561157227, 8.33328914642334, 10.0]),
        (135, [30.0, 20.0, 10.0]),
        (136, [5.728412946065267, 4.911317666371663, 0.0]),
        (153, [14.607011795043945, 9.097802956899008, 0.0]),
        (170, [14.04043992360433, 8.181752681732178, 10.0]),
        (187, [4.911317666371663, 14.271587053934732, 10.0]),
        (204, [15.61842409769694, 9.768853187561035, 0.0]),
        (221, [24.271586736043293, 4.911317666371663, 10.0]),
        (238, [25.088682810465496, 14.271587053934732, 0.0]),
        (255, [25.31411870320638, 13.528423627217611, 10.0]),
        (272, [8.030216217041016, 7.754316965738933, 3.3333333333333335]),
        (289, [8.181752681732178, 7.629954179128011, 6.666666666666667]),
        (306, [12.980785369873047, 10.195090293884277, 3.3333333333333335]),
        (323, [12.828544616699219, 10.96045716603597, 6.666666666666667]),
        (340, [22.245683670043945, 8.030216217041016, 3.3333333333333335]),
        (357, [22.679229100545246, 8.679062843322754, 6.666666666666667]),
        (374, [22.88545036315918, 10.7728640238444, 3.3333333333333335]),
        (391, [22.586819330851238, 11.493824005126953, 6.666666666666667]),
        (392, [0.0, 0.0, 0.0]),
        (408, [20.292635440826416, 3.528822183609009, 10.0]),
        (425, [9.707364082336426, 16.47117805480957, 0.0]),
        (442, [19.41472816467285, 7.057644367218018, 0.0]),
        (459, [20.292635440826416, 3.528822183609009, 10.0]),
        (476, [0.0, 20.0, 10.0]),
        (487, [30.0, 20.0, 0.0]),
        (488, [0.0, 0.0, 0.0]),
        (493, [10.0, 0.0, 3.333333333333333]),
        (510, [20.0, 0.0, 10.0]),
        (527, [30.0, 20.0, 10.0]),
    ];
    for &(idx, want) in SAMPLES_15 {
        let got = pts[idx];
        for c in 0..3 {
            let diff = (got[c] - want[c]).abs();
            assert!(diff <= 1e-12, "pts15[{idx}][{c}]: got {} want {} (diff {diff})", got[c], want[c]);
        }
    }

    let want_sum15 = [7920.000006357831, 5280.000038782757, 2640.0];
    let want_wsum15 = [4161.755797733865, 2687.177564419578, 1341.6698232323233];
    let (mut sum15, mut wsum15) = ([0.0f64; 3], [0.0f64; 3]);
    let n15 = pts.len() as f64;
    for (i, p) in pts.iter().enumerate() {
        let w = (i + 1) as f64 / n15;
        for c in 0..3 {
            sum15[c] += p[c];
            wsum15[c] += p[c] * w;
        }
    }
    for c in 0..3 {
        assert!((sum15[c] - want_sum15[c]).abs() <= 1e-12 * n15, "sum15[{c}] got {} want {}", sum15[c], want_sum15[c]);
        assert!(
            (wsum15[c] - want_wsum15[c]).abs() <= 1e-12 * n15,
            "wsum15[{c}] got {} want {}",
            wsum15[c],
            want_wsum15[c]
        );
    }

    // 2. Raw holed_block.stl at spacing = 1.0 (as called by mesh_occupancy(v, f, 2.0)).
    let pts1 = surface_points(&raw_v, &raw_f, 1.0);
    assert_eq!(pts1.len(), 27348, "raw holed_block spacing=1 count");
    const SAMPLES_1: &[(usize, [f64; 3])] = &[
        (0, [0.0, 0.0, 0.0]),
        (827, [22.494409561157227, 11.66671085357666, 10.0]),
        (828, [17.505590438842773, 8.33328914642334, 10.0]),
        (13674, [7.2514640887578325, 8.808727979660032, 10.0]),
        (27347, [30.0, 20.0, 10.0]),
    ];
    for &(idx, want) in SAMPLES_1 {
        let got = pts1[idx];
        for c in 0..3 {
            let diff = (got[c] - want[c]).abs();
            assert!(diff <= 1e-12, "pts1[{idx}][{c}]: got {} want {} (diff {diff})", got[c], want[c]);
        }
    }
    let want_sum1 = [410220.00037860934, 273480.00196154957, 137000.00000000012];
    let want_wsum1 = [212631.80306116142, 138746.17249978957, 69936.23016308306];
    let (mut sum1, mut wsum1) = ([0.0f64; 3], [0.0f64; 3]);
    let n1 = pts1.len() as f64;
    for (i, p) in pts1.iter().enumerate() {
        let w = (i + 1) as f64 / n1;
        for c in 0..3 {
            sum1[c] += p[c];
            wsum1[c] += p[c] * w;
        }
    }
    for c in 0..3 {
        assert!((sum1[c] - want_sum1[c]).abs() <= 1e-12 * n1, "sum1[{c}] got {} want {}", sum1[c], want_sum1[c]);
        assert!((wsum1[c] - want_wsum1[c]).abs() <= 1e-12 * n1, "wsum1[{c}] got {} want {}", wsum1[c], want_wsum1[c]);
    }
}

#[test]
fn check_grid_rejects_before_alloc() {
    assert_eq!(check_grid([1000, 1000, 300]), Ok(()));
    assert_eq!(check_grid([1000, 1000, 301]), Err(MeshError::Message(CAP_MSG.to_string())));
    assert_eq!(Grid3::try_new([1000, 1000, 301], false), Err(MeshError::Message(CAP_MSG.to_string())));
    assert_eq!(
        Grid3::try_new([usize::MAX, usize::MAX, usize::MAX], 0.0f32),
        Err(MeshError::Message(CAP_MSG.to_string()))
    );
}
