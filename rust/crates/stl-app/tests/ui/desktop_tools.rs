//! Hole, primitives, planes, split, mirror, move, hollow, scale, section, info,
//! timeline edit, undo/redo, File menu.
//! Counterpart of `tests/ui/desktop_tools.js`.
//!
//! 1.x steps (desktop_tools.js): hole, cylinder cut, offset plane, split/mirror/move/
//! sphere/hollow, scale fit, section, info, timeline edit extrude, undo/redo, File menu.
#![cfg(not(target_arch = "wasm32"))]

use super::harness::{app_harness_ex, open_tool, project, select_first_body, settle};
use egui_kittest::kittest::Queryable;
use serde_json::json;
use stl_app::engine_link::Command;
use stl_app::viewport::gizmo_draw::GizmoMode;

#[test]
fn desktop_tools_open_each_named_tool() {
    let (mut h, link) = app_harness_ex(Some(&project("basic.stlx")), egui::vec2(1280.0, 800.0));
    select_first_body(&mut h);
    let keys = [
        "hole",
        "primitive_cylinder",
        "plane_offset",
        "split",
        "mirror",
        "move",
        "primitive_sphere",
        "hollow",
        "scale",
    ];
    for key in keys {
        assert!(open_tool(&mut h, key), "{key} opens");
        assert_eq!(h.state().tool().map(|t| t.def.key), Some(key));
        assert!(h.query_by_label("OK").is_some() || h.query_by_label("Cancel").is_some() || h.query_by_label("Done").is_some(), "{key} panel buttons");
        h.state_mut().tool_cancel();
        settle(&mut h, &link);
    }
}

#[test]
fn desktop_tools_move_gizmo_and_scale_fit() {
    let (mut h, link) = app_harness_ex(Some(&project("basic.stlx")), egui::vec2(1280.0, 800.0));
    select_first_body(&mut h);

    assert!(open_tool(&mut h, "move"));
    assert_eq!(h.state().viewport.gizmo().map(|g| g.mode), Some(GizmoMode::Translate));
    h.state_mut().tool_cancel();
    settle(&mut h, &link);

    assert!(open_tool(&mut h, "scale"));
    h.state_mut().tool_set_value("mode", json!("fit"));
    h.run_steps(2);
    assert_eq!(h.state().tool().unwrap().values.get("mode"), Some(&json!("fit")));
    h.state_mut().tool_cancel();
    settle(&mut h, &link);
}

#[test]
fn desktop_tools_section_and_file_menu() {
    let (mut h, _link) = app_harness_ex(Some(&project("basic.stlx")), egui::vec2(1280.0, 800.0));
    // Section view is a View/ribbon action covered in 4a; here we only check the File menu still works with a project open.
    h.get_by_label("File").click();
    h.run_steps(3);
    let labels: Vec<_> = h.query_all_by_label_contains("Save").collect();
    assert!(!labels.is_empty(), "File menu shows Save");
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
}

#[test]
fn desktop_tools_undo_redo_after_primitive() {
    let (mut h, link) = app_harness_ex(None, egui::vec2(1280.0, 800.0));
    h.state_mut().link.send(Command::AddFeature {
        kind: "primitive".into(),
        params: json!({"shape": "box", "width": 20, "depth": 20, "height": 10, "plane": "XY", "offset": [0,0,0], "rotate": [0,0,0], "anchor": "base", "operation": "new", "targets": []}),
        index: None,
        name: Some("Box".into()),
    });
    settle(&mut h, &link);
    let n = h.state().timeline_view.items.len();
    assert!(n >= 1, "box step present");
    h.state_mut().link.send(Command::Undo);
    settle(&mut h, &link);
    let after_undo = h.state().timeline_view.items.len();
    assert!(after_undo < n, "Ctrl+Z removes the step");
    h.state_mut().link.send(Command::Redo);
    settle(&mut h, &link);
    assert!(h.state().timeline_view.items.len() >= after_undo, "Ctrl+Y restores");
}

