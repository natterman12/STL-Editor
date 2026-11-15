//! From the hands-on test of the M7 build (2026-11-15): the XY plane stayed highlighted orange
//! after the Sphere tool closed. (The panel seemed to ignore mouse clicks too, but that was
//! another window over it; the click tests stay as a guard.)
#![cfg(not(target_arch = "wasm32"))]

use egui_kittest::kittest::Queryable;

use super::harness::{click_at, empty_harness, open_tool, settle};

fn plane_marks(h: &egui_kittest::Harness<'static, stl_app::app::StlApp>) -> Vec<String> {
    h.state().viewport.overlay_items().iter().map(|o| o.tag.clone()).filter(|t| ["plane-ghost", "marker", "regions", "sk", "hinge"].contains(&t.as_str())).collect()
}

#[test]
fn sphere_ok_by_mouse_adds_the_sphere() {
    let (mut h, link) = empty_harness();
    // twice: the second sphere is placed on the first (the case Nathan hit)
    for n in 1..=2 {
        assert!(open_tool(&mut h, "primitive_sphere"));
        settle(&mut h, &link);
        h.run_steps(3);
        let ok = h.get_by_label("OK").rect().center();
        click_at(&mut h, ok);
        settle(&mut h, &link);
        assert!(h.state().tool().is_none(), "sphere {n}: a mouse click on OK closes the panel");
        assert_eq!(h.state().bodies.len(), n, "sphere {n}: OK added a body");
    }
}

#[test]
fn sphere_panel_cancel_by_mouse_closes_it() {
    let (mut h, link) = empty_harness();
    assert!(open_tool(&mut h, "primitive_sphere"));
    settle(&mut h, &link);
    h.run_steps(3);
    let cancel = h.get_by_label("Cancel").rect().center();
    click_at(&mut h, cancel);
    h.run_steps(3);
    assert!(h.state().tool().is_none(), "a mouse click on Cancel closes the panel");
}

#[test]
fn closing_a_tool_removes_its_plane_ghost() {
    let (mut h, link) = empty_harness();
    assert!(open_tool(&mut h, "primitive_sphere"));
    settle(&mut h, &link);
    h.run_steps(3);
    assert!(plane_marks(&h).iter().any(|t| t == "plane-ghost"), "the XY plane is shown while the tool is open");
    h.state_mut().tool_ok();
    settle(&mut h, &link);
    h.run_steps(3);
    assert!(plane_marks(&h).is_empty(), "OK leaves no tool marks: {:?}", plane_marks(&h));

    assert!(open_tool(&mut h, "primitive_sphere"));
    h.run_steps(3);
    h.state_mut().tool_cancel();
    h.run_steps(3);
    assert!(plane_marks(&h).is_empty(), "Cancel leaves no tool marks: {:?}", plane_marks(&h));
}
