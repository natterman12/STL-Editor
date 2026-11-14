//! Task 1 of milestone 4c: the fixed sketch interfaces (session, ids, undo, frame maths, geometry).

use std::time::Instant;

use serde_json::{json, Value};
use stl_app::sketch::{geom, Constraint, Entity, Ref, RefEnt, SketchSession};
use stl_app::viewport::PlaneFrame;

fn frame() -> PlaneFrame {
    PlaneFrame { origin: [0.0, 0.0, 0.0], x: [1.0, 0.0, 0.0], y: [0.0, 1.0, 0.0], normal: [0.0, 0.0, 1.0] }
}

fn ent(v: Value) -> Entity {
    serde_json::from_value(v).unwrap()
}

fn session(entities: Vec<Entity>) -> SketchSession {
    let mut s = SketchSession::new(frame(), json!("xy"), None, None, entities, vec![], vec![], None);
    s.seed = "abc".into();
    s
}

#[test]
fn new_ids_follow_1x() {
    let mut s = session(vec![ent(json!({"id": "eabc2", "type": "line", "p1": [0, 0], "p2": [1, 1]}))]);
    assert_eq!(s.new_id(), "eabc1");
    assert_eq!(s.new_id(), "eabc3"); // eabc2 is in use
    assert_eq!(s.new_id(), "eabc4");
    // The seed is base 36 of the time, as Date.now().toString(36).
    let fresh = SketchSession::new(frame(), json!("xy"), None, None, vec![], vec![], vec![], None);
    assert!(fresh.seed.len() >= 8 && fresh.seed.chars().all(|c| c.is_ascii_digit() || c.is_ascii_lowercase()), "{}", fresh.seed);
}

#[test]
fn undo_local_restores() {
    let mut s = session(vec![ent(json!({"id": "a", "type": "circle", "cx": 0, "cy": 0, "r": 1}))]);
    assert!(!s.undo_local());
    s.push();
    s.entities.push(ent(json!({"id": "b", "type": "circle", "cx": 5, "cy": 0, "r": 1})));
    s.constraints.push({ let mut c = Constraint::new("horizontal"); c.a = Some(Ref::ent("a")); c });
    s.sel.insert("b".into());
    s.sel_c = Some(0);
    assert!(s.undo_local());
    assert_eq!(s.entities.len(), 1);
    assert!(s.constraints.is_empty());
    assert!(s.sel.is_empty() && s.sel_c.is_none());
    assert!(!s.undo_local());
    for _ in 0..105 {
        s.push();
    }
    assert_eq!(s.history.len(), 100);
}

#[test]
fn uv_round_trip() {
    // A tilted frame: x and y orthonormal, away from the origin.
    let (c, sn) = (0.6, 0.8);
    let f = PlaneFrame { origin: [10.0, -4.0, 2.5], x: [c, sn, 0.0], y: [-sn * 0.5, c * 0.5, 0.8660254037844386], normal: [0.0; 3] };
    let y = f.y;
    let n = (y[0] * y[0] + y[1] * y[1] + y[2] * y[2]).sqrt();
    let f = PlaneFrame { y: [y[0] / n, y[1] / n, y[2] / n], ..f };
    let s = SketchSession::new(f, json!("xy"), None, None, vec![], vec![], vec![], None);
    for uv in [[0.0, 0.0], [3.5, -7.25], [-120.0, 44.0]] {
        let p = s.to_3(uv);
        let back = s.to_uv(p);
        assert!((back[0] - uv[0]).abs() < 1e-9 && (back[1] - uv[1]).abs() < 1e-9, "{uv:?} {back:?}");
        let p2 = s.to_3(back);
        assert!((0..3).all(|i| (p2[i] - p[i]).abs() < 1e-9));
    }
}

