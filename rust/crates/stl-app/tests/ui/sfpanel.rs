//! Support Fins panel: mode buttons, Held card, folding sections remembered, footer credit.
//! Counterpart of `tests/ui/sfpanel.js`.
#![cfg(not(target_arch = "wasm32"))]

use super::harness::{empty_harness, select_first_body, settle, wait_support_open};
use egui_kittest::kittest::Queryable;
use serde_json::json;
use stl_app::engine_link::Command;
use stl_app::supports::{self, MODES};

fn start_box(h: &mut egui_kittest::Harness<'static, stl_app::app::StlApp>, link: &stl_app::engine_link::native::NativeLink) {
    h.state_mut().link.send(Command::AddFeature {
        kind: "primitive".into(),
        params: json!({
            "shape": "box", "width": 20, "depth": 20, "height": 20,
            "plane": "XY", "offset": [0,0,0], "rotate": [0,0,0],
            "anchor": "base", "operation": "new", "targets": []
        }),
        index: None,
        name: Some("Box".into()),
    });
    settle(h, link);
    select_first_body(h);
}

#[test]
fn sfpanel_modes_credit_and_held_card() {
    let (mut h, link) = empty_harness();
    start_box(&mut h, &link);
    wait_support_open(&mut h, &link, "supportfins");

    let t = h.state().tool().expect("Support Fins open");
    assert_eq!(t.def.key, "supportfins");
    assert!(t.sf.is_some(), "Support Fins form, not the generic one");
    assert_eq!(supports::mode_of(&t.values), "rotate");

    // Mode button labels match 1.x
    let mode_labels: Vec<&str> = MODES.iter().map(|(_, l)| *l).collect();
    assert_eq!(mode_labels, vec!["Rotate", "Add fins", "Remove fins"]);
    for (_, label) in MODES {
        assert!(h.query_by_label(*label).is_some(), "mode button {label}");
    }

    // Footer credit
    let credit = t.credit.as_ref().map(|(line, _)| line.as_str()).unwrap_or("");
    assert!(
        credit.starts_with("Support Fins ") && credit.contains("gittrahan") && credit.contains("MIT"),
        "credit: {credit:?}"
    );
    // Hint has no credit
    assert!(!h.state().shell.hint.contains("gittrahan"), "hint must not carry the credit");

    // Held card / status: a plain box has nothing to hold up (coverage may still be running)
    let end = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        h.run_steps(2);
        settle(&mut h, &link);
        let status = h
            .state()
            .tool()
            .and_then(|t| t.sf.as_ref())
            .and_then(|s| s.cover.as_ref())
            .map(|c| (c.busy, c.error.clone(), c.total));
        match status {
            Some((false, None, _)) => break,
            Some((_, Some(e), _)) => panic!("Held card error: {e}"),
            _ if std::time::Instant::now() > end => break,
            _ => std::thread::sleep(std::time::Duration::from_millis(20)),
        }
    }
    // Plastic / status widgets painted without panic
    h.run_steps(2);
}

#[test]
fn sfpanel_body_then_output_order() {
    let (mut h, link) = empty_harness();
    start_box(&mut h, &link);
    wait_support_open(&mut h, &link, "supportfins");
    let specs = supports::fields("supportfins");
    let visible: Vec<_> = specs.iter().filter(|f| supports::field_visible(f, &h.state().tool().unwrap().values)).map(|f| f.key).collect();
    assert!(visible.first().is_some_and(|k| *k == "body"), "body first: {visible:?}");
    assert!(visible.get(1).is_some_and(|k| *k == "output"), "output second: {visible:?}");
    assert!(specs.iter().any(|f| f.key == "_hdr_check"), "Print check section present");
}

#[test]
fn sfpanel_hole_has_no_sf_credit() {
    let (mut h, link) = empty_harness();
    start_box(&mut h, &link);
    wait_support_open(&mut h, &link, "supportfins");
    h.state_mut().tool_cancel();
    settle(&mut h, &link);
    assert!(h.state_mut().open_tool("hole", None));
    h.run_steps(3);
    let t = h.state().tool().expect("Hole");
    assert!(t.credit.is_none(), "no Support Fins credit on other tools");
}

/// A folded section stays folded after the tool is closed and opened again, and after a restart:
/// 1.x kept `stle_sections` in `localStorage`, 2.0 keeps it in the platform's setting store.
#[test]
fn sfpanel_section_memory_across_reopen() {
    use super::harness::{app_harness_with, FakePlatform};
    let platform = FakePlatform::default();
    let (mut h, link) = app_harness_with(None, egui::vec2(1280.0, 1500.0), platform.clone());
    start_box(&mut h, &link);
    wait_support_open(&mut h, &link, "supportfins");
    h.run_steps(3);
    let spec = supports::fields("supportfins").iter().find(|f| f.key == "_hdr_check").expect("Print check section");
    let open = |h: &egui_kittest::Harness<'static, stl_app::app::StlApp>| {
        h.state().tool().and_then(|t| t.sf.as_ref()).map(|s| s.sections.is_open("supportfins", "_hdr_check", spec.default_open))
    };
    assert_eq!(open(&h), Some(true), "Print check starts open");
    h.get_by_label_contains(spec.label).click();
    h.run_steps(3);
    assert_eq!(open(&h), Some(false), "a click on the header folds it");
    assert!(platform.settings.lock().unwrap().get("stle_sections").is_some_and(|v| v.contains("supportfins:_hdr_check")));

    // closed and opened again
    h.state_mut().tool_cancel();
    settle(&mut h, &link);
    wait_support_open(&mut h, &link, "supportfins");
    assert_eq!(open(&h), Some(false), "still folded after reopening the tool");
    drop(h);

    // and after a restart: a new app on the same stored settings
    let (mut h2, link2) = app_harness_with(None, egui::vec2(1280.0, 1500.0), platform);
    start_box(&mut h2, &link2);
    wait_support_open(&mut h2, &link2, "supportfins");
    assert_eq!(open(&h2), Some(false), "folded in the next run too");
}
