//! Milestone 7 Task 5 (spec 2.4): Hole points snap to sketch geometry and can be measured from
//! part edges; the references are saved with the step and shown again when it is reopened.
#![cfg(not(target_arch = "wasm32"))]

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use serde_json::{json, Value};
use stl_app::app::StlApp;
use stl_app::engine_link::native::NativeLink;
use stl_app::engine_link::Command;
use stl_app::viewport::pick::Hit;

use super::harness::{app_harness_ex, click_at, open_tool, settle};

type H = Harness<'static, StlApp>;

/// A 40 × 30 × 5 plate with its corner at the origin and a sketch on its top with one line from
/// (5, 5) to (35, 5), seen from the top.
fn plate_with_sketch() -> (H, std::sync::Arc<NativeLink>) {
    let (mut h, link) = app_harness_ex(None, egui::vec2(1280.0, 800.0));
    h.state_mut().link.send(Command::AddFeature {
        kind: "primitive".into(),
        params: json!({"shape": "box", "length": 40, "width": 30, "height": 5, "plane": "XY", "offset": [0, 0, 0], "rotate": [0, 0, 0], "anchor": "corner", "operation": "new", "targets": []}),
        index: None,
        name: Some("Plate".into()),
    });
    settle(&mut h, &link);
    h.state_mut().link.send(Command::AddFeature {
        kind: "sketch".into(),
        params: json!({"plane": {"origin": [0, 0, 5], "normal": [0, 0, 1]}, "entities": [{"type": "line", "id": "l1", "p1": [5, 5], "p2": [35, 5]}]}),
        index: None,
        name: Some("Sketch1".into()),
    });
    settle(&mut h, &link);
    assert_eq!(h.state().sketches.len(), 1, "the sketch is there");
    h.state_mut().viewport.set_view("top");
    h.state_mut().viewport.flush();
    h.run_steps(3);
    (h, link)
}

/// Click the part at a world point (it must be on the plate's top).
fn click_world(h: &mut H, p: [f64; 3]) {
    let at = h.state().viewport.world_to_screen(p);
    let hit = h.state().viewport.pick(at, true, false);
    assert!(matches!(hit, Some(Hit::Body { .. })), "{p:?} is on the plate: {hit:?}");
    click_at(h, at);
}

fn values(h: &H) -> serde_json::Map<String, Value> {
    h.state().tool().expect("the Hole tool is open").values.clone()
}

fn first_point(h: &H) -> [f64; 3] {
    let v = values(h);
    let p = &v["points"][0];
    [p[0].as_f64().unwrap(), p[1].as_f64().unwrap(), p[2].as_f64().unwrap()]
}

fn pick_edge_at(h: &mut H, p: [f64; 3]) {
    h.get_by_label("Pick edge").click();
    h.run_steps(2);
    assert!(h.query_by_label("Click an edge…").is_some(), "the From row is picking");
    click_world(h, p);
}

/// Type into the `k`-th distance box of the From rows (the text boxes right under Pick edge).
fn type_distance(h: &mut H, k: usize, text: &str) {
    let pick = h.get_by_label("Pick edge").rect();
    let mut boxes: Vec<egui::Rect> = h
        .query_all_by_role(egui::accesskit::Role::TextInput)
        .map(|n| n.rect())
        .filter(|r| r.min.y > pick.min.y && r.min.y < pick.max.y + 90.0 && (r.center().x - pick.center().x).abs() < 200.0)
        .collect();
    boxes.sort_by(|a, b| a.min.y.total_cmp(&b.min.y));
    let r = *boxes.get(k).unwrap_or_else(|| panic!("distance box {k}: {boxes:?}"));
    h.query_all_by_role(egui::accesskit::Role::TextInput).find(|n| n.rect() == r).unwrap().focus();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(1);
    h.event(egui::Event::Text(text.to_owned()));
    h.run_steps(1);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
}

