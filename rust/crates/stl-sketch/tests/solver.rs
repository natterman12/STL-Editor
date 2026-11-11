//! The solver against 1.x `constraints.js` (rust/tests/parity/solver_golden.json).
//! Rule: converged coordinates within 1e-6 mm, identical analyze and unsatisfied results.

use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use stl_sketch::solver::{
    analyze, glyph_spots, infer_dimension, is_redundant, line_meet, make_constraint, measure, unsatisfied,
    unsatisfied_ids, Constraint, Lock, Outcome, Ref,
};
use stl_sketch::Entity;

fn parity() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity")
}

fn load(p: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn list<T: serde::de::DeserializeOwned>(v: &Value, key: &str) -> Vec<T> {
    match v.get(key) {
        Some(a) => serde_json::from_value(a.clone()).unwrap(),
        None => Vec::new(),
    }
}

/// Compare JSON, numbers within `tol`.
fn close(a: &Value, b: &Value, tol: f64, path: &str) -> Result<(), String> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            if (x - y).abs() <= tol {
                Ok(())
            } else {
                Err(format!("{path}: {x} vs {y}"))
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return Err(format!("{path}: length {} vs {}", x.len(), y.len()));
            }
            x.iter().zip(y).enumerate().try_for_each(|(i, (p, q))| close(p, q, tol, &format!("{path}[{i}]")))
        }
        (Value::Object(x), Value::Object(y)) => {
            let kx: BTreeSet<_> = x.keys().collect();
            let ky: BTreeSet<_> = y.keys().collect();
            if kx != ky {
                return Err(format!("{path}: keys {kx:?} vs {ky:?}"));
            }
            x.iter().try_for_each(|(k, v)| close(v, &y[k], tol, &format!("{path}.{k}")))
        }
        _ if a == b => Ok(()),
        _ => Err(format!("{path}: {a} vs {b}")),
    }
}

struct Scene {
    ents: Vec<Entity>,
    cons: Vec<Constraint>,
    locks: Vec<Lock>,
    fixed: Vec<Entity>,
    fixed_ids: BTreeSet<String>,
    raw: Value,
}

fn scene(name: &str) -> Scene {
    let raw = load(&parity().join("solver").join(format!("{name}.json")));
    Scene {
        ents: list(&raw, "entities"),
        cons: list(&raw, "constraints"),
        locks: list(&raw, "locks"),
        fixed: list(&raw, "fixed"),
        fixed_ids: list::<String>(&raw, "fixed_ids").into_iter().collect(),
        raw,
    }
}

fn to_json<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap()
}

/// `{result: msg|null}` for a message or a wait, the constraint itself otherwise (the golden's shape).
fn wrap(o: &Outcome) -> Value {
    match o {
        Outcome::Constraint(c) => to_json(c),
        Outcome::Message(m) => json!({ "result": m }),
        Outcome::Pending => json!({ "result": null }),
    }
}

fn has_null(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Array(a) => a.iter().any(has_null),
        Value::Object(o) => o.values().any(has_null),
        _ => false,
    }
}

const DIMS: [&str; 7] = ["distance", "hdistance", "vdistance", "length", "radius", "diameter", "angle"];

