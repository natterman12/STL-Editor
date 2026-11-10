//! The document model: timeline, rollback, parameters, cache, undo/redo and panic containment.
//! The evaluators here are test-only stand-ins; the real ones arrive in milestone 2.

use serde_json::{Map, Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};
use stl_engine::params::Param;
use stl_engine::{
    Body, Ctx, Document, Feature, FeatureError, FeatureResult, Registry, RollbackAfter, State,
};
use stl_kernel::Solid;

type P = Map<String, Value>;

/// `box`: creates body `b<fid>`, a cube of side `size` (10 when not given).
fn ev_box(st: &mut State, fid: &str, p: &P, res: &mut FeatureResult, _: &Ctx) -> Result<(), FeatureError> {
    let s = p.get("size").and_then(Value::as_f64).unwrap_or(10.0);
    let bid = format!("b{fid}");
    st.bodies.insert(
        bid.clone(),
        Body { name: "Box".into(), solid: Some(Solid::cube([s, s, s], false)), mesh: None },
    );
    res.created.push(bid);
    Ok(())
}

/// `grow`: scales body `body` (the first body when not given) by `k`.
fn ev_grow(st: &mut State, _: &str, p: &P, res: &mut FeatureResult, _: &Ctx) -> Result<(), FeatureError> {
    let bid = match p.get("body").and_then(Value::as_str) {
        Some(b) => b.to_string(),
        None => st.bodies.keys().next().cloned().unwrap_or_default(),
    };
    let k = p.get("k").and_then(Value::as_f64).ok_or_else(|| FeatureError("k is missing".into()))?;
    let solid = st.body(&bid)?.require_solid()?.scale([k, k, k]);
    st.bodies.get_mut(&bid).unwrap().solid = Some(solid);
    res.modified.push(bid);
    Ok(())
}

fn ev_fail(_: &mut State, _: &str, _: &P, _: &mut FeatureResult, _: &Ctx) -> Result<(), FeatureError> {
    Err(FeatureError("nope".into()))
}

fn ev_boom(st: &mut State, _: &str, _: &P, _: &mut FeatureResult, _: &Ctx) -> Result<(), FeatureError> {
    st.bodies.clear(); // a half-done change that must not leak out
    panic!("kaboom");
}

fn ev_noop(_: &mut State, _: &str, _: &P, _: &mut FeatureResult, _: &Ctx) -> Result<(), FeatureError> {
    Ok(())
}

static COUNTED: AtomicUsize = AtomicUsize::new(0);

/// Used by `cache_reuses_unchanged_prefix` only (tests run in parallel).
fn ev_counted(st: &mut State, fid: &str, p: &P, res: &mut FeatureResult, c: &Ctx) -> Result<(), FeatureError> {
    COUNTED.fetch_add(1, Ordering::SeqCst);
    ev_box(st, fid, p, res, c)
}

/// `two_big`: creates two large bodies.
fn ev_two_big(st: &mut State, _: &str, _: &P, res: &mut FeatureResult, _: &Ctx) -> Result<(), FeatureError> {
    for bid in ["big1", "big2"] {
        let s = Solid::sphere(10.0, 256);
        st.bodies.insert(bid.into(), Body { name: bid.into(), solid: Some(s), mesh: None });
        res.created.push(bid.into());
    }
    Ok(())
}

fn doc() -> Document {
    Document::new(
        Registry::new()
            .with("box", ev_box)
            .with("grow", ev_grow)
            .with("fail", ev_fail)
            .with("boom", ev_boom)
            .with("counted", ev_counted)
            .with("primitive", ev_box)
            .with("move", ev_noop)
            .with("two_big", ev_two_big)
            .with("noop", ev_noop),
    )
}

fn obj(v: Value) -> P {
    v.as_object().unwrap().clone()
}

fn param(name: &str, expr: &str) -> Param {
    Param { name: name.into(), expr: expr.into(), note: String::new() }
}

fn volumes(d: &Document) -> Vec<f64> {
    d.state.bodies.values().map(|b| b.solid.as_ref().unwrap().volume()).collect()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
}

fn ids(d: &Document) -> Vec<&str> {
    d.features.iter().map(|f| f.id.as_str()).collect()
}

