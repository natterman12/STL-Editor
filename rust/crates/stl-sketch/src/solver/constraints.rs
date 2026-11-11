//! Constraint kinds and residuals (`constraints.js`: PARAMS, residual, measure, refsExist,
//! glyphSpots) plus the JS number helpers the port needs.

use crate::entities::{point, point_keys, Entity, P2};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;

pub const D2R: f64 = std::f64::consts::PI / 180.0;
const NAN2: P2 = [f64::NAN, f64::NAN];

// ------------------------------------------------------------------ JS number semantics

/// `Math.hypot(...v)` as V8 computes it (scaled sum of squares with Kahan compensation).
pub fn hypot(v: &[f64]) -> f64 {
    let mut max = 0.0_f64;
    let mut nan = false;
    for &x in v {
        let a = x.abs();
        if a == f64::INFINITY {
            return f64::INFINITY;
        }
        if a.is_nan() {
            nan = true;
        } else if a > max {
            max = a;
        }
    }
    if nan {
        return f64::NAN;
    }
    if max == 0.0 {
        return 0.0;
    }
    let (mut sum, mut comp) = (0.0_f64, 0.0_f64);
    for &x in v {
        let n = x.abs() / max;
        let summand = n * n - comp;
        let prelim = sum + summand;
        comp = (prelim - sum) - summand;
        sum = prelim;
    }
    sum.sqrt() * max
}

/// `x || 1` for a number: 0 and NaN become 1.
pub fn or1(x: f64) -> f64 {
    if x == 0.0 || x.is_nan() {
        1.0
    } else {
        x
    }
}

/// `Math.sign` (NaN stays NaN, 0 stays 0).
pub fn js_sign(x: f64) -> f64 {
    if x.is_nan() || x == 0.0 {
        x
    } else if x > 0.0 {
        1.0
    } else {
        -1.0
    }
}

/// `Math.max(a, b)`: NaN propagates.
pub fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}

/// `Math.min(a, b)`: NaN propagates.
pub fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

pub fn sub(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}
pub fn dot(a: P2, b: P2) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
pub fn crs(a: P2, b: P2) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
pub fn len(a: P2) -> f64 {
    hypot(&[a[0], a[1]])
}
fn wrap(mut a: f64) -> f64 {
    let pi = std::f64::consts::PI;
    while a > pi {
        a -= 2.0 * pi;
    }
    while a < -pi {
        a += 2.0 * pi;
    }
    a
}

// ------------------------------------------------------------------ entity access

pub fn is_line(e: Option<&Entity>) -> bool {
    matches!(e, Some(Entity::Line(_)))
}
pub fn is_round(e: Option<&Entity>) -> bool {
    matches!(e, Some(Entity::Circle(_)) | Some(Entity::Arc(_)))
}
/// Line ends (NaN for anything that is not a line; the JS would throw).
pub fn line_ends(e: Option<&Entity>) -> (P2, P2) {
    match e {
        Some(Entity::Line(l)) => (l.p1, l.p2),
        _ => (NAN2, NAN2),
    }
}
pub fn dir(e: Option<&Entity>) -> P2 {
    let (p1, p2) = line_ends(e);
    sub(p2, p1)
}
pub fn center(e: Option<&Entity>) -> P2 {
    match e {
        Some(Entity::Arc(a)) => [a.cx, a.cy],
        Some(Entity::Circle(c)) => [c.cx, c.cy],
        Some(Entity::Polygon(p)) => [p.cx, p.cy],
        _ => NAN2,
    }
}
pub fn radius(e: Option<&Entity>) -> f64 {
    match e {
        Some(Entity::Arc(a)) => a.r,
        Some(Entity::Circle(c)) => c.r,
        Some(Entity::Polygon(p)) => p.r,
        Some(Entity::Slot(s)) => s.r,
        _ => f64::NAN,
    }
}

