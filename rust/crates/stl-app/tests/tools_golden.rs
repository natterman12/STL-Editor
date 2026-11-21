//! Replays `rust/tests/parity/tools/*.json` through `tools::defs` (M4b Task 1).

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{Value, json};
use stl_app::engine_link::{BodyView, PlaneView, SketchView, TimelineItem};
use stl_app::tools::defs::{Ctx, Field, FieldKind, ToolDef, Values, tool, tools};
use stl_kernel::MeshArrays;

const TOOL_KEYS: &[&str] = &[
    "extrude",
    "revolve",
    "loft",
    "sweep",
    "primitive_box",
    "primitive_cylinder",
    "primitive_sphere",
    "primitive_cone",
    "primitive_torus",
    "snapfit",
    "hinge",
    "hole",
    "combine",
    "split",
    "bedsplit",
    "plate",
    "layflat",
    "thread",
    "offset",
    "thincheck",
    "fittest",
    "resizehole",
    "pushpull",
    "mirror",
    "move",
    "scale",
    "pattern",
    "hollow",
    "lattice",
    "shell",
    "plane_offset",
    "plane_angle",
    "plane_3pt",
    "plane_mid",
    "import",
    "sculpt",
    "remesh",
    "repair",
    "simplify",
    "separate",
    "delete",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
}

fn parity_dir() -> PathBuf {
    repo_root().join("rust").join("tests").join("parity").join("tools")
}

fn box_mesh(min: [f32; 3], max: [f32; 3]) -> MeshArrays {
    let [mx, my, mz] = min;
    let [hx, hy, hz] = max;
    let positions = vec![
        mx, my, mz, hx, my, mz, mx, hy, mz, hx, hy, mz, mx, my, hz, hx, my, hz, mx, hy, hz, hx, hy, hz,
    ];
    let triangles = vec![
        0, 1, 2, 1, 3, 2, 4, 6, 5, 5, 6, 7, 0, 2, 4, 2, 6, 4, 1, 5, 3, 3, 5, 7, 0, 4, 1, 1, 4, 5, 2, 3, 6, 3, 7, 6,
    ];
    MeshArrays { positions, triangles }
}

fn fixture_ctx<'a>(
    bodies: &'a [BodyView],
    planes: &'a [PlaneView],
    sketches: &'a [SketchView],
    features: &'a [TimelineItem],
    selection: &'a [String],
) -> Ctx<'a> {
    Ctx {
        bodies,
        planes,
        sketches,
        features,
        selection,
        last_sketch: Some("sk1"),
        sketch_on_body: true,
        inch: false,
    }
}

fn fixture_data() -> (
    Vec<BodyView>,
    Vec<PlaneView>,
    Vec<SketchView>,
    Vec<TimelineItem>,
    Vec<String>,
) {
    let bodies = vec![
        BodyView {
            id: "body_a".into(),
            name: "Body A".into(),
            color: None,
            hidden: false,
            solid: true,
            mesh: Arc::new(box_mesh([0.0, 0.0, 0.0], [40.0, 20.0, 10.0])),
            revision: 0,
            holds_up: None,
        },
        BodyView {
            id: "body_b".into(),
            name: "Body B".into(),
            color: None,
            hidden: false,
            solid: true,
            mesh: Arc::new(box_mesh([50.0, 0.0, 0.0], [70.0, 20.0, 10.0])),
            revision: 0,
            holds_up: None,
        },
    ];
    let plane = |id: &str, name: &str| PlaneView {
        id: id.into(),
        name: name.into(),
        ..Default::default()
    };
    let planes = vec![plane("XY", "Top"), plane("XZ", "Front"), plane("YZ", "Right")];
    let sketches = vec![SketchView {
        id: "sk1".into(),
        name: "Sketch 1".into(),
        ..Default::default()
    }];
    let features = vec![
        TimelineItem {
            id: "f_import".into(),
            kind: "import".into(),
            label: String::new(),
            error: None,
            suppressed: false,
            warning: None,
            active: true,
            params: json!({}),
        },
        TimelineItem {
            id: "f_sketch".into(),
            kind: "sketch".into(),
            label: String::new(),
            error: None,
            suppressed: false,
            warning: None,
            active: true,
            params: json!({ "plane": { "body": "body_a", "origin": [0, 0, 10], "normal": [0, 0, 1] } }),
        },
        TimelineItem {
            id: "f_extrude".into(),
            kind: "extrude".into(),
            label: String::new(),
            error: None,
            suppressed: false,
            warning: None,
            active: true,
            params: json!({ "sketch": "sk1", "distance": 5, "operation": "cut" }),
        },
    ];
    let selection = vec!["body_a".into()];
    (bodies, planes, sketches, features, selection)
}

