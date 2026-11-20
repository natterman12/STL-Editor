//! Exact placement for Hole and Thread points (spec 2.4): snapping to sketch geometry, a distance
//! along a sketch line, and the "From" row (distances from one or two part edges or sketch lines).
//! Stored beside the points as `point_refs` (Hole) or `position_refs` (Thread); see
//! `stl_engine::features::common::placement`, which re-places the points when the project runs.
//!
//! A reference here is the engine's JSON plus, on sketch lines, the line's world end points when
//! it was picked (`"a"`, `"b"`): the panel and the viewport use them without the engine state;
//! the engine ignores them and finds the line again by its id.

use std::sync::Arc;

use egui::{RichText, Ui};
use serde_json::{Value, json};
use stl_engine::features::common::placement::{self, DistLine};
use stl_sketch::contours::entity_path;
use stl_sketch::entities::parse_entities;
use stl_sketch::{Entity, point, point_keys};

use super::bodies::{Mark, pt_from, pt4};
use super::number::{fmt_len, parse_len};
use crate::engine_link::SketchOutline;
use crate::theme::Tokens;
use crate::tools::{Ctx, Values};
use crate::viewport::PlaneFrame;

/// Hole's references, one list per point.
pub const POINT_REFS: &str = "point_refs";
/// Thread's references (one list).
pub const POSITION_REFS: &str = "position_refs";
/// While set, a click on the part picks an edge or sketch line for the "From" row instead of
/// adding a point. A `_` key, so it never reaches the engine (`params_cleanup`).
pub const FROM_PICK: &str = "_from_pick";
/// The index of the Hole point picked in the viewport (click its dot): the next click on the face
/// moves that point instead of adding one, and the "From" rows are its rows. A `_` key.
pub const SEL: &str = "_sel";
/// The marker colour of the selected point.
pub const SELECTED: u32 = 0xffc94a;
/// The marker colour of a point that snapped to sketch geometry or is measured from something.
pub const SNAPPED: u32 = 0x2f9e44;
/// At most this many distances per point.
pub const MAX_DIST: usize = 2;

type V3 = [f64; 3];

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn len(a: V3) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: V3) -> Option<V3> {
    let l = len(a);
    (l > 1e-12).then(|| mul(a, 1.0 / l))
}

/// The nearest point of segment `a -> b` to `p`, and how far along it (mm from `a`).
fn nearest_on(p: V3, a: V3, b: V3) -> (V3, f64) {
    let d = sub(b, a);
    let l2 = dot(d, d);
    if l2 < 1e-18 {
        return (a, 0.0);
    }
    let t = (dot(sub(p, a), d) / l2).clamp(0.0, 1.0);
    (add(a, mul(d, t)), t * l2.sqrt())
}

/// `p` moved along `n` onto the plane through `at` with normal `n`.
fn onto_plane(p: V3, n: V3, at: V3) -> V3 {
    add(p, mul(n, dot(sub(at, p), n)))
}

// ------------------------------------------------------------------ sketch geometry

/// A straight piece of a sketch, in world coordinates.
#[derive(Clone, Debug)]
pub struct SkLine {
    pub sketch: String,
    /// `None` when the entity has no id (it can be snapped to, not measured from).
    pub entity: Option<String>,
    pub a: V3,
    pub b: V3,
}

/// What a click can snap to in the visible sketches lying parallel to a face.
#[derive(Clone, Debug, Default)]
pub struct SkGeom {
    /// Sketch points, line ends, midpoints, circle and arc centres.
    pub points: Vec<V3>,
    pub lines: Vec<SkLine>,
}

fn straight(e: &Entity) -> bool {
    matches!(
        e,
        Entity::Line(_) | Entity::Rect(_) | Entity::Polygon(_) | Entity::Polyline(_)
    )
}

/// The visible sketches' geometry whose plane is parallel to the face with normal `n` (1°).
pub fn sketch_geometry(ctx: &Ctx, n: V3) -> SkGeom {
    let mut g = SkGeom::default();
    for s in ctx.sketches.iter().filter(|s| !s.hidden) {
        let [o, x, y, sn] = s.frame;
        if dot(sn, n).abs() < 0.999_847_7 {
            continue;
        }
        let Some(list) = ctx
            .features
            .iter()
            .find(|f| f.id == s.id)
            .and_then(|f| f.params.get("entities"))
            .and_then(Value::as_array)
        else {
            continue;
        };
        let Ok(ents) = parse_entities(list) else { continue };
        let w = |p: [f64; 2]| add(o, add(mul(x, p[0]), mul(y, p[1])));
        for e in &ents {
            for k in point_keys(e) {
                if let Some(p) = point(e, &k) {
                    g.points.push(w(p));
                }
            }
            if !straight(e) {
                continue;
            }
            let (pts, closed) = entity_path(e);
            let n_seg = if closed { pts.len() } else { pts.len().saturating_sub(1) };
            for k in 0..n_seg {
                let (a, b) = (w(pts[k]), w(pts[(k + 1) % pts.len()]));
                if len(sub(b, a)) < 1e-9 {
                    continue;
                }
                let entity = e.id().map(|id| {
                    if matches!(e, Entity::Line(_)) {
                        id.to_owned()
                    } else {
                        format!("{id}#{k}")
                    }
                });
                g.points.push(mul(add(a, b), 0.5));
                if !matches!(e, Entity::Line(_)) {
                    g.points.push(a);
                }
                g.lines.push(SkLine {
                    sketch: s.id.clone(),
                    entity,
                    a,
                    b,
                });
            }
        }
    }
    g
}

