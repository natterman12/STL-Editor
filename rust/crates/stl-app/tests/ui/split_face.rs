//! UI test `split_face` (milestone 6): Split on a picked side face. The face sits on the body's
//! boundary, so the pick starts Distance halfway through the part and OK gives two bodies;
//! reopening the step from the timeline keeps its distance.
#![cfg(not(target_arch = "wasm32"))]

use super::harness::{app_harness_ex, mesh_volume, open_tool, project, select_first_body, settle};
use serde_json::json;

/// The body's box from its mesh: `(min, max)`.
fn body_box(h: &egui_kittest::Harness<'static, stl_app::app::StlApp>) -> ([f64; 3], [f64; 3]) {
    let m = &h.state().bodies[0].mesh;
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for p in m.positions.chunks_exact(3) {
        for i in 0..3 {
            lo[i] = lo[i].min(f64::from(p[i]));
            hi[i] = hi[i].max(f64::from(p[i]));
        }
    }
    (lo, hi)
}

#[test]
fn split_face_pick_side_face_gives_two_bodies() {
    let (mut h, link) = app_harness_ex(Some(&project("basic.stlx")), egui::vec2(1280.0, 800.0));
    select_first_body(&mut h);
    let v0: f64 = h.state().bodies.iter().map(|b| mesh_volume(&b.mesh)).sum();
    let n0 = h.state().bodies.len();
    let (lo, hi) = body_box(&h);

    assert!(open_tool(&mut h, "split"));
    // pick the +X side face: on the box's boundary, normal pointing out
    let face = json!({
        "origin": [hi[0], (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0],
        "normal": [1, 0, 0], "xdir": [0, 1, 0], "label": "Face",
    });
    h.state_mut().tool_set_value("plane", face);
    h.run_steps(2);
    let want = -(hi[0] - lo[0]) / 2.0;
    let got = h
        .state()
        .tool()
        .unwrap()
        .values
        .get("offset")
        .and_then(|v| v.as_f64())
        .expect("Distance is set");
    assert!(
        (got - want).abs() < 1e-3,
        "a face on the boundary starts halfway: {got} vs {want}"
    );

    h.state_mut().tool_ok();
    settle(&mut h, &link);
    let split = h
        .state()
        .timeline_view
        .items
        .iter()
        .find(|i| i.kind == "split")
        .expect("the Split step");
    assert_eq!(split.error, None);
    assert_eq!(h.state().bodies.len(), n0 + 1, "two bodies from one");
    let total: f64 = h.state().bodies.iter().map(|b| mesh_volume(&b.mesh)).sum();
    assert!(
        (total - v0).abs() < 1e-2 * v0.max(1.0),
        "halves add up: {total} vs {v0}"
    );

    // reopen from the timeline: the stored distance stays
    let sid = split.id.clone();
    assert!(h.state_mut().edit_feature(&sid));
    settle(&mut h, &link);
    let again = h
        .state()
        .tool()
        .unwrap()
        .values
        .get("offset")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    assert!(
        (again - want).abs() < 1e-3,
        "reopened Split keeps Distance: {again} vs {want}"
    );
}
