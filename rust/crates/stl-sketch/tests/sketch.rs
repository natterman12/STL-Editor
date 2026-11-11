//! 1.x `stleditor/sketch.py` golden (`rust/tests/parity/sketch_golden.json`, made by
//! `dev/parity/make_sketch_golden.py`): profile faces and the sketch operations.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use stl_sketch::Entity;
use stl_sketch::ops;
use stl_sketch::profile::profile_with;

fn parity_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity")
}

fn read(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn entities(v: &Value) -> Vec<Entity> {
    v.as_array().cloned().unwrap_or_default().into_iter().map(|e| serde_json::from_value(e).unwrap()).collect()
}

fn id_of(spec: &Value) -> String {
    spec.as_str().unwrap_or_else(|| spec["id"].as_str().unwrap()).to_string()
}

fn pt(v: &Value) -> [f64; 2] {
    [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()]
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

fn ents_json(es: &[Entity]) -> Value {
    serde_json::to_value(es).unwrap()
}

fn run_op(name: &str, spec: &Value, ents: &[Entity]) -> Value {
    let err = |e: stl_sketch::SketchError| json!({ "error": e.message() });
    match name {
        "explode" => {
            let id = id_of(spec);
            let target = ents.iter().find(|e| e.id() == Some(id.as_str())).unwrap();
            ents_json(&ops::explode(target))
        }
        "trim" => match ops::trim(ents, spec["id"].as_str().unwrap(), pt(&spec["point"])) {
            Ok(r) => ents_json(&r),
            Err(e) => err(e),
        },
        "offset" => {
            // a list of specs is a list of results (1.x golden: chain order, open errors)
            let one = |spec: &Value| {
                let mut nid = new_id_factory();
                match ops::offset(ents, &ids(&spec["ids"]), spec["d"].as_f64().unwrap(), &mut nid) {
                    Ok(r) => ents_json(&r),
                    Err(e) => err(e),
                }
            };
            match spec.as_array() {
                Some(list) => Value::Array(list.iter().map(one).collect()),
                None => one(spec),
            }
        }
        "mirror" => {
            let mut nid = new_id_factory();
            match ops::mirror(ents, &ids(&spec["ids"]), spec["axis"].as_str().unwrap(), &mut nid) {
                Ok(r) => ents_json(&r),
                Err(e) => err(e),
            }
        }
        "find_axis" => match ops::find_axis(ents, &id_of(spec)) {
            Ok((p0, d)) => json!({ "origin": p0, "direction": d }),
            Err(e) => err(e),
        },
        "snap_points" => json!(ops::snap_points(ents)),
        other => panic!("unknown op {other}"),
    }
}

/// Compare two JSON values: numbers within `tol` (1e-6 inside arcs), strings and shapes exactly.
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

/// `polygonize` key: per face `[exterior, *holes]`, each ring closed, as shapely returns it.
fn polygonize_json(ents: &[Entity], only: Option<&[String]>) -> Value {
    let close = |r: &[[f64; 2]]| {
        let mut v: Vec<[f64; 2]> = r.to_vec();
        if let Some(f) = r.first() {
            v.push(*f);
        }
        json!(v)
    };
    Value::Array(
        stl_sketch::profile::faces(ents, only)
            .iter()
            .map(|f| {
                let mut rings = vec![close(&f.exterior)];
                rings.extend(f.holes.iter().map(|h| close(h)));
                Value::Array(rings)
            })
            .collect(),
    )
}

#[test]
fn sketch_matches_golden() {
    let golden = read(&parity_dir().join("sketch_golden.json"));
    let mut problems: Vec<String> = vec![];
    let mut cases = 0;
    for (name, want) in golden.as_object().unwrap() {
        let case = read(&parity_dir().join(format!("sketch/{name}.json")));
        let ents = entities(&case["entities"]);
        let only = case.get("only").filter(|v| !v.is_null()).map(ids);
        let regions: Option<Vec<[f64; 2]>> =
            case.get("regions").filter(|v| !v.is_null()).map(|r| r.as_array().unwrap().iter().map(pt).collect());
        let mut got = serde_json::Map::new();
        got.insert("polygonize".into(), polygonize_json(&ents, only.as_deref()));
        match profile_with(&ents, only.as_deref(), regions.as_deref()) {
            Ok(shape) => {
                got.insert("faces".into(), json!(shape.contours));
            }
            Err(e) => {
                got.insert("error".into(), json!(e.message()));
            }
        }
        // 1.x records the ops whether or not the profile worked
        if let Some(ops_spec) = case.get("ops").and_then(Value::as_object) {
            let mut res = serde_json::Map::new();
            for (op, spec) in ops_spec {
                res.insert(op.clone(), run_op(op, spec, &ents));
            }
            got.insert("ops".into(), Value::Object(res));
        }
        let mut diffs = vec![];
        same(name, want, &Value::Object(got), 1e-9, &mut diffs);
        problems.extend(diffs);
        cases += 1;
    }
    // every case file must have a golden entry and the other way round: nothing is skipped
    let mut files: Vec<String> = std::fs::read_dir(parity_dir().join("sketch"))
        .unwrap()
        .filter_map(|e| e.ok()?.path().file_stem().map(|s| s.to_string_lossy().into_owned()))
        .collect();
    files.sort();
    let mut keys: Vec<String> = golden.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(files, keys, "case files and golden entries differ");
    assert_eq!(cases, keys.len());
    assert!(cases >= 20, "only {cases} golden cases");
    assert!(problems.is_empty(), "{} differences:\n{}", problems.len(), problems.join("\n"));
}

/// Face order and ring start/direction as shapely `polygonize` gives them (values taken from
/// shapely 1.x on these golden inputs): rings clockwise, each starting at the tail of the
/// reversed-or-forward edge GEOS meets first, faces in edge order.
#[test]
fn polygonize_order_matches_shapely() {
    let load = |name: &str| entities(&read(&parity_dir().join(format!("sketch/{name}.json")))["entities"]);
    let ring = |f: &stl_sketch::polygon::Face| f.exterior.iter().map(|p| (p[0].round(), p[1].round())).collect::<Vec<_>>();

    let f = stl_sketch::profile::faces(&load("lines_loop"), None);
    assert_eq!(f.len(), 1);
    assert_eq!(ring(&f[0]), [(20.0, 0.0), (0.0, 0.0), (0.0, 10.0), (20.0, 10.0)]);

    let f = stl_sketch::profile::faces(&load("touching_shapes"), None);
    assert_eq!(f.len(), 3);
    assert_eq!(ring(&f[0]), [(10.0, 0.0), (0.0, 0.0), (0.0, 10.0), (10.0, 10.0)]);
    assert_eq!(ring(&f[1]), [(10.0, 0.0), (10.0, 10.0), (20.0, 10.0), (20.0, 0.0)]);
    assert_eq!(ring(&f[2]), [(20.0, 10.0), (20.0, 20.0), (30.0, 20.0), (30.0, 10.0)]);

    let f = stl_sketch::profile::faces(&load("rect_ops"), None);
    assert_eq!(ring(&f[0]), [(0.0, 0.0), (0.0, 10.0), (20.0, 10.0), (20.0, 0.0)]);
}

fn ents_from(v: Value) -> Vec<Entity> {
    entities(&v)
}

#[test]
fn zero_length_mirror_axis_gives_the_axis_error() {
    let ents = ents_from(json!([
        {"id": "ax", "type": "line", "p1": [3, 3], "p2": [3, 3]},
        {"id": "c", "type": "circle", "cx": 5, "cy": 0, "r": 1}
    ]));
    let mut nid = new_id_factory();
    let err = ops::mirror(&ents, &["c".to_string()], "ax", &mut nid).unwrap_err();
    assert!(err.message().starts_with("Pick a straight sketch line"), "{}", err.message());
}

#[test]
fn polygon_defaults_to_six_sides_and_huge_counts_are_capped() {
    let ents = ents_from(json!([
        {"id": "p", "type": "polygon", "cx": 0, "cy": 0, "r": 5},
        {"id": "q", "type": "polygon", "cx": 0, "cy": 0, "r": 5, "n": 1e18},
        {"id": "z", "type": "polygon", "cx": 0, "cy": 0, "r": 5, "n": 0}
    ]));
    let shape = profile_with(&ents, Some(&["p".to_string()]), None).unwrap();
    assert_eq!(shape.contours.len(), 1);
    assert_eq!(shape.contours[0].len(), 6);
    let big = profile_with(&ents, Some(&["q".to_string()]), None).unwrap();
    assert!(big.contours[0].len() <= 10_000);
    // a polygon without sides draws nothing, and offsetting it does not panic
    assert!(profile_with(&ents, Some(&["z".to_string()]), None).is_err());
    let mut nid = new_id_factory();
    assert!(ops::offset(&ents, &["z".to_string()], 1.0, &mut nid).is_ok());
}

/// Cases beyond the golden: pieces from a split come back clockwise with the one new id, and
/// a shape shrunk away comes back as one empty outline (both as 1.x).
#[test]
fn offset_split_and_collapse_shapes() {
    let ents = ents_from(json!([{
        "id": "d", "type": "polyline", "closed": true,
        "pts": [[0,0],[10,0],[10,4],[6,4],[6,6],[10,6],[10,10],[0,10],[0,6],[4,6],[4,4],[0,4]]
    }]));
    let mut nid = new_id_factory();
    let out = ops::offset(&ents, &["d".to_string()], -1.0, &mut nid).unwrap();
    assert_eq!(out.len(), 3);
    assert_eq!(out[1].id(), out[2].id());
    let mut nid = new_id_factory();
    let gone = ops::offset(&ents, &["d".to_string()], -6.0, &mut nid).unwrap();
    assert_eq!(gone.len(), 2);
    assert!(matches!(&gone[1], Entity::Polyline(p) if p.pts.is_empty() && p.is_closed()));
}
