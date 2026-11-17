//! UI test `section_view` (milestone 6, spec 2.3): the ribbon's Section button opens Section View
//! and turns the cut on; picking a side face and dragging its arrow moves the cut; Flip swaps the
//! kept half; X turns the cut off and on; Remove cut clears it; New removes it.
#![cfg(not(target_arch = "wasm32"))]

use egui::{Event, PointerButton, Pos2};
use egui_kittest::kittest::Queryable;
use serde_json::json;
use stl_app::engine_link::Command;
use stl_app::ui::section_panel;
use stl_app::viewport::pick::Hit;
use stl_app::viewport::plane_drag;

use super::harness::{click_at, empty_harness, settle};

type H = egui_kittest::Harness<'static, stl_app::app::StlApp>;

/// A 20 mm cube on the bed, seen from the usual iso view.
fn cube() -> (H, std::sync::Arc<stl_app::engine_link::native::NativeLink>) {
    let (mut h, link) = empty_harness();
    h.state_mut().link.send(Command::AddFeature {
        kind: "primitive".into(),
        params: json!({
            "shape": "box", "width": 20, "depth": 20, "height": 20,
            "plane": "XY", "offset": [0,0,0], "rotate": [0,0,0],
            "anchor": "base", "operation": "new", "targets": [], "body_name": "Box"
        }),
        index: None,
        name: Some("Box".into()),
    });
    settle(&mut h, &link);
    h.state_mut().viewport.set_view("iso");
    h.state_mut().viewport.camera_mut().finish_flight();
    h.state_mut().viewport.flush();
    h.run_steps(3);
    (h, link)
}

fn pointer(h: &mut H, at: Pos2, pressed: Option<bool>) {
    h.event(Event::PointerMoved(at));
    if let Some(p) = pressed {
        h.event(Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed: p,
            modifiers: Default::default(),
        });
    }
    h.step();
}

/// Is `p` kept by the cut in force?
fn kept(h: &H, p: [f64; 3]) -> bool {
    let e = h.state().viewport.active_section().expect("a cut is on");
    e[0] * p[0] + e[1] * p[1] + e[2] * p[2] + e[3] >= 0.0
}

fn open_from_ribbon(h: &mut H) {
    // the ribbon's button (the view bar's Section is painted, not a labelled widget)
    h.get_by_label("Section").click();
    h.run_steps(3);
}

#[test]
fn ribbon_section_opens_the_panel_with_the_cut_on() {
    let (mut h, _link) = cube();
    assert!(!h.state().section_open());
    assert!(h.state().viewport.active_section().is_none());
    open_from_ribbon(&mut h);
    assert!(h.state().section_open(), "the ribbon's Section opens Section View");
    assert!(h.state().viewport.active_section().is_some(), "the cut is on");
    assert!(h.query_by_label("Remove cut").is_some(), "the panel shows");
    assert!(h.query_by_label("Done").is_some(), "the button reads Done");
    assert!(h.state().viewport.section_hatch(), "Hatch is on by default");
    assert!(h.state().viewport.drag_plane().is_some(), "the drag control shows");
    // a new cut starts halfway up the parts, on Z
    let d = h.state().section_distance().unwrap();
    assert!((d - 10.0).abs() < 1e-6, "starts at {d}");
    let e = h.state().viewport.active_section().unwrap();
    assert!(
        e[0].abs() < 1e-9 && e[1].abs() < 1e-9 && (e[2].abs() - 1.0).abs() < 1e-9 && (e[3].abs() - 10.0).abs() < 1e-9,
        "{e:?}"
    );
    // seen from above, the top half is cut away (the half away from the camera stays)
    assert!(kept(&h, [0.0, 0.0, 5.0]) && !kept(&h, [0.0, 0.0, 15.0]));

    // Done closes the panel; the cut stays, and the drag control goes
    h.get_by_label("Done").click();
    h.run_steps(2);
    assert!(!h.state().section_open());
    assert!(
        h.state().viewport.active_section().is_some(),
        "the cut stays after Done"
    );
    assert!(h.state().viewport.drag_plane().is_none());
}

