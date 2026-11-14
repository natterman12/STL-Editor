//! Task 3 of milestone 4c: the drawing and modify tools, typed sizes, snapping.
//! Expected entities and constraints are worked out by hand from `web/js/sketcher.js`.

use std::collections::BTreeMap;

use serde_json::{json, Value};
use stl_app::sketch::snap::{self, SnapKind};
use stl_app::sketch::{draw, modify, typed, Constraint, Entity, Ref, RefEnt, SketchDialog, SketchEffect, SketchInput, SketchSession, SketchTool, ToolCx};
use stl_app::viewport::{PlaneFrame, Viewport};

fn frame() -> PlaneFrame {
    PlaneFrame { origin: [0.0, 0.0, 0.0], x: [1.0, 0.0, 0.0], y: [0.0, 1.0, 0.0], normal: [0.0, 0.0, 1.0] }
}

fn ent(v: Value) -> Entity {
    serde_json::from_value(v).unwrap()
}

struct H {
    s: SketchSession,
    vp: Viewport,
    tool: Box<dyn SketchTool>,
    out: Vec<SketchEffect>,
    params: BTreeMap<String, f64>,
    inch: bool,
}

impl H {
    fn new(key: &str, entities: Vec<Entity>) -> H {
        let mut s = SketchSession::new(frame(), json!("xy"), None, None, entities, vec![], vec![], None);
        s.seed = "g".into();
        s.tool = key.into();
        let mut vp = Viewport::new();
        vp.set_view_size(1000.0, 800.0);
        vp.look_at_plane(&frame(), Some(50.0));
        vp.camera_mut().finish_flight();
        snap::set_grid(true, 1.0);
        H { s, vp, tool: draw::tool_for(key).expect(key), out: Vec::new(), params: BTreeMap::new(), inch: false }
    }

    fn set_tool(&mut self, key: &str) {
        self.tool = draw::tool_for(key).expect(key);
        self.s.tool = key.into();
    }

    fn px(&self, uv: [f64; 2]) -> f64 {
        self.vp.pixel_size(self.s.to_3(uv))
    }

    fn send(&mut self, ev: SketchInput) -> bool {
        let mut cx = ToolCx { s: &mut self.s, vp: &self.vp, inch: self.inch, params: &self.params, out: &mut self.out };
        self.tool.input(&mut cx, &ev)
    }

    fn with_cx<R>(&mut self, f: impl FnOnce(&mut ToolCx) -> R) -> R {
        let mut cx = ToolCx { s: &mut self.s, vp: &self.vp, inch: self.inch, params: &self.params, out: &mut self.out };
        f(&mut cx)
    }

    fn pos(&self, uv: [f64; 2]) -> egui::Pos2 {
        self.vp.world_to_screen(self.s.to_3(uv))
    }

    fn mv(&mut self, uv: [f64; 2]) {
        let pos = self.pos(uv);
        self.send(SketchInput::Move { uv, pos, mods: egui::Modifiers::NONE });
    }

    fn click_mods(&mut self, uv: [f64; 2], mods: egui::Modifiers) {
        let pos = self.pos(uv);
        self.send(SketchInput::Move { uv, pos, mods });
        self.send(SketchInput::Down { uv, pos, mods });
        self.send(SketchInput::Up { uv, pos });
    }

    fn click(&mut self, uv: [f64; 2]) {
        self.click_mods(uv, egui::Modifiers::NONE);
    }

    fn key(&mut self, key: egui::Key) -> bool {
        self.send(SketchInput::Key { key, mods: egui::Modifiers::NONE })
    }

    fn typed(&mut self, t: &str) -> bool {
        self.send(SketchInput::Typed(t.into()))
    }

    fn ents(&self) -> Value {
        serde_json::to_value(&self.s.entities).unwrap()
    }

    fn cons(&self) -> Value {
        serde_json::to_value(&self.s.constraints).unwrap()
    }

    fn toasts(&self) -> Vec<String> {
        self.out.iter().filter_map(|e| if let SketchEffect::Toast(_, t) = e { Some(t.clone()) } else { None }).collect()
    }

