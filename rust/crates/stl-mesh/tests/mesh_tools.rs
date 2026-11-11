//! Sculpt, remesh, offset and the thin-wall scan against the 1.x / web goldens
//! (`rust/tests/parity/features*/{sculpt,remesh,offset,thincheck}.json`).

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::Value;
use stl_kernel::{MeshArrays, Solid};
use stl_mesh::offset::offset_solid;
use stl_mesh::remesh::remesh;
use stl_mesh::sculpt::{sculpt, Stroke};
use stl_mesh::thin::thin_scan;
use stl_parity::{compare_mesh, mesh_stats, Exactness, MeshStats};

fn tests_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests")
}

fn load(rel: &str) -> Value {
    let p = tests_dir().join("parity").join(rel);
    serde_json::from_str(&std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))).unwrap()
}

fn f(v: &Value, k: &str, d: f64) -> f64 {
    v.get(k).and_then(Value::as_f64).unwrap_or(d)
}

fn stats(s: &Solid) -> MeshStats {
    let m = s.to_mesh();
    mesh_stats(&m.positions, &m.triangles)
}

fn golden_stats(g: &Value) -> MeshStats {
    let s = &g["stats"];
    let a3 = |k: &str| {
        let a = s[k].as_array().unwrap();
        [a[0].as_f64().unwrap(), a[1].as_f64().unwrap(), a[2].as_f64().unwrap()]
    };
    MeshStats {
        triangles: s["triangles"].as_u64().unwrap() as usize,
        vertices: s["vertices"].as_u64().unwrap() as usize,
        volume: s["volume"].as_f64().unwrap(),
        area: s["area"].as_f64().unwrap(),
        bbox_min: a3("bbox_min"),
        bbox_max: a3("bbox_max"),
        watertight: s["watertight"].as_bool().unwrap(),
    }
}

/// A binary STL as a welded solid.
fn read_stl(path: &PathBuf) -> Solid {
    let data = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let n = u32::from_le_bytes(data[80..84].try_into().unwrap()) as usize;
    let mut idx: HashMap<[u32; 3], u32> = HashMap::new();
    let mut positions = Vec::new();
    let mut triangles = Vec::new();
    for t in 0..n {
        let rec = &data[84 + 50 * t + 12..84 + 50 * t + 48];
        for c in 0..3 {
            let x: Vec<f32> = (0..3).map(|k| f32::from_le_bytes(rec[c * 12 + k * 4..c * 12 + k * 4 + 4].try_into().unwrap())).collect();
            let key = [x[0].to_bits(), x[1].to_bits(), x[2].to_bits()];
            let next = idx.len() as u32;
            let id = *idx.entry(key).or_insert_with(|| {
                positions.extend_from_slice(&x);
                next
            });
            triangles.push(id);
        }
    }
    Solid::from_mesh(&MeshArrays { positions, triangles }).expect("the model closes up")
}

/// The bodies of a case, by feature id (`f1`, `f2`...), for primitives, joins and imports.
fn bodies(case: &Value) -> HashMap<String, Solid> {
    let mut out: HashMap<String, Solid> = HashMap::new();
    for (i, feat) in case["features"].as_array().unwrap().iter().enumerate() {
        let id = format!("f{}", i + 1);
        let p = &feat["params"];
        match feat["type"].as_str().unwrap() {
            "primitive" => {
                let seg = f(p, "segments", 96.0) as u32;
                let anchor = p.get("anchor").and_then(Value::as_str).unwrap_or("base");
                let mut s = match p["shape"].as_str().unwrap() {
                    "box" => {
                        let (l, w, h) = (f(p, "length", 10.0), f(p, "width", 10.0), f(p, "height", 10.0));
                        let shift = match anchor {
                            "corner" => [0.0; 3],
                            "center" => [-l / 2.0, -w / 2.0, -h / 2.0],
                            _ => [-l / 2.0, -w / 2.0, 0.0],
                        };
                        Solid::cube([l, w, h], false).translate(shift)
                    }
                    "sphere" => {
                        let r = f(p, "radius", 5.0);
                        let s = Solid::sphere(r, seg);
                        if anchor == "base" { s.translate([0.0, 0.0, r]) } else { s }
                    }
                    other => panic!("primitive {other} is not needed by these cases"),
                };
                if let Some(o) = p.get("offset").and_then(Value::as_array) {
                    s = s.translate([o[0].as_f64().unwrap(), o[1].as_f64().unwrap(), o[2].as_f64().unwrap()]);
                }
                out.insert(id, s);
            }
            "combine" => {
                let target = p["target"].as_str().unwrap();
                let mut s = out[target].clone();
                for t in p["tools"].as_array().unwrap() {
                    s = s.union(&out[t.as_str().unwrap()]);
                }
                out.insert(id, s);
            }
            "import" => {
                let src = case["sources"][p["source"].as_str().unwrap()].as_str().unwrap();
                out.insert(id, read_stl(&tests_dir().join("models").join(src)));
            }
            _ => {}
        }
    }
    out
}