#[test]
fn step_params_shape() {
    let e1 = json!({"id": "e1", "type": "line", "p1": [0.0, 0.0], "p2": [10.0, 0.0]});
    let r_used = json!({"id": "r1", "type": "line", "p1": [0.0, 0.0], "p2": [0.0, 5.0]});
    let r_other = json!({"id": "r2", "type": "line", "p1": [1.0, 0.0], "p2": [1.0, 5.0]});
    let con = { let mut c = Constraint::new("coincident"); c.a = Some(Ref::pt("e1", "p1")); c.b = Some(Ref::pt("r1", "p1")); c };
    let mut s = SketchSession::new(
        frame(),
        json!({"id": "front"}),
        None,
        None,
        vec![ent(e1.clone())],
        vec![con.clone()],
        vec![RefEnt { ent: ent(r_other), missing: false }, RefEnt { ent: ent(r_used.clone()), missing: true }],
        Some("edges".into()),
    );
    s.seed = "x".into();
    let p = s.step_params();
    let keys: Vec<&String> = p.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["plane", "entities", "refs", "constraints"]);
    assert_eq!(p["plane"], json!({"id": "front"}));
    assert_eq!(p["entities"], json!([e1]));
    assert_eq!(p["refs"], json!([r_used])); // only the part edge a constraint uses, with no `missing` key
    assert_eq!(p["constraints"], serde_json::to_value([con]).unwrap());
    assert_eq!(s.used_refs().len(), 1);
}

