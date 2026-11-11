//! Building constraints from picks (`makeConstraint`, `inferDimension`, `angleTo`, `lineMeet`
//! in `web/js/constraints.js`).

use super::constraints::{
    center, crs, dir, get_param, is_line, is_round, js_max, js_min, js_sign, len, line_ends, or1, params_of, sub, Constraint, Ref,
    World,
};
use crate::entities::{point, Entity, P2};
use serde::Serialize;
use std::collections::BTreeSet;

const NAN2: P2 = [f64::NAN, f64::NAN];

/// What the JS functions return: a constraint, a plain-English message, or null (wait for
/// another pick).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Outcome {
    Constraint(Constraint),
    Message(String),
    Pending,
}

fn msg(s: &str) -> Outcome {
    Outcome::Message(s.to_string())
}

fn pk(w: &World, r: &Ref) -> P2 {
    w.get(&r.e).and_then(|e| point(e, r.k.as_deref().unwrap_or(""))).unwrap_or(NAN2)
}

fn dim(kind: &str, a: Ref, b: Option<Ref>) -> Constraint {
    Constraint::new(kind).with(Some(a), b)
}

fn signed(kind: &str, a: Ref, b: Ref, s: f64) -> Constraint {
    let mut c = dim(kind, a, Some(b));
    c.sign = Some(or1(s));
    c
}

/// `makeConstraint`: picks are `{e, k?}` in click order.
pub fn make_constraint(tool: &str, picks: &[Ref], entities: &[Entity]) -> Outcome {
    let w = World::new(entities);
    let ent = |r: &Ref| w.get(&r.e);
    let (p, q, r) = (picks.first(), picks.get(1), picks.get(2));
    let need = |n: usize| picks.len() >= n;
    match tool {
        "c_coincident" => {
            if !need(2) {
                return Outcome::Pending;
            }
            let (p, q) = (p.unwrap(), q.unwrap());
            if p.has_k() && q.has_k() {
                return Outcome::Constraint(dim("coincident", p.clone(), Some(q.clone())));
            }
            if p.has_k() && (is_line(ent(q)) || is_round(ent(q))) {
                return Outcome::Constraint(dim("onentity", p.clone(), Some(Ref::ent(&q.e))));
            }
            if q.has_k() && (is_line(ent(p)) || is_round(ent(p))) {
                return Outcome::Constraint(dim("onentity", q.clone(), Some(Ref::ent(&p.e))));
            }
            msg("Pick two points, or a point and a line / circle.")
        }
        "c_hv" => {
            if let Some(p) = p {
                if !p.has_k() && is_line(ent(p)) {
                    let d = dir(ent(p));
                    let kind = if d[0].abs() >= d[1].abs() { "horizontal" } else { "vertical" };
                    return Outcome::Constraint(dim(kind, Ref::ent(&p.e), None));
                }
            }
            if !need(2) {
                return match p {
                    Some(p) if !p.has_k() => msg("Pick a line, or two points."),
                    _ => Outcome::Pending,
                };
            }
            let (p, q) = (p.unwrap(), q.unwrap());
            if p.has_k() && q.has_k() {
                let d = sub(pk(&w, q), pk(&w, p));
                let kind = if d[0].abs() >= d[1].abs() { "horizontal" } else { "vertical" };
                return Outcome::Constraint(dim(kind, p.clone(), Some(q.clone())));
            }
            msg("Pick a line, or two points.")
        }
        "c_parallel" | "c_perpendicular" => {
            if !need(2) {
                return Outcome::Pending;
            }
            let (p, q) = (p.unwrap(), q.unwrap());
            if is_line(ent(p)) && is_line(ent(q)) && !p.has_k() && !q.has_k() {
                return Outcome::Constraint(dim(&tool[2..], Ref::ent(&p.e), Some(Ref::ent(&q.e))));
            }
            msg("Pick two lines.")
        }
        "c_equal" => {
            if !need(2) {
                return Outcome::Pending;
            }
            let (p, q) = (p.unwrap(), q.unwrap());
            if (is_line(ent(p)) && is_line(ent(q))) || (is_round(ent(p)) && is_round(ent(q))) {
                return Outcome::Constraint(dim("equal", Ref::ent(&p.e), Some(Ref::ent(&q.e))));
            }
            msg("Pick two lines, or two circles / arcs.")
        }
        "c_tangent" => {
            if !need(2) {
                return Outcome::Pending;
            }
            let (p, q) = (p.unwrap(), q.unwrap());
            let (a, b) = (ent(p), ent(q));
            if (is_line(a) && is_round(b)) || (is_round(a) && is_line(b)) {
                return Outcome::Constraint(dim("tangent", Ref::ent(&p.e), Some(Ref::ent(&q.e))));
            }
            if is_round(a) && is_round(b) {
                let dd = len(sub(center(a), center(b)));
                let (ra, rb) = (super::constraints::radius(a), super::constraints::radius(b));
                let mut c = dim("tangent", Ref::ent(&p.e), Some(Ref::ent(&q.e)));
                c.inner = Some((dd - (ra - rb).abs()).abs() < (dd - (ra + rb)).abs());
                return Outcome::Constraint(c);
            }
            msg("Pick a line and a circle / arc, or two circles / arcs.")
        }
        "c_concentric" => {
            if !need(2) {
                return Outcome::Pending;
            }
            let (p, q) = (p.unwrap(), q.unwrap());
            if is_round(ent(p)) && is_round(ent(q)) {
                return Outcome::Constraint(dim("concentric", Ref::ent(&p.e), Some(Ref::ent(&q.e))));
            }
            msg("Pick two circles or arcs.")
        }
        "c_midpoint" => {
            if !need(2) {
                return Outcome::Pending;
            }
            let (p, q) = (p.unwrap(), q.unwrap());
            if p.has_k() && is_line(ent(q)) && !q.has_k() {
                return Outcome::Constraint(dim("midpoint", p.clone(), Some(Ref::ent(&q.e))));
            }
            if q.has_k() && is_line(ent(p)) && !p.has_k() {
                return Outcome::Constraint(dim("midpoint", q.clone(), Some(Ref::ent(&p.e))));
            }
            msg("Pick a point and a line.")
        }
        "c_fix" => {
            let Some(p) = p else { return Outcome::Pending };
            let Some(e) = ent(p) else { return Outcome::Pending };
            let mut c = dim("fix", if p.has_k() { p.clone() } else { Ref::ent(&p.e) }, None);
            if p.has_k() {
                // JS spreads `point(...)`, which throws on a bad key: treat it like a missing entity.
                let Some(pt) = point(e, p.k.as_deref().unwrap()) else { return Outcome::Pending };
                c.at = Some(pt.to_vec());
            } else {
                c.at = Some((0..params_of(e).len()).map(|i| get_param(e, i)).collect());
            }
            Outcome::Constraint(c)
        }
        "c_symmetric" => {
            if !need(3) {
                return Outcome::Pending;
            }
            let (p, q, r) = (p.unwrap(), q.unwrap(), r.unwrap());
            if p.has_k() && q.has_k() && is_line(ent(r)) {
                let mut c = dim("symmetric", p.clone(), Some(q.clone()));
                c.c = Some(Ref::ent(&r.e));
                return Outcome::Constraint(c);
            }
            msg("Pick two points, then the line of symmetry.")
        }
        "c_dimension" => infer_dimension(picks, entities, None, &BTreeSet::new()),
        _ => msg("Unknown constraint tool."),
    }
}