/// Where a click on a face snaps to, and the sketch line it is on (as a reference with `along`).
#[derive(Clone, Debug, PartialEq)]
pub struct Snap {
    pub p: V3,
    pub line_ref: Option<Value>,
}

/// Snap `hit` (on the face with normal `n`) to the sketch geometry within `tol` mm: a point
/// (end, midpoint, centre) first, else the nearest point on a line. The result is put on the
/// face along its normal. `None`: nothing near.
pub fn snap(hit: V3, n: V3, ctx: &Ctx, tol: f64) -> Option<Snap> {
    let g = sketch_geometry(ctx, n);
    // distances in the face's plane (the sketch may sit a little above or below it)
    let flat = |p: V3| onto_plane(p, n, hit);
    let near_pt = g
        .points
        .iter()
        .map(|p| (len(sub(flat(*p), hit)), *p))
        .filter(|(d, _)| *d <= tol)
        .min_by(|a, b| a.0.total_cmp(&b.0));
    let line_through = |p: V3| -> Option<Value> {
        g.lines.iter().filter(|l| l.entity.is_some()).find_map(|l| {
            let (q, t) = nearest_on(p, l.a, l.b);
            (len(sub(q, p)) < 1e-6).then(|| line_ref(l, Some(t), None))
        })
    };
    if let Some((_, p)) = near_pt {
        return Some(Snap {
            p: flat(p),
            line_ref: line_through(p),
        });
    }
    let near_line = g
        .lines
        .iter()
        .map(|l| {
            let (q, t) = nearest_on(flat(hit), flat(l.a), flat(l.b));
            (len(sub(q, hit)), l, t)
        })
        .filter(|(d, _, _)| *d <= tol)
        .min_by(|a, b| a.0.total_cmp(&b.0))?;
    let (_, l, t) = near_line;
    let d = unit(sub(l.b, l.a))?;
    let on = add(l.a, mul(d, t));
    Some(Snap {
        p: flat(on),
        line_ref: l.entity.is_some().then(|| line_ref(l, Some(t), None)),
    })
}

fn r4(x: f64) -> f64 {
    (x * 1e4).round() / 1e4
}

fn line_ref(l: &SkLine, along: Option<f64>, dist: Option<f64>) -> Value {
    let mut v = json!({"kind": "sketch_line", "sketch": l.sketch, "entity": l.entity, "a": pt4(l.a), "b": pt4(l.b)});
    if let Some(t) = along {
        v["along"] = json!(r4(t));
    }
    if let Some(d) = dist {
        v["dist"] = json!(r4(d));
    }
    v
}

/// The snap distance for a click on body `id`: 3 % of its size, between 0.5 and 5 mm. (The
/// click carries no screen scale; see the Task 5 report.)
pub fn tolerance(ctx: &Ctx, id: &str) -> f64 {
    let Some(b) = ctx.bodies.iter().find(|b| b.id == id) else {
        return 1.0;
    };
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in b.mesh.positions.chunks_exact(3) {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i] as f64);
            hi[i] = hi[i].max(p[i] as f64);
        }
    }
    if !lo[0].is_finite() {
        return 1.0;
    }
    (len(sub(hi, lo)) * 0.03).clamp(0.5, 5.0)
}

// ------------------------------------------------------------------ references

/// A reference's line as stored (`a`, `b`).
fn ref_line(r: &Value) -> Option<(V3, V3)> {
    Some((pt_from(r.get("a")?)?, pt_from(r.get("b")?)?))
}

/// Distance from `p` to the line `a -> b`, measured in the plane with normal `n`.
fn dist_in_plane(p: V3, a: V3, b: V3, n: V3) -> f64 {
    let d = sub(b, a);
    let d = unit(sub(d, mul(n, dot(d, n)))).unwrap_or(d);
    match unit(cross(n, d)) {
        Some(s) => dot(sub(p, a), s).abs(),
        None => 0.0,
    }
}

