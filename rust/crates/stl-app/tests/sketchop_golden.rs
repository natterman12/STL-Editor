//! Every Task 0 `sketch_op` case through `Document::sketch_op` matches 1.x JSON.

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Map, Value};
use stl_engine::{Document, Registry};

fn parity_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/sketchop")
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn norm(v: &Value) -> Value {
    match v {
        Value::Number(n) => json!(n.as_f64().unwrap_or(f64::NAN)),
        Value::Array(a) => Value::Array(a.iter().map(norm).collect()),
        Value::Object(o) => {
            let mut m = Map::new();
            for (k, child) in o {
                m.insert(k.clone(), norm(child));
            }
            Value::Object(m)
        }
        other => other.clone(),
    }
}

/// `offset_open` only: numbers may differ by at most this much.
/// 2.0 follows 1.x desktop (GEOS), which is exactly 1.5; 1.x web `offsetOpen` is
/// 1.4999999999999998 (float noise in its mitre formula). See `rust/tests/EXCEPTIONS.md`.
const OFFSET_OPEN_ABS_TOL: f64 = 1e-12;

fn numbers_match(x: f64, y: f64, tol: f64) -> bool {
    if x == y || (x - y).abs() == 0.0 {
        true
    } else {
        tol > 0.0 && (x - y).abs() <= tol
    }
}

fn all_diffs(got: &Value, want: &Value, path: &str, tol: f64, out: &mut Vec<String>) {
    match (got, want) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, bv) in b {
                match a.get(k) {
                    None => out.push(format!("{path}.{k}: missing")),
                    Some(av) => all_diffs(av, bv, &format!("{path}.{k}"), tol, out),
                }
            }
            for k in a.keys() {
                if !b.contains_key(k) {
                    out.push(format!("{path}.{k}: unexpected"));
                }
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                out.push(format!("{path}: length {} != {}", a.len(), b.len()));
                return;
            }
            for (i, (av, bv)) in a.iter().zip(b.iter()).enumerate() {
                all_diffs(av, bv, &format!("{path}[{i}]"), tol, out);
            }
        }
        (Value::Number(a), Value::Number(b)) => {
            let (x, y) = (a.as_f64().unwrap_or(f64::NAN), b.as_f64().unwrap_or(f64::NAN));
            if !numbers_match(x, y, tol) {
                out.push(format!("{path}: {x} != {y}"));
            }
        }
        (a, b) if a == b => {}
        (a, b) => out.push(format!("{path}: {a} != {b}")),
    }
}

fn object(v: Value) -> Map<String, Value> {
    match v {
        Value::Object(m) => m,
        _ => Map::new(),
    }
}

fn box_doc() -> Document {
    let mut doc = Document::new(Registry::builtin());
    let params = object(json!({
        "shape": "box",
        "length": 40,
        "width": 20,
        "height": 10,
        "anchor": "corner",
        "plane": "XY",
        "operation": "new",
        "offset": [0, 0, 0],
        "rotate": [0, 0, 0],
    }));
    doc.add_feature("primitive", params, None).expect("box");
    doc
}

fn tipped_doc() -> Document {
    let mut doc = Document::new(Registry::builtin());
    let path = repo_root().join("tests/ui/fixtures/tipped.stl");
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let sid = doc.add_source("tipped.stl", bytes);
    let params = object(json!({"source": sid, "repair": true, "place": "center"}));
    doc.add_feature("import", params, None).expect("tipped");
    doc
}

fn document_for(name: &str) -> Document {
    if name.starts_with("project_") || name == "refs_box" {
        box_doc()
    } else if name == "refs_tipped" {
        tipped_doc()
    } else {
        Document::new(Registry::builtin())
    }
}

#[test]
fn sketchop_golden_matches() {
    let dir = parity_dir();
    let mut names: Vec<String> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            n.strip_suffix(".json").map(str::to_string)
        })
        .collect();
    names.sort();
    assert_eq!(names.len(), 24, "expected the 24 sketch_op cases, got {names:?}");
    let mut failures = Vec::new();
    for name in &names {
        let path = dir.join(format!("{name}.json"));
        let case: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
        let op = case["op"].as_str().unwrap();
        let data = &case["data"];
        let doc = document_for(name);
        let got = doc.sketch_op(op, data).unwrap_or_else(|e| panic!("{name}: {e}"));
        let (got, want) = (norm(&got), norm(&case["result"]));
        let tol = if name == "offset_open" { OFFSET_OPEN_ABS_TOL } else { 0.0 };
        let mut found = Vec::new();
        all_diffs(&got, &want, name, tol, &mut found);
        if !found.is_empty() {
            failures.push(format!("{name}:\n{}", found.join("\n")));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