/// `paramsOf`: the names of an entity's numeric parameters, in the 1.x order.
pub fn params_of(e: &Entity) -> Vec<String> {
    let s = |v: &[&str]| v.iter().map(|k| k.to_string()).collect();
    match e {
        Entity::Line(_) => s(&["p1.0", "p1.1", "p2.0", "p2.1"]),
        Entity::Arc(_) => s(&["cx", "cy", "r", "a0", "a1"]),
        Entity::Circle(_) => s(&["cx", "cy", "r"]),
        Entity::Rect(_) => s(&["x", "y", "w", "h"]),
        Entity::Polygon(_) => s(&["cx", "cy", "r"]),
        Entity::Slot(_) => s(&["x1", "y1", "x2", "y2", "r"]),
        Entity::Point(_) => s(&["x", "y"]),
        Entity::Polyline(p) => (0..p.pts.len()).flat_map(|i| [format!("pts.{i}.0"), format!("pts.{i}.1")]).collect(),
    }
}

/// `getP` by parameter index (the index into `params_of`). NaN when out of range.
pub fn get_param(e: &Entity, i: usize) -> f64 {
    match e {
        Entity::Line(l) => [l.p1[0], l.p1[1], l.p2[0], l.p2[1]].get(i).copied().unwrap_or(f64::NAN),
        Entity::Arc(a) => [a.cx, a.cy, a.r, a.a0, a.a1].get(i).copied().unwrap_or(f64::NAN),
        Entity::Circle(c) => [c.cx, c.cy, c.r].get(i).copied().unwrap_or(f64::NAN),
        Entity::Rect(r) => [r.x, r.y, r.w, r.h].get(i).copied().unwrap_or(f64::NAN),
        Entity::Polygon(p) => [p.cx, p.cy, p.r].get(i).copied().unwrap_or(f64::NAN),
        Entity::Slot(s) => [s.x1, s.y1, s.x2, s.y2, s.r].get(i).copied().unwrap_or(f64::NAN),
        Entity::Point(p) => [p.x, p.y].get(i).copied().unwrap_or(f64::NAN),
        Entity::Polyline(p) => p.pts.get(i / 2).map(|pt| pt[i % 2]).unwrap_or(f64::NAN),
    }
}

/// `setP` by parameter index. Out of range is ignored.
pub fn set_param(e: &mut Entity, i: usize, v: f64) {
    match e {
        Entity::Line(l) => match i {
            0 => l.p1[0] = v,
            1 => l.p1[1] = v,
            2 => l.p2[0] = v,
            3 => l.p2[1] = v,
            _ => {}
        },
        Entity::Arc(a) => match i {
            0 => a.cx = v,
            1 => a.cy = v,
            2 => a.r = v,
            3 => a.a0 = v,
            4 => a.a1 = v,
            _ => {}
        },
        Entity::Circle(c) => match i {
            0 => c.cx = v,
            1 => c.cy = v,
            2 => c.r = v,
            _ => {}
        },
        Entity::Rect(r) => match i {
            0 => r.x = v,
            1 => r.y = v,
            2 => r.w = v,
            3 => r.h = v,
            _ => {}
        },
        Entity::Polygon(p) => match i {
            0 => p.cx = v,
            1 => p.cy = v,
            2 => p.r = v,
            _ => {}
        },
        Entity::Slot(s) => match i {
            0 => s.x1 = v,
            1 => s.y1 = v,
            2 => s.x2 = v,
            3 => s.y2 = v,
            4 => s.r = v,
            _ => {}
        },
        Entity::Point(p) => match i {
            0 => p.x = v,
            1 => p.y = v,
            _ => {}
        },
        Entity::Polyline(p) => {
            if let Some(pt) = p.pts.get_mut(i / 2) {
                pt[i % 2] = v;
            }
        }
    }
}

/// `getP(e, path)` with a path name such as `"r"`, `"p1.0"`, `"angle"` or `"n"`: any numeric
/// property of the entity (named fields and numbers kept in `extra`). NaN when it is missing
/// or not a number, as the JS arithmetic on `undefined` gives.
pub fn get_path(e: &Entity, path: &str) -> f64 {
    if let Some(i) = params_of(e).iter().position(|p| p == path) {
        return get_param(e, i);
    }
    let Ok(mut v) = serde_json::to_value(e) else { return f64::NAN };
    for k in path.split('.') {
        let next = match &v {
            Value::Object(o) => o.get(k).cloned(),
            Value::Array(a) => k.parse::<usize>().ok().and_then(|i| a.get(i).cloned()),
            _ => None,
        };
        match next {
            Some(n) => v = n,
            None => return f64::NAN,
        }
    }
    v.as_f64().unwrap_or(f64::NAN)
}