/// A click while picking for the "From" row: the sketch line near it, else the nearest straight
/// edge of the clicked body on a face like the hole's. The reference keeps the point's distance
/// from it now.
pub fn pick_ref(ctx: &Ctx, body: &str, hit: V3, n: V3, at: V3, tol: f64) -> Option<Value> {
    let g = sketch_geometry(ctx, n);
    let flat = |p: V3| onto_plane(p, n, hit);
    let sk = g
        .lines
        .iter()
        .filter(|l| l.entity.is_some())
        .map(|l| (len(sub(nearest_on(hit, flat(l.a), flat(l.b)).0, hit)), l))
        .filter(|(d, _)| *d <= tol)
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, l)) = sk {
        return Some(line_ref(l, None, Some(dist_in_plane(at, l.a, l.b, n))));
    }
    let b = ctx.bodies.iter().find(|b| b.id == body)?;
    let edges = placement::straight_edges(&b.mesh, n);
    // like a sketch line, an edge only counts when the click is within `tol` of it
    let (_, (a, bb)) = edges
        .iter()
        .map(|e| (len(sub(nearest_on(hit, e.0, e.1).0, hit)), *e))
        .filter(|(d, _)| *d <= tol)
        .min_by(|x, y| x.0.total_cmp(&y.0))?;
    let r = placement::PlaceRef::Edge {
        a,
        b: bb,
        face_normal: n,
        dist: r4(dist_in_plane(at, a, bb, n)),
    };
    let mut v = placement::ref_json(&r, Some(body));
    v["a"] = pt4(a);
    v["b"] = pt4(bb);
    v["normal"] = pt4(n);
    Some(v)
}

/// Where the point goes from its references as they are now (the same rule the engine uses):
/// an `along` sketch line puts it there; else it keeps its distances. `None`: nothing to do, or
/// the references can't both hold.
pub fn resolve(at: V3, n: V3, refs: &[Value]) -> Option<V3> {
    let mut fixed = None;
    let mut lines = Vec::new();
    for r in refs {
        let (a, b) = ref_line(r)?;
        match (
            r.get("along").and_then(Value::as_f64),
            r.get("dist").and_then(Value::as_f64),
        ) {
            (Some(t), _) => {
                fixed.get_or_insert(add(a, mul(unit(sub(b, a))?, t)));
            }
            (None, Some(d)) => lines.push(DistLine {
                a,
                b,
                dist: d,
                side: placement::side_of(at, a, b, n),
            }),
            _ => {}
        }
    }
    if fixed.is_none() && lines.is_empty() {
        return None;
    }
    placement::place(at, n, at, fixed, &lines).ok()
}

/// Where the point goes from its references against the geometry as it is now: the same rule as
/// the engine's `resolve_point` (part edges are found again on the current meshes, sketch lines by
/// id). Returns the point and the references with their `a` / `b` brought up to date. `None`: no
/// references, or one can't be found / the two can't both hold (the engine keeps the stored point
/// then, and so does the panel).
pub fn resolve_current(ctx: &Ctx, body: &str, stored: V3, normal: V3, refs: &[Value]) -> Option<(V3, Vec<Value>)> {
    let n = unit(normal)?;
    let mut out: Vec<Value> = refs.to_vec();
    let mut edges: Option<Vec<(V3, V3)>> = None;
    let mut fixed: Option<V3> = None;
    let mut lines = Vec::new();
    let mut plane_at = stored;
    let mut moved_plane = false;
    let sk = sketch_geometry(ctx, n);
    for (k, raw) in refs.iter().enumerate() {
        match placement::parse_ref(raw)? {
            placement::PlaceRef::Edge {
                a,
                b,
                face_normal,
                dist,
            } => {
                let cands = edges.get_or_insert_with(|| {
                    let want = raw
                        .get("body")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                        .unwrap_or(body);
                    ctx.bodies
                        .iter()
                        .filter(|x| want.is_empty() || x.id == want)
                        .flat_map(|x| placement::straight_edges(&x.mesh, face_normal))
                        .collect()
                });
                let (na, nb) = placement::pick_edge(cands, a, b)?;
                if !moved_plane {
                    plane_at = add(stored, mul(n, dot(sub(na, a), n)));
                    moved_plane = true;
                }
                lines.push(DistLine {
                    a: na,
                    b: nb,
                    dist,
                    side: placement::side_of(stored, a, b, n),
                });
                out[k]["a"] = pt4(na);
                out[k]["b"] = pt4(nb);
            }
            placement::PlaceRef::SketchLine {
                sketch,
                entity,
                along,
                dist,
            } => {
                let l = sk
                    .lines
                    .iter()
                    .find(|l| l.sketch == sketch && l.entity.as_deref() == Some(entity.as_str()))?;
                let (a, b) = (l.a, l.b);
                match (along, dist) {
                    (Some(t), off) => {
                        let d = unit(sub(b, a))?;
                        let mut p = add(a, mul(d, t));
                        if let Some(o) = off {
                            p = add(p, mul(cross(n, d), placement::side_of(stored, a, b, n) * o));
                        }
                        fixed.get_or_insert(p);
                    }
                    (None, Some(o)) => lines.push(DistLine {
                        a,
                        b,
                        dist: o,
                        side: placement::side_of(stored, a, b, n),
                    }),
                    (None, None) => {}
                }
                out[k]["a"] = pt4(a);
                out[k]["b"] = pt4(b);
            }
        }
    }
    let p = placement::place(stored, n, plane_at, fixed, &lines).ok()?;
    Some((p, out))
}

