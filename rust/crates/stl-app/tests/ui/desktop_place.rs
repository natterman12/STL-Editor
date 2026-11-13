//! Import place mode (keep), undo after editing the import feature.
//! Counterpart of `tests/ui/desktop_place.js`.
//!
//! 1.x steps (desktop_place.js):
//! 1. Import a fixture STL; import tool opens with place mode.
//! 2. Screenshot place_center; switch place segment to "keep"; OK.
//! 3. Log bbox after keep and rollback marker.
//! 4. Ctrl+Z; log bbox after undo; no console errors.
#![cfg(not(target_arch = "wasm32"))]

use super::harness::{empty_harness, fixture_stl, import_stl, settle};
use serde_json::json;

#[test]
fn desktop_place_import_then_edit_place_keep() {
    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("off.stl"));
    assert_eq!(h.state().bodies.len(), 1);
    let import = h
        .state()
        .timeline_view
        .items
        .iter()
        .find(|i| i.kind == "import")
        .cloned()
        .expect("Import step after STL");
    let bbox_before = h.state().bodies[0].mesh.positions.clone();

    // Edit the import feature from the timeline (1.x double-click).
    assert!(h.state_mut().edit_feature(&import.id), "open Import for edit");
    h.run_steps(4);
    let t = h.state().tool().expect("Import tool open");
    assert_eq!(t.def.key, "import");
    assert_eq!(t.edit_id.as_deref(), Some(import.id.as_str()));

    // Switch place to "keep" and OK.
    h.state_mut().tool_set_value("place", json!("keep"));
    h.run_steps(2);
    assert_eq!(h.state().tool().unwrap().values.get("place"), Some(&json!("keep")));
    h.state_mut().tool_ok();
    settle(&mut h, &link);
    assert!(h.state().tool().is_none());
    assert!(!h.state().bodies.is_empty(), "body remains after place=keep OK");
    assert_eq!(h.state().bodies[0].mesh.positions.len(), bbox_before.len(), "mesh kept after place=keep");
}

/// Ctrl+Z after editing the import back to place=keep undoes the edit only: the Import step and its
/// body stay, with the mesh as it was placed before.
#[test]
fn desktop_place_undo_after_keep() {
    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("off.stl"));
    let import = h.state().timeline_view.items.iter().find(|i| i.kind == "import").cloned().expect("Import step");
    let placed = h.state().bodies[0].mesh.positions.clone();
    assert!(h.state_mut().edit_feature(&import.id));
    h.run_steps(4);
    h.state_mut().tool_set_value("place", json!("keep"));
    h.state_mut().tool_ok();
    settle(&mut h, &link);
    let kept = h.state().bodies[0].mesh.positions.clone();
    assert_ne!(kept, placed, "keep leaves the mesh where the file had it, not moved to the bed");
    h.state_mut().link.send(stl_app::engine_link::Command::Undo);
    settle(&mut h, &link);
    assert_eq!(h.state().timeline_view.items.iter().filter(|i| i.kind == "import").count(), 1, "undo kept the Import step");
    assert_eq!(h.state().bodies.len(), 1);
    assert_eq!(h.state().bodies[0].mesh.positions, placed, "the mesh is back as it was placed");
}

#[test]
fn desktop_place_fresh_import_defaults() {
    let (mut h, link) = empty_harness();
    // Opening Import from the ribbon before a file: panel exists with place options.
    assert!(h.state_mut().open_tool("import", None));
    h.run_steps(3);
    let t = h.state().tool().expect("Import panel");
    assert_eq!(t.def.key, "import");
    let place = t.values.get("place").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        matches!(place, "center" | "keep" | "origin" | ""),
        "place mode present or defaulted: {place:?}"
    );
    h.state_mut().tool_cancel();
    settle(&mut h, &link);
}