// ------------------------------------------------------------------ constraints

/// A reference to an entity, or to a point on it (`k`). Picks may also carry a rectangle `side`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ref {
    pub e: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub k: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<i64>,
}

impl Ref {
    pub fn ent(e: &str) -> Ref {
        Ref { e: e.to_string(), k: None, side: None }
    }
    pub fn pt(e: &str, k: &str) -> Ref {
        Ref { e: e.to_string(), k: Some(k.to_string()), side: None }
    }
    /// JS truthiness of `r.k` (undefined and "" are false).
    pub fn has_k(&self) -> bool {
        self.k.as_deref().is_some_and(|k| !k.is_empty())
    }
}

/// One constraint, in the same JSON as 1.x.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Constraint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a: Option<Ref>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b: Option<Ref>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub c: Option<Ref>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sign: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ra: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rb: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Vec<f64>>,
    /// Constraints with a truthy `ref` belong to something else (a part edge) and are skipped.
    #[serde(default, rename = "ref", skip_serializing_if = "Option::is_none")]
    pub ref_: Option<Value>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Constraint {
    pub fn new(kind: &str) -> Constraint {
        Constraint {
            id: None,
            kind: kind.to_string(),
            a: None,
            b: None,
            c: None,
            value: None,
            sign: None,
            ra: None,
            rb: None,
            inner: None,
            at: None,
            ref_: None,
            extra: Map::new(),
        }
    }
    pub fn with(mut self, a: Option<Ref>, b: Option<Ref>) -> Constraint {
        self.a = a;
        self.b = b;
        self
    }
    /// JS truthiness of `c.ref`.
    pub fn has_ref(&self) -> bool {
        match &self.ref_ {
            None | Some(Value::Null) => false,
            Some(Value::Bool(b)) => *b,
            Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
            Some(Value::String(s)) => !s.is_empty(),
            Some(_) => true,
        }
    }
}

/// The entities by id (later duplicates win, as `Object.fromEntries` does). Entities without an
/// id are skipped (1.x always gives every entity an id).
#[derive(Debug, Clone)]
pub struct World {
    pub ents: Vec<Entity>,
    idx: HashMap<String, usize>,
}

impl World {
    pub fn new(entities: &[Entity]) -> World {
        let mut w = World { ents: Vec::new(), idx: HashMap::new() };
        for e in entities {
            if let Some(id) = e.id() {
                match w.idx.get(id) {
                    Some(&i) => w.ents[i] = e.clone(),
                    None => {
                        w.idx.insert(id.to_string(), w.ents.len());
                        w.ents.push(e.clone());
                    }
                }
            }
        }
        w
    }
    /// `for (const f of fixed) if (!E[f.id]) E[f.id] = f;`
    pub fn add_missing(&mut self, fixed: &[Entity]) {
        for f in fixed {
            if let Some(id) = f.id() {
                if !self.idx.contains_key(id) {
                    self.idx.insert(id.to_string(), self.ents.len());
                    self.ents.push(f.clone());
                }
            }
        }
    }
    pub fn get(&self, id: &str) -> Option<&Entity> {
        self.idx.get(id).map(|&i| &self.ents[i])
    }
    pub fn index(&self, id: &str) -> Option<usize> {
        self.idx.get(id).copied()
    }
}

/// `refsExist`.
pub fn refs_exist(c: &Constraint, w: &World) -> bool {
    for r in [&c.a, &c.b, &c.c].into_iter().flatten() {
        let Some(e) = w.get(&r.e) else { return false };
        if r.has_k() && !point_keys(e).iter().any(|k| Some(k.as_str()) == r.k.as_deref()) {
            return false;
        }
    }
    true
}