/// Bring Hole's points up to date with their references (a rebuild may have moved an edge): each
/// point with references takes the point they now give, so what the panel shows, what is stored
/// when OK is pressed and what the engine cuts agree. True when something changed.
pub fn refresh_points(values: &mut Values, field: &str, ctx: &Ctx) -> bool {
    let mut pts: Vec<Value> = values.get(field).and_then(Value::as_array).cloned().unwrap_or_default();
    let Some(n) = values.get("normal").and_then(pt_from) else {
        return false;
    };
    let body = values
        .get("targets")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let mut lists = ref_lists(values, POINT_REFS);
    let mut changed = false;
    for i in 0..pts.len().min(lists.len()) {
        let Some(at) = pt_from(&pts[i]) else { continue };
        if lists[i].is_empty() {
            continue;
        }
        let list_body = lists[i]
            .iter()
            .find_map(|r| r.get("body").and_then(Value::as_str))
            .unwrap_or(&body)
            .to_owned();
        let Some((p, refs)) = resolve_current(ctx, &list_body, at, n, &lists[i]) else {
            continue;
        };
        let p = pt_from(&pt4(p)).unwrap_or(p);
        if len(sub(p, at)) > 1e-6 {
            pts[i] = pt4(p);
            changed = true;
        }
        if refs != lists[i] {
            lists[i] = refs;
            changed = true;
        }
    }
    if changed {
        values.insert(field.to_owned(), Value::Array(pts));
        set_ref_lists(values, POINT_REFS, lists);
    }
    changed
}

