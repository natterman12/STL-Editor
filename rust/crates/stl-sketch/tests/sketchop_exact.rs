//! Exact numbers of three 1.x web `sketch_op` cases (`rust/tests/parity/sketchop/*.json`, made by
//! `dev/parity/js/sketchop_golden.mjs` from `web/engine/sketch2d.js`). The full JSON comparison is
//! in stl-app's `sketchop_golden`; these check the stl-sketch numbers bit for bit.

use serde_json::Value;
use std::path::{Path, PathBuf};
use stl_sketch::{Entity, ops, profile};

fn case(name: &str) -> Value {
    let p: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../tests/parity/sketchop/{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn entities(v: &Value) -> Vec<Entity> {
    v.as_array().unwrap().iter().map(|e| serde_json::from_value(e.clone()).unwrap()).collect()
}

fn pt(v: &Value) -> [f64; 2] {
    [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()]
}

/// The points of every ring, sorted, so the check doesn't depend on ring start or direction.
fn sorted_points(rings: Vec<Vec<[f64; 2]>>) -> Vec<Vec<(u64, u64)>> {
    let mut out: Vec<Vec<(u64, u64)>> = rings
        .into_iter()
        .map(|r| {
            let mut v: Vec<(u64, u64)> = r.iter().map(|p| (p[0].to_bits(), p[1].to_bits())).collect();
            v.sort();
            v.dedup();
            v
        })
        .collect();
    out.sort();
    out
}

#[test]
fn trim_arc_cuts_at_the_exact_crossings() {
    let c = case("trim_arc");
    let d = &c["data"];
    let got = ops::trim(&entities(&d["entities"]), d["id"].as_str().unwrap(), pt(&d["point"])).unwrap();
    let want = &c["result"]["entities"];
    for i in 0..2 {
        let Entity::Arc(a) = &got[i] else { panic!("piece {i} is not an arc") };
        assert_eq!(a.a0, want[i]["a0"].as_f64().unwrap(), "piece {i} a0");
        assert_eq!(a.a1, want[i]["a1"].as_f64().unwrap(), "piece {i} a1");
    }
}

/// Parked: open chains go through the GEOS `offset_curve` port (1.x desktop gives 1.5 here);
/// 1.x web's own mitre formula in `offsetOpen` gives 1.4999999999999998. Which 1.x is the
/// reference for loose offsets is the lead's call (the sketch and offset-corpus goldens are GEOS).
#[test]
#[ignore = "1.x desktop (GEOS) and 1.x web (offsetOpen) differ; reference not chosen yet"]
fn offset_open_chain_matches_js() {
    let c = case("offset_open");
    let d = &c["data"];
    let ids: Vec<String> = d["ids"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().into()).collect();
    let mut n = 0;
    let mut nid = || {
        n += 1;
        format!("eg{n}")
    };
    let got = ops::offset(&entities(&d["entities"]), &ids, d["distance"].as_f64().unwrap(), &mut nid).unwrap();
    let Entity::Polyline(p) = got.last().unwrap() else { panic!("no offset polyline") };
    let want: Vec<[f64; 2]> = c["result"]["entities"].as_array().unwrap().last().unwrap()["pts"]
        .as_array()
        .unwrap()
        .iter()
        .map(pt)
        .collect();
    assert_eq!(p.pts, want);
}

#[test]
fn faces_keep_exact_grid_coordinates() {
    let c = case("faces_nested");
    let got = profile::faces(&entities(&c["data"]["entities"]), None);
    let mut got_rings = vec![];
    for f in &got {
        got_rings.push(f.exterior.clone());
        got_rings.extend(f.holes.iter().cloned());
    }
    let mut want_rings = vec![];
    for f in c["result"]["faces"].as_array().unwrap() {
        for r in f.as_array().unwrap() {
            want_rings.push(r.as_array().unwrap().iter().map(pt).collect::<Vec<_>>());
        }
    }
    assert_eq!(sorted_points(got_rings), sorted_points(want_rings));
}
