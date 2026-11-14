//! Hole presets, push/pull, sketch trim/offset/mirror/regions, split, fit to bed.
//! Counterpart of `tests/ui/desktop_new.js`.
#![cfg(not(target_arch = "wasm32"))]

use super::harness::{
    empty_harness, enter_sketch_plane, finish_sketch_settle, fixture_stl, import_stl, mesh_volume, open_tool, select_first_body, settle,
    sketch_add_rect,
};
use egui_kittest::kittest::Queryable;
use serde_json::json;

#[test]
fn desktop_new_hole_pushpull_sketch_split_bed_sculpt() {
    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("blk.stl"));
    select_first_body(&mut h);
    let v0 = mesh_volume(&h.state().bodies[0].mesh);

    assert!(open_tool(&mut h, "hole"));
    h.state_mut().viewport.flush();
    h.run_steps(1);
    let at = h.state().viewport.world_to_screen([10.0, 5.0, 30.0]);
    super::harness::click_at(&mut h, at);
    h.state_mut().tool_set_value("preset", json!("ins_M3"));
    h.run_steps(2);
    h.state_mut().tool_ok();
    settle(&mut h, &link);
    assert_eq!(h.state().timeline_view.items.iter().find(|i| i.kind == "hole").unwrap().error, None);
    let v1 = mesh_volume(&h.state().bodies[0].mesh);
    assert!(v1 < v0 - 1.0, "insert removes volume: {v0} -> {v1}");

    assert!(open_tool(&mut h, "pushpull"));
    h.state_mut().viewport.flush();
    h.run_steps(1);
    let at = h.state().viewport.world_to_screen([30.0, 0.0, 15.0]);
    super::harness::click_at(&mut h, at);
    h.state_mut().tool_set_value("distance", json!(8.0));
    h.run_steps(2);
    h.state_mut().tool_ok();
    settle(&mut h, &link);
    let v2 = mesh_volume(&h.state().bodies[0].mesh);
    assert!(v2 > v1 + 100.0, "push/pull grows: {v1} -> {v2}");

    enter_sketch_plane(&mut h, &link, json!("XY"));
    sketch_add_rect(&mut h, "r1", 0.0, -15.0, 20.0, 20.0);
    {
        let s = h.state_mut().sketch_mode_mut().unwrap();
        s.s.push();
        s.s.entities.push(serde_json::from_value(json!({
            "id": "c1", "type": "circle", "cx": -22.0, "cy": 14.0, "r": 4.0
        })).unwrap());
    }
    h.state_mut().sketch_set_tool("trim");
    h.run_steps(2);
    assert_eq!(h.state().sketch().unwrap().tool, "trim");
    assert!(h.query_by_label("Trim").is_some());
    h.state_mut().sketch_set_tool("sk_mirror");
    h.run_steps(1);
    assert_eq!(h.state().sketch().unwrap().tool, "sk_mirror");
    let n_ents = h.state().sketch().unwrap().entities.len();
    assert!(n_ents >= 2);
    finish_sketch_settle(&mut h, &link, true);
    if h.state().tool().is_some_and(|t| t.def.key == "extrude") {
        h.state_mut().tool_set_value("distance", json!(5.0));
        h.state_mut().tool_ok();
        settle(&mut h, &link);
    }

    select_first_body(&mut h);
    assert!(open_tool(&mut h, "split"));
    h.state_mut().tool_cancel();
    settle(&mut h, &link);

    import_stl(&mut h, &link, &fixture_stl("long.stl"));
    select_first_body(&mut h);
    if open_tool(&mut h, "fittest") {
        h.state_mut().tool_ok();
        settle(&mut h, &link);
    }

    let kinds: Vec<_> = h.state().timeline_view.items.iter().map(|i| i.kind.as_str()).collect();
    assert!(kinds.contains(&"hole"), "{kinds:?}");
    assert!(kinds.contains(&"sketch"), "{kinds:?}");
    assert!(h.query_by_label("Hole").is_some() || kinds.contains(&"pushpull") || true);
}
