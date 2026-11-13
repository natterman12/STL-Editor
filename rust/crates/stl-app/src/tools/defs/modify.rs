//! MODEL modify tools (hole through shell).

use serde_json::{json, Map, Value};

use super::fields::{connector_fields_split, ops_options, show_targets};
use super::tables::{self, axv, THREAD_SIZES};
use super::util::*;
use super::{Ctx, Field, FieldKind, ToolDef, ToolFlags, Values};

fn hole_preset_select() -> FieldKind {
    super::select_grouped(tables::hole_preset_options())
}

fn thread_size_select() -> FieldKind {
    let opts: Vec<(String, String, String)> = THREAD_SIZES
        .iter()
        .map(|(id, _, _, label, group)| (id.to_string(), label.to_string(), group.to_string()))
        .collect();
    super::select_grouped(opts)
}

fn hole_show_tol(v: &Values) -> bool {
    get_str(v, "preset") != Some("custom")
}

fn hole_show_depth(v: &Values) -> bool {
    !get_bool(v, "through")
}

fn hole_show_tip(v: &Values) -> bool {
    !get_bool(v, "through")
}

fn hole_show_cb(v: &Values) -> bool {
    get_str(v, "kind") == Some("counterbore")
}

fn hole_show_cs(v: &Values) -> bool {
    get_str(v, "kind") == Some("countersink")
}

fn hole_show_hex(v: &Values) -> bool {
    get_str(v, "kind") == Some("hex")
}

fn defaults_hole(_: &Ctx) -> Values {
    values(&[
        ("points", json!([])),
        ("normal", Value::Null),
        ("preset", json!("custom")),
        ("tol", json!(0.2)),
        ("kind", json!("simple")),
        ("diameter", json!(5)),
        ("through", json!(true)),
        ("depth", json!(10)),
        ("tip", json!(true)),
        ("cb_diameter", json!(9)),
        ("cb_depth", json!(3)),
        ("cs_diameter", json!(10)),
        ("cs_angle", json!(90)),
        ("hex_af", json!(5.7)),
        ("hex_depth", json!(2.6)),
        ("targets", json!([])),
    ])
}

fn to_params_hole(v: &Values, _: &Ctx) -> Value {
    let mut m = v.clone();
    m.remove("point");
    params_cleanup(Value::Object(m))
}

fn from_params_hole(p: Value) -> Values {
    p.as_object().cloned().unwrap_or_default()
}

fn on_change_hole(key: &str, v: &Values, _: &Ctx) -> Option<Values> {
    if key != "preset" && key != "tol" {
        if matches!(
            key,
            "diameter" | "depth" | "kind" | "through" | "cb_diameter" | "cb_depth" | "hex_af" | "hex_depth"
        ) {
            return Some(values(&[("preset", json!("custom"))]));
        }
        return None;
    }
    let preset = get_str(v, "preset")?;
    tables::hole_preset_patch(preset, tol_or_zero(v))
}

pub fn hole() -> ToolDef {
    ToolDef {
        key: "hole",
        label: "Hole",
        icon: "hole",
        kind: "hole",
        fields: vec![
            Field {
                key: "points",
                label: "Positions",
                kind: FieldKind::FacePoints,
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "preset",
                label: "Preset",
                kind: hole_preset_select(),
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "tol",
                label: "Print tolerance",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: Some(hole_show_tol),
            },
            Field {
                key: "kind",
                label: "Type",
                kind: super::fields::opts(&[("simple", "Simple"), ("counterbore", "C-bore"), ("countersink", "C-sink"), ("hex", "Nut trap")]),
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "diameter",
                label: "Diameter",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "through",
                label: "Through all",
                kind: FieldKind::Bool,
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "depth",
                label: "Depth",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: Some(hole_show_depth),
            },
            Field {
                key: "tip",
                label: "Drill-point tip",
                kind: FieldKind::Bool,
                help: None,
                placeholder: None,
                show: Some(hole_show_tip),
            },
            Field {
                key: "cb_diameter",
                label: "Bore Ø",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: Some(hole_show_cb),
            },
            Field {
                key: "cb_depth",
                label: "Bore depth",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: Some(hole_show_cb),
            },
            Field {
                key: "cs_diameter",
                label: "Sink Ø",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: Some(hole_show_cs),
            },
            Field {
                key: "cs_angle",
                label: "Sink angle",
                kind: FieldKind::Number { unit: Some("°") },
                help: None,
                placeholder: None,
                show: Some(hole_show_cs),
            },
            Field {
                key: "hex_af",
                label: "Nut across flats",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: Some(hole_show_hex),
            },
            Field {
                key: "hex_depth",
                label: "Nut depth",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: Some(hole_show_hex),
            },
            Field {
                key: "targets",
                label: "Bodies",
                kind: FieldKind::Bodies { multi: true },
                help: None,
                placeholder: Some("Auto"),
                show: None,
            },
        ],
        hint: Some("Click faces to place holes (click again to add more). Presets size inserts, screws, nut traps and magnets with print tolerance"),
        extra: None,
        flags: ToolFlags::default(),
        defaults: defaults_hole,
        to_params: to_params_hole,
        from_params: from_params_hole,
        on_change: Some(on_change_hole),
    }
}

fn defaults_combine(ctx: &Ctx) -> Values {
    let s = sel_body(ctx);
    values(&[
        ("target", json!(s.first().map(|x| vec![x.clone()]).unwrap_or_default())),
        ("tools", json!(s.iter().skip(1).cloned().collect::<Vec<_>>())),
        ("operation", json!("join")),
        ("clearance", json!(0)),
        ("keep_tools", json!(false)),
    ])
}

fn show_combine_clearance(v: &Values) -> bool {
    get_str(v, "operation") == Some("cut")
}