    fn ops(&self) -> Vec<(String, Value)> {
        self.out.iter().filter_map(|e| if let SketchEffect::SketchOp { op, data } = e { Some((op.clone(), data.clone())) } else { None }).collect()
    }

    fn changed(&self) -> bool {
        self.out.iter().any(|e| matches!(e, SketchEffect::Changed))
    }
}

/// Equal JSON, numbers within 1e-9 (so 6 and 6.0 match).
fn same(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            assert!((x - y).abs() < 1e-9, "{path}: {x} != {y}");
        }
        (Value::Array(x), Value::Array(y)) => {
            assert_eq!(x.len(), y.len(), "{path}: lengths {a} vs {b}");
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                same(p, q, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(x), Value::Object(y)) => {
            let kx: Vec<_> = x.keys().collect();
            let mut ky: Vec<_> = y.keys().collect();
            let mut kx2 = kx.clone();
            kx2.sort();
            ky.sort();
            assert_eq!(kx2, ky, "{path}: keys {a} vs {b}");
            for k in kx {
                same(&x[k], &y[k], &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}

// ---------------------------------------------------------------- one test per draw tool

#[test]
fn draw_line() {
    let mut h = H::new("line", vec![]);
    assert!(h.px([0.0, 0.0]) < 0.3, "the test zoom keeps the points apart");
    h.click([0.0, 0.0]);
    h.click([10.0, 0.0]);
    h.click([10.0, 5.0]);
    assert!(h.key(egui::Key::Enter));
    same(
        &h.ents(),
        &json!([
            {"id": "eg1", "type": "line", "p1": [0, 0], "p2": [10, 0]},
            {"id": "eg4", "type": "line", "p1": [10, 0], "p2": [10, 5]},
        ]),
        "entities",
    );
    // the first point snapped to the origin; every segment after the first joins the last
    same(
        &h.cons(),
        &json!([
            {"id": "eg2", "type": "coincident", "a": {"e": "eg1", "k": "p1"}, "b": {"e": "_o", "k": "p"}, "auto": true},
            {"id": "eg3", "type": "horizontal", "a": {"e": "eg1"}, "auto": true},
            {"id": "eg5", "type": "coincident", "a": {"e": "eg1", "k": "p2"}, "b": {"e": "eg4", "k": "p1"}, "auto": true},
            {"id": "eg6", "type": "vertical", "a": {"e": "eg4"}, "auto": true},
        ]),
        "constraints",
    );
    assert!(h.tool.preview(&h.s).is_empty(), "the chain is finished");
}

#[test]
fn draw_arc3() {
    let mut h = H::new("arc3", vec![]);
    h.click([20.0, 0.0]);
    h.click([30.0, 0.0]);
    h.mv([25.0, 5.0]);
    assert_eq!(h.tool.preview(&h.s).len(), 1, "a rubber band while drawing");
    h.click([25.0, 5.0]);
    // through (20,0), (25,5), (30,0): centre (25,0), r 5, counter-clockwise 0° to 180°
    same(&h.ents(), &json!([{"id": "eg1", "type": "arc", "cx": 25, "cy": 0, "r": 5, "a0": 0, "a1": 180}]), "arc");
    assert!(h.s.constraints.is_empty());

    // three points in a line
    let mut h = H::new("arc3", vec![]);
    h.click([20.0, 0.0]);
    h.click([30.0, 0.0]);
    h.click([40.0, 0.0]);
    assert_eq!(h.toasts(), vec!["The three points are in a line."]);
    assert!(h.s.entities.is_empty());
}

#[test]
fn draw_rect() {
    let mut h = H::new("rect", vec![]);
    h.click([2.0, 3.0]);
    h.click([12.0, 8.0]);
    same(&h.ents(), &json!([{"id": "eg1", "type": "rect", "x": 2, "y": 3, "w": 10, "h": 5}]), "rect");
    assert_eq!(h.s.sel.iter().cloned().collect::<Vec<_>>(), vec!["eg1"]);
    assert!(h.changed());

    // a rectangle with no size is refused and the drawing goes on
    let mut h = H::new("rect", vec![]);
    h.click([2.0, 3.0]);
    h.click([12.0, 3.0]);
    assert_eq!(h.toasts(), vec!["Too small: give it a size."]);
    assert!(h.s.entities.is_empty());
    h.click([12.0, 8.0]);
    assert_eq!(h.s.entities.len(), 1);
}

#[test]
fn draw_rect_center() {
    let mut h = H::new("rect_center", vec![]);
    h.click([20.0, 20.0]);
    h.click([25.0, 23.0]);
    same(&h.ents(), &json!([{"id": "eg1", "type": "rect", "x": 15, "y": 17, "w": 10, "h": 6}]), "rect");
}

#[test]
fn draw_circle() {
    let mut h = H::new("circle", vec![]);
    h.click([5.0, 5.0]);
    h.click([8.0, 9.0]);
    same(&h.ents(), &json!([{"id": "eg1", "type": "circle", "cx": 5, "cy": 5, "r": 5}]), "circle");
}

#[test]
fn draw_polygon() {
    let mut h = H::new("polygon", vec![]);
    h.click([30.0, 30.0]);
    h.click([35.0, 30.0]);
    same(&h.ents(), &json!([{"id": "eg1", "type": "polygon", "cx": 30, "cy": 30, "r": 5, "n": 6, "angle": 0}]), "polygon");
    // typed radius and sides: the angle is 90°, and the next polygon keeps the side count
    h.click([60.0, 60.0]);
    assert!(h.typed("4"));
    assert!(h.key(egui::Key::Tab));
    assert!(h.typed("8"));
    assert!(h.key(egui::Key::Enter));
    same(&serde_json::to_value(&h.s.entities[1]).unwrap(), &json!({"id": "eg2", "type": "polygon", "cx": 60, "cy": 60, "r": 4, "n": 8, "angle": 90}), "typed");
    h.click([80.0, 80.0]);
    h.click([83.0, 84.0]);
    assert_eq!(serde_json::to_value(h.s.entities.last().unwrap()).unwrap()["n"].as_f64(), Some(8.0));
}

#[test]
fn draw_slot() {
    let mut h = H::new("slot", vec![]);
    h.click([40.0, 0.0]);
    h.click([50.0, 0.0]);
    h.click([45.0, 2.0]);
    same(&h.ents(), &json!([{"id": "eg1", "type": "slot", "x1": 40, "y1": 0, "x2": 50, "y2": 0, "r": 2}]), "slot");
}

// ---------------------------------------------------------------- typed sizes

#[test]
fn typed_size_mm() {
    let mut h = H::new("rect", vec![]);
    h.click([2.0, 3.0]);
    assert!(h.typed("20"));
    assert!(h.key(egui::Key::Tab));
    assert!(h.typed("10"));
    assert!(h.key(egui::Key::Enter));
    same(&h.ents(), &json!([{"id": "eg1", "type": "rect", "x": 2, "y": 3, "w": 20, "h": 10}]), "rect");
    let px = typed::plane_px(&h.s, &h.vp);
    // the bottom edge's dimension sits below the rectangle (away from its middle)
    same(
        &h.cons(),
        &json!([
            {"id": "eg2", "type": "hdistance", "a": {"e": "eg1", "k": "c0"}, "b": {"e": "eg1", "k": "c1"}, "sign": 1, "value": 20, "off": -26.0 * px, "slide": 0},
            {"id": "eg3", "type": "vdistance", "a": {"e": "eg1", "k": "c1"}, "b": {"e": "eg1", "k": "c2"}, "sign": 1, "value": 10, "off": -26.0 * px, "slide": 0},
        ]),
        "dims",
    );

    // Tab selects the other field's text: the next key replaces it
    let mut h = H::new("rect", vec![]);
    h.click([2.0, 3.0]);
    h.typed("20");
    h.key(egui::Key::Tab);
    h.typed("10");
    h.key(egui::Key::Tab);
    h.typed("30");
    h.key(egui::Key::Enter);
    same(&h.ents(), &json!([{"id": "eg1", "type": "rect", "x": 2, "y": 3, "w": 30, "h": 10}]), "replaced");

    // a circle's typed diameter
    let mut h = H::new("circle", vec![]);
    h.click([5.0, 5.0]);
    h.typed("12");
    h.key(egui::Key::Enter);
    let c: Vec<Constraint> = h.s.constraints.clone();
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].kind, "diameter");
    assert_eq!(c[0].value, Some(12.0));
    assert_eq!(c[0].extra["ang"], json!(45));

    // the live readout is not a typed size: no dimension
    let mut h = H::new("circle", vec![]);
    h.click([5.0, 5.0]);
    h.click([8.0, 9.0]);
    assert!(h.s.constraints.is_empty());
}

#[test]
fn typed_size_inch() {
    assert_eq!(typed::parse_expr("1/2"), 0.5);
    assert!((typed::parse_len("1 3/8", true) - 34.925).abs() < 1e-9);
    assert!((typed::parse_len("1/2", true) - 12.7).abs() < 1e-9);
    assert_eq!(typed::parse_expr("2*3+1"), 7.0);
    assert_eq!(typed::parse_expr("1,5"), 1.5);
    assert_eq!(typed::parse_expr("3 4"), 3.0, "a JS syntax error falls back to parseFloat");
    assert_eq!(typed::parse_expr("12mm"), 12.0);
    assert_eq!(typed::parse_expr("2\""), 2.0);
    assert_eq!(typed::parse_expr("-1 1/2"), -1.5);
    assert!(typed::parse_expr("abc").is_nan());

    // a circle: "1/2" in inch mode is a 12.7 mm diameter dimension
    let mut h = H::new("circle", vec![]);
    h.inch = true;
    h.click([5.0, 5.0]);
    h.typed("1/2");
    h.key(egui::Key::Enter);
    let Entity::Circle(c) = &h.s.entities[0] else { panic!("a circle") };
    assert!((c.r - 6.35).abs() < 1e-9);
    assert!((h.s.constraints[0].value.unwrap() - 12.7).abs() < 1e-9);

    // a line: "1 3/8" is 34.925 mm long, shown in inches
    let mut h = H::new("line", vec![]);
    h.inch = true;
    h.click([3.0, 20.0]);
    h.mv([13.0, 20.0]);
    h.typed("1 3/8");
    let hud: Vec<String> = h.out.iter().filter_map(|e| if let SketchEffect::Hud(Some(t)) = e { Some(t.clone()) } else { None }).collect();
    assert_eq!(hud.last().unwrap(), "Length\t1 3/8\nAngle\t0");
    h.key(egui::Key::Enter);
    let Entity::Line(l) = &h.s.entities[0] else { panic!("a line") };
    assert!((l.p2[0] - (3.0 + 34.925)).abs() < 1e-9 && (l.p2[1] - 20.0).abs() < 1e-9);
    let len = h.s.constraints.iter().find(|c| c.kind == "length").expect("a length dimension");
    assert!((len.value.unwrap() - 34.925).abs() < 1e-9);
    assert_eq!(typed::fmt_len(34.925, true), "1.375");
}

// ---------------------------------------------------------------- lines

#[test]
fn line_chain_finish() {
    // double-click (a browser sends click, click, dblclick)
    let mut h = H::new("line", vec![]);
    h.click([2.0, 2.0]);
    h.click([12.0, 2.0]);
    h.click([12.0, 2.0]);
    let pos = h.pos([12.0, 2.0]);
    assert!(h.send(SketchInput::DoubleClick { uv: [12.0, 2.0], pos }));
    assert_eq!(h.s.entities.len(), 1);
    assert!(h.tool.preview(&h.s).is_empty());
    assert!(!h.tool.picks_geometry());

    // Enter, and Escape
    for key in [egui::Key::Enter, egui::Key::Escape] {
        let mut h = H::new("line", vec![]);
        h.click([2.0, 2.0]);
        h.click([12.0, 2.0]);
        h.click([12.0, 7.0]);
        assert!(h.key(key));
        assert_eq!(h.s.entities.len(), 2, "{key:?} keeps the lines");
        assert!(h.tool.preview(&h.s).is_empty());
        assert!(!h.key(egui::Key::Escape), "Escape with nothing drawn goes to sketch mode");
    }

    // clicking the first point closes the chain
    let mut h = H::new("line", vec![]);
    for p in [[2.0, 2.0], [12.0, 2.0], [12.0, 8.0], [2.0, 2.0]] {
        h.click(p);
    }
    assert_eq!(h.s.entities.len(), 3);
    assert!(h.tool.preview(&h.s).is_empty(), "closed: the chain stops");
    let closing = &h.s.constraints;
    assert!(closing.iter().any(|c| c.kind == "coincident"
        && c.a == Some(Ref::pt("eg6", "p2"))
        && c.b == Some(Ref::pt("eg1", "p1"))));
}

#[test]
fn auto_constrain_horizontal_and_coincident() {
    let mut h = H::new("line", vec![]);
    h.click([2.0, 2.0]);
    h.click([12.0, 2.0]);
    h.key(egui::Key::Enter);
    assert_eq!(h.s.constraints.iter().map(|c| c.kind.as_str()).collect::<Vec<_>>(), vec!["horizontal"]);
    // a rectangle started on the line's end is tied to it
    h.set_tool("rect");
    h.click([12.0, 2.0]);
    h.click([20.0, 10.0]);
    let c = h.s.constraints.last().unwrap();
    same(
        &serde_json::to_value(c).unwrap(),
        &json!({"id": "eg4", "type": "coincident", "a": {"e": "eg3", "k": "c0"}, "b": {"e": "eg1", "k": "p2"}, "auto": true}),
        "corner",
    );
    // a circle centred on a part edge's corner is tied to the edge
    let mut h = H::new("circle", vec![]);
    h.s.refs = vec![RefEnt { ent: ent(json!({"id": "E1", "type": "line", "p1": [40, 0], "p2": [50, 0]})), missing: false }];
    h.click([40.0, 0.0]);
    h.click([43.0, 4.0]);
    same(
        &h.cons(),
        &json!([{"id": "eg2", "type": "coincident", "a": {"e": "eg1", "k": "c"}, "b": {"e": "E1", "k": "p1"}, "auto": true}]),
        "edge",
    );
}

// ---------------------------------------------------------------- modify

#[test]
fn drag_point_resolves() {
    let line = ent(json!({"id": "l", "type": "line", "p1": [0, 0], "p2": [10, 0]}));
    let mut h = H::new("select", vec![line]);
    let c: Vec<Constraint> = serde_json::from_value(json!([
        {"id": "c1", "type": "coincident", "a": {"e": "l", "k": "p1"}, "b": {"e": "_o", "k": "p"}},
        {"id": "c2", "type": "horizontal", "a": {"e": "l"}},
    ]))
    .unwrap();
    h.s.constraints = c;
    let (p0, p1) = (h.pos([10.0, 0.0]), h.pos([10.0, 5.0]));
    let mods = egui::Modifiers::NONE;
    assert!(h.send(SketchInput::Down { uv: [10.0, 0.0], pos: p0, mods }), "grabs the line's end");
    assert!(h.send(SketchInput::Move { uv: [10.0, 5.0], pos: p1, mods }));
    assert_eq!(serde_json::to_value(&h.s.locks).unwrap(), json!([{"e": "l", "k": "p2", "at": [10.0, 5.0]}]));
    assert!(h.changed());
    assert_eq!(h.s.history.len(), 1, "one undo step for the drag");
    // the dispatcher's solve: the relations win over the pull
    let r = stl_sketch::solver::solve(&h.s.entities, &h.s.constraints, &h.s.locks, &snap::fixed_ents(&h.s));
    assert!(r.ok);
    let Entity::Line(l) = &r.entities[0] else { panic!() };
    assert!(l.p1[0].abs() < 1e-6 && l.p1[1].abs() < 1e-6, "p1 stays on the origin: {:?}", l.p1);
    assert!((l.p2[1]).abs() < 1e-6, "p2 stays level: {:?}", l.p2);
    assert!((l.p2[0] - 10.0).abs() < 1e-6, "p2 follows the pointer along x: {:?}", l.p2);
    h.s.entities = r.entities;
    h.send(SketchInput::Up { uv: [10.0, 5.0], pos: p1 });
    assert!(h.s.locks.is_empty());
    assert!(h.s.sel.is_empty(), "a drag doesn't select");

    // dragging a whole shape locks all its points (a rectangle's centre is not one)
    let rect = ent(json!({"id": "r", "type": "rect", "x": 0, "y": 0, "w": 20, "h": 10}));
    let mut h = H::new("select", vec![rect]);
    let (a, b) = (h.pos([10.0, 10.0]), h.pos([12.0, 13.0]));
    h.send(SketchInput::Down { uv: [10.0, 10.0], pos: a, mods });
    h.send(SketchInput::Move { uv: [12.0, 13.0], pos: b, mods });
    let ks: Vec<(String, Vec<f64>)> = h.s.locks.iter().map(|l| (l.k.clone().unwrap(), l.at.clone().unwrap())).collect();
    assert_eq!(
        ks,
        vec![
            ("c0".into(), vec![2.0, 3.0]),
            ("c1".into(), vec![22.0, 3.0]),
            ("c2".into(), vec![22.0, 13.0]),
            ("c3".into(), vec![2.0, 13.0]),
        ]
    );
}

#[test]
fn select_click_and_shift() {
    let a = ent(json!({"id": "a", "type": "line", "p1": [0, 20], "p2": [10, 20]}));
    let b = ent(json!({"id": "b", "type": "circle", "cx": 40, "cy": 40, "r": 5}));
    let mut h = H::new("select", vec![a, b]);
    h.click([5.0, 20.0]);
    assert_eq!(h.s.sel.iter().cloned().collect::<Vec<_>>(), vec!["a"]);
    h.click_mods([45.0, 40.0], egui::Modifiers::SHIFT);
    assert_eq!(h.s.sel.len(), 2);
    h.click_mods([45.0, 40.0], egui::Modifiers::SHIFT);
    assert_eq!(h.s.sel.iter().cloned().collect::<Vec<_>>(), vec!["a"]);
    h.click([70.0, 70.0]);
    assert!(h.s.sel.is_empty(), "a click on nothing clears the selection");
}

fn golden(name: &str) -> Value {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/sketchop").join(format!("{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn without_seed(mut v: Value) -> Value {
    let seed = v.as_object_mut().unwrap().remove("seed");
    assert!(seed.is_some_and(|s| s.as_str().is_some_and(|s| !s.is_empty())), "a fresh seed");
    v
}

#[test]
fn trim_offset_mirror_send_ops() {
    // trim
    let g = golden("trim_line");
    let ents: Vec<Entity> = serde_json::from_value(g["data"]["entities"].clone()).unwrap();
    let mut h = H::new("trim", ents);
    h.s.constraints = serde_json::from_value(json!([{"id": "k", "type": "horizontal", "a": {"e": "a"}}])).unwrap();
    h.click([5.0, 0.0]);
    let ops = h.ops();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].0, "trim");
    let mut want = g["data"].clone();
    want.as_object_mut().unwrap().remove("seed");
    same(&without_seed(ops[0].1.clone()), &want, "trim data");
    assert!(h.s.entities.len() == 3, "nothing changes until the answer");
    h.with_cx(|cx| {
        let mut t = draw::tool_for("trim").unwrap();
        t.sketch_op_result(cx, "trim", &g["result"]);
    });
    same(&h.ents(), &g["result"]["entities"], "trimmed");
    assert!(h.s.constraints.is_empty(), "the relation on the cut line is gone");
    assert_eq!(h.s.history.len(), 1);

    // offset: asks the distance, then sends it
    let g = golden("offset_closed");
    let ents: Vec<Entity> = serde_json::from_value(g["data"]["entities"].clone()).unwrap();
    let mut h = H::new("select", ents);
    assert!(h.with_cx(|cx| modify::action("sk_offset", cx)));
    assert_eq!(h.toasts(), vec!["Select the shapes to offset first (Select tool, Shift-click for more)."]);
    h.s.sel.insert("r".into());
    h.with_cx(|cx| modify::action("sk_offset", cx));
    assert!(h.out.iter().any(|e| matches!(e, SketchEffect::Dialog(SketchDialog::Offset))));
    h.with_cx(|cx| modify::offset_send(cx, modify::OFFSET_DEFAULT));
    let ops = h.ops();
    assert_eq!(ops[0].0, "offset");
    let mut want = g["data"].clone();
    want.as_object_mut().unwrap().remove("seed");
    same(&without_seed(ops[0].1.clone()), &want, "offset data");
    // the answer comes back through whatever tool is active (here Select), once
    let mut cx = ToolCx { s: &mut h.s, vp: &h.vp, inch: false, params: &h.params, out: &mut h.out };
    h.tool.sketch_op_result(&mut cx, "offset", &g["result"]);
    same(&h.ents(), &g["result"]["entities"], "offset");
    assert_eq!(h.s.history.len(), 1);
    assert_eq!(modify::parse_num("1 3/8"), 1.0, "app.js parseNum has no fractions");
    assert_eq!(modify::parse_num("-2"), -2.0);
    assert_eq!(modify::parse_num("x"), 0.0);

    // mirror: the selection across a clicked line
    let g = golden("mirror_line");
    let ents: Vec<Entity> = serde_json::from_value(g["data"]["entities"].clone()).unwrap();
    let mut h = H::new("sk_mirror", ents);
    assert!(h.tool.picks_geometry());
    h.click([0.0, 5.0]);
    assert_eq!(h.toasts(), vec!["First select the shapes to mirror (Select tool), then choose Mirror and click the mirror line."]);
    h.s.sel.insert("c".into());
    h.click([11.0, 4.0]);
    assert_eq!(h.toasts().last().unwrap(), "Click a straight line to mirror across.");
    h.click([0.0, 5.0]);
    let ops = h.ops();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].0, "mirror");
    let mut want = g["data"].clone();
    want.as_object_mut().unwrap().remove("seed");
    same(&without_seed(ops[0].1.clone()), &want, "mirror data");
}