fn body_ids(d: &Document) -> Vec<&str> {
    d.state.bodies.keys().map(String::as_str).collect()
}

fn features_json(d: &Document) -> String {
    serde_json::to_string(&d.features).unwrap()
}

#[test]
fn add_then_evaluate_builds_bodies() {
    let mut d = doc();
    let a = d.add_feature("box", obj(json!({"size": 2})), None).unwrap();
    let b = d.add_feature("box", P::new(), Some("Big".into())).unwrap();
    assert_eq!((a.as_str(), b.as_str()), ("f1", "f2"));
    assert_eq!(body_ids(&d), ["bf1", "bf2"]);
    assert!(near(volumes(&d)[0], 8.0) && near(volumes(&d)[1], 1000.0));
    assert_eq!(d.features[1].name.as_deref(), Some("Big"));
    assert_eq!(d.features[0].kind, "box");

    let res = d.evaluate();
    assert_eq!(res.len(), 2);
    assert_eq!(res[0].id, "f1");
    assert_eq!(res[0].error, None);
    assert_eq!(res[0].created, ["bf1"]);
    assert_eq!(d.results().len(), 2);
    assert!(d.dirty);

    assert_eq!(d.body_name("bf1"), "Box");
    d.set_body_props("bf1", Some("Grün".into()), Some("#ff0000".into()), Some(true));
    assert_eq!(d.body_name("bf1"), "Grün");
    assert_eq!(d.ui["colors"]["bf1"], json!("#ff0000"));
    assert_eq!(d.ui["hidden"], json!(["bf1"]));
    d.set_body_props("bf1", None, None, Some(false));
    assert_eq!(d.ui["hidden"], json!([]));

    assert_eq!(d.add_feature("nothing", P::new(), None).unwrap_err().0, "Unknown feature nothing.");

    // ids are shared by features and sources, and never reused
    assert_eq!(d.add_source("Teil ö.STL", vec![1, 2, 3]), "s3");
    assert_eq!(d.sources["s3"].ext, ".stl");
    assert_eq!(d.new_id("f"), "f4");
}

#[test]
fn failing_feature_is_refused() {
    let mut d = doc();
    d.add_feature("box", P::new(), None).unwrap();
    let before = features_json(&d);
    let e = d.add_feature("fail", P::new(), None).unwrap_err();
    assert_eq!(e.0, "nope");
    assert_eq!(features_json(&d), before);
    assert_eq!(body_ids(&d), ["bf1"]);
    // a refused feature leaves nothing to undo beyond the box
    assert!(d.undo());
    assert!(!d.undo());

    // an edit that fails is refused too
    let mut d = doc();
    d.add_feature("box", P::new(), None).unwrap();
    let g = d.add_feature("grow", obj(json!({"k": 2})), None).unwrap();
    let before = features_json(&d);
    let e = d.edit_feature(&g, Some(obj(json!({"k": 2, "body": "gone"}))), None, None, RollbackAfter::Keep);
    assert_eq!(e.unwrap_err().0, "Body 'gone' no longer exists.");
    assert_eq!(features_json(&d), before);
}

#[test]
fn expressions_rebuild_on_param_change() {
    let mut d = doc();
    d.set_params(vec![param("s", "1")]).unwrap();
    d.add_feature("box", P::new(), None).unwrap();
    d.add_feature("grow", obj(json!({"k": "=2*s"})), None).unwrap();
    assert!(near(volumes(&d)[0], 8000.0));

    d.set_params(vec![param("s", "1.5")]).unwrap();
    assert!(near(volumes(&d)[0], 27000.0));
    assert_eq!(d.param_values().values["s"], 1.5);

    // the stored feature keeps the expression, not the number
    assert_eq!(d.features[1].params["k"], json!("=2*s"));

    let e = d.add_feature("grow", obj(json!({"k": "=2*zz"})), None).unwrap_err();
    assert_eq!(e.0, "Parameter: there's no parameter \"zz\" (in =2*zz).");

    // removing a parameter a feature uses shows on that feature and keeps the document usable
    d.set_params(vec![]).unwrap();
    assert_eq!(d.results()[1].error.as_deref(), Some("Parameter: there's no parameter \"s\" (in =2*s)."));
    assert!(near(volumes(&d)[0], 1000.0));
    assert!(d.undo());
    assert!(near(volumes(&d)[0], 27000.0));
}