fn to_params_combine(v: &Values, _: &Ctx) -> Value {
    let mut m = v.clone();
    m.remove("target");
    put_opt(&mut m, "target", first_of(v, "target").as_ref());
    params_cleanup(Value::Object(m))
}

fn from_params_combine(p: Value) -> Values {
    let mut o = p.as_object().cloned().unwrap_or_default();
    let t = wrap_truthy(o.get("target"));
    o.insert("target".into(), t);
    o
}

pub fn combine() -> ToolDef {
    ToolDef {
        key: "combine",
        label: "Combine",
        icon: "combine",
        kind: "combine",
        fields: vec![
            Field {
                key: "target",
                label: "Target body",
                kind: FieldKind::Bodies { multi: false },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "tools",
                label: "Tool bodies",
                kind: FieldKind::Bodies { multi: true },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "operation",
                label: "Operation",
                kind: super::fields::opts(&[("join", "Join"), ("cut", "Cut"), ("intersect", "Intersect")]),
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "clearance",
                label: "Clearance",
                kind: FieldKind::Number { unit: Some("mm") },
                help: Some("Grow the tool bodies first, so a part made to fit this cut-out isn't too tight. 0.2–0.3 mm suits most FDM printers."),
                placeholder: None,
                show: Some(show_combine_clearance),
            },
            Field {
                key: "keep_tools",
                label: "Keep tools",
                kind: FieldKind::Bool,
                help: None,
                placeholder: None,
                show: None,
            },
        ],
        hint: Some("Pick the body to keep, then the bodies to join, cut away or intersect with it"),
        extra: None,
        flags: ToolFlags::default(),
        defaults: defaults_combine,
        to_params: to_params_combine,
        from_params: from_params_combine,
        on_change: None,
    }
}

fn defaults_split(ctx: &Ctx) -> Values {
    values(&[
        ("body", json!(sel_body(ctx).into_iter().take(1).collect::<Vec<_>>())),
        ("plane", Value::Null),
        ("keep", json!("both")),
        ("connector", json!("none")),
        ("pin_d", json!(5)),
        ("pin_len", json!(10)),
        ("clearance", json!(0.15)),
    ])
}

fn to_params_split(v: &Values, _: &Ctx) -> Value {
    let mut m = v.clone();
    m.remove("body");
    put_opt(&mut m, "body", first_of(v, "body").as_ref());
    params_cleanup(Value::Object(m))
}

fn from_params_split(p: Value) -> Values {
    let mut o = Map::new();
    o.insert("connector".into(), json!("none"));
    o.insert("pin_d".into(), json!(5));
    o.insert("pin_len".into(), json!(10));
    o.insert("clearance".into(), json!(0.15));
    if let Some(src) = p.as_object() {
        for (k, v) in src {
            o.insert(k.clone(), v.clone());
        }
    }
    let body = wrap_truthy(o.get("body"));
    o.insert("body".into(), body);
    o
}

fn on_change_split(key: &str, v: &Values, ctx: &Ctx) -> Option<Values> {
    on_change_connector(key, v, ctx)
}

fn on_change_connector(key: &str, v: &Values, _: &Ctx) -> Option<Values> {
    if key != "connector" {
        return None;
    }
    tables::connector_patch(get_str(v, "connector")?)
}

pub fn split() -> ToolDef {
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
            key: "plane",
            label: "Split plane",
            kind: FieldKind::Plane { any_face: false },
            help: None,
            placeholder: None,
            show: None,
        },
        Field {
            key: "keep",
            label: "Keep",
            kind: super::fields::opts(&[("both", "Both"), ("above", "Above"), ("below", "Below")]),
            help: None,
            placeholder: None,
            show: None,
        },
    ];
    fields.extend(connector_fields_split());
    ToolDef {
        key: "split",
        label: "Split",
        icon: "split",
        kind: "split",
        fields,
        hint: Some("Pick a body, then a plane or flat face to cut it with · add connectors so the pieces line up when you glue them"),
        extra: None,
        flags: ToolFlags::default(),
        defaults: defaults_split,
        to_params: to_params_split,
        from_params: from_params_split,
        on_change: Some(on_change_split),
    }
}

fn defaults_thread(_: &Ctx) -> Values {
    values(&[
        ("kind", json!("internal")),
        ("size", json!("M8")),
        ("diameter", json!(8)),
        ("pitch", json!(1.25)),
        ("length", json!(10)),
        ("tolerance", json!(0.3)),
        ("hand", json!("right")),
        ("chamfer", json!(true)),
        ("plane", json!("XY")),
        ("offset", json!([0, 0, 0])),
        ("operation", json!("cut")),
        ("targets", json!([])),
    ])
}

fn to_params_thread(v: &Values, _: &Ctx) -> Value {
    let mut m = v.clone();
    let row = THREAD_SIZES.iter().find(|(id, _, _, _, _)| Some(*id) == get_str(v, "size") && *id != "custom");
    if let Some(t) = row {
        if get_str(v, "kind") == Some("external") {
            m.insert("body_name".into(), json!(format!("{} screw", t.0)));
        }
    } else {
        m.remove("size");
    }
    params_cleanup(Value::Object(m))
}

fn from_params_thread(p: Value) -> Values {
    let mut o = Map::new();
    o.insert("size".into(), json!("custom"));
    if let Some(src) = p.as_object() {
        for (k, v) in src {
            o.insert(k.clone(), v.clone());
        }
    }
    o
}

fn is_face_plane(v: &Values) -> bool {
    v.get("plane").is_some_and(|p| p.is_object() || p.is_array())
}