#[test]
fn round_corner() {
    let sq = ent(json!({"id": "p", "type": "polyline", "pts": [[0, 0], [10, 0], [10, 10], [0, 10]], "closed": true}));
    let mut h = H::new("sk_corner", vec![sq]);
    h.click([10.0, 10.0]);
    let Entity::Polyline(p) = &h.s.entities[0] else { panic!() };
    // a 90° corner, r 2: tangent points (10, 8) and (8, 10), centre (8, 8), 7 points between
    let mut want = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 8.0]];
    for j in 1..8 {
        let t = std::f64::consts::FRAC_PI_2 * j as f64 / 8.0;
        want.push([8.0 + 2.0 * t.cos(), 8.0 + 2.0 * t.sin()]);
    }
    want.extend([[8.0, 10.0], [0.0, 10.0]]);
    assert_eq!(p.pts.len(), want.len());
    for (a, b) in p.pts.iter().zip(&want) {
        assert!((a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9, "{a:?} vs {b:?}");
    }
    assert!(h.changed());
    assert_eq!(h.s.history.len(), 1);

    // the end of an open polyline, and a shape that isn't a polyline
    let open = ent(json!({"id": "o", "type": "polyline", "pts": [[0, 30], [10, 30], [10, 40]]}));
    let circ = ent(json!({"id": "c", "type": "circle", "cx": 50, "cy": 50, "r": 5}));
    let mut h = H::new("sk_corner", vec![open, circ]);
    h.click([0.0, 30.0]);
    h.click([55.0, 50.0]);
    assert_eq!(h.toasts(), vec!["Click on a corner in the middle of the shape.", "Click a corner of a polyline (a shape drawn as connected lines)."]);
}

