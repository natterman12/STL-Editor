//! 2.0 split connectors: sliding dovetails, where connectors go (a count, the centre, spots
//! clicked by hand) and a cut giving each separate piece its own body. Volumes worked out by
//! hand from the shapes.

use serde_json::{Map, Value, json};
use stl_engine::{Document, Registry};
use stl_kernel::Solid;

const TAN15: f64 = 0.267_949_192_431_122_7;

fn obj(v: Value) -> Map<String, Value> {
    v.as_object().unwrap().clone()
}

/// A 20 mm cube from the origin (body `f1`).
fn cube_doc() -> Document {
    let mut doc = Document::new(Registry::builtin());
    let p = json!({"shape": "box", "length": 20, "width": 20, "height": 20, "anchor": "corner",
                   "plane": "XY", "operation": "new"});
    doc.add_feature("primitive", obj(p), None).unwrap();
    doc
}

/// A U: a 30 × 10 × 5 base with two 10 × 10 × 20 legs at x 0..10 and 20..30 (4500 mm³).
fn u_doc() -> Document {
    let mut doc = Document::new(Registry::builtin());
    let boxp = |l: f64, h: f64, x: f64, op: &str| {
        obj(
            json!({"shape": "box", "length": l, "width": 10, "height": h, "anchor": "corner",
                   "plane": "XY", "offset": [x, 0, 0], "operation": op, "targets": ["f1"]}),
        )
    };
    doc.add_feature("primitive", boxp(30.0, 5.0, 0.0, "new"), None).unwrap();
    doc.add_feature("primitive", boxp(10.0, 20.0, 0.0, "join"), None)
        .unwrap();
    doc.add_feature("primitive", boxp(10.0, 20.0, 20.0, "join"), None)
        .unwrap();
    assert!((volumes(&doc).iter().sum::<f64>() - 4500.0).abs() < 1e-6);
    doc
}

/// A split of `f1` across z = 10, with `extra` params on top.
fn split_at_10(extra: Value) -> Map<String, Value> {
    let mut p = obj(
        json!({"body": "f1", "plane": {"origin": [10, 10, 10], "normal": [0, 0, 1]},
                           "keep": "both"}),
    );
    p.extend(obj(extra));
    p
}

fn volumes(doc: &Document) -> Vec<f64> {
    let mut v: Vec<f64> = doc
        .state
        .bodies
        .values()
        .map(|b| b.solid.as_ref().unwrap().volume())
        .collect();
    v.sort_by(f64::total_cmp);
    v
}

fn solid(doc: &Document, id: &str) -> Solid {
    doc.state.bodies[id].solid.clone().unwrap()
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() < tol
}

/// A pin hole as Split cuts it for 5 mm pins with 0.15 clearance: a 48-sided prism of radius
/// 2.65, 5.4 deep from 0.01 above the cut, so 5.39 of it in the half below.
fn pin_hole() -> f64 {
    24.0 * 2.65_f64.powi(2) * (std::f64::consts::PI / 24.0).sin() * 5.39
}

#[test]
fn dovetail_tongue_and_groove_volumes() {
    let mut doc = cube_doc();
    doc.add_feature(
        "split",
        split_at_10(json!({"connector": "dovetail", "dovetail_width": 8, "dovetail_depth": 4, "clearance": 0.2})),
        None,
    )
    .unwrap();
    assert_eq!(doc.state.bodies.len(), 2);
    // tongue: 8 wide at the cut face, 4 deep, each side leaning out 15°, the whole 20 across
    let tongue = (8.0 * 4.0 + TAN15 * 16.0) * 20.0;
    // groove: the tongue grown by 0.2 on its sides and tip
    let groove = (8.4 * 4.2 + TAN15 * 4.2 * 4.2) * 20.0;
    let above = solid(&doc, "f1").volume();
    let below = solid(&doc, "f2.2").volume();
    assert!(
        close(above, 4000.0 + tongue, 1e-6),
        "above {above} vs {}",
        4000.0 + tongue
    );
    assert!(
        close(below, 4000.0 - groove, 1e-6),
        "below {below} vs {}",
        4000.0 - groove
    );
    assert_eq!(doc.results()[1].warning, None);
}

