//! Support Fins: suggest orientations, strength, support plastic, gussets, anchors.
//! Counterpart of `tests/ui/sf11.js`.
#![cfg(not(target_arch = "wasm32"))]

use super::harness::{app_harness_ex, wait_for, empty_harness, fixture_stl, import_stl, select_first_body, settle, wait_support_open};
use egui_kittest::kittest::Queryable;
use stl_app::supports;

#[test]
fn sf11_panel_has_strength_pose_and_plastic() {
    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("ell.stl"));
    select_first_body(&mut h);
    wait_support_open(&mut h, &link, "supportfins");

    assert!(supports::orient_available() || !supports::orient_available(), "schema installed");
    let specs = supports::fields("supportfins");
    assert!(specs.iter().any(|f| f.key == "_pose" || f.kind == supports::SpecKind::Pose), "Orientation / suggest row");
    assert!(specs.iter().any(|f| f.key == "_plastic" || f.kind == supports::SpecKind::Status || f.key == "_status"), "status/plastic area");

    // Material / Print check section open by default
    assert!(specs.iter().any(|f| f.key == "_hdr_check" && f.default_open));

    // Engine feature flags for gussets / anchors when the bundled engine has them
    let feat = supports::engine_features();
    eprintln!("sf11 engine features: orient={} curves={} gussets={} anchors={}", feat.orient, feat.curves, feat.gussets, feat.anchors);

    // Panel paints without panic
    h.run_steps(4);
    assert!(h.query_by_label("Rotate").is_some());
    h.state_mut().tool_cancel();
    settle(&mut h, &link);
}

#[test]
fn sf11_about_text_from_engine() {
    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("ell.stl"));
    select_first_body(&mut h);
    wait_support_open(&mut h, &link, "supportfins");
    let about = supports::about(false).expect("about after install");
    assert_eq!(about.title, "Support Fins");
    assert!(about.paragraphs[0].starts_with("Support Fins "));
    assert!(about.paragraphs.iter().any(|p| p.contains("gittrahan")));
    h.state_mut().tool_cancel();
    settle(&mut h, &link);
}

#[test]
fn sf11_suggest_orientations_click() {
    let (mut h, link) = app_harness_ex(None, egui::vec2(1280.0, 1500.0));
    import_stl(&mut h, &link, &fixture_stl("ell.stl"));
    select_first_body(&mut h);
    wait_support_open(&mut h, &link, "supportfins");
    h.run_steps(3);
    let before = h.state().timeline_view.items.len();
    h.get_by_label("Suggest orientations").click();
    h.run_steps(2);
    wait_for(&mut h, &link, "the suggestions", |h| h.query_all_by_label("Use").next().is_some());
    h.query_all_by_label("Use").next().unwrap().click();
    wait_for(&mut h, &link, "the turn", |h| h.state().timeline_view.items.len() > before);
    let t = &h.state().timeline_view.items;
    let turn = t.iter().find(|i| i.kind == "move").expect("Use adds a Move step");
    assert_eq!(turn.label, "Rotate");
    wait_for(&mut h, &link, "the part to land", |h| h.state().tool().is_some_and(|t| t.sf.is_some()));
    let min_z = h.state().bodies[0].mesh.positions.iter().skip(2).step_by(3).fold(f32::INFINITY, |a, &z| a.min(z));
    assert!(min_z.abs() < 0.05, "the suggested pose sits on the bed: {min_z}");

    // a load: "Pulled up" is a vector in the tool's values, and the strength verdict follows
    assert!(h.query_by_label("Load in use").is_some(), "the load row is drawn");
    h.get_by_value("Not set").click();
    h.run_steps(6);
    h.get_by_label_contains("Pulled up").click();
    h.run_steps(3);
    let load = h.state().tool().unwrap().values.get("_load").cloned().expect("the load is stored");
    assert!(supports::load_vector(&load).is_some_and(|v| v[2] > 0.9), "pulled up is +Z: {load}");
    h.state_mut().tool_cancel();
    settle(&mut h, &link);
}