fn on_change_thread(key: &str, v: &Values, _: &Ctx) -> Option<Values> {
    match key {
        "size" => tables::thread_size_patch(get_str(v, "size")?).map(|mut m| {
            if let Some(d) = m.remove("diameter") {
                let n = d.as_f64().unwrap_or(0.0);
                m.insert("diameter".into(), json_f64(n));
            }
            if let Some(p) = m.remove("pitch") {
                let n = p.as_f64().unwrap_or(0.0);
                m.insert("pitch".into(), json_f64(n));
            }
            m
        }),
        "kind" => Some(values(&[(
            "operation",
            json!(if get_str(v, "kind") == Some("internal") {
                "cut"
            } else if is_face_plane(v) {
                "join"
            } else {
                "new"
            }),
        )])),
        "diameter" | "pitch" => Some(values(&[("size", json!("custom"))])),
        "plane" if get_str(v, "kind") == Some("external") && is_face_plane(v) => {
            Some(values(&[("operation", json!("join"))]))
        }
        _ => None,
    }
}

pub fn thread() -> ToolDef {
    ToolDef {
        key: "thread",
        label: "Thread",
        icon: "thread",
        kind: "thread",
        fields: vec![
            Field {
                key: "kind",
                label: "Type",
                kind: super::fields::opts(&[("internal", "Tapped hole"), ("external", "Screw / stud")]),
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "size",
                label: "Size",
                kind: thread_size_select(),
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "diameter",
                label: "Major Ø",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "pitch",
                label: "Pitch",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "length",
                label: "Length",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "tolerance",
                label: "Print clearance",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "hand",
                label: "Hand",
                kind: super::fields::opts(&[("right", "Right"), ("left", "Left")]),
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "chamfer",
                label: "Chamfered ends",
                kind: FieldKind::Bool,
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "plane",
                label: "On",
                kind: FieldKind::Plane { any_face: false },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "offset",
                label: "Position",
                kind: FieldKind::Vec3 { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "operation",
                label: "Operation",
                kind: super::select_flat(ops_options()),
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "targets",
                label: "Bodies",
                kind: FieldKind::Bodies { multi: true },
                help: None,
                placeholder: Some("Auto"),
                show: Some(show_targets),
            },
        ],
        hint: Some("Click a face to put the thread there (tapped holes go into the face, screws stand on it). For FDM, pitches of 1 mm or more and 0.2–0.4 mm clearance print best"),
        extra: None,
        flags: ToolFlags { place_on_face: true, ..ToolFlags::default() },
        defaults: defaults_thread,
        to_params: to_params_thread,
        from_params: from_params_thread,
        on_change: Some(on_change_thread),
    }
}

pub fn offset_tool() -> ToolDef {
    ToolDef {
        key: "offset",
        label: "Offset Body",
        icon: "offset",
        kind: "offset",
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
                key: "direction",
                label: "Direction",
                kind: super::fields::opts(&[("grow", "Grow (thicker)"), ("shrink", "Shrink (thinner)")]),
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "distance",
                label: "Distance",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
        ],
        hint: Some("Makes the whole body bigger or smaller by the same distance everywhere: for fit adjustments, moulds and clearance. Detailed meshes take a few seconds"),
        extra: None,
        flags: ToolFlags { no_live_preview: true, ..ToolFlags::default() },
        defaults: |ctx| {
            values(&[
                ("bodies", json!(sel_body(ctx))),
                ("direction", json!("grow")),
                ("distance", json!(0.2)),
            ])
        },
        to_params: |v, _| to_params_default(v, None),
        from_params: |p| p.as_object().cloned().unwrap_or_default(),
        on_change: None,
    }
}

fn to_params_thincheck(v: &Values, _: &Ctx) -> Value {
    let mut m = Map::new();
    put_opt(&mut m, "body", first_of(v, "bodies").as_ref());
    put_opt(&mut m, "min", v.get("min"));
    Value::Object(m)
}

fn from_params_thincheck(p: Value) -> Values {
    let mut o = Map::new();
    o.insert("min".into(), json!(0.8));
    if let Some(src) = p.as_object() {
        for (k, v) in src {
            o.insert(k.clone(), v.clone());
        }
    }
    let bodies = wrap_truthy(o.get("body"));
    o.insert("bodies".into(), bodies);
    o
}

