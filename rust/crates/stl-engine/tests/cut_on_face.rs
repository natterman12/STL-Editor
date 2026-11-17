//! A cut sketched on a face opens that face: no paper-thin skin is left over the pocket, even
//! when the picked face's plane sits a rounding error inside the part (Nathan 2026-11-16: a
//! pocket drawn on the side of a box "cut into the part but did not remove the surface").

use serde_json::{Map, Value, json};
use stl_engine::{Document, Registry};

fn obj(v: Value) -> Map<String, Value> {
    v.as_object().unwrap().clone()
}

/// Nathan's project: a 20 mm box (base on the bed, centred), a pocket from the top, then a
/// 10 x 15 rectangle on the +X face cut 10 mm in. The face plane was saved a hair inside the
/// box face (x = 9.999999999999998), as picking a face gives.
fn doc() -> Document {
    let mut d = Document::new(Registry::builtin());
    d.add_feature("primitive", obj(json!({"shape":"box","length":20.0,"width":20.0,"height":20.0,"plane":"XY","offset":[0,0,0],"rotate":[0,0,0],"anchor":"base","operation":"new","targets":[],"body_name":"Box"})), None).unwrap();
    d.add_feature("sketch", obj(json!({"plane":{"origin":[0.0,0.0,19.999999999999996],"normal":[0.0,0.0,1.0],"xdir":[1.0,0.0,0.0],"label":"Face of Box","body":"f1"},"entities":[{"type":"rect","id":"a","x":-7.0,"y":7.0,"w":14.0,"h":-13.0}],"refs":[],"constraints":[]})), None).unwrap();
    d.add_feature("extrude", obj(json!({"sketch":"f2","regions":[],"operation":"cut","direction":"symmetric","distance":10.0,"distance2":10.0,"start_offset":0.0,"taper":0.0,"through_all":false,"targets":[]})), None).unwrap();
    d.add_feature("sketch", obj(json!({"plane":{"origin":[9.999999999999998,0.0,0.0],"normal":[1.0,0.0,0.0],"xdir":[-0.0,1.0,0.0],"label":"Face of Box","body":"f1"},"entities":[{"type":"rect","id":"b","x":-5.0,"y":18.0,"w":10.0,"h":-15.0}],"refs":[],"constraints":[]})), None).unwrap();
    d
}

#[test]
fn a_cut_on_a_side_face_opens_the_face() {
    let mut d = doc();
    let before = d
        .preview(
            "extrude",
            obj(json!({"sketch":"f4","operation":"cut","direction":"symmetric","distance":0.001,"targets":[]})),
            None,
        )
        .0;
    let _v0: f64 = before
        .bodies
        .values()
        .filter_map(|b| b.solid.as_ref())
        .map(|s| s.volume())
        .sum();
    let (st, res) = d.preview("extrude", obj(json!({"sketch":"f4","regions":[],"operation":"cut","direction":"one","distance":10.0,"distance2":10.0,"start_offset":0.0,"taper":0.0,"through_all":false,"targets":[]})), None);
    assert_eq!(res.error, None);
    let solids: Vec<_> = st.bodies.values().filter_map(|b| b.solid.as_ref()).collect();
    // one body, no sliver: a skin left over the pocket would be a second, near-zero piece, or
    // keep the part closed (genus 0 with a hidden void instead of an open pocket)
    assert_eq!(solids.len(), 1, "the cut leaves one body");
    // no triangle of the part lies on the face (x = 10) inside the pocket's opening
    let m = solids[0].to_mesh();
    let p = |i: u32| {
        let k = i as usize * 3;
        [
            f64::from(m.positions[k]),
            f64::from(m.positions[k + 1]),
            f64::from(m.positions[k + 2]),
        ]
    };
    let skin = m
        .triangles
        .chunks_exact(3)
        .filter(|t| {
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            let ctr = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            (ctr[0] - 10.0).abs() < 1e-3 && ctr[1].abs() < 4.9 && ctr[2] > 3.1 && ctr[2] < 9.9
        })
        .count();
    assert_eq!(skin, 0, "{skin} triangles still cover the pocket's opening");
}

#[test]
fn a_cut_from_a_plane_inside_the_part_keeps_its_depth() {
    let mut d = Document::new(Registry::builtin());
    d.add_feature(
        "primitive",
        obj(json!({"shape":"box","length":20,"width":20,"height":20,"anchor":"corner","plane":"XY","operation":"new"})),
        None,
    )
    .unwrap();
    d.add_feature("sketch", obj(json!({"plane":{"origin":[0,0,10],"normal":[0,0,1]},"entities":[{"type":"rect","id":"r","x":5,"y":5,"w":10,"h":10}]})), None).unwrap();
    for (dir, want) in [("one", 8000.0 - 500.0), ("reverse", 8000.0 - 500.0)] {
        let (st, res) = d.preview(
            "extrude",
            obj(json!({"sketch":"f2","operation":"cut","direction":dir,"distance":5,"targets":["f1"]})),
            None,
        );
        assert_eq!(res.error, None);
        let v: f64 = st
            .bodies
            .values()
            .filter_map(|b| b.solid.as_ref())
            .map(|s| s.volume())
            .sum();
        assert!((v - want).abs() < 1e-6, "{dir}: {v} vs {want}");
    }
}

#[test]
fn a_cut_on_the_top_face_removes_exactly_the_pocket() {
    // the face plane a rounding error below the top (z = 20): the pocket is open and exact
    let mut d = Document::new(Registry::builtin());
    d.add_feature(
        "primitive",
        obj(json!({"shape":"box","length":20,"width":20,"height":20,"anchor":"corner","plane":"XY","operation":"new"})),
        None,
    )
    .unwrap();
    d.add_feature("sketch", obj(json!({"plane":{"origin":[0,0,19.999999999999996],"normal":[0,0,1]},"entities":[{"type":"rect","id":"r","x":5,"y":5,"w":10,"h":10}]})), None).unwrap();
    let (st, res) = d.preview(
        "extrude",
        obj(json!({"sketch":"f2","operation":"cut","direction":"one","distance":5,"targets":["f1"]})),
        None,
    );
    assert_eq!(res.error, None);
    assert!(res.flipped);
    let s = st.bodies.values().find_map(|b| b.solid.as_ref()).unwrap();
    assert!((s.volume() - 7500.0).abs() < 1e-6, "{}", s.volume());
    let m = s.to_mesh();
    let top_skin = m
        .triangles
        .chunks_exact(3)
        .filter(|t| {
            let z = |i: u32| f64::from(m.positions[i as usize * 3 + 2]);
            let x = |i: u32| f64::from(m.positions[i as usize * 3]);
            let y = |i: u32| f64::from(m.positions[i as usize * 3 + 1]);
            let c = [
                (x(t[0]) + x(t[1]) + x(t[2])) / 3.0,
                (y(t[0]) + y(t[1]) + y(t[2])) / 3.0,
                (z(t[0]) + z(t[1]) + z(t[2])) / 3.0,
            ];
            (c[2] - 20.0).abs() < 1e-3 && c[0] > 5.1 && c[0] < 14.9 && c[1] > 5.1 && c[1] < 14.9
        })
        .count();
    assert_eq!(top_skin, 0);
}
