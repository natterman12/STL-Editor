//! Support Fins rebuild for split and moved parts (Nathan 2026-11-17): a Move, Scale, Lay Flat
//! or Split added at the end of the History for a part that has supports puts the part's
//! Support Fins step last again, on the parts as they are now, so the fins are made fresh.
//! `Document::keep_supports_last`.

use serde_json::{json, Map, Value};
use stl_engine::{Body, Ctx, Document, FeatureError, FeatureResult, Registry, State};
use stl_kernel::Solid;

fn obj(v: Value) -> Map<String, Value> {
    v.as_object().unwrap().clone()
}

/// A stand-in for Support Fins that needs no JavaScript engine: one 2 x 2 mm fin, 5 mm tall,
/// under the middle of each body it is given, made as a body of its own held by that body.
fn fake_fins(state: &mut State, fid: &str, p: &Map<String, Value>, res: &mut FeatureResult, _: &Ctx<'_>) -> Result<(), FeatureError> {
    let ids: Vec<String> = match p.get("bodies").and_then(Value::as_array) {
        Some(a) if !a.is_empty() => a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
        _ => p.get("body").and_then(Value::as_str).map(|s| vec![s.to_string()]).unwrap_or_default(),
    };
    for (i, bid) in ids.iter().enumerate() {
        let b = state.body(bid)?.clone();
        let bb = b.solid.as_ref().unwrap().bounding_box();
        let c = [(bb.min[0] + bb.max[0]) / 2.0, (bb.min[1] + bb.max[1]) / 2.0];
        let fin = Solid::cube([2.0, 2.0, 5.0], false).translate([c[0] - 1.0, c[1] - 1.0, bb.min[2] - 5.0]);
        let nid = if i == 0 { fid.to_string() } else { format!("{fid}.{}", i + 1) };
        state.bodies.insert(nid.clone(), Body { name: format!("{} supports", b.name), solid: Some(fin), mesh: None });
        state.attached.insert(nid.clone(), bid.clone());
        res.created.push(nid);
    }
    Ok(())
}

/// A 40 x 10 x 10 box 10 mm up (f1) with Support Fins (f2) as a body of their own.
fn boxed(fins: Value) -> Document {
    let mut doc = Document::new(Registry::builtin().with("supportfins", fake_fins));
    doc.add_feature(
        "primitive",
        obj(json!({"shape": "box", "length": 40, "width": 10, "height": 10, "anchor": "corner", "plane": "XY", "operation": "new"})),
        None,
    )
    .unwrap();
    doc.add_feature("move", obj(json!({"bodies": ["f1"], "translate": [0, 0, 10], "rotate": [0, 0, 0], "pivot": "center"})), None).unwrap();
    doc.add_feature("supportfins", obj(fins), None).unwrap();
    doc
}

fn split_f1(doc: &mut Document) -> String {
    doc.add_feature("split", obj(json!({"body": "f1", "plane": {"origin": [20, 0, 0], "normal": [1, 0, 0]}, "keep": "both"})), None).unwrap()
}

/// Add a step at the end the way the app does: add it, then keep the supports last.
fn add(doc: &mut Document, kind: &str, p: Value) -> (String, Vec<String>) {
    let id = doc.add_feature(kind, obj(p), None).unwrap();
    let notes = doc.keep_supports_last(&id);
    (id, notes)
}

fn order(doc: &Document) -> Vec<String> {
    doc.features.iter().map(|f| f.id.clone()).collect()
}

/// The middle (x, y) of every fin body, by the part it holds up.
fn fins_under(doc: &Document, part: &str) -> Vec<[f64; 2]> {
    let mut v: Vec<[f64; 2]> = doc
        .state
        .attached
        .iter()
        .filter(|(_, p)| p.as_str() == part)
        .filter_map(|(s, _)| doc.state.bodies.get(s))
        .map(|b| {
            let bb = b.solid.as_ref().unwrap().bounding_box();
            [(bb.min[0] + bb.max[0]) / 2.0, (bb.min[1] + bb.max[1]) / 2.0]
        })
        .collect();
    v.sort_by(|a, b| a[0].total_cmp(&b[0]));
    v
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6
}

fn no_errors(doc: &Document) {
    for r in doc.results() {
        assert_eq!(r.error, None, "step {} failed", r.id);
    }
}

