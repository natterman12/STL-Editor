//! `.stlx` project files: reading what 1.x wrote, and writing what 1.x can open.

use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use stl_engine::stlx;
use stl_engine::{Document, Registry};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/projects").join(format!("{name}.stlx"))
}

const NAMES: [&str; 3] = ["basic", "unicode", "extra_keys"];

/// `document.json` and every `sources/...` entry of a project file.
fn raw(path: &Path) -> (Value, Vec<(String, Vec<u8>)>) {
    let mut z = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut doc = Vec::new();
    z.by_name("document.json").unwrap().read_to_end(&mut doc).unwrap();
    let mut files = Vec::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        if f.name().unwrap().starts_with("sources/") {
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            files.push((f.name().unwrap().to_string(), b));
        }
    }
    files.sort();
    (serde_json::from_slice(&doc).unwrap(), files)
}

fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (n, b) in entries {
        w.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(b).unwrap();
    }
    w.finish().unwrap().into_inner()
}

fn as_json(d: &Document) -> (Value, Value, Value) {
    (
        serde_json::to_value(&d.features).unwrap(),
        Value::Object(d.ui.clone()),
        serde_json::to_value(&d.params).unwrap(),
    )
}

#[test]
fn opens_1x_fixtures() {
    for n in NAMES {
        let path = fixture(n);
        let doc = stlx::open(&path, Registry::builtin()).unwrap();
        let (j, files) = raw(&path);
        let (f, u, p) = as_json(&doc);
        assert_eq!(f, j["features"], "{n} features");
        assert_eq!(u, j["ui"], "{n} ui");
        assert_eq!(p, j["params"], "{n} params");
        assert_eq!(doc.rollback.map(|r| json!(r)).unwrap_or(Value::Null), j["rollback"], "{n} rollback");
        assert_eq!(doc.sources.len(), files.len(), "{n} sources");
        for (sid, s) in &doc.sources {
            let want = files.iter().find(|(name, _)| *name == format!("sources/{sid}{}", s.ext)).unwrap();
            assert_eq!(s.data, want.1, "{n} {sid} bytes");
            assert_eq!(s.name, j["sources"][sid]["name"].as_str().unwrap());
        }
    }
}

#[test]
fn extra_keys_round_trip() {
    let doc = stlx::open(&fixture("extra_keys"), Registry::builtin()).unwrap();
    assert!(doc.features[1].suppressed);
    assert_eq!(doc.features[1].extra["memo"], json!("x"));
    assert_eq!(doc.ui["camera"], json!({"pos": [1, 2, 3]}));
    let back = stlx::from_bytes(&stlx::to_bytes(&doc).unwrap(), Registry::builtin()).unwrap();
    assert_eq!(as_json(&back), as_json(&doc));
    assert_eq!(back.features[1].extra["memo"], json!("x"));
    assert!(back.features[1].suppressed);
}

#[test]
fn unknown_top_level_keys_are_kept() {
    let (mut j, _) = raw(&fixture("extra_keys"));
    j["future"] = json!({"a": [1, 2]});
    let src = raw(&fixture("extra_keys")).1;
    let bytes = zip_of(&[
        ("document.json", serde_json::to_vec(&j).unwrap().as_slice()),
        (&src[0].0, &src[0].1),
    ]);
    let doc = stlx::from_bytes(&bytes, Registry::builtin()).unwrap();
    let out = zip_of_bytes(&stlx::to_bytes(&doc).unwrap());
    assert_eq!(out["future"], json!({"a": [1, 2]}));
}

fn zip_of_bytes(data: &[u8]) -> Value {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(data)).unwrap();
    let mut b = Vec::new();
    z.by_name("document.json").unwrap().read_to_end(&mut b).unwrap();
    serde_json::from_slice(&b).unwrap()
}