#[test]
fn click_near_line_end_snaps_to_it() {
    let (mut h, _link) = plate_with_sketch();
    assert!(open_tool(&mut h, "hole"));
    click_world(&mut h, [5.4, 5.3, 5.0]);
    let p = first_point(&h);
    assert!((p[0] - 5.0).abs() < 1e-3 && (p[1] - 5.0).abs() < 1e-3 && (p[2] - 5.0).abs() < 1e-3, "snapped to the line's start: {p:?}");
    let v = values(&h);
    let r = &v["point_refs"][0][0];
    assert_eq!((r["kind"].as_str(), r["sketch"].as_str(), r["entity"].as_str()), (Some("sketch_line"), Some("f2"), Some("l1")), "{r}");
    assert_eq!(r["along"].as_f64(), Some(0.0));

    // onto the line, 12 mm along it after typing in the Along box
    click_world(&mut h, [17.4, 5.6, 5.0]);
    let pts = values(&h)["points"].clone();
    assert!((pts[1][1].as_f64().unwrap() - 5.0).abs() < 1e-3, "on the line: {pts}");
    type_distance(&mut h, 0, "12");
    let pts = values(&h)["points"].clone();
    assert!((pts[1][0].as_f64().unwrap() - 17.0).abs() < 1e-3 && (pts[1][1].as_f64().unwrap() - 5.0).abs() < 1e-3, "12 mm along: {pts}");

    // a click away from the sketch stays where it was clicked
    click_world(&mut h, [25.0, 20.0, 5.0]);
    let pts = values(&h)["points"].clone();
    assert!((pts[2][0].as_f64().unwrap() - 25.0).abs() < 0.05 && (pts[2][1].as_f64().unwrap() - 20.0).abs() < 0.05, "{pts}");
    assert_eq!(values(&h)["point_refs"][2], json!([]));
}

#[test]
fn two_edge_distances_give_exact_centre_and_reopen_shows_them() {
    let (mut h, link) = plate_with_sketch();
    assert!(open_tool(&mut h, "hole"));
    click_world(&mut h, [12.0, 19.0, 5.0]);
    assert!(values(&h).get("point_refs").is_none(), "a plain click has no references");

    pick_edge_at(&mut h, [0.4, 24.0, 5.0]); // the left edge
    let v = values(&h);
    assert_eq!(v["point_refs"][0][0]["kind"], json!("edge"), "{v:?}");
    assert!((v["point_refs"][0][0]["dist"].as_f64().unwrap() - 12.0).abs() < 0.05);
    type_distance(&mut h, 0, "10");

    pick_edge_at(&mut h, [20.0, 0.4, 5.0]); // the front edge
    type_distance(&mut h, 1, "8");
    let p = first_point(&h);
    assert!((p[0] - 10.0).abs() < 1e-3 && (p[1] - 8.0).abs() < 1e-3 && (p[2] - 5.0).abs() < 1e-3, "10 from the left, 8 from the front: {p:?}");

    h.state_mut().tool_ok();
    settle(&mut h, &link);
    let item = h.state().timeline_view.items.iter().find(|i| i.kind == "hole").cloned().expect("the Hole step");
    assert_eq!(item.error, None);
    assert_eq!(item.warning, None);
    let refs = item.params["point_refs"][0].as_array().cloned().unwrap_or_default();
    assert_eq!(refs.len(), 2, "both edges are saved: {}", item.params);
    assert!(item.params.get("_from_pick").is_none());

    // reopening the step shows both references with their distances
    assert!(h.state_mut().edit_feature(&item.id));
    h.run_steps(4);
    assert!(h.query_by_label("Edge 1").is_some() && h.query_by_label("Edge 2").is_some(), "the From rows are back");
    let p = first_point(&h);
    assert!((p[0] - 10.0).abs() < 1e-3 && (p[1] - 8.0).abs() < 1e-3, "{p:?}");
}

/// Review Focus 2: a reference that ties after a rebuild (a line down the middle of the plate,
/// equally far from both long edges) is missing. The hole keeps its point and the panel says so.
#[test]
fn ambiguous_edge_keeps_point_and_says_so() {
    let (mut h, link) = plate_with_sketch();
    h.state_mut().link.send(Command::AddFeature {
        kind: "hole".into(),
        params: json!({"points": [[15, 8, 5]], "normal": [0, 0, 1], "kind": "simple", "diameter": 4, "through": true, "targets": [],
            "point_refs": [[{"kind": "edge", "a": [20, 0, 5], "b": [20, 30, 5], "normal": [0, 0, 1], "dist": 5, "body": "f1"}]]}),
        index: None,
        name: Some("Hole".into()),
    });
    settle(&mut h, &link);
    let item = h.state().timeline_view.items.iter().find(|i| i.kind == "hole").cloned().expect("the Hole step");
    assert_eq!(item.error, None);
    assert!(item.warning.as_deref().is_some_and(|w| w.contains("can no longer be found")), "the step says so: {:?}", item.warning);

    assert!(h.state_mut().edit_feature(&item.id));
    h.run_steps(2);
    super::harness::wait_for(&mut h, &link, "the preview's message", |h| h.state().tool().is_some_and(|t| t.msg.is_some()));
    let msg = h.state().tool().unwrap().msg.as_ref().unwrap().text.clone();
    assert!(msg.contains("Hole point 1") && msg.contains("can no longer be found"), "the panel says so: {msg}");
    let p = first_point(&h);
    assert!((p[0] - 15.0).abs() < 1e-9 && (p[1] - 8.0).abs() < 1e-9, "nothing moved: {p:?}");
}