#[test]
fn dovetail_slides_together_but_does_not_pull_apart() {
    let mut doc = cube_doc();
    doc.add_feature(
        "split",
        split_at_10(json!({"connector": "dovetail", "dovetail_width": 8, "dovetail_depth": 4, "clearance": 0.2})),
        None,
    )
    .unwrap();
    let (above, below) = (solid(&doc, "f1"), solid(&doc, "f2.2"));
    assert!(above.intersection(&below).volume() < 1e-6, "they fit together");
    // a square cut face: the dovetail runs along the plane's x; slide the top half out along it
    for k in [2.0, 7.5, 13.0, 21.0] {
        let moved = above.translate([k, 0.0, 0.0]);
        assert!(
            moved.intersection(&below).volume() < 1e-6,
            "slides {k} mm without hitting"
        );
    }
    // lifting it straight up, or sliding it across the dovetail, runs into the groove's sides.
    // The 0.2 gap on the 15° sides lets it lift 0.2 / tan 15° = 0.75 mm before they touch.
    let up = above.translate([0.0, 0.0, 0.7]);
    assert!(up.intersection(&below).volume() < 1e-6, "the gap's play");
    let up = above.translate([0.0, 0.0, 1.5]);
    assert!(up.intersection(&below).volume() > 1.0, "lifting is blocked");
    let across = above.translate([0.0, 1.0, 0.0]);
    assert!(across.intersection(&below).volume() > 1.0, "sliding across is blocked");
}

#[test]
fn dovetail_slide_short_side() {
    // a 40 × 20 cut face: on its long side the dovetail runs along x, on the short side along y
    let bar = || {
        let mut doc = Document::new(Registry::builtin());
        let p = json!({"shape": "box", "length": 40, "width": 20, "height": 20, "anchor": "corner",
                       "plane": "XY", "operation": "new"});
        doc.add_feature("primitive", obj(p), None).unwrap();
        doc
    };
    let mut long = bar();
    let dt = json!({"connector": "dovetail", "dovetail_width": 6, "dovetail_depth": 3, "clearance": 0.2});
    long.add_feature("split", split_at_10(dt.clone()), None).unwrap();
    let mut short = dt;
    short["dovetail_slide"] = json!("short");
    let mut doc = bar();
    doc.add_feature("split", split_at_10(short), None).unwrap();
    let tongue = |len: f64| (6.0 * 3.0 + TAN15 * 9.0) * len;
    let a_long = solid(&long, "f1").volume();
    let a_short = solid(&doc, "f1").volume();
    assert!(close(a_long, 8000.0 + tongue(40.0), 1e-6), "{a_long}");
    assert!(close(a_short, 8000.0 + tongue(20.0), 1e-6), "{a_short}");
}

#[test]
fn dovetail_on_each_leg() {
    // a cut through both legs of the U: each leg's 10 × 10 face gets its own dovetail, 10 long
    let mut doc = u_doc();
    doc.add_feature(
        "split",
        split_at_10(json!({"connector": "dovetail", "dovetail_width": 4, "dovetail_depth": 2, "clearance": 0.2})),
        None,
    )
    .unwrap();
    let tongue = (4.0 * 2.0 + TAN15 * 4.0) * 10.0;
    let groove = (4.4 * 2.2 + TAN15 * 2.2 * 2.2) * 10.0;
    let above = solid(&doc, "f1").volume();
    let below = solid(&doc, "f4.2").volume();
    assert!(close(above, 2000.0 + 2.0 * tongue, 1e-6), "{above}");
    assert!(close(below, 2500.0 - 2.0 * groove, 1e-6), "{below}");
}

#[test]
fn pins_how_many() {
    // Auto puts one pin on a 20 mm face; asking for 3 gives 3
    let mut one = cube_doc();
    one.add_feature("split", split_at_10(json!({"connector": "pins"})), None)
        .unwrap();
    let mut three = cube_doc();
    three
        .add_feature(
            "split",
            split_at_10(json!({"connector": "pins", "connector_count": 3})),
            None,
        )
        .unwrap();
    let b1 = solid(&one, "f2.2").volume();
    let b3 = solid(&three, "f2.2").volume();
    assert!(close(b1, 4000.0 - pin_hole(), 1e-6), "{b1}");
    assert!(close(b3, 4000.0 - 3.0 * pin_hole(), 1e-6), "{b3}");
    assert_eq!(three.results()[1].warning, None);
    // more than fit: as many as there's room for, and a warning that says so
    let mut lots = cube_doc();
    lots.add_feature(
        "split",
        split_at_10(json!({"connector": "pins", "connector_count": 20})),
        None,
    )
    .unwrap();
    let w = lots.results()[1].warning.clone().unwrap_or_default();
    assert!(w.starts_with("The cut face only has room for "), "{w}");
    let holes = ((4000.0 - solid(&lots, "f2.2").volume()) / pin_hole()).round();
    assert!(holes > 3.0 && holes < 20.0, "{holes}");
    assert!(w.contains(&format!("{holes} connectors")), "{w}");
}