#[test]
fn unicode_round_trip() {
    let doc = stlx::open(&fixture("unicode"), Registry::builtin()).unwrap();
    let back = stlx::from_bytes(&stlx::to_bytes(&doc).unwrap(), Registry::builtin()).unwrap();
    assert_eq!(as_json(&back), as_json(&doc));
    assert_eq!(back.sources["s1"].name, "Teil ö.stl");
    assert_eq!(back.sources["s1"].data, doc.sources["s1"].data);
    assert_eq!(back.ui["names"]["f2"], json!("Grün"));
    assert_eq!(back.params[0].note, "größe");
}

#[test]
fn id_counter_continues() {
    let mut doc = stlx::open(&fixture("basic"), Registry::builtin()).unwrap();
    assert_eq!(doc.new_id("f"), "f6");
    assert_eq!(doc.add_source("more.stl", vec![1]), "s7"); // one counter shared by all prefixes, as in 1.x
}

#[test]
fn suppressed_false_round_trips() {
    let (mut j, files) = raw(&fixture("basic"));
    j["features"][0]["suppressed"] = json!(false);
    let doc_bytes = serde_json::to_vec(&j).unwrap();
    let mut entries: Vec<(&str, &[u8])> = vec![("document.json", doc_bytes.as_slice())];
    for (n, b) in &files {
        entries.push((n.as_str(), b.as_slice()));
    }
    let doc = stlx::from_bytes(&zip_of(&entries), Registry::builtin()).unwrap();
    assert!(!doc.features[0].suppressed);
    let out = zip_of_bytes(&stlx::to_bytes(&doc).unwrap());
    assert_eq!(out["features"][0]["suppressed"], json!(false));
    // features that never had the key still don't get one
    assert!(out["features"][1].get("suppressed").is_none());
}

#[test]
fn only_import_sources_are_written() {
    let mut doc = stlx::open(&fixture("unicode"), Registry::builtin()).unwrap();
    doc.add_source("spare.stl", vec![1, 2, 3]);
    let j = zip_of_bytes(&stlx::to_bytes(&doc).unwrap());
    assert_eq!(j["format"], json!(1));
    assert_eq!(j["app"], json!("STL Editor"));
    assert_eq!(j["sources"].as_object().unwrap().len(), 1);
}

#[test]
fn rejects_non_project() {
    let msg = "This isn't an STL Editor project.";
    assert_eq!(stlx::from_bytes(b"hello", Registry::builtin()).err().unwrap().0, msg);
    let no_doc = zip_of(&[("other.txt", b"x")]);
    assert_eq!(stlx::from_bytes(&no_doc, Registry::builtin()).err().unwrap().0, msg);
    let bad_json = zip_of(&[("document.json", b"not json")]);
    assert_eq!(stlx::from_bytes(&bad_json, Registry::builtin()).err().unwrap().0, msg);
}

#[test]
fn rejects_newer_format() {
    let d = zip_of(&[("document.json", br#"{"format": 2, "features": []}"#)]);
    assert_eq!(
        stlx::from_bytes(&d, Registry::builtin()).err().unwrap().0,
        "This project was made by a newer version of STL Editor."
    );
}

#[test]
fn rejects_missing_source() {
    let d = zip_of(&[(
        "document.json",
        br#"{"format": 1, "features": [], "sources": {"s1": {"name": "a.stl", "ext": ".stl"}}}"#,
    )]);
    assert_eq!(
        stlx::from_bytes(&d, Registry::builtin()).err().unwrap().0,
        "The imported file is missing from the project."
    );
}

#[test]
fn writes_for_1x() {
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/parity-out");
    std::fs::create_dir_all(&out).unwrap();
    for n in NAMES {
        let doc = stlx::open(&fixture(n), Registry::builtin()).unwrap();
        let dest = out.join(format!("{n}.stlx"));
        stlx::save(&doc, &dest).unwrap();
        // what was written reads back the same, and the sources are in 1.x's places
        let (j, files) = raw(&dest);
        assert_eq!(j["format"], json!(1));
        assert_eq!(j["features"], raw(&fixture(n)).0["features"], "{n}");
        assert_eq!(files, raw(&fixture(n)).1, "{n} sources");
    }
}
