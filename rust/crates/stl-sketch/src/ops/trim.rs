//! Port of `trim` (`stleditor/sketch.py`): remove the piece of a shape between its nearest
//! intersections.

use super::explode::explode;
use crate::contours::{entity_path, py_mod};
use crate::entities::{Arc, Entity, Line, P2};
use crate::error::SketchError;
use crate::polygon::{distance_to_path, intersection_points};
use serde_json::Map;

/// 1.x `_geom`: the outline as a path (closed shapes repeat their first point), or `None`.
fn geom(e: &Entity) -> Option<Vec<P2>> {
    let (mut pts, closed) = entity_path(e);
    if pts.len() < 2 {
        return None;
    }
    if closed {
        pts.push(pts[0]);
    }
    Some(pts)
}

/// 1.x `_param`: position of a point on a line (0 to 1) or on an arc or circle (degrees).
fn param(prim: &Entity, pt: P2) -> f64 {
    match prim {
        Entity::Line(l) => {
            let d = [l.p2[0] - l.p1[0], l.p2[1] - l.p1[1]];
            ((pt[0] - l.p1[0]) * d[0] + (pt[1] - l.p1[1]) * d[1]) / (d[0] * d[0] + d[1] * d[1]).max(1e-18)
        }
        Entity::Arc(a) => (pt[1] - a.cy).atan2(pt[0] - a.cx).to_degrees(),
        Entity::Circle(c) => (pt[1] - c.cy).atan2(pt[0] - c.cx).to_degrees(),
        _ => 0.0,
    }
}

/// 1.x web `crossings` for an arc or circle: where the exact circle meets each segment of the
/// path `other`, as angles in degrees (the arc's range is checked by the caller). Same formulas
/// and order as `web/engine/sketch2d.js`, so the angles agree bit for bit.
fn circle_crossings(cx: f64, cy: f64, r: f64, other: &[P2], out: &mut Vec<f64>) {
    for w in other.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (dx, dy, fx, fy) = (b[0] - a[0], b[1] - a[1], a[0] - cx, a[1] - cy);
        let qa = dx * dx + dy * dy;
        let qb = 2.0 * (fx * dx + fy * dy);
        let qc = fx * fx + fy * fy - r * r;
        let disc = qb * qb - 4.0 * qa * qc;
        if disc < 0.0 || qa < 1e-18 {
            continue;
        }
        for s in [-1.0, 1.0] {
            let t = (-qb + s * disc.sqrt()) / (2.0 * qa);
            if (-1e-9..=1.0 + 1e-9).contains(&t) {
                out.push((a[1] + t * dy - cy).atan2(a[0] + t * dx - cx) * 180.0 / std::f64::consts::PI);
            }
            if disc == 0.0 {
                break;
            }
        }
    }
}

fn round_to(x: f64, digits: i32) -> f64 {
    let k = 10f64.powi(digits);
    (x * k).round_ties_even() / k
}

fn sorted_set(mut v: Vec<f64>) -> Vec<f64> {
    v.sort_by(|a, b| a.total_cmp(b));
    v.dedup();
    v
}