/// `point(E[r.e], r.k)`, NaN where the JS would give `undefined`.
fn pt(w: &World, r: Option<&Ref>) -> P2 {
    let Some(r) = r else { return NAN2 };
    match w.get(&r.e) {
        Some(e) => point(e, r.k.as_deref().unwrap_or("")).unwrap_or(NAN2),
        None => NAN2,
    }
}

fn pt_of(e: Option<&Entity>, k: &str) -> P2 {
    e.and_then(|e| point(e, k)).unwrap_or(NAN2)
}

/// Residuals of one constraint (numbers that should all be 0), or `None` (the JS `null`).
pub fn residual(c: &Constraint, w: &World) -> Option<Vec<f64>> {
    let ent = |r: &Option<Ref>| r.as_ref().and_then(|r| w.get(&r.e));
    let a_ref = c.a.as_ref();
    let b_ref = c.b.as_ref();
    let ea = ent(&c.a);
    let eb = ent(&c.b);
    ea?;
    if c.b.is_some() && eb.is_none() {
        return None;
    }
    if c.c.as_ref().is_some_and(|r| w.get(&r.e).is_none()) {
        return None;
    }
    let value = c.value.unwrap_or(f64::NAN);
    let (la, lb) = (line_ends(ea), line_ends(eb));
    match c.kind.as_str() {
        "coincident" => {
            let d = sub(pt(w, a_ref), pt(w, b_ref));
            Some(vec![d[0], d[1]])
        }
        "onentity" => {
            let p = pt(w, a_ref);
            if is_line(eb) {
                let d = dir(eb);
                return Some(vec![crs(sub(p, lb.0), d) / or1(len(d))]);
            }
            if is_round(eb) {
                return Some(vec![len(sub(p, center(eb))) - radius(eb)]);
            }
            Some(vec![0.0])
        }
        "horizontal" => Some(vec![if c.b.is_some() { pt(w, a_ref)[1] - pt(w, b_ref)[1] } else { la.1[1] - la.0[1] }]),
        "vertical" => Some(vec![if c.b.is_some() { pt(w, a_ref)[0] - pt(w, b_ref)[0] } else { la.1[0] - la.0[0] }]),
        "parallel" => {
            let (a, b) = (dir(ea), dir(eb));
            Some(vec![crs(a, b) / or1(len(a) * len(b))])
        }
        "perpendicular" => {
            let (a, b) = (dir(ea), dir(eb));
            Some(vec![dot(a, b) / or1(len(a) * len(b))])
        }
        "equal" => {
            if is_line(ea) && is_line(eb) {
                return Some(vec![len(dir(ea)) - len(dir(eb))]);
            }
            if is_round(ea) && is_round(eb) {
                return Some(vec![radius(ea) - radius(eb)]);
            }
            Some(vec![0.0])
        }
        "tangent" => {
            if is_line(ea) && is_round(eb) {
                let d = dir(ea);
                return Some(vec![crs(sub(center(eb), la.0), d).abs() / or1(len(d)) - radius(eb)]);
            }
            if is_round(ea) && is_line(eb) {
                let d = dir(eb);
                return Some(vec![crs(sub(center(ea), lb.0), d).abs() / or1(len(d)) - radius(ea)]);
            }
            if is_round(ea) && is_round(eb) {
                let dd = len(sub(center(ea), center(eb)));
                let (ra, rb) = (radius(ea), radius(eb));
                return Some(vec![if c.inner.unwrap_or(false) { dd - (ra - rb).abs() } else { dd - (ra + rb) }]);
            }
            Some(vec![0.0])
        }
        "concentric" => {
            let d = sub(center(ea), center(eb));
            Some(vec![d[0], d[1]])
        }
        "midpoint" => {
            let p = pt(w, a_ref);
            let m = pt_of(eb, "m");
            Some(vec![p[0] - m[0], p[1] - m[1]])
        }
        "fix" => {
            let at = |i: usize| c.at.as_ref().and_then(|v| v.get(i)).copied().unwrap_or(f64::NAN);
            if c.a.as_ref().is_some_and(|r| r.has_k()) {
                let p = pt(w, a_ref);
                return Some(vec![p[0] - at(0), p[1] - at(1)]);
            }
            let e = ea?;
            Some((0..params_of(e).len()).map(|i| get_param(e, i) - at(i)).collect())
        }
        "symmetric" => {
            let (p, q) = (pt(w, a_ref), pt(w, b_ref));
            let lent = c.c.as_ref().and_then(|r| w.get(&r.e));
            let l = line_ends(lent);
            let d = dir(lent);
            let m = [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0];
            Some(vec![crs(sub(m, l.0), d) / or1(len(d)), dot(sub(q, p), d) / or1(len(d))])
        }
        "distance" => {
            if c.b.as_ref().is_some_and(|r| r.k.is_none()) && is_line(eb) {
                let d = dir(eb);
                return Some(vec![crs(sub(pt(w, a_ref), lb.0), d).abs() / or1(len(d)) - value]);
            }
            Some(vec![len(sub(pt(w, a_ref), pt(w, b_ref))) - value])
        }
        "hdistance" => Some(vec![(pt(w, b_ref)[0] - pt(w, a_ref)[0]) * or1(c.sign.unwrap_or(0.0)) - value]),
        "vdistance" => Some(vec![(pt(w, b_ref)[1] - pt(w, a_ref)[1]) * or1(c.sign.unwrap_or(0.0)) - value]),
        "length" => {
            if is_line(ea) {
                return Some(vec![len(dir(ea)) - value]);
            }
            if let Some(Entity::Slot(s)) = ea {
                return Some(vec![hypot(&[s.x2 - s.x1, s.y2 - s.y1]) - value]);
            }
            Some(vec![0.0])
        }
        "radius" => Some(vec![radius(ea) - value]),
        "diameter" => Some(vec![2.0 * radius(ea) - value]),
        "angle" => {
            let (ra, rb) = (or1(c.ra.unwrap_or(0.0)), or1(c.rb.unwrap_or(0.0)));
            let da = dir(ea);
            let db = dir(eb);
            let a = [da[0] * ra, da[1] * ra];
            let b = [db[0] * rb, db[1] * rb];
            Some(vec![wrap(crs(a, b).atan2(dot(a, b)) - or1(c.sign.unwrap_or(0.0)) * value * D2R)])
        }
        _ => None,
    }
}