/// The same for Thread's one point (`offset` in the `plane` frame, `position_refs`).
pub fn refresh_thread(values: &mut Values, ctx: &Ctx) -> bool {
    let Some((fr, p)) = thread_point(values, ctx) else {
        return false;
    };
    let refs = thread_refs(values);
    if refs.is_empty() || !values.get("plane").is_some_and(Value::is_object) {
        return false;
    }
    let body = values
        .get("plane")
        .and_then(|v| v.get("body"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let Some((q, new_refs)) = resolve_current(ctx, &body, p, fr.normal, &refs) else {
        return false;
    };
    let mut changed = false;
    if len(sub(q, p)) > 1e-6 {
        set_thread_point(values, &fr, q);
        changed = true;
    }
    if new_refs != refs {
        set_thread_refs(values, new_refs);
        changed = true;
    }
    changed
}

// ------------------------------------------------------------------ moving a placed point

/// The selected point of `field`, if there is one and it still exists.
pub fn selected(values: &Values, field: &str) -> Option<usize> {
    let n = values.get(field).and_then(Value::as_array).map_or(0, Vec::len);
    let i = values.get(SEL).and_then(Value::as_u64)? as usize;
    (i < n).then_some(i)
}

/// The point the "From" rows and an edge pick act on: the selected one, else the last.
pub fn from_index(values: &Values, field: &str) -> Option<usize> {
    let n = values.get(field).and_then(Value::as_array).map_or(0, Vec::len);
    selected(values, field).or(n.checked_sub(1))
}

/// Whether a click at `hit` (face normal `hit_n`) is on the face the point `at` sits on.
pub fn same_face(at: V3, n: V3, hit: V3, hit_n: V3) -> bool {
    let (Some(n), Some(h)) = (unit(n), unit(hit_n)) else {
        return false;
    };
    dot(n, h) > 0.999 && dot(sub(hit, at), n).abs() < 1e-2
}

/// A point moved by hand to `p` (on its face, normal `n`), and what becomes of its references.
/// Distances from edges and sketch lines stay and take the values they have at `p`, so the
/// point is still measured from the same things. A point with none of those snaps to the
/// sketch geometry near `p`, as a click does.
fn moved_refs(refs: &[Value], p: V3, n: V3, ctx: &Ctx, tol: f64) -> (V3, Vec<Value>) {
    let kept: Vec<Value> = refs
        .iter()
        .filter(|r| r.get("along").is_none() && r.get("dist").is_some())
        .map(|r| {
            let mut r = r.clone();
            if let Some((a, b)) = ref_line(&r) {
                r["dist"] = json!(r4(dist_in_plane(p, a, b, n)));
            }
            r
        })
        .collect();
    if !kept.is_empty() {
        return (p, kept);
    }
    match snap(p, n, ctx, tol) {
        Some(s) => (s.p, s.line_ref.into_iter().collect()),
        None => (p, Vec::new()),
    }
}

/// Move Hole's point `i` to `to`, kept on its face. False when there is no such point.
pub fn move_point(values: &mut Values, field: &str, i: usize, to: V3, ctx: &Ctx, tol: f64) -> bool {
    let mut pts: Vec<Value> = values.get(field).and_then(Value::as_array).cloned().unwrap_or_default();
    let (Some(at), Some(n)) = (
        pts.get(i).and_then(pt_from),
        values.get("normal").and_then(pt_from).and_then(unit),
    ) else {
        return false;
    };
    let mut lists = ref_lists(values, POINT_REFS);
    lists.resize(pts.len(), Vec::new());
    let (p, refs) = moved_refs(&lists[i], onto_plane(to, n, at), n, ctx, tol);
    pts[i] = pt4(p);
    lists[i] = refs;
    values.insert(field.to_owned(), Value::Array(pts));
    set_ref_lists(values, POINT_REFS, lists);
    true
}

/// Move Thread's point to `to`, kept on its face or plane. False when it has no plane yet.
pub fn move_thread(values: &mut Values, ctx: &Ctx, to: V3, tol: f64) -> bool {
    let Some((fr, at)) = thread_point(values, ctx) else {
        return false;
    };
    let Some(n) = unit(fr.normal) else { return false };
    let (p, refs) = moved_refs(&thread_refs(values), onto_plane(to, n, at), n, ctx, tol);
    set_thread_point(values, &fr, p);
    set_thread_refs(values, refs);
    true
}

/// Whether Thread's point is placed from something (drawn in [`SNAPPED`]).
pub fn thread_has_refs(values: &Values) -> bool {
    !thread_refs(values).is_empty()
}

/// The reference lists, as stored (`key` holds one list per point).
pub fn ref_lists(values: &Values, key: &str) -> Vec<Vec<Value>> {
    values
        .get(key)
        .and_then(Value::as_array)
        .map(|a| a.iter().map(|l| l.as_array().cloned().unwrap_or_default()).collect())
        .unwrap_or_default()
}

/// Store the lists; all empty means no key at all (so a hole without references saves as before).
pub fn set_ref_lists(values: &mut Values, key: &str, lists: Vec<Vec<Value>>) {
    if lists.iter().all(|l| l.is_empty()) {
        values.remove(key);
    } else {
        values.insert(
            key.to_owned(),
            Value::Array(lists.into_iter().map(Value::Array).collect()),
        );
    }
}

fn ref_name(r: &Value, i: usize) -> String {
    let what = if r.get("kind").and_then(Value::as_str) == Some("edge") {
        "Edge"
    } else {
        "Sketch line"
    };
    format!("{what} {}", i + 1)
}

// ------------------------------------------------------------------ the panel rows

/// A distance box. The text being typed is kept in egui's memory (the pick fields have no
/// `Bufs` of their own).
fn dist_box(ui: &mut Ui, tk: &Tokens, key: &str, shown: &str) -> Option<String> {
    let id = egui::Id::unique("place-refs-bufs");
    let mut bufs: super::Bufs = ui.data_mut(|d| d.get_temp(id)).unwrap_or_default();
    let r = super::text_commit(
        ui,
        tk,
        &mut bufs,
        key,
        shown,
        56.0,
        egui::FontId::proportional(12.0),
        tk.text,
        None,
    );
    ui.data_mut(|d| d.insert_temp(id, bufs));
    r
}

/// What the "From" rows did.
struct RowsOut {
    /// A distance was typed or a reference removed: `refs` changed.
    changed: bool,
    /// Pick edge was clicked.
    toggle_pick: bool,
}

/// A Pick button, then one row per reference with a distance box (or an Along box for a point
/// on a sketch line). `key` names the text boxes.
fn ref_rows(ui: &mut Ui, tk: &Tokens, refs: &mut Vec<Value>, picking: bool, key: &str, inch: bool) -> RowsOut {
    let mut out = RowsOut {
        changed: false,
        toggle_pick: false,
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new("From").size(11.5).color(tk.label));
        let txt = if picking { "Click an edge…" } else { "Pick edge" };
        let b = ui.add(egui::Button::new(RichText::new(txt).size(11.5)).selected(picking));
        let b = b.on_hover_text("Click a straight edge of the part or a sketch line, then type how far the point is from it. Pick two to fix the point exactly.");
        out.toggle_pick = b.clicked();
    });
    let unit = if inch { "in" } else { "mm" };
    let mut remove: Option<usize> = None;
    for (k, r) in refs.clone().iter().enumerate() {
        let along = r.get("along").and_then(Value::as_f64);
        let val = along.or_else(|| r.get("dist").and_then(Value::as_f64)).unwrap_or(0.0);
        ui.horizontal(|ui| {
            let name = if along.is_some() {
                "Along line".to_owned()
            } else {
                ref_name(r, k)
            };
            ui.label(RichText::new(&name).size(11.5).color(tk.text))
                .on_hover_text(if along.is_some() {
                    "Distance from the line's start"
                } else {
                    "Distance from this edge, in the face"
                });
            let typed = dist_box(ui, tk, &format!("{key}:{k}"), &fmt_len(val, None, inch));
            ui.label(RichText::new(unit).size(11.0).color(tk.muted));
            if let Some(t) = typed.and_then(|t| parse_len(&t, inch)) {
                refs[k][if along.is_some() { "along" } else { "dist" }] = json!(r4(t));
                out.changed = true;
            }
            let x = ui
                .add(egui::Button::new(RichText::new("×").size(12.0)).frame(false))
                .on_hover_text("Stop measuring from this");
            if x.clicked() {
                remove = Some(k);
            }
        });
    }
    if let Some(k) = remove {
        refs.remove(k);
        out.changed = true;
    }
    out
}

fn picking(values: &Values) -> bool {
    values.get(FROM_PICK).and_then(Value::as_bool).unwrap_or(false)
}

fn toggle_pick(values: &mut Values, active: &mut Option<String>, field: &str) {
    if picking(values) {
        values.remove(FROM_PICK);
    } else {
        values.insert(FROM_PICK.to_owned(), json!(true));
        *active = Some(field.to_owned());
    }
}

