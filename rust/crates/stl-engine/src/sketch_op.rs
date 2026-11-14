//! `sketch_op` route (`web/engine/routes.js` lines 100–142).
//!
//! Trim, offset, mirror, explode, project, refs, text and faces. New ids are `e<seed><n>`.
//! Project and refs read this document's solid, visible bodies.

use std::collections::HashSet;

use serde_json::{json, Map, Value};
use stl_kernel::{Frame, Shape2d};
use stl_mesh::{outline, outline_entities, ref_geometry, OutlineEntity, RefEntity};
use stl_sketch::entities::{parse_entities, Circle, Entity, Polyline};
use stl_sketch::ops::{explode, mirror, offset, trim};
use stl_sketch::profile::faces;
use stl_sketch::text::{text_contours, TextOpts};

use crate::{Document, FeatureError};

/// 1.x `GeometryError`. The engine spells it [`FeatureError`]; a type alias is not a constructor.
pub type GeometryError = FeatureError;

const NO_SOLIDS: &str = "There are no solid bodies to project.";
const NO_TEXT: &str = "Type some text first.";
const EMPTY_OUTLINE: &str = "The outline came out empty.";

impl Document {
    /// 1.x `ROUTES.sketch_op`: `{op, ...data}` in, the route's JSON out.
    pub fn sketch_op(&self, op: &str, data: &Value) -> Result<Value, GeometryError> {
        let entities = entities_of(data)?;
        let seed = seed_of(data);
        match op {
            "trim" => {
                let point = point_of(data)?;
                let id = data.get("id").and_then(Value::as_str).unwrap_or("");
                let out = trim(&entities, id, point).map_err(sketch_err)?;
                // Arc and circle pieces are built as `{..., construction: !!prim.construction}`
                // in sketch2d.js, so a missing flag becomes false. Line pieces spread the
                // original entity and keep a missing flag missing.
                ok_entities(&stamp_trim_arcs(&entities, out))
            }
            "offset" => {
                let ids = ids_of(data);
                let distance = num_nullish(data, "distance", 1.0);
                let mut fresh = id_factory(&entities, seed);
                let mut out = offset(&entities, &ids, distance, &mut fresh).map_err(sketch_err)?;
                stamp_offset_shapes(&entities, &mut out);
                ok_entities(&out)
            }
            "mirror" => {
                let ids = ids_of(data);
                let axis = data.get("axis").and_then(Value::as_str).unwrap_or("");
                let mut fresh = id_factory(&entities, seed);
                let out = mirror(&entities, &ids, axis, &mut fresh).map_err(sketch_err)?;
                ok_entities(&out)
            }
            "explode" => ok_entities(&explode_like_js(&entities, &ids_of(data))),
            "project" => self.op_project(&entities, data, seed),
            "refs" => self.op_refs(data),
            "text" => op_text(&entities, data, seed),
            "faces" => Ok(op_faces(&entities)),
            _ => Err(FeatureError(format!("Unknown sketch operation {op}."))),
        }
    }
}

fn sketch_err(e: stl_sketch::SketchError) -> GeometryError {
    FeatureError(e.to_string())
}

fn entities_of(data: &Value) -> Result<Vec<Entity>, GeometryError> {
    match data.get("entities") {
        Some(Value::Array(list)) => parse_entities(list).map_err(FeatureError),
        _ => Ok(Vec::new()),
    }
}

/// `d.seed || 'x'`.
fn seed_of(data: &Value) -> &str {
    match data.get("seed").and_then(Value::as_str) {
        Some(s) if !s.is_empty() => s,
        _ => "x",
    }
}

/// `+(d[key] ?? default)`. A missing or null value takes the default; `0` stays `0`.
fn num_nullish(data: &Value, key: &str, default: f64) -> f64 {
    match data.get(key) {
        None | Some(Value::Null) => default,
        Some(v) => v.as_f64().unwrap_or(default),
    }
}

/// `+(d[key] || default)`: `0`, `""` and a missing value take the default.
fn num_or(data: &Value, key: &str, default: f64) -> f64 {
    match data.get(key) {
        None | Some(Value::Null) | Some(Value::Bool(false)) => default,
        Some(Value::String(s)) if s.is_empty() => default,
        Some(Value::Number(n)) => {
            let x = n.as_f64().unwrap_or(default);
            if x == 0.0 { default } else { x }
        }
        _ => default,
    }
}

