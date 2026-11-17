//! The cut tint's shape (milestone 7, spec 2.14): a cutting step reports the shape it removes
//! while previewing (`FeatureResult::cutter`), and never when it is committed.

use serde_json::{Map, Value, json};
use stl_engine::{Document, Registry};
use stl_kernel::MeshArrays;

fn obj(v: Value) -> Map<String, Value> {
    v.as_object().unwrap().clone()
}

/// A 20 mm cube on the bed (corner at the origin) and a sketch on its top face.
fn doc_with(entities: Value) -> Document {
    let mut doc = Document::new(Registry::builtin());
    doc.add_feature(
        "primitive",
        obj(json!({"shape": "box", "length": 20, "width": 20, "height": 20, "anchor": "corner", "plane": "XY", "operation": "new"})),
        None,
    )
    .unwrap();
    doc.add_feature(
        "sketch",
        obj(json!({"plane": {"origin": [0, 0, 20], "normal": [0, 0, 1]}, "entities": entities})),
        None,
    )
    .unwrap();
    doc
}

fn volume(m: &MeshArrays) -> f64 {
    let p = |i: u32| {
        let k = i as usize * 3;
        [
            f64::from(m.positions[k]),
            f64::from(m.positions[k + 1]),
            f64::from(m.positions[k + 2]),
        ]
    };
    m.triangles
        .chunks_exact(3)
        .map(|t| {
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.0
        })
        .sum()
}

#[test]
fn extrude_cut_reports_its_cutter_only_in_a_preview() {
    let mut doc = doc_with(json!([{"type": "rect", "id": "r1", "x": 5, "y": 5, "w": 10, "h": 10}]));
    let params = obj(json!({"sketch": "f2", "operation": "cut", "distance": 8, "direction": "one", "targets": ["f1"]}));
    let (_, res) = doc.preview("extrude", params.clone(), None);
    assert_eq!(res.error, None);
    let cutter = res.cutter.as_ref().expect("a preview of a cut has its cutter");
    assert!(
        (volume(cutter) - 800.0).abs() < 1e-3,
        "a 10 x 10 x 8 pocket: {}",
        volume(cutter)
    );
    // committed: the result carries none
    doc.add_feature("extrude", params, None).unwrap();
    assert!(
        doc.results().iter().all(|r| r.cutter.is_none()),
        "a committed step has no cutter"
    );
    // a join or a new body has none even in a preview
    let (_, res) = doc.preview(
        "extrude",
        obj(json!({"sketch": "f2", "operation": "join", "distance": 4})),
        None,
    );
    assert!(res.cutter.is_none());
}

#[test]
fn revolve_cut_reports_its_cutter() {
    let mut doc = doc_with(json!([{"type": "rect", "id": "r1", "x": 5, "y": 0, "w": 5, "h": 10}]));
    let (_, res) = doc.preview(
        "revolve",
        obj(json!({"sketch": "f2", "operation": "cut", "axis": "y", "angle": 360, "targets": ["f1"]})),
        None,
    );
    assert_eq!(res.error, None);
    let cutter = res.cutter.as_ref().expect("a revolve cut has its cutter");
    // a ring 5..10 mm out and 10 mm tall (a 128-sided approximation of the circle)
    let ring = std::f64::consts::PI * (100.0 - 25.0) * 10.0;
    assert!(
        (volume(cutter) - ring).abs() < ring * 0.01,
        "{} vs {ring}",
        volume(cutter)
    );
}

#[test]
fn hinge_reports_the_slot() {
    let mut doc = Document::new(Registry::builtin());
    doc.add_feature("primitive", obj(json!({"shape": "box", "length": 20, "width": 20, "height": 20, "anchor": "corner", "plane": "XY", "operation": "new"})), None).unwrap();
    let (_, res) = doc.preview(
        "hinge",
        obj(json!({"plane": "XZ", "slot": 2, "thickness": 1, "targets": ["f1"]})),
        None,
    );
    assert_eq!(res.error, None);
    assert!(volume(res.cutter.as_ref().expect("the slot")) > 0.0);
    let (_, res) = {
        doc.add_feature(
            "hinge",
            obj(json!({"plane": "XZ", "slot": 2, "thickness": 1, "targets": ["f1"]})),
            None,
        )
        .unwrap();
        ((), doc.results().last().cloned().unwrap())
    };
    assert!(res.cutter.is_none());
}

#[test]
fn split_reports_the_discarded_side() {
    let mut doc = Document::new(Registry::builtin());
    doc.add_feature("primitive", obj(json!({"shape": "box", "length": 20, "width": 20, "height": 20, "anchor": "corner", "plane": "XY", "operation": "new"})), None).unwrap();
    let plane = json!({"origin": [10, 10, 5], "normal": [0, 0, 1]});
    let keep_above = obj(json!({"body": "f1", "plane": plane.clone(), "keep": "above", "connector": "none"}));
    let (_, res) = doc.preview("split", keep_above, None);
    assert_eq!(res.error, None);
    assert!(
        (volume(res.cutter.as_ref().expect("the side that goes")) - 2000.0).abs() < 1e-3,
        "20 x 20 x 5 below the plane"
    );
    let keep_below = obj(json!({"body": "f1", "plane": plane.clone(), "keep": "below", "connector": "none"}));
    let (_, res) = doc.preview("split", keep_below, None);
    assert!(
        (volume(res.cutter.as_ref().unwrap()) - 6000.0).abs() < 1e-3,
        "20 x 20 x 15 above"
    );
    // keeping both throws nothing away
    let both = obj(json!({"body": "f1", "plane": plane, "keep": "both", "connector": "none"}));
    let (_, res) = doc.preview("split", both, None);
    assert!(res.cutter.is_none());
}

#[test]
fn hole_and_thread_cut_report_their_cutters() {
    let mut doc = Document::new(Registry::builtin());
    doc.add_feature("primitive", obj(json!({"shape": "box", "length": 20, "width": 20, "height": 20, "anchor": "corner", "plane": "XY", "operation": "new"})), None).unwrap();
    let hole = obj(
        json!({"point": [10, 10, 20], "normal": [0, 0, 1], "diameter": 4, "through": false, "depth": 5, "tip": false, "targets": ["f1"]}),
    );
    let (_, res) = doc.preview("hole", hole.clone(), None);
    assert_eq!(res.error, None);
    let v = volume(res.cutter.as_ref().expect("a hole has its cutter"));
    assert!(v > 60.0 && v < 70.0, "a 4 mm hole 5 mm deep (plus a sliver above): {v}");
    doc.add_feature("hole", hole, None).unwrap();
    assert!(doc.results().iter().all(|r| r.cutter.is_none()));
    let (_, res) = doc.preview(
        "thread",
        obj(json!({"kind": "internal", "diameter": 8, "pitch": 1.25, "length": 6, "targets": ["f1"]})),
        None,
    );
    assert_eq!(res.error, None);
    assert!(volume(res.cutter.as_ref().expect("an internal thread cuts")) > 0.0);
    let (_, res) = doc.preview(
        "thread",
        obj(json!({"kind": "external", "diameter": 8, "pitch": 1.25, "length": 6})),
        None,
    );
    assert!(res.cutter.is_none(), "an external thread makes a new body");
}
