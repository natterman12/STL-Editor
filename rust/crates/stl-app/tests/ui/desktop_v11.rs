//! Thread tool, lay flat, sketch relations and dimensions, inch mode, dark theme.
//! Counterpart of `tests/ui/desktop_v11.js`.
#![cfg(not(target_arch = "wasm32"))]

use super::harness::{
    empty_harness, enter_sketch_plane, finish_sketch_settle, fixture_stl, import_stl, mesh_volume, open_tool, select_first_body, settle,
};
use egui_kittest::kittest::Queryable;
use serde_json::json;
use stl_app::ui::ThemePref;

#[test]
fn desktop_v11_thread_layflat_relations_inch_theme() {
    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("blk.stl"));
    select_first_body(&mut h);
    let v0 = mesh_volume(&h.state().bodies[0].mesh);

    assert!(open_tool(&mut h, "thread"));
    h.state_mut().viewport.flush();
    h.run_steps(1);
    let at = h.state().viewport.world_to_screen([10.0, 5.0, 30.0]);
    super::harness::click_at(&mut h, at);
    h.run_steps(4);
    h.state_mut().tool_ok();
    settle(&mut h, &link);
    let th = h.state().timeline_view.items.iter().find(|i| i.kind == "thread").expect("thread");
    assert_eq!(th.error, None, "thread {:?}", th.error);
    let v1 = mesh_volume(&h.state().bodies[0].mesh);
    assert!(v1 < v0, "tapped hole removes material");

    assert!(open_tool(&mut h, "move"));
    h.state_mut().tool_set_value("rotate", json!([30.0, 20.0, 0.0]));
    h.state_mut().tool_ok();
    settle(&mut h, &link);
    assert!(open_tool(&mut h, "layflat"));
    h.state_mut().tool_set_value("how", json!("auto"));
    h.state_mut().tool_ok();
    settle(&mut h, &link);

    // Sketch on XY with driving dimensions (1.x: auto H/V square + Smart Dimension 30/15).
    enter_sketch_plane(&mut h, &link, json!("XY"));
    {
        let s = h.state_mut().sketch_mode_mut().unwrap();
        s.s.push();
        for (id, p1, p2) in [
            ("L0", [0.0, 0.0], [20.0, 0.0]),
            ("L1", [20.0, 0.0], [20.0, 20.0]),
            ("L2", [20.0, 20.0], [0.0, 20.0]),
            ("L3", [0.0, 20.0], [0.0, 0.0]),
        ] {
            s.s.entities.push(serde_json::from_value(json!({ "id": id, "type": "line", "p1": p1, "p2": p2 })).unwrap());
        }
        s.s.constraints.push(serde_json::from_value(json!({
            "id": "d0", "type": "hdistance", "a": {"e":"L0","k":"p1"}, "b": {"e":"L0","k":"p2"},
            "off": -6.0, "slide": 0.0, "value": 30.0
        })).unwrap());
        s.s.constraints.push(serde_json::from_value(json!({
            "id": "d1", "type": "vdistance", "a": {"e":"L1","k":"p1"}, "b": {"e":"L1","k":"p2"},
            "off": 6.0, "slide": 0.0, "value": 15.0
        })).unwrap());
    }
    h.state_mut().sketch_set_tool("c_dimension");
    h.run_steps(2);
    assert_eq!(h.state().sketch().unwrap().tool, "c_dimension");
    assert!(h.state().sketch().unwrap().constraints.len() >= 2);
    let step = h.state().sketch().unwrap().step_params();
    assert_eq!(step["constraints"].as_array().unwrap().len(), 2);
    finish_sketch_settle(&mut h, &link, false);

    let sid = h.state().timeline_view.items.iter().rev().find(|i| i.kind == "sketch").map(|i| i.id.clone()).unwrap();
    let before = h.state().timeline_view.items.iter().find(|i| i.id == sid).unwrap().params.clone();
    assert!(h.state_mut().edit_feature(&sid));
    settle(&mut h, &link);
    h.state_mut().cancel_sketch();
    settle(&mut h, &link);
    assert_eq!(h.state().timeline_view.items.iter().find(|i| i.id == sid).unwrap().params, before);

    // Inch mode words (1.x) and dark theme + box tool.
    h.state_mut().shell.inch = true;
    h.state_mut().toasts.push(stl_app::engine_link::Level::Info, "Showing inches. Models are still saved in millimetres.".into());
    h.run_steps(2);
    assert!(h.query_by_label_contains("Showing inches").is_some());
    h.state_mut().shell.theme_pref = ThemePref::Dark;
    assert!(open_tool(&mut h, "primitive_box"));
    h.state_mut().tool_cancel();
}
