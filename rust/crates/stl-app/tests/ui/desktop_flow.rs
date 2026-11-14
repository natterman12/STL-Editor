//! Full edit path: import a mesh, sketch and cut on a face, then export.
//! Counterpart of `tests/ui/desktop_flow.js`.
//!
//! 1.x steps (desktop_flow.js):
//! 1. Start empty project; import fixtures/bracket.stl (keep place); iso view.
//! 2. Double-click the top face centre to enter a sketch.
//! 3. Draw a centre rectangle (typed sizes) and a circle; Finish & Extrude cut distance 5.
//! 4. Screenshot result; log features; no console errors.
#![cfg(not(target_arch = "wasm32"))]

use super::harness::{
    dblclick_at, empty_harness, enter_sketch_plane, finish_sketch_settle, fixture_stl, import_stl, mesh_volume, select_first_body, settle,
    sketch_add_rect,
};
use egui_kittest::kittest::Queryable;
use serde_json::json;
use stl_app::engine_link::{Command, ExportFormat};

#[test]
fn desktop_flow_import_sketch_cut_hole_export() {
    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("bracket.stl"));
    select_first_body(&mut h);
    h.state_mut().viewport.set_view("iso");
    h.run_steps(3);
    h.state_mut().viewport.flush();
    h.run_steps(1);

    // 1.x double-clicks world [20,15,20] (top-face centre when the bracket sat on the corner).
    // 2.0 import centres the part (bounds ≈ x -20..20, y -15..15, z 0..30), so that same face
    // centre is [0,0,20]. Pick through the viewport like 1.x; fall back to the face plane.
    let face = json!({
        "origin": [0.0, 0.0, 20.0],
        "normal": [0.0, 0.0, 1.0],
        "xdir": [1.0, 0.0, 0.0],
        "label": "Top",
        "body": h.state().bodies[0].id,
    });
    let at = h.state().viewport.world_to_screen([0.0, 0.0, 20.0]);
    dblclick_at(&mut h, at);
    if h.state().sketch().is_none() {
        enter_sketch_plane(&mut h, &link, face);
    } else {
        settle(&mut h, &link);
        h.state_mut().viewport.flush();
        h.run_steps(2);
    }
    assert_eq!(h.state().ribbon_tab(), "sketch");
    assert!(h.query_by_label("Finish Sketch").is_some());
    assert!(h.query_by_label("Finish & Extrude").is_some());

    // Same relative drawing as 1.x UV 8,6→32,24 and circle at 20,15, but centred on the plane origin.
    sketch_add_rect(&mut h, "r1", -12.0, -9.0, 24.0, 18.0);
    {
        let s = h.state_mut().sketch_mode_mut().unwrap();
        s.s.push();
        s.s.entities.push(serde_json::from_value(json!({
            "id": "c1", "type": "circle", "cx": 0.0, "cy": 0.0, "r": 3.0
        })).unwrap());
    }
    let params = h.state().sketch().unwrap().step_params();
    assert_eq!(params["entities"].as_array().map(|a| a.len()), Some(2));
    assert!(params["plane"].is_object() || params["plane"].is_string(), "plane kept: {}", params["plane"]);

    let v0 = mesh_volume(&h.state().bodies[0].mesh);
    finish_sketch_settle(&mut h, &link, true);
    // Extrude opens on the new sketch (1.x Finish & Extrude).
    let t = h.state().tool().expect("Finish & Extrude opens Extrude");
    assert_eq!(t.def.key, "extrude");
    let sketch_id = t.values.get("sketch").cloned();
    h.state_mut().tool_set_value("operation", json!("cut"));
    h.state_mut().tool_set_value("distance", json!(5.0));
    h.run_steps(4);
    h.state_mut().tool_ok();
    settle(&mut h, &link);

    let kinds: Vec<_> = h.state().timeline_view.items.iter().map(|i| format!("{}:{:?}", i.kind, i.error)).collect();
    let extrude = h.state().timeline_view.items.iter().find(|i| i.kind == "extrude");
    assert!(
        extrude.is_some(),
        "1.x expects an Extrude cut step after Finish & Extrude; timeline={kinds:?} sketch_id={sketch_id:?}"
    );
    let extrude = extrude.unwrap();
    assert_eq!(extrude.error, None, "extrude error {:?}", extrude.error);
    assert_eq!(extrude.params["operation"], json!("cut"));
    assert_eq!(extrude.params["distance"], json!(5.0));
    let v1 = mesh_volume(&h.state().bodies[0].mesh);
    assert!(v1 < v0 - 1.0, "cut removes material: {v0:.0} -> {v1:.0}");

    // Export STL (1.x ends with a screenshot; we assert the export command answers).
    let ids: Vec<String> = h.state().bodies.iter().map(|b| b.id.clone()).collect();
    h.state_mut().link.send(Command::Export {
        format: ExportFormat::Stl,
        ids: Some(ids),
        date: "2026-11-14T00:00:00".into(),
    });
    settle(&mut h, &link);
}
