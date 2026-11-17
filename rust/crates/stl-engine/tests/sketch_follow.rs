//! Sketches follow their body (milestone 7, spec 2.9): a sketch whose plane was picked on a
//! body's face moves with that body's later Move, Rotate and Mirror steps, worked out each time
//! the project runs. The stored plane is never rewritten. Sketches on origin or user planes stay.

use serde_json::{Map, Value, json};
use stl_engine::{Document, Registry, RollbackAfter, stlx};
use stl_kernel::{Aabb, Frame};

fn obj(v: Value) -> Map<String, Value> {
    v.as_object().unwrap().clone()
}

/// The plane value the app stores for a face picked on body `body` (tools/fields/plane.rs).
fn face(origin: [f64; 3], normal: [f64; 3], xdir: [f64; 3], body: &str) -> Value {
    json!({"origin": origin, "normal": normal, "xdir": xdir, "label": "Face of Box", "body": body})
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Value {
    json!([{"type": "rect", "id": "r1", "x": x, "y": y, "w": w, "h": h}])
}

/// A box with its corner at the origin (f1), and a sketch (f2) on `plane`.
fn doc_with(size: [f64; 3], plane: Value, entities: Value) -> Document {
    let mut doc = Document::new(Registry::builtin());
    doc.add_feature(
        "primitive",
        obj(json!({"shape": "box", "length": size[0], "width": size[1], "height": size[2], "anchor": "corner", "plane": "XY", "operation": "new"})),
        None,
    )
    .unwrap();
    doc.add_feature("sketch", obj(json!({"plane": plane, "entities": entities})), None)
        .unwrap();
    doc
}

fn move_params(translate: [f64; 3], rotate: [f64; 3], bed: bool, copy: bool) -> Map<String, Value> {
    obj(
        json!({"bodies": ["f1"], "translate": translate, "rotate": rotate, "pivot": "center", "bed": bed, "copy": copy}),
    )
}

fn join(sketch: &str, distance: f64) -> Map<String, Value> {
    obj(json!({"sketch": sketch, "operation": "join", "distance": distance, "direction": "one", "targets": ["f1"]}))
}

fn no_errors(doc: &Document) {
    for r in doc.results() {
        assert_eq!(r.error, None, "step {} failed", r.id);
    }
}

fn frame(doc: &Document, sid: &str) -> Frame {
    doc.state.sketches[sid].frame
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
}

fn assert_frame(f: Frame, o: [f64; 3], x: [f64; 3], y: [f64; 3], n: [f64; 3]) {
    assert!(
        near(f.o, o) && near(f.x, x) && near(f.y, y) && near(f.n, n),
        "frame {f:?}, expected o {o:?} x {x:?} y {y:?} n {n:?}"
    );
}

fn bbox(doc: &Document, bid: &str) -> Aabb {
    doc.state.bodies[bid].solid.as_ref().expect("a solid").bounding_box()
}

fn volume(doc: &Document, bid: &str) -> f64 {
    doc.state.bodies[bid].solid.as_ref().expect("a solid").volume()
}

fn box_near(b: &Aabb, min: [f64; 3], max: [f64; 3]) {
    let ok = (0..3).all(|i| (b.min[i] - min[i]).abs() < 1e-4 && (b.max[i] - max[i]).abs() < 1e-4);
    assert!(ok, "box {:?}..{:?}, expected {min:?}..{max:?}", b.min, b.max);
}

/// The box around the body's vertices that pass `keep` (to find where a boss landed).
fn part_box(doc: &Document, bid: &str, keep: impl Fn([f64; 3]) -> bool) -> ([f64; 3], [f64; 3]) {
    let m = doc.state.bodies[bid].solid.as_ref().unwrap().to_mesh();
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in m.positions.chunks_exact(3) {
        let v = [p[0] as f64, p[1] as f64, p[2] as f64];
        if keep(v) {
            for i in 0..3 {
                lo[i] = lo[i].min(v[i]);
                hi[i] = hi[i].max(v[i]);
            }
        }
    }
    (lo, hi)
}

fn assert_part(got: ([f64; 3], [f64; 3]), min: [f64; 3], max: [f64; 3]) {
    let ok = (0..3).all(|i| (got.0[i] - min[i]).abs() < 1e-4 && (got.1[i] - max[i]).abs() < 1e-4);
    assert!(ok, "part {:?}..{:?}, expected {min:?}..{max:?}", got.0, got.1);
}

const X: [f64; 3] = [1.0, 0.0, 0.0];
const Y: [f64; 3] = [0.0, 1.0, 0.0];
const Z: [f64; 3] = [0.0, 0.0, 1.0];

#[test]
fn sketch_follows_move() {
    let mut doc = doc_with(
        [20.0, 20.0, 20.0],
        face([0.0, 0.0, 20.0], Z, X, "f1"),
        rect(5.0, 5.0, 10.0, 10.0),
    );
    doc.add_feature("move", move_params([30.0, 0.0, 0.0], [0.0; 3], false, false), None)
        .unwrap();
    assert_frame(frame(&doc, "f2"), [30.0, 0.0, 20.0], X, Y, Z);
    // the stored plane is never rewritten
    assert_eq!(doc.features[1].params["plane"]["origin"], json!([0.0, 0.0, 20.0]));
    // an extrude after the move lands on the moved face
    doc.add_feature("extrude", join("f2", 5.0), None).unwrap();
    no_errors(&doc);
    box_near(&bbox(&doc, "f1"), [30.0, 0.0, 0.0], [50.0, 20.0, 25.0]);
    assert!((volume(&doc, "f1") - 8500.0).abs() < 1e-3, "{}", volume(&doc, "f1"));
    assert_part(
        part_box(&doc, "f1", |p| p[2] > 20.5),
        [35.0, 5.0, 25.0],
        [45.0, 15.0, 25.0],
    );

    // the same again on a second run (cached steps) and after editing the move
    doc.evaluate();
    assert_frame(frame(&doc, "f2"), [30.0, 0.0, 20.0], X, Y, Z);
    doc.edit_feature(
        "f3",
        Some(move_params([0.0, -40.0, 3.0], [0.0; 3], false, false)),
        None,
        None,
        RollbackAfter::Keep,
    )
    .unwrap();
    no_errors(&doc);
    assert_frame(frame(&doc, "f2"), [0.0, -40.0, 23.0], X, Y, Z);
    assert_part(
        part_box(&doc, "f1", |p| p[2] > 23.5),
        [5.0, -35.0, 28.0],
        [15.0, -25.0, 28.0],
    );

    // Make a copy leaves the original, and its sketch, where it was
    doc.edit_feature(
        "f3",
        Some(move_params([30.0, 0.0, 0.0], [0.0; 3], false, true)),
        None,
        None,
        RollbackAfter::Keep,
    )
    .unwrap();
    no_errors(&doc);
    assert_frame(frame(&doc, "f2"), [0.0, 0.0, 20.0], X, Y, Z);
}

#[test]
fn sketch_follows_rotate() {
    // a 20 x 20 x 10 slab turned 90° about X stands on its side; Land on bed lifts it 5 mm
    let mut doc = doc_with(
        [20.0, 20.0, 10.0],
        face([0.0, 0.0, 10.0], Z, X, "f1"),
        rect(5.0, 5.0, 10.0, 10.0),
    );
    doc.add_feature("move", move_params([0.0; 3], [90.0, 0.0, 0.0], true, false), None)
        .unwrap();
    no_errors(&doc);
    box_near(&bbox(&doc, "f1"), [0.0, 5.0, 0.0], [20.0, 15.0, 20.0]);
    // the top face now faces -Y at y = 5; the sketch went with it, lift included
    assert_frame(frame(&doc, "f2"), [0.0, 5.0, 0.0], X, Z, [0.0, -1.0, 0.0]);
    doc.add_feature("extrude", join("f2", 5.0), None).unwrap();
    no_errors(&doc);
    box_near(&bbox(&doc, "f1"), [0.0, 0.0, 0.0], [20.0, 15.0, 20.0]);
    assert!((volume(&doc, "f1") - 4500.0).abs() < 1e-3, "{}", volume(&doc, "f1"));
    assert_part(part_box(&doc, "f1", |p| p[1] < 4.5), [5.0, 0.0, 5.0], [15.0, 0.0, 15.0]);
}

#[test]
fn mirror_keep_original_sketch_stays() {
    for mode in ["join", "new"] {
        let mut doc = doc_with(
            [20.0, 20.0, 20.0],
            face([0.0, 0.0, 20.0], Z, X, "f1"),
            rect(5.0, 5.0, 10.0, 10.0),
        );
        doc.add_feature(
            "mirror",
            obj(json!({"bodies": ["f1"], "plane": "YZ", "mode": mode})),
            None,
        )
        .unwrap();
        no_errors(&doc);
        assert_frame(frame(&doc, "f2"), [0.0, 0.0, 20.0], X, Y, Z);
        doc.add_feature("extrude", join("f2", 5.0), None).unwrap();
        no_errors(&doc);
        assert_part(
            part_box(&doc, "f1", |p| p[2] > 20.5),
            [5.0, 5.0, 25.0],
            [15.0, 15.0, 25.0],
        );
    }
}

#[test]
fn mirror_replace_sketch_follows() {
    let mut doc = doc_with(
        [20.0, 20.0, 20.0],
        face([0.0, 0.0, 20.0], Z, X, "f1"),
        rect(5.0, 5.0, 10.0, 10.0),
    );
    doc.add_feature(
        "mirror",
        obj(json!({"bodies": ["f1"], "plane": "YZ", "mode": "replace"})),
        None,
    )
    .unwrap();
    no_errors(&doc);
    // mirrored across YZ: x flips, the frame is left-handed so its points sit on the mirrored face
    assert_frame(frame(&doc, "f2"), [0.0, 0.0, 20.0], [-1.0, 0.0, 0.0], Y, Z);
    doc.add_feature("extrude", join("f2", 5.0), None).unwrap();
    no_errors(&doc);
    box_near(&bbox(&doc, "f1"), [-20.0, 0.0, 0.0], [0.0, 20.0, 25.0]);
    // not inside out: the boss adds material
    assert!((volume(&doc, "f1") - 8500.0).abs() < 1e-3, "{}", volume(&doc, "f1"));
    assert_part(
        part_box(&doc, "f1", |p| p[2] > 20.5),
        [-15.0, 5.0, 25.0],
        [-5.0, 15.0, 25.0],
    );
    // a cut on the mirrored sketch goes into the material
    doc.add_feature(
        "extrude",
        obj(json!({"sketch": "f2", "operation": "cut", "distance": 3, "direction": "one", "targets": ["f1"]})),
        None,
    )
    .unwrap();
    no_errors(&doc);
    assert!(
        (volume(&doc, "f1") - 8500.0).abs() > 1.0,
        "the cut removed material: {}",
        volume(&doc, "f1")
    );
}

/// Review Focus 4: the sketch is reopened (the timeline rolled back to just after it, where the
/// body has not moved yet), edited, and the timeline runs on: the edit lands on the moved face.
#[test]
fn edit_followed_sketch_after_move() {
    let mut doc = doc_with(
        [20.0, 20.0, 20.0],
        face([0.0, 0.0, 20.0], Z, X, "f1"),
        rect(5.0, 5.0, 10.0, 10.0),
    );
    doc.add_feature("move", move_params([30.0, 0.0, 0.0], [0.0; 3], false, false), None)
        .unwrap();
    doc.add_feature("extrude", join("f2", 5.0), None).unwrap();
    no_errors(&doc);

    // opened for editing: rolled back to the sketch, the body and the sketch are where they were
    doc.set_rollback(Some(2));
    doc.evaluate();
    assert_frame(frame(&doc, "f2"), [0.0, 0.0, 20.0], X, Y, Z);
    box_near(&bbox(&doc, "f1"), [0.0, 0.0, 0.0], [20.0, 20.0, 20.0]);

    // the edit (plane coordinates) is saved and the timeline runs to the end again
    let mut p = doc.features[1].params.clone();
    p.insert("entities".into(), rect(2.0, 2.0, 4.0, 4.0));
    doc.edit_feature("f2", Some(p), None, None, RollbackAfter::Set(None))
        .unwrap();
    no_errors(&doc);
    assert_eq!(
        doc.features[1].params["plane"],
        face([0.0, 0.0, 20.0], Z, X, "f1"),
        "the stored plane is unchanged"
    );
    assert_frame(frame(&doc, "f2"), [30.0, 0.0, 20.0], X, Y, Z);
    assert!((volume(&doc, "f1") - 8080.0).abs() < 1e-3, "{}", volume(&doc, "f1"));
    assert_part(
        part_box(&doc, "f1", |p| p[2] > 20.5),
        [32.0, 2.0, 25.0],
        [36.0, 6.0, 25.0],
    );
}

#[test]
fn origin_plane_sketch_does_not_move() {
    for plane in [json!("XY"), json!({"origin": [0, 0, 20], "normal": [0, 0, 1]})] {
        let mut doc = doc_with([20.0, 20.0, 20.0], plane.clone(), rect(5.0, 5.0, 10.0, 10.0));
        let before = frame(&doc, "f2");
        doc.add_feature(
            "move",
            move_params([30.0, 0.0, 0.0], [0.0, 0.0, 45.0], false, false),
            None,
        )
        .unwrap();
        doc.add_feature(
            "mirror",
            obj(json!({"bodies": ["f1"], "plane": "XZ", "mode": "replace"})),
            None,
        )
        .unwrap();
        no_errors(&doc);
        assert_eq!(frame(&doc, "f2"), before, "a sketch on {plane} stays put");
    }
    // a user plane made on the body does not move either (only sketches belong to bodies)
    let mut doc = doc_with([20.0, 20.0, 20.0], json!("XY"), rect(5.0, 5.0, 10.0, 10.0));
    doc.add_feature(
        "plane",
        obj(json!({"mode": "offset", "base": "XY", "distance": 20})),
        None,
    )
    .unwrap();
    doc.add_feature(
        "sketch",
        obj(json!({"plane": "f3", "entities": rect(5.0, 5.0, 10.0, 10.0)})),
        None,
    )
    .unwrap();
    no_errors(&doc);
    let before = frame(&doc, "f4");
    assert!(near(before.o, [0.0, 0.0, 20.0]));
    doc.add_feature("move", move_params([30.0, 0.0, 0.0], [0.0; 3], false, false), None)
        .unwrap();
    no_errors(&doc);
    assert_eq!(frame(&doc, "f4"), before);
}

/// A project saved before sketches followed their body (1.x or 2.0 before milestone 7): its
/// sketch plane was picked on the body, so after this change the sketch follows the later move
/// (listed in rust/tests/EXCEPTIONS.md). The project file itself is read unchanged.
#[test]
fn old_project_sketch_on_body_follows() {
    let project = json!({
        "features": [
            {"id": "f1", "type": "primitive", "params": {"shape": "box", "length": 20, "width": 20, "height": 20, "anchor": "corner", "plane": "XY", "operation": "new"}},
            {"id": "f2", "type": "sketch", "params": {"plane": {"origin": [10, 10, 20], "normal": [0, 0, 1], "xdir": [1, 0, 0], "label": "Face of Box", "body": "f1"},
                "entities": [{"type": "circle", "id": "c1", "cx": 0, "cy": 0, "r": 4}]}},
            {"id": "f3", "type": "move", "params": {"bodies": ["f1"], "translate": [0, 25, 0], "rotate": [0, 0, 0], "pivot": "center", "copy": false}},
            {"id": "f4", "type": "extrude", "params": {"sketch": "f2", "operation": "cut", "distance": 5, "direction": "one", "targets": ["f1"]}}
        ],
        "ui": {}
    });
    let doc = stlx::from_parts(&project, Vec::new(), Registry::builtin()).unwrap();
    no_errors(&doc);
    assert_frame(frame(&doc, "f2"), [10.0, 35.0, 20.0], X, Y, Z);
    // the hole is cut in the moved part's top face, round its middle
    let b = bbox(&doc, "f1");
    box_near(&b, [0.0, 25.0, 0.0], [20.0, 45.0, 20.0]);
    assert!(
        volume(&doc, "f1") < 8000.0 - 200.0,
        "the cut went into the part: {}",
        volume(&doc, "f1")
    );
    let (lo, hi) = part_box(&doc, "f1", |p| p[2] < 19.5 && p[2] > 14.5);
    assert!(
        (lo[0] - 6.0).abs() < 0.1 && (hi[0] - 14.0).abs() < 0.1,
        "hole x {lo:?}..{hi:?}"
    );
    assert!(
        (lo[1] - 31.0).abs() < 0.1 && (hi[1] - 39.0).abs() < 0.1,
        "hole y {lo:?}..{hi:?}"
    );
}
