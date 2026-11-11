//! Runs the feature cases in `rust/tests/parity/features/<type>.json` through `Document` and
//! compares them with the 1.x goldens in `features_golden/`. Mirrors `dev/parity/make_golden.py`:
//! sources are added in sorted case-key order (s1, s2, ...), features get ids f1, f2, ...,
//! then the document is evaluated.
//!
//! A case whose feature types aren't all in the registry yet is *pending* (milestone 2 fills
//! the registry); any other mismatch is a failure.

use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use stl_engine::params::Param;
use stl_engine::{Body, Ctx, Document, Feature, FeatureError, FeatureResult, Registry, State};
use stl_kernel::Solid;
use stl_parity::{Exactness, MeshStats, compare_mesh, mesh_stats};

const BATCH1: [&str; 11] = [
    "import", "primitive", "plane", "move", "scale", "mirror", "combine", "split", "delete", "separate", "repair",
];

const BATCH2: [&str; 7] = [
    "sketch", "extrude", "revolve", "loft", "sweep", "hole", "pushpull",
];

const BATCH3: [&str; 8] = [
    "simplify", "remesh", "sculpt", "offset", "hollow", "shell", "lattice", "thincheck",
];

#[derive(Debug, PartialEq)]
enum Outcome {
    Passed,
    Pending,
    Failed(Vec<String>),
}

fn parity_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity")
}

