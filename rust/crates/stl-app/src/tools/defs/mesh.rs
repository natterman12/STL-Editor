//! MESH tab tools.

use serde_json::{json, Map, Value};

use super::util::*;
use super::{Ctx, Field, FieldKind, ToolDef, ToolFlags, Values};

fn first_body_slice(ctx: &Ctx) -> Value {
    json!(sel_body(ctx).into_iter().take(1).collect::<Vec<_>>())
}

/// JS `{ ...v, body: v.body[0] }`
fn to_params_body0(v: &Values, _: &Ctx) -> Value {
    let mut m = v.clone();
    m.remove("body");
    put_opt(&mut m, "body", first_of(v, "body").as_ref());
    params_cleanup(Value::Object(m))
}

/// JS `{ body: v.body[0], <keys> }`
fn to_params_body0_only(v: &Values, keys: &[&str]) -> Value {
    let mut m = Map::new();
    put_opt(&mut m, "body", first_of(v, "body").as_ref());
    for k in keys {
        put_opt(&mut m, k, v.get(*k));
    }
    Value::Object(m)
}

fn to_params_remesh(v: &Values, _: &Ctx) -> Value {
    to_params_body0_only(v, &["edge"])
}

fn to_params_separate(v: &Values, _: &Ctx) -> Value {
    to_params_body0_only(v, &[])
}

/// JS `{ ...p, body: p.body ? [p.body] : [] }`
fn from_params_body_array(p: Value) -> Values {
    let mut o = p.as_object().cloned().unwrap_or_default();
    let body = wrap_truthy(o.get("body"));
    o.insert("body".into(), body);
    o
}

fn defaults_sculpt(ctx: &Ctx) -> Values {
    let body = sel_body(ctx).into_iter().next();
    values(&[
        ("body", json!(body)),
        ("mode", json!("smooth")),
        ("radius", json!(5)),
        ("strength", json!(0.5)),
        ("strokes", json!([])),
    ])
}

fn to_params_sculpt(v: &Values, _: &Ctx) -> Value {
    let mut m = Map::new();
    put_opt(&mut m, "body", v.get("body"));
    put_opt(&mut m, "strokes", v.get("strokes"));
    Value::Object(m)
}

/// JS `{ mode: 'smooth', radius: 5, strength: 0.5, ...p }`: the params win.
fn from_params_sculpt(p: Value) -> Values {
    let mut o = values(&[("mode", json!("smooth")), ("radius", json!(5)), ("strength", json!(0.5))]);
    if let Some(src) = p.as_object() {
        for (k, v) in src {
            o.insert(k.clone(), v.clone());
        }
    }
    o
}

pub fn sculpt() -> ToolDef {
    ToolDef {
        key: "sculpt",
        label: "Sculpt",
        icon: "sculpt",
        kind: "sculpt",
        fields: vec![
            Field {
                key: "mode",
                label: "Brush",
                kind: super::fields::opts(&[("smooth", "Smooth"), ("flatten", "Flatten"), ("inflate", "Inflate"), ("deflate", "Deflate")]),
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "radius",
                label: "Size",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "strength",
                label: "Strength",
                kind: FieldKind::Number { unit: Some("0–1") },
                help: None,
                placeholder: None,
                show: None,
            },
        ],
        hint: Some("Drag across the model to brush it · right-drag to orbit · works best on finely meshed bodies (use Remesh first on blocky parts)"),
        extra: None,
        flags: ToolFlags { sculpt: true, ..ToolFlags::default() },
        defaults: defaults_sculpt,
        to_params: to_params_sculpt,
        from_params: from_params_sculpt,
        on_change: None,
    }
}

