//! Instant red overhangs, fins-only preview, Draw Walls lines / multi-line / curve.
//! Counterpart of `tests/ui/web8.js`.
#![cfg(not(target_arch = "wasm32"))]

use super::harness::{mesh_volume, wait_for, empty_harness, fixture_stl, import_stl, select_first_body, settle, wait_support_open};
use egui::{Event, PointerButton, Pos2};
use serde_json::json;
use stl_app::supports;

fn click(h: &mut egui_kittest::Harness<'_, stl_app::app::StlApp>, at: Pos2) {
    h.event(Event::PointerMoved(at));
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Default::default() });
    h.step();
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Default::default() });
    h.run_steps(2);
}

#[test]
fn web8_instant_red_overhangs_on_open() {
    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("tipped.stl"));
    select_first_body(&mut h);
    wait_support_open(&mut h, &link, "supportfins");
    // quickPaint runs as the panel opens; session is live in Rotate
    h.run_steps(6);
    assert_eq!(supports::mode_of(&h.state().tool().unwrap().values), "rotate");
    assert!(h.state().tool().unwrap().sf.is_some());
    h.state_mut().tool_cancel();
    settle(&mut h, &link);
}

#[test]
fn web8_draw_walls_line_value() {
    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("ell.stl"));
    select_first_body(&mut h);
    wait_support_open(&mut h, &link, "supportwall");
    assert_eq!(h.state().wall_drawer().shape, "line");
    h.state_mut().tool_set_value(
        "walls",
        json!([{ "shape": "line", "points": [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]] }]),
    );
    h.run_steps(2);
    let walls = h.state().tool().unwrap().values.get("walls").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    assert_eq!(walls.len(), 1);
    assert_eq!(walls[0]["shape"], json!("line"));
    h.state_mut().tool_cancel();
    settle(&mut h, &link);
}

/// Two points on the part as the camera draws it: screen spots worked out from the camera and
/// checked with the viewport's own pick, not guessed from the chrome.
fn two_points_on_the_part(h: &egui_kittest::Harness<'_, stl_app::app::StlApp>) -> (Pos2, Pos2) {
    use stl_app::viewport::pick::Hit;
    let vp = &h.state().viewport;
    let id = h.state().bodies[0].id.clone();
    let b = vp.body_box(&id).expect("the body is on screen");
    let c = vp.world_to_screen([0, 1, 2].map(|i| (b.min[i] + b.max[i]) / 2.0));
    let mut hits = Vec::new();
    for dy in (-90..=90).step_by(15) {
        for dx in (-90..=90).step_by(15) {
            let at = Pos2::new(c.x + dx as f32, c.y + dy as f32);
            if matches!(vp.pick(at, true, false), Some(Hit::Body { .. })) {
                hits.push(at);
            }
        }
    }
    let p1 = *hits.first().expect("a point on the part");
    let p2 = *hits.iter().rev().find(|p| (p.x - p1.x).hypot(p.y - p1.y) > 40.0).expect("a second point on the part");
    (p1, p2)
}

#[test]
fn web8_draw_walls_line_clicks() {
    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("ell.stl"));
    select_first_body(&mut h);
    wait_support_open(&mut h, &link, "supportwall");
    h.run_steps(3);
    let (p1, p2) = two_points_on_the_part(&h);
    click(&mut h, p1);
    assert_eq!(h.state().wall_drawer().pending.len(), 1, "the first click starts a wall");
    click(&mut h, p2);
    let walls = h.state().tool().unwrap().values.get("walls").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    assert_eq!(walls.len(), 1, "the second click finishes the line");
    assert_eq!(walls[0]["shape"], json!("line"));
    h.state_mut().tool_cancel();
    settle(&mut h, &link);
}

#[test]
fn web8_draw_walls_curve_value() {
    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("ell.stl"));
    select_first_body(&mut h);
    wait_support_open(&mut h, &link, "supportwall");
    if supports::engine_features().curves {
        eprintln!("web8: bundled engine supports curves");
    }
    h.state_mut().tool_set_value(
        "walls",
        json!([{
            "shape": "curve",
            "points": [[0.0, 0.0, 0.0], [5.0, 8.0, 0.0], [10.0, 0.0, 0.0]]
        }]),
    );
    h.run_steps(2);
    let walls = h.state().tool().unwrap().values["walls"].as_array().cloned().unwrap_or_default();
    assert_eq!(walls.len(), 1);
    assert_eq!(walls[0]["shape"], json!("curve"));
    h.state_mut().tool_cancel();
    settle(&mut h, &link);
}

#[test]
fn web8_fins_preview_then_ok_grows_volume() {
    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("tipped.stl"));
    let v0 = mesh_volume(&h.state().bodies[0].mesh);
    h.state_mut().shell.selection.clear();
    h.state_mut().shell.selection = vec![h.state().bodies[0].id.clone()];
    wait_support_open(&mut h, &link, "supportfins");
    // the engine's answer: the fins arrive as preview meshes
    wait_for(&mut h, &link, "the fins preview", |h| h.state().viewport.preview_meshes().next().is_some());
    h.state_mut().tool_ok();
    wait_for(&mut h, &link, "the fins to join the part", |h| h.state().tool().is_none() && h.state().timeline_view.items.iter().any(|i| i.kind == "supportfins"));
    let item = h.state().timeline_view.items.iter().find(|i| i.kind == "supportfins").unwrap();
    assert_eq!(item.error, None);
    let v1 = h.state().bodies.iter().map(|b| mesh_volume(&b.mesh)).fold(0.0, f64::max);
    assert!(v1 > v0 + 1.0, "the fins add plastic: {v0:.0} -> {v1:.0}");
}
