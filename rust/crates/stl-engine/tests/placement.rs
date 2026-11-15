//! Exact Hole and Thread placement (M7 spec 2.4): points measured from part edges and sketch
//! lines are re-placed from those references when the project runs; projects without references
//! rebuild byte for byte as before.

use std::path::PathBuf;

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use stl_engine::params::Param;
use stl_engine::{Document, Feature, Registry};
use stl_kernel::Solid;

// ------------------------------------------------------------------ helpers

fn parity_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity")
}

fn read_json(path: &PathBuf) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).unwrap()
}

fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
}

fn feed_mesh(h: &mut Sha256, solid: &Solid) {
    let m = solid.to_mesh();
    h.update((m.positions.len() as u64).to_le_bytes());
    for p in &m.positions {
        h.update(p.to_le_bytes());
    }
    h.update((m.triangles.len() as u64).to_le_bytes());
    for t in &m.triangles {
        h.update(t.to_le_bytes());
    }
}

fn doc_of(features: &[Value], params: &[(&str, &str)]) -> Document {
    let mut doc = Document::new(Registry::builtin());
    doc.params = params.iter().map(|(n, e)| Param { name: n.to_string(), expr: e.to_string(), note: String::new() }).collect();
    doc.features = features
        .iter()
        .enumerate()
        .map(|(i, raw)| Feature {
            id: format!("f{}", i + 1),
            kind: raw["type"].as_str().unwrap_or_default().to_string(),
            name: raw["name"].as_str().filter(|n| !n.is_empty()).map(str::to_string),
            params: raw["params"].as_object().cloned().unwrap_or_else(Map::new),
            suppressed: false,
            extra: Map::new(),
        })
        .collect();
    doc
}

/// Result errors and warnings, then every body's mesh, hashed.
fn hash_doc(doc: &mut Document) -> String {
    let results = doc.evaluate();
    let mut h = Sha256::new();
    for r in &results {
        h.update(r.id.as_bytes());
        h.update([0]);
        h.update(r.error.as_deref().unwrap_or("").as_bytes());
        h.update([0]);
        h.update(r.warning.as_deref().unwrap_or("").as_bytes());
        h.update([0]);
    }
    h.update([1]);
    for (id, body) in &doc.state.bodies {
        h.update(id.as_bytes());
        h.update([0]);
        h.update(body.name.as_bytes());
        h.update([0]);
        match &body.solid {
            Some(s) => feed_mesh(&mut h, s),
            None => h.update(b"none"),
        }
    }
    hex(&h.finalize())
}

/// A 40 × 30 × 5 plate with its corner at the origin (its top face at z = 5). `length` may be an
/// expression.
fn plate(length: Value) -> Value {
    json!({"type": "primitive", "params": {"shape": "box", "length": length, "width": 30, "height": 5, "anchor": "corner", "plane": "XY", "operation": "new"}})
}

fn hole_with(points: Value, refs: Option<Value>) -> Value {
    let mut p = json!({"points": points, "normal": [0, 0, 1], "kind": "simple", "diameter": 4, "through": true, "targets": ["f1"]});
    if let Some(r) = refs {
        p["point_refs"] = r;
    }
    json!({"type": "hole", "params": p})
}

fn edge(a: [f64; 3], b: [f64; 3], dist: f64) -> Value {
    json!({"kind": "edge", "a": a, "b": b, "normal": [0, 0, 1], "dist": dist})
}

/// The centre of the (single) round hole through the plate: the mean of the top-face vertices
/// that are not on the plate's outline.
fn hole_centre(doc: &Document, outline: [f64; 2]) -> [f64; 2] {
    let m = doc.state.bodies["f1"].solid.as_ref().expect("the plate is a solid").to_mesh();
    let (mut sx, mut sy, mut n) = (0.0, 0.0, 0.0);
    for p in m.positions.chunks_exact(3) {
        let (x, y, z) = (p[0] as f64, p[1] as f64, p[2] as f64);
        let on_outline = x.abs() < 1e-4 || y.abs() < 1e-4 || (x - outline[0]).abs() < 1e-4 || (y - outline[1]).abs() < 1e-4;
        if (z - 5.0).abs() < 1e-4 && !on_outline {
            sx += x;
            sy += y;
            n += 1.0;
        }
    }
    assert!(n > 0.0, "the plate has a hole");
    [sx / n, sy / n]
}