// ---------------------------------------------------------------- geometry against 1.x
// RECORDED = web/js/sketchgeom.js run in Node on the entities below (entityOutline summaries,
// keyPoints, distToEntity to (6.5, -3.25), describe, dimensions, arcSweep, arcPts, arcFrom3 and
// applyDimension), numbers printed by JSON.stringify.
const RECORDED: &str = r##"{"rect":{"outline":{"n":44,"first":[1.1614023831020028,5.707518959299019],"last":[-23.95266910041377,-1.0217762133665182],"mid":[-29.161402383102,18.41748104070098],"sx":-616.0000000000001,"sy":530.7500000000002,"closed":true},"keyPoints":[[1,2],[-29,2],[-29,22.125],[1,22.125],[-14,12.0625],[-14,2],[-29,12.0625],[-14,22.125],[1,12.0625]],"dist":10.370284742159022,"describe":"30 × 20.13","dims":[{"key":"w","value":30,"label":"30","at":[-14,2]},{"key":"h","value":20.125,"label":"20.13","at":[1,12.0625]}]},"rect0":{"outline":{"n":4,"first":[1,2],"last":[1,22],"mid":[31,22],"sx":64,"sy":48,"closed":true},"keyPoints":[[1,2],[31,2],[31,22],[1,22],[16,12],[16,2],[31,12],[16,22],[1,12]],"dist":5.25,"describe":"30 × 20","dims":[{"key":"w","value":30,"label":"30","at":[16,2]},{"key":"h","value":20,"label":"20","at":[31,12]}]},"circle":{"outline":{"n":96,"first":[8.004999999999999,-4],"last":[7.9942839108092105,-4.327342661796865],"mid":[-2.005,-3.9999999999999996],"sx":288,"sy":-383.99999999999994,"closed":true},"keyPoints":[[3,-4],[8.004999999999999,-4],[-2.005,-4],[3,1.005],[3,-9.004999999999999]],"dist":1.4234436355727622,"describe":"Ø10.01","dims":[{"key":"d","value":10.01,"label":"⌀10.01","at":[6.5390355,-0.4609645000000002]}]},"polygon":{"outline":{"n":5,"first":[7.893654271085456,2.2155372436685123],"last":[4.286300939501233,-5.18063315001249],"mid":[-5.291558324094168,4.068598027523544],"sx":4.999999999999997,"sy":5.0000000000000036,"closed":true},"keyPoints":[[1,1],[7.893654271085456,2.2155372436685123],[1.9742117067204583,7.931876481190993],[-5.291558324094168,4.068598027523544],[-3.8626085932129826,-4.035378602370556],[4.286300939501233,-5.18063315001249]],"dist":1.1433256678743668,"describe":"5 sides, R7","dims":[{"key":"r","value":7,"label":"R7","at":[8,1]}]},"slot":{"outline":{"n":50,"first":[32.5,38.125],"last":[2.5,-1.8750000000000002],"mid":[-2.5,1.875],"sx":750,"sy":1000.0000000000002,"closed":true},"keyPoints":[[0,0],[30,40]],"dist":4.025,"describe":"L50 W6.25","dims":[{"key":"len","value":50,"label":"50","at":[15,20]},{"key":"w","value":6.25,"label":"⌀6.25","at":[33.125,40]}]},"polyline":{"outline":{"n":4,"first":[0,0],"last":[2,9],"mid":[10,7],"sx":22,"sy":16,"closed":true},"keyPoints":[[0,0],[10,0],[10,7],[2,9],[5,0],[10,3.5],[6,8]],"dist":3.25,"describe":"4 pts, closed","dims":[]},"line":{"outline":{"n":2,"first":[1,1],"last":[4,5],"mid":[4,5],"sx":5,"sy":6,"closed":false},"keyPoints":[[1,1],[4,5],[2.5,3]],"dist":6.950719387228922,"describe":"L5","dims":[{"key":"len","value":5,"label":"5","at":[2.5,3]}]},"arc":{"outline":{"n":26,"first":[7.000000000000001,-5.6602540378443855],"last":[9.66044443118978,9.427876096865392],"mid":[11.902680687415703,1.6082689903993412],"sx":274.3639802671653,"sy":38.791230741174004,"closed":false},"keyPoints":[[7.000000000000001,-5.6602540378443855],[9.66044443118978,9.427876096865392],[11.84807753012208,1.2635182233306872],[2,3]],"dist":2.292518420031407,"describe":"R10, 100°","dims":[{"key":"r","value":10,"label":"R10","at":[11.84807753012208,1.2635182233306872]}],"sweep":100,"pts5":[[7.000000000000001,-5.6602540378443855],[9.660444431189777,-3.427876096865396],[11.396926207859085,-0.42020143325668613],[12,2.9999999999999973],[11.396926207859083,6.42020143325669],[9.66044443118978,9.427876096865392]],"pts":{"n":26,"first":[7.000000000000001,-5.6602540378443855],"last":[9.66044443118978,9.427876096865392],"mid":[11.902680687415703,1.6082689903993412],"sx":274.3639802671653,"sy":38.791230741174004}},"from3":{"type":"arc","cx":0,"cy":0,"r":10,"a0":0,"a1":180},"from3b":null,"from3c":{"type":"arc","cx":0,"cy":0,"r":10,"a0":0,"a1":180},"apply":{"rect:w":{"w":-12},"rect:h":{"h":7},"circle:d":{"r":4.5},"arc:r":{"r":4},"polygon:r":{"r":3},"slot:len":{"x2":12,"y2":16},"slot:w":{"r":1},"line:len":{"p2":[7,9]}}}"##;

fn entities() -> Vec<(&'static str, Value)> {
    vec![
        ("rect", json!({"type":"rect","x":1,"y":2,"w":-30,"h":20.125,"angle":15,"fillet":2})),
        ("rect0", json!({"type":"rect","x":1,"y":2,"w":30,"h":20})),
        ("circle", json!({"type":"circle","cx":3,"cy":-4,"r":5.005})),
        ("polygon", json!({"type":"polygon","cx":1,"cy":1,"r":7,"n":5,"angle":10})),
        ("slot", json!({"type":"slot","x1":0,"y1":0,"x2":30,"y2":40,"r":3.125})),
        ("polyline", json!({"type":"polyline","pts":[[0,0],[10,0],[10,7],[2,9]],"closed":true})),
        ("line", json!({"type":"line","p1":[1,1],"p2":[4,5]})),
        ("arc", json!({"type":"arc","cx":2,"cy":3,"r":10,"a0":300,"a1":40})),
    ]
}