fn last_op<'a>(case: &'a Value, ty: &str) -> &'a Value {
    case["features"].as_array().unwrap().iter().rev().find(|x| x["type"] == ty).unwrap()
}

fn golden_body(g: &Value) -> Option<&Value> {
    g["bodies"].as_array().and_then(|b| b.first())
}

fn last_error(g: &Value) -> Option<String> {
    g["results"].as_array().unwrap().last().unwrap()["error"].as_str().map(String::from)
}

fn check(name: &str, got: &Solid, g: &Value, ex: Exactness, fails: &mut Vec<String>) {
    let want = golden_stats(g);
    let have = stats(got);
    println!("{name}: tris golden {} got {}, volume golden {} got {}, area golden {} got {}",
        want.triangles, have.triangles, want.volume, have.volume, want.area, have.area);
    for d in compare_mesh(&want, &have, ex) {
        fails.push(format!("{name}: {d}"));
    }
}

#[test]
fn sculpt_modes_match_1x() {
    let cases = load("features/sculpt.json");
    let golden = load("features_golden/sculpt.json");
    let mut fails = Vec::new();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        if last_op(case, "sculpt")["params"]["strokes"].as_array().unwrap().is_empty() {
            continue; // "Drag over the model to sculpt." is the feature's own check
        }
        let g = &golden[name];
        let body = bodies(case)["f1"].clone();
        let strokes: Vec<Stroke> = serde_json::from_value(last_op(case, "sculpt")["params"]["strokes"].clone()).unwrap();
        let ex = if case["exactness"] == "exact" { Exactness::Exact } else { Exactness::Approx };
        match (sculpt(&body, &strokes), last_error(g)) {
            (Ok(s), None) => check(name, &s, golden_body(g).unwrap(), ex, &mut fails),
            (Err(e), Some(want)) => {
                if e.to_string() != want {
                    fails.push(format!("{name}: message golden {want:?} got {e}"));
                }
            }
            (Ok(_), Some(want)) => fails.push(format!("{name}: golden error {want:?}, got a solid")),
            (Err(e), None) => fails.push(format!("{name}: golden ok, got error {e}")),
        }
    }
    assert!(fails.is_empty(), "{}", fails.join("\n"));
}

#[test]
fn unknown_brush_message() {
    let s = Solid::cube([10.0; 3], true);
    let st = Stroke { points: vec![[5.0, 5.0, 5.0], [4.0, 5.0, 5.0]], radius: 5.0, strength: 0.5, mode: "smash".into() };
    assert_eq!(sculpt(&s, &[st]).err().unwrap().to_string(), "Unknown brush smash.");
    let d: Stroke = serde_json::from_str("{}").unwrap();
    assert_eq!((d.radius, d.strength, d.mode.as_str(), d.points.len()), (5.0, 0.5, "smooth", 0));
}

#[test]
fn remesh_matches_web_golden() {
    let cases = load("features/remesh.json");
    let golden = load("remesh_golden.json");
    let feature_golden = load("features_golden/remesh.json");
    let mut fails = Vec::new();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let body = bodies(case)["f1"].clone();
        let edge = f(&last_op(case, "remesh")["params"], "edge", 0.0);
        let got = remesh(&body, edge, 5);
        match golden.get(name) {
            Some(g) => {
                let s = got.unwrap_or_else(|e| panic!("{name}: {e}"));
                check(name, &s, g, Exactness::Approx, &mut fails);
            }
            None => {
                let e = got.err().expect("an edge of zero or less is refused");
                let want = last_error(&feature_golden[name]).unwrap_or_else(|| panic!("{name}: golden has no error"));
                assert_eq!(e.to_string(), want, "{name}");
            }
        }
    }
    assert!(fails.is_empty(), "{}", fails.join("\n"));
}