/// A picked reference joins `refs` (a point snapped onto a line is measured from edges instead
/// once one is picked; a third distance replaces the second).
fn push_ref(refs: &mut Vec<Value>, r: Value) {
    refs.retain(|x| x.get("along").is_none());
    refs.truncate(MAX_DIST - 1);
    refs.push(r);
}

/// The "From" rows under a point list. They act on the selected point, else the last. `active` is set to `field`
/// while picking, so the click comes to that field. True when a value changed.
pub fn show_from(
    ui: &mut Ui,
    tk: &Tokens,
    values: &mut Values,
    field: &str,
    active: &mut Option<String>,
    inch: bool,
) -> bool {
    let mut pts: Vec<Value> = values.get(field).and_then(Value::as_array).cloned().unwrap_or_default();
    let Some(i) = from_index(values, field) else {
        return false;
    };
    let Some(at) = pt_from(&pts[i]) else { return false };
    let n = values.get("normal").and_then(pt_from).unwrap_or([0.0, 0.0, 1.0]);
    let mut lists = ref_lists(values, POINT_REFS);
    lists.resize(pts.len(), Vec::new());
    let mut refs = lists[i].clone();
    let out = ref_rows(ui, tk, &mut refs, picking(values), &format!("{POINT_REFS}:{i}"), inch);
    if out.toggle_pick {
        toggle_pick(values, active, field);
    }
    if out.changed {
        if let Some(p) = resolve(at, n, &refs) {
            pts[i] = pt4(p);
            values.insert(field.to_owned(), Value::Array(pts));
        }
        lists[i] = refs;
        set_ref_lists(values, POINT_REFS, lists);
    }
    out.changed || out.toggle_pick
}

/// Add a picked reference to the selected point (else the last) and move the point so the distances hold.
pub fn add_ref(values: &mut Values, field: &str, r: Value) {
    let mut pts: Vec<Value> = values.get(field).and_then(Value::as_array).cloned().unwrap_or_default();
    let Some(i) = from_index(values, field) else { return };
    let mut lists = ref_lists(values, POINT_REFS);
    lists.resize(pts.len(), Vec::new());
    push_ref(&mut lists[i], r);
    if let (Some(at), Some(n)) = (pt_from(&pts[i]), values.get("normal").and_then(pt_from))
        && let Some(p) = resolve(at, n, &lists[i])
    {
        pts[i] = pt4(p);
        values.insert(field.to_owned(), Value::Array(pts));
    }
    set_ref_lists(values, POINT_REFS, lists);
}

// ------------------------------------------------------------------ Thread (On + Position)
//
// Thread keeps its point as the `plane` frame plus `offset` (in the frame's x, y and normal),
// and one list `position_refs`. These are ready for the plane field's place-on-face click, the
// Position row and the tool marks (the wiring is outside Task 5's files; see its report).

/// The thread's face frame, its point now and the face normal.
pub fn thread_point(values: &Values, ctx: &Ctx) -> Option<(PlaneFrame, V3)> {
    let fr = super::plane::frame_of(values.get("plane")?, ctx)?;
    let off = values.get("offset").and_then(pt_from).unwrap_or([0.0; 3]);
    let p = add(
        fr.origin,
        add(add(mul(fr.x, off[0]), mul(fr.y, off[1])), mul(fr.normal, off[2])),
    );
    Some((fr, p))
}

/// Store `p` as the thread's `offset` (3 decimals, as the place-on-face click does).
fn set_thread_point(values: &mut Values, fr: &PlaneFrame, p: V3) {
    let d = sub(p, fr.origin);
    let r3 = |x: f64| (x * 1e3).round() / 1e3;
    values.insert(
        "offset".to_owned(),
        json!([r3(dot(d, fr.x)), r3(dot(d, fr.y)), r3(dot(d, fr.normal))]),
    );
}