/// `lineMeet`: where two lines (taken as endless) cross, or `None` when they're parallel.
pub fn line_meet(a: &Entity, b: &Entity) -> Option<P2> {
    let (ea, eb) = (Some(a), Some(b));
    let (da, db) = (dir(ea), dir(eb));
    let d = crs(da, db);
    if d.abs() < 1e-12 * or1(len(da) * len(db)) {
        return None;
    }
    let (a1, b1) = (line_ends(ea).0, line_ends(eb).0);
    let t = crs(sub(b1, a1), db) / d;
    Some([a1[0] + da[0] * t, a1[1] + da[1] * t])
}

fn angle_to_w(c: &Constraint, w: &World, cur: P2) -> Constraint {
    let ea = c.a.as_ref().and_then(|r| w.get(&r.e));
    let eb = c.b.as_ref().and_then(|r| w.get(&r.e));
    let (a, b) = (dir(ea), dir(eb));
    let v = match (ea, eb) {
        (Some(x), Some(y)) => line_meet(x, y),
        _ => None,
    }
    .unwrap_or_else(|| {
        let (p1, p2) = line_ends(ea);
        [(p1[0] + p2[0]) / 2.0, (p1[1] + p2[1]) / 2.0]
    });
    let wv = sub(cur, v);
    let det = crs(a, b);
    let ra = or1(js_sign(crs(wv, b) / det));
    let rb = or1(js_sign(crs(a, wv) / det));
    let sa = [a[0] * ra, a[1] * ra];
    let sb = [b[0] * rb, b[1] * rb];
    let mut out = c.clone();
    out.ra = Some(ra);
    out.rb = Some(rb);
    out.sign = Some(or1(js_sign(crs(sa, sb))));
    out
}

/// `angleTo`: `entities` must be the sketch plus the reference entities (`allE()` in 1.x), so
/// both lines are found. Point the angle's rays at the sector the pointer is in (sets ra, rb, sign).
pub fn angle_to(c: &Constraint, entities: &[Entity], cur: P2) -> Constraint {
    angle_to_w(c, &World::new(entities), cur)
}