#[test]
fn offset_grow_and_shrink_match_1x() {
    let cases = load("features/offset.json");
    let golden = load("features_golden/offset.json");
    let exceptions = load("exceptions.json");
    let mut fails = Vec::new();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let g = &golden[name];
        let all = bodies(case);
        let Some(op) = case["features"].as_array().unwrap().iter().find(|x| x["type"] == "offset") else { continue };
        let p = &op["params"];
        let ids: Vec<&str> = p["bodies"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect();
        if ids.is_empty() {
            continue;
        }
        let d = f(p, "distance", 0.0);
        let d = if p["direction"] == "shrink" { -d } else { d };
        let ex = if case["exactness"] == "exact" { Exactness::Exact } else { Exactness::Approx };
        let mut checked = 0;
        for (n, id) in ids.iter().enumerate() {
            let res = offset_solid(&all[*id], d, 12000);
            let gb = &g["bodies"].as_array().unwrap();
            match (res, last_error(g)) {
                (Ok(s), None) if d.abs() >= 1e-6 => {
                    let label = format!("{name}/{id}");
                    match exceptions.get(format!("offset/{name}")) {
                        // an approved exception: must differ from 1.x and match its `expect`
                        Some(exc) => {
                            let mut vs_golden = Vec::new();
                            check(&label, &s, &gb[n], ex, &mut vs_golden);
                            if vs_golden.is_empty() {
                                fails.push(format!("{label}: stale exception, it now matches the 1.x golden"));
                            }
                            let want = exc["expect"]["bodies"]
                                .as_array()
                                .and_then(|a| a.iter().find(|b| b["id"] == *id))
                                .unwrap_or_else(|| panic!("{label}: exception has no expect for this body"));
                            check(&format!("{label} (vs exception)"), &s, want, ex, &mut fails);
                        }
                        None => check(&label, &s, &gb[n], ex, &mut fails),
                    }
                    checked += 1;
                }
                (Err(e), Some(want)) => {
                    if e.to_string() != want {
                        fails.push(format!("{name}: message golden {want:?} got {e}"));
                    }
                    checked += 1;
                }
                (Ok(_), _) if d.abs() < 1e-6 => checked += 1, // 1.x refuses a zero distance in the feature
                (r, e) => fails.push(format!("{name}: got {:?}, golden error {e:?}", r.map(|_| "a solid"))),
            }
        }
        assert!(checked > 0, "{name}");
    }
    assert!(fails.is_empty(), "{}", fails.join("\n"));
}

#[test]
fn thin_scan_matches_1x() {
    let cases = load("features/thincheck.json");
    let golden = load("features_golden/thincheck.json");
    let mut fails = Vec::new();
    let mut ran = 0;
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let g = &golden[name];
        let t = f(&last_op(case, "thincheck")["params"], "min", 0.8);
        if t <= 0.0 || case["features"].as_array().unwrap().len() < 2 {
            continue;
        }
        let all = bodies(case);
        let id = format!("f{}", case["features"].as_array().unwrap().len() - 1);
        let r = thin_scan(&all[&id], t);
        ran += 1;
        let last = g["results"].as_array().unwrap().last().unwrap();
        println!("{name}: {r:?}");
        let thin = &last["thin"];
        if thin.is_null() {
            fails.push(format!("{name}: golden has no thin record"));
            continue;
        }
        if thin["count"].as_u64().unwrap() as usize != r.count {
            fails.push(format!("{name}: count golden {} got {}", thin["count"], r.count));
        }
        match (thin["min"].as_f64(), r.min) {
            (None, None) => {}
            (Some(a), Some(b)) if (a - b).abs() <= 0.001 => {}
            (a, b) => fails.push(format!("{name}: min golden {a:?} got {b:?}")),
        }
        let spots = thin["spots"].as_array().unwrap();
        if spots.len() != r.spots.len() {
            fails.push(format!("{name}: {} spots in golden, {} got", spots.len(), r.spots.len()));
        }
        for (k, (gs, rs)) in spots.iter().zip(&r.spots).enumerate() {
            for c in 0..3 {
                let a = gs[c].as_f64().unwrap();
                if !((a - rs[c]).abs() <= 0.01) {
                    fails.push(format!("{name}: spot {k} axis {c} golden {a} got {}", rs[c]));
                }
            }
        }
    }
    assert_eq!(ran, 4);
    assert!(fails.is_empty(), "{}", fails.join("\n"));
}
