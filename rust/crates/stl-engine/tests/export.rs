//! Document export: the 1.x golden cases (rust/tests/parity/export), file naming and errors.

use std::path::PathBuf;

use serde_json::{json, Value};
use stl_engine::{iso_utc_now, Document, Registry};
use stl_io::{ExportFormat, ExportOptions};

fn export_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/export")
}

fn find_model(file: &str) -> Vec<u8> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/models");
    for dir in ["", "export"] {
        if let Ok(d) = std::fs::read(root.join(dir).join(file)) {
            return d;
        }
    }
    panic!("model {file} not found");
}

/// A real Document built the way 1.x `make_export_golden.py` does.
fn build(case: &Value) -> Document {
    let mut doc = Document::new(Registry::builtin());
    doc.title = case["title"].as_str().map(String::from);
    let mut ids = Vec::new();
    for m in case["models"].as_array().unwrap() {
        let file = m["file"].as_str().unwrap();
        let sid = doc.add_source(file, find_model(file));
        let params = json!({"source": sid, "repair": false, "place": "keep"}).as_object().unwrap().clone();
        doc.add_feature("import", params, None).unwrap();
        let bid = doc.state.bodies.keys().last().unwrap().clone();
        doc.set_body_props(&bid, Some(m["name"].as_str().unwrap().to_string()), None, None);
        ids.push(bid);
    }
    let hidden: Vec<&str> = case["hidden"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    for bid in ids {
        if hidden.contains(&doc.body_name(&bid).as_str()) {
            doc.set_body_props(&bid, None, None, Some(true));
        }
    }
    doc
}

fn body_ids(doc: &Document, case: &Value) -> Option<Vec<String>> {
    case["body_ids"].as_array().map(|a| {
        a.iter()
            .map(|n| {
                doc.state.bodies.keys().find(|b| doc.body_name(b) == n.as_str().unwrap()).unwrap().clone()
            })
            .collect()
    })
}

fn uuid_normalize(xml: &str) -> String {
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

fn zip_entry(bytes: &[u8], name: &str) -> String {
    use std::io::Read;
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut s = String::new();
    z.by_name(name).unwrap().read_to_string(&mut s).unwrap();
    s
}

#[test]
fn export_cases_match_1x() {
    let cases: Value = serde_json::from_str(&std::fs::read_to_string(export_dir().join("cases.json")).unwrap()).unwrap();
    for (name, case) in cases.as_object().unwrap() {
        let g: Value =
            serde_json::from_str(&std::fs::read_to_string(export_dir().join(name).join("golden.json")).unwrap()).unwrap();
        let doc = build(case);
        let ids = body_ids(&doc, case);
        let opts = |base: &str| ExportOptions {
            uuid_seed: 1,
            step_title: base.to_string(),
            step_date: "2026-01-01T00:00:00".into(),
        };
        // error, count and base name
        if let Some(err) = g["error"].as_str() {
            let e = doc.export_bytes(ExportFormat::Stl, ids.as_deref(), &opts("")).unwrap_err();
            assert_eq!(e.0, err, "{name}: error");
            continue;
        }
        let used = doc.export_bodies_for(ids.as_deref()).unwrap();
        assert_eq!(used.len() as u64, g["count"].as_u64().unwrap(), "{name}: count");
        let base = doc.export_base_name(&used, case["path"].as_str());
        assert_eq!(base, g["base"].as_str().unwrap(), "{name}: base");
        // What was written, read back, has the stats 1.x measured on its own files. (The golden
        // files come from the raw source mesh; a Document holds the solid's mesh, whose vertex
        // order differs, so bytes are compared against the "document" block below.)
        for (fmt, key, file) in [
            (ExportFormat::Stl, "stl", "out.stl"),
            (ExportFormat::Obj, "obj", "out.obj"),
            (ExportFormat::ThreeMf, "3mf", "out.3mf"),
        ] {
            let (got, n) = doc.export_bytes(fmt, ids.as_deref(), &opts(&base)).unwrap();
            assert_eq!(n, used.len());
            let parts = stl_io::load_mesh_file(&got, file).unwrap();
            let want = g["readback"][key].as_array().unwrap();
            assert_eq!(parts.len(), want.len(), "{name}/{key}: part count");
            for (p, w) in parts.iter().zip(want) {
                let s = &w["stats"];
                assert_eq!(p.triangles.len() / 3, s["triangles"].as_u64().unwrap() as usize, "{name}/{key}: triangles");
                let (vol, area) = volume_area(&p.positions, &p.triangles);
                let close = |what: &str, a: f64, b: f64, abs: f64| {
                    assert!((a - b).abs() <= abs.max(0.001 * b.abs()), "{name}/{key}: {what}: got {a}, golden {b}");
                };
                close("volume", vol, s["volume"].as_f64().unwrap(), 0.0);
                close("area", area, s["area"].as_f64().unwrap(), 0.0);
                for k in 0..3 {
                    let col = p.positions.iter().skip(k).step_by(3);
                    let lo = col.clone().cloned().fold(f64::INFINITY, f64::min);
                    let hi = col.cloned().fold(f64::NEG_INFINITY, f64::max);
                    close("bbox_min", lo, s["bbox_min"][k].as_f64().unwrap(), 1e-4);
                    close("bbox_max", hi, s["bbox_max"][k].as_f64().unwrap(), 1e-4);
                }
            }
        }
        // Bytes of a real 1.x Document.export: STL and OBJ whole, the 3MF model XML after UUID/CRLF normalising.
        let d = &g["document"];
        let (stl, _) = doc.export_bytes(ExportFormat::Stl, ids.as_deref(), &opts(&base)).unwrap();
        let (obj, _) = doc.export_bytes(ExportFormat::Obj, ids.as_deref(), &opts(&base)).unwrap();
        let (mf, _) = doc.export_bytes(ExportFormat::ThreeMf, ids.as_deref(), &opts(&base)).unwrap();
        let xml = uuid_normalize(&zip_entry(&mf, "3D/3dmodel.model")).replace("\r\n", "\n");
        for (key, bytes) in [("stl", stl), ("obj", obj), ("3mf_model_xml", xml.into_bytes())] {
            assert_eq!(bytes.len() as u64, d[key]["size"].as_u64().unwrap(), "{name}/{key}: size");
            assert_eq!(sha(&bytes), d[key]["sha256"].as_str().unwrap(), "{name}/{key}: sha256");
        }
        // 3MF structure: the three entries, with the same text apart from the mesh numbers' order.
        let (got, _) = doc.export_bytes(ExportFormat::ThreeMf, ids.as_deref(), &opts(&base)).unwrap();
        for (entry, file) in [("_rels/.rels", ".rels"), ("[Content_Types].xml", "[Content_Types].xml")] {
            let want = std::fs::read_to_string(export_dir().join(name).join(file)).unwrap();
            assert_eq!(uuid_normalize(&zip_entry(&got, entry)), want, "{name}: 3mf {entry}");
        }
        // STEP: written, or refused with 1.x's (step.js's) message.
        match doc.export_bytes(ExportFormat::Step, ids.as_deref(), &opts(&base)) {
            Ok((got, _)) => {
                assert!(g["step_error"].is_null(), "{name}: STEP should have failed");
                let text = String::from_utf8(got).unwrap();
                assert!(text.starts_with("ISO-10303-21;") && text.contains("FILE_NAME("), "{name}: step header");
            }
            Err(e) => assert_eq!(Some(e.0.as_str()), g["step_error"].as_str(), "{name}: step error"),
        }
    }
}

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("stl_export_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn one_body() -> Document {
    let cases: Value = serde_json::from_str(&std::fs::read_to_string(export_dir().join("cases.json")).unwrap()).unwrap();
    build(&cases["one_body"])
}

#[test]
fn export_adds_extension() {
    let d = temp_dir("ext");
    let (p, n) = one_body().export(&d.join("part"), ExportFormat::ThreeMf, None, None).unwrap();
    assert_eq!(p, d.join("part.3mf"));
    assert_eq!(n, 1);
    assert!(std::fs::read(&p).unwrap().starts_with(b"PK"));
}

#[test]
fn export_ext_case() {
    let d = temp_dir("case");
    let (p, _) = one_body().export(&d.join("PART.STL"), ExportFormat::ThreeMf, None, None).unwrap();
    assert_eq!(p, d.join("PART.STL"));
    let bytes = std::fs::read(&p).unwrap();
    assert!(bytes.starts_with(b"STL Editor binary STL"));
}

#[test]
fn export_unknown_ext() {
    let d = temp_dir("bad");
    let e = one_body().export(&d.join("a.ply"), ExportFormat::Stl, None, None).unwrap_err();
    assert_eq!(e.0, "Can't export .ply files.");
    assert!(!d.join("a.ply").exists());
}

#[test]
fn iso_utc_now_shape() {
    let s = iso_utc_now();
    assert_eq!(s.len(), 19);
    for (i, c) in s.bytes().enumerate() {
        match i {
            4 | 7 => assert_eq!(c, b'-'),
            10 => assert_eq!(c, b'T'),
            13 | 16 => assert_eq!(c, b':'),
            _ => assert!(c.is_ascii_digit(), "{s}"),
        }
    }
    let n = |r: std::ops::Range<usize>| s[r].parse::<u32>().unwrap();
    assert!(n(0..4) >= 2026 && (1..=12).contains(&n(5..7)) && (1..=31).contains(&n(8..10)));
    assert!(n(11..13) < 24 && n(14..16) < 60 && n(17..19) < 61);
    assert_eq!(stl_engine::iso_from_unix_secs(0), "1970-01-01T00:00:00");
    assert_eq!(stl_engine::iso_from_unix_secs(1_782_950_400), "2026-07-02T00:00:00");
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

fn sha(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect()
}