/// A project state the golden's `ctxCases` were recorded in (`CTX_VARIANTS` in tools_golden.mjs).
struct World {
    bodies: Vec<BodyView>,
    planes: Vec<PlaneView>,
    sketches: Vec<SketchView>,
    features: Vec<TimelineItem>,
    selection: Vec<String>,
    last_sketch: Option<&'static str>,
    sketch_on_body: bool,
}

impl World {
    fn ctx(&self) -> Ctx<'_> {
        Ctx {
            bodies: &self.bodies,
            planes: &self.planes,
            sketches: &self.sketches,
            features: &self.features,
            selection: &self.selection,
            last_sketch: self.last_sketch,
            sketch_on_body: self.sketch_on_body,
            inch: false,
        }
    }
}

fn body(id: &str, name: &str, solid: bool, mesh: MeshArrays) -> BodyView {
    BodyView {
        id: id.into(),
        name: name.into(),
        color: None,
        hidden: false,
        solid,
        mesh: Arc::new(mesh),
        revision: 0,
        holds_up: None,
    }
}

const VARIANTS: &[&str] = &["nosel", "two", "nonsolid", "nobbox", "empty"];

fn world(name: &str) -> World {
    let (mut bodies, planes, sketches, mut features, mut selection) = fixture_data();
    let mut last_sketch = Some("sk1");
    let mut sketch_on_body = true;
    let (mut planes, mut sketches) = (planes, sketches);
    match name {
        "fixture" => {}
        "nosel" => selection.clear(),
        "two" => selection = vec!["body_a".into(), "body_b".into()],
        "nonsolid" => {
            selection.clear();
            bodies.push(body("body_n", "Body N", false, box_mesh([0.0; 3], [5.0; 3])));
        }
        "nobbox" => {
            selection = vec!["body_c".into()];
            bodies = vec![body(
                "body_c",
                "Body C",
                true,
                MeshArrays {
                    positions: vec![],
                    triangles: vec![],
                },
            )];
            features.clear();
        }
        "empty" => {
            bodies.clear();
            planes.clear();
            sketches.clear();
            features.clear();
            selection.clear();
            last_sketch = None;
            sketch_on_body = false;
        }
        other => panic!("unknown ctx variant {other}"),
    }
    World {
        bodies,
        planes,
        sketches,
        features,
        selection,
        last_sketch,
        sketch_on_body,
    }
}

fn to_params_of(ctx: &Ctx, def: &ToolDef, values: &Values) -> Value {
    let v = values.clone();
    let p = (def.to_params)(&v, ctx);
    cleanup_params(p)
}

fn cleanup_params(mut p: Value) -> Value {
    let Some(obj) = p.as_object_mut() else {
        return p;
    };
    let keys: Vec<String> = obj.keys().cloned().collect();
    for k in keys {
        if k.starts_with('_') {
            obj.remove(&k);
        }
    }
    p
}

fn shown_keys(def: &ToolDef, values: &Values) -> Vec<String> {
    let mut out = Vec::new();
    for f in &def.fields {
        if let Some(show) = f.show
            && !show(values)
        {
            continue;
        }
        out.push(f.key.to_string());
    }
    out
}

fn flags_json(def: &ToolDef) -> Value {
    let gizmo = match def.key {
        "move" => json!("move"),
        "scale" => json!("scale"),
        _ if def.flags.gizmo => json!(true),
        _ => json!(false),
    };
    json!({
        "placeOnFace": def.flags.place_on_face,
        "noLivePreview": def.flags.no_live_preview,
        "gizmo": gizmo,
        "planeTool": def.flags.plane_tool,
        "sculpt": def.flags.sculpt,
        "paint": def.flags.paint,
        "wallTool": def.flags.wall_tool,
    })
}

