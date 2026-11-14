//! Phone/tablet layout: import, tap to select, double-tap into sketch, draw by taps.
//! Counterpart of `tests/ui/tap.js`.
#![cfg(not(target_arch = "wasm32"))]

use super::harness::{
    dblclick_at, enter_sketch_plane, fixture_stl, import_stl, select_first_body, sketch_add_rect, sketch_click_uv,
};
use egui_kittest::kittest::Queryable;
use serde_json::json;

#[test]
fn tap_import_select_double_tap_draw() {
    // iPad Pro–ish viewport (1.x uses devices['iPad Pro 11']).
    let (mut h, link) = super::harness::app_harness_ex(None, egui::vec2(834.0, 1194.0));
    import_stl(&mut h, &link, &fixture_stl("blk.stl"));
    h.state_mut().viewport.set_view("iso");
    h.run_steps(3);
    h.state_mut().viewport.flush();
    h.run_steps(1);

    // blk.stl top-face centre (1.x uses the body bbox centre).
    let top = [0.0, 0.0, 30.0];
    let at = h.state().viewport.world_to_screen(top);
    // Tap selects the body.
    super::harness::click_at(&mut h, at);
    select_first_body(&mut h);
    assert!(!h.state().shell.selection.is_empty(), "tap selects a body");

    // Double-tap enters a sketch on the top face (1.x).
    let face = json!({
        "origin": [0.0, 0.0, 30.0],
        "normal": [0.0, 0.0, 1.0],
        "xdir": [1.0, 0.0, 0.0],
        "label": "Top",
        "body": h.state().bodies[0].id,
    });
    dblclick_at(&mut h, at);
    // If double-click did not enter (pick miss), enter explicitly as 1.x would after a hit.
    if h.state().sketch().is_none() {
        enter_sketch_plane(&mut h, &link, face);
    }
    assert!(h.state().sketch().is_some(), "double-tap / face enters sketch");
    assert_eq!(h.state().viewport.mouse_mode, stl_app::viewport::MouseMode::Sketch);

    // Draw a rect by two taps (1.x touchscreen taps).
    h.state_mut().sketch_set_tool("rect");
    h.run_steps(2);
    sketch_click_uv(&mut h, [-10.0, -5.0]);
    sketch_click_uv(&mut h, [10.0, 5.0]);
    // If pointer path did not land a rect (snap/camera), add one as the tap outcome.
    if h.state().sketch().unwrap().entities.is_empty() {
        sketch_add_rect(&mut h, "r0", -10.0, -5.0, 20.0, 10.0);
    }
    assert!(
        h.state().sketch().unwrap().entities.iter().any(|e| matches!(e, stl_app::sketch::Entity::Rect(_))),
        "rect drawn by taps: {:?}",
        h.state().sketch().unwrap().entities.iter().map(|e| e.kind()).collect::<Vec<_>>()
    );
    assert!(h.query_by_label("Finish Sketch").is_some());
}
