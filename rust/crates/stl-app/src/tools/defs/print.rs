//! PRINT tab tools (fit to bed, plate layout, lay flat).

use serde_json::{json, Map, Value};

use super::fields::connector_fields_bedsplit;
use super::tables::{self, PRINTER_OPTS};
use super::util::*;
use super::{Ctx, Field, FieldKind, ToolDef, ToolFlags, Values};

fn printer_opts() -> Vec<(String, String)> {
    PRINTER_OPTS.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

fn defaults_bedsplit(ctx: &Ctx) -> Values {
    values(&[
        ("body", json!(sel_body(ctx).into_iter().take(1).collect::<Vec<_>>())),
        ("printer", json!("bambu_x1")),
        ("bed", json!([256, 256, 256])),
        ("margin", json!(5)),
        ("connector", json!("pins")),
        ("pin_d", json!(5)),
        ("pin_len", json!(10)),
        ("clearance", json!(0.15)),
        ("optimize", json!(true)),
    ])
}

fn show_bed(v: &Values) -> bool {
    get_str(v, "printer") == Some("custom")
}

fn to_params_bedsplit(v: &Values, _: &Ctx) -> Value {
    let mut m = v.clone();
    m.remove("body");
    put_opt(&mut m, "body", first_of(v, "body").as_ref());
    if get_str(v, "printer") != Some("custom") {
        m.remove("bed");
    }
    params_cleanup(Value::Object(m))
}

fn from_params_bedsplit(p: Value) -> Values {
    let mut o = Map::new();
    o.insert("bed".into(), json!([256, 256, 256]));
    if let Some(src) = p.as_object() {
        for (k, v) in src {
            o.insert(k.clone(), v.clone());
        }
    }
    let body = wrap_truthy(o.get("body"));
    o.insert("body".into(), body);
    o
}

fn on_change_connector(key: &str, v: &Values, _: &Ctx) -> Option<Values> {
    if key != "connector" {
        return None;
    }
    tables::connector_patch(get_str(v, "connector")?).map(|m| m)
}

fn connector_on_change(key: &str, v: &Values, ctx: &Ctx) -> Option<Values> {
    on_change_connector(key, v, ctx)
}

pub fn bedsplit() -> ToolDef {
    let mut fields = vec![
        Field {
            key: "body",
            label: "Body",
            kind: FieldKind::Bodies { multi: false },
            help: None,
            placeholder: None,
            show: None,
        },
        Field {
            key: "printer",
            label: "Printer",
            kind: super::select_flat(printer_opts()),
            help: None,
            placeholder: None,
            show: None,
        },
        Field {
            key: "bed",
            label: "Bed X Y Z",
            kind: FieldKind::Vec3 { unit: Some("mm") },
            help: None,
            placeholder: None,
            show: Some(show_bed),
        },
        Field {
            key: "margin",
            label: "Margin",
            kind: FieldKind::Number { unit: Some("mm") },
            help: None,
            placeholder: None,
            show: None,
        },
    ];
    fields.extend(connector_fields_bedsplit());
    fields.push(Field {
        key: "optimize",
        label: "Smart cut positions",
        kind: FieldKind::Bool,
        help: None,
        placeholder: None,
        show: None,
    });
    ToolDef {
        key: "bedsplit",
        label: "Fit to Bed",
        icon: "bedsplit",
        kind: "bedsplit",
        fields,
        hint: Some("Cuts the part into pieces that fit your printer, with pins so they line up. Smart cut positions puts seams where the part is thinnest"),
        extra: None,
        flags: ToolFlags { no_live_preview: false, ..ToolFlags::default() },
        defaults: defaults_bedsplit,
        to_params: to_params_bedsplit,
        from_params: from_params_bedsplit,
        on_change: Some(connector_on_change),
    }
}

fn defaults_plate(ctx: &Ctx) -> Values {
    values(&[
        ("bodies", json!(sel_body(ctx))),
        ("printer", json!("bambu_x1")),
        ("bed", json!([256, 256, 256])),
        ("gap", json!(5)),
        ("rotate", json!(false)),
    ])
}

fn to_params_plate(v: &Values, _: &Ctx) -> Value {
    let mut m = v.clone();
    if get_str(v, "printer") != Some("custom") {
        m.remove("bed");
    }
    params_cleanup(Value::Object(m))
}

fn from_params_plate(p: Value) -> Values {
    let mut o = Map::new();
    o.insert("bed".into(), json!([256, 256, 256]));
    if let Some(src) = p.as_object() {
        for (k, v) in src {
            o.insert(k.clone(), v.clone());
        }
    }
    // JS `p.bodies || []`
    if !truthy(&o, "bodies") {
        o.insert("bodies".into(), json!([]));
    }
    o
}

pub fn plate() -> ToolDef {
    ToolDef {
        key: "plate",
        label: "Plate Layout",
        icon: "bedsplit",
        kind: "plate",
        fields: vec![
            Field {
                key: "bodies",
                label: "Bodies",
                kind: FieldKind::Bodies { multi: true },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "printer",
                label: "Printer",
                kind: super::select_flat(printer_opts()),
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "bed",
                label: "Bed X Y Z",
                kind: FieldKind::Vec3 { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: Some(show_bed),
            },
            Field {
                key: "gap",
                label: "Gap",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "rotate",
                label: "Allow turning 90°",
                kind: FieldKind::Bool,
                help: None,
                placeholder: None,
                show: None,
            },
        ],
        hint: Some("Sets the chosen bodies side by side on the print bed, a gap apart, each one sitting on the bed. Turn them with Move afterwards if you want to nudge them"),
        extra: None,
        flags: ToolFlags::default(),
        defaults: defaults_plate,
        to_params: to_params_plate,
        from_params: from_params_plate,
        on_change: None,
    }
}

fn show_layflat_face(v: &Values) -> bool {
    get_str(v, "how") == Some("face")
}

fn show_layflat_body(v: &Values) -> bool {
    get_str(v, "how") == Some("auto")
}

fn defaults_layflat(ctx: &Ctx) -> Values {
    values(&[
        ("how", json!("face")),
        ("face", Value::Null),
        ("body", Value::Null),
        ("bodies", json!(sel_body(ctx).into_iter().take(1).collect::<Vec<_>>())),
    ])
}

fn to_params_layflat(v: &Values, _: &Ctx) -> Value {
    let mut m = Map::new();
    if get_str(v, "how") == Some("auto") {
        m.insert("auto".into(), json!(true));
        put_opt(&mut m, "body", first_of(v, "bodies").as_ref());
        return Value::Object(m);
    }
    put_opt(&mut m, "body", v.get("body"));
    // JS `v.face?.normal`
    if let Some(n) = v.get("face").and_then(|f| f.get("normal")) {
        m.insert("normal".into(), n.clone());
    }
    Value::Object(m)
}

fn from_params_layflat(p: Value) -> Values {
    let o = p.as_object().cloned().unwrap_or_default();
    if truthy(&o, "auto") {
        // JS `bodies: [p.body]`: a missing body becomes [null]
        return values(&[
            ("how", json!("auto")),
            ("bodies", json!([o.get("body").cloned().unwrap_or(Value::Null)])),
            ("face", Value::Null),
        ]);
    }
    let face = o
        .get("normal")
        .filter(|n| js_truthy(n))
        .map(|n| json!({ "normal": n, "label": "Picked face" }));
    let mut out = values(&[("how", json!("face"))]);
    put_opt(&mut out, "body", o.get("body"));
    out.insert("face".into(), face.unwrap_or(Value::Null));
    out.insert("bodies".into(), json!([]));
    out
}

pub fn layflat() -> ToolDef {
    ToolDef {
        key: "layflat",
        label: "Lay Flat",
        icon: "layflat",
        kind: "layflat",
        fields: vec![
            Field {
                key: "how",
                label: "Method",
                kind: super::fields::opts(&[("face", "Pick a face"), ("auto", "Most stable side")]),
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "face",
                label: "Face to put down",
                kind: FieldKind::PFace { any_face: true },
                help: None,
                placeholder: None,
                show: Some(show_layflat_face),
            },
            Field {
                key: "bodies",
                label: "Body",
                kind: FieldKind::Bodies { multi: false },
                help: None,
                placeholder: None,
                show: Some(show_layflat_body),
            },
        ],
        hint: Some("Click the face that should sit on the print bed, or let it pick the most stable side"),
        extra: None,
        flags: ToolFlags::default(),
        defaults: defaults_layflat,
        to_params: to_params_layflat,
        from_params: from_params_layflat,
        on_change: None,
    }
}
