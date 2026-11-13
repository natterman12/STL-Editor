//! Construction planes and import.

use serde_json::{json, Value};

use super::util::*;
use super::{Field, FieldKind, ToolDef, ToolFlags, Values};

fn to_params_plane_mode(v: &Values, mode: &str) -> Value {
    let mut m = v.clone();
    m.insert("mode".into(), json!(mode));
    params_cleanup(Value::Object(m))
}

pub fn plane_offset() -> ToolDef {
    ToolDef {
        key: "plane_offset",
        label: "Offset Plane",
        icon: "plane",
        kind: "plane",
        fields: vec![
            Field {
                key: "base",
                label: "From",
                kind: FieldKind::Plane { any_face: false },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "distance",
                label: "Offset",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "flip",
                label: "Flip normal",
                kind: FieldKind::Bool,
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "name",
                label: "Name",
                kind: FieldKind::Text { placeholder: None },
                help: None,
                placeholder: None,
                show: None,
            },
        ],
        hint: Some("Pick a base plane or a flat face, then set the offset"),
        extra: None,
        flags: ToolFlags { plane_tool: true, ..ToolFlags::default() },
        defaults: |_| values(&[("mode", json!("offset")), ("base", json!("XY")), ("distance", json!(10)), ("flip", json!(false)), ("name", json!(""))]),
        to_params: |v, _| to_params_plane_mode(v, "offset"),
        from_params: |p| p.as_object().cloned().unwrap_or_default(),
        on_change: None,
    }
}

pub fn plane_angle() -> ToolDef {
    ToolDef {
        key: "plane_angle",
        label: "Plane at Angle",
        icon: "plane_angle",
        kind: "plane",
        fields: vec![
            Field {
                key: "base",
                label: "From",
                kind: FieldKind::Plane { any_face: false },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "axis",
                label: "Hinge axis",
                kind: super::fields::opts(&[("x", "Plane X"), ("y", "Plane Y")]),
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "angle",
                label: "Angle",
                kind: FieldKind::Number { unit: Some("°") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "distance",
                label: "Offset",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "name",
                label: "Name",
                kind: FieldKind::Text { placeholder: None },
                help: None,
                placeholder: None,
                show: None,
            },
        ],
        hint: None,
        extra: None,
        flags: ToolFlags { plane_tool: true, ..ToolFlags::default() },
        defaults: |_| {
            values(&[
                ("mode", json!("angle")),
                ("base", json!("XY")),
                ("axis", json!("x")),
                ("angle", json!(45)),
                ("distance", json!(0)),
                ("name", json!("")),
            ])
        },
        to_params: |v, _| to_params_plane_mode(v, "angle"),
        from_params: |p| p.as_object().cloned().unwrap_or_default(),
        on_change: None,
    }
}

pub fn plane_3pt() -> ToolDef {
    ToolDef {
        key: "plane_3pt",
        label: "3-Point Plane",
        icon: "plane_3pt",
        kind: "plane",
        fields: vec![
            Field {
                key: "points",
                label: "Points",
                kind: FieldKind::Points3,
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "flip",
                label: "Flip normal",
                kind: FieldKind::Bool,
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "name",
                label: "Name",
                kind: FieldKind::Text { placeholder: None },
                help: None,
                placeholder: None,
                show: None,
            },
        ],
        hint: Some("Click three points on the model"),
        extra: None,
        flags: ToolFlags { plane_tool: true, ..ToolFlags::default() },
        defaults: |_| values(&[("mode", json!("three_points")), ("points", json!([])), ("flip", json!(false)), ("name", json!(""))]),
        to_params: |v, _| to_params_plane_mode(v, "three_points"),
        from_params: |p| p.as_object().cloned().unwrap_or_default(),
        on_change: None,
    }
}

pub fn plane_mid() -> ToolDef {
    ToolDef {
        key: "plane_mid",
        label: "Midplane",
        icon: "plane_mid",
        kind: "plane",
        fields: vec![
            Field {
                key: "base",
                label: "First",
                kind: FieldKind::Plane { any_face: false },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "base2",
                label: "Second",
                kind: FieldKind::Plane { any_face: false },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "name",
                label: "Name",
                kind: FieldKind::Text { placeholder: None },
                help: None,
                placeholder: None,
                show: None,
            },
        ],
        hint: Some("Pick two planes or flat faces"),
        extra: None,
        flags: ToolFlags { plane_tool: true, ..ToolFlags::default() },
        defaults: |_| values(&[("mode", json!("midplane")), ("base", Value::Null), ("base2", Value::Null), ("name", json!(""))]),
        to_params: |v, _| to_params_plane_mode(v, "midplane"),
        from_params: |p| p.as_object().cloned().unwrap_or_default(),
        on_change: None,
    }
}

fn from_params_import(p: Value) -> Values {
    let mut o = p.as_object().cloned().unwrap_or_default();
    // JS `p.place || 'keep'`
    if !truthy(&o, "place") {
        o.insert("place".into(), json!("keep"));
    }
    let repair = o.get("repair").and_then(Value::as_bool).unwrap_or(true);
    o.insert("repair".into(), json!(repair));
    o
}

pub fn import_tool() -> ToolDef {
    ToolDef {
        key: "import",
        label: "Imported Mesh",
        icon: "import",
        kind: "import",
        fields: vec![
            Field {
                key: "place",
                label: "Position",
                kind: super::fields::opts(&[("center", "On bed at origin"), ("keep", "As in file")]),
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "repair",
                label: "Auto-repair",
                kind: FieldKind::Bool,
                help: None,
                placeholder: None,
                show: None,
            },
        ],
        hint: Some("\"On bed at origin\" centres the part and sets it on Z = 0. \"As in file\" keeps its original coordinates (use this to line up parts of an assembly)"),
        extra: None,
        flags: ToolFlags::default(),
        defaults: |_| values(&[("place", json!("center")), ("repair", json!(true))]),
        to_params: |v, _| to_params_default(v, None),
        from_params: from_params_import,
        on_change: None,
    }
}