fn read_json(path: &std::path::Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn body_stats(b: &Body) -> MeshStats {
    let m = match (&b.solid, &b.mesh) {
        (Some(s), _) => s.to_mesh(),
        (None, Some(m)) => (**m).clone(),
        (None, None) => Default::default(),
    };
    mesh_stats(&m.positions, &m.triangles)
}

fn requires_pending(registry: &Registry, case: &Value, golden: &Value) -> bool {
    let caps = registry.capabilities();
    for src in [case, golden] {
        if let Some(reqs) = src.get("requires").and_then(Value::as_array) {
            for r in reqs {
                let name = r.as_str().unwrap_or("");
                if !caps.contains(name) {
                    return true;
                }
            }
        }
    }
    false
}

/// Thin-wall Check record: count exact, each spot within 0.01, min within 0.001.
fn compare_thin(gid: &str, g: Option<&Value>, got: Option<&Value>, bad: &mut Vec<String>) {
    let (g, got) = match (g, got) {
        (None, None) => return,
        (Some(g), Some(got)) => (g, got),
        (g, got) => {
            bad.push(format!("{gid} thin: golden {g:?} got {got:?}"));
            return;
        }
    };
    if g["count"] != got["count"] {
        bad.push(format!("{gid} thin count: golden {} got {}", g["count"], got["count"]));
    }
    match (g["min"].as_f64(), got["min"].as_f64()) {
        (None, None) => {}
        (Some(a), Some(b)) if (a - b).abs() <= 0.001 => {}
        (a, b) => bad.push(format!("{gid} thin min: golden {a:?} got {b:?}")),
    }
    let (gs, rs) = (g["spots"].as_array().cloned().unwrap_or_default(), got["spots"].as_array().cloned().unwrap_or_default());
    if gs.len() != rs.len() {
        bad.push(format!("{gid} thin spots: golden {} got {}", gs.len(), rs.len()));
        return;
    }
    for (i, (a, b)) in gs.iter().zip(&rs).enumerate() {
        for k in 0..3 {
            let (x, y) = (a[k].as_f64().unwrap_or(f64::NAN), b[k].as_f64().unwrap_or(f64::NAN));
            if !((x - y).abs() <= 0.01) {
                bad.push(format!("{gid} thin spot {i}[{k}]: golden {x} got {y}"));
            }
        }
    }
}

/// Run one case against `registry` and compare it with its golden.
fn run_case(registry: &Registry, case: &Value, golden: &Value) -> Outcome {
    let features = case["features"].as_array().cloned().unwrap_or_default();
    if features.iter().any(|f| !registry.has(f["type"].as_str().unwrap_or(""))) {
        return Outcome::Pending;
    }
    if requires_pending(registry, case, golden) {
        return Outcome::Pending;
    }
    let exactness = match case["exactness"].as_str() {
        Some("exact") => Exactness::Exact,
        _ => Exactness::Approx,
    };

    let mut doc = Document::new(registry.clone());
    let mut mapping: BTreeMap<String, String> = BTreeMap::new();
    if let Some(sources) = case["sources"].as_object() {
        let mut keys: Vec<&String> = sources.keys().collect();
        keys.sort();
        for key in keys {
            let file = sources[key].as_str().unwrap_or_default();
            let data = std::fs::read(parity_dir().join("../models").join(file))
                .unwrap_or_else(|e| panic!("model {file}: {e}"));
            mapping.insert(key.clone(), doc.add_source(file, data));
        }
    }
    doc.params = case["params"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|q| Param {
            name: q["name"].as_str().unwrap_or_default().to_string(),
            expr: q["expr"].as_str().unwrap_or_default().to_string(),
            note: q["note"].as_str().unwrap_or_default().to_string(),
        })
        .collect();
    doc.features = features
        .iter()
        .enumerate()
        .map(|(i, raw)| {
            let mut params: Map<String, Value> = raw["params"].as_object().cloned().unwrap_or_default();
            if let Some(Value::String(s)) = params.get("source") {
                if let Some(real) = mapping.get(s) {
                    params.insert("source".into(), Value::String(real.clone()));
                }
            }
            Feature {
                id: format!("f{}", i + 1),
                kind: raw["type"].as_str().unwrap_or_default().to_string(),
                name: raw["name"].as_str().filter(|n| !n.is_empty()).map(str::to_string),
                params,
                suppressed: false,
                extra: Map::new(),
            }
        })
        .collect();

    let results = doc.evaluate();
    let mut bad: Vec<String> = Vec::new();

    let gres = golden["results"].as_array().cloned().unwrap_or_default();
    if gres.len() != results.len() {
        bad.push(format!("results: golden {} got {}", gres.len(), results.len()));
    }
    for (g, r) in gres.iter().zip(&results) {
        let gid = g["id"].as_str().unwrap_or_default();
        if gid != r.id {
            bad.push(format!("result id: golden {gid} got {}", r.id));
        }
        for (what, gv, rv) in [
            ("error", g["error"].as_str(), r.error.as_deref()),
            ("warning", g["warning"].as_str(), r.warning.as_deref()),
        ] {
            if gv != rv {
                bad.push(format!("{gid} {what}: golden {gv:?} got {rv:?}"));
            }
        }
        let ginfo = g["info"].as_str();
        if ginfo != r.info.as_deref() {
            bad.push(format!("{gid} info: golden {ginfo:?} got {:?}", r.info));
        }
        compare_thin(gid, g.get("thin").filter(|v| !v.is_null()), r.thin.as_ref(), &mut bad);
    }

    let gbodies = golden["bodies"].as_array().cloned().unwrap_or_default();
    let got: Vec<(&String, &Body)> = doc.state.bodies.iter().collect();
    if gbodies.len() != got.len() {
        let ids: Vec<&String> = got.iter().map(|(k, _)| *k).collect();
        bad.push(format!("bodies: golden {} got {} {ids:?}", gbodies.len(), got.len()));
    }
    for (g, (id, b)) in gbodies.iter().zip(&got) {
        let gid = g["id"].as_str().unwrap_or_default();
        if gid != id.as_str() {
            bad.push(format!("body id: golden {gid} got {id}"));
            continue;
        }
        let gname = g["name"].as_str().unwrap_or_default();
        if gname != b.name {
            bad.push(format!("{gid} name: golden {gname:?} got {:?}", b.name));
        }
        let gsolid = g["solid"].as_bool().unwrap_or(false);
        if gsolid != b.solid.is_some() {
            bad.push(format!("{gid} solid: golden {gsolid} got {}", b.solid.is_some()));
        }
        let gstats: MeshStats =
            serde_json::from_value(g["stats"].clone()).unwrap_or_else(|e| panic!("golden stats of body {gid}: {e}"));
        for m in compare_mesh(&gstats, &body_stats(b), exactness) {
            bad.push(format!("{gid} {m}"));
        }
    }
    if bad.is_empty() { Outcome::Passed } else { Outcome::Failed(bad) }
}

/// The cases where 2.0 differs from 1.x on purpose: `"<type>/<case>"` -> reason and 2.0's values.
/// The `expect` record has the same shape as a golden record. See `tests/EXCEPTIONS.md`.
fn read_exceptions() -> Map<String, Value> {
    let v = read_json(&parity_dir().join("exceptions.json"));
    v.as_object().cloned().expect("exceptions.json is an object")
}

/// Count of cases that passed because their exception matched.
struct Tally {
    passed: usize,
    by_exception: usize,
    pending: usize,
    failures: Vec<String>,
}

/// Run one case, honouring an exception for it. A case with an exception must NOT match the
/// 1.x golden (that would make the exception stale) and must match the exception's `expect`.
fn run_with_exception(registry: &Registry, case: &Value, golden: &Value, exc: Option<&Value>) -> (Outcome, bool) {
    let Some(exc) = exc else {
        return (run_case(registry, case, golden), false);
    };
    match run_case(registry, case, golden) {
        Outcome::Pending => return (Outcome::Pending, false),
        Outcome::Passed => {
            return (
                Outcome::Failed(vec!["stale exception: the case now matches the 1.x golden; remove it from exceptions.json".into()]),
                false,
            );
        }
        Outcome::Failed(_) => {}
    }
    if exc.get("reason").and_then(Value::as_str).is_none_or(str::is_empty) {
        return (Outcome::Failed(vec!["exception has no reason".into()]), false);
    }
    let Some(expect) = exc.get("expect") else {
        return (Outcome::Failed(vec!["exception has no expect".into()]), false);
    };
    match run_case(registry, case, expect) {
        Outcome::Passed => (Outcome::Passed, true),
        Outcome::Pending => (Outcome::Pending, false),
        Outcome::Failed(why) => (
            Outcome::Failed(why.into_iter().map(|m| format!("(vs exception expect) {m}")).collect()),
            false,
        ),
    }
}

/// Run every case of `types` against `exceptions`.
fn run_all_with(registry: &Registry, types: &[&str], exceptions: &Map<String, Value>) -> Tally {
    let mut tally = Tally { passed: 0, by_exception: 0, pending: 0, failures: Vec::new() };
    let mut seen: Vec<String> = Vec::new();
    for t in types {
        let cases = read_json(&parity_dir().join("features").join(format!("{t}.json")));
        let golden = read_json(&parity_dir().join("features_golden").join(format!("{t}.json")));
        for case in cases.as_array().expect("case file is a list") {
            let name = case["name"].as_str().unwrap_or("?");
            let key = format!("{t}/{name}");
            let Some(g) = golden.get(name) else {
                tally.failures.push(format!("{key}: no golden"));
                continue;
            };
            let exc = exceptions.get(&key);
            if exc.is_some() {
                seen.push(key.clone());
            }
            match run_with_exception(registry, case, g, exc) {
                (Outcome::Passed, by_exc) => {
                    tally.passed += 1;
                    tally.by_exception += usize::from(by_exc);
                }
                (Outcome::Pending, _) => tally.pending += 1,
                (Outcome::Failed(why), _) => {
                    tally.failures.extend(why.into_iter().map(|m| format!("{key}: {m}")));
                }
            }
        }
    }
    for key in exceptions.keys() {
        if !seen.contains(key) {
            tally.failures.push(format!("{key}: exceptions.json names a case or golden that no longer exists"));
        }
    }
    tally
}

fn failed_cases(failures: &[String]) -> usize {
    let mut names: Vec<&str> = failures.iter().filter_map(|f| f.split(": ").next()).collect();
    names.dedup();
    names.len()
}

#[test]
fn feature_parity() {
    let mut types = Vec::from(BATCH1);
    types.extend_from_slice(&BATCH2);
    types.extend_from_slice(&BATCH3);
    let tally = run_all_with(&Registry::builtin(), &types, &read_exceptions());
    for f in &tally.failures {
        println!("{f}");
    }
    println!(
        "parity: {} passed ({} by exception), {} pending, {} failed",
        tally.passed,
        tally.by_exception,
        tally.pending,
        failed_cases(&tally.failures)
    );
    assert!(tally.failures.is_empty(), "{} parity mismatches", tally.failures.len());
}

// -- the non-pending path, proved with a test-only `primitive` (boxes only) --------------------

fn num(p: &Map<String, Value>, k: &str) -> f64 {
    p.get(k).and_then(Value::as_f64).unwrap_or(0.0)
}

fn ev_box(
    st: &mut State,
    fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _: &Ctx,
) -> Result<(), FeatureError> {
    if p.get("shape").and_then(Value::as_str) != Some("box") {
        return Err(FeatureError("test-only primitive: boxes only".into()));
    }
    let (l, w, h) = (num(p, "length"), num(p, "width"), num(p, "height"));
    let off: Vec<f64> = p
        .get("offset")
        .and_then(Value::as_array)
        .map_or(vec![0.0; 3], |a| a.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect());
    // centred in XY, standing on its base, then moved by `offset`
    let solid = Solid::cube([l, w, h], true).translate([off[0], off[1], off[2] + h / 2.0]);
    let name = p.get("body_name").and_then(Value::as_str).unwrap_or("Box").to_string();
    st.bodies.insert(fid.to_string(), Body { name, solid: Some(solid), mesh: None });
    res.created.push(fid.to_string());
    Ok(())
}

fn primitive_case(name: &str) -> (Value, Value) {
    let cases = read_json(&parity_dir().join("features/primitive.json"));
    let golden = read_json(&parity_dir().join("features_golden/primitive.json"));
    let case = cases.as_array().unwrap().iter().find(|c| c["name"] == name).expect("case").clone();
    (case, golden[name].clone())
}

#[test]
fn runner_passes_a_case_with_a_registered_feature() {
    let reg = Registry::new().with("primitive", ev_box);
    for name in ["primitive_box", "primitive_box_expr_size"] {
        let (case, golden) = primitive_case(name);
        assert_eq!(run_case(&reg, &case, &golden), Outcome::Passed, "{name}");
    }
}

#[test]
fn runner_reports_a_mismatch() {
    let reg = Registry::new().with("primitive", ev_box);
    let (case, _) = primitive_case("primitive_box");
    let (_, other) = primitive_case("primitive_box_expr_size");
    assert!(matches!(run_case(&reg, &case, &other), Outcome::Failed(_)));
}

// -- the exception mechanism -------------------------------------------------------------------

fn exception_for(name: &str, expect: Value) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert(format!("primitive/{name}"), serde_json::json!({ "reason": "test", "expect": expect }));
    m
}