#[test]
fn construction_toggle() {
    let l = ent(json!({"id": "l", "type": "line", "p1": [0, 0], "p2": [10, 0]}));
    let c = ent(json!({"id": "c", "type": "circle", "cx": 50, "cy": 50, "r": 5}));
    let mut h = H::new("select", vec![l, c]);
    h.s.sel.insert("l".into());
    assert!(h.with_cx(|cx| modify::action("construction", cx)));
    assert_eq!(h.ents()[0]["construction"], json!(true));
    assert!(h.ents()[1].get("construction").is_none(), "only the selection");
    h.with_cx(|cx| modify::toggle_construction(cx));
    assert_eq!(h.ents()[0]["construction"], json!(false));
    assert_eq!(h.s.history.len(), 2);

    // delete: the shape and the relations on it
    h.s.constraints = serde_json::from_value(json!([{"id": "k", "type": "horizontal", "a": {"e": "l"}}])).unwrap();
    assert!(h.with_cx(|cx| modify::action("sk_delete", cx)));
    assert_eq!(h.s.entities.len(), 1);
    assert!(h.s.constraints.is_empty() && h.s.sel.is_empty());
}

#[test]
fn snap_priorities() {
    let line = ent(json!({"id": "l", "type": "line", "p1": [20, 20], "p2": [30, 20]}));
    let circle = ent(json!({"id": "c", "type": "circle", "cx": 50, "cy": 50, "r": 5}));
    let same_spot = ent(json!({"id": "l2", "type": "line", "p1": [60, 0], "p2": [60, 10]}));
    let h = {
        let mut h = H::new("select", vec![line, circle, same_spot]);
        h.s.refs = vec![
            RefEnt { ent: ent(json!({"id": "E1", "type": "line", "p1": [60, 0], "p2": [70, 0]})), missing: false },
            RefEnt { ent: ent(json!({"id": "E2", "type": "line", "p1": [80, 0], "p2": [90, 0]})), missing: true },
        ];
        h
    };
    let px = h.px([20.0, 20.0]);
    let sn = |uv: [f64; 2]| snap::snap(&h.s, &h.vp, uv);

    // an end point within 10 px, not at 10.1 px
    let s = sn([20.0 + 9.9 * px, 20.0]);
    assert_eq!((s.uv, s.kind, s.on.clone()), ([20.0, 20.0], SnapKind::Point, Some(Ref::pt("l", "p1"))));
    assert!(s.snapped());
    assert_eq!(sn([20.0 - 10.1 * px, 20.3]).kind, SnapKind::Grid);
    // a line's midpoint
    assert_eq!(sn([25.0, 20.0 + 3.0 * px]).on, Some(Ref::pt("l", "m")));
    // a circle's centre ties the point; its quadrant points snap without a tie
    assert_eq!(sn([50.0 + 2.0 * px, 50.0]).on, Some(Ref::pt("c", "c")));
    let q = sn([55.0, 50.0 + 2.0 * px]);
    assert_eq!((q.uv, q.kind, q.on), ([55.0, 50.0], SnapKind::Point, None));
    // the origin, a part edge (missing ones too)
    assert_eq!(sn([2.0 * px, -2.0 * px]).on, Some(Ref::pt("_o", "p")));
    assert_eq!(sn([70.0, 2.0 * px]).on, Some(Ref::pt("E1", "p2")));
    assert_eq!(sn([90.0 - px, 0.0]).on, Some(Ref::pt("E2", "p2")));
    // where a sketch point and a part edge's corner coincide, the sketch's own point wins
    assert_eq!(sn([60.0, px]).on, Some(Ref::pt("l2", "p1")));
    // the grid, then no grid
    let g = sn([3.3, 7.6]);
    assert_eq!((g.uv, g.kind, g.on), ([3.0, 8.0], SnapKind::Grid, None));
    snap::set_grid(true, 5.0);
    assert_eq!(sn([3.3, 7.6]).uv, [5.0, 10.0]);
    snap::set_grid(false, 1.0);
    let f = sn([3.3, 7.6]);
    assert_eq!((f.uv, f.kind), ([3.3, 7.6], SnapKind::Free));
    snap::set_grid(true, 1.0);
    // the shape under the pointer: 8 px
    assert_eq!(snap::entity_at(&h.s, &h.vp, [25.0, 20.0 + 7.9 * px]), Some("l".into()));
    assert_eq!(snap::entity_at(&h.s, &h.vp, [25.0, 20.0 + 8.1 * px]), None);
}

#[test]
fn registry_covers_the_ribbon() {
    for k in ["line", "arc3", "rect", "rect_center", "circle", "polygon", "slot", "select", "trim", "sk_mirror", "sk_corner"] {
        let t = stl_app::sketch::tool_for(k).unwrap_or_else(|| panic!("{k}"));
        assert_eq!(t.key(), k);
        assert!(!draw::hint_for(k).is_empty(), "{k} has a hint");
    }
    for k in modify::ACTION_KEYS {
        assert!(draw::tool_for(k).is_none(), "{k} is an action, not a tool");
    }
}
