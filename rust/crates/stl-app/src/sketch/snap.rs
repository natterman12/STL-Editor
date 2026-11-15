//! Snapping and picking under the pointer (`sketcher.js` `eventUV`, `entityAt`, `pickRef`,
//! `baseRefs`), at 1.x's pixel radii.

use std::cell::Cell;

use serde_json::{json, Map, Value};
use stl_sketch::entities::{point, point_keys, Entity, Line, Point, P2};

use super::guides::{self, Guide};
use super::{geom, Ref, SketchSession};
use crate::viewport::Viewport;

/// What the pointer snapped to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapKind {
    /// A key point of a shape, a part edge, the origin, or the drawing in progress.
    Point,
    /// The grid.
    Grid,
    /// Nothing (the grid is off).
    Free,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Snap {
    pub uv: P2,
    pub kind: SnapKind,
    /// The point snapped to, when snapping there ties the new point to it (`snapRef`).
    pub on: Option<Ref>,
    /// Dotted guides to the points the pointer lines up with (spec 2.3).
    pub guides: Vec<Guide>,
}

impl Snap {
    /// `this.snapped`.
    pub fn snapped(&self) -> bool {
        self.kind == SnapKind::Point
    }
}

/// Snap radius, in screen points (`10 * px`).
pub const SNAP_PX: f64 = 10.0;
/// Hold a line straight within this many screen points of horizontal / vertical.
pub const STRAIGHT_PX: f64 = 6.0;
/// `entityAt`'s radius.
pub const ENTITY_PX: f64 = 8.0;
/// `pickRef`'s point radius (its shapes use [`ENTITY_PX`]).
pub const POINT_PX: f64 = 9.0;

const AXIS: f64 = 1e5;

thread_local! {
    static GRID: Cell<(bool, f64)> = const { Cell::new((true, 1.0)) };
    /// Grid snap follows the spacing the grid is drawn at (and whether that is in inches).
    static AUTO: Cell<(bool, bool)> = const { Cell::new((false, false)) };
    /// The spacing the plane grid is drawn at now (mm), once a sketch has drawn it.
    static SHOWN: Cell<Option<f64>> = const { Cell::new(None) };
}

/// The plane grid is drawn at `step` mm (`None`: no grid is up). While it is, Auto snaps to
/// exactly the lines drawn, wherever the pointer is (a perspective view has a different scale at
/// each point, but the grid has one spacing).
pub fn set_shown_step(step: Option<f64>) {
    SHOWN.with(|c| c.set(step.filter(|s| s.is_finite() && *s > 0.0)));
}

/// The sketch panel's Grid snap (`snapGrid`, `snapStep`, in mm): a fixed step. 1.x keeps these
/// on the sketcher; the session has no field for them yet.
pub fn set_grid(on: bool, step: f64) {
    GRID.with(|g| g.set((on, step)));
    AUTO.with(|a| a.set((false, false)));
}

/// Grid snap with the panel's choice: `auto` snaps to the spacing the grid shows at this zoom
/// ([`guides::visible_grid_step_in`]), else to the fixed `step`.
pub fn set_grid_mode(on: bool, step: f64, auto: bool, inch: bool) {
    GRID.with(|g| g.set((on, step)));
    AUTO.with(|a| a.set((auto, inch)));
}

/// `(follows the zoom, in inches)`.
pub fn grid_auto() -> (bool, bool) {
    AUTO.with(|a| a.get())
}

/// The grid spacing in effect with the pointer where one screen point is `px` mm: the spacing
/// the grid is drawn at (when it is up), else the one this zoom gives ([`zoom_step_at`]).
pub fn grid_step_at(px: f64) -> f64 {
    let (_, fixed) = grid();
    let (auto, _) = grid_auto();
    if !auto {
        return fixed;
    }
    SHOWN.with(|c| c.get()).unwrap_or_else(|| zoom_step_at(px))
}

