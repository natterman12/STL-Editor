//! Milestone 7 Task 9 (spec 2.10, 2.11, 2.13): origin planes show while a tool field takes a
//! plane and go back afterwards; a body click or a tree row fills the active Bodies field; the
//! Living Hinge says what it does and previews its slot.
#![cfg(not(target_arch = "wasm32"))]

use super::harness::{app_harness_ex, click_at, empty_harness, fixture_stl, import_stl, open_tool, project, select_first_body, settle};
use serde_json::json;
use stl_app::ui::UiEvent;

/// A point on the XZ plane (y = 0), above the plate (its top is z = 10) and off the Z axis, so
/// the click meets the XZ plane alone in the front view.
const XZ_CLEAR: [f64; 3] = [3.0, 0.0, 15.0];

/// Imports the plate, selects it, and sets the front view (XZ face-on) the tests click in.
fn plate_scene() -> (egui_kittest::Harness<'static, stl_app::app::StlApp>, std::sync::Arc<stl_app::engine_link::native::NativeLink>) {
    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("plate.stl"));
    select_first_body(&mut h);
    h.state_mut().viewport.set_view("front");
    h.run_steps(2);
    (h, link)
}

/// A screen point where the body itself is on top (its vertices, highest first, are tried in
/// turn), so a click there picks that body and not one standing in front of it.
fn visible_point(h: &egui_kittest::Harness<'static, stl_app::app::StlApp>, id: &str) -> Option<egui::Pos2> {
    let body = h.state().bodies.iter().find(|b| b.id == id)?;
    let mut pts: Vec<[f64; 3]> = body.mesh.positions.chunks_exact(3).map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]).collect();
    pts.sort_by(|a, b| b[2].total_cmp(&a[2]));
    pts.into_iter().step_by(7).find_map(|p| {
        let at = h.state().viewport.world_to_screen(p);
        matches!(h.state().viewport.pick(at, true, false), Some(stl_app::viewport::pick::Hit::Body { id: ref hit, .. }) if hit == id).then_some(at)
    })
}

/// Opens the Mirror panel, which shows the planes while its plane field is active.
#[test]
fn mirror_shows_planes() {
    let (mut h, _link) = plate_scene();
    h.state_mut().viewport.set_base_planes_visible(false);
    assert!(open_tool(&mut h, "mirror"));
    assert!(h.state().viewport.base_planes_visible(), "opening Mirror shows the origin planes");
    h.state_mut().tool_cancel();
    h.run_steps(2);
    assert!(!h.state().viewport.base_planes_visible(), "closing Mirror puts the Origin eye back as it was");
}

/// Mirror's plane field is active at open; a click on the XZ plane fills it, and the planes go
/// back to how they were (hidden here) once the field is filled.
#[test]
fn pick_xz_fills_and_restores() {
    let (mut h, _link) = plate_scene();
    h.state_mut().viewport.set_base_planes_visible(false);
    assert!(open_tool(&mut h, "mirror"));
    let at = h.state().viewport.world_to_screen(XZ_CLEAR);
    click_at(&mut h, at);
    h.run_steps(2);
    let t = h.state().tool().expect("Mirror still open");
    assert_eq!(t.values.get("plane"), Some(&json!("XZ")), "the XZ plane fills the field");
    assert!(!h.state().viewport.base_planes_visible(), "the planes go back once the field is filled");
}

/// Escape closes the tool mid-pick; the Origin eye is the same as before the tool opened, whether
/// it was on or off.
#[test]
fn escape_restores_planes() {
    let (mut h, _link) = plate_scene();
    for before in [false, true] {
        h.state_mut().viewport.set_base_planes_visible(before);
        assert!(open_tool(&mut h, "mirror"));
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        assert!(h.state().tool().is_none(), "Escape closes Mirror");
        assert_eq!(h.state().viewport.base_planes_visible(), before, "eye state {before} restored after Escape");
    }
}

