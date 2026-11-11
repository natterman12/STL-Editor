//! Port of `entity_contours`, `entity_path`, arcs and linework from `stleditor/sketch.py`.

use crate::entities::{Entity, P2};
use std::f64::consts::PI;

/// Corner count for a round shape of radius `r` (1.x `_segs`).
pub fn segs(r: f64) -> i64 {
    (256.0f64).min((48.0f64).max(r * 10.0)) as i64
}

/// Python's `%` for floats: the result takes the sign of the divisor.
pub fn py_mod(x: f64, y: f64) -> f64 {
    let m = x % y;
    if m != 0.0 {
        if (y < 0.0) != (m < 0.0) { m + y } else { m }
    } else {
        0.0f64.copysign(y)
    }
}

/// `numpy.linspace(0, stop, num, endpoint=False)`.
fn linspace_open(stop: f64, num: usize) -> Vec<f64> {
    let step = stop / num as f64;
    (0..num).map(|i| i as f64 * step).collect()
}

/// `numpy.linspace(0, stop, num + 1)` (end point included, exactly `stop`).
fn linspace_closed(stop: f64, num: usize) -> Vec<f64> {
    let step = stop / num as f64;
    let mut v: Vec<f64> = (0..=num).map(|i| i as f64 * step).collect();
    if num > 0 {
        v[num] = stop;
    }
    v
}

fn circle_points(cx: f64, cy: f64, r: f64) -> Vec<P2> {
    let n = segs(r) as usize;
    linspace_open(2.0 * PI, n).into_iter().map(|a| [cx + r * a.cos(), cy + r * a.sin()]).collect()
}

/// Closed contours (counter-clockwise) of a closed entity; empty for open and construction entities.
pub fn entity_contours(e: &Entity) -> Vec<Vec<P2>> {
    if e.construction() {
        return vec![];
    }
    match e {
        Entity::Rect(rc) => {
            let (x, y, w, h) = (rc.x, rc.y, rc.w, rc.h);
            if w.abs() < 1e-9 || h.abs() < 1e-9 {
                return vec![];
            }
            let (x0, x1) = if x <= x + w { (x, x + w) } else { (x + w, x) };
            let (y0, y1) = if y <= y + h { (y, y + h) } else { (y + h, y) };
            let mut r = rc.fillet.unwrap_or(0.0);
            r = r.min((x1 - x0) / 2.0 - 1e-6).min((y1 - y0) / 2.0 - 1e-6);
            let c: Vec<P2> = if r > 1e-6 {
                let n = 8.max(segs(r) / 4);
                let mut pts = vec![];
                for (cx, cy, a0) in
                    [(x1 - r, y0 + r, -90.0), (x1 - r, y1 - r, 0.0), (x0 + r, y1 - r, 90.0), (x0 + r, y0 + r, 180.0)]
                {
                    for i in 0..=n {
                        let a = (a0 + 90.0 * i as f64 / n as f64).to_radians();
                        pts.push([cx + r * a.cos(), cy + r * a.sin()]);
                    }
                }
                pts
            } else {
                vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
            };
            vec![rotate(c, rc.angle.unwrap_or(0.0), x + w / 2.0, y + h / 2.0)]
        }
        Entity::Circle(c) => {
            if c.r <= 0.0 {
                return vec![];
            }
            vec![circle_points(c.cx, c.cy, c.r)]
        }
        Entity::Polygon(p) => {
            let n = p.sides();
            if p.r <= 0.0 || n < 3 {
                return vec![];
            }
            let a0 = p.angle.unwrap_or(0.0).to_radians();
            vec![(0..n).map(|i| a0 + i as f64 * 2.0 * PI / n as f64).map(|a| [p.cx + p.r * a.cos(), p.cy + p.r * a.sin()]).collect()]
        }
        Entity::Slot(s) => {
            if s.r <= 0.0 {
                return vec![];
            }
            let d = [s.x2 - s.x1, s.y2 - s.y1];
            let l = d[0].hypot(d[1]);
            if l < 1e-9 {
                return vec![circle_points(s.x1, s.y1, s.r)];
            }
            let ang = d[1].atan2(d[0]);
            let n = 12.max(segs(s.r) / 2) as usize;
            let mut pts = vec![];
            for i in 0..=n {
                let a = ang - PI / 2.0 + PI * i as f64 / n as f64;
                pts.push([s.x2 + s.r * a.cos(), s.y2 + s.r * a.sin()]);
            }
            for i in 0..=n {
                let a = ang + PI / 2.0 + PI * i as f64 / n as f64;
                pts.push([s.x1 + s.r * a.cos(), s.y1 + s.r * a.sin()]);
            }
            vec![pts]
        }
        Entity::Polyline(p) => {
            if !p.is_closed() || p.pts.len() < 3 {
                return vec![];
            }
            vec![p.pts.clone()]
        }
        _ => vec![],
    }
}