/// What Auto gives with one screen point being `px` mm: the 1-2-5 steps (fractions of an inch in
/// inch mode) that keep the lines [`guides::GRID_MIN_PX`] apart.
pub fn zoom_step_at(px: f64) -> f64 {
    let (_, inch) = grid_auto();
    guides::visible_grid_step_in(1.0 / px, inch)
}

pub fn grid() -> (bool, f64) {
    GRID.with(|g| g.get())
}

fn tagged(kind: &str) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("ref".into(), json!(kind));
    m
}

/// `baseRefs()`: the origin and the two axes every sketch can measure from.
pub fn base_refs() -> [Entity; 3] {
    let line = |id: &str, p1, p2| Entity::Line(Line { id: Some(id.into()), p1, p2, construction: None, extra: tagged("axis") });
    [
        Entity::Point(Point { id: Some("_o".into()), x: 0.0, y: 0.0, construction: None, extra: tagged("origin") }),
        line("_h", [-AXIS, 0.0], [AXIS, 0.0]),
        line("_v", [0.0, -AXIS], [0.0, AXIS]),
    ]
}

/// An entity's unknown keys.
pub fn extra(e: &Entity) -> &Map<String, Value> {
    match e {
        Entity::Line(x) => &x.extra,
        Entity::Arc(x) => &x.extra,
        Entity::Circle(x) => &x.extra,
        Entity::Rect(x) => &x.extra,
        Entity::Polygon(x) => &x.extra,
        Entity::Slot(x) => &x.extra,
        Entity::Polyline(x) => &x.extra,
        Entity::Point(x) => &x.extra,
    }
}

/// `e.ref === 'axis'`.
pub fn is_axis(e: &Entity) -> bool {
    extra(e).get("ref").and_then(Value::as_str) == Some("axis")
}

/// `refEnts` in `setRefs` order: the origin, the part edges, the missing ones, the axes. These
/// are the solver's `fixed` entities.
pub fn fixed_ents(s: &SketchSession) -> Vec<Entity> {
    let [o, h, v] = base_refs();
    let has = |id: &str| s.refs.iter().any(|r| r.ent.id() == Some(id));
    let mut out = Vec::new();
    if !has("_o") {
        out.push(o);
    }
    out.extend(s.refs.iter().filter(|r| !r.missing).map(|r| r.ent.clone()));
    out.extend(s.refs.iter().filter(|r| r.missing).map(|r| r.ent.clone()));
    for a in [h, v] {
        if !has(a.id().unwrap_or_default()) {
            out.push(a);
        }
    }
    out
}

/// mm per screen point where `uv` is on the plane.
pub fn px_at(s: &SketchSession, vp: &Viewport, uv: P2) -> f64 {
    vp.pixel_size(s.to_3(uv))
}

/// `eventUV` without a drawing in progress.
pub fn snap(s: &SketchSession, vp: &Viewport, uv: P2) -> Snap {
    snap_with(s, vp, uv, &[], None)
}

/// `eventUV`: snap `raw` (plane coordinates under the pointer) to the nearest key point within
/// 10 px (shapes' points, their midpoints and centres, the origin and part edges, `draw_pts` of
/// the drawing in progress); else to the grid. `straight_from` is the line tool's last point:
/// near-horizontal and near-vertical lines are held straight (6 px).
pub fn snap_with(s: &SketchSession, vp: &Viewport, raw: P2, draw_pts: &[P2], straight_from: Option<P2>) -> Snap {
    snap_impl(s, vp, raw, draw_pts, straight_from, false)
}

/// [`snap_with`] for the drawing tools: when no point is near, the pointer also lines up with the
/// X or Y of any point on the sketch (or of the drawing in progress) within 6 px, and the snap
/// carries the dotted guides to draw (spec 2.3).
pub fn snap_tracking(s: &SketchSession, vp: &Viewport, raw: P2, draw_pts: &[P2], straight_from: Option<P2>) -> Snap {
    snap_impl(s, vp, raw, draw_pts, straight_from, true)
}

