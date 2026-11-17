//! Split's optional `offset`: the cut plane moved along its normal, worked out by hand.

use serde_json::{Map, Value, json};
use stl_engine::params::Param;
use stl_engine::{Document, Registry};

fn cube_doc() -> Document {
    let mut doc = Document::new(Registry::builtin());
    let p = json!({"shape": "box", "length": 20, "width": 20, "height": 20, "anchor": "corner",
                   "plane": "XY", "operation": "new"});
    doc.add_feature("primitive", p.as_object().unwrap().clone(), None)
        .unwrap();
    doc
}

fn split_params(offset: Option<Value>) -> Map<String, Value> {
    // the cube's top face: z = 20, normal +Z
    let mut p = json!({"body": "f1", "plane": {"origin": [10, 10, 20], "normal": [0, 0, 1]},
                       "keep": "both", "connector": "none"})
    .as_object()
    .unwrap()
    .clone();
    if let Some(o) = offset {
        p.insert("offset".into(), o);
    }
    p
}

fn volumes(doc: &Document) -> Vec<f64> {
    let mut v: Vec<f64> = doc
        .state
        .bodies
        .values()
        .map(|b| b.solid.as_ref().unwrap().volume())
        .collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v
}

#[test]
fn split_offset_by_hand() {
    let mut doc = cube_doc();
    doc.add_feature("split", split_params(Some(json!(-5))), None).unwrap();
    let v = volumes(&doc);
    assert_eq!(v.len(), 2);
    assert!((v[0] - 2000.0).abs() < 1e-6, "{v:?}"); // 20 x 20 x 5
    assert!((v[1] - 6000.0).abs() < 1e-6, "{v:?}"); // 20 x 20 x 15
}

#[test]
fn split_without_offset_unchanged() {
    // A plane through the middle cuts the same with no offset, offset 0, and offset null.
    let mid = |off: Option<Value>| {
        let mut doc = cube_doc();
        let mut p = split_params(off);
        p.insert("plane".into(), json!({"origin": [10, 10, 10], "normal": [0, 0, 1]}));
        doc.add_feature("split", p, None).unwrap();
        let bits: Vec<Vec<u32>> = doc
            .state
            .bodies
            .values()
            .map(|b| {
                b.solid
                    .as_ref()
                    .unwrap()
                    .to_mesh()
                    .positions
                    .iter()
                    .map(|x| x.to_bits())
                    .collect()
            })
            .collect();
        (volumes(&doc), bits)
    };
    let none = mid(None);
    assert!(none.0.iter().all(|v| (v - 4000.0).abs() < 1e-6), "{:?}", none.0);
    assert_eq!(none, mid(Some(json!(0))));
    assert_eq!(none, mid(Some(Value::Null)));
}

#[test]
fn split_offset_expression() {
    let mut doc = cube_doc();
    doc.set_params(vec![Param {
        name: "h".into(),
        expr: "-10".into(),
        note: String::new(),
    }])
    .unwrap();
    doc.add_feature("split", split_params(Some(json!("=h/2"))), None)
        .unwrap();
    let v = volumes(&doc);
    assert!((v[0] - 2000.0).abs() < 1e-6 && (v[1] - 6000.0).abs() < 1e-6, "{v:?}");
}

#[test]
fn split_offset_misses_with_new_message() {
    let mut doc = cube_doc();
    let e = doc
        .add_feature("split", split_params(Some(json!(3))), None)
        .unwrap_err();
    assert_eq!(
        e.0,
        "The plane doesn't cut through this body. Drag the arrow or change Distance to move it into the part."
    );
    // the same words with no offset key and at Distance 0
    let msg = "The plane doesn't cut through this body. Drag the arrow or change Distance to move it into the part.";
    let e = doc.add_feature("split", split_params(None), None).unwrap_err();
    assert_eq!(e.0, msg);
    let e = doc
        .add_feature("split", split_params(Some(json!(0))), None)
        .unwrap_err();
    assert_eq!(e.0, msg);
}
