//! Task 5 of milestone 4c: relations, dimensions and solving. The 1.x values below were computed
//! with `web/js/dims.js` and `web/js/constraints.js` in Node (numbers are compared to 1e-9).

// keeps this test binary out of the real 2.0 folders (STLEDITOR2_CONFIG_DIR)
#[path = "support/test_env.rs"]
mod test_env;

use std::collections::BTreeMap;

use serde_json::{json, Value};
use stl_app::engine_link::{Command, Level};
use stl_app::sketch::dims::{self, Layout};
use stl_app::sketch::solve::{self, Def};
use stl_app::sketch::{relations, Constraint, Entity, Ref, RefEnt, SketchEffect, SketchInput, SketchSession, ToolCx};
use stl_app::viewport::{PlaneFrame, Viewport};
use stl_sketch::solver::World;

const DIMS_1X: &str = r#"{"layouts":{"length":{"lines":[[[-3.7947331922020546,11.384199576606164],[26.205266807797948,21.384199576606164]]],"thin":[[[-0.12649110640673517,0.37947331922020555],[-4.016092628413841,12.048277885241523]],[[29.873508893593264,10.379473319220205],[25.98390737158616,22.048277885241525]]],"arrows":[{"at":[-3.7947331922020546,11.384199576606164],"d":[-0.9486832980505138,-0.3162277660168379]},{"at":[26.205266807797948,21.384199576606164],"d":[0.9486832980505138,0.3162277660168379]}],"arcs":[],"text":[11.205266807797946,16.384199576606164]},"length_short":{"lines":[[[1.581138830084189,-4.743416490252568],[31.58113883008419,5.25658350974743]],[[31.58113883008419,5.25658350974743],[54.52847075210474,12.905694150420947]]],"thin":[[[0.12649110640673517,-0.37947331922020555],[1.8024982662959756,-5.407494798887928]],[[30.126491106406736,9.620526680779795],[31.802498266295977,4.59250520111207]]],"arrows":[{"at":[1.581138830084189,-4.743416490252568],"d":[-0.9486832980505138,-0.3162277660168379]},{"at":[31.58113883008419,5.25658350974743],"d":[0.9486832980505138,0.3162277660168379]}],"arcs":[],"text":[54.52847075210474,12.905694150420947]},"hdist":{"lines":[[[0,13],[30,13]]],"thin":[[[0,0.4],[0,13.7]],[[30,10.4],[30,13.7]]],"arrows":[{"at":[0,13],"d":[-1,0]},{"at":[30,13],"d":[1,0]}],"arcs":[],"text":[15,13]},"vdist":{"lines":[[[22,0],[22,10]]],"thin":[[[0.4,0],[22.7,0]],[[29.6,10],[21.3,10]]],"arrows":[{"at":[22,0],"d":[0,-1]},{"at":[22,10],"d":[0,1]}],"arcs":[],"text":[22,5]},"dist_line":{"lines":[[[7.846049894151543,5.948683298050513],[8.846049894151541,2.9486832980505127]],[[7.846049894151543,5.948683298050513],[7.1503488089145,8.035786553761644]],[[8.846049894151541,2.9486832980505127],[9.541750979388583,0.8615800423393818]]],"thin":[[[5.379473319220206,5.126491106406735],[8.510128202786902,6.170042734262299]],[[6.379473319220205,2.1264911064067347],[9.5101282027869,3.170042734262299]]],"arrows":[{"at":[7.846049894151543,5.948683298050513],"d":[0.3162277660168376,-0.9486832980505139]},{"at":[8.846049894151541,2.9486832980505127],"d":[-0.3162277660168376,0.9486832980505139]}],"arcs":[],"text":[8.346049894151541,4.448683298050513]},"diameter":{"lines":[[[1.5358983848622452,3],[11.062177826491071,8.5]]],"thin":[],"arrows":[{"at":[8.464101615137755,7],"d":[0.8660254037844387,0.49999999999999994]},{"at":[1.5358983848622452,3],"d":[-0.8660254037844387,-0.49999999999999994]}],"arcs":[],"text":[11.062177826491071,8.5]},"radius_in":{"lines":[[[5,5],[1.2412295168563663,3.6319194266973254]]],"thin":[],"arrows":[{"at":[1.2412295168563663,3.6319194266973254],"d":[-0.9396926207859084,-0.34202014332566866]}],"arcs":[],"text":[3.120614758428183,4.315959713348663]},"radius_arc":{"lines":[[[0,0],[-11.276311449430901,-4.104241719908024]]],"thin":[],"arrows":[{"at":[-9.396926207859085,-3.4202014332566866],"d":[-0.9396926207859084,-0.34202014332566866]}],"arcs":[{"c":[0,0],"r":10,"a0":1.5707963267948966,"a1":3.490658503988659,"thin":true}],"text":[-11.276311449430901,-4.104241719908024]},"angle":{"lines":[],"thin":[],"arrows":[{"at":[4.743416490252569,1.5811388300841898],"d":[0.31622776601683794,-0.9486832980505138]},{"at":[3.5355339059327378,3.5355339059327373],"d":[-0.7071067811865475,0.7071067811865476]}],"arcs":[{"c":[0,0],"r":5,"a0":0.3217505543966422,"a1":0.7853981633974483}],"text":[4.2532540417602,2.628655560595668]}},"default":{"length":{"off":2.6,"slide":0},"radius":{"ang":45,"out":1.8},"angle":{"off":4.6000000000000005},"hrect":{"off":-2.6,"slide":0}},"placed":{"length":{"off":4.743416490252569,"slide":0},"hdist":{"off":-8,"slide":35},"diameter":{"ang":60.25511870305778,"out":4.062257748298549},"angle":{"off":8.54400374531753}}}"#;
const REL_1X: &str = r#"{"rel":{"c_coincident":{"type":"coincident","a":{"e":"L1","k":"p2"},"b":{"e":"L2","k":"p1"}},"c_hv":{"type":"horizontal","a":{"e":"L3"}},"c_parallel":{"type":"parallel","a":{"e":"L1"},"b":{"e":"L3"}},"c_perpendicular":{"type":"perpendicular","a":{"e":"L1"},"b":{"e":"L2"}},"c_equal":{"type":"equal","a":{"e":"L1"},"b":{"e":"L3"}},"c_tangent":{"type":"tangent","a":{"e":"L1"},"b":{"e":"C1"}},"c_concentric":{"type":"concentric","a":{"e":"C1"},"b":{"e":"C2"}},"c_midpoint":{"type":"midpoint","a":{"e":"P","k":"p"},"b":{"e":"L1"}},"c_fix":{"type":"fix","a":{"e":"L2"},"at":[30,5,30,25]},"c_symmetric":{"type":"symmetric","a":{"e":"L1","k":"p1"},"b":{"e":"L1","k":"p2"},"c":{"e":"L3"}}},"dim":{"circle":{"type":"diameter","a":{"e":"C1"},"ang":-90,"out":12,"value":16},"hdist":{"type":"hdistance","a":{"e":"L1","k":"p1"},"b":{"e":"L1","k":"p2"},"sign":1,"off":-6,"slide":0,"value":20},"angle":{"type":"angle","a":{"e":"L1"},"b":{"e":"L2"},"ra":1,"rb":-1,"sign":-1,"off":3.6055512754639896,"value":90}}}"#;