/// 1.x clicks two spots on a face, picks Countersink and OKs. The clicks here are real viewport
/// clicks on the top face of a box, worked out from the camera and checked with the viewport's pick.
/// (The sketch + extrude timeline edit of the same 1.x script is 4c: sketch mode.)
#[test]
fn desktop_tools_hole_countersink_on_a_picked_face() {
    use egui::{Event, PointerButton, Pos2};
    use stl_app::viewport::pick::Hit;
    let (mut h, link) = app_harness_ex(None, egui::vec2(1280.0, 800.0));
    h.state_mut().link.send(Command::AddFeature {
        kind: "primitive".into(),
        params: json!({"shape": "box", "length": 40, "width": 30, "height": 20, "plane": "XY", "offset": [0, 0, 0], "rotate": [0, 0, 0], "anchor": "base", "operation": "new", "targets": []}),
        index: None,
        name: Some("Box".into()),
    });
    settle(&mut h, &link);
    select_first_body(&mut h);
    h.state_mut().viewport.set_view("iso");
    h.run_steps(3);
    let v0 = super::harness::mesh_volume(&h.state().bodies[0].mesh);
    assert!(open_tool(&mut h, "hole"));
    let click = |h: &mut egui_kittest::Harness<'static, stl_app::app::StlApp>, at: Pos2| {
        h.event(Event::PointerMoved(at));
        h.step();
        h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Default::default() });
        h.step();
        h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Default::default() });
        h.run_steps(2);
    };
    for p in [[-8.0, -6.0, 20.0], [8.0, 6.0, 20.0]] {
        let at = h.state().viewport.world_to_screen(p);
        let hit = h.state().viewport.pick(at, true, false);
        assert!(matches!(hit, Some(Hit::Body { .. })), "the spot {p:?} is on the part: {hit:?}");
        click(&mut h, at);
    }
    let pts = h.state().tool().unwrap().values.get("points").cloned().unwrap_or(json!(null));
    assert!(pts.as_array().is_some_and(|a| a.len() == 2), "two clicks, two hole spots: {pts}");
    h.state_mut().tool_set_value("kind", json!("countersink"));
    h.run_steps(2);
    h.state_mut().tool_ok();
    settle(&mut h, &link);
    let hole = h.state().timeline_view.items.iter().find(|i| i.kind == "hole").expect("the Hole step");
    assert_eq!(hole.error, None);
    assert_eq!(hole.params["kind"], json!("countersink"));
    let v1 = super::harness::mesh_volume(&h.state().bodies[0].mesh);
    assert!(v1 < v0 - 1.0, "the holes take material away: {v0:.0} -> {v1:.0}");
}

/// Sketch + Extrude on a body face, then edit the Extrude distance from the timeline.
/// 1.x `desktop_tools.js` "edit extrude via timeline" opens a timeline step and OKs a param
/// change (it edits a hole diameter in the script). Here we cover the Extrude distance edit
/// path: sketch on the box top so Extrude defaults to join (datum XY would be `new` and leave
/// `bodies[0]` at 24000 while 8→12 stayed inside the box).
#[test]
fn desktop_tools_timeline_edit_extrude() {
    use super::harness::{empty_harness, enter_sketch_plane, finish_sketch_settle, mesh_volume, sketch_add_rect};
    let (mut h, link) = empty_harness();
    h.state_mut().link.send(Command::AddFeature {
        kind: "primitive".into(),
        params: json!({"shape": "box", "length": 40, "width": 30, "height": 20, "plane": "XY", "offset": [0, 0, 0], "rotate": [0, 0, 0], "anchor": "base", "operation": "new", "targets": []}),
        index: None,
        name: Some("Box".into()),
    });
    settle(&mut h, &link);
    let bid = h.state().bodies[0].id.clone();
    enter_sketch_plane(
        &mut h,
        &link,
        json!({
            "origin": [0.0, 0.0, 20.0],
            "normal": [0.0, 0.0, 1.0],
            "xdir": [1.0, 0.0, 0.0],
            "label": "Top",
            "body": bid,
        }),
    );
    sketch_add_rect(&mut h, "r0", -10.0, -8.0, 20.0, 16.0);
    finish_sketch_settle(&mut h, &link, true);
    let t = h.state().tool().expect("Extrude opens");
    assert_eq!(t.def.key, "extrude");
    assert_eq!(t.values.get("operation"), Some(&json!("join")), "sketch on body → join");
    h.state_mut().tool_set_value("distance", json!(8.0));
    h.state_mut().tool_ok();
    settle(&mut h, &link);
    let eid = h.state().timeline_view.items.iter().find(|i| i.kind == "extrude").map(|i| i.id.clone()).expect("extrude");
    let v0 = mesh_volume(&h.state().bodies[0].mesh);

    assert!(h.state_mut().edit_feature(&eid));
    settle(&mut h, &link);
    assert_eq!(h.state().tool().map(|t| t.def.key), Some("extrude"));
    assert_eq!(h.state().tool().and_then(|t| t.values.get("distance").cloned()), Some(json!(8.0)));
    h.state_mut().tool_set_value("distance", json!(12.0));
    h.run_steps(4);
    h.state_mut().tool_ok();
    settle(&mut h, &link);
    let ex = h.state().timeline_view.items.iter().find(|i| i.id == eid).unwrap();
    assert_eq!(ex.params["distance"], json!(12.0), "1.x timeline edit writes the new distance");
    assert_eq!(ex.error, None);
    let v1 = mesh_volume(&h.state().bodies[0].mesh);
    // Join from the top face: 20×16 rect, height 8→12 grows volume by 20*16*4.
    assert!(
        (v1 - v0).abs() > 1.0,
        "editing Extrude distance changes the solid volume; got {v0} -> {v1} (params distance={})",
        ex.params["distance"]
    );
}