#[test]
fn set_params_messages() {
    let mut d = doc();
    let msg = |d: &mut Document, list: Vec<Param>| d.set_params(list).unwrap_err().0;
    assert_eq!(
        msg(&mut d, vec![param("2x", "1")]),
        "\"2x\" can't be a parameter name: use letters, digits and _, starting with a letter."
    );
    assert_eq!(msg(&mut d, vec![param("sqrt", "1")]), "\"sqrt\" is a built-in name; pick another.");
    assert_eq!(msg(&mut d, vec![param("w", "  ")]), "Give \"w\" a value.");
    assert_eq!(msg(&mut d, vec![param("w", "1"), param("w", "2")]), "\"w\" is there twice.");
    assert!(d.params.is_empty());
    assert!(!d.undo(), "a refused list must not leave an undo step");

    // names and values are trimmed, and empty rows are dropped
    let note = Param { name: " w ".into(), expr: " 4 ".into(), note: "größe".into() };
    d.set_params(vec![note, param("", "")]).unwrap();
    assert_eq!(d.params, vec![Param { name: "w".into(), expr: "4".into(), note: "größe".into() }]);
    assert_eq!(d.param_values().values["w"], 4.0);
}

#[test]
fn rollback_and_shift() {
    let mut d = doc();
    let a = d.add_feature("box", P::new(), None).unwrap();
    let b = d.add_feature("box", P::new(), None).unwrap();
    d.set_rollback(Some(1));
    assert_eq!(d.rollback, Some(1));
    assert_eq!(body_ids(&d), ["bf1"]);
    assert_eq!(d.results().len(), 1);

    // a feature added while rolled back goes in at the bar, and the bar moves past it
    let c = d.add_feature("box", P::new(), None).unwrap();
    assert_eq!(ids(&d), [a.as_str(), c.as_str(), b.as_str()]);
    assert_eq!(d.rollback, Some(2));
    assert_eq!(body_ids(&d), ["bf1", "bf3"]);

    d.set_rollback(Some(0));
    assert!(d.state.bodies.is_empty());

    // at or past the end means "not rolled back"
    d.set_rollback(Some(3));
    assert_eq!(d.rollback, None);
    assert_eq!(body_ids(&d), ["bf1", "bf3", "bf2"]);
}

#[test]
fn delete_and_move_adjust_rollback() {
    let mut d = doc();
    let a = d.add_feature("box", P::new(), None).unwrap();
    let b = d.add_feature("box", P::new(), None).unwrap();
    let c = d.add_feature("box", P::new(), None).unwrap();
    d.set_rollback(Some(2));

    d.delete_feature(&a).unwrap(); // before the bar: the bar moves back
    assert_eq!(d.rollback, Some(1));
    assert_eq!(ids(&d), [b.as_str(), c.as_str()]);
    assert_eq!(body_ids(&d), ["bf2"]);

    d.move_feature(&c, 0).unwrap(); // a move leaves the bar where it is
    assert_eq!(d.rollback, Some(1));
    assert_eq!(ids(&d), [c.as_str(), b.as_str()]);
    assert_eq!(body_ids(&d), ["bf3"]);

    d.move_feature(&c, 99).unwrap(); // clamped to the end
    assert_eq!(ids(&d), [b.as_str(), c.as_str()]);

    d.delete_feature(&c).unwrap(); // after the bar: the bar stays
    assert_eq!(d.rollback, Some(1));

    assert_eq!(d.delete_feature("f99").unwrap_err().0, "That feature no longer exists.");
    assert_eq!(d.move_feature("f99", 0).unwrap_err().0, "That feature no longer exists.");
}

