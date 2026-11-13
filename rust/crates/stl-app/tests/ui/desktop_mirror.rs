//! Mirror and pattern tool panels (preview, apply, timeline).
//! Counterpart of `tests/ui/desktop_mirror.js`.
//!
//! 1.x steps (desktop_mirror.js):
//! 1. Start empty project; import fixtures/plate.stl; iso view.
//! 2. Add a sketch on Z=10 (rect + circle) and extrude cut distance 5.
//! 3. Open Mirror tool; log values; screenshot mirror_preview; OK.
//! 4. Check timeline and removed volume vs expected 2*5*(120+pi*9).
//! 5. Open Pattern; log defaults (source/features); re-open is no-op; Escape.
//! 6. Top view; screenshot mirror_done; no console errors.
#![cfg(not(target_arch = "wasm32"))]

use super::harness::{mesh_volume, fixture_stl, import_stl, open_tool, select_first_body, settle, empty_harness};
use egui_kittest::kittest::Queryable;
use serde_json::json;
use stl_app::engine_link::Command;

#[test]
fn desktop_mirror_opens_panel_and_pattern_defaults() {
    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("plate.stl"));
    select_first_body(&mut h);
    h.state_mut().viewport.set_view("iso");
    h.run_steps(2);

    assert!(open_tool(&mut h, "mirror"), "Mirror is registered");
    let t = h.state().tool().expect("Mirror panel open");
    assert_eq!(t.def.key, "mirror");
    assert_eq!(t.def.label, "Mirror");
    assert!(h.query_by_label("OK").is_some() || h.query_by_label("Cancel").is_some());
    // body defaults to the selection
    let bodies = t.values.get("bodies").cloned().unwrap_or(json!([]));
    assert!(!bodies.as_array().unwrap_or(&vec![]).is_empty() || t.values.get("body").is_some(), "mirror has a body: {:?}", t.values);

    h.state_mut().tool_cancel();
    h.run_steps(2);

    assert!(open_tool(&mut h, "pattern"), "Pattern is registered");
    let t = h.state().tool().expect("Pattern panel open");
    assert_eq!(t.def.key, "pattern");
    // re-open is a no-op for the same tool key while open
    let before = format!("{:?}", t.values);
    assert!(h.state_mut().open_tool("pattern", None));
    h.run_steps(2);
    let after = format!("{:?}", h.state().tool().expect("still open").values);
    assert_eq!(before, after, "re-opening Pattern while open keeps values");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(h.state().tool().is_none(), "Escape closes Pattern");
}

/// 1.x cuts a rectangle and a circle out of the plate with a sketch and Extrude, then mirrors those
/// cuts. The cuts here are primitives (box and cylinder, operation cut) so no sketch mode is needed;
/// the removed volume after Mirror is the same: 2 * 5 * (120 + pi * 9).
#[test]
fn desktop_mirror_cut_volume_after_ok() {
    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("plate.stl"));
    select_first_body(&mut h);
    let body = h.state().bodies[0].id.clone();
    let v0 = mesh_volume(&h.state().bodies[0].mesh);
    let cut = |shape: &str, size: serde_json::Value, offset: [f64; 3]| {
        let mut p = serde_json::Map::new();
        p.insert("shape".into(), json!(shape));
        for (k, v) in size.as_object().unwrap() {
            p.insert(k.clone(), v.clone());
        }
        p.insert("plane".into(), json!("XY"));
        p.insert("offset".into(), json!(offset));
        p.insert("anchor".into(), json!("base"));
        p.insert("operation".into(), json!("cut"));
        p.insert("targets".into(), json!([body]));
        Command::AddFeature { kind: "primitive".into(), params: serde_json::Value::Object(p), index: None, name: None }
    };
    h.state_mut().link.send(cut("box", json!({"length": 12, "width": 10, "height": 5}), [16.0, 0.0, 5.0]));
    h.state_mut().link.send(cut("cylinder", json!({"radius": 3, "height": 5}), [20.0, 12.0, 5.0]));
    settle(&mut h, &link);
    let cuts: Vec<String> = h.state().timeline_view.items.iter().filter(|i| i.kind == "primitive").map(|i| i.id.clone()).collect();
    assert_eq!(cuts.len(), 2, "two cuts in the timeline");
    let single = v0 - mesh_volume(&h.state().bodies[0].mesh);
    assert!((single - 5.0 * (120.0 + std::f64::consts::PI * 9.0)).abs() < 5.0, "one set of cuts removes {single:.0}");

    select_first_body(&mut h);
    assert!(open_tool(&mut h, "mirror"));
    h.state_mut().tool_set_value("kind", json!("features"));
    h.state_mut().tool_set_value("features", json!(cuts));
    h.run_steps(3);
    h.state_mut().tool_ok();
    settle(&mut h, &link);
    assert!(h.state().timeline_view.items.iter().all(|i| i.error.is_none()), "{:?}", h.state().timeline_view.items.iter().map(|i| &i.error).collect::<Vec<_>>());
    let removed = v0 - mesh_volume(&h.state().bodies[0].mesh);
    let expected = 2.0 * 5.0 * (120.0 + std::f64::consts::PI * 9.0);
    assert!((removed - expected).abs() < 10.0, "removed {removed:.0}, expected {expected:.0}");
}

#[test]
fn desktop_mirror_ok_adds_timeline_step() {
    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("plate.stl"));
    select_first_body(&mut h);
    let before = h.state().timeline_view.items.len();
    assert!(open_tool(&mut h, "mirror"));
    // fill plane if the form requires it: Mirror on XY through body centre is the default path
    if let Some(t) = h.state_mut().tool_mut() {
        if t.values.get("plane").is_none() {
            t.values.insert("plane".into(), json!("XY"));
        }
    }
    h.state_mut().tool_ok();
    settle(&mut h, &link);
    let after = h.state().timeline_view.items.len();
    assert!(
        after > before || h.state().timeline_view.items.iter().any(|i| i.kind.contains("mirror")),
        "Mirror OK should add a timeline step (before={before}, items={:?})",
        h.state().timeline_view.items.iter().map(|i| (&i.kind, &i.label)).collect::<Vec<_>>()
    );
}
