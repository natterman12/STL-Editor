//! The kernel's buffer pool is kept between features, and emptied at the end of a rebuild and
//! when the document is dropped. One test only: the pool is process-wide.

use serde_json::{Map, Value};
use std::time::Duration;
use stl_engine::{Body, Ctx, Document, FeatureError, RollbackAfter, FeatureResult, Registry, State};
use stl_kernel::{release_scratch, scratch_held_bytes, set_idle_timeout, Solid};

type P = Map<String, Value>;

fn big_boolean() -> Solid {
    let ball = Solid::sphere(20.0, 400);
    let cube = Solid::cube([25.0, 25.0, 25.0], true);
    ball.difference(&cube)
}

fn ev_cut(st: &mut State, fid: &str, _: &P, res: &mut FeatureResult, _: &Ctx) -> Result<(), FeatureError> {
    let bid = format!("b{fid}");
    st.bodies.insert(bid.clone(), Body { name: "Cut".into(), solid: Some(big_boolean()), mesh: None });
    res.created.push(bid);
    Ok(())
}

fn ev_cut_then_fail(_: &mut State, _: &str, _: &P, _: &mut FeatureResult, _: &Ctx) -> Result<(), FeatureError> {
    let _ = big_boolean();
    Err(FeatureError("nope".into()))
}

#[test]
fn pool_is_kept_between_features_and_released_on_rebuild_end_and_close() {
    // long enough that the idle release cannot fire during the test
    set_idle_timeout(Duration::from_secs(600));
    release_scratch();
    let mut doc = Document::new(Registry::new().with("cut", ev_cut).with("cut_fail", ev_cut_then_fail));
    doc.add_feature("cut", Map::new(), None).unwrap();
    assert!(scratch_held_bytes() > 0, "kept after a successful feature");
    assert!(doc.add_feature("cut_fail", Map::new(), None).is_err());
    assert!(scratch_held_bytes() > 0, "kept after a failing feature");
    // an edit of the first feature makes the rebuild evaluate the later one again: the pool is
    // emptied at the end of that rebuild
    let first = doc.features[0].id.clone();
    doc.add_feature("cut", Map::new(), None).unwrap();
    let mut p = Map::new();
    p.insert("x".into(), Value::from(1));
    doc.edit_feature(&first, Some(p), None, None, RollbackAfter::Keep).unwrap();
    assert_eq!(scratch_held_bytes(), 0, "after a rebuild");
    // closing (dropping) the document empties it too
    doc.add_feature("cut", Map::new(), None).unwrap();
    assert!(scratch_held_bytes() > 0, "kept after another feature");
    drop(doc);
    assert_eq!(scratch_held_bytes(), 0, "after the document is dropped");
}