#[test]
fn undo_redo_round_trip() {
    let mut d = doc();
    d.add_feature("box", P::new(), None).unwrap();
    d.add_feature("grow", obj(json!({"k": 2})), None).unwrap();
    d.add_feature("box", obj(json!({"size": 3})), None).unwrap();
    let original = features_json(&d);
    let vols = volumes(&d);

    assert!(d.undo() && d.undo() && d.undo());
    assert!(d.features.is_empty());
    assert!(d.state.bodies.is_empty());
    assert!(!d.undo());

    assert!(d.redo() && d.redo() && d.redo());
    assert!(!d.redo());
    assert_eq!(features_json(&d), original);
    assert_eq!(volumes(&d), vols);

    // a new change drops the redo steps
    assert!(d.undo());
    d.add_feature("box", P::new(), None).unwrap();
    assert!(!d.redo());

    // an edit session: undo lands on the timeline as it was before the session
    let first = d.features[0].id.clone();
    d.set_rollback(Some(1));
    d.edit_feature(&first, Some(obj(json!({"size": 4}))), None, None, RollbackAfter::Set(None)).unwrap();
    assert_eq!(d.rollback, None);
    assert!(near(volumes(&d)[0], 64.0 * 8.0));
    assert!(d.undo());
    assert_eq!(d.rollback, None);
    assert!(near(volumes(&d)[0], 8000.0));
}

#[test]
fn auto_names() {
    let mut d = doc();
    d.add_feature("primitive", obj(json!({"shape": "box"})), None).unwrap();
    d.add_feature("primitive", obj(json!({"shape": "box"})), None).unwrap();
    d.add_feature("primitive", obj(json!({"shape": "cylinder"})), None).unwrap();
    d.add_feature("move", P::new(), None).unwrap();
    d.add_feature("box", P::new(), None).unwrap(); // no label: the type itself
    d.add_feature("move", P::new(), Some(String::new())).unwrap(); // an empty name counts as none
    let names: Vec<&str> = d.features.iter().map(|f| f.name.as_deref().unwrap()).collect();
    assert_eq!(names, ["Box1", "Box2", "Cylinder1", "Move1", "box1", "Move2"]);

    assert_eq!(stl_engine::LABELS.len(), 37);
    assert_eq!(stl_engine::LABELS[0], ("import", "Import"));
    assert_eq!(stl_engine::LABELS[36], ("hinge", "Living hinge"));
    assert!(stl_engine::LABELS.contains(&("bedsplit", "Fit to Bed")));
}

#[test]
fn suppressed_is_skipped() {
    let mut d = doc();
    let a = d.add_feature("box", P::new(), None).unwrap();
    d.add_feature("box", P::new(), None).unwrap();

    d.edit_feature(&a, None, None, Some(true), RollbackAfter::Keep).unwrap();
    assert!(d.features[0].suppressed);
    assert_eq!(body_ids(&d), ["bf2"]);
    assert!(d.results()[0].suppressed);
    assert_eq!(d.results()[0].error, None);
    assert!(d.results()[0].created.is_empty());
    assert!(!d.results()[1].suppressed);

    d.edit_feature(&a, None, Some("First".into()), Some(false), RollbackAfter::Keep).unwrap();
    assert_eq!(body_ids(&d), ["bf1", "bf2"]);
    assert_eq!(d.features[0].name.as_deref(), Some("First"));

    // a suppressed feature that would fail is still skipped
    let f: Feature = serde_json::from_value(json!({"id": "x", "type": "fail", "suppressed": true})).unwrap();
    d.features.push(f);
    let res = d.evaluate();
    assert_eq!(res[2].error, None);
    assert!(res[2].suppressed);
}

