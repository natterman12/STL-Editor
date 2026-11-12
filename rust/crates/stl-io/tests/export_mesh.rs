mod common;
use common::*;

use std::io::Read;
use stl_io::{ExportFormat, load_mesh_file};

fn has_bodies(case: &serde_json::Value) -> bool {
    !case["models"].as_array().unwrap().is_empty()
}

#[test]
fn obj_matches_1x() {
    for (name, case) in cases() {
        if !has_bodies(&case) {
            continue;
        }
        let got = export_bodies_of(&exported_bodies(&case), ExportFormat::Obj);
        let (bytes, hash) = golden_file(&name, "out.obj");
        match (bytes, hash) {
            (Some(b), _) => assert!(got == b, "{name}: out.obj differs"),
            (None, Some(h)) => assert_eq!(sha256_hex(&got), h["sha256"].as_str().unwrap(), "{name}"),
            _ => unreachable!(),
        }
    }
}

fn zip_entries(data: &[u8]) -> Vec<(String, Vec<u8>, zip::CompressionMethod)> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(data)).unwrap();
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).unwrap();
            let mut buf = Vec::new();
            f.read_to_end(&mut buf).unwrap();
            (f.name().unwrap().to_string(), buf, f.compression())
        })
        .collect()
}

#[test]
fn threemf_matches_1x() {
    for (name, case) in cases() {
        if !has_bodies(&case) {
            continue;
        }
        let got = export_bodies_of(&exported_bodies(&case), ExportFormat::ThreeMf);
        let entries = zip_entries(&got);
        let names: Vec<&str> = entries.iter().map(|e| e.0.as_str()).collect();
        assert_eq!(names, ["3D/3dmodel.model", "_rels/.rels", "[Content_Types].xml"], "{name}");
        for (e, file) in entries.iter().zip(["3dmodel.model", ".rels", "[Content_Types].xml"]) {
            assert_eq!(e.2, zip::CompressionMethod::Deflated, "{name}: {file}");
            let text = String::from_utf8(e.1.clone()).unwrap();
            let want = String::from_utf8(golden_file(&name, file).0.expect("golden file")).unwrap();
            assert!(uuid_normalize(&text) == want, "{name}: {file} differs");
        }
    }
}

#[test]
fn threemf_reads_back() {
    let close = |what: &str, a: f64, b: f64, tol_abs: f64| {
        assert!((a - b).abs() <= tol_abs.max(0.001 * b.abs()), "{what}: got {a}, golden {b}");
    };
    for (name, case) in cases() {
        if !has_bodies(&case) {
            continue;
        }
        let got = export_bodies_of(&exported_bodies(&case), ExportFormat::ThreeMf);
        let want = golden(&name)["readback"]["3mf"].clone();
        let parts = load_mesh_file(&got, "out.3mf").unwrap();
        let want = want.as_array().unwrap();
        assert_eq!(parts.len(), want.len(), "{name}: part count");
        for (p, w) in parts.iter().zip(want) {
            assert_eq!(p.name, w["name"].as_str().unwrap(), "{name}: part name");
            let s = &w["stats"];
            assert_eq!(p.triangles.len() / 3, s["triangles"].as_u64().unwrap() as usize, "{name}: triangles");
            assert_eq!(p.positions.len() / 3, s["vertices"].as_u64().unwrap() as usize, "{name}: vertices");
            assert_eq!(is_watertight(&p.triangles), s["watertight"].as_bool().unwrap(), "{name}: watertight");
            let (vol, area) = volume_area(&p.positions, &p.triangles);
            close(&format!("{name}: volume"), vol, s["volume"].as_f64().unwrap(), 0.0);
            close(&format!("{name}: area"), area, s["area"].as_f64().unwrap(), 0.0);
            for k in 0..3 {
                let col = p.positions.iter().skip(k).step_by(3);
                let lo = col.clone().cloned().fold(f64::INFINITY, f64::min);
                let hi = col.cloned().fold(f64::NEG_INFINITY, f64::max);
                close(&format!("{name}: bbox_min[{k}]"), lo, s["bbox_min"][k].as_f64().unwrap(), 1e-4);
                close(&format!("{name}: bbox_max[{k}]"), hi, s["bbox_max"][k].as_f64().unwrap(), 1e-4);
            }
        }
    }
}

/// trimesh `is_watertight`: every edge is used by exactly two faces.
fn is_watertight(tri: &[u32]) -> bool {
    let mut count = std::collections::HashMap::new();
    for t in tri.chunks_exact(3) {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *count.entry((a.min(b), a.max(b))).or_insert(0u32) += 1;
        }
    }
    !count.is_empty() && count.values().all(|&c| c == 2)
}

fn volume_area(pos: &[f64], tri: &[u32]) -> (f64, f64) {
    let v = |i: u32| [pos[i as usize * 3], pos[i as usize * 3 + 1], pos[i as usize * 3 + 2]];
    let (mut vol, mut area) = (0.0, 0.0);
    for t in tri.chunks_exact(3) {
        let (a, b, c) = (v(t[0]), v(t[1]), v(t[2]));
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let w = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]];
        area += 0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        vol += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]))
            / 6.0;
    }
    (vol, area)
}