fn field_meta(f: &Field) -> Value {
    let mut m = json!({
        "key": f.key,
        "label": f.label,
        "kind": field_kind_name(&f.kind),
    });
    let obj = m.as_object_mut().unwrap();
    // one arm per kind, so a kind with nothing to add doesn't fall through to another arm
    #[allow(clippy::collapsible_match)]
    match &f.kind {
        FieldKind::Number { unit } => {
            if let Some(u) = unit {
                obj.insert("unit".into(), json!(u));
            }
        }
        FieldKind::Vec3 { unit } => {
            if let Some(u) = unit {
                obj.insert("unit".into(), json!(u));
            }
        }
        FieldKind::Text { placeholder } => {
            if let Some(p) = placeholder {
                obj.insert("placeholder".into(), json!(p));
            }
        }
        FieldKind::Bodies { multi } => {
            obj.insert("multi".into(), json!(multi));
        }
        FieldKind::Plane { any_face } => {
            if *any_face {
                obj.insert("anyFace".into(), json!(true));
            }
        }
        FieldKind::PFace { any_face } => {
            if *any_face {
                obj.insert("anyFace".into(), json!(true));
            }
        }
        FieldKind::Select { options, grouped } => {
            let opts: Vec<Value> = if *grouped {
                options
                    .iter()
                    .map(|(id, label, g)| {
                        Value::Array(vec![json!(id), json!(label), json!(g.clone().unwrap_or_default())])
                    })
                    .collect()
            } else {
                options
                    .iter()
                    .map(|(id, label, _)| Value::Array(vec![json!(id), json!(label)]))
                    .collect()
            };
            obj.insert("options".into(), Value::Array(opts));
            if *grouped {
                obj.insert("groups".into(), json!(true));
            }
        }
        _ => {}
    }
    if let Some(h) = f.help {
        obj.insert("help".into(), json!(h));
    }
    if let Some(p) = f.placeholder {
        obj.insert("placeholder".into(), json!(p));
    }
    if f.show.is_some() {
        obj.insert("hasShow".into(), json!(true));
    }
    m
}

fn field_kind_name(k: &FieldKind) -> &'static str {
    match k {
        FieldKind::Number { .. } => "number",
        FieldKind::Select { .. } => "select",
        FieldKind::Bool => "bool",
        FieldKind::Vec3 { .. } => "vec3",
        FieldKind::Text { .. } => "text",
        FieldKind::Bodies { .. } => "bodies",
        FieldKind::Plane { .. } => "plane",
        FieldKind::Sketch => "sketch",
        FieldKind::Regions => "regions",
        FieldKind::Features => "features",
        FieldKind::FacePoints => "facepoints",
        FieldKind::CutPoints => "cutpoints",
        FieldKind::PFace { .. } => "pface",
        FieldKind::CylFace => "cylface",
        FieldKind::Points3 => "points3",
        FieldKind::AxisLine => "axisline",
    }
}

fn map_to_value(m: &Values) -> Value {
    Value::Object(m.clone())
}

fn record_tool(def: &ToolDef, ctx: &Ctx, golden_extra: &Value) -> Value {
    let defaults = (def.defaults)(ctx);
    let to_params_defaults = to_params_of(ctx, def, &defaults);
    let mut cases = Vec::new();
    for f in &def.fields {
        match &f.kind {
            FieldKind::Select { options, .. } => {
                for (id, _, _) in options {
                    let mut values = defaults.clone();
                    values.insert(f.key.to_string(), json!(id));
                    cases.push(json!({
                        "name": format!("select:{}={}", f.key, id),
                        "values": map_to_value(&values),
                        "toParams": to_params_of(ctx, def, &values),
                        "shown": shown_keys(def, &values),
                    }));
                }
            }
            FieldKind::Bool => {
                for bit in [true, false] {
                    let mut values = defaults.clone();
                    values.insert(f.key.to_string(), json!(bit));
                    cases.push(json!({
                        "name": format!("bool:{}={}", f.key, bit),
                        "values": map_to_value(&values),
                        "toParams": to_params_of(ctx, def, &values),
                        "shown": shown_keys(def, &values),
                    }));
                }
            }
            _ => {}
        }
    }
    let on_change = record_on_change(def, &defaults, ctx);
    let mut round_trips = Vec::new();
    let from0 = (def.from_params)(to_params_defaults.clone());
    round_trips.push(json!({
        "name": "defaults",
        "params": to_params_defaults,
        "fromParams": map_to_value(&from0),
        "toParams": to_params_of(ctx, def, &from0),
    }));
    // one trip per option of every select field
    for f in &def.fields {
        let FieldKind::Select { options, .. } = &f.kind else {
            continue;
        };
        for (id, _, _) in options {
            let mut values = defaults.clone();
            values.insert(f.key.to_string(), json!(id));
            let params = to_params_of(ctx, def, &values);
            let from = (def.from_params)(params.clone());
            round_trips.push(json!({
                "name": format!("select:{}={}", f.key, id),
                "params": params,
                "fromParams": map_to_value(&from),
                "toParams": to_params_of(ctx, def, &from),
            }));
        }
    }

    json!({
        "tool": def.key,
        "label": def.label,
        "type": def.kind,
        "icon": def.icon,
        "flags": flags_json(def),
        "hint": def.hint,
        "extra": def.extra,
        "fields": def.fields.iter().map(field_meta).collect::<Vec<_>>(),
        "ctxNote": "FIXTURE_CTX: selection [body_a], lastSketch sk1, sketchOnBody true, two bodies, one extrude cut",
        "defaults": map_to_value(&defaults),
        "toParams_defaults": to_params_defaults,
        "shown_defaults": shown_keys(def, &defaults),
        "cases": cases,
        "onChange": on_change,
        "roundTrips": round_trips,
        "ctxCases": ctx_cases(def),
        "extra": replay_extra(def, ctx, golden_extra),
    })
}

