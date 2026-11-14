//! Snapping and picking under the pointer (`sketcher.js` `eventUV`, `entityAt`, `pickRef`,
//! `baseRefs`), at 1.x's pixel radii.

use std::cell::Cell;

use serde_json::{json, Map, Value};
use stl_sketch::entities::{point, point_keys, Entity, Line, Point, P2};

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
}

/// The sketch panel's Grid snap (`snapGrid`, `snapStep`, in mm). 1.x keeps these on the sketcher;
/// the session has no field for them yet.
pub fn set_grid(on: bool, step: f64) {
    GRID.with(|g| g.set((on, step)));
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
        return Snap { uv: p, kind: SnapKind::Point, on: r };
    }
    let (on, step) = grid();
    if on && step > 0.0 {
        let r = |v: f64| (v / step + 0.5).floor() * step;
        let mut uv = [r(raw[0]), r(raw[1])];
        if let Some(last) = straight_from {
            if (raw[1] - last[1]).abs() < STRAIGHT_PX * px {
                uv[1] = last[1];
            }
            if (raw[0] - last[0]).abs() < STRAIGHT_PX * px {
                uv[0] = last[0];
            }
        }
        return Snap { uv, kind: SnapKind::Grid, on: None };
    }
    Snap { uv: raw, kind: SnapKind::Free, on: None }
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
