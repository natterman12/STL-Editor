//! Move / Rotate / Scale handles on the part, and Land on bed.
//! Counterpart of `tests/ui/gizmo.js`.
#![cfg(not(target_arch = "wasm32"))]

use super::harness::{empty_harness, open_tool, select_first_body, settle};
use egui::{Event, PointerButton, Pos2};
use serde_json::json;
use stl_app::engine_link::Command;
use stl_app::viewport::camera;
use stl_app::viewport::gizmo_draw::{handle_unit, GizmoMode};

fn add_box(h: &mut egui_kittest::Harness<'static, stl_app::app::StlApp>, link: &stl_app::engine_link::native::NativeLink) {
    h.state_mut().link.send(Command::AddFeature {
        kind: "primitive".into(),
        params: json!({
            "shape": "box", "width": 40, "depth": 20, "height": 10,
            "plane": "XY", "offset": [0,0,0], "rotate": [0,0,0],
            "anchor": "base", "operation": "new", "targets": [], "body_name": "Box"
        }),
        index: None,
        name: Some("Box".into()),
    });
    settle(h, link);
    select_first_body(h);
    h.state_mut().viewport.set_view("front");
    h.run_steps(3);
}

fn pointer(h: &mut egui_kittest::Harness<'_, stl_app::app::StlApp>, at: Pos2, pressed: Option<bool>) {
    h.event(Event::PointerMoved(at));
    if let Some(p) = pressed {
        h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: p, modifiers: Default::default() });
    }
    h.step();
}

#[test]
fn gizmo_rotate_from_ribbon_shows_rings() {
    let (mut h, link) = empty_harness();
    add_box(&mut h, &link);
    h.state_mut().on_ribbon("rotate", Pos2::ZERO);
    h.run_steps(4);
    let t = h.state().tool().expect("Move opens for Rotate");
    assert_eq!(t.def.key, "move");
    assert_eq!(t.values.get("_gizmo"), Some(&json!("rotate")));
    assert_eq!(h.state().viewport.gizmo().map(|g| g.mode), Some(GizmoMode::Rotate));
    assert!(t.values.get("bed").and_then(|v| v.as_bool()).unwrap_or(false), "Land on bed default on rotate");
}

#[test]
fn gizmo_move_drag_writes_translate() {
    let (mut h, link) = empty_harness();
    add_box(&mut h, &link);
    assert!(open_tool(&mut h, "move"));
    h.run_steps(3);
    assert_eq!(h.state().viewport.gizmo().map(|g| g.mode), Some(GizmoMode::Translate));

    let vp = &h.state().viewport;
    let c = vp.gizmo().expect("handles").position;
    let u = handle_unit(vp.camera(), c);
    let grab = camera::add(c, [0.45 * u, 0.0, 0.0]);
    let to = camera::add(grab, [15.0, 0.0, 0.0]);
    let (g, t) = (vp.world_to_screen(grab), vp.world_to_screen(to));

    pointer(&mut h, g, None);
    pointer(&mut h, g, Some(true));
    pointer(&mut h, t, None);
    let live = h.state().tool().unwrap().values["translate"].clone();
    assert!((live[0].as_f64().unwrap_or(0.0) - 15.0).abs() < 1.0, "translate while dragging: {live}");
    pointer(&mut h, t, Some(false));
    h.run_steps(3);
    // Move auto-accepts on drop → Done/Close labels
    let tool = h.state().tool().expect("still open");
    assert!(
        tool.ok_label == "Done" || tool.values.get("translate").is_some(),
        "after drop: ok={} values={:?}",
        tool.ok_label,
        tool.values.get("translate")
    );
}

#[test]
fn gizmo_scale_escape_hides_handles() {
    let (mut h, link) = empty_harness();
    add_box(&mut h, &link);
    assert!(open_tool(&mut h, "scale"));
    h.run_steps(3);
    assert!(h.state().viewport.gizmo().is_some(), "scale handles visible");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(h.state().tool().is_none());
    assert!(h.state().viewport.gizmo().is_none(), "handles gone after cancel");
}

/// The Y ring seen from the front is a circle of half a handle unit around the centre: a point on it
/// at 45 degrees is worked out from the camera, so the grab doesn't depend on pixel luck.
#[test]
fn gizmo_land_on_bed_after_rotate_ok() {
    use stl_app::viewport::gizmo_draw::Handle;
    let (mut h, link) = empty_harness();
    add_box(&mut h, &link);
    h.state_mut().on_ribbon("rotate", Pos2::ZERO);
    h.run_steps(4);
    let vp = &h.state().viewport;
    let c = vp.gizmo().expect("rings").position;
    let u = handle_unit(vp.camera(), c);
    let on_ring = camera::add(c, [u * 0.3536, 0.0, u * 0.3536]);
    let grab = vp.world_to_screen(on_ring);
    pointer(&mut h, grab, None);
    h.run_steps(1);
    assert_eq!(h.state().viewport.gizmo().and_then(|g| g.axis), Some(Handle::Y), "the pointer is on the Y ring");
    pointer(&mut h, grab, Some(true));
    for i in 1..=10 {
        pointer(&mut h, Pos2::new(grab.x + i as f32 * 6.0, grab.y + i as f32 * 9.0), None);
    }
    let live = h.state().tool().unwrap().values["rotate"].clone();
    assert!(live.as_array().unwrap().iter().any(|v| v.as_f64().unwrap_or(0.0).abs() > 1.0), "the ring turned the part: {live}");
    pointer(&mut h, Pos2::new(grab.x + 60.0, grab.y + 90.0), Some(false));
    settle(&mut h, &link);
    h.run_steps(3);
    assert_eq!(h.state().tool().unwrap().values.get("bed"), Some(&json!(true)));
    h.state_mut().tool_ok();
    settle(&mut h, &link);
    let min_z = h.state().bodies[0].mesh.positions.iter().skip(2).step_by(3).fold(f32::INFINITY, |a, &z| a.min(z));
    assert!(min_z.abs() < 0.05, "the turned part lands on the bed: min z {min_z}");
}