/// `measure`: the current value of a dimension (for defaults and labels), `None` when the
/// constraint's entities are missing.
pub fn measure(c: &Constraint, entities: &[Entity]) -> Option<f64> {
    let w = World::new(entities);
    let mut c0 = c.clone();
    c0.value = Some(0.0);
    c0.sign = Some(1.0);
    let r = residual(&c0, &w)?;
    let x = r.first().copied().unwrap_or(f64::NAN);
    if c.kind == "angle" {
        return Some(x.abs() / D2R);
    }
    Some(x.abs())
}

/// `glyphSpots`: where to draw a constraint's glyph(s).
pub fn glyph_spots(c: &Constraint, entities: &[Entity]) -> Vec<P2> {
    let w = World::new(entities);
    let at = |r: &Ref| -> Option<P2> {
        let e = w.get(&r.e)?;
        if r.has_k() {
            return point(e, r.k.as_deref().unwrap());
        }
        match e {
            Entity::Line(_) => point(e, "m"),
            Entity::Arc(a) => {
                let ang = (a.a0 + ((a.a1 - a.a0 + 360.0) % 360.0) / 2.0) * D2R;
                Some([a.cx + a.r * ang.cos(), a.cy + a.r * ang.sin()])
            }
            Entity::Circle(ci) => Some([ci.cx + ci.r * 0.7, ci.cy - ci.r * 0.7]),
            Entity::Polygon(p) => Some([p.cx + p.r * 0.7, p.cy - p.r * 0.7]),
            Entity::Rect(r) => Some([r.x + r.w / 2.0, r.y + r.h / 2.0]),
            Entity::Slot(s) => Some([(s.x1 + s.x2) / 2.0, (s.y1 + s.y2) / 2.0]),
            _ => None,
        }
    };
    [&c.a, &c.b].into_iter().flatten().filter_map(at).collect()
}