fn near(a: f64, b: f64, what: &str) {
    assert!((a - b).abs() <= 1e-9 * (1.0 + b.abs()), "{what}: {a} vs {b}");
}

fn near_pt(a: [f64; 2], b: &Value, what: &str) {
    near(a[0], b[0].as_f64().unwrap(), what);
    near(a[1], b[1].as_f64().unwrap(), what);
}

#[test]
fn geom_matches_1x() {
    let rec: Value = serde_json::from_str(RECORDED).unwrap();
    for (name, v) in entities() {
        let e = ent(v);
        let r = &rec[name];
        let (pts, closed) = geom::outline(&e);
        let o = &r["outline"];
        assert_eq!(pts.len() as u64, o["n"].as_u64().unwrap(), "{name} outline count");
        assert_eq!(closed, o["closed"].as_bool().unwrap(), "{name} closed");
        assert_eq!(geom::entity_outline(&e), pts);
        near_pt(pts[0], &o["first"], name);
        near_pt(*pts.last().unwrap(), &o["last"], name);
        near_pt(pts[pts.len() >> 1], &o["mid"], name);
        near(pts.iter().map(|p| p[0]).sum(), o["sx"].as_f64().unwrap(), name);
        near(pts.iter().map(|p| p[1]).sum(), o["sy"].as_f64().unwrap(), name);
        let kp = geom::key_points(&e);
        let want = r["keyPoints"].as_array().unwrap();
        assert_eq!(kp.len(), want.len(), "{name} key points");
        for (k, w) in kp.iter().zip(want) {
            near_pt(k.1, w, name);
        }
        near(geom::dist_to_entity(&e, [6.5, -3.25]), r["dist"].as_f64().unwrap(), name);
        assert_eq!(geom::describe(&e), r["describe"].as_str().unwrap(), "{name} describe");
        let dims = geom::dimension_items(&e);
        let want = r["dims"].as_array().unwrap();
        assert_eq!(dims.len(), want.len(), "{name} dims");
        for (d, w) in dims.iter().zip(want) {
            assert_eq!(d.key, w["key"].as_str().unwrap());
            assert_eq!(d.label, w["label"].as_str().unwrap(), "{name} dim label");
            near(d.value, w["value"].as_f64().unwrap(), name);
            near_pt(d.at, &w["at"], name);
        }
        assert_eq!(geom::dimensions(&e).len(), want.len());
        if name == "arc" {
            near(geom::arc_sweep(&e), r["sweep"].as_f64().unwrap(), "sweep");
            let p5 = geom::arc_pts(&e, 5);
            for (p, w) in p5.iter().zip(r["pts5"].as_array().unwrap()) {
                near_pt(*p, w, "arc_pts(5)");
            }
            assert_eq!(p5.len(), 6);
        }
    }
    // arc_from3: through three points, and collinear.
    let a = geom::arc_from3([10.0, 0.0], [0.0, 10.0], [-10.0, 0.0]).unwrap();
    let w = &rec["from3"];
    let Entity::Arc(a) = a else { panic!("not an arc") };
    for (got, key) in [(a.cx, "cx"), (a.cy, "cy"), (a.r, "r"), (a.a0, "a0"), (a.a1, "a1")] {
        near(got, w[key].as_f64().unwrap(), key);
    }
    assert!(rec["from3b"].is_null() && geom::arc_from3([0.0, 0.0], [1.0, 1.0], [2.0, 2.0]).is_none());
    let Some(Entity::Arc(c)) = geom::arc_from3([-10.0, 0.0], [0.0, 10.0], [10.0, 0.0]) else { panic!() };
    near(c.a0, rec["from3c"]["a0"].as_f64().unwrap(), "from3c a0");
    near(c.a1, rec["from3c"]["a1"].as_f64().unwrap(), "from3c a1");
    // apply_dimension: the patch 1.x returns, applied.
    for (name, key, v) in [("rect", "w", 12.0), ("rect", "h", 7.0), ("circle", "d", 9.0), ("arc", "r", 4.0), ("polygon", "r", 3.0), ("slot", "len", 20.0), ("slot", "w", 2.0), ("line", "len", 10.0)] {
        let mut e = ent(entities().into_iter().find(|(n, _)| *n == name).unwrap().1);
        geom::apply_dimension(&mut e, key, v);
        let patch = &rec["apply"][format!("{name}:{key}")];
        let now = serde_json::to_value(&e).unwrap();
        for (k, pv) in patch.as_object().unwrap() {
            if let Some(f) = pv.as_f64() {
                near(now[k].as_f64().unwrap(), f, k);
            } else {
                near_pt([now[k][0].as_f64().unwrap(), now[k][1].as_f64().unwrap()], pv, k);
            }
        }
    }
    let mut e = ent(entities()[2].1.clone());
    geom::apply_dimension(&mut e, "d", -1.0);
    assert_eq!(e, ent(entities()[2].1.clone()), "a non-positive value changes nothing");
}