fn frame() -> PlaneFrame {
    PlaneFrame { origin: [0.0; 3], x: [1.0, 0.0, 0.0], y: [0.0, 1.0, 0.0], normal: [0.0, 0.0, 1.0] }
}

fn ent(v: Value) -> Entity {
    serde_json::from_value(v).unwrap()
}

fn con(v: Value) -> Constraint {
    serde_json::from_value(v).unwrap()
}

fn base_ents() -> Vec<Entity> {
    vec![
        ent(json!({"id": "L1", "type": "line", "p1": [0, 0], "p2": [20, 0]})),
        ent(json!({"id": "L2", "type": "line", "p1": [30, 5], "p2": [30, 25]})),
        ent(json!({"id": "L3", "type": "line", "p1": [0, 40], "p2": [20, 41]})),
        ent(json!({"id": "C1", "type": "circle", "cx": 60, "cy": 0, "r": 8})),
        ent(json!({"id": "C2", "type": "circle", "cx": 60, "cy": 30, "r": 8})),
        ent(json!({"id": "P", "type": "point", "x": 80, "y": -20})),
    ]
}

fn session_with(entities: Vec<Entity>, constraints: Vec<Constraint>, refs: Vec<RefEnt>) -> SketchSession {
    let mut s = SketchSession::new(frame(), json!("xy"), None, None, entities, constraints, refs, None);
    s.seed = "t5".into();
    s
}

fn session() -> SketchSession {
    session_with(base_ents(), vec![], vec![])
}

fn viewport() -> Viewport {
    let mut vp = Viewport::new();
    vp.set_view_size(800.0, 600.0);
    vp.look_at_plane(&frame(), Some(100.0));
    vp.flush();
    vp.camera_mut().finish_flight();
    vp
}

fn pos() -> egui::Pos2 {
    egui::Pos2::ZERO
}

type Tool = Box<dyn stl_app::sketch::SketchTool>;

/// Drive a tool with a pointer event; the effects it produced.
fn send(tool: &mut Tool, s: &mut SketchSession, vp: &Viewport, ev: SketchInput) -> Vec<SketchEffect> {
    let params = BTreeMap::new();
    let mut out = Vec::new();
    {
        let mut cx = ToolCx { s, vp, inch: false, params: &params, out: &mut out };
        tool.input(&mut cx, &ev);
    }
    out
}

fn down(uv: [f64; 2]) -> SketchInput {
    SketchInput::Down { uv, pos: pos(), mods: egui::Modifiers::NONE }
}
fn mv(uv: [f64; 2]) -> SketchInput {
    SketchInput::Move { uv, pos: pos(), mods: egui::Modifiers::NONE }
}