/// Remove the piece of entity `eid` under `point`, between its nearest intersections with the
/// other entities (and with the other pieces of a compound shape).
pub fn trim(entities: &[Entity], eid: &str, point: P2) -> Result<Vec<Entity>, SketchError> {
    let nothing = || SketchError::new("Nothing to trim there.");
    let target = entities.iter().find(|e| e.id() == Some(eid)).ok_or_else(nothing)?;
    // construction lines don't cut (1.x web; 1.x desktop let them)
    let others: Vec<&Entity> = entities.iter().filter(|e| e.id() != Some(eid) && !e.construction()).collect();
    let prims = explode(target);
    if prims.is_empty() {
        return Err(nothing());
    }
    // the piece closest to the point (the first one on a tie)
    let mut gi = 0;
    let mut best = f64::INFINITY;
    for (i, p) in prims.iter().enumerate() {
        let d = geom(p).map_or(0.0, |g| distance_to_path(&g, point));
        if d < best {
            best = d;
            gi = i;
        }
    }
    let prim = prims[gi].clone();
    let rest: Vec<Entity> = prims.iter().enumerate().filter(|(i, _)| *i != gi).map(|(_, p)| p.clone()).collect();
    let g = geom(&prim).unwrap_or_default();
    let mut cuts: Vec<f64> = vec![];
    for o in others.iter().copied().chain(rest.iter()) {
        let Some(go) = geom(o) else { continue };
        match &prim {
            Entity::Arc(a) => circle_crossings(a.cx, a.cy, a.r, &go, &mut cuts),
            Entity::Circle(c) => circle_crossings(c.cx, c.cy, c.r, &go, &mut cuts),
            _ => {
                for q in intersection_points(&g, &go) {
                    cuts.push(param(&prim, q));
                }
            }
        }
    }
    let tp = param(&prim, point);
    let pid = prim.id().unwrap_or("").to_string();
    let mut out: Vec<Entity> = vec![];
    match &prim {
        Entity::Line(l) => {
            let cs = sorted_set(cuts.iter().filter(|&&c| 1e-6 < c && c < 1.0 - 1e-6).map(|&c| round_to(c, 9)).collect());
            let lo = cs.iter().copied().filter(|&c| c <= tp).fold(0.0, f64::max);
            let hi = cs.iter().copied().filter(|&c| c >= tp).fold(1.0, f64::min);
            let (a, b) = (l.p1, l.p2);
            let at = |t: f64| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
            let mk = |suffix: &str, p1: P2, p2: P2| {
                let mut n: Line = l.clone();
                n.id = Some(format!("{pid}{suffix}"));
                n.p1 = p1;
                n.p2 = p2;
                Entity::Line(n)
            };
            if lo > 1e-6 {
                out.push(mk("a", a, at(lo)));
            }
            if hi < 1.0 - 1e-6 {
                out.push(mk("b", at(hi), b));
            }
        }
        Entity::Arc(_) | Entity::Circle(_) => {
            let (full, a0, sweep, cx, cy, r, construction) = match &prim {
                Entity::Arc(a) => {
                    let s = py_mod(a.a1 - a.a0, 360.0);
                    (false, a.a0, if s == 0.0 { 360.0 } else { s }, a.cx, a.cy, a.r, a.construction)
                }
                Entity::Circle(c) => (true, 0.0, 360.0, c.cx, c.cy, c.r, c.construction),
                _ => unreachable!(),
            };
            let rel = |ang: f64| py_mod(ang - a0, 360.0);
            let cs: Vec<f64> = if full {
                sorted_set(cuts.iter().map(|&c| py_mod(round_to(rel(c), 7), 360.0)).collect())
            } else {
                sorted_set(cuts.iter().map(|&c| rel(c)).filter(|&c| 1e-5 < c && c < sweep - 1e-5).map(|c| round_to(c, 7)).collect())
            };
            let t = rel(tp);
            let mk = |suffix: &str, s: f64, e: f64| {
                Entity::Arc(Arc {
                    id: Some(format!("{pid}{suffix}")),
                    cx,
                    cy,
                    r,
                    a0: s,
                    a1: e,
                    construction,
                    extra: Map::new(),
                })
            };
            if full {
                // a circle cut once (or not at all) is removed entirely
                if cs.len() >= 2 {
                    let lo = cs.iter().copied().filter(|&c| c <= t).reduce(f64::max).unwrap_or(cs[cs.len() - 1] - 360.0);
                    let hi = cs.iter().copied().filter(|&c| c >= t).reduce(f64::min).unwrap_or(cs[0] + 360.0);
                    out.push(mk("a", a0 + hi, a0 + lo + 360.0));
                }
            } else {
                let lo = cs.iter().copied().filter(|&c| c <= t).fold(0.0, f64::max);
                let hi = cs.iter().copied().filter(|&c| c >= t).fold(sweep, f64::min);
                if lo > 1e-5 {
                    out.push(mk("a", a0, a0 + lo));
                }
                if hi < sweep - 1e-5 {
                    out.push(mk("b", a0 + hi, a0 + sweep));
                }
            }
        }
        _ => {}
    }
    let mut res = vec![];
    for e in entities {
        if e.id() == Some(eid) {
            res.extend(rest.iter().cloned());
            res.extend(out.iter().cloned());
        } else {
            res.push(e.clone());
        }
    }
    Ok(res)
}
