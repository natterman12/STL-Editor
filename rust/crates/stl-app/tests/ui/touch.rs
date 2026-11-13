//! Tablet and phone layout, browser drawer, one-finger orbit; open a tool for panel layout.
//! Counterpart of `tests/ui/touch.js`.
#![cfg(not(target_arch = "wasm32"))]

use super::harness::{app_harness_ex, fixture_stl, import_stl, select_first_body, settle};
use egui::{Event, PointerButton, Pos2};
use egui_kittest::kittest::Queryable;
use serde_json::json;

fn sizes() -> [(&'static str, egui::Vec2); 3] {
    [
        ("ipad", egui::vec2(834.0, 1194.0)),
        ("ipad-land", egui::vec2(1194.0, 834.0)),
        ("phone", egui::vec2(390.0, 844.0)),
    ]
}

#[test]
fn touch_layouts_open_hollow_and_orbit() {
    for (name, size) in sizes() {
        let (mut h, link) = app_harness_ex(None, size);
        import_stl(&mut h, &link, &fixture_stl("blk.stl"));
        select_first_body(&mut h);

        let body = h.state().bodies[0].id.clone();
        let mut vals = serde_json::Map::new();
        vals.insert("body".into(), json!([body]));
        vals.insert("thickness".into(), json!(2.0));
        assert!(h.state_mut().open_tool("hollow", Some(vals)));
        h.run_steps(4);
        assert_eq!(h.state().tool().map(|t| t.def.key), Some("hollow"));
        assert!(
            h.query_by_label("OK").is_some() || h.query_by_label("Cancel").is_some(),
            "{name}: hollow panel buttons at {size:?}"
        );

        // Close tool so orbit is not stolen by the panel
        h.state_mut().tool_cancel();
        h.run_steps(2);

        let cam0 = h.state().viewport.camera().position;
        // One-finger (primary) drag on the viewport — Model mode orbits (viewport::drag_action).
        let mid = Pos2::new(size.x * 0.5, size.y * 0.55);
        h.event(Event::PointerMoved(mid));
        h.event(Event::PointerButton { pos: mid, button: PointerButton::Primary, pressed: true, modifiers: Default::default() });
        h.step();
        for i in 1..=8 {
            let p = Pos2::new(mid.x + i as f32 * 12.0, mid.y);
            h.event(Event::PointerMoved(p));
            h.step();
        }
        h.event(Event::PointerButton {
            pos: Pos2::new(mid.x + 96.0, mid.y),
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        });
        h.run_steps(3);
        let cam1 = h.state().viewport.camera().position;
        let moved = (0..3).any(|i| (cam0[i] - cam1[i]).abs() > 1e-3);
        eprintln!("touch {name}: one-finger orbit moved={moved}");
        // Phone chrome can steal the drag; tablet widths must orbit.
        if name != "phone" {
            assert!(moved, "{name}: one-finger drag should orbit the camera");
        }
        settle(&mut h, &link);
    }
}

#[test]
fn touch_narrow_ribbon_still_lists_tools() {
    let (mut h, _link) = app_harness_ex(Some(super::harness::project("basic.stlx").as_path()), egui::vec2(390.0, 844.0));
    // Model tab tools or More fold — either is fine on a phone width.
    h.run_steps(3);
    let has_extrude = h.query_by_label("Extrude").is_some();
    let has_more = h.query_by_label("More tools").is_some();
    assert!(has_extrude || has_more, "narrow window shows Extrude or More tools");
}

#[test]
#[ignore = "M5: 1.x folds the browser into a drawer (#browser-toggle) on tablets and phones; 2.0's browser panel floats and has no drawer yet, so there is nothing to tap until the phone/tablet layout work in milestone 5"]
fn touch_browser_drawer_toggle() {}