fn snap_impl(s: &SketchSession, vp: &Viewport, raw: P2, draw_pts: &[P2], straight_from: Option<P2>, track: bool) -> Snap {
    let px = px_at(s, vp, raw);
    let mut cands: Vec<(P2, Option<Ref>)> = Vec::new();
    for e in &s.entities {
        for k in point_keys(e) {
            if let Some(p) = point(e, &k) {
                cands.push((p, e.id().map(|id| Ref::pt(id, &k))));
            }
        }
        for (_, q) in geom::key_points(e) {
            cands.push((q, None));
        }
    }
    for e in fixed_ents(s).iter().filter(|e| !is_axis(e)) {
        for k in point_keys(e) {
            if let Some(p) = point(e, &k) {
                cands.push((p, e.id().map(|id| Ref::pt(id, &k))));
            }
        }
    }
    cands.extend(draw_pts.iter().map(|p| (*p, None)));
    let tracked: Vec<P2> = cands.iter().map(|c| c.0).collect();
    let mut bd = SNAP_PX * px;
    let mut best: Option<(P2, Option<Ref>)> = None;
    for (p, r) in cands {
        let d = (p[0] - raw[0]).hypot(p[1] - raw[1]);
        if d < bd - 1e-12 {
            bd = d;
            best = Some((p, r));
        }
    }
    if let Some((p, r)) = best {
        return Snap { uv: p, kind: SnapKind::Point, on: r, guides: Vec::new() };
    }
    // lined up with a point's X or Y (the grid still rounds the other coordinate)
    let (ax, ay) = if track { guides::align(raw, &tracked, guides::TRACK_PX * px) } else { (None, None) };
    let (on, _) = grid();
    let step = grid_step_at(px);
    let mut uv = raw;
    let mut kind = SnapKind::Free;
    if on && step > 0.0 {
        let r = |v: f64| (v / step + 0.5).floor() * step;
        uv = [r(raw[0]), r(raw[1])];
        kind = SnapKind::Grid;
        if let Some(last) = straight_from {
            if (raw[1] - last[1]).abs() < STRAIGHT_PX * px {
                uv[1] = last[1];
            }
            if (raw[0] - last[0]).abs() < STRAIGHT_PX * px {
                uv[0] = last[0];
            }
        }
    }
    let mut from = Vec::new();
    if let Some((x, p)) = ax {
        uv[0] = x;
        from.push(p);
    }
    if let Some((y, p)) = ay {
        uv[1] = y;
        from.push(p);
    }
    let guides = from.into_iter().map(|p| Guide { from: p, to: uv }).collect();
    Snap { uv, kind, on: None, guides }
}

/// `entityAt`: the sketch shape within 8 px of `uv`, nearest first.
pub fn entity_at(s: &SketchSession, vp: &Viewport, uv: P2) -> Option<String> {
    let px = px_at(s, vp, uv);
    let mut bd = ENTITY_PX * px;
    let mut best = None;
    for e in &s.entities {
        let d = geom::dist_to_entity(e, uv);
        if d < bd {
            bd = d;
            best = e.id().map(str::to_string);
        }
    }
    best
}

/// `pickRef(uv)` on the sketch's own shapes (what a drag grabs): a point within 9 px (not a
/// line's midpoint), else a shape within 8 px.
pub fn pick_own(s: &SketchSession, vp: &Viewport, uv: P2) -> Option<Ref> {
    let px = px_at(s, vp, uv);
    let mut bd = POINT_PX * px;
    let mut best = None;
    for e in &s.entities {
        let Some(id) = e.id() else { continue };
        for k in point_keys(e) {
            if k == "m" {
                continue;
            }
            if let Some(q) = point(e, &k) {
                let d = (q[0] - uv[0]).hypot(q[1] - uv[1]);
                if d < bd {
                    bd = d;
                    best = Some(Ref::pt(id, &k));
                }
            }
        }
    }
    if best.is_some() {
        return best;
    }
    let mut bed = ENTITY_PX * px;
    let mut be = None;
    for e in &s.entities {
        if matches!(e, Entity::Point(_)) {
            continue;
        }
        let d = geom::dist_to_entity(e, uv);
        if d < bed {
            bed = d;
            be = e.id().map(Ref::ent);
        }
    }
    be
}