fn close(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            assert!((x - y).abs() < 1e-9, "{path}: {x} vs {y}");
        }
        (Value::Array(x), Value::Array(y)) => {
            assert_eq!(x.len(), y.len(), "{path}: length");
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                close(p, q, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(x), Value::Object(y)) => {
            let mut kx: Vec<_> = x.keys().collect();
            let mut ky: Vec<_> = y.keys().collect();
            kx.sort();
            ky.sort();
            assert_eq!(kx, ky, "{path}: keys");
            for (k, v) in x {
                close(v, &y[k], &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}

/// A constraint as JSON without its id.
fn no_id(c: &Constraint) -> Value {
    let mut v = serde_json::to_value(c).unwrap();
    v.as_object_mut().unwrap().remove("id");
    v
}

fn layout_json(l: &Layout) -> Value {
    json!({
        "lines": l.lines, "thin": l.thin,
        "arrows": l.arrows.iter().map(|a| json!({"at": a.at, "d": a.d})).collect::<Vec<_>>(),
        "arcs": l.arcs.iter().map(|a| {
            let mut v = json!({"c": a.c, "r": a.r, "a0": a.a0, "a1": a.a1});
            if a.thin {
                v["thin"] = json!(true); // 1.x only sets it on the thin ones
            }
            v
        }).collect::<Vec<_>>(),
        "text": l.text,
    })
}

fn is_warning(fx: &[SketchEffect], text: &str) -> bool {
    matches!(fx, [SketchEffect::Toast(Level::Warning, m)] if m == text)
}

// ------------------------------------------------------------------ relations

/// Where to click for each pick of a relation (points at the key point, shapes away from them).
fn click_points(tool: &str) -> Vec<[f64; 2]> {
    match tool {
        "c_coincident" => vec![[20.0, 0.0], [30.0, 5.0]],
        "c_hv" => vec![[10.0, 40.5]],
        "c_parallel" | "c_equal" => vec![[10.0, 0.0], [10.0, 40.5]],
        "c_perpendicular" => vec![[10.0, 0.0], [30.0, 15.0]],
        "c_tangent" => vec![[10.0, 0.0], [68.0, 0.0]],
        "c_concentric" => vec![[68.0, 0.0], [68.0, 30.0]],
        "c_midpoint" => vec![[80.0, -20.0], [10.0, 0.0]],
        "c_fix" => vec![[30.0, 15.0]],
        "c_symmetric" => vec![[0.0, 0.0], [20.0, 0.0], [10.0, 40.5]],
        _ => unreachable!(),
    }
}

#[test]
fn every_relation_tool_makes_the_1x_constraint() {
    let want: Value = serde_json::from_str::<Value>(REL_1X).unwrap()["rel"].clone();
    let vp = viewport();
    assert!(vp.pixel_size([0.0; 3]) < 0.5, "the test view is too zoomed out");
    for (key, _, hint) in relations::CTOOLS.iter().filter(|(k, _, _)| *k != "c_dimension") {
        let mut s = session();
        let mut tool = relations::tool_for(key).unwrap_or_else(|| panic!("no tool {key}"));
        assert_eq!(tool.key(), *key);
        assert_eq!(tool.hint(&s), *hint);
        assert!(tool.picks_geometry());
        let pts = click_points(key);
        let mut last = Vec::new();
        for (i, p) in pts.iter().enumerate() {
            last = send(&mut tool, &mut s, &vp, down(*p));
            if i + 1 < pts.len() {
                assert!(
                    matches!(last.as_slice(), [SketchEffect::Hint(h)] if *h == format!("{hint} · {} picked", i + 1)),
                    "{key}: pick {}",
                    i + 1
                );
            }
        }
        assert!(last.iter().any(|e| matches!(e, SketchEffect::Changed)), "{key}: not added ({} effects)", last.len());
        assert_eq!(s.constraints.len(), 1, "{key}");
        let c = &s.constraints[0];
        assert!(c.id.as_ref().and_then(Value::as_str).is_some_and(|i| i.starts_with("et5")), "{key}: id");
        close(&no_id(c), &want[key], key);
        assert_eq!(s.history.len(), 1);
        assert_eq!(tool.hint(&s), *hint, "picks are cleared after the constraint");
    }
}

#[test]
fn relation_messages_follow_1x() {
    let vp = viewport();
    let mut s = session();
    // Fix on the axis: it is fixed already.
    let mut t = relations::tool_for("c_fix").unwrap();
    let fx = send(&mut t, &mut s, &vp, down([-30.0, 0.0]));
    assert!(is_warning(&fx, "Part edges and the origin are fixed already."), "{}", fx.len());
    // Coincident of two shapes (not points): the solver's message.
    let mut t = relations::tool_for("c_coincident").unwrap();
    send(&mut t, &mut s, &vp, down([10.0, 0.0]));
    let fx = send(&mut t, &mut s, &vp, down([30.0, 15.0]));
    assert!(is_warning(&fx, "Pick two points, or a point and a line / circle."));
    assert!(s.constraints.is_empty());
    // Two things that never move.
    let refs = vec![RefEnt { ent: ent(json!({"id": "r1", "type": "line", "p1": [0, -10], "p2": [20, -10]})), missing: false }];
    let mut s = session_with(vec![], vec![], refs);
    let mut t = relations::tool_for("c_parallel").unwrap();
    send(&mut t, &mut s, &vp, down([10.0, -10.0]));
    let fx = send(&mut t, &mut s, &vp, down([-30.0, 0.0]));
    assert!(is_warning(&fx, "Pick at least one shape of the sketch: part edges and the origin don't move."), "{}", fx.len());
}

#[test]
fn conflicting_relation_is_refused() {
    let vp = viewport();
    let mut s = session();
    s.constraints.push(con(json!({"id": "k1", "type": "perpendicular", "a": {"e": "L1"}, "b": {"e": "L2"}})));
    solve::solve_all(&mut s);
    let mut t = relations::tool_for("c_parallel").unwrap();
    send(&mut t, &mut s, &vp, down([10.0, 0.0]));
    let fx = send(&mut t, &mut s, &vp, down([30.0, 15.0]));
    assert!(is_warning(&fx, "That would conflict with the existing constraints (over-constrained)."));
    assert_eq!(s.constraints.len(), 1);
    assert!(s.history.is_empty());
}

#[test]
fn delete_relation() {
    let mut s = session();
    s.constraints.push(con(json!({"id": "k1", "type": "horizontal", "a": {"e": "L3"}})));
    s.constraints.push(con(json!({"id": "k2", "type": "parallel", "a": {"e": "L1"}, "b": {"e": "L3"}})));
    s.sel_c = Some(1);
    let fx = relations::delete_constraint(&mut s, 1);
    assert!(matches!(fx.as_slice(), [SketchEffect::Changed]));
    assert_eq!(s.constraints.len(), 1);
    assert_eq!(s.constraints[0].kind, "horizontal");
    assert_eq!(s.sel_c, None);
    assert!(s.undo_local());
    assert_eq!(s.constraints.len(), 2);
}

// ------------------------------------------------------------------ Smart Dimension

#[test]
fn smart_dimension_linear_radial_angle() {
    let want: Value = serde_json::from_str::<Value>(REL_1X).unwrap()["dim"].clone();
    let vp = viewport();
    let tool = relations::tool_for("c_dimension").unwrap();
    assert!(!tool.picks_geometry());
    let mut tool = tool;

    // A circle: the diameter follows the pointer, the next click places it.
    let mut s = session();
    let fx = send(&mut tool, &mut s, &vp, down([68.0, 0.0]));
    assert!(matches!(fx.as_slice(), [SketchEffect::Hint(h)] if h == "Move to place the dimension and click · or click a second item (point, line, circle)"));
    assert!(tool.picks_geometry());
    send(&mut tool, &mut s, &vp, mv([60.0, -20.0]));
    let fx = send(&mut tool, &mut s, &vp, down([60.0, -20.0]));
    assert!(fx.iter().any(|e| matches!(e, SketchEffect::Dialog(stl_app::sketch::SketchDialog::DimValue(0)))));
    assert!(fx.iter().any(|e| matches!(e, SketchEffect::Changed)));
    close(&no_id(&s.constraints[0]), &want["circle"], "circle");
    assert_eq!(s.sel_c, Some(0));

    // Two points of a line: a horizontal distance (the pointer is beside the line).
    let mut tool = relations::tool_for("c_dimension").unwrap();
    let mut s = session();
    let fx = send(&mut tool, &mut s, &vp, down([0.0, 0.0]));
    assert!(matches!(fx.as_slice(), [SketchEffect::Hint(h)] if h == "Now click a second item: a point, line or circle"));
    send(&mut tool, &mut s, &vp, down([20.0, 0.0]));
    send(&mut tool, &mut s, &vp, mv([10.0, -6.0]));
    send(&mut tool, &mut s, &vp, down([10.0, -6.0]));
    close(&no_id(&s.constraints[0]), &want["hdist"], "hdist");

    // Two lines: the angle between them.
    let mut tool = relations::tool_for("c_dimension").unwrap();
    let mut s = session();
    send(&mut tool, &mut s, &vp, down([7.0, 0.0]));
    send(&mut tool, &mut s, &vp, down([30.0, 12.0]));
    send(&mut tool, &mut s, &vp, mv([32.0, -3.0]));
    send(&mut tool, &mut s, &vp, down([32.0, -3.0]));
    close(&no_id(&s.constraints[0]), &want["angle"], "angle");
}

#[test]
fn dimension_to_part_edge() {
    let vp = viewport();
    let edge = || RefEnt { ent: ent(json!({"id": "r1", "type": "line", "p1": [0, -10], "p2": [20, -10]})), missing: false };
    let mut s = session_with(base_ents(), vec![], vec![edge()]);
    let mut tool = relations::tool_for("c_dimension").unwrap();
    send(&mut tool, &mut s, &vp, down([20.0, 0.0])); // L1's end
    send(&mut tool, &mut s, &vp, down([10.0, -10.0])); // the part edge
    send(&mut tool, &mut s, &vp, mv([-5.0, -5.0]));
    send(&mut tool, &mut s, &vp, down([-5.0, -5.0]));
    let c = &s.constraints[0];
    assert_eq!(c.kind, "distance");
    assert_eq!((c.a.clone().unwrap(), c.b.clone().unwrap()), (Ref::pt("L1", "p2"), Ref::ent("r1")));
    assert!((c.value.unwrap() - 10.0).abs() < 1e-9);
    // The step keeps the part edge it measures from.
    let used = s.step_params()["refs"].clone();
    assert_eq!(used.as_array().unwrap().len(), 1);
    assert_eq!(used[0]["id"], "r1");

    // Two things that never move: the same message as 1.x.
    let mut s = session_with(vec![], vec![], vec![edge()]);
    let mut tool = relations::tool_for("c_dimension").unwrap();
    send(&mut tool, &mut s, &vp, down([10.0, -10.0]));
    let fx = send(&mut tool, &mut s, &vp, down([-30.0, 0.0]));
    assert!(is_warning(&fx, "Pick at least one shape of the sketch: the origin, the axes and the part edges don't move."), "{}", fx.len());
}

#[test]
fn dimension_with_parameter() {
    let params: BTreeMap<String, f64> = [("width".to_string(), 20.0)].into();
    assert_eq!(dims::parse_dim("=width/2", false, false, &params), Ok((10.0, Some("=width/2".to_string()))));
    assert_eq!(dims::parse_dim("width", false, false, &params), Ok((20.0, Some("=width".to_string()))));
    assert_eq!(dims::parse_dim("12.5", false, false, &params), Ok((12.5, None)));
    assert_eq!(dims::parse_dim("1 3/8", false, true, &params), Ok((34.925, None)));
    assert_eq!(dims::parse_dim("45", true, true, &params), Ok((45.0, None)));
    assert_eq!(dims::parse_dim("((", false, false, &params).unwrap_err(), "Type a number, or a parameter (like width or =width/2).");
    assert!(dims::parse_dim("=nope", false, false, &params).is_err());

    // Storing it: the dimension keeps the expression, and the sketch follows the value.
    let mut s = session_with(
        vec![ent(json!({"id": "L", "type": "line", "p1": [0, 0], "p2": [30, 0]}))],
        vec![con(json!({"id": "d", "type": "length", "a": {"e": "L"}, "value": 30}))],
        vec![],
    );
    let fx = dims::set_dim_value(&mut s, 0, 10.0, Some("=width/2".into()));
    assert!(matches!(fx.as_slice(), [SketchEffect::Changed]));
    assert_eq!(s.step_params()["constraints"][0]["expr"], "=width/2");
    let Entity::Line(l) = &s.entities[0] else { panic!() };
    assert!((((l.p2[0] - l.p1[0]).powi(2) + (l.p2[1] - l.p1[1]).powi(2)).sqrt() - 10.0).abs() < 1e-6);
    assert_eq!(dims::dim_class(&s.constraints[0], false, false, false), "expr  ");
    // A plain number drops the expression.
    dims::set_dim_value(&mut s, 0, 12.0, None);
    assert!(s.step_params()["constraints"][0].get("expr").is_none());
}

#[test]
fn dimension_text_inch() {
    let ents = vec![
        ent(json!({"id": "L", "type": "line", "p1": [0, 0], "p2": [12.7, 0]})),
        ent(json!({"id": "C", "type": "circle", "cx": 0, "cy": 0, "r": 12.7})),
        ent(json!({"id": "M", "type": "line", "p1": [0, 0], "p2": [0, 10]})),
    ];
    let t = |c: Value, inch: bool, bare: bool| dims::dim_text(&con(c), &ents, inch, bare);
    let len = json!({"type": "length", "a": {"e": "L"}, "value": 12.7});
    assert_eq!(t(len.clone(), false, false), "12.7");
    assert_eq!(t(len, true, false), "0.5");
    assert_eq!(t(json!({"type": "diameter", "a": {"e": "C"}, "value": 25.4}), true, false), "⌀1");
    assert_eq!(t(json!({"type": "diameter", "a": {"e": "C"}, "value": 25.4}), false, false), "⌀25.4");
    assert_eq!(t(json!({"type": "radius", "a": {"e": "C"}, "value": 34.925}), true, false), "R1.375");
    assert_eq!(t(json!({"type": "angle", "a": {"e": "L"}, "b": {"e": "M"}, "value": 45.126}), true, false), "45.13°");
    // A reference dimension shows what the sketch measures, in brackets.
    let r = json!({"type": "length", "a": {"e": "L"}, "value": 99, "ref": true});
    assert_eq!(t(r.clone(), false, false), "(12.7)");
    assert_eq!(t(r.clone(), false, true), "12.7");
    assert_eq!(t(r, true, false), "(0.5)");
}

#[test]
fn dims_layout_matches_1x() {
    let want: Value = serde_json::from_str(DIMS_1X).unwrap();
    let ents = vec![
        ent(json!({"id": "L", "type": "line", "p1": [0, 0], "p2": [30, 10]})),
        ent(json!({"id": "M", "type": "line", "p1": [0, 0], "p2": [20, 20]})),
        ent(json!({"id": "C", "type": "circle", "cx": 5, "cy": 5, "r": 4})),
        ent(json!({"id": "A", "type": "arc", "cx": 0, "cy": 0, "r": 10, "a0": 0, "a1": 90})),
        ent(json!({"id": "R", "type": "rect", "x": 0, "y": 0, "w": 20, "h": 10})),
    ];
    let w = World::new(&ents);
    let px = 0.1;
    let cases: BTreeMap<&str, Value> = [
        ("length", json!({"type": "length", "a": {"e": "L"}, "off": 12, "slide": 0})),
        ("length_short", json!({"type": "length", "a": {"e": "L"}, "off": -5, "slide": 40})),
        ("hdist", json!({"type": "hdistance", "a": {"e": "L", "k": "p1"}, "b": {"e": "L", "k": "p2"}, "off": 8, "slide": 0})),
        ("vdist", json!({"type": "vdistance", "a": {"e": "L", "k": "p1"}, "b": {"e": "L", "k": "p2"}, "off": -7, "slide": 0})),
        ("dist_line", json!({"type": "distance", "a": {"e": "C", "k": "c"}, "b": {"e": "L"}, "off": 3, "slide": 0})),
        ("diameter", json!({"type": "diameter", "a": {"e": "C"}, "ang": 30, "out": 3})),
        ("radius_in", json!({"type": "radius", "a": {"e": "C"}, "ang": 200, "out": -2})),
        ("radius_arc", json!({"type": "radius", "a": {"e": "A"}, "ang": 200, "out": 2})),
        ("angle", json!({"type": "angle", "a": {"e": "L"}, "b": {"e": "M"}, "off": 5, "ra": 1, "rb": 1, "sign": 1})),
    ]
    .into();
    for (k, c) in &cases {
        let l = dims::layout(&con(c.clone()), &w, px).unwrap_or_else(|| panic!("{k}: no layout"));
        close(&layout_json(&l), &want["layouts"][k], k);
    }
    // Default placements (dims.js defaultPlacement), as the keys a dimension stores.
    let dp = |c: Value| -> Value {
        let mut m = serde_json::Map::new();
        for (k, v) in dims::default_placement(&con(c), &w, px) {
            m.insert(k.into(), json!(v));
        }
        Value::Object(m)
    };
    close(&dp(json!({"type": "length", "a": {"e": "L"}})), &want["default"]["length"], "default.length");
    close(&dp(json!({"type": "radius", "a": {"e": "C"}})), &want["default"]["radius"], "default.radius");
    close(&dp(json!({"type": "angle", "a": {"e": "L"}, "b": {"e": "M"}})), &want["default"]["angle"], "default.angle");
    close(&dp(json!({"type": "hdistance", "a": {"e": "R", "k": "c0"}, "b": {"e": "R", "k": "c1"}})), &want["default"]["hrect"], "default.hrect");
    // placeAt: where dragging the label to a point leaves off / slide / ang / out.
    for (k, cur) in [("length", [12.0, 9.0]), ("hdist", [50.0, -3.0]), ("diameter", [9.0, 12.0]), ("angle", [8.0, 3.0])] {
        let mut c = con(cases[k].clone());
        dims::place_at(&mut c, &w, cur);
        for key in ["off", "slide", "ang", "out"] {
            if let Some(v) = want["placed"][k].get(key).and_then(Value::as_f64) {
                let got = dims::get(&c, key).unwrap_or_else(|| panic!("{k}.{key} missing"));
                assert!((got - v).abs() < 1e-9, "{k}.{key}: {got} vs {v}");
            }
        }
    }
}

// ------------------------------------------------------------------ solving

fn line_sketch(extra: Vec<Value>) -> SketchSession {
    let mut s = session_with(
        vec![ent(json!({"id": "L", "type": "line", "p1": [1, 1], "p2": [9, 3]}))],
        vec![
            con(json!({"id": "c1", "type": "coincident", "a": {"e": "L", "k": "p1"}, "b": {"e": "_o", "k": "p"}})),
            con(json!({"id": "c2", "type": "horizontal", "a": {"e": "L"}})),
        ],
        vec![],
    );
    s.constraints.extend(extra.into_iter().map(con));
    s
}

#[test]
fn under_full_over() {
    // A bare circle: three free numbers.
    let mut s = session_with(vec![ent(json!({"id": "C", "type": "circle", "cx": 1, "cy": 1, "r": 2}))], vec![], vec![]);
    let st = solve::solve_all(&mut s);
    assert_eq!(st.def, Def::Under(3));
    assert_eq!(solve::def_text(&st), "Under defined: 3 more dimensions needed");
    assert!(st.conflicts.is_empty() && st.under.contains("C"));
    // One left.
    let mut s = line_sketch(vec![]);
    let st = solve::solve_all(&mut s);
    assert_eq!(st.def, Def::Under(1));
    assert_eq!(solve::def_text(&st), "Under defined: 1 more dimension needed");
    // Pinned down by a length.
    let mut s = line_sketch(vec![json!({"id": "d1", "type": "length", "a": {"e": "L"}, "value": 10})]);
    let st = solve::solve_all(&mut s);
    assert_eq!(st.def, Def::Full);
    assert_eq!(solve::def_text(&st), "Fully defined");
    assert!(st.conflicts.is_empty() && st.under.is_empty());
    let Entity::Line(l) = &s.entities[0] else { panic!() };
    assert!((l.p2[0] - 10.0).abs() < 1e-6 && l.p2[1].abs() < 1e-6 && l.p1[0].abs() < 1e-6, "{l:?}");
    // No shapes (text outlines don't count): nothing to say.
    let mut s = session_with(vec![ent(json!({"id": "T", "type": "polyline", "pts": [[0, 0], [1, 0], [1, 1]]}))], vec![], vec![]);
    assert_eq!(solve::def_text(&solve::solve_all(&mut s)), "");
}

#[test]
fn over_defined_reported() {
    let mut s = line_sketch(vec![
        json!({"id": "d1", "type": "length", "a": {"e": "L"}, "value": 10}),
        json!({"id": "d2", "type": "length", "a": {"e": "L"}, "value": 25}),
    ]);
    let st = solve::solve_all(&mut s);
    assert_eq!(st.def, Def::Over);
    assert!(!st.conflicts.is_empty());
    let n = st.conflicts.len();
    assert_eq!(solve::def_text(&st), format!("Over defined: {n} relation{} can't be met (red)", if n > 1 { "s" } else { "" }));
    // The conflicting dimension is one of the two lengths, and is marked "bad".
    let i = st.conflicts[0];
    assert_eq!(s.constraints[i].kind, "length", "{:?}", s.constraints[i]);
    assert_eq!(dims::dim_class(&s.constraints[i], false, true, false), "driving  bad");
    assert_eq!(dims::dim_class(&s.constraints[i], true, true, false), "driving sel bad");
    assert!(st.bad.contains("L"));
    // The painters see the same state.
    assert_eq!(solve::last(), st);
}

#[test]
fn request_and_apply_match_solve_all() {
    let mut a = line_sketch(vec![json!({"id": "d1", "type": "length", "a": {"e": "L"}, "value": 10})]);
    a.locks.push(stl_sketch::solver::Lock { e: "L".into(), k: Some("p2".into()), at: Some(vec![12.0, 1.0]), path: None, value: None });
    let mut b = a.clone();
    let Command::Solve { seq, entities, constraints, locks, fixed } = solve::request(&b, 7) else { panic!("not a Solve") };
    assert_eq!((seq, entities.len(), constraints.len(), locks.len()), (7, 1, 3, 1));
    // The origin, then the (no) part edges, then the two axes.
    let ids: Vec<_> = fixed.iter().map(|e| e.id().unwrap().to_string()).collect();
    assert_eq!(ids, ["_o", "_h", "_v"]);
    let sync = solve::solve_all(&mut a);
    let report = stl_sketch::solver::solve(&entities, &constraints, &locks, &fixed);
    let applied = solve::apply(&mut b, report.entities, report.ok, report.error);
    assert_eq!(a.entities, b.entities);
    assert_eq!(sync, applied);
}

#[test]
fn missing_ref_warned() {
    let edge = |id: &str, y: f64, missing: bool| RefEnt { ent: ent(json!({"id": id, "type": "line", "p1": [0, y], "p2": [20, y]})), missing };
    // One gone: the 1.x words (singular).
    let s = session_with(vec![], vec![], vec![edge("r1", -10.0, true), edge("r2", -20.0, false)]);
    let Some(SketchEffect::Toast(Level::Warning, m)) = relations::missing_toast(&s) else { panic!("no toast") };
    assert_eq!(m, "1 part edge this sketch measured from has changed (shown red). Its dimensions hold to where it was: delete them and dimension to the new edge.");
    // Two gone (plural).
    let s2 = session_with(vec![], vec![], vec![edge("r1", -10.0, true), edge("r2", -20.0, true)]);
    let Some(SketchEffect::Toast(Level::Warning, m)) = relations::missing_toast(&s2) else { panic!("no toast") };
    assert_eq!(m, "2 part edges this sketch measured from have changed (shown red). Its dimensions hold to where they were: delete them and dimension to the new edges.");
    // None gone: no toast.
    assert!(relations::missing_toast(&session_with(vec![], vec![], vec![edge("r1", -10.0, false)])).is_none());

    // The dimension to the missing edge holds: the edge stays where it was, the line is kept 7 away.
    let mut s = session_with(
        vec![ent(json!({"id": "L", "type": "line", "p1": [0, 0], "p2": [20, 0]}))],
        vec![
            con(json!({"id": "c1", "type": "horizontal", "a": {"e": "L"}})),
            con(json!({"id": "d1", "type": "distance", "a": {"e": "L", "k": "p1"}, "b": {"e": "r1"}, "value": 7})),
        ],
        vec![edge("r1", -10.0, true)],
    );
    assert!(solve::fixed_ents(&s).iter().any(|e| e.id() == Some("r1")), "a missing edge still holds its dimensions");
    solve::solve_all(&mut s);
    let Entity::Line(l) = &s.entities[0] else { panic!() };
    assert!((l.p1[1] - -3.0).abs() < 1e-6 || (l.p1[1] - -17.0).abs() < 1e-6, "{l:?}");
    assert_eq!(s.refs[0].ent, edge("r1", -10.0, true).ent);
    // It is kept in the step while a dimension uses it.
    assert_eq!(s.step_params()["refs"][0]["id"], "r1");
}

// ------------------------------------------------------------------ painting

struct Scene {
    s: SketchSession,
    vp: Viewport,
    effects: Vec<SketchEffect>,
}

fn scene(s: SketchSession) -> egui_kittest::Harness<'static, Scene> {
    egui_kittest::Harness::builder().with_size(egui::vec2(800.0, 600.0)).with_accessibility_check(false).build_ui_state(
        |ui, sc: &mut Scene| {
            stl_app::fonts::install(ui.ctx());
            let params = BTreeMap::new();
            let fx = relations::paint(ui, &mut sc.s, &sc.vp, false);
            sc.effects.extend(fx);
            let fx = dims::paint(ui, &mut sc.s, &sc.vp, false, &params);
            sc.effects.extend(fx);
        },
        Scene { s, vp: viewport(), effects: vec![] },
    )
}

#[test]
fn dimension_labels_and_chips_have_names() {
    use egui_kittest::kittest::Queryable;
    let mut s = session_with(
        vec![
            ent(json!({"id": "L", "type": "line", "p1": [0, 0], "p2": [20, 0]})),
            ent(json!({"id": "C", "type": "circle", "cx": 40, "cy": 0, "r": 5})),
        ],
        vec![
            con(json!({"id": "d1", "type": "length", "a": {"e": "L"}, "value": 20})),
            con(json!({"id": "d2", "type": "diameter", "a": {"e": "C"}, "value": 10, "ref": true})),
            con(json!({"id": "d3", "type": "radius", "a": {"e": "C"}, "value": 5, "expr": "=r", "ang": 200, "out": 30})),
            con(json!({"id": "k1", "type": "horizontal", "a": {"e": "L"}})),
        ],
        vec![RefEnt { ent: ent(json!({"id": "r1", "type": "line", "p1": [0, -10], "p2": [20, -10]})), missing: true }],
    );
    s.sel_c = Some(0);
    let mut h = scene(s);
    h.run();
    h.get_by_label("20");
    h.get_by_label("(⌀10)");
    h.get_by_label("R5");
    h.get_by_label("Horizontal relation");
    // Clicking a label selects its dimension.
    h.get_by_label("(⌀10)").click();
    h.run();
    assert_eq!(h.state().s.sel_c, Some(1));
    assert!(h.state().effects.iter().any(|e| matches!(e, SketchEffect::Changed)));
}

// ------------------------------------------------------------------ timing

/// The 200-entity / 100-constraint grid of `sketch_core::solver_timing`.
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
#[ignore = "timing harness: cargo test -p stl-app --test sketch_dims --release -- --ignored --nocapture analyze_timing"]
fn analyze_timing() {
    use std::time::Instant;
    use stl_sketch::solver::{analyze, is_redundant, unsatisfied};
    let (mut ents, cons) = grid();
    let fixed = solve::fixed_ents(&session());
    // Solved first, as it is when the definition line is worked out.
    ents = stl_sketch::solver::solve(&ents, &cons, &[], &fixed).entities;
    let extra = {
        let mut c = Constraint::new("hdistance");
        c.a = Some(Ref::pt("g0", "c0"));
        c.b = Some(Ref::pt("g0", "c1"));
        c.value = Some(10.4);
        c
    };
    type Job<'a> = (&'a str, Box<dyn Fn() + 'a>);
    let jobs: Vec<Job> = vec![
        ("analyze", Box::new(|| drop(analyze(&ents, &cons, &fixed)))),
        ("unsatisfied", Box::new(|| drop(unsatisfied(&ents, &cons, &fixed)))),
        ("analyze + unsatisfied (one update)", Box::new(|| {
            drop(analyze(&ents, &cons, &fixed));
            drop(unsatisfied(&ents, &cons, &fixed));
        })),
        ("is_redundant (placing a dimension)", Box::new(|| { let _ = is_redundant(&ents, &cons, &extra, &fixed); })),
    ];
    for (label, job) in &jobs {
        let mut times = Vec::new();
        for _ in 0..100 {
            let t = Instant::now();
            job();
            times.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        println!("{label}, {} entities, {} constraints: p50 {:.3} ms, p99 {:.3} ms", ents.len(), cons.len(), percentile(&mut times.clone(), 0.5), percentile(&mut times, 0.99));
    }
}