#[test]
fn pick_a_side_face_and_drag_the_arrow_moves_the_cut() {
    let (mut h, _link) = cube();
    open_from_ribbon(&mut h);
    // the front face (y = -10, normal -Y) where it is still shown (below the Z cut)
    let spot = h.state().viewport.world_to_screen([2.0, -10.0, 4.0]);
    let hit = h.state().viewport.pick(spot, true, false);
    assert!(
        matches!(&hit, Some(Hit::Body { normal, .. }) if normal[1] < -0.99),
        "the spot is on the front face: {hit:?}"
    );
    click_at(&mut h, spot);
    h.run_steps(2);
    // the face only touches the cube: the cut starts halfway through it, at y = 0
    let d0 = h.state().section_distance().unwrap();
    assert!((d0 + 10.0).abs() < 1e-6, "a face on the boundary starts halfway: {d0}");
    let p = *h.state().viewport.drag_plane().expect("the drag control");
    assert!((p.normal[1] + 1.0).abs() < 1e-9, "the plane is the face's: {p:?}");
    assert!(
        kept(&h, [0.0, 5.0, 10.0]) && !kept(&h, [0.0, -5.0, 10.0]),
        "the half away from the camera stays"
    );

    // drag the arrow 60 px along itself: the plane follows, the camera doesn't orbit
    let (cam, rect) = (h.state().viewport.camera().clone(), h.state().viewport.rect());
    let base = cam.world_to_view(plane_drag::center(&p));
    let tip = plane_drag::handle_at(&cam, &p);
    let len = ((tip[0] - base[0]).powi(2) + (tip[1] - base[1]).powi(2)).sqrt();
    assert!(len > 20.0, "the arrow is not face on in the iso view ({len} px)");
    let dir = [(tip[0] - base[0]) / len, (tip[1] - base[1]) / len];
    let at = |t: f64| {
        Pos2::new(
            rect.min.x + (base[0] + dir[0] * t) as f32,
            rect.min.y + (base[1] + dir[1] * t) as f32,
        )
    };
    let before = (cam.position, cam.target);
    pointer(&mut h, at(len * 0.5), None);
    assert!(h.state().viewport.drag_plane_hot(), "the pointer is on the arrow");
    pointer(&mut h, at(len * 0.5), Some(true));
    for k in 1..=6 {
        pointer(&mut h, at(len * 0.5 + 10.0 * k as f64), None);
    }
    let live = h.state().section_distance().unwrap();
    assert!(live > d0 + 1.0, "the cut follows while dragging: {live}");
    pointer(&mut h, at(len * 0.5 + 60.0), Some(false));
    h.run_steps(3);
    let cam_after = h.state().viewport.camera();
    assert_eq!(
        (cam_after.position, cam_after.target),
        before,
        "dragging the arrow orbited the view"
    );
    let d1 = h.state().section_distance().unwrap();
    // the plane is where the pointer's projection on the normal line says (to the f32 pointer)
    let want = plane_drag::drag_distance(
        &cam,
        &p,
        [base[0] + dir[0] * len * 0.5, base[1] + dir[1] * len * 0.5],
        [
            base[0] + dir[0] * (len * 0.5 + 60.0),
            base[1] + dir[1] * (len * 0.5 + 60.0),
        ],
        d0,
    );
    assert!((d1 - want).abs() < 1e-3, "after the drag {d1}, want {want}");
    // the arrow points out of the face (-Y): the cut moved toward the camera
    let y = -10.0 - d1;
    assert!(y < -1.0, "the cut moved to y = {y}");
    assert!(
        kept(&h, [0.0, y + 0.5, 10.0]) && !kept(&h, [0.0, y - 0.5, 10.0]),
        "the cut is at y = {y}"
    );
    assert!(h.state().section_open(), "a drag doesn't close the panel");
}

#[test]
fn flip_swaps_the_kept_half() {
    let (mut h, _link) = cube();
    open_from_ribbon(&mut h);
    let (lo, hi) = ([0.0, 0.0, 5.0], [0.0, 0.0, 15.0]);
    assert!(kept(&h, lo) && !kept(&h, hi));
    h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Flip").click();
    h.run_steps(2);
    assert!(!kept(&h, lo) && kept(&h, hi), "Flip keeps the other half");
    h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Flip").click();
    h.run_steps(2);
    assert!(kept(&h, lo) && !kept(&h, hi), "Flip again puts it back");
    // Hatch off: the flat cap colour
    h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Hatch")
        .click();
    h.run_steps(2);
    assert!(!h.state().viewport.section_hatch());
    // Remove cut, then a new cut: Hatch is back to on
    h.get_by_label("Remove cut").click();
    h.run_steps(2);
    open_from_ribbon(&mut h);
    assert!(h.state().viewport.section_hatch(), "a new cut starts with Hatch on");
}

#[test]
fn x_turns_the_cut_off_and_on_and_remove_cut_clears_it() {
    let (mut h, _link) = cube();
    // with no cut, X opens Section View (`toggleCut`)
    h.key_press(egui::Key::X);
    h.run_steps(3);
    assert!(h.state().section_open(), "X with no cut opens the panel");
    assert!(h.state().viewport.active_section().is_some());
    let cut = h.state().viewport.section_plane();
    h.key_press(egui::Key::X);
    h.run_steps(2);
    assert!(h.state().viewport.active_section().is_none(), "X turns the cut off");
    assert!(h.state().section_has_cut());
    h.key_press(egui::Key::X);
    h.run_steps(2);
    assert_eq!(h.state().viewport.active_section(), cut, "X turns the same cut back on");

    h.get_by_label("Remove cut").click();
    h.run_steps(2);
    assert!(!h.state().section_has_cut(), "Remove cut clears it");
    assert!(h.state().viewport.active_section().is_none());
    assert!(!h.state().section_open(), "Remove cut closes Section View");
    // and X opens the panel again, with a new cut
    h.key_press(egui::Key::X);
    h.run_steps(3);
    assert!(h.state().section_open() && h.state().viewport.active_section().is_some());

    // Cancel puts back the cut as it was before the panel opened: none
    h.get_by_label("Cancel").click();
    h.run_steps(2);
    assert!(!h.state().section_open());
    assert!(
        !h.state().section_has_cut() && h.state().viewport.active_section().is_none(),
        "Cancel removes a cut it made"
    );
}

