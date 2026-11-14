//! SVG/DXF import, project, Use Edge, and region faces from a 4c sketch.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Map, Value};
use stl_app::sketch::import::import_file;
use stl_app::sketch::project::{self, UseEdge};
use stl_app::sketch::{SketchInput, SketchSession, ToolCx};
use stl_app::viewport::{PlaneFrame, Viewport};
use stl_engine::{Document, Registry};
use stl_sketch::entities::Entity;

fn rust_tests() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests")
}

fn round9(v: &Value) -> Value {
    match v {
        Value::Number(n) => {
            let x = n.as_f64().unwrap_or(f64::NAN);
            let p = 1e9_f64;
            json!((x * p).round() / p)
        }
        Value::Array(a) => Value::Array(a.iter().map(round9).collect()),
        Value::Object(o) => {
            let mut m = Map::new();
            for (k, child) in o {
                m.insert(k.clone(), round9(child));
            }
            Value::Object(m)
        }
        other => other.clone(),
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

/// Even-odd point in the rings `sketch_op faces` returns (`app.js` `inPoly`).
fn in_poly(p: [f64; 2], rings: &[Value]) -> bool {
    let mut c = false;
    for ring in rings {
        let Some(pts) = ring.as_array() else { continue };
        let n = pts.len();
        if n == 0 {
            continue;
        }
        let mut j = n - 1;
        for i in 0..n {
            let xi = pts[i][0].as_f64().unwrap_or(0.0);
            let yi = pts[i][1].as_f64().unwrap_or(0.0);
            let xj = pts[j][0].as_f64().unwrap_or(0.0);
            let yj = pts[j][1].as_f64().unwrap_or(0.0);
            if ((yi > p[1]) != (yj > p[1])) && p[0] < (xj - xi) * (p[1] - yi) / (yj - yi) + xi {
                c = !c;
            }
            j = i;
        }
    }
    c
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn num(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn xy(v: &Value) -> [f64; 2] {
    let a = v.as_array().expect("point");
    [a[0].as_f64().unwrap_or(0.0), a[1].as_f64().unwrap_or(0.0)]
}

/// Same bbox the import2d parity test uses, taken from the golden entities only.
fn golden_drawing_size(want_ents: &[Value]) -> f64 {
    let mut lo = [f64::INFINITY, f64::INFINITY];
    let mut hi = [f64::NEG_INFINITY, f64::NEG_INFINITY];
    let mut push = |x: f64, y: f64| {
        lo[0] = lo[0].min(x);
        lo[1] = lo[1].min(y);
        hi[0] = hi[0].max(x);
        hi[1] = hi[1].max(y);
    };
    for w in want_ents {
        match w.get("type").and_then(Value::as_str).unwrap_or("") {
            "line" => {
                for key in ["p1", "p2"] {
                    if let Some(p) = w.get(key) {
                        let p = xy(p);
                        push(p[0], p[1]);
                    }
                }
            }
            "circle" | "arc" => {
                let (cx, cy, r) = (num(w, "cx"), num(w, "cy"), num(w, "r"));
                push(cx - r, cy - r);
                push(cx + r, cy + r);
            }
            "polyline" => {
                if let Some(pts) = w.get("pts").and_then(Value::as_array) {
                    for p in pts {
                        let p = xy(p);
                        push(p[0], p[1]);
                    }
                }
            }
            _ => {}
        }
    }
    if !lo[0].is_finite() {
        return 0.0;
    }
    (hi[0] - lo[0]).max(hi[1] - lo[1])
}

fn same_entity(got: &Entity, want: &Value, tol: f64, file: &str, i: usize) {
    let kind = got.kind();
    let want_kind = want.get("type").and_then(Value::as_str).unwrap_or("");
    assert_eq!(kind, want_kind, "{file} entity {i} type");
    match got {
        Entity::Line(l) => {
            let p1 = xy(&want["p1"]);
            let p2 = xy(&want["p2"]);
            assert!(close(l.p1[0], p1[0], tol) && close(l.p1[1], p1[1], tol), "{file} entity {i} p1");
            assert!(close(l.p2[0], p2[0], tol) && close(l.p2[1], p2[1], tol), "{file} entity {i} p2");
        }
        Entity::Circle(c) => {
            assert!(close(c.cx, num(want, "cx"), tol), "{file} entity {i} cx");
            assert!(close(c.cy, num(want, "cy"), tol), "{file} entity {i} cy");
            assert!(close(c.r, num(want, "r"), tol), "{file} entity {i} r");
        }
        Entity::Arc(a) => {
            assert!(close(a.cx, num(want, "cx"), tol), "{file} entity {i} cx");
            assert!(close(a.cy, num(want, "cy"), tol), "{file} entity {i} cy");
            assert!(close(a.r, num(want, "r"), tol), "{file} entity {i} r");
            assert!(close(a.a0, num(want, "a0"), tol), "{file} entity {i} a0");
            assert!(close(a.a1, num(want, "a1"), tol), "{file} entity {i} a1");
        }
        Entity::Polyline(p) => {
            let closed = want.get("closed").and_then(Value::as_bool).unwrap_or(false);
            assert_eq!(p.is_closed(), closed, "{file} entity {i} closed");
            let gp = want["pts"].as_array().expect("pts");
            assert_eq!(p.pts.len(), gp.len(), "{file} entity {i} point count");
            for (k, (a, b)) in p.pts.iter().zip(gp.iter()).enumerate() {
                let b = xy(b);
                assert!(
                    close(a[0], b[0], tol) && close(a[1], b[1], tol),
                    "{file} entity {i} pt {k}: got {a:?} want {b:?} tol {tol}"
                );
            }
        }
        other => panic!("{file} entity {i}: unexpected {}", other.kind()),
    }
}

#[test]
fn import_svg_dxf_into_sketch() {
    let golden: Value = serde_json::from_str(
        &fs::read_to_string(rust_tests().join("parity/import2d_golden.json")).expect("golden"),
    )
    .expect("parse golden");
    let cases = rust_tests().join("parity/import2d");
    let obj = golden.as_object().expect("object");
    assert!(!obj.is_empty());
    for (file, want) in obj {
        let bytes = fs::read(cases.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
        let got = import_file(file, &bytes);
        if let Some(err) = want.get("error").and_then(Value::as_str) {
            assert_eq!(got.expect_err(file), err, "{file}");
            continue;
        }
        let want_ents = want.get("entities").and_then(Value::as_array).unwrap_or_else(|| panic!("{file}: entities"));
        if want_ents.is_empty() {
            assert_eq!(got.expect_err(file), "No lines or curves found in that file.", "{file}");
            continue;
        }
        let ents = got.unwrap_or_else(|e| panic!("{file}: {e}"));
        assert_eq!(ents.len(), want_ents.len(), "{file}: entity count");
        let dxf = file.to_ascii_lowercase().ends_with(".dxf");
        let text = String::from_utf8_lossy(&bytes);
        let size = if dxf {
            0.0
        } else {
            golden_drawing_size(want_ents).max(stl_sketch::import2d::svg_drawing_size_mm(&text).unwrap_or(0.0))
        };
        let tol = if dxf { 1e-9 } else { 0.01_f64.max(size * 0.001) };
        for (i, (g, w)) in ents.iter().zip(want_ents.iter()).enumerate() {
            same_entity(g, w, tol, file, i);
        }
    }
}

#[test]
fn project_section_and_silhouette() {
    let doc = box_doc();
    let dir = rust_tests().join("parity/sketchop");
    for name in ["project_section", "project_silhouette", "project_section_construction", "project_silhouette_construction"] {
        let case: Value = serde_json::from_str(&fs::read_to_string(dir.join(format!("{name}.json"))).unwrap()).unwrap();
        let got = doc.sketch_op(case["op"].as_str().unwrap(), &case["data"]).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(round9(&got), round9(&case["result"]), "{name}");
    }
}

#[test]
fn use_edge_adds_ref() {
    let frame = PlaneFrame { origin: [0.0, 0.0, 0.0], x: [1.0, 0.0, 0.0], y: [0.0, 1.0, 0.0], normal: [0.0, 0.0, 1.0] };
    let edge: Entity = serde_json::from_value(json!({
        "id": "r0", "type": "line", "p1": [0, 0], "p2": [10, 0], "ref": "edge"
    }))
    .unwrap();
    let mut session = SketchSession::new(frame, json!("XY"), None, None, Vec::new(), Vec::new(), vec![stl_app::sketch::RefEnt { ent: edge, missing: false }], None);
    assert_eq!(project::use_edge(&mut session, [5.0, 0.0], 1.0), UseEdge::Added);
    assert_eq!(session.entities.len(), 1);
    let line = &session.entities[0];
    assert_eq!(line.kind(), "line");
    let id = line.id().unwrap().to_string();
    assert!(session.sel.contains(&id));
    assert_eq!(session.constraints.len(), 2);
    assert_eq!(session.constraints[0].kind, "coincident");
    assert_eq!(session.constraints[1].kind, "coincident");
    assert_eq!(session.constraints[0].extra.get("auto").and_then(Value::as_bool), Some(true));
    assert_eq!(project::use_edge(&mut session, [50.0, 50.0], 1.0), UseEdge::Miss);

    let mut tool = project::tool_for("sk_useedge").expect("use edge tool");
    assert_eq!(tool.key(), "sk_useedge");
    assert!(tool.hint(&session).contains("orange part edge"));
    let vp = Viewport::default();
    let params = BTreeMap::new();
    let mut out = Vec::new();
    let mut cx = ToolCx { s: &mut session, vp: &vp, inch: false, params: &params, out: &mut out };
    let handled = tool.input(&mut cx, &SketchInput::Down { uv: [5.0, 1.0], pos: egui::pos2(0.0, 0.0), mods: egui::Modifiers::default() });
    assert!(handled);
    assert!(project::tool_for("sk_project").is_some());
}

#[test]
fn regions_from_new_sketch() {
    let frame = PlaneFrame { origin: [0.0, 0.0, 0.0], x: [1.0, 0.0, 0.0], y: [0.0, 1.0, 0.0], normal: [0.0, 0.0, 1.0] };
    let outer: Entity = serde_json::from_value(json!({"id": "outer", "type": "rect", "x": 0, "y": 0, "w": 20, "h": 16})).unwrap();
    let inner: Entity = serde_json::from_value(json!({"id": "inner", "type": "rect", "x": 5, "y": 4, "w": 8, "h": 6})).unwrap();
    let session = SketchSession::new(frame, json!("XY"), None, Some("Sketch".into()), vec![outer, inner], Vec::new(), Vec::new(), None);
    let doc = Document::new(Registry::builtin());
    let data = json!({"entities": session.entities, "seed": session.seed});
    let result = doc.sketch_op("faces", &data).expect("faces");
    let faces = result["faces"].as_array().expect("faces");
    assert_eq!(faces.len(), 2, "{result}");
    // A point in the wall between the rectangles, and a point in the hole, as Extrude's region field.
    let wall = [1.0, 1.0];
    let hole = [9.0, 7.0];
    let wall_hits = faces.iter().filter(|f| in_poly(wall, f.as_array().unwrap())).count();
    let hole_hits = faces.iter().filter(|f| in_poly(hole, f.as_array().unwrap())).count();
    assert_eq!(wall_hits, 1, "wall point should pick one region: {result}");
    assert_eq!(hole_hits, 1, "hole point should pick the inner region: {result}");
}
