//! Replays `rust/tests/parity/tools/*.json` through `tools::defs` (M4b Task 1).

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{json, Value};
use stl_app::engine_link::{BodyView, PlaneView, SketchView, TimelineItem};
use stl_app::tools::defs::{tool, tools, Ctx, Field, FieldKind, ToolDef, Values};
use stl_kernel::MeshArrays;

const TOOL_KEYS: &[&str] = &[
    "extrude", "revolve", "loft", "sweep", "primitive_box", "primitive_cylinder", "primitive_sphere",
    "primitive_cone", "primitive_torus", "snapfit", "hinge", "hole", "combine", "split", "bedsplit", "plate",
    "layflat", "thread", "offset", "thincheck", "fittest", "resizehole", "pushpull", "mirror", "move", "scale",
    "pattern", "hollow", "lattice", "shell", "plane_offset", "plane_angle", "plane_3pt", "plane_mid", "import",
    "sculpt", "remesh", "repair", "simplify", "separate", "delete",
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
    let [Mx, My, Mz] = max;
    let positions = vec![
        mx, my, mz, Mx, my, mz, mx, My, mz, Mx, My, mz, mx, my, Mz, Mx, my, Mz, mx, My, Mz, Mx, My, Mz,
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
    }
}

fn fixture_data() -> (Vec<BodyView>, Vec<PlaneView>, Vec<SketchView>, Vec<TimelineItem>, Vec<String>) {
    let bodies = vec![
        BodyView {
            id: "body_a".into(),
            name: "Body A".into(),
            color: None,
            hidden: false,
            solid: true,
            mesh: Arc::new(box_mesh([0.0, 0.0, 0.0], [40.0, 20.0, 10.0])),
            revision: 0,
        },
        BodyView {
            id: "body_b".into(),
            name: "Body B".into(),
            color: None,
            hidden: false,
            solid: true,
            mesh: Arc::new(box_mesh([50.0, 0.0, 0.0], [70.0, 20.0, 10.0])),
            revision: 0,
        },
    ];
    let mut p0 = PlaneView::default();
    p0.id = "XY".into();
    p0.name = "Top".into();
    let mut p1 = PlaneView::default();
    p1.id = "XZ".into();
    p1.name = "Front".into();
    let mut p2 = PlaneView::default();
    p2.id = "YZ".into();
    p2.name = "Right".into();
    let planes = vec![p0, p1, p2];
    let mut sk = SketchView::default();
    sk.id = "sk1".into();
    sk.name = "Sketch 1".into();
    let sketches = vec![sk];
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
            bodies = vec![body("body_c", "Body C", true, MeshArrays { positions: vec![], triangles: vec![] })];
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
    World { bodies, planes, sketches, features, selection, last_sketch, sketch_on_body }
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
        if let Some(show) = f.show {
            if !show(values) {
                continue;
            }
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
            if let FieldKind::Select { options, .. } = &f.kind {
                if f.key == "preset" {
                    for (id, _, _) in options {
                        let mut values = defaults.clone();
                        values.insert("preset".into(), json!(id));
                        push_on_change(&mut out, def, on_change, &format!("onchange:preset={id}"), "preset", values, ctx);
                    }
                }
            }
        }
        let mut v = defaults.clone();
        v.insert("preset".into(), json!("ins_M3"));
        v.insert("tol".into(), json!(0.3));
        push_on_change(&mut out, def, on_change, "onchange:tol@ins_M3", "tol", v, ctx);
    } else if def.key == "thread" {
        for f in &def.fields {
            if let FieldKind::Select { options, .. } = &f.kind {
                if f.key == "size" {
                    for (id, _, _) in options {
                        let mut values = defaults.clone();
                        values.insert("size".into(), json!(id));
                        push_on_change(&mut out, def, on_change, &format!("onchange:size={id}"), "size", values, ctx);
                    }
                }
            }
        }
        for kind in ["internal", "external"] {
            let mut values = defaults.clone();
            values.insert("kind".into(), json!(kind));
            push_on_change(&mut out, def, on_change, &format!("onchange:kind={kind}"), "kind", values, ctx);
        }
    } else if def.key == "resizehole" {
        for row in stl_app::tools::defs::tables::RESIZE_SIZES {
            let id = row.0;
            let mut values = defaults.clone();
            values.insert("size".into(), json!(id));
            push_on_change(&mut out, def, on_change, &format!("onchange:size={id}"), "size", values, ctx);
        }
    } else {
        for f in &def.fields {
            if let FieldKind::Select { options, .. } = &f.kind {
                let watch_connector = f.key == "connector" || f.key == "printer";
                let generic = f.key != "connector"
                    && def.key != "split"
                    && def.key != "bedsplit"
                    && def.key != "plate";
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

#[test]
fn tools_golden_all_41() {
    let (bodies, planes, sketches, features, selection) = fixture_data();
    let ctx = fixture_ctx(&bodies, &planes, &sketches, &features, &selection);
    let keys: Vec<&str> = tools().iter().map(|t| t.key).collect();
    assert_eq!(keys, TOOL_KEYS, "tools() must list the tools in the order of TOOLS in tools.js");
    let registry: BTreeSet<String> = keys.iter().map(|k| k.to_string()).collect();
    assert_eq!(registry.len(), 41, "tools() should expose 41 tools");
    let mut failed = 0usize;
    for key in TOOL_KEYS {
        let golden_path = parity_dir().join(format!("{key}.json"));
        let golden_text = fs::read_to_string(&golden_path).unwrap_or_else(|_| panic!("missing golden {golden_path:?}"));
        let golden: Value = serde_json::from_str(&golden_text).unwrap();
        let def = tool(key).unwrap_or_else(|| panic!("tool {key} missing from registry"));
        let recorded = record_tool(def, &ctx, &golden["extra"]);
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
