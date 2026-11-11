//! Tracked random offset corpus (`rust/tests/parity/offset_corpus.json`, made by
//! `dev/parity/make_offset_corpus.py`). Compares Rust `ops::offset` to 1.x with the same
//! rules as `tests/sketch.rs`. Every case must match 1.x: any mismatch fails the test. The
//! corpus keeps a `baseline` of 0 mismatches per group (the Python script preserves it).

use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};
use stl_sketch::Entity;
use stl_sketch::ops;

fn parity_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity")
}

fn read(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn entities(v: &Value) -> Vec<Entity> {
    v.as_array().cloned().unwrap_or_default().into_iter().map(|e| serde_json::from_value(e).unwrap()).collect()
}

fn ids(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect()
}

fn new_id_factory() -> impl FnMut() -> String {
    let mut n = 0;
    move || {
        n += 1;
        format!("n{n}")
    }
}

/// Compare two JSON values: numbers within `tol`, strings and shapes exactly.
/// A missing `construction` key counts as `false` (our entities leave it out when false).
fn same(path: &str, want: &Value, got: &Value, tol: f64, out: &mut Vec<String>) {
    match (want, got) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            if (a - b).abs() > tol {
                out.push(format!("{path}: want {a}, got {b}"));
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                out.push(format!("{path}: want {} items, got {}", a.len(), b.len()));
                return;
            }
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                same(&format!("{path}[{i}]"), x, y, tol, out);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            let tol = if a.get("type").and_then(Value::as_str) == Some("arc") { tol.max(1e-6) } else { tol };
            let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                let (x, y) = (a.get(k), b.get(k));
                let falsy = Some(&Value::Bool(false));
                match (x, y) {
                    (Some(x), Some(y)) => same(&format!("{path}.{k}"), x, y, tol, out),
                    (Some(v), None) | (None, Some(v)) if k == "construction" && Some(v) == falsy => {}
                    _ => out.push(format!("{path}.{k}: present on one side only (want {x:?}, got {y:?})")),
                }
            }
        }
        (a, b) => {
            if a != b {
                out.push(format!("{path}: want {a}, got {b}"));
            }
        }
    }
}

fn run_offset(ents: &[Entity], id_list: &[String], d: f64) -> Value {
    let mut nid = new_id_factory();
    match ops::offset(ents, id_list, d, &mut nid) {
        Ok(r) => serde_json::to_value(r).unwrap(),
        Err(e) => json!({ "error": e.message() }),
    }
}

fn is_error(v: &Value) -> bool {
    v.as_object().is_some_and(|o| o.contains_key("error"))
}

fn piece_count(v: &Value) -> Option<usize> {
    v.as_array().map(|a| a.len())
}

/// Largest absolute difference between corresponding numbers in `want` and `got`.
fn max_coord_delta(want: &Value, got: &Value, out: &mut f64) {
    match (want, got) {
        (Value::Number(a), Value::Number(b)) => {
            let d = (a.as_f64().unwrap() - b.as_f64().unwrap()).abs();
            if d > *out {
                *out = d;
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            for (x, y) in a.iter().zip(b) {
                max_coord_delta(x, y, out);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            for k in a.keys() {
                if let (Some(x), Some(y)) = (a.get(k), b.get(k)) {
                    max_coord_delta(x, y, out);
                }
            }
        }
        _ => {}
    }
}

/// Every group the corpus must have, with its case count.
const GROUPS: [(&str, u64); 5] = [
    ("open_lines", 300),
    ("messy_large", 300),
    ("closed_polylines", 300),
    ("closed_self_crossing", 100),
    ("open_curves", 150),
];

#[test]
fn offset_corpus_within_baseline() {
    let path = parity_dir().join("offset_corpus.json");
    assert!(path.is_file(), "missing {}; run make_offset_corpus.py", path.display());
    let corpus = read(&path);
    let groups = corpus["groups"].as_object().expect("groups object");
    let baseline = corpus.get("baseline").and_then(Value::as_object).cloned().unwrap_or_default();
    let mut measured = Map::new();
    let mut problems: Vec<String> = vec![];

    for key in groups.keys() {
        assert!(GROUPS.iter().any(|(n, _)| n == key), "corpus has group {key} the test does not know");
    }

    for (name, cases_per_group) in GROUPS {
        let cases = groups.get(name).and_then(Value::as_array).unwrap_or_else(|| panic!("missing group {name}"));
        assert_eq!(
            cases.len() as u64,
            cases_per_group,
            "{name}: case count {} != cases_per_group {cases_per_group}",
            cases.len()
        );
        let mut slowest = (0.0_f64, 0usize);
        let mut mismatches = 0u64;
        let mut piece_count_diffs = 0u64;
        let mut error_diffs = 0u64;
        let mut max_same_piece_delta = 0.0_f64;
        let mut same_piece_mismatches = 0u64;
        for (i, case) in cases.iter().enumerate() {
            let ents = entities(&case["entities"]);
            let id_list = ids(&case["ids"]);
            let d = case["d"].as_f64().unwrap();
            let t0 = std::time::Instant::now();
            let got = run_offset(&ents, &id_list, d);
            let took = t0.elapsed().as_secs_f64() * 1000.0;
            if took > slowest.0 {
                slowest = (took, i);
            }
            let want = &case["expected"];
            let mut diffs = vec![];
            same(&format!("{name}[{i}]"), want, &got, 1e-9, &mut diffs);
            if diffs.is_empty() {
                continue;
            }
            mismatches += 1;
            println!("{name}[{i}] differs: {}", diffs.iter().take(3).cloned().collect::<Vec<_>>().join(" | "));
            let want_err = is_error(want);
            let got_err = is_error(&got);
            if want_err != got_err {
                error_diffs += 1;
            } else if want_err && got_err {
                // both sides errored with different messages
                error_diffs += 1;
            } else if let (Some(a), Some(b)) = (piece_count(want), piece_count(&got)) {
                if a != b {
                    piece_count_diffs += 1;
                } else {
                    same_piece_mismatches += 1;
                    max_coord_delta(want, &got, &mut max_same_piece_delta);
                }
            } else {
                // unexpected shapes
                piece_count_diffs += 1;
            }
        }
        let total = cases.len() as u64;
        let rate = if total == 0 { 0.0 } else { mismatches as f64 / total as f64 };
        println!("{name}: {mismatches}/{total} mismatches ({:.2}%)", rate * 100.0);
        if same_piece_mismatches > 0 {
            println!(
                "{name}: largest same-piece-count coordinate difference {max_same_piece_delta:.6e} ({same_piece_mismatches} mismatches)"
            );
        } else {
            println!("{name}: no same-piece-count coordinate mismatches");
        }
        println!(
            "{name}: piece-count differences {piece_count_diffs}, error/no-error differences {error_diffs}"
        );
        println!("{name}: slowest case [{}] took {:.1} ms", slowest.1, slowest.0);
        measured.insert(name.to_string(), json!(mismatches));

        if mismatches > 0 {
            problems.push(format!("{name}: {mismatches} of {total} cases differ from 1.x"));
        }
        if baseline.get(name).and_then(Value::as_u64) != Some(0) {
            problems.push(format!("{name}: the corpus baseline must be 0"));
        }
    }

    println!("measured baseline: {}", Value::Object(measured));
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