// ---------------------------------------------------------------- solver timing (Step 3)

/// 200 rectangles in a 20 x 10 grid, each a little off its slot; 100 constraints join a rectangle's
/// right corners to its right neighbour's left corners (`coincident`), row by row, so the first
/// five rows are 20-rectangle chains (100 rectangles, 400 unknowns, 200 equations).
fn grid() -> (Vec<Entity>, Vec<Constraint>) {
    let mut ents = Vec::new();
    let mut cons = Vec::new();
    for i in 0..200 {
        let (cx, cy) = ((i % 20) as f64 * 15.0, (i / 20) as f64 * 15.0);
        ents.push(ent(json!({"id": format!("g{i}"), "type": "rect", "x": cx + 0.3, "y": cy - 0.2, "w": 10.4, "h": 9.7})));
    }
    let mut i = 0;
    while cons.len() < 100 {
        if i % 20 < 19 {
            let mut c = Constraint::new("coincident");
            c.a = Some(Ref::pt(&format!("g{i}"), "c1"));
            c.b = Some(Ref::pt(&format!("g{}", i + 1), "c0"));
            cons.push(c);
        }
        i += 1;
    }
    (ents, cons)
}

fn percentile(times: &mut [f64], p: f64) -> f64 {
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    times[((times.len() as f64 * p).ceil() as usize).clamp(1, times.len()) - 1]
}

#[test]
#[ignore = "timing harness: cargo test -p stl-app --test sketch_core --release -- --ignored --nocapture solver_timing"]
fn solver_timing() {
    use stl_sketch::solver::{solve, Lock};
    let (ents, cons) = grid();
    // Second case: the user drags the first corner 3 mm right and 2 mm up (a lock on a point).
    let drag = [Lock { e: "g0".into(), k: Some("c0".into()), at: Some(vec![3.3, 1.8]), path: None, value: None }];
    for (label, locks) in [("solve", &[][..]), ("solve + drag lock", &drag[..])] {
        let mut times = Vec::new();
        let mut ok = false;
        for _ in 0..100 {
            let t = Instant::now();
            let r = solve(&ents, &cons, locks, &[]);
            times.push(t.elapsed().as_secs_f64() * 1000.0);
            ok = r.ok;
        }
        let max = times.iter().cloned().fold(0.0, f64::max);
        println!("solver, {} entities, {} constraints, {label}: p50 {:.3} ms, p99 {:.3} ms, max {:.3} ms (ok {ok})",
            ents.len(), cons.len(), percentile(&mut times.clone(), 0.5), percentile(&mut times, 0.99), max);
    }
}