/// Defaults under the other project states.
fn ctx_cases(def: &ToolDef) -> Value {
    let mut out = serde_json::Map::new();
    for name in VARIANTS {
        let w = world(name);
        let ctx = w.ctx();
        let defaults = (def.defaults)(&ctx);
        out.insert(
            (*name).to_string(),
            json!({
                "defaults": map_to_value(&defaults),
                "toParams_defaults": to_params_of(&ctx, def, &defaults),
                "shown_defaults": shown_keys(def, &defaults),
            }),
        );
    }
    Value::Object(out)
}

/// Replays the golden's hand-picked `extra` inputs (toParams / fromParams / onChange).
fn replay_extra(def: &ToolDef, ctx: &Ctx, golden_extra: &Value) -> Value {
    let mut out = Vec::new();
    for e in golden_extra.as_array().cloned().unwrap_or_default() {
        let op = e["op"].as_str().unwrap();
        let input = &e["input"];
        let result = match op {
            "toParams" => to_params_of(ctx, def, input.as_object().unwrap()),
            "fromParams" => map_to_value(&(def.from_params)(input.clone())),
            "onChange" => {
                let f = def.on_change.unwrap_or_else(|| panic!("{} has no on_change", def.key));
                let values = input["values"].as_object().unwrap();
                f(input["trigger"].as_str().unwrap(), values, ctx).map_or(Value::Null, |m| map_to_value(&m))
            }
            other => panic!("unknown extra op {other}"),
        };
        out.push(json!({ "name": e["name"], "op": op, "input": input, "result": result }));
    }
    Value::Array(out)
}

