//! MODEL tab tools (create / primitives).

use serde_json::{json, Value};

use super::fields::{ops_options, show_targets};
use super::util::*;
use super::{Ctx, Field, FieldKind, ToolDef, ToolFlags, Values};

fn show_extrude_through(v: &Values) -> bool {
    get_str(v, "operation") == Some("cut")
}

fn show_extrude_distance(v: &Values) -> bool {
    !(get_bool(v, "through_all") && get_str(v, "operation") == Some("cut"))
}

fn show_extrude_distance2(v: &Values) -> bool {
    get_str(v, "direction") == Some("two") && !(get_bool(v, "through_all") && get_str(v, "operation") == Some("cut"))
}

/// JS `sketch: ctx.lastSketch` is undefined (so left out) when no sketch exists yet.
fn unset_sketch(mut v: Values, ctx: &Ctx, keys: &[&str]) -> Values {
    if ctx.last_sketch.is_none() {
        for k in keys {
            v.remove(*k);
        }
    }
    v
}

fn defaults_extrude(ctx: &Ctx) -> Values {
    let v = values(&[
        ("sketch", json!(ctx.last_sketch.unwrap_or(""))),
        ("regions", json!([])),
        (
            "operation",
            json!(if ctx.sketch_on_body { "join" } else { "new" }),
        ),
        ("direction", json!("one")),
        ("distance", json!(10)),
        ("distance2", json!(10)),
        ("start_offset", json!(0)),
        ("taper", json!(0)),
        ("through_all", json!(false)),
        ("targets", json!([])),
    ]);
    unset_sketch(v, ctx, &["sketch"])
}

fn to_params_extrude(v: &Values, _: &Ctx) -> Value {
    to_params_default(v, None)
}

fn from_params_extrude(p: Value) -> Values {
    p.as_object().cloned().unwrap_or_default()
}