#[test]
fn pins_centre_and_by_hand() {
    // Centre: one pin at the middle of the face
    let mut doc = cube_doc();
    doc.add_feature(
        "split",
        split_at_10(json!({"connector": "magnets", "connector_place": "centre"})),
        None,
    )
    .unwrap();
    let above = solid(&doc, "f1");
    // a magnet pocket in each half; the top half's sits right in the middle
    let probe = Solid::cube([0.5, 0.5, 0.5], true).translate([10.0, 10.0, 10.5]);
    assert!(
        probe.intersection(&above).volume() < 1e-9,
        "the pocket is at the centre"
    );

    // by hand: two spots, the second given above the cut (it is moved onto it)
    let mut doc = cube_doc();
    doc.add_feature(
        "split",
        split_at_10(json!({"connector": "pins", "connector_place": "manual",
                            "connector_points": [[6, 6, 10], [14, 13, 17]]})),
        None,
    )
    .unwrap();
    let below = solid(&doc, "f2.2");
    assert!(close(below.volume(), 4000.0 - 2.0 * pin_hole(), 1e-6));
    for (x, y) in [(6.0, 6.0), (14.0, 13.0)] {
        let probe = Solid::cube([0.5, 0.5, 0.5], true).translate([x, y, 8.0]);
        assert!(probe.intersection(&below).volume() < 1e-9, "a hole at ({x}, {y})");
    }

    // a spot too near the edge is an error the user can fix
    let mut doc = cube_doc();
    let e = doc
        .add_feature(
            "split",
            split_at_10(json!({"connector": "pins", "connector_place": "manual",
                                "connector_points": [[10, 10, 10], [1, 10, 10]]})),
            None,
        )
        .unwrap_err();
    assert_eq!(
        e.0,
        "Spot 2 is off the cut face or too close to its edge. Move it further in, or make the connectors smaller."
    );
}

#[test]
fn separate_pieces() {
    // the U cut across its legs: two legs above, the base below
    let mut doc = u_doc();
    doc.add_feature("split", split_at_10(json!({"connector": "none"})), None)
        .unwrap();
    assert_eq!(volumes(&doc).len(), 2, "without it: two bodies");
    let mut doc = u_doc();
    doc.add_feature(
        "split",
        split_at_10(json!({"connector": "none", "separate": true})),
        None,
    )
    .unwrap();
    let v = volumes(&doc);
    assert_eq!(v.len(), 3, "{v:?}");
    assert!(
        close(v[0], 1000.0, 1e-6) && close(v[1], 1000.0, 1e-6) && close(v[2], 2500.0, 1e-6),
        "{v:?}"
    );
    let mut names: Vec<_> = doc.state.bodies.values().map(|b| b.name.clone()).collect();
    names.sort();
    assert_eq!(names, ["Box (1)", "Box (2)", "Box (3)"]);
}

#[test]
fn bed_split_with_dovetails() {
    // a 300 mm bar on a 220 mm bed: one cut, and a dovetail along the cut face's 30 mm side
    let mut doc = Document::new(Registry::builtin());
    let p = json!({"shape": "box", "length": 300, "width": 30, "height": 20, "anchor": "corner",
                   "plane": "XY", "operation": "new"});
    doc.add_feature("primitive", obj(p), None).unwrap();
    doc.add_feature(
        "bedsplit",
        obj(
            json!({"body": "f1", "bed": [220, 220, 250], "margin": 5, "connector": "dovetail",
                   "dovetail_width": 8, "dovetail_depth": 4, "clearance": 0.2}),
        ),
        None,
    )
    .unwrap();
    let info = &doc.results()[1].extra["info"];
    assert_eq!(info["connectors"], json!(1), "{info}");
    let v = volumes(&doc);
    assert_eq!(v.len(), 2);
    assert!(
        close(
            v.iter().sum::<f64>(),
            180_000.0 + (8.0 * 4.0 + TAN15 * 16.0) * 30.0 - (8.4 * 4.2 + TAN15 * 4.2 * 4.2) * 30.0,
            1e-5
        ),
        "{v:?}"
    );
}