fn record_on_change(def: &ToolDef, defaults: &Values, ctx: &Ctx) -> Vec<Value> {
    let Some(on_change) = def.on_change else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if def.key == "hole" {
        for f in &def.fields {
            if let FieldKind::Select { options, .. } = &f.kind
                && f.key == "preset"
            {
                for (id, _, _) in options {
                    let mut values = defaults.clone();
                    values.insert("preset".into(), json!(id));
                    push_on_change(
                        &mut out,
                        def,
                        on_change,
                        &format!("onchange:preset={id}"),
                        "preset",
                        values,
                        ctx,
                    );
                }
            }
        }
        let mut v = defaults.clone();
        v.insert("preset".into(), json!("ins_M3"));
        v.insert("tol".into(), json!(0.3));
        push_on_change(&mut out, def, on_change, "onchange:tol@ins_M3", "tol", v, ctx);
    } else if def.key == "thread" {
        for f in &def.fields {
            if let FieldKind::Select { options, .. } = &f.kind
                && f.key == "size"
            {
                for (id, _, _) in options {
                    let mut values = defaults.clone();
                    values.insert("size".into(), json!(id));
                    push_on_change(
                        &mut out,
                        def,
                        on_change,
                        &format!("onchange:size={id}"),
                        "size",
                        values,
                        ctx,
                    );
                }
            }
        }
        for kind in ["internal", "external"] {
            let mut values = defaults.clone();
            values.insert("kind".into(), json!(kind));
            push_on_change(
                &mut out,
                def,
                on_change,
                &format!("onchange:kind={kind}"),
                "kind",
                values,
                ctx,
            );
        }
    } else if def.key == "resizehole" {
        for row in stl_app::tools::defs::tables::RESIZE_SIZES {
            let id = row.0;
            let mut values = defaults.clone();
            values.insert("size".into(), json!(id));
            push_on_change(
                &mut out,
                def,
                on_change,
                &format!("onchange:size={id}"),
                "size",
                values,
                ctx,
            );
        }
    } else {
        for f in &def.fields {
            if let FieldKind::Select { options, .. } = &f.kind {
                let watch_connector = f.key == "connector" || f.key == "printer";
                let generic = f.key != "connector" && def.key != "split" && def.key != "bedsplit" && def.key != "plate";
                for (id, _, _) in options {
                    let mut before = defaults.clone();
                    before.insert(f.key.to_string(), json!(id));
                    let patch = on_change(f.key, &before, ctx);
                    if generic {
                        if patch.is_none() {
                            continue;
                        }
                        let mut after = before.clone();
                        if let Some(p) = &patch {
                            for (k, v) in p {
                                after.insert(k.clone(), v.clone());
                            }
                        }
                        out.push(json!({
                            "name": format!("onchange:{}={}", f.key, id),
                            "trigger": f.key,
                            "values_before": map_to_value(&before),
                            "patch": patch.as_ref().map(map_to_value),
                            "values_after": map_to_value(&after),
                            "toParams": to_params_of(ctx, def, &after),
                            "shown": shown_keys(def, &after),
                        }));
                        continue;
                    }
                    if watch_connector {
                        push_on_change(
                            &mut out,
                            def,
                            on_change,
                            &format!("onchange:{}={}", f.key, id),
                            f.key,
                            before,
                            ctx,
                        );
                    }
                }
            }
        }
    }
    out
}

fn push_on_change(
    out: &mut Vec<Value>,
    def: &ToolDef,
    on_change: fn(&str, &Values, &Ctx) -> Option<Values>,
    name: &str,
    trigger: &str,
    before: Values,
    ctx: &Ctx,
) {
    let patch = on_change(trigger, &before, ctx);
    let mut after = before.clone();
    if let Some(p) = &patch {
        for (k, v) in p {
            after.insert(k.clone(), v.clone());
        }
    }
    out.push(json!({
        "name": name,
        "trigger": trigger,
        "values_before": map_to_value(&before),
        "patch": patch.as_ref().map(map_to_value),
        "values_after": map_to_value(&after),
        "toParams": to_params_of(ctx, def, &after),
        "shown": shown_keys(def, &after),
    }));
}

fn values_eq_tool(_key: &str, recorded: &Value, golden: &Value) -> bool {
    recorded == golden
}

/// Split's one deliberate difference from 1.x (milestone 6, spec 2.4; `EXCEPTIONS.md`): the
/// Distance field, key `offset`, right after Split plane. Takes it out of a recorded definition so
/// the rest of Split is still compared with 1.x field for field.
fn without_split_distance(v: &mut Value) {
    match v {
        Value::Object(o) => {
            o.remove("offset");
            if let Some(Value::Array(fields)) = o.get_mut("fields") {
                fields.retain(|f| f["key"] != "offset");
            }
            o.values_mut().for_each(without_split_distance);
        }
        Value::Array(a) => {
            a.retain(|x| x != "offset");
            a.iter_mut().for_each(without_split_distance);
        }
        _ => {}
    }
}

/// Thread's screw head pocket is 2.0 only (Nathan 2026-11-19; `EXCEPTIONS.md`): the `head`
/// choice, its three size fields and the cases recorded for the choice are taken out, and what is
/// left is compared with 1.x. `thread_head_pocket_is_the_one_difference` checks what was taken out.
fn without_thread_head(v: &mut Value) {
    let is_head = |s: &str| s == "head" || s.starts_with("head_");
    match v {
        Value::Object(o) => {
            o.retain(|k, _| !is_head(k));
            if let Some(Value::Array(fields)) = o.get_mut("fields") {
                fields.retain(|f| !f["key"].as_str().is_some_and(is_head));
            }
            o.values_mut().for_each(without_thread_head);
        }
        Value::Array(a) => {
            a.retain(|x| {
                !x.as_str().is_some_and(is_head) && !x["name"].as_str().is_some_and(|n| n.starts_with("select:head="))
            });
            a.iter_mut().for_each(without_thread_head);
        }
        _ => {}
    }
}

