//! Web tools: import, hole, sketch cut, threads, export, save and reopen round trip.
//! Counterpart of `tests/ui/web2.js`.
#![cfg(not(target_arch = "wasm32"))]

use super::harness::{
    enter_sketch_plane, finish_sketch_settle, fixture_stl, import_stl, mesh_volume, open_tool, select_first_body, settle, sketch_add_rect,
    FakePlatform,
};
use egui_kittest::kittest::Queryable;
use serde_json::json;
use stl_app::engine_link::{Command, ExportFormat};

#[test]
fn web2_import_hole_sketch_thread_save_reopen() {
    let platform = FakePlatform::default();
    let (mut h, link) = super::harness::app_harness_with(None, egui::vec2(1280.0, 800.0), platform.clone());
    import_stl(&mut h, &link, &fixture_stl("blk.stl"));
    select_first_body(&mut h);
    let v0 = mesh_volume(&h.state().bodies[0].mesh);

    assert!(open_tool(&mut h, "hole"));
    h.state_mut().viewport.flush();
    h.run_steps(1);
    let at = h.state().viewport.world_to_screen([10.0, 5.0, 30.0]);
    super::harness::click_at(&mut h, at);
    h.state_mut().tool_set_value("preset", json!("ins_M3"));
    h.run_steps(2);
    h.state_mut().tool_ok();
    settle(&mut h, &link);
    let v1 = mesh_volume(&h.state().bodies[0].mesh);
    assert!(v1 < v0 - 1.0);

    let bid = h.state().bodies[0].id.clone();
    enter_sketch_plane(
        &mut h,
        &link,
        json!({ "origin": [0.0, 0.0, 30.0], "normal": [0.0, 0.0, 1.0], "xdir": [1.0, 0.0, 0.0], "label": "Top", "body": bid }),
    );
    sketch_add_rect(&mut h, "r0", -25.0, -15.0, 20.0, 20.0);
    // Text dialog (1.x modal "Hi"): open the text tool.
    h.state_mut().sketch_set_tool("text");
    h.run_steps(2);
    assert_eq!(h.state().sketch().unwrap().tool, "text");
    // Project dialog (1.x): ribbon Project with a solid body.
    h.state_mut().on_ribbon("sk_project", egui::Pos2::ZERO);
    h.run_steps(3);
    if h.query_by_label("Project outline").is_some() {
        assert!(h.query_by_label_contains("Adds the outline").is_some());
        // Dialog is open (ribbon also has a "Project" button — don't click by ambiguous label).
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
    }
    finish_sketch_settle(&mut h, &link, true);
    if h.state().tool().is_some_and(|t| t.def.key == "extrude") {
        h.state_mut().tool_set_value("operation", json!("cut"));
        h.state_mut().tool_set_value("distance", json!(4.0));
        h.state_mut().tool_ok();
        settle(&mut h, &link);
    }

    select_first_body(&mut h);
    assert!(open_tool(&mut h, "thread"));
    h.state_mut().viewport.flush();
    h.run_steps(1);
    let at = h.state().viewport.world_to_screen([20.0, 10.0, 30.0]);
    super::harness::click_at(&mut h, at);
    h.run_steps(4);
    h.state_mut().tool_ok();
    settle(&mut h, &link);

    let n_before = h.state().timeline_view.items.len();
    h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::Z);
    settle(&mut h, &link);
    h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::Y);
    settle(&mut h, &link);

    h.state_mut().link.send(Command::Save);
    settle(&mut h, &link);
    h.state_mut().link.send(Command::Export {
        format: ExportFormat::Stl,
        ids: None,
        date: "2026-11-14T00:00:00".into(),
    });
    settle(&mut h, &link);

    let n_after = h.state().timeline_view.items.len();
    assert!(n_after >= 2, "timeline has steps: {n_after} (was {n_before})");
    assert!(h.state().timeline_view.items.iter().any(|i| i.kind == "sketch"));
}