const RS: [[&str; 2]; 4] = [["c0", "c1"], ["c1", "c2"], ["c2", "c3"], ["c3", "c0"]];

#[derive(PartialEq, Clone, Copy)]
enum Kind {
    Pt,
    Rs,
    Ln,
    Rd,
    Other,
}

/// `inferDimension`: what to dimension from what was picked. `fixed`: ids that can't move
/// (the origin, the axes, part edges). Returns a constraint without value, `Pending` to wait
/// for another pick, or a message.
pub fn infer_dimension(picks: &[Ref], entities: &[Entity], cursor: Option<P2>, fixed: &BTreeSet<String>) -> Outcome {
    let w = World::new(entities);
    let cur = cursor.unwrap_or([0.0, 0.0]);
    if picks.len() >= 2 && picks.iter().all(|p| fixed.contains(&p.e)) {
        return msg("Pick at least one shape of the sketch: the origin, the axes and the part edges don't move.");
    }
    let zone = |p: P2, q: P2| -> char {
        let (dx, dy) = ((q[0] - p[0]).abs(), (q[1] - p[1]).abs());
        let l = or1(super::constraints::hypot(&[dx, dy]));
        if dy / l < 1e-6 {
            return 'h';
        }
        if dx / l < 1e-6 {
            return 'v';
        }
        let in_x = cur[0] > js_min(p[0], q[0]) && cur[0] < js_max(p[0], q[0]);
        let in_y = cur[1] > js_min(p[1], q[1]) && cur[1] < js_max(p[1], q[1]);
        if in_x && !in_y {
            'h'
        } else if in_y && !in_x {
            'v'
        } else {
            'a'
        }
    };
    let linear = |a: Ref, b: Ref, p: P2, q: P2| -> Constraint {
        match zone(p, q) {
            'h' => signed("hdistance", a, b, js_sign(q[0] - p[0])),
            'v' => signed("vdistance", a, b, js_sign(q[1] - p[1])),
            _ => dim("distance", a, Some(b)),
        }
    };
    let kind = |p: &Ref| -> Option<Kind> {
        let e = w.get(&p.e)?;
        if p.has_k() {
            return Some(Kind::Pt);
        }
        Some(match e {
            Entity::Rect(_) if p.side.is_some() => Kind::Rs,
            Entity::Line(_) => Kind::Ln,
            Entity::Circle(_) | Entity::Arc(_) => Kind::Rd,
            Entity::Point(_) => Kind::Pt,
            _ => Kind::Other,
        })
    };
    let pt_of = |p: &Ref| -> Option<Ref> {
        let e = w.get(&p.e)?;
        if p.has_k() {
            return Some(p.clone());
        }
        match e {
            Entity::Point(_) => Some(Ref::pt(&p.e, "p")),
            Entity::Circle(_) | Entity::Arc(_) => Some(Ref::pt(&p.e, "c")),
            _ => None,
        }
    };
    let rs_keys = |p: &Ref| -> [&'static str; 2] { RS[(p.side.unwrap_or(0).rem_euclid(4)) as usize] };
    let side_even = |p: &Ref| p.side.unwrap_or(0) % 2 == 0;
    let Some(p) = picks.first() else { return Outcome::Pending };

    if picks.len() == 1 {
        let Some(e) = w.get(&p.e) else { return Outcome::Pending };
        let k = kind(p);
        if k == Some(Kind::Pt) || fixed.contains(&p.e) {
            return Outcome::Pending;
        }
        if k == Some(Kind::Ln) {
            let (p1, p2) = line_ends(Some(e));
            let o = zone(p1, p2);
            if o == 'h' && (p2[1] - p1[1]).abs() > 1e-9 {
                return Outcome::Constraint(signed("hdistance", Ref::pt(&p.e, "p1"), Ref::pt(&p.e, "p2"), js_sign(p2[0] - p1[0])));
            }
            if o == 'v' && (p2[0] - p1[0]).abs() > 1e-9 {
                return Outcome::Constraint(signed("vdistance", Ref::pt(&p.e, "p1"), Ref::pt(&p.e, "p2"), js_sign(p2[1] - p1[1])));
            }
            return Outcome::Constraint(dim("length", Ref::ent(&p.e), None));
        }
        if k == Some(Kind::Rs) {
            let [k1, k2] = rs_keys(p);
            let a = point(e, k1).unwrap_or(NAN2);
            let b = point(e, k2).unwrap_or(NAN2);
            let horiz = side_even(p);
            let s = js_sign(if horiz { b[0] - a[0] } else { b[1] - a[1] });
            return Outcome::Constraint(signed(
                if horiz { "hdistance" } else { "vdistance" },
                Ref::pt(&p.e, k1),
                Ref::pt(&p.e, k2),
                s,
            ));
        }
        return match e {
            Entity::Circle(_) => Outcome::Constraint(dim("diameter", Ref::ent(&p.e), None)),
            Entity::Arc(_) | Entity::Polygon(_) => Outcome::Constraint(dim("radius", Ref::ent(&p.e), None)),
            Entity::Slot(_) => Outcome::Constraint(dim("length", Ref::ent(&p.e), None)),
            Entity::Rect(_) => msg("Click one edge of the rectangle."),
            _ => msg("That shape can't take a dimension: pick lines, circles, arcs or points."),
        };
    }

    let q = &picks[1];
    if p.e == q.e && p.k == q.k && p.side == q.side {
        return msg("That's the same item twice.");
    }
    let (ka, kb) = (kind(p), kind(q));
    let is_pt = |k: Option<Kind>| matches!(k, Some(Kind::Pt) | Some(Kind::Rd));
    let is_ln = |k: Option<Kind>| matches!(k, Some(Kind::Ln) | Some(Kind::Rs));
    if is_pt(ka) && is_pt(kb) {
        let (Some(pa), Some(pb)) = (pt_of(p), pt_of(q)) else { return msg("Pick a line, circle or arc; or two points; a point and a line; or two lines.") };
        let (a, b) = (pk(&w, &pa), pk(&w, &pb));
        if super::constraints::hypot(&[b[0] - a[0], b[1] - a[1]]) < 1e-9 {
            return msg("Those two points are in the same place.");
        }
        return Outcome::Constraint(linear(pa, pb, a, b));
    }
    let line_dir = |r: &Ref| -> P2 {
        let e = w.get(&r.e);
        if kind(r) == Some(Kind::Ln) {
            return dir(e);
        }
        let [k1, k2] = rs_keys(r);
        let g = |k: &str| e.and_then(|e| point(e, k)).unwrap_or(NAN2);
        sub(g(k2), g(k1))
    };
    let pt_line = |pt: &Ref, ln: &Ref| -> Result<Constraint, String> {
        if kind(ln) == Some(Kind::Rs) {
            let e = w.get(&ln.e);
            let horiz = side_even(ln);
            let k1 = rs_keys(ln)[0];
            let a = pk(&w, pt);
            let b = e.and_then(|e| point(e, k1)).unwrap_or(NAN2);
            let s = js_sign(if horiz { b[1] - a[1] } else { b[0] - a[0] });
            return Ok(signed(if horiz { "vdistance" } else { "hdistance" }, pt.clone(), Ref::pt(&ln.e, k1), s));
        }
        let l = w.get(&ln.e);
        let d = dir(l);
        if crs(sub(pk(&w, pt), line_ends(l).0), d).abs() / or1(len(d)) < 1e-9 {
            return Err("That point is on the line.".to_string());
        }
        Ok(dim("distance", pt.clone(), Some(Ref::ent(&ln.e))))
    };
    let res = |r: Result<Constraint, String>| match r {
        Ok(c) => Outcome::Constraint(c),
        Err(m) => Outcome::Message(m),
    };
    if is_pt(ka) && is_ln(kb) {
        return match pt_of(p) {
            Some(pp) => res(pt_line(&pp, q)),
            None => msg("Pick a line, circle or arc; or two points; a point and a line; or two lines."),
        };
    }
    if is_ln(ka) && is_pt(kb) {
        return match pt_of(q) {
            Some(pp) => res(pt_line(&pp, p)),
            None => msg("Pick a line, circle or arc; or two points; a point and a line; or two lines."),
        };
    }
    if is_ln(ka) && is_ln(kb) {
        let (a, b) = (line_dir(p), line_dir(q));
        let par = crs(a, b).abs() / or1(len(a) * len(b)) < 0.0175;
        if par {
            // the distance between them: from a point of one (one that can move) to the other
            let (m, o) = if fixed.contains(&p.e) { (q, p) } else { (p, q) };
            let mp = if kind(m) == Some(Kind::Ln) { Ref::pt(&m.e, "m") } else { Ref::pt(&m.e, rs_keys(m)[0]) };
            return match pt_line(&mp, o) {
                Ok(c) => Outcome::Constraint(c),
                Err(_) => msg("Those lines are on top of each other."),
            };
        }
        if ka == Some(Kind::Rs) || kb == Some(Kind::Rs) {
            return msg("Angles to a rectangle's sides can't be set: its sides are always square. Draw it with lines to angle it.");
        }
        let c = dim("angle", Ref::ent(&p.e), Some(Ref::ent(&q.e)));
        return Outcome::Constraint(angle_to_w(&c, &w, cur));
    }
    let _ = Kind::Other;
    msg("Pick a line, circle or arc; or two points; a point and a line; or two lines.")
}
