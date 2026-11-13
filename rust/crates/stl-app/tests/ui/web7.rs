//! Support Fins panel from the engine schema, preview, edit, Draw Wall.
//! Counterpart of `tests/ui/web7.js`.
#![cfg(not(target_arch = "wasm32"))]

use super::harness::{empty_harness, fixture_stl, import_stl, select_first_body, settle, wait_support_open};
use egui_kittest::kittest::Queryable;
use serde_json::json;
use stl_app::supports;

#[test]
fn web7_schema_panel_preview_and_credit() {
    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("tipped.stl"));
    select_first_body(&mut h);
    wait_support_open(&mut h, &link, "supportfins");

    let t = h.state().tool().unwrap();
    assert!(t.sf.is_some());
    assert!(!supports::fields("supportfins").is_empty(), "schema built fields");
    assert!(t.credit.as_ref().is_some_and(|(c, _)| c.contains("gittrahan")));

    // Firm grip / tine density if present
    if t.values.contains_key("sf:tineDensity") {
        h.state_mut().tool_set_value("sf:tineDensity", json!(100.0));
        h.run_steps(2);
    }
    h.run_steps(4); // allow preview / coverage to start
    h.state_mut().tool_ok();
    settle(&mut h, &link);
    assert!(h.state().timeline_view.items.iter().any(|i| i.kind == "supportfins"));
}

#[test]
fn web7_edit_from_timeline_keeps_grip() {
    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("tipped.stl"));
    select_first_body(&mut h);
    wait_support_open(&mut h, &link, "supportfins");
    if h.state().tool().unwrap().values.contains_key("sf:tineDensity") {
        h.state_mut().tool_set_value("sf:tineDensity", json!(80.0));
    }
    h.state_mut().tool_ok();
    settle(&mut h, &link);
    let id = h.state().timeline_view.items.iter().find(|i| i.kind == "supportfins").map(|i| i.id.clone()).expect("sf step");
    assert!(h.state_mut().edit_feature(&id));
    h.run_steps(4);
    let t = h.state().tool().expect("edit");
    assert_eq!(t.edit_id.as_deref(), Some(id.as_str()));
    if let Some(v) = t.values.get("sf:tineDensity") {
        assert!((v.as_f64().unwrap_or(0.0) - 80.0).abs() < 1e-6 || v.as_i64() == Some(80), "grip restored: {v}");
    }
    h.state_mut().tool_ok();
    settle(&mut h, &link);
}

#[test]
fn web7_draw_walls_adds_wall_step() {
    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("ell.stl"));
    select_first_body(&mut h);
    wait_support_open(&mut h, &link, "supportwall");
    assert_eq!(h.state().tool().map(|t| t.def.key), Some("supportwall"));
    // Seed one finished wall the way Draw Walls would leave it
    h.state_mut().tool_set_value(
        "walls",
        json!([{ "shape": "line", "points": [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]] }]),
    );
    h.run_steps(2);
    h.state_mut().tool_ok();
    settle(&mut h, &link);
    assert!(
        h.state().timeline_view.items.iter().any(|i| i.kind == "supportwall" || i.kind == "supportfins"),
        "wall / fins step after OK: {:?}",
        h.state().timeline_view.items.iter().map(|i| &i.kind).collect::<Vec<_>>()
    );
}

#[test]
fn web7_about_support_fins() {
    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("blk.stl"));
    select_first_body(&mut h);
    wait_support_open(&mut h, &link, "supportfins");
    let about = supports::about(false).expect("about");
    assert_eq!(about.title, "Support Fins");
    assert!(about.paragraphs.iter().any(|p| p.contains("MIT")));
    // Help menu entry when present
    h.get_by_label("Help").click();
    h.run_steps(3);
    let _ = h.query_by_label("About Support Fins");
    h.key_press(egui::Key::Escape);
    h.state_mut().tool_cancel();
    settle(&mut h, &link);
}