fn thread_refs(values: &Values) -> Vec<Value> {
    values
        .get(POSITION_REFS)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn set_thread_refs(values: &mut Values, refs: Vec<Value>) {
    if refs.is_empty() {
        values.remove(POSITION_REFS);
    } else {
        values.insert(POSITION_REFS.to_owned(), Value::Array(refs));
    }
}

/// After the place-on-face click set `plane` and `offset`: snap the point to the sketch
/// geometry near it (a point on a sketch line keeps the line as its reference). Clears the old
/// references (it is a new position). True when it snapped.
pub fn thread_after_place(values: &mut Values, ctx: &Ctx, body: &str) -> bool {
    values.remove(POSITION_REFS);
    let Some((fr, p)) = thread_point(values, ctx) else {
        return false;
    };
    let Some(s) = snap(p, fr.normal, ctx, tolerance(ctx, body)) else {
        return false;
    };
    set_thread_point(values, &fr, s.p);
    set_thread_refs(values, s.line_ref.into_iter().collect());
    true
}

/// A click while the thread's From row is picking: `Some(changed)` when the click was taken
/// (call this before the place-on-face handling and return `PickOut::changed()` when `Some`).
pub fn thread_feed_pick(values: &mut Values, ctx: &Ctx, body: &str, hit: V3) -> Option<bool> {
    if !picking(values) {
        return None;
    }
    values.remove(FROM_PICK);
    let Some((fr, p)) = thread_point(values, ctx) else {
        return Some(true);
    };
    let Some(r) = pick_ref(ctx, body, hit, fr.normal, p, tolerance(ctx, body)) else {
        return Some(true);
    };
    let mut refs = thread_refs(values);
    push_ref(&mut refs, r);
    if let Some(q) = resolve(p, fr.normal, &refs) {
        set_thread_point(values, &fr, q);
    }
    set_thread_refs(values, refs);
    Some(true)
}

/// The thread's From rows (under Position). `field` is the field the pick click should go to
/// (`"plane"`). True when a value changed.
pub fn show_thread_from(
    ui: &mut Ui,
    tk: &Tokens,
    values: &mut Values,
    ctx: &Ctx,
    field: &str,
    active: &mut Option<String>,
) -> bool {
    if !values.get("plane").is_some_and(Value::is_object) {
        return false; // an origin plane has no part edges to measure from
    }
    let moved = refresh_thread(values, ctx);
    let Some((fr, p)) = thread_point(values, ctx) else {
        return false;
    };
    let mut refs = thread_refs(values);
    let out = ref_rows(ui, tk, &mut refs, picking(values), POSITION_REFS, ctx.inch);
    if out.toggle_pick {
        toggle_pick(values, active, field);
    }
    if out.changed {
        if let Some(q) = resolve(p, fr.normal, &refs) {
            set_thread_point(values, &fr, q);
        }
        set_thread_refs(values, refs);
    }
    out.changed || out.toggle_pick || moved
}

/// The thread's dimension lines (see [`dim_marks`]).
pub fn thread_dim_marks(values: &Values, ctx: &Ctx) -> Vec<Mark> {
    let Some((fr, p)) = thread_point(values, ctx) else {
        return Vec::new();
    };
    let mut v = Values::new();
    v.insert("pts".into(), json!([pt4(p)]));
    v.insert("normal".into(), pt4(fr.normal));
    v.insert(POINT_REFS.into(), json!([thread_refs(values)]));
    dim_marks(&v, "pts")
}

// ------------------------------------------------------------------ the viewport

/// A frame in the face's plane at `o`.
pub fn face_frame(o: V3, n: V3) -> Option<PlaneFrame> {
    let f = stl_engine::features::common::frames::plane_frame(o, n, None).ok()?;
    Some(PlaneFrame {
        origin: f.o,
        x: f.x,
        y: f.y,
        normal: f.n,
    })
}

/// The dimension lines of the points' references: from each point straight to the edge or line
/// it is measured from, plus that line, drawn in the face's plane.
pub fn dim_marks(values: &Values, field: &str) -> Vec<Mark> {
    let pts: Vec<V3> = values
        .get(field)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(pt_from).collect())
        .unwrap_or_default();
    let n = values.get("normal").and_then(pt_from).unwrap_or([0.0, 0.0, 1.0]);
    let lists = ref_lists(values, POINT_REFS);
    let mut out = Vec::new();
    for (p, refs) in pts.iter().zip(lists.iter()) {
        let Some(fr) = face_frame(*p, n) else { continue };
        let uv = |q: V3| {
            let d = sub(onto_plane(q, n, *p), *p);
            [dot(d, fr.x), dot(d, fr.y)]
        };
        let mut outlines = Vec::new();
        for r in refs {
            let Some((a, b)) = ref_line(r) else { continue };
            outlines.push(SketchOutline {
                pts: vec![uv(a), uv(b)],
                closed: false,
                construction: false,
            });
            if r.get("along").is_none() {
                let flat_a = onto_plane(a, n, *p);
                let d = unit(sub(onto_plane(b, n, *p), flat_a)).unwrap_or([1.0, 0.0, 0.0]);
                let foot = add(flat_a, mul(d, dot(sub(*p, flat_a), d)));
                outlines.push(SketchOutline {
                    pts: vec![uv(foot), [0.0, 0.0]],
                    closed: false,
                    construction: true,
                });
            }
        }
        if !outlines.is_empty() {
            let outlines: Arc<[SketchOutline]> = outlines.into();
            out.push(Mark::SketchLines { frame: fr, outlines });
        }
    }
    out
}