fn rotate(c: Vec<P2>, angle: f64, cx: f64, cy: f64) -> Vec<P2> {
    if angle == 0.0 {
        return c;
    }
    let a = angle.to_radians();
    let (s, co) = (a.sin(), a.cos());
    c.into_iter()
        .map(|p| {
            let (px, py) = (p[0] - cx, p[1] - cy);
            [px * co + py * -s + cx, px * s + py * co + cy]
        })
        .collect()
}

/// The points of an arc (1.x `_arc_points`); `n` is the piece count, or chosen from the radius.
pub fn arc_points(cx: f64, cy: f64, r: f64, a0_deg: f64, a1_deg: f64, n: Option<usize>) -> Vec<P2> {
    let (a0, a1) = (a0_deg.to_radians(), a1_deg.to_radians());
    let mut sweep = py_mod(a1 - a0, 2.0 * PI);
    if sweep < 1e-9 {
        sweep = 2.0 * PI;
    }
    let n = n.unwrap_or_else(|| 8.max((segs(r) as f64 * sweep / (2.0 * PI)) as i64) as usize);
    linspace_closed(sweep, n).into_iter().map(|t| a0 + t).map(|a| [cx + r * a.cos(), cy + r * a.sin()]).collect()
}

/// The outline of any entity and whether it is closed (1.x `entity_path`). Used for linework,
/// picking and trimming.
pub fn entity_path(e: &Entity) -> (Vec<P2>, bool) {
    match e {
        Entity::Line(l) => (vec![l.p1, l.p2], false),
        Entity::Arc(a) => (arc_points(a.cx, a.cy, a.r, a.a0, a.a1, None), false),
        Entity::Polyline(p) => (p.pts.clone(), p.is_closed()),
        _ => {
            let mut e = e.clone();
            set_construction(&mut e, false);
            let mut c = entity_contours(&e);
            if c.is_empty() { (vec![], false) } else { (c.swap_remove(0), true) }
        }
    }
}

pub(crate) fn set_construction(e: &mut Entity, v: bool) {
    match e {
        Entity::Line(x) => x.construction = v.then_some(true),
        Entity::Arc(x) => x.construction = v.then_some(true),
        Entity::Circle(x) => x.construction = v.then_some(true),
        Entity::Rect(x) => x.construction = v.then_some(true),
        Entity::Polygon(x) => x.construction = v.then_some(true),
        Entity::Slot(x) => x.construction = v.then_some(true),
        Entity::Polyline(x) => x.construction = v.then_some(true),
        Entity::Point(x) => x.construction = v.then_some(true),
    }
}

/// Snap to a 1e-5 grid (1.x `_q`; Python rounds halves to even).
fn quantize(p: P2) -> P2 {
    let tol = 1e-5;
    [(p[0] / tol).round_ties_even() * tol, (p[1] / tol).round_ties_even() * tol]
}

fn wanted(e: &Entity, only: Option<&[String]>) -> bool {
    match only {
        Some(ids) if !ids.is_empty() => e.id().is_some_and(|id| ids.iter().any(|o| o == id)),
        _ => true,
    }
}

pub(crate) fn id_wanted(e: &Entity, only: Option<&[String]>) -> bool {
    wanted(e, only)
}

/// The lines of the sketch for polygonize (1.x `_linework`): quantized, closed shapes with
/// their first point repeated at the end.
pub fn linework(entities: &[Entity], only: Option<&[String]>) -> Vec<Vec<P2>> {
    let mut lines = vec![];
    for e in entities {
        if e.construction() || !wanted(e, only) {
            continue;
        }
        let (pts, closed) = entity_path(e);
        if pts.len() < 2 {
            continue;
        }
        let mut pts: Vec<P2> = pts.into_iter().map(quantize).collect();
        if closed {
            pts.push(pts[0]);
        }
        lines.push(pts);
    }
    lines
}