fn ids_of(data: &Value) -> Vec<String> {
    data.get("ids")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

fn point_of(data: &Value) -> Result<[f64; 2], GeometryError> {
    let Some(a) = data.get("point").and_then(Value::as_array) else {
        return Err(FeatureError("Nothing to trim there.".into()));
    };
    let x = a.first().and_then(Value::as_f64).unwrap_or(0.0);
    let y = a.get(1).and_then(Value::as_f64).unwrap_or(0.0);
    Ok([x, y])
}

/// routes.js `newId`: `e<seed><n>`, skipping ids already on the sketch.
fn id_factory(entities: &[Entity], seed: &str) -> impl FnMut() -> String {
    let mut used: HashSet<String> = entities.iter().filter_map(|e| e.id().map(str::to_string)).collect();
    let seed = seed.to_string();
    let mut n = 1u32;
    move || loop {
        let id = format!("e{seed}{n}");
        n += 1;
        if used.insert(id.clone()) {
            return id;
        }
    }
}

fn ok_entities(entities: &[Entity]) -> Result<Value, GeometryError> {
    Ok(json!({
        "ok": true,
        "entities": serde_json::to_value(entities).map_err(|e| FeatureError(e.to_string()))?,
    }))
}

fn construction_slot(e: &mut Entity) -> &mut Option<bool> {
    match e {
        Entity::Line(x) => &mut x.construction,
        Entity::Arc(x) => &mut x.construction,
        Entity::Circle(x) => &mut x.construction,
        Entity::Rect(x) => &mut x.construction,
        Entity::Polygon(x) => &mut x.construction,
        Entity::Slot(x) => &mut x.construction,
        Entity::Polyline(x) => &mut x.construction,
        Entity::Point(x) => &mut x.construction,
    }
}

/// JS explode writes `construction: false` on every piece of a compound shape.
fn explode_like_js(entities: &[Entity], ids: &[String]) -> Vec<Entity> {
    let mut out = Vec::new();
    for e in entities {
        let hit = e.id().is_some_and(|id| ids.iter().any(|i| i == id));
        if !hit {
            out.push(e.clone());
            continue;
        }
        let passthrough = matches!(e, Entity::Line(_) | Entity::Arc(_) | Entity::Circle(_));
        for mut part in explode(e) {
            if !passthrough {
                let flag = part.construction();
                *construction_slot(&mut part) = Some(flag);
            }
            out.push(part);
        }
    }
    out
}

/// JS offset copies of rects, circles, arcs, polygons and slots carry `construction`.
/// Open chains become polylines without that key.
fn stamp_offset_shapes(original: &[Entity], out: &mut [Entity]) {
    let had: HashSet<&str> = original.iter().filter_map(Entity::id).collect();
    for e in out.iter_mut() {
        let fresh = e.id().is_none_or(|id| !had.contains(id));
        if fresh && !matches!(e, Entity::Polyline(_)) {
            let flag = e.construction();
            *construction_slot(e) = Some(flag);
        }
    }
}

/// New arc pieces from a trim (`id + "a"` / `"b"`) carry an explicit construction flag in 1.x.
fn stamp_trim_arcs(original: &[Entity], out: Vec<Entity>) -> Vec<Entity> {
    let had: HashSet<&str> = original.iter().filter_map(Entity::id).collect();
    out.into_iter()
        .map(|e| match e {
            Entity::Arc(mut a) if a.id.as_deref().is_none_or(|id| !had.contains(id)) => {
                if a.construction.is_none() {
                    a.construction = Some(false);
                }
                Entity::Arc(a)
            }
            other => other,
        })
        .collect()
}

fn hidden_ids(doc: &Document) -> HashSet<String> {
    doc.ui
        .get("hidden")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

fn frame_of(data: &Value) -> Result<Frame, GeometryError> {
    let Some(a) = data.get("frame").and_then(Value::as_array) else {
        return Err(FeatureError("The sketch plane is missing.".into()));
    };
    if a.len() < 4 {
        return Err(FeatureError("The sketch plane is missing.".into()));
    }
    let v3 = |v: &Value| -> Result<[f64; 3], GeometryError> {
        let Some(p) = v.as_array() else {
            return Err(FeatureError("The sketch plane is missing.".into()));
        };
        if p.len() < 3 {
            return Err(FeatureError("The sketch plane is missing.".into()));
        }
        Ok([
            p[0].as_f64().unwrap_or(0.0),
            p[1].as_f64().unwrap_or(0.0),
            p[2].as_f64().unwrap_or(0.0),
        ])
    };
    Ok(Frame { o: v3(&a[0])?, x: v3(&a[1])?, y: v3(&a[2])?, n: v3(&a[3])? })
}

impl Document {
    fn op_project(&self, entities: &[Entity], data: &Value, seed: &str) -> Result<Value, GeometryError> {
        let solids = self.project_solids(data)?;
        let frame = frame_of(data)?;
        let mode = data.get("mode").and_then(Value::as_str).filter(|s| !s.is_empty()).unwrap_or("section");
        let tolerance = num_nullish(data, "tolerance", 0.02);
        let off = num_or(data, "offset", 0.0);
        let construction = data.get("construction").and_then(Value::as_bool).unwrap_or(false);
        let polys = if off.abs() <= 1e-9 {
            outline(solids.as_slice(), &frame, mode, tolerance).map_err(FeatureError)?
        } else {
            // 1.x offsets the section, then simplifies. `outline` simplifies inside, so a
            // non-zero offset asks for the raw section (tolerance 0) and offsets that.
            let raw = outline(solids.as_slice(), &frame, mode, 0.0).map_err(FeatureError)?;
            let mut shape = Shape2d { contours: raw };
            // 1.x `cs.offset(+offset, 'Round', 2, 64)`.
            shape = shape.offset_round_segments(off, 64);
            if tolerance > 0.0 {
                shape = shape.simplify(tolerance);
            }
            let polys: Vec<Vec<[f64; 2]>> = shape.contours.into_iter().filter(|p| p.len() >= 3).collect();
            if polys.is_empty() {
                return Err(FeatureError(EMPTY_OUTLINE.into()));
            }
            polys
        };
        let outlined = outline_entities(&polys, || 0, construction);
        let mut fresh = id_factory(entities, seed);
        let add: Vec<Entity> = outlined.into_iter().map(|o| outline_entity(o, fresh())).collect();
        let added: Vec<String> = add.iter().filter_map(|e| e.id().map(str::to_string)).collect();
        let mut all = entities.to_vec();
        all.extend(add);
        Ok(json!({
            "ok": true,
            "entities": serde_json::to_value(&all).map_err(|e| FeatureError(e.to_string()))?,
            "added": added,
        }))
    }

    /// Bodies named in `data.bodies`, or every visible solid.
    fn project_solids(&self, data: &Value) -> Result<Vec<&stl_kernel::Solid>, GeometryError> {
        let wanted: Option<Vec<&str>> = data.get("bodies").and_then(Value::as_array).filter(|a| !a.is_empty()).map(|a| {
            a.iter().filter_map(Value::as_str).collect()
        });
        let mut solids = Vec::new();
        if let Some(ids) = wanted {
            for id in ids {
                if !self.state.bodies.contains_key(id) {
                    continue;
                }
                let body = self.state.body(id)?;
                solids.push(body.require_solid()?);
            }
        } else {
            let hidden = hidden_ids(self);
            for (id, body) in &self.state.bodies {
                if hidden.contains(id) {
                    continue;
                }
                if let Some(solid) = &body.solid {
                    solids.push(solid);
                }
            }
        }
        if solids.is_empty() {
            return Err(FeatureError(NO_SOLIDS.into()));
        }
        Ok(solids)
    }

    fn op_refs(&self, data: &Value) -> Result<Value, GeometryError> {
        let frame = frame_of(data)?;
        let hidden = hidden_ids(self);
        let mut bodies = Vec::new();
        for (id, body) in &self.state.bodies {
            if hidden.contains(id) {
                continue;
            }
            if let Some(solid) = &body.solid {
                bodies.push((id.as_str(), solid));
            }
        }
        if bodies.is_empty() {
            return Ok(json!({"ok": true, "kind": null, "refs": []}));
        }
        let geo = ref_geometry(&bodies, &frame);
        let refs: Vec<Value> = geo.entities.iter().map(ref_json).collect();
        let kind = geo.kind.map(Value::String).unwrap_or(Value::Null);
        Ok(json!({"ok": true, "kind": kind, "refs": refs}))
    }
}

fn outline_entity(o: OutlineEntity, id: String) -> Entity {
    let mut extra = Map::new();
    extra.insert("projected".into(), Value::Bool(true));
    match o {
        OutlineEntity::Circle { cx, cy, r, construction, .. } => Entity::Circle(Circle {
            id: Some(id),
            cx,
            cy,
            r,
            construction: construction.then_some(true),
            extra,
        }),
        OutlineEntity::Polyline { pts, closed, construction, .. } => Entity::Polyline(Polyline {
            id: Some(id),
            pts,
            closed: Some(closed),
            construction: construction.then_some(true),
            extra,
        }),
    }
}

fn ref_json(e: &RefEntity) -> Value {
    match e {
        RefEntity::Line { p1, p2 } => json!({"type": "line", "p1": p1, "p2": p2}),
        RefEntity::Circle { cx, cy, r } => json!({"type": "circle", "cx": cx, "cy": cy, "r": r}),
    }
}

fn op_faces(entities: &[Entity]) -> Value {
    let found = faces(entities, None);
    let list: Vec<Value> = found
        .iter()
        .map(|f| {
            let mut rings = vec![close_ring(&f.exterior)];
            rings.extend(f.holes.iter().map(|h| close_ring(h)));
            Value::Array(rings)
        })
        .collect();
    json!({"ok": true, "faces": list})
}

fn close_ring(ring: &[[f64; 2]]) -> Value {
    // stl-sketch faces follow GEOS: outer rings run clockwise, holes counter-clockwise,
    // and the ring is not closed. routes.js / sketch2d.js keep the face on the left
    // (outer counter-clockwise, holes clockwise) and repeat the first point. Reverse
    // the open ring, keeping its start vertex, then close it.
    let Some(first) = ring.first() else {
        return Value::Array(Vec::new());
    };
    let mut pts = Vec::with_capacity(ring.len() + 1);
    pts.push(json!([first[0], first[1]]));
    for p in ring.iter().skip(1).rev() {
        pts.push(json!([p[0], p[1]]));
    }
    pts.push(json!([first[0], first[1]]));
    Value::Array(pts)
}

fn op_text(entities: &[Entity], data: &Value, seed: &str) -> Result<Value, GeometryError> {
    let text = data.get("text").and_then(Value::as_str).unwrap_or("");
    let family = data.get("font").and_then(Value::as_str).unwrap_or("");
    let size = num_or(data, "size", 10.0);
    let bold = data.get("bold").and_then(Value::as_bool).unwrap_or(false);
    let italic = data.get("italic").and_then(Value::as_bool).unwrap_or(false);
    let spacing = num_or(data, "spacing", 0.0);
    let align = data.get("align").and_then(Value::as_str).filter(|s| !s.is_empty()).unwrap_or("left");
    let anchor = data.get("anchor").and_then(Value::as_str).filter(|s| !s.is_empty()).unwrap_or("baseline");
    let angle = num_or(data, "angle", 0.0);
    let x0 = num_or(data, "x", 0.0);
    let y0 = num_or(data, "y", 0.0);
    let opts = TextOpts { spacing, align: align.to_string(), line_gap: 1.2 };
    let font = font_bytes(family, bold, italic);
    let cont = text_contours(font, text, size, &opts).map_err(sketch_err)?;
    if cont.is_empty() {
        return Err(FeatureError(NO_TEXT.into()));
    }
    // `text_contours` (ttf-parser) starts one vertex after opentype.js `getPath`'s moveTo
    // and puts that moveTo point last. The coordinates match; routes.js numbers from the moveTo.
    let cont: Vec<Vec<[f64; 2]>> = cont.into_iter().map(opentype_start).collect();
    let (lo, hi) = extent_corners(&cont);
    let ox = if anchor == "center" {
        (lo[0] + hi[0]) / 2.0
    } else {
        match align {
            "center" => (lo[0] + hi[0]) / 2.0,
            "right" => hi[0],
            _ => 0.0,
        }
    };
    let oy = if anchor == "center" { (lo[1] + hi[1]) / 2.0 } else { 0.0 };
    let a = angle * std::f64::consts::PI / 180.0;
    let (ca, sa) = (a.cos(), a.sin());
    let group = match data.get("group").and_then(Value::as_str) {
        Some(g) if !g.is_empty() => g.to_string(),
        _ => format!("t{seed}"),
    };
    let meta = text_meta(data);
    let mut fresh = id_factory(entities, seed);
    let add: Vec<Entity> = cont
        .into_iter()
        .map(|c| {
            let pts = c
                .into_iter()
                .map(|p| {
                    let u = p[0] - ox;
                    let w = p[1] - oy;
                    [js_to_fixed(x0 + ca * u - sa * w, 4), js_to_fixed(y0 + sa * u + ca * w, 4)]
                })
                .collect();
            let mut extra = Map::new();
            extra.insert("group".into(), Value::String(group.clone()));
            extra.insert("text".into(), Value::Object(meta.clone()));
            Entity::Polyline(Polyline {
                id: Some(fresh()),
                pts,
                closed: Some(true),
                construction: None,
                extra,
            })
        })
        .collect();
    let added: Vec<String> = add.iter().filter_map(|e| e.id().map(str::to_string)).collect();
    let mut keep = Vec::new();
    for e in entities {
        let same = entity_group(e).is_some_and(|g| g == group);
        if !same {
            keep.push(e.clone());
        }
    }
    keep.extend(add);
    Ok(json!({
        "ok": true,
        "entities": serde_json::to_value(&keep).map_err(|e| FeatureError(e.to_string()))?,
        "added": added,
        "group": group,
    }))
}

fn opentype_start(mut contour: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    if let Some(start) = contour.pop() {
        contour.insert(0, start);
    }
    contour
}

fn extent_corners(contours: &[Vec<[f64; 2]>]) -> ([f64; 2], [f64; 2]) {
    let mut lo = [f64::INFINITY, f64::INFINITY];
    let mut hi = [f64::NEG_INFINITY, f64::NEG_INFINITY];
    for c in contours {
        for p in c {
            lo[0] = lo[0].min(p[0]);
            lo[1] = lo[1].min(p[1]);
            hi[0] = hi[0].max(p[0]);
            hi[1] = hi[1].max(p[1]);
        }
    }
    (lo, hi)
}

fn text_meta(data: &Value) -> Map<String, Value> {
    let mut meta = Map::new();
    for k in ["text", "font", "size", "bold", "italic", "spacing", "align", "angle", "x", "y", "anchor"] {
        meta.insert(k.to_string(), data.get(k).cloned().unwrap_or(Value::Null));
    }
    meta
}

fn entity_group(e: &Entity) -> Option<&str> {
    let extra = match e {
        Entity::Line(x) => &x.extra,
        Entity::Arc(x) => &x.extra,
        Entity::Circle(x) => &x.extra,
        Entity::Rect(x) => &x.extra,
        Entity::Polygon(x) => &x.extra,
        Entity::Slot(x) => &x.extra,
        Entity::Polyline(x) => &x.extra,
        Entity::Point(x) => &x.extra,
    };
    extra.get("group").and_then(Value::as_str)
}

/// `Number(n.toFixed(digits))`: half away from zero, then back to a float.
fn js_to_fixed(n: f64, digits: u32) -> f64 {
    if !n.is_finite() {
        return n;
    }
    let scale = 10f64.powi(digits as i32);
    let y = n * scale;
    let rounded = if y >= 0.0 { (y + 0.5).floor() } else { (y - 0.5).ceil() };
    rounded / scale
}

/// The web edition's DejaVu files (`sketchop_golden.mjs` font loader).
fn font_bytes(family: &str, bold: bool, italic: bool) -> &'static [u8] {
    match family {
        "DejaVu Serif" => {
            if bold {
                include_bytes!("../../../../web/fonts/DejaVuSerif-Bold.ttf")
            } else {
                include_bytes!("../../../../web/fonts/DejaVuSerif.ttf")
            }
        }
        "DejaVu Sans Mono" => {
            if bold {
                include_bytes!("../../../../web/fonts/DejaVuSansMono-Bold.ttf")
            } else {
                include_bytes!("../../../../web/fonts/DejaVuSansMono.ttf")
            }
        }
        _ => match (bold, italic) {
            (true, true) => include_bytes!("../../../../web/fonts/DejaVuSans-BoldOblique.ttf"),
            (true, false) => include_bytes!("../../../../web/fonts/DejaVuSans-Bold.ttf"),
            (false, true) => include_bytes!("../../../../web/fonts/DejaVuSans-Oblique.ttf"),
            (false, false) => include_bytes!("../../../../web/fonts/DejaVuSans.ttf"),
        },
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod golden {
    use std::fs;
    use std::path::PathBuf;

    use serde_json::{json, Map, Value};

    use super::*;
    use crate::Registry;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
    }

    fn norm(v: &Value) -> Value {
        match v {
            Value::Number(n) => json!(n.as_f64().unwrap_or(f64::NAN)),
            Value::Array(a) => Value::Array(a.iter().map(norm).collect()),
            Value::Object(o) => {
                let mut m = Map::new();
                for (k, child) in o {
                    m.insert(k.clone(), norm(child));
                }
                Value::Object(m)
            }
            other => other.clone(),
        }
    }

    /// `offset_open` only: numbers may differ by at most this much.
    /// 2.0 follows 1.x desktop (GEOS), which is exactly 1.5; 1.x web `offsetOpen` is
    /// 1.4999999999999998 (float noise in its mitre formula). See `rust/tests/EXCEPTIONS.md`.
    const OFFSET_OPEN_ABS_TOL: f64 = 1e-12;
    /// Last-bit differences between platform maths libraries (sin, cos), in mm.
    const PLATFORM_ABS_TOL: f64 = 1e-12;

    fn numbers_match(x: f64, y: f64, tol: f64) -> bool {
        if x == y || (x - y).abs() == 0.0 {
            true
        } else {
            tol > 0.0 && (x - y).abs() <= tol
        }
    }

    fn all_diffs(got: &Value, want: &Value, path: &str, tol: f64, out: &mut Vec<String>) {
        match (got, want) {
            (Value::Object(a), Value::Object(b)) => {
                for (k, bv) in b {
                    match a.get(k) {
                        None => out.push(format!("{path}.{k}: missing")),
                        Some(av) => all_diffs(av, bv, &format!("{path}.{k}"), tol, out),
                    }
                }
                for k in a.keys() {
                    if !b.contains_key(k) {
                        out.push(format!("{path}.{k}: unexpected"));
                    }
                }
            }
            (Value::Array(a), Value::Array(b)) => {
                if a.len() != b.len() {
                    out.push(format!("{path}: length {} != {}", a.len(), b.len()));
                    return;
                }
                for (i, (av, bv)) in a.iter().zip(b.iter()).enumerate() {
                    all_diffs(av, bv, &format!("{path}[{i}]"), tol, out);
                }
            }
            (Value::Number(a), Value::Number(b)) => {
                let (x, y) = (a.as_f64().unwrap_or(f64::NAN), b.as_f64().unwrap_or(f64::NAN));
                if !numbers_match(x, y, tol) {
                    out.push(format!("{path}: {x} != {y}"));
                }
            }
            (a, b) if a == b => {}
            (a, b) => out.push(format!("{path}: {a} != {b}")),
        }
    }

    fn object(v: Value) -> Map<String, Value> {
        match v {
            Value::Object(m) => m,
            _ => Map::new(),
        }
    }

    fn document_for(name: &str) -> Document {
        let mut doc = Document::new(Registry::builtin());
        if name.starts_with("project_") || name == "refs_box" {
            let params = object(json!({
                "shape": "box", "length": 40, "width": 20, "height": 10,
                "anchor": "corner", "plane": "XY", "operation": "new",
                "offset": [0, 0, 0], "rotate": [0, 0, 0],
            }));
            doc.add_feature("primitive", params, None).expect("box");
        } else if name == "refs_tipped" {
            let path = repo_root().join("tests/ui/fixtures/tipped.stl");
            let bytes = fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let sid = doc.add_source("tipped.stl", bytes);
            doc.add_feature("import", object(json!({"source": sid, "repair": true, "place": "center"})), None).expect("tipped");
        }
        doc
    }

    #[test]
    fn matches_routes_js() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/sketchop");
        let mut names: Vec<String> = fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).filter_map(|e| {
            e.file_name().to_string_lossy().strip_suffix(".json").map(str::to_string)
        }).collect();
        names.sort();
        assert_eq!(names.len(), 24, "{names:?}");
        let mut failures = Vec::new();
        for name in &names {
            let case: Value = serde_json::from_str(&fs::read_to_string(dir.join(format!("{name}.json"))).unwrap()).unwrap();
            let got = document_for(name).sketch_op(case["op"].as_str().unwrap(), &case["data"]).unwrap_or_else(|e| panic!("{name}: {e}"));
            // 1e-12 mm everywhere: sin/cos round their last bit differently in each platform's
            // maths library (Linux glibc gave -8.660254037844384 where Windows and Node give ...386)
            let tol = if name == "offset_open" { OFFSET_OPEN_ABS_TOL.max(PLATFORM_ABS_TOL) } else { PLATFORM_ABS_TOL };
            let mut found = Vec::new();
            all_diffs(&norm(&got), &norm(&case["result"]), name, tol, &mut found);
            if !found.is_empty() {
                failures.push(format!("{name}:\n{}", found.join("\n")));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    }
}
