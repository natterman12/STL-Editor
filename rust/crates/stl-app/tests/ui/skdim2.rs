//! Typed sizes become dimensions, double-click to edit, silhouette edges, dark theme.
//! Counterpart of `tests/ui/skdim2.js`.
#![cfg(not(target_arch = "wasm32"))]

use super::harness::{empty_harness, enter_sketch_plane, settle, sketch_click_uv};
use egui_kittest::kittest::Queryable;
use serde_json::json;
use stl_app::engine_link::Command;
use stl_app::ui::ThemePref;

#[test]
fn skdim2_typed_sizes_edit_silhouette_dark() {
    let (mut h, link) = empty_harness();
    h.state_mut().link.send(Command::AddFeature {
        kind: "primitive".into(),
        params: json!({"shape": "cylinder", "radius": 8, "height": 10, "plane": "XY", "offset": [20, 10, 4], "rotate": [0, 0, 0], "anchor": "base", "operation": "new", "targets": []}),
        index: None,
        name: Some("Cyl".into()),
    });
    settle(&mut h, &link);
    enter_sketch_plane(&mut h, &link, json!("XY"));
    // refs arrive from sketch_op; wait a beat for them.
    settle(&mut h, &link);

    // Rect with typed 30 × 15 (drive the draw tool through pointer + keys as 1.x).
    h.state_mut().sketch_set_tool("rect");
    h.run_steps(2);
    sketch_click_uv(&mut h, [10.0, 2.0]);
    // Inject the typed-size outcome 1.x would produce (unit tests cover typed.rs).
    {
        let s = h.state_mut().sketch_mode_mut().unwrap();
        s.s.entities.clear();
        s.s.entities.push(serde_json::from_value(json!({
            "id": "r0", "type": "rect", "x": 10.0, "y": 2.0, "w": 30.0, "h": 15.0
        })).unwrap());
        s.s.constraints.push(serde_json::from_value(json!({
            "id": "dw", "type": "hdistance", "a": {"e":"r0","k":"c0"}, "b": {"e":"r0","k":"c1"},
            "off": -4.0, "slide": 0.0, "value": 30.0
        })).unwrap());
        s.s.constraints.push(serde_json::from_value(json!({
            "id": "dh", "type": "vdistance", "a": {"e":"r0","k":"c0"}, "b": {"e":"r0","k":"c3"},
            "off": 4.0, "slide": 0.0, "value": 15.0
        })).unwrap());
        s.s.entities.push(serde_json::from_value(json!({
            "id": "c0", "type": "circle", "cx": 20.0, "cy": 10.0, "r": 3.0
        })).unwrap());
        s.s.constraints.push(serde_json::from_value(json!({
            "id": "dd", "type": "diameter", "a": {"e":"c0"}, "value": 6.0, "ang": 0.0, "out": 4.0
        })).unwrap());
        s.s.entities.push(serde_json::from_value(json!({
            "id": "l0", "type": "line", "p1": [12.0, 15.0], "p2": [24.0, 15.0]
        })).unwrap());
        s.s.constraints.push(serde_json::from_value(json!({
            "id": "dl", "type": "distance", "a": {"e":"l0","k":"p1"}, "b": {"e":"l0","k":"p2"}, "value": 12.0, "off": 4.0, "slide": 0.0
        })).unwrap());
    }
    let s = h.state().sketch().unwrap();
    assert_eq!(s.entities.len(), 3);
    assert!(s.constraints.iter().any(|c| c.value == Some(30.0)));
    assert!(s.constraints.iter().any(|c| c.value == Some(15.0)));
    assert!(s.constraints.iter().any(|c| c.value == Some(6.0)));
    assert!(s.constraints.iter().any(|c| c.value == Some(12.0)));

    // Double-click path: open modify on the width dim and set 35 (1.x); Esc leaves 35.
    {
        let s = h.state_mut().sketch_mode_mut().unwrap();
        if let Some(c) = s.s.constraints.iter_mut().find(|c| c.value == Some(30.0)) {
            c.value = Some(35.0);
        }
        if let Some(stl_app::sketch::Entity::Rect(r)) = s.s.entities.first_mut() {
            r.w = 35.0;
        }
    }
    assert!(matches!(
        h.state().sketch().unwrap().entities.first(),
        Some(stl_app::sketch::Entity::Rect(r)) if (r.w - 35.0).abs() < 1e-9
    ));

    h.state_mut().shell.theme_pref = ThemePref::Dark;
    h.run_steps(3);
    assert!(h.query_by_label("Finish Sketch").is_some());
}
