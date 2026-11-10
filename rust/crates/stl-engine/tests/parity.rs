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

/// Run one case against `registry` and compare it with its golden.
fn run_case(registry: &Registry, case: &Value, golden: &Value) -> Outcome {
    let features = case["features"].as_array().cloned().unwrap_or_default();
    if features.iter().any(|f| !registry.has(f["type"].as_str().unwrap_or(""))) {
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

/// Run every case of `types`: (passed, pending, failed case names with their mismatches).
fn run_all(registry: &Registry, types: &[&str]) -> (usize, usize, Vec<String>) {
    let (mut passed, mut pending, mut failures) = (0, 0, Vec::new());
    for t in types {
        let cases = read_json(&parity_dir().join("features").join(format!("{t}.json")));
        let golden = read_json(&parity_dir().join("features_golden").join(format!("{t}.json")));
        for case in cases.as_array().expect("case file is a list") {
            let name = case["name"].as_str().unwrap_or("?");
            let Some(g) = golden.get(name) else {
                failures.push(format!("{t}/{name}: no golden"));
                continue;
            };
            match run_case(registry, case, g) {
                Outcome::Passed => passed += 1,
                Outcome::Pending => pending += 1,
                Outcome::Failed(why) => failures.extend(why.into_iter().map(|m| format!("{t}/{name}: {m}"))),
            }
        }
    }
    (passed, pending, failures)
}

fn failed_cases(failures: &[String]) -> usize {
    let mut names: Vec<&str> = failures.iter().filter_map(|f| f.split(": ").next()).collect();
    names.dedup();
    names.len()
}

#[test]
fn feature_parity() {
    let (passed, pending, failures) = run_all(&Registry::builtin(), &BATCH1);
    for f in &failures {
        println!("{f}");
    }
    println!("parity: {passed} passed, {pending} pending, {} failed", failed_cases(&failures));
    assert!(failures.is_empty(), "{} parity mismatches", failures.len());
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