fn result_of<'a>(doc: &'a Document, id: &str) -> &'a stl_engine::FeatureResult {
    doc.results().iter().find(|r| r.id == id).expect("the step ran")
}

// ------------------------------------------------------------------ tests

/// Hashes of every Hole, Thread and Pattern parity case, recorded on Windows on the code before
/// M7 Task 5 (2026-11-15; debug and release give the same). The cases have no `point_refs` / `position_refs`.
const EXPECTED: &[(&str, &str)] = &[
    ("hole/hole_simple_through", "34b9f5d0c05976b84f5e042fefc4a4f4ac67820d9d90cbf3e944f229318cd949"),
    ("hole/hole_blind_tip", "5e43bcb23982de69fd9281148b35489b4ea4950dd03de53a8e50d5d1054843bb"),
    ("hole/hole_counterbore", "695ba940042f5a13131c6712124461544548f07d91e096c883fe656b574a0fbf"),
    ("hole/hole_countersink", "d0815db2f74a27c8e5c033e10c8777383de3869d4b0af1becaa39095b45a4376"),
    ("hole/hole_hex", "3eedf5f02f4d7e89fcdc56736fb2564ff81318ea7c63f3941d781f1cfe8193e0"),
    ("hole/hole_no_points", "594f829c906b96dbc14b17b55b198b23e646ecdc620aaedfd46ffc263ea360e2"),
    ("hole/hole_thread_flag_ignored", "d35e826955ae2267ceb78397200689697fd1f2ad1c4c09b644995be5a1af05c3"),
    ("hole/hole_several_points", "6908efcf3022e4884e6bf4e558ff8d81156d0f5e87af85ad6d234390c9e7e1a4"),
    ("thread/thread_external", "351f48ac763c7b0ee811a2bd0caf4a02f219e6b2b24ec3f4f33a8e45b5d13fe5"),
    ("thread/thread_internal", "e0fb7f59a34654819d27bfb188722937856b72723aa20289c66d7b4a39cc467c"),
    ("thread/thread_left_hand", "634c45813049765269809c9b8c17533ae2b28c079c436919b3f1fb6caa0a71ed"),
    ("thread/thread_chamfer_off", "ff64bc4d306f8d7694577ca3ec99bebfcfb77e10b32117fb6be805138598ce45"),
    ("thread/thread_tolerance", "dae8cedefd625b94711e7c5440e9416ad070dbe1ac9e9b5ef6169ecf22c6035f"),
    ("thread/thread_placed_plane", "1e65427eddb3c6b85ba014faf82fc56ebbecf02a756792c43f0accad50751343"),
    ("thread/thread_bad_size", "c1a6b00bb2ff66cf6b37de394048c6f224ba731f45bfdcafc8d90089b9c20287"),
    ("pattern/pattern_linear", "522d3b36d8987f34422c29f19ba472ca5384327fbd5a61fed18a85d8d4bd1d91"),
    ("pattern/pattern_circular_360", "43f2445bcfad8d62b13b6f40119ccb822f0f2fdd79d16629d6068a1de02a49c5"),
    ("pattern/pattern_circular_partial", "56938362cf4ea7489ec67f570a80510352480c4bc78da0334e3897a8d1a8c785"),
    ("pattern/pattern_join_false", "49d7f6767ebf4bbd4b6cdbc01cedb291f97fcc4d57bcee4d9c2db6e1a4868d45"),
    ("pattern/pattern_features_source", "abd8eb36f618028aa473f3c1421740886a10f83efa9eb04d8734b7d0f26134e6"),
    ("pattern/pattern_no_body", "81c004655d3314c0815bea98f6f6c5f80fb5db6d0ce5598ae98c3217fee9714c"),
    ("pattern/pattern_count_too_low", "43cefe8175e2b833a0031ac20575f4b8c16787f8dda6589a21cbd8d6b5858536"),
];