/// 2.0 split connectors (dovetails, Placement, How many, spots placed by hand, "Each piece its
/// own body"; `EXCEPTIONS.md`): their fields, keys, the Dovetail choice and the cases recorded for
/// it are taken out, and what is left is compared with 1.x. `split_connectors_are_the_one_difference`
/// checks what was taken out.
fn without_connector_extras(v: &mut Value) {
    const KEYS: &[&str] = &[
        "connector_place",
        "connector_count",
        "connector_points",
        "dovetail_width",
        "dovetail_depth",
        "dovetail_slide",
        "separate",
    ];
    let new_key = |s: &str| KEYS.contains(&s);
    match v {
        Value::Object(o) => {
            o.retain(|k, _| !new_key(k));
            if let Some(Value::Array(fields)) = o.get_mut("fields") {
                fields.retain(|f| !f["key"].as_str().is_some_and(new_key));
            }
            o.values_mut().for_each(without_connector_extras);
        }
        Value::Array(a) => {
            a.retain(|x| {
                !x.as_str().is_some_and(new_key)
                    && !x["name"].as_str().is_some_and(|n| {
                        // "select:connector=dovetail", "bool:separate=true", ...
                        let k = n.split_once(':').map_or(n, |(_, r)| r);
                        n.ends_with("=dovetail") || new_key(k.split_once('=').map_or(k, |(k, _)| k))
                    })
                    && x.get(0).and_then(Value::as_str) != Some("dovetail")
            });
            a.iter_mut().for_each(without_connector_extras);
        }
        _ => {}
    }
}

#[test]
fn split_connectors_are_the_one_difference() {
    let (bodies, planes, sketches, features, selection) = fixture_data();
    let ctx = fixture_ctx(&bodies, &planes, &sketches, &features, &selection);
    for key in ["split", "bedsplit"] {
        let def = tool(key).unwrap();
        // a 1.x step opens with the new settings at their defaults and is saved as 1.x saved it
        let old = json!({"body": "body_a", "plane": "XY", "keep": "both", "connector": "pins",
                         "pin_d": 5, "pin_len": 10, "clearance": 0.15});
        let v = (def.from_params)(old.clone());
        assert_eq!(v["connector_place"], json!("auto"), "{key}");
        assert_eq!(v["dovetail_width"], json!(10), "{key}");
        let p = (def.to_params)(&v, &ctx);
        let mut want = old.clone();
        if key == "bedsplit" {
            want["bed"] = json!(null);
            want.as_object_mut().unwrap().remove("bed");
        }
        for (k, w) in want.as_object().unwrap() {
            assert_eq!(p.get(k), Some(w), "{key}: {k} in {p}");
        }
        for k in [
            "connector_place",
            "connector_count",
            "connector_points",
            "dovetail_width",
            "separate",
        ] {
            assert!(p.get(k).is_none(), "{key}: a 1.x step gains no {k}: {p}");
        }
        // Dovetail shows its own sizes in place of the pin ones
        let mut v = (def.defaults)(&ctx);
        v.insert("connector".into(), json!("dovetail"));
        let shown = shown_keys(def, &v);
        for k in [
            "dovetail_width",
            "dovetail_depth",
            "dovetail_slide",
            "clearance",
            "connector_count",
        ] {
            assert!(shown.iter().any(|s| s == k), "{key}: {k} shows: {shown:?}");
        }
        assert!(
            !shown.iter().any(|s| s == "pin_d" || s == "pin_len"),
            "{key}: {shown:?}"
        );
        let p = (def.to_params)(&v, &ctx);
        assert_eq!(
            (&p["dovetail_width"], &p["dovetail_depth"]),
            (&json!(10), &json!(5)),
            "{key}"
        );
        assert!(p.get("dovetail_slide").is_none(), "the long side is the default: {p}");
    }
    // Split: Place by hand swaps How many for the spots; a new Split separates pieces
    let def = tool("split").unwrap();
    let mut v = (def.defaults)(&ctx);
    assert_eq!(v["separate"], json!(true));
    v.insert("connector".into(), json!("pins"));
    v.insert("connector_place".into(), json!("manual"));
    v.insert("connector_points".into(), json!([[1, 2, 3]]));
    let shown = shown_keys(def, &v);
    assert!(shown.iter().any(|s| s == "connector_points") && !shown.iter().any(|s| s == "connector_count"));
    let p = (def.to_params)(&v, &ctx);
    assert_eq!(
        (&p["connector_place"], &p["connector_points"], &p["separate"]),
        (&json!("manual"), &json!([[1, 2, 3]]), &json!(true))
    );
    let back = (def.from_params)(p.clone());
    assert_eq!((def.to_params)(&back, &ctx), p, "opens again as saved");
    // a 1.x Split keeps each half whole
    assert_eq!((def.from_params)(json!({"body": "body_a"}))["separate"], json!(false));
}