/// Snap-fit: pick On (the XZ plane), then click two bodies in the viewport; both are listed.
#[test]
fn snapfit_click_two_bodies() {
    let (mut h, link) = app_harness_ex(Some(project("basic.stlx").as_path()), egui::vec2(1280.0, 800.0));
    settle(&mut h, &link);
    assert!(h.state().bodies.len() >= 2, "basic.stlx has two bodies");
    let ids: Vec<String> = h.state().bodies.iter().take(2).map(|b| b.id.clone()).collect();
    h.state_mut().viewport.set_view("front");
    assert!(open_tool(&mut h, "snapfit"));
    h.state_mut().tool_set_active_field(Some("plane".into()));
    let at = h.state().viewport.world_to_screen(XZ_CLEAR);
    click_at(&mut h, at);
    h.run_steps(2);
    // from above the two bodies sit side by side, so neither hides the other
    h.state_mut().viewport.set_view("top");
    h.run_steps(3);
    for id in &ids {
        let at = visible_point(&h, id).unwrap_or_else(|| panic!("no visible point on body {id}"));
        click_at(&mut h, at);
        h.run_steps(2);
    }
    let targets = h.state().tool().expect("Snap-fit still open").values.get("targets").cloned().unwrap_or_default();
    for id in &ids {
        assert!(targets.as_array().is_some_and(|a| a.iter().any(|v| v == id)), "body {id} is listed in Bodies: {targets}");
    }
}

/// Once On is filled, Bodies (optional, placeholder "Auto") becomes the active field.
#[test]
fn bodies_activates_after_on() {
    let (mut h, _link) = plate_scene();
    assert!(open_tool(&mut h, "snapfit"));
    h.state_mut().tool_set_active_field(Some("plane".into()));
    let at = h.state().viewport.world_to_screen(XZ_CLEAR);
    click_at(&mut h, at);
    h.run_steps(2);
    let t = h.state().tool().expect("Snap-fit open");
    assert_eq!(t.active_field.as_deref(), Some("targets"), "Bodies is active after On is filled");
}

/// A tree body row fills the active Bodies field the same way a viewport click does.
#[test]
fn tree_row_fills_bodies() {
    let (mut h, _link) = plate_scene();
    let id = h.state().bodies[0].id.clone();
    assert!(open_tool(&mut h, "snapfit"));
    h.state_mut().tool_set_active_field(Some("targets".into()));
    h.state_mut().push_ui_event(UiEvent::Select(vec![id.clone()]));
    h.run_steps(3);
    let targets = h.state().tool().expect("Snap-fit still open").values.get("targets").cloned().unwrap_or_default();
    assert_eq!(targets, json!([id]), "the tree row fills Bodies");
}

/// The Living Hinge panel starts with the two numbered lines of spec 2.13, word for word.
#[test]
fn hinge_panel_text() {
    let (mut h, _link) = plate_scene();
    assert!(open_tool(&mut h, "hinge"));
    let hint = h.state().tool().expect("Living hinge open").def.hint.expect("hinge has a hint");
    let lines: Vec<&str> = hint.lines().collect();
    assert_eq!(lines[0], "1. Pick the face or plane the hinge runs across.");
    assert_eq!(lines[1], "2. Set where it sits and how thin it is.");
}

/// Picking a plane for the hinge draws its line and flex arrow in the viewport.
#[test]
fn hinge_preview_after_plane() {
    let (mut h, _link) = plate_scene();
    assert!(open_tool(&mut h, "hinge"));
    h.state_mut().tool_set_active_field(Some("plane".into()));
    let at = h.state().viewport.world_to_screen(XZ_CLEAR);
    click_at(&mut h, at);
    h.run_steps(3);
    let t = h.state().tool().expect("Living hinge open");
    assert_eq!(t.values.get("plane"), Some(&json!("XZ")));
    let marks = h.state().viewport.overlay_items().iter().filter(|o| o.tag == "hinge").count();
    assert!(marks > 0, "the hinge line and flex arrow are drawn after a plane is picked");
}
