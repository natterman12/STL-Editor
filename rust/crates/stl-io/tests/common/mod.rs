#![allow(dead_code)]
use std::path::PathBuf;

use serde_json::Value;
use stl_io::{ExportBody, ExportFormat, ExportOptions, export_bodies, load_mesh_file};
use stl_kernel::MeshArrays;

fn tests_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests")
}

pub fn export_dir() -> PathBuf {
    tests_dir().join("parity/export")
}

pub fn opts() -> ExportOptions {
    ExportOptions { uuid_seed: 1, step_title: String::new(), step_date: "2026-01-01T00:00:00".into() }
}

/// The cases in `cases.json`, in file order.
pub fn cases() -> Vec<(String, Value)> {
    let path = export_dir().join("cases.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        panic!("{} is missing: run dev/parity/make_export_golden.py", path.display());
    };
    let v: Value = serde_json::from_str(&text).unwrap();
    v.as_object().unwrap().iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

fn find_model(file: &str) -> Vec<u8> {
    for dir in ["models", "models/export", "models/readers"] {
        if let Ok(d) = std::fs::read(tests_dir().join(dir).join(file)) {
            return d;
        }
    }
    panic!("model {file} not found under rust/tests/models");
}

/// Owned bodies of a case: (name, mesh) for every model part, in case order.
pub fn load_bodies(case: &Value) -> Vec<(String, MeshArrays)> {
    let mut out = Vec::new();
    for m in case["models"].as_array().unwrap() {
        let file = m["file"].as_str().unwrap();
        let name = m["name"].as_str().unwrap();
        let parts = load_mesh_file(&find_model(file), file).unwrap_or_else(|e| panic!("{file}: {e:?}"));
        for p in parts {
            // The 1.x golden exports the mesh as read (the reader's vertex and triangle order).
            let mesh = MeshArrays { positions: p.positions.iter().map(|v| *v as f32).collect(), triangles: p.triangles };
            out.push((name.to_string(), mesh));
        }
    }
    out
}

/// The bodies `Document::export` writes: `body_ids` if given, else the visible ones, else all.
pub fn exported_bodies(case: &Value) -> Vec<(String, MeshArrays)> {
    let all = load_bodies(case);
    let names = |key: &str| -> Option<Vec<String>> {
        case[key].as_array().map(|a| a.iter().map(|v| v.as_str().unwrap().to_string()).collect())
    };
    if let Some(ids) = names("body_ids").filter(|v| !v.is_empty()) {
        return ids.iter().filter_map(|i| all.iter().find(|(n, _)| n == i).cloned()).collect();
    }
    let hidden = names("hidden").unwrap_or_default();
    let visible: Vec<_> = all.iter().filter(|(n, _)| !hidden.contains(n)).cloned().collect();
    if visible.is_empty() { all } else { visible }
}

pub fn export_bodies_of(bodies: &[(String, MeshArrays)], format: ExportFormat) -> Vec<u8> {
    let refs: Vec<ExportBody> =
        bodies.iter().map(|(n, m)| ExportBody { name: n.clone(), mesh: m }).collect();
    export_bodies(&refs, format, &opts()).unwrap()
}

pub fn golden(case: &str) -> Value {
    let path = export_dir().join(case).join("golden.json");
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())))
        .unwrap()
}

/// 3MF: every UUID value replaced by `UUID` before comparing.
#[allow(dead_code)]
pub fn uuid_normalize(xml: &str) -> String {
    let mut out = String::new();
    let mut rest = xml;
    while let Some(i) = rest.find("UUID=\"") {
        let (head, tail) = rest.split_at(i + 6);
        out.push_str(head);
        out.push_str("UUID");
        match tail.find('"') {
            Some(j) => rest = &tail[j..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// A golden file's bytes: the file itself, or only its `{sha256, size}` entry in `golden.json`.
/// Returns (expected size, expected sha256 hex or None, full bytes or None).
pub fn golden_file(case: &str, file: &str) -> (Option<Vec<u8>>, Option<Value>) {
    let p = export_dir().join(case).join(file);
    if let Ok(b) = std::fs::read(&p) {
        return (Some(b), None);
    }
    (None, Some(golden(case)[file].clone()))
}

/// SHA-256 (golden files over 1 MB are stored as hashes; no hashing crate is allowed).
#[allow(dead_code)]
pub fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(chunk[i * 4..i * 4 + 4].try_into().unwrap());
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
        for i in 0..8 {
            h[i] = h[i].wrapping_add(v[i]);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}