fn only_primitive(reg: &Registry, exc: &Map<String, Value>) -> Tally {
    run_all_with(reg, &["primitive"], exc)
}

#[test]
fn exception_is_used_instead_of_the_golden() {
    // The test-only primitive refuses a cylinder, so the 1.x golden does not match; the
    // exception records what this build gives instead.
    let reg = Registry::new().with("primitive", ev_box);
    let (_, mut expect) = primitive_case("primitive_cylinder");
    expect["results"][0]["error"] = Value::String("test-only primitive: boxes only".into());
    expect["bodies"] = Value::Array(Vec::new());
    let mut exc = Map::new();
    exc.insert(
        "primitive/primitive_cylinder".into(),
        serde_json::json!({ "reason": "test", "expect": expect }),
    );
    let t = only_primitive(&reg, &exc);
    assert_eq!(t.by_exception, 1);
    assert!(t.failures.iter().all(|f| !f.starts_with("primitive/primitive_cylinder:")), "{:?}", t.failures);
}

#[test]
fn stale_exception_fails() {
    let reg = Registry::new().with("primitive", ev_box);
    let (_, same) = primitive_case("primitive_box");
    let t = only_primitive(&reg, &exception_for("primitive_box", same));
    assert!(t.failures.iter().any(|f| f.contains("stale exception")), "{:?}", t.failures);
}

#[test]
fn exception_for_a_missing_case_fails() {
    let reg = Registry::new().with("primitive", ev_box);
    let (_, same) = primitive_case("primitive_box");
    let t = only_primitive(&reg, &exception_for("no_such_case", same));
    assert!(t.failures.iter().any(|f| f.contains("no longer exists")), "{:?}", t.failures);
}