#[test]
fn panicking_feature_is_contained() {
    let mut d = doc();
    // a panicking feature can't be added (the trial run refuses it)...
    d.add_feature("box", P::new(), None).unwrap();
    let e = d.add_feature("boom", P::new(), None).unwrap_err();
    assert_eq!(e.0, "Internal error: kaboom");
    assert_eq!(d.features.len(), 1);

    // ...so put one in the timeline the way an opened project would
    for v in [json!({"id": "f8", "type": "boom"}), json!({"id": "f9", "type": "grow", "params": {"k": 2}})] {
        d.features.push(serde_json::from_value(v).unwrap());
    }
    let res = d.evaluate();
    assert_eq!(res.len(), 3);
    assert_eq!(res[0].error, None);
    assert!(res[1].error.as_deref().unwrap().starts_with("Internal error:"));
    assert_eq!(res[1].error.as_deref(), Some("Internal error: kaboom"));
    assert_eq!(res[2].error, None, "the feature after the panic still runs");
    assert_eq!(body_ids(&d), ["bf1"], "on the state before the panic");
    assert!(near(volumes(&d)[0], 8000.0));

    // the document is still usable: add, undo, redo
    d.add_feature("box", P::new(), None).unwrap();
    assert_eq!(d.features.len(), 4);
    assert_eq!(d.state.bodies.len(), 2);
    assert!(d.undo());
    assert_eq!(d.features.len(), 3);
    assert_eq!(body_ids(&d), ["bf1"]);
    assert!(d.results()[1].error.as_deref().unwrap().starts_with("Internal error:"));
    assert!(d.redo());
    assert_eq!(d.state.bodies.len(), 2);

    // a preview of a panicking feature reports it too
    let (st, r) = d.preview("boom", P::new(), None);
    assert_eq!(r.error.as_deref(), Some("Internal error: kaboom"));
    assert_eq!(st.bodies.len(), 2);

    // a feature type this build doesn't know is an error on that feature, not a crash
    d.features.push(serde_json::from_value(json!({"id": "f20", "type": "warp"})).unwrap());
    assert_eq!(d.evaluate().last().unwrap().error.as_deref(), Some("Unknown feature warp."));
}

#[test]
fn cache_reuses_unchanged_prefix() {
    let mut d = doc();
    let a = d.add_feature("counted", P::new(), None).unwrap();
    assert_eq!(COUNTED.load(Ordering::SeqCst), 1, "the trial run is reused, not repeated");
    d.evaluate();
    d.evaluate();
    assert_eq!(COUNTED.load(Ordering::SeqCst), 1);

    // changes after it don't rebuild it
    let g = d.add_feature("grow", obj(json!({"k": 2})), None).unwrap();
    d.edit_feature(&g, Some(obj(json!({"k": 3}))), None, None, RollbackAfter::Keep).unwrap();
    d.set_rollback(Some(1));
    d.set_rollback(None);
    assert_eq!(COUNTED.load(Ordering::SeqCst), 1);
    assert!(near(volumes(&d)[0], 27000.0));

    // a preview doesn't change the document, and an edit preview is reused by the edit
    let (st, r) = d.preview("box", P::new(), None);
    assert_eq!((r.id.as_str(), st.bodies.len()), ("preview", 2));
    assert_eq!(d.state.bodies.len(), 1);
    let (_, r) = d.preview("counted", obj(json!({"size": 2})), Some(&a));
    assert_eq!(r.error, None);
    assert_eq!(COUNTED.load(Ordering::SeqCst), 2);
    assert!(near(volumes(&d)[0], 27000.0), "a preview leaves the document as it was");

    // an edit runs the feature once (its trial run); the rebuild reuses that
    d.edit_feature(&a, Some(obj(json!({"size": 2}))), None, None, RollbackAfter::Keep).unwrap();
    assert_eq!(COUNTED.load(Ordering::SeqCst), 3);
    assert!(near(volumes(&d)[0], 8.0 * 27.0));
    d.evaluate();
    assert_eq!(COUNTED.load(Ordering::SeqCst), 3);
}

#[test]
fn unchanged_solids_are_shared_not_copied() {
    let mut d = doc();
    d.add_feature("two_big", P::new(), None).unwrap();
    for _ in 0..4 {
        d.add_feature("noop", P::new(), None).unwrap();
    }
    assert_eq!(d.features.len(), 5);
    for b in d.state.bodies.values() {
        assert!(b.solid.as_ref().unwrap().num_tri() > 10_000);
    }
    // a preview starts from the cached state: its solids are the same objects
    let t = std::time::Instant::now();
    let (st, r) = d.preview("noop", P::new(), None);
    let took = t.elapsed();
    assert_eq!(r.error, None);
    for (id, b) in &st.bodies {
        assert!(b.solid.as_ref().unwrap().shares_with(d.state.bodies[id].solid.as_ref().unwrap()), "{id} was copied");
    }
    assert!(took < std::time::Duration::from_millis(200), "preview took {took:?}");
    // re-evaluating from the cache also keeps the same solids
    let before = d.state.clone();
    d.evaluate();
    for (id, b) in &d.state.bodies {
        assert!(b.solid.as_ref().unwrap().shares_with(before.bodies[id].solid.as_ref().unwrap()));
    }
}
