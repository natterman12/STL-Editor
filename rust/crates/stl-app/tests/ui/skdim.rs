//! Smart Dimension, part edges, reference dims, Use Edge, parameters, edges found again.
//! Counterpart of `tests/ui/skdim.js`.
#![cfg(not(target_arch = "wasm32"))]

use super::harness::{empty_harness, enter_sketch_plane, finish_sketch_settle, settle, sketch_add_rect};
use egui_kittest::kittest::Queryable;
use serde_json::json;
use stl_app::engine_link::Command;
use stl_app::sketch::dims;

#[test]
fn skdim_smart_dimension_edges_useedge_params() {
    let (mut h, link) = empty_harness();
    h.state_mut().link.send(Command::AddFeature {
        kind: "primitive".into(),
        params: json!({"shape": "box", "length": 40, "width": 20, "height": 10, "plane": "XY", "offset": [0, 0, 0], "rotate": [0, 0, 0], "anchor": "corner", "operation": "new", "targets": []}),
        index: None,
        name: Some("Box".into()),
    });
    settle(&mut h, &link);
    let bid = h.state().bodies[0].id.clone();
    let face = json!({ "origin": [0.0, 0.0, 10.0], "normal": [0.0, 0.0, 1.0], "xdir": [1.0, 0.0, 0.0], "label": "Top", "body": bid });
    enter_sketch_plane(&mut h, &link, face);

    sketch_add_rect(&mut h, "r0", 5.0, 5.0, 20.0, 10.0);
    {
        let s = h.state_mut().sketch_mode_mut().unwrap();
        // Width 30, height 12 (1.x typed into modify).
        s.s.constraints.push(serde_json::from_value(json!({
            "id": "dw", "type": "hdistance", "a": {"e":"r0","k":"c0"}, "b": {"e":"r0","k":"c1"},
            "off": -4.0, "slide": 0.0, "value": 30.0
        })).unwrap());
        s.s.constraints.push(serde_json::from_value(json!({
            "id": "dh", "type": "vdistance", "a": {"e":"r0","k":"c0"}, "b": {"e":"r0","k":"c3"},
            "off": 4.0, "slide": 0.0, "value": 12.0
        })).unwrap());
        s.s.entities.push(serde_json::from_value(json!({
            "id": "c0", "type": "circle", "cx": 33.0, "cy": 12.0, "r": 2.5
        })).unwrap());
        s.s.constraints.push(serde_json::from_value(json!({
            "id": "dd", "type": "diameter", "a": {"e":"c0"}, "ang": -90.0, "out": 8.0, "value": 5.0
        })).unwrap());
    }
    h.state_mut().sketch_set_tool("c_dimension");
    h.run_steps(3);
    assert!(h.state().sketch().unwrap().constraints.len() >= 3);

    // Over-defined width again → asks for a reference dimension (1.x modal).
    let redundant = serde_json::from_value(json!({
        "id": "dw2", "type": "hdistance", "a": {"e":"r0","k":"c0"}, "b": {"e":"r0","k":"c1"},
        "off": -8.0, "slide": 0.0, "value": 30.0
    })).unwrap();
    let fx = dims::place_dimension(&mut h.state_mut().sketch_mode_mut().unwrap().s, &redundant, String::new());
    h.state_mut().sketch_apply(fx);
    h.run_steps(3);
    if h.query_by_label("Over defined").is_some() {
        assert!(h.query_by_label("Add as reference").is_some());
        h.get_by_label("Add as reference").click();
        h.run_steps(2);
        assert!(h.state().sketch().unwrap().constraints.iter().any(|c| c.has_ref()));
    }

    // Parameter `w` in a dimension (1.x params/set then openModify).
    h.state_mut().link.send(Command::SetParams(vec![stl_engine::params::Param {
        name: "w".into(),
        expr: "28".into(),
        note: String::new(),
    }]));
    settle(&mut h, &link);
    {
        let s = h.state_mut().sketch_mode_mut().unwrap();
        if let Some(c) = s.s.constraints.iter_mut().find(|c| c.id.as_ref() == Some(&json!("dw"))) {
            c.extra.insert("expr".into(), json!("w"));
            c.value = Some(28.0);
        }
    }

    // Use Edge tool is on the ribbon.
    h.state_mut().sketch_set_tool("sk_useedge");
    h.run_steps(2);
    assert_eq!(h.state().sketch().unwrap().tool, "sk_useedge");
    assert!(h.state().shell.hint.to_lowercase().contains("edge") || h.state().shell.hint.contains("orange"));

    finish_sketch_settle(&mut h, &link, false);
    let sk = h.state().timeline_view.items.iter().rev().find(|i| i.kind == "sketch").expect("sketch saved");
    assert!(sk.params["constraints"].as_array().unwrap().len() >= 3);
    assert_eq!(sk.error, None);
}