#[test]
fn moving_a_split_half_rebuilds_the_fins_for_both_halves() {
    let mut doc = boxed(json!({"body": "f1", "output": "separate"}));
    let split = split_f1(&mut doc);
    let half = format!("{split}.2");
    let (mv, notes) = add(&mut doc, "move", json!({"bodies": [half], "translate": [0, 30, 0], "rotate": [0, 0, 0], "pivot": "center"}));
    no_errors(&doc);
    // f1 box, f2 lift, f3 Support Fins: the fins go after the split and the move
    assert_eq!(order(&doc), vec!["f1".to_string(), "f2".into(), split.clone(), mv.clone(), "f3".into()]);
    let sf = doc.features.last().unwrap();
    assert_eq!(sf.params["bodies"], json!(["f1", half]), "it supports both halves now");
    // fresh fins: one under the middle of each half where it is now
    assert!(near(fins_under(&doc, "f1")[0], [30.0, 5.0]), "{:?}", fins_under(&doc, "f1"));
    assert!(near(fins_under(&doc, &half)[0], [10.0, 35.0]), "{:?}", fins_under(&doc, &half));
    assert!(notes.iter().any(|n| n.contains("rebuilt")), "{notes:?}");
    // one Undo takes the Move out and puts Support Fins back where it was
    assert!(doc.undo());
    assert_eq!(order(&doc), vec!["f1".to_string(), "f2".into(), "f3".into(), split]);
}

#[test]
fn a_split_at_the_end_rebuilds_the_fins_for_both_halves() {
    let mut doc = boxed(json!({"body": "f1", "output": "separate"}));
    let (split, _) = add(&mut doc, "split", json!({"body": "f1", "plane": {"origin": [20, 0, 0], "normal": [1, 0, 0]}, "keep": "both"}));
    no_errors(&doc);
    assert_eq!(order(&doc).last().map(String::as_str), Some("f3"));
    assert!(near(fins_under(&doc, "f1")[0], [30.0, 5.0]));
    assert!(near(fins_under(&doc, &format!("{split}.2"))[0], [10.0, 5.0]));
}

#[test]
fn keeping_one_half_rebuilds_the_fins_for_that_half_only() {
    let mut doc = boxed(json!({"body": "f1", "output": "separate"}));
    add(&mut doc, "split", json!({"body": "f1", "plane": {"origin": [20, 0, 0], "normal": [1, 0, 0]}, "keep": "above"}));
    no_errors(&doc);
    let sf = doc.features.last().unwrap();
    assert_eq!(sf.params["bodies"], json!(["f1"]));
    assert_eq!(doc.state.attached.len(), 1);
    assert!(near(fins_under(&doc, "f1")[0], [30.0, 5.0]));
}

#[test]
fn walls_drawn_by_hand_are_dropped_with_a_note() {
    let wall = json!({"shape": "line", "points": [[0, 0, 0], [5, 0, 0]]});
    let mut doc = boxed(json!({"body": "f1", "output": "separate", "walls": [wall]}));
    let (_, notes) = add(&mut doc, "move", json!({"bodies": ["f1"], "translate": [0, 30, 0], "rotate": [0, 0, 0], "pivot": "center"}));
    let sf = doc.features.last().unwrap();
    assert_eq!(sf.kind, "supportfins");
    assert!(sf.params.get("walls").is_none(), "{:?}", sf.params);
    assert!(notes.iter().any(|n| n.contains("by hand")), "{notes:?}");
}

#[test]
fn a_copy_leaves_the_supports_where_they_are() {
    let mut doc = boxed(json!({"body": "f1", "output": "separate"}));
    let (mv, notes) = add(&mut doc, "move", json!({"bodies": ["f1"], "translate": [0, 30, 0], "copy": true}));
    assert_eq!(order(&doc).last(), Some(&mv));
    assert!(notes.is_empty());
}

#[test]
fn a_step_that_uses_the_fins_keeps_them_where_they_are() {
    let mut doc = boxed(json!({"body": "f1", "output": "separate"}));
    // the fins body (f3) moved on its own: Support Fins can't go after that step
    doc.add_feature("move", obj(json!({"bodies": ["f3"], "translate": [5, 0, 0]})), None).unwrap();
    let (mv, notes) = add(&mut doc, "move", json!({"bodies": ["f1"], "translate": [0, 30, 0], "rotate": [0, 0, 0], "pivot": "center"}));
    no_errors(&doc);
    assert_eq!(order(&doc).last(), Some(&mv));
    assert!(notes.iter().any(|n| n.contains("aren't rebuilt")), "{notes:?}");
}

#[test]
fn other_parts_supports_stay_put() {
    let mut doc = boxed(json!({"body": "f1", "output": "separate"}));
    doc.add_feature(
        "primitive",
        obj(json!({"shape": "box", "length": 5, "width": 5, "height": 5, "anchor": "corner", "plane": "XY", "operation": "new"})),
        None,
    )
    .unwrap();
    let before = order(&doc);
    let other = before.last().unwrap().clone();
    let (mv, notes) = add(&mut doc, "move", json!({"bodies": [other], "translate": [0, 30, 0]}));
    let mut want = before;
    want.push(mv);
    assert_eq!(order(&doc), want);
    assert!(notes.is_empty());
}