pub fn extrude() -> ToolDef {
    ToolDef {
        key: "extrude",
        label: "Extrude",
        icon: "extrude",
        kind: "extrude",
        fields: vec![
            Field {
                key: "sketch",
                label: "Sketch",
                kind: FieldKind::Sketch,
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "regions",
                label: "Profiles",
                kind: FieldKind::Regions,
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
                key: "direction",
                label: "Direction",
                kind: super::fields::opts(&[("one", "One side"), ("reverse", "Flipped"), ("symmetric", "Symmetric"), ("two", "Two sides")]),
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "through_all",
                label: "Through all",
                kind: FieldKind::Bool,
                help: None,
                placeholder: None,
                show: Some(show_extrude_through),
            },
            Field {
                key: "distance",
                label: "Distance",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: Some(show_extrude_distance),
            },
            Field {
                key: "distance2",
                label: "Distance 2",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: Some(show_extrude_distance2),
            },
            Field {
                key: "start_offset",
                label: "Start offset",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "taper",
                label: "Taper angle",
                kind: FieldKind::Number { unit: Some("°") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "targets",
                label: "Bodies",
                kind: FieldKind::Bodies { multi: true },
                help: None,
                placeholder: Some("Auto (bodies it touches)"),
                show: Some(show_targets),
            },
        ],
        hint: Some("Pick a sketch, set the distance · Cut goes into the material automatically"),
        extra: None,
        flags: ToolFlags::default(),
        defaults: defaults_extrude,
        to_params: to_params_extrude,
        from_params: from_params_extrude,
        on_change: None,
    }
}

fn defaults_revolve(ctx: &Ctx) -> Values {
    let v = values(&[
        ("sketch", json!(ctx.last_sketch.unwrap_or(""))),
        ("regions", json!([])),
        ("axis", json!("y")),
        ("angle", json!(360)),
        ("operation", json!("new")),
        ("targets", json!([])),
    ]);
    unset_sketch(v, ctx, &["sketch"])
}

fn to_params_revolve(v: &Values, _: &Ctx) -> Value {
    to_params_default(v, None)
}

fn from_params_revolve(p: Value) -> Values {
    p.as_object().cloned().unwrap_or_default()
}

pub fn revolve() -> ToolDef {
    ToolDef {
        key: "revolve",
        label: "Revolve",
        icon: "revolve",
        kind: "revolve",
        fields: vec![
            Field {
                key: "sketch",
                label: "Sketch",
                kind: FieldKind::Sketch,
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "regions",
                label: "Profiles",
                kind: FieldKind::Regions,
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "axis",
                label: "Axis",
                kind: FieldKind::AxisLine,
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
        hint: Some("Revolves the sketch profile about the sketch X or Y axis, or a construction line in the sketch"),
        extra: None,
        flags: ToolFlags::default(),
        defaults: defaults_revolve,
        to_params: to_params_revolve,
        from_params: from_params_revolve,
        on_change: None,
    }
}

fn defaults_loft(ctx: &Ctx) -> Values {
    let v = values(&[
        ("profile_1", json!(ctx.last_sketch.unwrap_or(""))),
        ("profile_2", json!("")),
        ("profile_3", json!("")),
        ("profile_4", json!("")),
        ("operation", json!("new")),
        ("targets", json!([])),
    ]);
    unset_sketch(v, ctx, &["profile_1"])
}

fn to_params_loft(v: &Values, _: &Ctx) -> Value {
    let profiles: Vec<Value> = ["profile_1", "profile_2", "profile_3", "profile_4"]
        .iter()
        .filter_map(|k| v.get(*k))
        .filter(|x| js_truthy(x))
        .cloned()
        .collect();
    let mut m = Values::new();
    m.insert("profiles".into(), Value::Array(profiles));
    put_opt(&mut m, "operation", v.get("operation"));
    put_opt(&mut m, "targets", v.get("targets"));
    params_cleanup(Value::Object(m))
}

/// JS `p.profiles?.[i] || ''`
fn str_at(profiles: Option<&Vec<Value>>, i: usize) -> Value {
    profiles
        .and_then(|a| a.get(i))
        .filter(|x| js_truthy(x))
        .cloned()
        .unwrap_or(json!(""))
}

fn from_params_loft(p: Value) -> Values {
    let o = p.as_object();
    let profiles = o.and_then(|o| o.get("profiles")).and_then(Value::as_array);
    values(&[
        ("profile_1", str_at(profiles, 0)),
        ("profile_2", str_at(profiles, 1)),
        ("profile_3", str_at(profiles, 2)),
        ("profile_4", str_at(profiles, 3)),
        ("operation", o.and_then(|o| o.get("operation")).filter(|x| js_truthy(x)).cloned().unwrap_or(json!("new"))),
        ("targets", o.and_then(|o| o.get("targets")).filter(|x| js_truthy(x)).cloned().unwrap_or(json!([]))),
    ])
}

pub fn loft() -> ToolDef {
    ToolDef {
        key: "loft",
        label: "Loft",
        icon: "extrude",
        kind: "loft",
        fields: vec![
            Field {
                key: "profile_1",
                label: "Profile 1",
                kind: FieldKind::Sketch,
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "profile_2",
                label: "Profile 2",
                kind: FieldKind::Sketch,
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "profile_3",
                label: "Profile 3 (optional)",
                kind: FieldKind::Sketch,
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "profile_4",
                label: "Profile 4 (optional)",
                kind: FieldKind::Sketch,
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
        hint: Some("Pick two or more closed sketches on parallel planes, in order from the first end to the last. Each one is a single outline with no holes."),
        extra: None,
        flags: ToolFlags::default(),
        defaults: defaults_loft,
        to_params: to_params_loft,
        from_params: from_params_loft,
        on_change: None,
    }
}

fn defaults_sweep(ctx: &Ctx) -> Values {
    let v = values(&[
        ("profile", json!(ctx.last_sketch.unwrap_or(""))),
        ("path", json!("")),
        ("operation", json!("new")),
        ("targets", json!([])),
    ]);
    unset_sketch(v, ctx, &["profile"])
}

fn to_params_sweep(v: &Values, _: &Ctx) -> Value {
    to_params_default(v, None)
}

fn from_params_sweep(p: Value) -> Values {
    p.as_object().cloned().unwrap_or_default()
}

pub fn sweep() -> ToolDef {
    ToolDef {
        key: "sweep",
        label: "Sweep",
        icon: "revolve",
        kind: "sweep",
        fields: vec![
            Field {
                key: "profile",
                label: "Profile sketch",
                kind: FieldKind::Sketch,
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "path",
                label: "Path sketch",
                kind: FieldKind::Sketch,
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
        hint: Some("The profile starts at the first point of the path and turns to face along it. Draw the profile centred on its sketch origin. The path must be one open chain of lines and arcs."),
        extra: None,
        flags: ToolFlags::default(),
        defaults: defaults_sweep,
        to_params: to_params_sweep,
        from_params: from_params_sweep,
        on_change: None,
    }
}

fn primitive_defaults(ctx: &Ctx, shape: &str, label: &str, dims: &[(&str, Value)]) -> Values {
    let mut pairs: Vec<(&str, Value)> = vec![("shape", json!(shape))];
    pairs.extend_from_slice(dims);
    pairs.extend_from_slice(&[
        ("plane", json!("XY")),
        ("offset", json!([0, 0, 0])),
        ("rotate", json!([0, 0, 0])),
        ("anchor", json!("base")),
        ("operation", json!("new")),
        ("targets", json!([])),
        ("body_name", json!(label)),
    ]);
    let _ = ctx; // JS: operation always 'new' whether bodies exist or not
    values(&pairs)
}

fn primitive_fields(shape: &str, dims: Vec<Field>) -> Vec<Field> {
    let mut f = dims;
    f.push(Field { key: "plane", label: "On", kind: FieldKind::Plane { any_face: false }, help: None, placeholder: None, show: None });
    f.push(Field {
        key: "offset",
        label: "Position",
        kind: FieldKind::Vec3 { unit: Some("mm") },
        help: None,
        placeholder: None,
        show: None,
    });
    f.push(Field {
        key: "rotate",
        label: "Rotate",
        kind: FieldKind::Vec3 { unit: Some("°") },
        help: None,
        placeholder: None,
        show: None,
    });
    let mut anchor_opts = vec![
        ("base".into(), "Base".into()),
        ("center".into(), "Centre".into()),
    ];
    if shape == "box" {
        anchor_opts.push(("corner".into(), "Corner".into()));
    }
    f.push(Field {
        key: "anchor",
        label: "Anchor",
        kind: super::select_flat(anchor_opts),
        help: None,
        placeholder: None,
        show: None,
    });
    f.push(Field {
        key: "operation",
        label: "Operation",
        kind: super::select_flat(ops_options()),
        help: None,
        placeholder: None,
        show: None,
    });
    f.push(Field {
        key: "targets",
        label: "Bodies",
        kind: FieldKind::Bodies { multi: true },
        help: None,
        placeholder: Some("Auto"),
        show: Some(show_targets),
    });
    f
}

fn to_params_primitive(v: &Values, _: &Ctx) -> Value {
    to_params_default(v, None)
}

fn from_params_primitive(p: Value) -> Values {
    p.as_object().cloned().unwrap_or_default()
}

pub fn primitive_box() -> ToolDef {
    ToolDef {
        key: "primitive_box",
        label: "Box",
        icon: "box",
        kind: "primitive",
        fields: primitive_fields(
            "box",
            vec![
                Field {
                    key: "length",
                    label: "Length (X)",
                    kind: FieldKind::Number { unit: Some("mm") },
                    help: None,
                    placeholder: None,
                    show: None,
                },
                Field {
                    key: "width",
                    label: "Width (Y)",
                    kind: FieldKind::Number { unit: Some("mm") },
                    help: None,
                    placeholder: None,
                    show: None,
                },
                Field {
                    key: "height",
                    label: "Height (Z)",
                    kind: FieldKind::Number { unit: Some("mm") },
                    help: None,
                    placeholder: None,
                    show: None,
                },
            ],
        ),
        hint: Some("Click a flat face to place it there, or set the position · use Cut to subtract it"),
        extra: None,
        flags: ToolFlags { place_on_face: true, ..ToolFlags::default() },
        defaults: |ctx| {
            primitive_defaults(
                ctx,
                "box",
                "Box",
                &[("length", json!(20)), ("width", json!(20)), ("height", json!(20))],
            )
        },
        to_params: to_params_primitive,
        from_params: from_params_primitive,
        on_change: None,
    }
}

pub fn primitive_cylinder() -> ToolDef {
    ToolDef {
        key: "primitive_cylinder",
        label: "Cylinder",
        icon: "cylinder",
        kind: "primitive",
        fields: primitive_fields(
            "cylinder",
            vec![
                Field {
                    key: "radius",
                    label: "Radius",
                    kind: FieldKind::Number { unit: Some("mm") },
                    help: None,
                    placeholder: None,
                    show: None,
                },
                Field {
                    key: "height",
                    label: "Height",
                    kind: FieldKind::Number { unit: Some("mm") },
                    help: None,
                    placeholder: None,
                    show: None,
                },
            ],
        ),
        hint: Some("Click a flat face to place it there, or set the position · use Cut to subtract it"),
        extra: None,
        flags: ToolFlags { place_on_face: true, ..ToolFlags::default() },
        defaults: |ctx| primitive_defaults(ctx, "cylinder", "Cylinder", &[("radius", json!(10)), ("height", json!(20))]),
        to_params: to_params_primitive,
        from_params: from_params_primitive,
        on_change: None,
    }
}

pub fn primitive_sphere() -> ToolDef {
    ToolDef {
        key: "primitive_sphere",
        label: "Sphere",
        icon: "sphere",
        kind: "primitive",
        fields: primitive_fields(
            "sphere",
            vec![Field { key: "radius", label: "Radius", kind: FieldKind::Number { unit: Some("mm") }, help: None, placeholder: None, show: None }],
        ),
        hint: Some("Click a flat face to place it there, or set the position · use Cut to subtract it"),
        extra: None,
        flags: ToolFlags { place_on_face: true, ..ToolFlags::default() },
        defaults: |ctx| primitive_defaults(ctx, "sphere", "Sphere", &[("radius", json!(10))]),
        to_params: to_params_primitive,
        from_params: from_params_primitive,
        on_change: None,
    }
}

pub fn primitive_cone() -> ToolDef {
    ToolDef {
        key: "primitive_cone",
        label: "Cone",
        icon: "cone",
        kind: "primitive",
        fields: primitive_fields(
            "cone",
            vec![
                Field {
                    key: "radius",
                    label: "Base radius",
                    kind: FieldKind::Number { unit: Some("mm") },
                    help: None,
                    placeholder: None,
                    show: None,
                },
                Field {
                    key: "radius2",
                    label: "Top radius",
                    kind: FieldKind::Number { unit: Some("mm") },
                    help: None,
                    placeholder: None,
                    show: None,
                },
                Field {
                    key: "height",
                    label: "Height",
                    kind: FieldKind::Number { unit: Some("mm") },
                    help: None,
                    placeholder: None,
                    show: None,
                },
            ],
        ),
        hint: Some("Click a flat face to place it there, or set the position · use Cut to subtract it"),
        extra: None,
        flags: ToolFlags { place_on_face: true, ..ToolFlags::default() },
        defaults: |ctx| {
            primitive_defaults(
                ctx,
                "cone",
                "Cone",
                &[("radius", json!(10)), ("radius2", json!(0)), ("height", json!(20))],
            )
        },
        to_params: to_params_primitive,
        from_params: from_params_primitive,
        on_change: None,
    }
}

pub fn primitive_torus() -> ToolDef {
    ToolDef {
        key: "primitive_torus",
        label: "Torus",
        icon: "torus",
        kind: "primitive",
        fields: primitive_fields(
            "torus",
            vec![
                Field {
                    key: "radius",
                    label: "Ring radius",
                    kind: FieldKind::Number { unit: Some("mm") },
                    help: None,
                    placeholder: None,
                    show: None,
                },
                Field {
                    key: "radius2",
                    label: "Tube radius",
                    kind: FieldKind::Number { unit: Some("mm") },
                    help: None,
                    placeholder: None,
                    show: None,
                },
            ],
        ),
        hint: Some("Click a flat face to place it there, or set the position · use Cut to subtract it"),
        extra: None,
        flags: ToolFlags { place_on_face: true, ..ToolFlags::default() },
        defaults: |ctx| {
            primitive_defaults(
                ctx,
                "torus",
                "Torus",
                &[("radius", json!(15)), ("radius2", json!(4))],
            )
        },
        to_params: to_params_primitive,
        from_params: from_params_primitive,
        on_change: None,
    }
}

fn defaults_snapfit(_: &Ctx) -> Values {
    values(&[
        ("plane", json!("XY")),
        ("x", json!(10)),
        ("y", json!(10)),
        ("length", json!(12)),
        ("width", json!(6)),
        ("thickness", json!(1.5)),
        ("hook_height", json!(1.5)),
        ("operation", json!("join")),
        ("targets", json!([])),
    ])
}

pub fn snapfit() -> ToolDef {
    ToolDef {
        key: "snapfit",
        label: "Snap-fit",
        icon: "extrude",
        kind: "snapfit",
        fields: vec![
            Field {
                key: "plane",
                label: "On",
                kind: FieldKind::Plane { any_face: false },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "x",
                label: "Position X",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "y",
                label: "Position Y",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "length",
                label: "Arm length",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "width",
                label: "Arm width",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "thickness",
                label: "Arm thickness",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "hook_height",
                label: "Hook height",
                kind: FieldKind::Number { unit: Some("mm") },
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
        hint: Some("A thin arm standing on a face, with a hook at its tip. Position is the corner of the arm’s foot on that face. Choose the face with On, then set the size. The hook sticks out one arm thickness"),
        extra: None,
        flags: ToolFlags::default(),
        defaults: defaults_snapfit,
        to_params: to_params_primitive,
        from_params: from_params_primitive,
        on_change: None,
    }
}

fn defaults_hinge(_: &Ctx) -> Values {
    values(&[
        ("plane", json!("XY")),
        ("x", json!(10)),
        ("slot", json!(1)),
        ("thickness", json!(0.5)),
        ("targets", json!([])),
    ])
}

pub fn hinge() -> ToolDef {
    ToolDef {
        key: "hinge",
        label: "Living hinge",
        icon: "split",
        kind: "hinge",
        fields: vec![
            Field {
                key: "plane",
                label: "On",
                kind: FieldKind::Plane { any_face: false },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "x",
                label: "Position",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "slot",
                label: "Slot width",
                kind: FieldKind::Number { unit: Some("mm") },
                help: None,
                placeholder: None,
                show: None,
            },
            Field {
                key: "thickness",
                label: "Hinge thickness",
                kind: FieldKind::Number { unit: Some("mm") },
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
                show: None,
            },
        ],
        hint: Some("Cuts a thin slot across the whole block, leaving a thin strip of material under it (the hinge) so the two sides can flex. Position is measured along the plane’s X; 0.5 mm is a common hinge thickness for PLA"),
        extra: None,
        flags: ToolFlags::default(),
        defaults: defaults_hinge,
        to_params: to_params_primitive,
        from_params: from_params_primitive,
        on_change: None,
    }
}