#[test]
fn thread_head_pocket_is_the_one_difference() {
    let (bodies, planes, sketches, features, selection) = fixture_data();
    let ctx = fixture_ctx(&bodies, &planes, &sketches, &features, &selection);
    let def = tool("thread").unwrap();
    let keys: Vec<&str> = def.fields.iter().map(|f| f.key).collect();
    assert_eq!(
        &keys[..6],
        ["kind", "size", "head", "head_diameter", "head_depth", "head_angle"],
        "Screw head sits right under Size"
    );
    let shown = |v: &Values| -> Vec<String> {
        shown_keys(def, v)
            .into_iter()
            .filter(|k| k.starts_with("head"))
            .collect()
    };
    let on_change = def.on_change.unwrap();
    let mut v = (def.defaults)(&ctx);

    // flush is the default: one row, and the step is stored exactly as 1.x stores it
    assert_eq!(v["head"], json!("flush"));
    assert_eq!(shown(&v), ["head"]);
    let p = (def.to_params)(&v, &ctx);
    assert!(p.as_object().unwrap().keys().all(|k| !k.starts_with("head")), "{p}");
    assert_eq!(on_change("head", &v, &ctx), None);

    // M8 socket head cap screw: 13 mm head + 0.5 + the 0.3 clearance, 8 mm high + 0.3
    v.insert("head".into(), json!("counterbore"));
    assert_eq!(shown(&v), ["head", "head_diameter", "head_depth"]);
    let patch = on_change("head", &v, &ctx).unwrap();
    assert_eq!(
        Value::Object(patch.clone()),
        json!({"head_diameter": 13.8, "head_depth": 8.3})
    );
    v.extend(patch);
    let p = (def.to_params)(&v, &ctx);
    assert_eq!(
        (&p["head"], &p["head_diameter"], &p["head_depth"], p.get("head_angle")),
        (&json!("counterbore"), &json!(13.8), &json!(8.3), None)
    );
    // the step opens again as it was saved
    let back = (def.from_params)(p.clone());
    assert_eq!((def.to_params)(&back, &ctx), p);

    // M8 countersunk: 16 mm head + 0.5 + 0.3, 90°
    v.insert("head".into(), json!("countersink"));
    assert_eq!(shown(&v), ["head", "head_diameter", "head_angle"]);
    let patch = on_change("head", &v, &ctx).unwrap();
    assert_eq!(
        Value::Object(patch.clone()),
        json!({"head_diameter": 16.8, "head_angle": 90})
    );
    v.extend(patch);
    assert!((def.to_params)(&v, &ctx).get("head_depth").is_none());

    // another size re-sizes the pocket with it: 1/4" flat head is 0.531" across, 82°
    v.insert("size".into(), json!("1/4\"-20"));
    let patch = on_change("size", &v, &ctx).unwrap();
    assert_eq!(patch["diameter"], json!(6.35));
    assert_eq!(patch["head_diameter"], json!(14.29)); // 13.4874 + 0.5 + 0.3
    assert_eq!(patch["head_angle"], json!(82));
    v.extend(patch);

    // more print clearance widens the pocket by the same amount
    v.insert("tolerance".into(), json!(0.5));
    assert_eq!(on_change("tolerance", &v, &ctx).unwrap()["head_diameter"], json!(14.49));

    // a screw has no pocket: no rows, nothing stored
    v.insert("kind".into(), json!("external"));
    assert!(shown(&v).is_empty());
    let p = (def.to_params)(&v, &ctx);
    assert!(p.as_object().unwrap().keys().all(|k| !k.starts_with("head")), "{p}");

    // a 1.x step has no head and opens as flush
    let old = (def.from_params)(json!({"kind": "internal", "diameter": 8, "pitch": 1.25, "length": 10}));
    assert_eq!(old["head"], json!("flush"));
}

