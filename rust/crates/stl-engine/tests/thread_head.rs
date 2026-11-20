//! Thread, 2.0 only (Nathan 2026-11-19): a tapped hole can have a pocket for the screw head, a
//! counterbore for a socket head cap screw or a countersink for a flat head. The thread starts
//! at the bottom of the pocket. "flush" (or no `head`) is the tapped hole as it always was.

use serde_json::{Map, Value, json};
use stl_engine::{Document, Registry, State};

fn obj(v: Value) -> Map<String, Value> {
    v.as_object().unwrap().clone()
}

/// A 30 x 30 plate `thick` mm high, one corner at the origin, so its top face is z = `thick`.
fn plate(thick: f64) -> Document {
    let mut d = Document::new(Registry::builtin());
    d.add_feature(
        "primitive",
        obj(json!({"shape":"box","length":30,"width":30,"height":thick,"anchor":"corner","plane":"XY","operation":"new"})),
        None,
    )
    .unwrap();
    d
}

/// An M8 tapped hole in the middle of the plate's top face, plus `more`.
fn thread(thick: f64, more: Value) -> Map<String, Value> {
    let mut p = obj(json!({
        "kind": "internal", "diameter": 8, "pitch": 1.25, "length": 6, "tolerance": 0, "hand": "right",
        "chamfer": true, "plane": {"origin": [15, 15, thick], "normal": [0, 0, 1]}, "offset": [0, 0, 0],
        "operation": "cut", "targets": ["f1"],
    }));
    p.extend(obj(more));
    p
}

fn volume(st: &State) -> f64 {
    st.bodies
        .values()
        .filter_map(|b| b.solid.as_ref())
        .map(|s| s.volume())
        .sum()
}

/// Area of the 96-sided polygon the pockets are cut with, for radius 1.
fn unit_area() -> f64 {
    0.5 * 96.0 * (std::f64::consts::TAU / 96.0).sin()
}

#[test]
fn a_counterbore_removes_the_head_pocket() {
    // the plate is as thick as the pocket is deep, so all that is cut from it is the pocket:
    // a 14 mm round, 5 mm deep
    let mut d = plate(5.0);
    let (st, res) = d.preview(
        "thread",
        thread(
            5.0,
            json!({"head": "counterbore", "head_diameter": 14, "head_depth": 5}),
        ),
        None,
    );
    assert_eq!(res.error, None);
    let want = 30.0 * 30.0 * 5.0 - unit_area() * 7.0 * 7.0 * 5.0;
    assert!((volume(&st) - want).abs() < 0.01, "{} vs {want}", volume(&st));
}

#[test]
fn a_countersink_removes_the_cone() {
    // 16.5 mm across at the surface, 90°, down to the 8 mm thread: (16.5 - 8) / 2 = 4.25 deep.
    // The plate is that thick, so all that is cut from it is the cone.
    let mut d = plate(4.25);
    let (st, res) = d.preview(
        "thread",
        thread(
            4.25,
            json!({"head": "countersink", "head_diameter": 16.5, "head_angle": 90}),
        ),
        None,
    );
    assert_eq!(res.error, None);
    let (top, bottom, deep) = (8.25, 4.0, 4.25);
    let cone = unit_area() * deep / 3.0 * (top * top + bottom * bottom + top * bottom);
    let want = 30.0 * 30.0 * 4.25 - cone;
    assert!((volume(&st) - want).abs() < 0.01, "{} vs {want}", volume(&st));
}

#[test]
fn the_thread_starts_at_the_bottom_of_the_pocket() {
    // as a body of its own, the cutter shows where it reaches: 1 mm above the surface (z = 20)
    // down to pocket depth + thread length below it
    let mut d = plate(20.0);
    for (more, deep, wide) in [
        (
            json!({"head": "counterbore", "head_diameter": 14, "head_depth": 5}),
            5.0,
            7.0,
        ),
        // the cone carries on 1 mm above the surface, 1 mm wider each side at 90°
        (
            json!({"head": "countersink", "head_diameter": 16.5, "head_angle": 90}),
            4.25,
            9.25,
        ),
    ] {
        let mut p = thread(20.0, more);
        p.insert("operation".into(), json!("new"));
        let (st, res) = d.preview("thread", p, None);
        assert_eq!(res.error, None);
        let id = res.created.first().expect("the cutter as a new body");
        let bb = st.bodies[id].solid.as_ref().unwrap().bounding_box();
        assert!((bb.max[2] - 21.0).abs() < 1e-4, "{bb:?}");
        assert!((bb.min[2] - (20.0 - deep - 6.0)).abs() < 1e-4, "{bb:?}");
        assert!((bb.max[0] - (15.0 + wide)).abs() < 1e-3, "{bb:?}");
    }
}

#[test]
fn flush_is_the_tapped_hole_as_before() {
    let mut d = plate(20.0);
    let (plain, res) = d.preview("thread", thread(20.0, json!({})), None);
    assert_eq!(res.error, None);
    // the pocket sizes are ignored while the head is flush
    let (flush, res) = d.preview(
        "thread",
        thread(20.0, json!({"head": "flush", "head_diameter": 14, "head_depth": 5})),
        None,
    );
    assert_eq!(res.error, None);
    assert_eq!(volume(&plain), volume(&flush));
    assert!(volume(&plain) < 30.0 * 30.0 * 20.0);
}

#[test]
fn a_screw_has_no_pocket() {
    let mut d = plate(20.0);
    let screw = |more: Value| {
        let mut p = thread(20.0, more);
        p.insert("kind".into(), json!("external"));
        p.insert("operation".into(), json!("new"));
        p
    };
    let (plain, res) = d.preview("thread", screw(json!({})), None);
    assert_eq!(res.error, None);
    let (headed, res) = d.preview(
        "thread",
        screw(json!({"head": "counterbore", "head_diameter": 14, "head_depth": 5})),
        None,
    );
    assert_eq!(res.error, None);
    assert_eq!(volume(&plain), volume(&headed));
}

#[test]
fn a_pocket_no_wider_than_the_thread_is_refused() {
    let mut d = plate(20.0);
    for more in [
        json!({"head": "counterbore", "head_diameter": 8, "head_depth": 5}),
        json!({"head": "countersink", "head_diameter": 6, "head_angle": 90}),
    ] {
        let (_, res) = d.preview("thread", thread(20.0, more), None);
        assert_eq!(
            res.error.as_deref(),
            Some("The head pocket must be wider than the thread.")
        );
    }
    let (_, res) = d.preview(
        "thread",
        thread(
            20.0,
            json!({"head": "counterbore", "head_diameter": 14, "head_depth": 0}),
        ),
        None,
    );
    assert_eq!(res.error.as_deref(), Some("The head pocket depth must be more than 0."));
}