/// Thread (milestone 7 Task 7b): placed on a face, measured from an edge, saved and reopened.
#[test]
fn thread_edge_distance_reopens_with_the_reference() {
    let (mut h, link) = plate_with_sketch();
    assert!(open_tool(&mut h, "thread"));
    click_world(&mut h, [12.0, 19.0, 5.0]);
    let v = values(&h);
    assert!(v.get("position_refs").is_none(), "a plain click has no references: {v:?}");
    pick_edge_at(&mut h, [0.4, 24.0, 5.0]); // the left edge
    assert_eq!(values(&h)["position_refs"][0]["kind"], json!("edge"), "{:?}", values(&h));
    type_distance(&mut h, 0, "10");
    let off = values(&h)["offset"].clone();
    assert!((off[0].as_f64().unwrap() - 10.0).abs() < 1e-3, "10 from the left edge: {off}");

    h.state_mut().tool_ok();
    settle(&mut h, &link);
    let item = h.state().timeline_view.items.iter().find(|i| i.kind == "thread").cloned().expect("the Thread step");
    assert!(item.params["position_refs"].as_array().is_some_and(|a| !a.is_empty()), "the reference is saved: {}", item.params);
    assert!(item.params.get("_from_pick").is_none());

    assert!(h.state_mut().edit_feature(&item.id));
    h.run_steps(4);
    assert!(h.query_by_label("Edge 1").is_some(), "the From row is back");
    let off = values(&h)["offset"].clone();
    assert!((off[0].as_f64().unwrap() - 10.0).abs() < 1e-3, "{off}");
}

/// A click in the middle of a face with Pick edge on is nowhere near an edge: it warns and stores
/// no reference.
#[test]
fn pick_edge_mid_face_warns_and_stores_nothing() {
    let (mut h, _link) = plate_with_sketch();
    assert!(open_tool(&mut h, "hole"));
    click_world(&mut h, [20.0, 15.0, 5.0]);
    h.get_by_label("Pick edge").click();
    h.run_steps(2);
    click_world(&mut h, [20.0, 15.0, 5.0]); // the middle: 15 mm from every edge
    h.run_steps(2);
    let t = h.state().tool().expect("the Hole tool is open");
    assert!(t.values.get("point_refs").is_none(), "no reference stored: {:?}", t.values);
    let msg = t.msg.as_ref().map(|m| m.text.clone()).unwrap_or_default();
    assert!(msg.contains("no straight edge"), "the warning shows: {msg:?}");
}

/// The panel shows the point the references give now: the plate grows from 40 to 60 and a hole
/// measured 5 from its right edge moves with it; reopening shows (and OK stores) the moved point.
#[test]
fn reopen_shows_the_point_a_rebuild_moved() {
    let (mut h, link) = plate_with_sketch();
    h.state_mut().link.send(Command::AddFeature {
        kind: "hole".into(),
        params: json!({"points": [[35, 10, 5]], "normal": [0, 0, 1], "kind": "simple", "diameter": 4, "through": true, "targets": [],
            "point_refs": [[{"kind": "edge", "a": [40, 0, 5], "b": [40, 30, 5], "normal": [0, 0, 1], "dist": 5, "body": "f1"}]]}),
        index: None,
        name: Some("Hole".into()),
    });
    settle(&mut h, &link);
    let plate = h.state().timeline_view.items.iter().find(|i| i.kind == "primitive").cloned().expect("the plate");
    let mut params = plate.params.clone();
    params["length"] = json!(60);
    h.state_mut().link.send(Command::EditFeature { id: plate.id.clone(), params, rollback_after: None });
    settle(&mut h, &link);
    let item = h.state().timeline_view.items.iter().find(|i| i.kind == "hole").cloned().expect("the Hole step");
    assert_eq!(item.error, None);
    assert_eq!(item.warning, None);
    assert_eq!(item.params["points"][0][0].as_f64(), Some(35.0), "still stored at the old place");

    assert!(h.state_mut().edit_feature(&item.id));
    h.run_steps(4);
    let p = first_point(&h);
    assert!((p[0] - 55.0).abs() < 1e-3 && (p[1] - 10.0).abs() < 1e-3, "the moved point, 5 from the new right edge: {p:?}");
    h.state_mut().tool_ok();
    settle(&mut h, &link);
    let item = h.state().timeline_view.items.iter().find(|i| i.kind == "hole").cloned().unwrap();
    assert!((item.params["points"][0][0].as_f64().unwrap() - 55.0).abs() < 1e-3, "stored too: {}", item.params);
}