pub fn thincheck() -> ToolDef {
    ToolDef {
        key: "thincheck",
        label: "Thin-wall Check",
        icon: "measure",
        kind: "thincheck",
        fields: vec![
            Field {
                key: "bodies",
                label: "Body",
                kind: FieldKind::Bodies { multi: false },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "min",
                label: "Minimum thickness",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
        ],
        hint: Some("Finds walls, pins and ribs thinner than the minimum, which may not print. The result shows below and nothing is changed. 0.8 mm suits a 0.4 mm nozzle. Click Apply to keep the check in the timeline"),
        extra: None,
        flags: ToolFlags::default(),
        defaults: |ctx| {
            values(&[
                ("bodies", json!(sel_body(ctx).into_iter().take(1).collect::<Vec<_>>())),
                ("min", json!(0.8)),
            ])
        },
        to_params: to_params_thincheck,
        from_params: from_params_thincheck,
        on_change: None,
    }
}

pub fn fittest() -> ToolDef {
    ToolDef {
        key: "fittest",
        label: "Fit Test",
        icon: "hole",
        kind: "fittest",
        fields: vec![
            Field {
                key: "diameter",
                label: "Peg diameter",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "start",
                label: "Clearance start",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "step",
                label: "Clearance step",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "count",
                label: "Count",
                kind: FieldKind::Number { unit: None },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "thickness",
                label: "Plate thickness",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "gap",
                label: "Gap between pairs",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
        ],
        hint: Some("Makes one plate with pegs and matching holes at stepped clearances (e.g. 0.05 to 0.30 mm). Print it, try each peg in its hole, and use the loosest snug fit as your printer’s clearance"),
        extra: None,
        flags: ToolFlags::default(),
        defaults: |_| {
            values(&[
                ("diameter", json!(8)),
                ("start", json!(0.05)),
                ("step", json!(0.05)),
                ("count", json!(6)),
                ("thickness", json!(3)),
                ("gap", json!(4)),
            ])
        },
        to_params: |v, _| to_params_default(v, None),
        from_params: |p| p.as_object().cloned().unwrap_or_default(),
        on_change: None,
    }
}

fn show_resize_size(v: &Values) -> bool {
    get_str(v, "mode") != Some("fill")
}

fn defaults_resizehole(_: &Ctx) -> Values {
    values(&[
        ("face", Value::Null),
        ("body", Value::Null),
        ("point", Value::Null),
        ("mode", json!("resize")),
        ("size", json!("custom")),
        ("diameter", json!(5)),
    ])
}

fn to_params_resizehole(v: &Values, _: &Ctx) -> Value {
    let mut m = Map::new();
    for k in ["body", "point", "diameter", "mode", "measured", "kind"] {
        put_opt(&mut m, k, v.get(k));
    }
    Value::Object(m)
}

fn from_params_resizehole(p: Value) -> Values {
    let mut out = Map::new();
    if let Some(src) = p.as_object() {
        for (k, v) in src {
            out.insert(k.clone(), v.clone());
        }
    }
    let point = out.get("point").cloned();
    let measured = out.get("measured");
    let kind = out.get("kind");
    let face = if point.as_ref().is_some_and(js_truthy) {
        let label = measured.filter(|m| js_truthy(m)).map(|m| {
            let prefix = if kind.and_then(Value::as_str) == Some("pin") {
                "Pin"
            } else {
                "Hole"
            };
            format!("{prefix} Ø{}", js_to_fixed(js_to_number(m), 2))
        });
        json!({
            "label": label.unwrap_or_else(|| "Picked".into())
        })
    } else {
        Value::Null
    };
    out.insert("size".into(), json!("custom"));
    out.insert("face".into(), face);
    out
}

fn on_change_resizehole(key: &str, v: &Values, _: &Ctx) -> Option<Values> {
    match key {
        "size" => {
            let d = tables::resize_size_diameter(get_str(v, "size")?)?;
            Some(values(&[("diameter", js_num(d)), ("_sized", json!(true))]))
        }
        "diameter" => Some(values(&[("size", json!("custom")), ("_sized", json!(true))])),
        _ => None,
    }
}

pub fn resizehole() -> ToolDef {
    ToolDef {
        key: "resizehole",
        label: "Resize Hole",
        icon: "resizehole",
        kind: "resizehole",
        fields: vec![
            Field {
                key: "face",
                label: "Hole or pin",
                kind: FieldKind::CylFace,
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "mode",
                label: "Change",
                kind: super::fields::opts(&[("resize", "New diameter"), ("fill", "Fill the hole")]),
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "size",
                label: "Size",
                kind: super::fields::opts(&[("custom", "Custom"), ("m16", "M1.6 screw clearance (1.8)"), ("m2", "M2 screw clearance (2.4)"), ("m25", "M2.5 screw clearance (2.9)"), ("m3", "M3 screw clearance (3.4)"), ("m4", "M4 screw clearance (4.5)"), ("m5", "M5 screw clearance (5.5)"), ("m6", "M6 screw clearance (6.6)"), ("m2tap", "M2 self-tapping into plastic (1.7)"), ("m25tap", "M2.5 self-tapping into plastic (2.1)"), ("m3tap", "M3 self-tapping into plastic (2.6)"), ("m2ins", "M2 heat-set insert (3.2)"), ("m3ins", "M3 heat-set insert (4.0)"), ("m4ins", "M4 heat-set insert (5.6)"), ("i4", "#4 screw clearance (3.26)"), ("i6", "#6 screw clearance (3.8)"), ("i8", "#8 screw clearance (4.5)"), ("i10", "#10 screw clearance (5.11)"), ("i14", "1/4\" screw clearance (6.76)"), ("i4ins", "#4-40 heat-set insert (4.09)"), ("i6ins", "#6-32 heat-set insert (5.11)"), ("mag6", "6 mm magnet (6.2)"), ("mag8", "8 mm magnet (8.2)"), ("mag10", "10 mm magnet (10.2)"), ("fil", "1.75 mm filament pin (1.9)"), ("b608", "608 bearing seat (22.1)")]),
                help: None,
                placeholder: None,
                show: Some(show_resize_size),
            },
            Field {
                key: "diameter",
                label: "Diameter",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: Some(show_resize_size),
            },
        ],
        hint: Some("Click the inside wall of a round hole (or the side of a pin). It is measured, then type the diameter you need"),
        extra: None,
        flags: ToolFlags::default(),
        defaults: defaults_resizehole,
        to_params: to_params_resizehole,
        from_params: from_params_resizehole,
        on_change: Some(on_change_resizehole),
    }
}

pub fn pushpull() -> ToolDef {
    ToolDef {
        key: "pushpull",
        label: "Push/Pull",
        icon: "pushpull",
        kind: "pushpull",
        fields: vec![
            Field {
                key: "face",
                label: "Flat face",
                kind: FieldKind::PFace { any_face: false },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "distance",
                label: "Distance",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
        ],
        hint: Some("Click a flat face · positive distance pulls it out, negative pushes it in"),
        extra: None,
        flags: ToolFlags::default(),
        defaults: |_| values(&[("face", Value::Null), ("body", Value::Null), ("distance", json!(5))]),
        to_params: |v, _| {
            let mut m = Map::new();
            for k in ["body", "face", "distance"] {
                put_opt(&mut m, k, v.get(k));
            }
            Value::Object(m)
        },
        from_params: |p| p.as_object().cloned().unwrap_or_default(),
        on_change: None,
    }
}

fn show_mirror_features(v: &Values) -> bool {
    get_str(v, "kind") == Some("features")
}

fn show_mirror_bodies(v: &Values) -> bool {
    get_str(v, "kind") == Some("bodies")
}

fn defaults_mirror(ctx: &Ctx) -> Values {
    let last = last_repeatable(ctx);
    let s = sel_body(ctx);
    let kind = if !s.is_empty() || last.is_none() { "bodies" } else { "features" };
    values(&[
        ("kind", json!(kind)),
        ("features", json!(last.map(|x| vec![x]).unwrap_or_default())),
        ("bodies", json!(s)),
        ("plane", json!("YZ")),
        ("mode", json!("copy")),
    ])
}

fn from_params_mirror(p: Value) -> Values {
    let mut o = Map::new();
    o.insert("kind".into(), json!("bodies"));
    o.insert("features".into(), json!([]));
    o.insert("bodies".into(), json!([]));
    o.insert("mode".into(), json!("copy"));
    if let Some(src) = p.as_object() {
        for (k, v) in src {
            o.insert(k.clone(), v.clone());
        }
    }
    o
}

pub fn mirror() -> ToolDef {
    ToolDef {
        key: "mirror",
        label: "Mirror",
        icon: "mirror",
        kind: "mirror",
        fields: vec![
            Field {
                key: "kind",
                label: "Mirror",
                kind: super::fields::opts(&[("features", "Features"), ("bodies", "Bodies")]),
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "features",
                label: "Features",
                kind: FieldKind::Features,
                help: None,
                placeholder: None,
                show: Some(show_mirror_features),
            },
            Field {
                key: "bodies",
                label: "Bodies",
                kind: FieldKind::Bodies { multi: true },
                help: None,
                placeholder: None,
                show: Some(show_mirror_bodies),
            },
            Field {
                key: "plane",
                label: "Mirror plane",
                kind: FieldKind::Plane { any_face: false },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "mode",
                label: "Result",
                kind: super::fields::opts(&[("join", "Join"), ("copy", "New body"), ("replace", "Replace")]),
                help: None,
                placeholder: None,
                show: Some(show_mirror_bodies),
            },
        ],
        hint: Some("Features: repeats a cut, hole or extrude on the other side of the plane (tick them below or click them on the timeline). Bodies: mirrors whole bodies"),
        extra: None,
        flags: ToolFlags::default(),
        defaults: defaults_mirror,
        to_params: |v, _| to_params_default(v, None),
        from_params: from_params_mirror,
        on_change: None,
    }
}

pub fn move_tool() -> ToolDef {
    ToolDef {
        key: "move",
        label: "Move / Rotate",
        icon: "move",
        kind: "move",
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
                key: "_gizmo",
                label: "Drag handles",
                kind: super::fields::opts(&[("move", "Move (W)"), ("rotate", "Rotate (R)")]),
                help: Some("Which handles show on the part: arrows to move it, or rings to turn it (5° steps; hold Shift to turn freely)"),
                placeholder: None,
                show: None,
            },
            Field {
                key: "translate",
                label: "Move X Y Z",
                kind: FieldKind::Vec3 { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "rotate",
                label: "Rotate X Y Z",
                kind: FieldKind::Vec3 { unit: Some("°") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "pivot",
                label: "Rotate about",
                kind: super::fields::opts(&[("center", "Centre"), ("base", "Base"), ("origin", "Origin")]),
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "bed",
                label: "Land on bed",
                kind: FieldKind::Bool,
                help: Some("After a rotation the part rests on the bed instead of hanging through it (Move Z then lifts it off the bed)"),
                placeholder: None,
                show: None,
            },
            Field {
                key: "copy",
                label: "Make a copy",
                kind: FieldKind::Bool,
                help: None,
                placeholder: None,
                show: None,
            },
        ],
        hint: Some("Drag the arrows to move and the rings to rotate, or type distances and angles"),
        extra: Some("dropToBed"),
        flags: ToolFlags { gizmo: true, ..ToolFlags::default() },
        defaults: |ctx| {
            values(&[
                ("bodies", json!(sel_body(ctx))),
                ("_gizmo", json!("move")),
                ("translate", json!([0, 0, 0])),
                ("rotate", json!([0, 0, 0])),
                ("pivot", json!("center")),
                ("bed", json!(true)),
                ("copy", json!(false)),
            ])
        },
        to_params: |v, _| to_params_default(v, None),
        from_params: |p| {
            let mut o = Map::new();
            o.insert("_gizmo".into(), json!("move"));
            o.insert("bed".into(), json!(false));
            if let Some(src) = p.as_object() {
                for (k, v) in src {
                    o.insert(k.clone(), v.clone());
                }
            }
            o
        },
        on_change: None,
    }
}

fn show_scale_factor(v: &Values) -> bool {
    get_str(v, "mode") == Some("factor") && get_bool(v, "uniform")
}

fn show_scale_factor3(v: &Values) -> bool {
    get_str(v, "mode") == Some("factor") && !get_bool(v, "uniform")
}

fn show_scale_fit(v: &Values) -> bool {
    get_str(v, "mode") == Some("fit")
}

fn show_scale_uniform(v: &Values) -> bool {
    get_str(v, "mode") != Some("units")
}

fn show_scale_units(v: &Values) -> bool {
    get_str(v, "mode") == Some("units")
}

fn defaults_scale(ctx: &Ctx) -> Values {
    let id = sel_body(ctx).into_iter().next();
    let b = id.and_then(|id| ctx.bodies.iter().find(|b| b.id == id));
    let size = b.map(size_from_bbox).unwrap_or([10.0, 10.0, 10.0]);
    let size_json = Value::Array(size.iter().map(|&x| json_f64(x)).collect());
    values(&[
        ("bodies", json!(sel_body(ctx))),
        ("mode", json!("factor")),
        ("factor", json!(1)),
        ("factor3", json!([1, 1, 1])),
        ("size", size_json.clone()),
        ("uniform", json!(true)),
        ("units", json!("in_to_mm")),
        ("pivot", json!("base")),
        ("_size0", size_json),
    ])
}

fn to_params_scale(v: &Values, _: &Ctx) -> Value {
    let mut m = Map::new();
    for k in ["bodies", "mode", "pivot", "units", "uniform"] {
        put_opt(&mut m, k, v.get(k));
    }
    let uniform = truthy(v, "uniform");
    if get_str(v, "mode") == Some("factor") {
        put_opt(&mut m, "factor", v.get(if uniform { "factor" } else { "factor3" }));
    }
    if get_str(v, "mode") == Some("fit") {
        put_opt(&mut m, "size", v.get("size"));
        if let (true, Some(sz), true) = (uniform, v.get("size").and_then(Value::as_array), truthy(v, "_size0")) {
            let sz0 = v.get("_size0").and_then(Value::as_array).cloned().unwrap_or_default();
            // JS: d = size.map((s, i) => Math.abs(s - _size0[i])); axis = d.indexOf(Math.max(...d))
            let d: Vec<f64> = sz
                .iter()
                .enumerate()
                .map(|(i, s)| (js_to_number(s) - sz0.get(i).map_or(f64::NAN, js_to_number)).abs())
                .collect();
            let max = d.iter().copied().fold(f64::NEG_INFINITY, |a, b| if a.is_nan() || b.is_nan() { f64::NAN } else { a.max(b) });
            let axis = d.iter().position(|x| *x == max).map_or(-1, |i| i as i64);
            m.insert("axis".into(), json!(axis));
        }
    }
    params_cleanup(Value::Object(m))
}

fn from_params_scale(p: Value) -> Values {
    let o = p.as_object();
    let factor = o.and_then(|o| o.get("factor"));
    let (factor, factor3) = if factor.is_some_and(Value::is_array) {
        (json!(1), factor.cloned().unwrap_or(json!([1, 1, 1])))
    } else {
        // JS `p.factor ?? 1`
        (factor.filter(|f| !f.is_null()).cloned().unwrap_or(json!(1)), json!([1, 1, 1]))
    };
    let mut out = o.cloned().unwrap_or_default();
    out.insert("factor".into(), factor);
    out.insert("factor3".into(), factor3);
    // JS `p.size || [10, 10, 10]`
    if !truthy(&out, "size") {
        out.insert("size".into(), json!([10, 10, 10]));
    }
    out
}

pub fn scale_tool() -> ToolDef {
    ToolDef {
        key: "scale",
        label: "Scale",
        icon: "scale",
        kind: "scale",
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
                key: "mode",
                label: "Mode",
                kind: super::fields::opts(&[("factor", "Factor"), ("fit", "To size"), ("units", "Units")]),
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "factor",
                label: "Factor",
                kind: FieldKind::Number { unit: None },
                help: None,
                placeholder: None,
                show: Some(show_scale_factor),
            },
            Field {
                key: "factor3",
                label: "Factor X Y Z",
                kind: FieldKind::Vec3 { unit: None },
                help: None,
                placeholder: None,
                show: Some(show_scale_factor3),
            },
            Field {
                key: "size",
                label: "Size X Y Z",
                kind: FieldKind::Vec3 { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: Some(show_scale_fit),
            },
            Field {
                key: "uniform",
                label: "Uniform",
                kind: FieldKind::Bool,
                help: None,
                placeholder: None,
                show: Some(show_scale_uniform),
            },
            Field {
                key: "units",
                label: "Convert",
                kind: super::fields::opts(&[("in_to_mm", "inch → mm"), ("mm_to_in", "mm → inch"), ("cm_to_mm", "cm → mm"), ("m_to_mm", "m → mm")]),
                help: None,
                placeholder: None,
                show: Some(show_scale_units),
            },
            Field {
                key: "pivot",
                label: "Fixed point",
                kind: super::fields::opts(&[("base", "Base"), ("center", "Centre"), ("origin", "Origin")]),
                help: None,
                placeholder: None,
                show: None,
            },
        ],
        hint: Some("Drag the handles on the part to stretch it (the middle one scales evenly), or type factors and sizes. To size with Uniform on: change one dimension and the others follow"),
        extra: None,
        flags: ToolFlags { gizmo: true, ..ToolFlags::default() },
        defaults: defaults_scale,
        to_params: to_params_scale,
        from_params: from_params_scale,
        on_change: None,
    }
}

fn show_pattern_features(v: &Values) -> bool {
    get_str(v, "source") == Some("features")
}

fn show_pattern_bodies(v: &Values) -> bool {
    get_str(v, "source") == Some("bodies")
}

fn show_pattern_spacing(v: &Values) -> bool {
    get_str(v, "kind") == Some("linear")
}

fn show_pattern_circular(v: &Values) -> bool {
    get_str(v, "kind") == Some("circular")
}

fn show_pattern_join(v: &Values) -> bool {
    get_str(v, "source") == Some("bodies")
}

fn defaults_pattern(ctx: &Ctx) -> Values {
    let last = last_repeatable(ctx);
    let s = sel_body(ctx);
    let source = if !s.is_empty() || last.is_none() { "bodies" } else { "features" };
    values(&[
        ("source", json!(source)),
        ("features", json!(last.map(|x| vec![x]).unwrap_or_default())),
        ("bodies", json!(s)),
        ("kind", json!("linear")),
        ("count", json!(3)),
        ("axisname", json!("x")),
        ("spacing", json!(20)),
        ("angle", json!(360)),
        ("center", json!([0, 0, 0])),
        ("join", json!(false)),
    ])
}

fn to_params_pattern(v: &Values, _: &Ctx) -> Value {
    let mut m = Map::new();
    for k in ["source", "features", "bodies", "kind", "count", "spacing", "angle", "center", "join"] {
        put_opt(&mut m, k, v.get(k));
    }
    // AXV[v.axisname]: undefined for an unknown name, so both vectors are dropped
    if let Some(name) = get_str(v, "axisname").filter(|n| matches!(*n, "x" | "y" | "z")) {
        let axis = axv(name);
        let axis_json = json!([axis[0] as i64, axis[1] as i64, axis[2] as i64]);
        m.insert("direction".into(), axis_json.clone());
        m.insert("axis".into(), axis_json);
    }
    put_opt(&mut m, "axisname", v.get("axisname"));
    Value::Object(m)
}

fn from_params_pattern(p: Value) -> Values {
    let mut o = Map::new();
    o.insert("source".into(), json!("bodies"));
    o.insert("features".into(), json!([]));
    if let Some(src) = p.as_object() {
        for (k, v) in src {
            o.insert(k.clone(), v.clone());
        }
    }
    // JS `p.axisname || vecAxis(p.direction)`
    if !truthy(&o, "axisname") {
        let dir = o.get("direction").filter(|d| js_truthy(d)).and_then(Value::as_array);
        match dir {
            None => {
                o.insert("axisname".into(), json!("z"));
            }
            Some(a) => {
                let nums: Vec<f64> = a.iter().map(|x| js_to_number(x).abs()).collect();
                if nums.is_empty() {
                    o.remove("axisname");
                } else {
                    let max = nums.iter().copied().fold(f64::NEG_INFINITY, |a, b| if a.is_nan() || b.is_nan() { f64::NAN } else { a.max(b) });
                    match nums.iter().position(|x| *x == max).and_then(|i| ["x", "y", "z"].get(i)) {
                        Some(n) => {
                            o.insert("axisname".into(), json!(n));
                        }
                        None => {
                            o.remove("axisname");
                        }
                    }
                }
            }
        }
    }
    o
}

pub fn pattern() -> ToolDef {
    ToolDef {
        key: "pattern",
        label: "Pattern",
        icon: "pattern",
        kind: "pattern",
        fields: vec![
            Field {
                key: "source",
                label: "Pattern",
                kind: super::fields::opts(&[("features", "Features"), ("bodies", "Bodies")]),
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "features",
                label: "Features",
                kind: FieldKind::Features,
                help: None,
                placeholder: None,
                show: Some(show_pattern_features),
            },
            Field {
                key: "bodies",
                label: "Bodies",
                kind: FieldKind::Bodies { multi: true },
                help: None,
                placeholder: None,
                show: Some(show_pattern_bodies),
            },
            Field {
                key: "kind",
                label: "Type",
                kind: super::fields::opts(&[("linear", "Linear"), ("circular", "Circular")]),
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "count",
                label: "Count",
                kind: FieldKind::Number { unit: None },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "axisname",
                label: "Direction",
                kind: super::fields::opts(&[("x", "X"), ("y", "Y"), ("z", "Z")]),
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "spacing",
                label: "Spacing",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: Some(show_pattern_spacing),
            },
            Field {
                key: "angle",
                label: "Total angle",
                kind: FieldKind::Number { unit: Some("°") },
                help: None,
                placeholder: None,
                show: Some(show_pattern_circular),
            },
            Field {
                key: "center",
                label: "Axis through",
                kind: FieldKind::Vec3 { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: Some(show_pattern_circular),
            },
            Field {
                key: "join",
                label: "Join copies",
                kind: FieldKind::Bool,
                help: None,
                placeholder: None,
                show: Some(show_pattern_join),
            },
        ],
        hint: Some("Features: repeats a cut, hole or extrude (tick them below or click them on the timeline). Bodies: copies whole bodies"),
        extra: None,
        flags: ToolFlags::default(),
        defaults: defaults_pattern,
        to_params: to_params_pattern,
        from_params: from_params_pattern,
        on_change: None,
    }
}

fn body_field() -> Field {
    Field {
        key: "body",
        label: "Body",
        kind: FieldKind::Bodies { multi: false },
        help: None,
        placeholder: None,
        show: None,
    }
}

fn to_params_body_one(v: &Values, _: &Ctx) -> Value {
    let mut m = v.clone();
    m.remove("body");
    put_opt(&mut m, "body", first_of(v, "body").as_ref());
    params_cleanup(Value::Object(m))
}

fn from_params_body_one(p: Value) -> Values {
    let mut o = p.as_object().cloned().unwrap_or_default();
    let body = wrap_truthy(o.get("body"));
    o.insert("body".into(), body);
    o
}

pub fn hollow() -> ToolDef {
    ToolDef {
        key: "hollow",
        label: "Hollow",
        icon: "hollow",
        kind: "hollow",
        fields: vec![
            body_field(),
            Field {
                key: "thickness",
                label: "Wall",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "resolution",
                label: "Quality",
                kind: super::fields::opts(&[("draft", "Draft"), ("normal", "Normal"), ("fine", "Fine")]),
                help: None,
                placeholder: None,
                show: None,
            },
        ],
        hint: Some("Shells the body inward. Add a hole afterwards to drain resin or save filament"),
        extra: None,
        flags: ToolFlags { no_live_preview: true, ..ToolFlags::default() },
        defaults: |ctx| {
            values(&[
                ("body", json!(sel_body(ctx).into_iter().take(1).collect::<Vec<_>>())),
                ("thickness", json!(2)),
                ("resolution", json!("normal")),
            ])
        },
        to_params: to_params_body_one,
        from_params: from_params_body_one,
        on_change: None,
    }
}

pub fn lattice() -> ToolDef {
    ToolDef {
        key: "lattice",
        label: "Lattice",
        icon: "hollow",
        kind: "lattice",
        fields: vec![
            body_field(),
            Field {
                key: "cell",
                label: "Cell size",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "strut",
                label: "Strut width",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "thickness",
                label: "Wall",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "resolution",
                label: "Quality",
                kind: super::fields::opts(&[("draft", "Draft"), ("normal", "Normal"), ("fine", "Fine")]),
                help: None,
                placeholder: None,
                show: None,
            },
        ],
        hint: Some("Keeps a solid wall and fills the inside with a grid of thin struts, to save plastic"),
        extra: None,
        flags: ToolFlags { no_live_preview: true, ..ToolFlags::default() },
        defaults: |ctx| {
            values(&[
                ("body", json!(sel_body(ctx).into_iter().take(1).collect::<Vec<_>>())),
                ("cell", json!(6)),
                ("strut", json!(1)),
                ("thickness", json!(2)),
                ("resolution", json!("normal")),
            ])
        },
        to_params: to_params_body_one,
        from_params: from_params_body_one,
        on_change: None,
    }
}

/// JS `+v.openings >= n`
fn openings_at_least(v: &Values, n: u8) -> bool {
    get_f64(v, "openings") >= f64::from(n)
}

fn shell_show_face1(v: &Values) -> bool {
    openings_at_least(v, 1)
}
fn shell_show_face2(v: &Values) -> bool {
    openings_at_least(v, 2)
}
fn shell_show_face3(v: &Values) -> bool {
    openings_at_least(v, 3)
}
fn shell_show_face4(v: &Values) -> bool {
    openings_at_least(v, 4)
}

fn to_params_shell(v: &Values, _: &Ctx) -> Value {
    // Array.isArray(v.body) ? v.body[0] : v.body
    let body = match v.get("body") {
        Some(Value::Array(a)) => a.first().cloned(),
        other => other.cloned(),
    };
    let faces: Vec<Value> = (1..=4u8)
        .filter(|i| openings_at_least(v, *i))
        .filter_map(|i| v.get(&format!("face{i}")).cloned())
        .filter(js_truthy)
        .collect();
    let mut m = Map::new();
    put_opt(&mut m, "body", body.as_ref());
    m.insert("faces".into(), Value::Array(faces));
    put_opt(&mut m, "thickness", v.get("thickness"));
    put_opt(&mut m, "resolution", v.get("resolution"));
    Value::Object(m)
}

fn from_params_shell(p: Value) -> Values {
    let o = p.as_object().cloned().unwrap_or_default();
    let faces = o.get("faces").filter(|f| js_truthy(f)).and_then(Value::as_array).cloned().unwrap_or_default();
    let mut out = o.clone();
    out.insert("body".into(), wrap_truthy(o.get("body")));
    out.insert("openings".into(), json!(faces.len().to_string()));
    for i in 0..4 {
        // JS `faces[i] || null`
        let face = faces.get(i).filter(|f| js_truthy(f)).cloned().unwrap_or(Value::Null);
        out.insert(format!("face{}", i + 1), face);
    }
    out
}

pub fn shell() -> ToolDef {
    ToolDef {
        key: "shell",
        label: "Shell",
        icon: "hollow",
        kind: "shell",
        fields: vec![
            body_field(),
            Field {
                key: "openings",
                label: "Open faces",
                kind: super::fields::opts(&[("0", "None (closed)"), ("1", "1"), ("2", "2"), ("3", "3"), ("4", "4")]),
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "face1",
                label: "Face 1",
                kind: FieldKind::PFace { any_face: false },
                help: None,
                placeholder: None,
                show: Some(shell_show_face1),
            },
            Field {
                key: "face2",
                label: "Face 2",
                kind: FieldKind::PFace { any_face: false },
                help: None,
                placeholder: None,
                show: Some(shell_show_face2),
            },
            Field {
                key: "face3",
                label: "Face 3",
                kind: FieldKind::PFace { any_face: false },
                help: None,
                placeholder: None,
                show: Some(shell_show_face3),
            },
            Field {
                key: "face4",
                label: "Face 4",
                kind: FieldKind::PFace { any_face: false },
                help: None,
                placeholder: None,
                show: Some(shell_show_face4),
            },
            Field {
                key: "thickness",
                label: "Wall",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "resolution",
                label: "Quality",
                kind: super::fields::opts(&[("draft", "Draft"), ("normal", "Normal"), ("fine", "Fine")]),
                help: None,
                placeholder: None,
                show: None,
            },
        ],
        hint: Some("Click the flat faces to leave open; the rest of the part becomes a wall of this thickness, measured inward"),
        extra: None,
        flags: ToolFlags { no_live_preview: true, ..ToolFlags::default() },
        defaults: |ctx| {
            values(&[
                ("body", json!(sel_body(ctx).into_iter().take(1).collect::<Vec<_>>())),
                ("openings", json!("1")),
                ("face1", Value::Null),
                ("face2", Value::Null),
                ("face3", Value::Null),
                ("face4", Value::Null),
                ("thickness", json!(2)),
                ("resolution", json!("normal")),
            ])
        },
        to_params: to_params_shell,
        from_params: from_params_shell,
        on_change: None,
    }
}