/// Living Hinge's hint (milestone 7, spec 2.13; `EXCEPTIONS.md`): two numbered steps, then what
/// the slot does and what the orange line and the arrow mean. Everything else is compared with 1.x.
#[test]
fn hinge_hint_says_what_to_pick() {
    let hint = tool("hinge").unwrap().hint.unwrap();
    assert!(
        hint.starts_with(
            "1. Pick the face or plane the hinge runs across.\n2. Set where it sits and how thin it is.\n"
        ),
        "{hint}"
    );
    assert!(
        hint.contains("The orange line is the hinge; the arrow shows which way it flexes."),
        "{hint}"
    );
}

#[test]
fn split_distance_is_the_one_difference() {
    let (bodies, planes, sketches, features, selection) = fixture_data();
    let ctx = fixture_ctx(&bodies, &planes, &sketches, &features, &selection);
    let def = tool("split").unwrap();
    let keys: Vec<&str> = def.fields.iter().map(|f| f.key).collect();
    assert_eq!(
        &keys[..4],
        ["body", "plane", "offset", "keep"],
        "Distance sits right after Split plane"
    );
    let f = &def.fields[2];
    assert_eq!(f.label, "Distance");
    assert!(matches!(f.kind, FieldKind::Number { unit: Some("mm") }));
    let d = def.flags.drag.expect("Split shows the drag-the-plane control");
    assert_eq!((d.plane, d.distance, d.body), ("plane", "offset", "body"));
    let mut v = (def.defaults)(&ctx);
    assert_eq!(v.get("offset"), Some(&json!(0)));
    assert!(
        (def.to_params)(&v, &ctx).get("offset").is_none(),
        "a Distance of 0 is not stored"
    );
    v.insert("offset".into(), json!(-5));
    assert_eq!((def.to_params)(&v, &ctx)["offset"], json!(-5));
    v.insert("offset".into(), json!("=h/2"));
    assert_eq!((def.to_params)(&v, &ctx)["offset"], json!("=h/2"));
    // a 1.x step has no Distance and comes back as 0
    let from = (def.from_params)(json!({"body": "body_a", "plane": "XY", "keep": "both"}));
    assert_eq!(from.get("offset"), Some(&json!(0)));
}

#[test]
fn tools_golden_all_41() {
    let (bodies, planes, sketches, features, selection) = fixture_data();
    let ctx = fixture_ctx(&bodies, &planes, &sketches, &features, &selection);
    let keys: Vec<&str> = tools().iter().map(|t| t.key).collect();
    assert_eq!(
        keys, TOOL_KEYS,
        "tools() must list the tools in the order of TOOLS in tools.js"
    );
    let registry: BTreeSet<String> = keys.iter().map(|k| k.to_string()).collect();
    assert_eq!(registry.len(), 41, "tools() should expose 41 tools");
    let mut failed = 0usize;
    for key in TOOL_KEYS {
        let golden_path = parity_dir().join(format!("{key}.json"));
        let golden_text = fs::read_to_string(&golden_path).unwrap_or_else(|_| panic!("missing golden {golden_path:?}"));
        let golden: Value = serde_json::from_str(&golden_text).unwrap();
        let def = tool(key).unwrap_or_else(|| panic!("tool {key} missing from registry"));
        let mut recorded = record_tool(def, &ctx, &golden["extra"]);
        if *key == "split" {
            without_split_distance(&mut recorded);
        }
        if *key == "thread" {
            without_thread_head(&mut recorded);
        }
        if *key == "split" || *key == "bedsplit" {
            without_connector_extras(&mut recorded);
        }
        if *key == "hinge" {
            // milestone 7 (spec 2.13): the hint is rewritten; checked in `hinge_hint_says_what_to_pick`
            recorded["hint"] = golden["hint"].clone();
        }
        if !values_eq_tool(key, &recorded, &golden) {
            eprintln!("MISMATCH {key}");
            if let (Some(ro), Some(go)) = (recorded.as_object(), golden.as_object()) {
                for k in go.keys() {
                    if ro.get(k) != go.get(k) {
                        eprintln!("  diff key: {k}");
                    }
                }
            }
            failed += 1;
        }
    }
    assert_eq!(failed, 0, "{failed} tool(s) failed golden compare");
}