#[test]
fn no_refs_unchanged() {
    let mut got: Vec<(String, String)> = Vec::new();
    for file in ["hole", "thread", "pattern"] {
        let cases = read_json(&parity_dir().join("features").join(format!("{file}.json")));
        for case in cases.as_array().expect("case file is a list") {
            let name = format!("{file}/{}", case["name"].as_str().unwrap());
            let features = case["features"].as_array().cloned().unwrap_or_default();
            let mut doc = doc_of(&features, &[]);
            let hash = hash_doc(&mut doc);
            // empty reference lists are the same as none (on every OS)
            let mut empty = features.clone();
            for f in &mut empty {
                match f["type"].as_str() {
                    Some("hole") => f["params"]["point_refs"] = json!([[], []]),
                    Some("thread") => f["params"]["position_refs"] = json!([]),
                    _ => {}
                }
            }
            assert_eq!(hash_doc(&mut doc_of(&empty, &[])), hash, "{name}: empty references changed the result");
            got.push((name, hash));
        }
    }
    // the exact bits are pinned on Windows only: Linux's sin/cos round a few last bits
    // differently (as in stl-kernel's union_groups)
    if !cfg!(windows) {
        return;
    }
    if std::env::var("PLACEMENT_PRINT_HASHES").is_ok() {
        for (k, v) in &got {
            println!("    (\"{k}\", \"{v}\"),");
        }
    }
    assert_eq!(got.len(), EXPECTED.len(), "one hash per case");
    for ((k, v), (ek, ev)) in got.iter().zip(EXPECTED) {
        assert_eq!((k.as_str(), v.as_str()), (*ek, *ev), "{k} changed");
    }
}

fn hole_point(doc: &Document, step: usize) -> Value {
    doc.features[step].params["points"][0].clone()
}

fn assert_near(got: [f64; 2], want: [f64; 2], what: &str) {
    assert!((got[0] - want[0]).abs() < 1e-3 && (got[1] - want[1]).abs() < 1e-3, "{what}: got {got:?}, want {want:?}");
}

#[test]
fn edge_distances_exact() {
    // 10 mm from the left edge and 8 mm from the front edge; the click was a little off
    let refs = json!([[edge([0.0, 0.0, 5.0], [0.0, 30.0, 5.0], 10.0), edge([0.0, 0.0, 5.0], [40.0, 0.0, 5.0], 8.0)]]);
    let mut doc = doc_of(&[plate(json!(40)), hole_with(json!([[11.3, 9.1, 5]]), Some(refs.clone()))], &[]);
    doc.evaluate();
    assert_eq!(result_of(&doc, "f2").error, None);
    assert_eq!(result_of(&doc, "f2").warning, None);
    assert_near(hole_centre(&doc, [40.0, 30.0]), [10.0, 8.0], "hole centre");
    // the same through the engine call the UI uses
    let list = stl_engine::features::common::placement::parse_refs(Some(&refs));
    let before = doc_of(&[plate(json!(40))], &[]);
    let mut before = before;
    before.evaluate();
    let (p, w) = stl_engine::features::common::placement::resolve_point(&before.state, "f1", [11.3, 9.1, 5.0], [0.0, 0.0, 1.0], &list[0]).unwrap();
    assert_eq!(w, None);
    assert!((p[0] - 10.0).abs() < 1e-3 && (p[1] - 8.0).abs() < 1e-3 && (p[2] - 5.0).abs() < 1e-3, "{p:?}");
}

#[test]
fn plate_longer_moves_hole() {
    // measured from the right edge and the front edge; the plate's length is a parameter
    let refs = json!([[edge([40.0, 0.0, 5.0], [40.0, 30.0, 5.0], 10.0), edge([0.0, 0.0, 5.0], [40.0, 0.0, 5.0], 8.0)]]);
    let mut doc = doc_of(&[plate(json!("=w")), hole_with(json!([[30, 8, 5]]), Some(refs))], &[("w", "40")]);
    doc.evaluate();
    assert_eq!(result_of(&doc, "f2").error, None);
    assert_near(hole_centre(&doc, [40.0, 30.0]), [30.0, 8.0], "before");
    doc.set_params(vec![Param { name: "w".into(), expr: "60".into(), note: String::new() }]).unwrap();
    doc.evaluate();
    assert_eq!(result_of(&doc, "f2").warning, None);
    assert_near(hole_centre(&doc, [60.0, 30.0]), [50.0, 8.0], "after the plate grew 20 mm");
    // the stored point is what was clicked; the project re-places it each time it runs
    assert_eq!(hole_point(&doc, 1), json!([30, 8, 5]));
}