#[test]
fn solver_matches_golden() {
    let golden = load(&parity().join("solver_golden.json"));
    let mut n = 0;
    for (name, want) in golden.as_object().unwrap() {
        let s = scene(name);
        let rep = stl_sketch::solver::solve(&s.ents, &s.cons, &s.locks, &s.fixed);
        // converged coordinates within 1e-6
        for (id, w) in want["solved"].as_object().unwrap() {
            let got = rep.entities.iter().find(|e| e.id() == Some(id.as_str())).unwrap_or_else(|| panic!("{name}: {id}"));
            close(&to_json(got), w, 1e-6, &format!("{name}.solved.{id}")).unwrap();
        }
        assert_eq!(rep.ok, want["ok"].as_bool().unwrap(), "{name}: ok");
        close(&json!(rep.error), &want["error"], 1e-6, &format!("{name}.error")).unwrap();
        // analyze on the solved sketch: identical
        let an = analyze(&rep.entities, &s.cons, &s.fixed);
        assert_eq!(an.dof as u64, want["analyze"]["dof"].as_u64().unwrap(), "{name}: dof");
        assert_eq!(an.rank as u64, want["analyze"]["rank"].as_u64().unwrap(), "{name}: rank");
        let under: Vec<&str> = an.under.iter().map(String::as_str).collect();
        let want_under: Vec<&str> = want["analyze"]["under"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(under, want_under, "{name}: under");
        // unsatisfied: identical
        let uns = unsatisfied(&rep.entities, &s.cons, &s.fixed);
        close(&to_json(&uns), &want["unsatisfied"], 1e-8, &format!("{name}.unsatisfied")).unwrap();
        // inferDimension picks
        let mut inferred = Vec::new();
        for inf in s.raw["infer"].as_array().cloned().unwrap_or_default() {
            let picks: Vec<Ref> = serde_json::from_value(inf["picks"].clone()).unwrap();
            let cursor = inf.get("cursor").and_then(|c| serde_json::from_value::<[f64; 2]>(c.clone()).ok());
            let fx: BTreeSet<String> = match inf.get("fixed") {
                Some(f) => serde_json::from_value(f.clone()).unwrap(),
                None => s.fixed_ids.clone(),
            };
            inferred.push(wrap(&infer_dimension(&picks, &rep.entities, cursor, &fx)));
        }
        close(&Value::Array(inferred), &want["infer"], 1e-8, &format!("{name}.infer")).unwrap();
        // every solved coordinate is finite
        for e in &rep.entities {
            assert!(!has_null(&to_json(e)), "{name}: non-finite coordinate in {}", to_json(e));
        }
        // makeConstraint: constraints and messages verbatim
        if let Some(makes) = s.raw.get("make").and_then(Value::as_array) {
            let got: Vec<Value> = makes
                .iter()
                .map(|m| {
                    let picks: Vec<Ref> = serde_json::from_value(m["picks"].clone()).unwrap();
                    wrap(&make_constraint(m["tool"].as_str().unwrap(), &picks, &rep.entities))
                })
                .collect();
            close(&Value::Array(got), &want["make"], 1e-8, &format!("{name}.make")).unwrap();
        } else {
            assert!(want.get("make").is_none(), "{name}: golden has make, scene has none");
        }
        // measure of each dimension
        if s.raw["measure"].as_bool().unwrap_or(false) {
            let got: Vec<Value> = s
                .cons
                .iter()
                .filter(|c| DIMS.contains(&c.kind.as_str()))
                .map(|c| {
                    let mut o = json!({ "type": c.kind, "value": measure(c, &rep.entities) });
                    if let Some(id) = &c.id {
                        o["id"] = id.clone();
                    }
                    if c.has_ref() {
                        o["ref"] = json!(true);
                    }
                    o
                })
                .collect();
            close(&Value::Array(got), &want["measure"], 1e-8, &format!("{name}.measure")).unwrap();
        }
        // glyph spots of every constraint
        if s.raw["glyphSpots"].as_bool().unwrap_or(false) {
            let got: Vec<Value> = s
                .cons
                .iter()
                .map(|c| {
                    let mut o = json!({ "type": c.kind, "spots": glyph_spots(c, &rep.entities) });
                    if let Some(id) = &c.id {
                        o["id"] = id.clone();
                    }
                    if c.has_ref() {
                        o["ref"] = json!(true);
                    }
                    o
                })
                .collect();
            close(&Value::Array(got), &want["glyphSpots"], 1e-8, &format!("{name}.glyphSpots")).unwrap();
        }
        // lineMeet of entity pairs
        if let Some(pairs) = s.raw.get("lineMeet").and_then(Value::as_array) {
            let got: Vec<Value> = pairs
                .iter()
                .map(|p| {
                    let (a, b) = (p["a"].as_str().unwrap(), p["b"].as_str().unwrap());
                    let find = |id: &str| rep.entities.iter().find(|e| e.id() == Some(id));
                    let at = match (find(a), find(b)) {
                        (Some(x), Some(y)) => line_meet(x, y),
                        _ => None,
                    };
                    json!({ "a": a, "b": b, "at": at })
                })
                .collect();
            close(&Value::Array(got), &want["lineMeet"], 1e-8, &format!("{name}.lineMeet")).unwrap();
        }
        // constraints with ref:true are carried through unchanged
        if s.raw["ref"].as_bool().unwrap_or(false) {
            let got: Vec<Value> = s.cons.iter().filter(|c| c.has_ref()).map(to_json).collect();
            close(&Value::Array(got), &want["ref"], 1e-8, &format!("{name}.ref")).unwrap();
        }
        // redundant
        for r in want.get("redundant").and_then(Value::as_array).cloned().unwrap_or_default() {
            let c: Constraint = serde_json::from_value(r["constraint"].clone()).unwrap();
            let got = is_redundant(&rep.entities, &s.cons, &c, &s.fixed);
            assert_eq!(got, r["redundant"].as_bool().unwrap(), "{name}: redundant");
        }
        n += 1;
    }
    // Every scene file has a golden entry and the other way round: no scene is skipped.
    let mut files: Vec<String> = std::fs::read_dir(parity().join("solver"))
        .unwrap()
        .filter_map(|e| e.ok()?.path().file_stem().map(|s| s.to_string_lossy().into_owned()))
        .collect();
    files.sort();
    let names: Vec<String> = golden.as_object().unwrap().keys().cloned().collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(files, sorted, "scene files and golden keys differ");
    assert_eq!(n, names.len());
    assert!(n >= 11, "only {n} scenes");
    println!("solver golden: {n} scenes checked");
}

#[test]
fn over_constrained_conflict() {
    let s = scene("over_constrained_conflict");
    let rep = stl_sketch::solver::solve(&s.ents, &s.cons, &s.locks, &s.fixed);
    assert!(!rep.ok);
    for e in &rep.entities {
        let t = serde_json::to_string(e).unwrap();
        assert!(!t.contains("null"), "NaN coordinate: {t}");
    }
    assert!(rep.error.is_finite());
    assert_eq!(unsatisfied_ids(&rep.entities, &s.cons, &s.fixed), vec!["c1", "c2"]);
}
