//! Sketch on the XZ plane, revolve, edit the sketch from the timeline, undo, save.
//! Counterpart of `tests/ui/desktop_sketch2.js`.
#![cfg(not(target_arch = "wasm32"))]

use super::harness::{empty_harness, finish_sketch_settle, mesh_volume, settle, sketch_add_rect};
use egui_kittest::kittest::Queryable;
use serde_json::json;
use stl_app::engine_link::Command;

#[test]
fn desktop_sketch2_xz_revolve_edit_undo_save() {
    let (mut h, link) = empty_harness();

    // Create Sketch → pick XZ (1.x clicks the XZ base plane in front view).
    h.state_mut().begin_sketch(None);
    h.run_steps(3);
    assert_eq!(h.state().tool().map(|t| t.def.key), Some("sketch"));
    assert!(h.query_by_label_contains("Click a flat face").is_some());
    h.state_mut().tool_set_value("plane", json!("XZ"));
    h.state_mut().tool_ok();
    settle(&mut h, &link);
    {
        let s = h.state().sketch().expect("opened on XZ");
        assert_eq!(s.plane_ref, json!("XZ"));
        assert_eq!(s.frame.normal, [0.0, -1.0, 0.0]);
    }
    assert_eq!(h.state().ribbon_tab(), "sketch");

    // Closed profile away from the axis (1.x: L-polyline; here a rect).
    sketch_add_rect(&mut h, "L", 10.0, 0.0, 10.0, 20.0);
    assert_eq!(h.state().sketch().unwrap().entities.len(), 1);
    finish_sketch_settle(&mut h, &link, false);

    let sid = h.state().timeline_view.items.iter().rev().find(|i| i.kind == "sketch").map(|i| i.id.clone()).expect("sketch step");
    let sketch_step = h.state().timeline_view.items.iter().find(|i| i.id == sid).unwrap().clone();
    assert_eq!(sketch_step.params["plane"], json!("XZ"));
    assert_eq!(sketch_step.params["entities"].as_array().map(|a| a.len()), Some(1));

    assert!(h.state_mut().open_tool("revolve", None));
    h.state_mut().tool_set_value("sketch", json!(sid));
    h.run_steps(4);
    h.state_mut().tool_ok();
    settle(&mut h, &link);
    let rev = h.state().timeline_view.items.iter().find(|i| i.kind == "revolve").expect("revolve step");
    assert_eq!(rev.error, None, "revolve error {:?}", rev.error);
    assert!(!h.state().bodies.is_empty());
    let v0 = mesh_volume(&h.state().bodies[0].mesh);
    assert!(v0 > 1.0, "revolved volume {v0}");

    // Edit from the timeline; widen the profile; Finish.
    assert!(h.state_mut().edit_feature(&sid));
    settle(&mut h, &link);
    assert_eq!(h.state().sketch().unwrap().feature_id.as_deref(), Some(sid.as_str()));
    assert!(h.query_by_label("Edit Sketch").is_some() || h.query_by_label("Finish Sketch").is_some());
    {
        let s = h.state_mut().sketch_mode_mut().unwrap();
        if let Some(stl_app::sketch::Entity::Rect(r)) = s.s.entities.first_mut() {
            r.w = 14.0;
        } else {
            panic!("expected a rect entity");
        }
    }
    finish_sketch_settle(&mut h, &link, false);
    let v1 = mesh_volume(&h.state().bodies[0].mesh);
    assert!((v1 - v0).abs() > 0.5, "edit changes volume: {v0} -> {v1}");

    h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::Z);
    settle(&mut h, &link);
    let v2 = mesh_volume(&h.state().bodies[0].mesh);
    assert!((v2 - v0).abs() < 1.0, "Ctrl+Z restores volume: {v2} vs {v0}");

    h.state_mut().link.send(Command::Save);
    settle(&mut h, &link);
}