#[test]
fn sketch_line_along() {
    let sketch = json!({"type": "sketch", "params": {"plane": {"origin": [0, 0, 5], "normal": [0, 0, 1]}, "entities": [
        {"type": "line", "id": "l1", "p1": [5, 5], "p2": [35, 5]},
        {"type": "rect", "id": "r1", "x": 2, "y": 20, "w": 10, "h": 6}
    ]}});
    let refs = json!([[{"kind": "sketch_line", "sketch": "f2", "entity": "l1", "along": 12}]]);
    let mut doc = doc_of(&[plate(json!(40)), sketch.clone(), hole_with(json!([[17.4, 5.3, 5]]), Some(refs))], &[]);
    doc.evaluate();
    assert_eq!(result_of(&doc, "f3").error, None);
    assert_eq!(result_of(&doc, "f3").warning, None);
    assert_near(hole_centre(&doc, [40.0, 30.0]), [17.0, 5.0], "12 mm along the line from its start");

    // a distance from a sketch line, plus one from a part edge
    let refs = json!([[{"kind": "sketch_line", "sketch": "f2", "entity": "l1", "dist": 6}, edge([0.0, 0.0, 5.0], [0.0, 30.0, 5.0], 25.0)]]);
    let mut doc = doc_of(&[plate(json!(40)), sketch, hole_with(json!([[24, 12, 5]]), Some(refs))], &[]);
    doc.evaluate();
    assert_eq!(result_of(&doc, "f3").warning, None);
    assert_near(hole_centre(&doc, [40.0, 30.0]), [25.0, 11.0], "6 mm above the line, 25 mm from the left edge");

    // a side of a rectangle, by `<id>#<k>`
    let st = &doc.state;
    let (a, b) = stl_engine::features::common::placement::sketch_line(st, "f2", "r1#0").expect("the rectangle's first side");
    assert!((a[2] - 5.0).abs() < 1e-9 && (b[2] - 5.0).abs() < 1e-9);
    assert!(stl_engine::features::common::placement::sketch_line(st, "f2", "gone").is_none());
}

#[test]
fn ambiguous_edge_is_missing() {
    use stl_engine::features::common::placement::find_edge;
    // a line down the middle: the left and right edges are both 20 mm away, so neither is chosen
    let refs = json!([[edge([20.0, 0.0, 5.0], [20.0, 30.0, 5.0], 5.0)]]);
    let mut doc = doc_of(&[plate(json!(40)), hole_with(json!([[15, 8, 5]]), Some(refs))], &[]);
    doc.evaluate();
    let r = result_of(&doc, "f2");
    assert_eq!(r.error, None);
    let w = r.warning.clone().expect("a missing reference is reported");
    assert!(w.contains("Hole point 1") && w.contains("can no longer be found"), "{w}");
    assert_near(hole_centre(&doc, [40.0, 30.0]), [15.0, 8.0], "the point stays where it was");

    let mut plain = doc_of(&[plate(json!(40))], &[]);
    plain.evaluate();
    let m = plain.state.bodies["f1"].solid.as_ref().unwrap().to_mesh();
    assert_eq!(find_edge(&m, [20.0, 0.0, 5.0], [20.0, 30.0, 5.0], [0.0, 0.0, 1.0]), None);
    // nearer one side, it is found, running the stored way
    let e = find_edge(&m, [33.0, 30.0, 5.0], [33.0, 0.0, 5.0], [0.0, 0.0, 1.0]).expect("the right edge");
    assert!((e.0[0] - 40.0).abs() < 1e-6 && (e.1[0] - 40.0).abs() < 1e-6 && e.0[1] > e.1[1], "{e:?}");
    // the bottom face's edges are not on a face that looks the same way
    assert_eq!(find_edge(&m, [0.0, 0.0, 0.0], [40.0, 0.0, 0.0], [0.0, 0.0, 1.0]).map(|e| e.0[2]), Some(5.0));
}