/// Whether point `i` is placed from something (drawn in [`SNAPPED`]).
pub fn has_refs(values: &Values, i: usize) -> bool {
    ref_lists(values, POINT_REFS).get(i).is_some_and(|l| !l.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_link::{SketchView, TimelineItem};

    /// One sketch on z = 5 with a line, circle, arc, point and rectangle.
    fn with_ctx<R>(f: impl FnOnce(&Ctx) -> R) -> R {
        let sketches = vec![SketchView {
            id: "s1".into(),
            name: "Sketch1".into(),
            plane: String::new(),
            hidden: false,
            frame: [[0.0, 0.0, 5.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            outlines: Arc::from(Vec::new()),
        }];
        let features = vec![TimelineItem {
            id: "s1".into(),
            kind: "sketch".into(),
            params: json!({"entities": [
                {"type": "line", "id": "l1", "p1": [0, 0], "p2": [20, 0]},
                {"type": "circle", "id": "c1", "cx": 50, "cy": 50, "r": 5},
                {"type": "arc", "id": "a1", "cx": 80, "cy": 20, "r": 6, "a0": 0, "a1": 90},
                {"type": "point", "id": "p1", "x": 30, "y": 70},
                {"type": "rect", "id": "r1", "x": 100, "y": 100, "w": 20, "h": 10}
            ]}),
            active: true,
            ..Default::default()
        }];
        let ctx = Ctx {
            bodies: &[],
            planes: &[],
            sketches: &sketches,
            features: &features,
            selection: &[],
            last_sketch: None,
            sketch_on_body: false,
            inch: false,
        };
        f(&ctx)
    }

    fn near(a: V3, b: V3) -> bool {
        len(sub(a, b)) < 1e-6
    }

    const UP: V3 = [0.0, 0.0, 1.0];

    #[test]
    fn snaps_to_each_kind() {
        with_ctx(|ctx| {
            let at = |x: f64, y: f64| snap([x, y, 5.0], UP, ctx, 1.0).map(|s| s.p);
            assert_eq!(at(0.4, 0.3), Some([0.0, 0.0, 5.0]), "line start");
            assert_eq!(at(19.6, -0.5), Some([20.0, 0.0, 5.0]), "line end");
            assert_eq!(at(10.3, 0.4), Some([10.0, 0.0, 5.0]), "line midpoint");
            assert_eq!(at(50.5, 49.6), Some([50.0, 50.0, 5.0]), "circle centre");
            assert_eq!(at(80.2, 20.3), Some([80.0, 20.0, 5.0]), "arc centre");
            assert_eq!(at(30.3, 69.8), Some([30.0, 70.0, 5.0]), "sketch point");
            assert_eq!(at(60.0, 60.0), None, "nothing near");
            // onto a line, with the distance from its start
            let s = snap([4.0, 0.6, 5.0], UP, ctx, 1.0).expect("on the line");
            assert!(near(s.p, [4.0, 0.0, 5.0]), "{s:?}");
            let r = s.line_ref.expect("the line is the reference");
            assert_eq!((r["entity"].as_str(), r["along"].as_f64()), (Some("l1"), Some(4.0)));
            // a rectangle's side is `<id>#<k>`
            let g = sketch_geometry(ctx, UP);
            let side = g
                .lines
                .iter()
                .find(|l| l.entity.as_deref().is_some_and(|e| e.starts_with("r1#")))
                .expect("a side of the rectangle")
                .clone();
            let mid = mul(add(side.a, side.b), 0.5);
            let q = add(add(side.a, mul(sub(mid, side.a), 0.5)), [0.0, 0.0, 0.0]);
            let s = snap(q, UP, ctx, 0.5).expect("on the side");
            assert_eq!(
                s.line_ref.and_then(|r| r["entity"].as_str().map(str::to_owned)),
                side.entity
            );
            // a sketch on a wall is not snapped to from a floor face
            assert!(snap([0.4, 0.3, 5.0], [1.0, 0.0, 0.0], ctx, 1.0).is_none());
        });
    }

    #[test]
    fn resolve_matches_engine_rule() {
        // 10 from x = 0 and 8 from y = 0, from a point that was clicked nearby
        let e1 = json!({"kind": "edge", "a": [0, 0, 5], "b": [0, 30, 5], "dist": 10});
        let e2 = json!({"kind": "edge", "a": [0, 0, 5], "b": [40, 0, 5], "dist": 8});
        let p = resolve([11.0, 9.5, 5.0], UP, &[e1.clone(), e2]).unwrap();
        assert!(near(p, [10.0, 8.0, 5.0]), "{p:?}");
        // one distance moves the point straight away from that edge only
        let p = resolve([11.0, 9.5, 5.0], UP, &[e1]).unwrap();
        assert!(near(p, [10.0, 9.5, 5.0]), "{p:?}");
        // the empty list is not stored
        let mut v = Values::new();
        set_ref_lists(&mut v, POINT_REFS, vec![vec![], vec![]]);
        assert!(v.get(POINT_REFS).is_none());
    }

    #[test]
    fn thread_snaps_and_keeps_its_line() {
        with_ctx(|ctx| {
            let mut v = Values::new();
            v.insert(
                "plane".into(),
                json!({"origin": [0, 0, 5], "normal": [0, 0, 1], "xdir": [1, 0, 0], "body": "b1"}),
            );
            v.insert("offset".into(), json!([4.0, 0.6, 0.0]));
            assert!(thread_after_place(&mut v, ctx, "b1"));
            assert_eq!(v["offset"], json!([4.0, 0.0, 0.0]));
            assert_eq!(v[POSITION_REFS][0]["entity"], json!("l1"));
            assert_eq!(thread_dim_marks(&v, ctx).len(), 1);
            // not picking: the click is the plane field's
            assert_eq!(thread_feed_pick(&mut v, ctx, "b1", [1.0, 1.0, 5.0]), None);
        });
    }
}