#[test]
fn the_note_is_1x_word_for_word_and_new_removes_the_cut() {
    let (mut h, _link) = cube();
    open_from_ribbon(&mut h);
    assert_eq!(
        section_panel::note_text(),
        "The cut only changes what you see: export, Properties and Measure use the whole part. It stays on after you click Done; turn it off and on with Section in the view bar or the X key."
    );
    assert!(
        h.query_by_label(&section_panel::note_text()).is_some(),
        "the note shows"
    );
    h.get_by_label("Done").click();
    h.run_steps(2);
    assert!(h.state().section_has_cut());
    // New (or Open) turns the view off from outside Section: the cut is gone (`clearCut`)
    h.state_mut().viewport.set_section_enabled(false);
    h.run_steps(2);
    assert!(!h.state().section_has_cut(), "a new project removes the cut");
    h.key_press(egui::Key::X);
    h.run_steps(3);
    assert!(h.state().section_open(), "after New, X opens Section View again");

    // the same when the cut was turned off with X before New
    h.get_by_label("Done").click();
    h.run_steps(2);
    h.key_press(egui::Key::X);
    h.run_steps(2);
    assert!(
        h.state().section_has_cut() && h.state().viewport.active_section().is_none(),
        "X turned the cut off"
    );
    h.state_mut().viewport.set_section_enabled(false);
    h.run_steps(2);
    assert!(
        !h.state().section_has_cut(),
        "New removes a cut that was turned off too"
    );
    h.key_press(egui::Key::X);
    h.run_steps(3);
    assert!(
        h.state().section_open(),
        "X opens Section View, it doesn't bring the old cut back"
    );
}

/// The Distance box: the panel's only text box, above Remove cut (the top bar's search box is
/// a text box too, above the panel).
fn distance_box(h: &H) -> egui::Rect {
    let remove = h.get_by_label("Remove cut").rect();
    let panel = egui::Rect::from_min_max(
        egui::pos2(remove.min.x - 120.0, remove.min.y - 400.0),
        egui::pos2(remove.max.x + 200.0, remove.min.y),
    );
    let boxes: Vec<egui::Rect> = h
        .query_all_by_role(egui::accesskit::Role::TextInput)
        .map(|n| n.rect())
        .filter(|r| panel.contains_rect(*r))
        .collect();
    assert_eq!(boxes.len(), 1, "one text box in the panel: {boxes:?}");
    boxes[0]
}

fn type_distance(h: &mut H, text: &str) {
    let r = distance_box(h);
    h.query_all_by_role(egui::accesskit::Role::TextInput)
        .find(|n| n.rect() == r)
        .expect("the Distance box")
        .focus();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(1);
    h.event(Event::Text(text.to_owned()));
    h.run_steps(1);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
}

#[test]
fn distance_takes_inches_and_expressions() {
    let (mut h, _link) = cube();
    open_from_ribbon(&mut h);
    type_distance(&mut h, "4");
    let d = h.state().section_distance().unwrap();
    assert!((d - 4.0).abs() < 1e-9, "4 mm: {d}");
    let e = h.state().viewport.active_section().unwrap();
    assert!((e[3].abs() - 4.0).abs() < 1e-9, "the cut is at z = 4: {e:?}");

    h.state_mut().shell.inch = true;
    h.run_steps(2);
    type_distance(&mut h, "0.5");
    let d = h.state().section_distance().unwrap();
    assert!((d - 12.7).abs() < 1e-9, "0.5 in: {d}");

    type_distance(&mut h, "=10/2");
    let d = h.state().section_distance().unwrap();
    assert!((d - 5.0).abs() < 1e-9, "=10/2: {d}");
    let e = h.state().viewport.active_section().unwrap();
    assert!((e[3].abs() - 5.0).abs() < 1e-9, "the cut follows the expression: {e:?}");
    h.run_steps(2);
    let shown = h
        .query_all_by_role(egui::accesskit::Role::TextInput)
        .find(|n| n.rect() == distance_box(&h))
        .and_then(|n| n.value());
    assert_eq!(shown.as_deref(), Some("=10/2"), "the box keeps the expression");
}