#[test]
fn removed_edge_is_missing() {
    // a slanted edge that the plate never had
    let refs = json!([[edge([0.0, 0.0, 5.0], [40.0, 30.0, 5.0], 3.0)]]);
    let mut doc = doc_of(&[plate(json!(40)), hole_with(json!([[20, 10, 5]]), Some(refs))], &[]);
    doc.evaluate();
    let r = result_of(&doc, "f2");
    assert_eq!(r.error, None);
    assert!(r.warning.as_deref().is_some_and(|w| w.contains("can no longer be found")), "{:?}", r.warning);
    assert_near(hole_centre(&doc, [40.0, 30.0]), [20.0, 10.0], "kept");

    // a sketch line that was deleted from its sketch
    let sketch = json!({"type": "sketch", "params": {"plane": {"origin": [0, 0, 5], "normal": [0, 0, 1]}, "entities": []}});
    let refs = json!([[{"kind": "sketch_line", "sketch": "f2", "entity": "l1", "along": 12}]]);
    let mut doc = doc_of(&[plate(json!(40)), sketch, hole_with(json!([[20, 10, 5]]), Some(refs))], &[]);
    doc.evaluate();
    assert!(result_of(&doc, "f3").warning.as_deref().is_some_and(|w| w.contains("sketch line")));
    assert_near(hole_centre(&doc, [40.0, 30.0]), [20.0, 10.0], "kept");

    // Thread: the same rules for its one position
    let face = json!({"origin": [20, 15, 5], "normal": [0, 0, 1], "xdir": [1, 0, 0], "body": "f1"});
    let thread = |refs: Value| json!({"type": "thread", "params": {"kind": "internal", "diameter": 6, "pitch": 1, "length": 4, "plane": face, "offset": [1, 1, 0], "operation": "cut", "targets": ["f1"], "position_refs": refs}});
    let mut doc = doc_of(&[plate(json!(40)), thread(json!([edge([0.0, 0.0, 5.0], [0.0, 30.0, 5.0], 10.0), edge([0.0, 0.0, 5.0], [40.0, 0.0, 5.0], 8.0)]))], &[]);
    doc.evaluate();
    assert_eq!(result_of(&doc, "f2").error, None);
    assert_eq!(result_of(&doc, "f2").warning, None);
    // the same cut as a thread typed in at (10, 8): offset (-10, -7) from the face's origin
    let mut typed = doc_of(&[plate(json!(40)), thread(Value::Null)], &[]);
    typed.features[1].params["offset"] = json!([-10, -7, 0]);
    typed.features[1].params.remove("position_refs");
    typed.evaluate();
    let (va, vb) = (doc.state.bodies["f1"].solid.as_ref().unwrap().volume(), typed.state.bodies["f1"].solid.as_ref().unwrap().volume());
    let (ba, bb) = (doc.state.bodies["f1"].solid.as_ref().unwrap().bounding_box(), typed.state.bodies["f1"].solid.as_ref().unwrap().bounding_box());
    assert!(va < 40.0 * 30.0 * 5.0 - 50.0 && (va - vb).abs() < 1e-3, "{va} vs {vb}");
    assert_eq!(ba, bb);
    let mut doc = doc_of(&[plate(json!(40)), thread(json!([edge([0.0, 0.0, 5.0], [40.0, 30.0, 5.0], 3.0)]))], &[]);
    doc.evaluate();
    assert!(result_of(&doc, "f2").warning.as_deref().is_some_and(|w| w.starts_with("Thread position")));
}

#[test]
fn lower_first_is_char_safe() {
    use stl_engine::features::common::placement::lower_first;
    assert_eq!(lower_first("An edge"), "an edge");
    assert_eq!(lower_first("Édge"), "édge");
    assert_eq!(lower_first("→ arrow"), "→ arrow");
    assert_eq!(lower_first(""), "");
}