pub fn remesh() -> ToolDef {
    ToolDef {
        key: "remesh",
        label: "Remesh",
        icon: "remesh",
        kind: "remesh",
        fields: vec![
            Field {
                key: "body",
                label: "Body",
                kind: FieldKind::Bodies { multi: false },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "edge",
                label: "Triangle size",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
        ],
        hint: Some("Rebuilds the surface with even triangles of this size, so brushes and smoothing work everywhere"),
        extra: None,
        flags: ToolFlags { no_live_preview: true, ..ToolFlags::default() },
        defaults: |ctx| values(&[("body", first_body_slice(ctx)), ("edge", json!(1))]),
        to_params: to_params_remesh,
        from_params: from_params_body_array,
        on_change: None,
    }
}

fn defaults_repair(ctx: &Ctx) -> Values {
    let sel = sel_body(ctx);
    let body = if !sel.is_empty() {
        json!(vec![sel[0].clone()])
    } else {
        let nonsolid: Vec<_> = ctx
            .bodies
            .iter()
            .filter(|b| !b.solid)
            .map(|b| b.id.clone())
            .take(1)
            .collect();
        json!(nonsolid)
    };
    values(&[("body", body), ("mode", json!("auto")), ("resolution", json!("normal"))])
}

fn show_repair_resolution(v: &Values) -> bool {
    get_str(v, "mode") != Some("quick")
}

pub fn repair() -> ToolDef {
    ToolDef {
        key: "repair",
        label: "Repair",
        icon: "repair",
        kind: "repair",
        fields: vec![
            Field {
                key: "body",
                label: "Body",
                kind: FieldKind::Bodies { multi: false },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "mode",
                label: "Method",
                kind: super::fields::opts(&[("auto", "Auto"), ("quick", "Quick fix"), ("rebuild", "Rebuild")]),
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "resolution",
                label: "Rebuild detail",
                kind: super::fields::opts(&[("draft", "Low"), ("normal", "Medium"), ("fine", "High")]),
                help: None,
                placeholder: None,
                show: Some(show_repair_resolution),
            },
        ],
        hint: Some("Quick fix closes small holes and flips bad triangles. Rebuild remakes the mesh as one watertight solid"),
        extra: None,
        flags: ToolFlags { no_live_preview: true, ..ToolFlags::default() },
        defaults: defaults_repair,
        to_params: to_params_body0,
        from_params: from_params_body_array,
        on_change: None,
    }
}

/// JS `Math.max(0.1, Math.min(100, v.percent)) / 100`; an `=expr` string is NaN, which JSON writes as null.
fn to_params_simplify(v: &Values, _: &Ctx) -> Value {
    let pct = get_f64(v, "percent");
    let ratio = if pct.is_nan() { Value::Null } else { js_num(pct.min(100.0).max(0.1) / 100.0) };
    let mut m = Map::new();
    put_opt(&mut m, "body", first_of(v, "body").as_ref());
    m.insert("ratio".into(), ratio);
    Value::Object(m)
}

/// JS `{ body: p.body ? [p.body] : [], percent: Math.round((p.ratio || 0.5) * 100) }`
fn from_params_simplify(p: Value) -> Values {
    let o = p.as_object();
    let ratio = o.and_then(|o| o.get("ratio")).filter(|r| js_truthy(r)).map_or(0.5, js_to_number);
    let pct = js_round(ratio * 100.0);
    values(&[
        ("body", wrap_truthy(o.and_then(|o| o.get("body")))),
        ("percent", if pct.is_nan() { Value::Null } else { js_num(pct) }),
    ])
}

pub fn simplify() -> ToolDef {
    ToolDef {
        key: "simplify",
        label: "Simplify",
        icon: "simplify",
        kind: "simplify",
        fields: vec![
            Field {
                key: "body",
                label: "Body",
                kind: FieldKind::Bodies { multi: false },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "percent",
                label: "Keep",
                kind: FieldKind::Number { unit: Some("%") },
                help: None,
                placeholder: None,
                show: None,
            },
        ],
        hint: Some("Reduces the triangle count while keeping the shape"),
        extra: None,
        flags: ToolFlags { no_live_preview: true, ..ToolFlags::default() },
        defaults: |ctx| values(&[("body", first_body_slice(ctx)), ("percent", json!(50))]),
        to_params: to_params_simplify,
        from_params: from_params_simplify,
        on_change: None,
    }
}

pub fn separate() -> ToolDef {
    ToolDef {
        key: "separate",
        label: "Separate",
        icon: "separate",
        kind: "separate",
        fields: vec![Field {
            key: "body",
            label: "Body",
            kind: FieldKind::Bodies { multi: false },
            help: None,
            placeholder: None,
            show: None,
        }],
        hint: Some("Splits a body made of disconnected pieces into one body per piece"),
        extra: None,
        flags: ToolFlags::default(),
        defaults: |ctx| values(&[("body", first_body_slice(ctx))]),
        to_params: to_params_separate,
        from_params: from_params_body_array,
        on_change: None,
    }
}

pub fn delete_tool() -> ToolDef {
    ToolDef {
        key: "delete",
        label: "Remove Bodies",
        icon: "delete",
        kind: "delete",
        fields: vec![Field {
            key: "bodies",
            label: "Bodies",
            kind: FieldKind::Bodies { multi: true },
            help: None,
            placeholder: None,
            show: None,
        }],
        hint: None,
        extra: None,
        flags: ToolFlags::default(),
        defaults: |ctx| values(&[("bodies", json!(sel_body(ctx)))]),
        to_params: |v, _| to_params_default(v, None),
        from_params: |p| p.as_object().cloned().unwrap_or_default(),
        on_change: None,
    }
}
