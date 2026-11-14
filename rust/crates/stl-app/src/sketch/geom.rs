//! Entity geometry, function for function from `web/js/sketchgeom.js`.

use std::f64::consts::PI;

use stl_sketch::entities::{Arc, Entity};

type P = [f64; 2];

fn rad(deg: f64) -> f64 {
    deg * PI / 180.0
}

/// JS `Math.round` (halves go up).
fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// 2D outline of an entity and whether it is closed (`entityOutline`'s `{pts, closed}`).
pub fn outline(e: &Entity) -> (Vec<P>, bool) {
    match e {
        Entity::Rect(r) => {
            let (x0, x1) = (r.x.min(r.x + r.w), r.x.max(r.x + r.w));
            let (y0, y1) = (r.y.min(r.y + r.h), r.y.max(r.y + r.h));
            let r0 = r.fillet.unwrap_or(0.0);
            let rr = r0.min((x1 - x0) / 2.0 - 1e-6).min((y1 - y0) / 2.0 - 1e-6);
            let mut pts: Vec<P>;
            if rr > 1e-6 {
                pts = Vec::new();
                let n = 10;
                for (cx, cy, a0) in [(x1 - rr, y0 + rr, -90.0), (x1 - rr, y1 - rr, 0.0), (x0 + rr, y1 - rr, 90.0), (x0 + rr, y0 + rr, 180.0)] {
                    for i in 0..=n {
                        let a = rad(a0 + 90.0 * i as f64 / n as f64);
                        pts.push([cx + rr * a.cos(), cy + rr * a.sin()]);
                    }
                }
            } else {
                pts = vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
            }
            let ang = r.angle.unwrap_or(0.0);
            if ang != 0.0 {
                let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
                let a = rad(ang);
                let (c, s) = (a.cos(), a.sin());
                for p in &mut pts {
                    let (px, py) = (p[0], p[1]);
                    *p = [cx + (px - cx) * c - (py - cy) * s, cy + (px - cx) * s + (py - cy) * c];
                }
            }
            (pts, true)
        }
        Entity::Circle(c) => {
            let n = 96;
            ((0..n).map(|i| {
                let a = 2.0 * PI * i as f64 / n as f64;
                [c.cx + c.r * a.cos(), c.cy + c.r * a.sin()]
            }).collect(), true)
        }
        Entity::Polygon(p) => {
            let n = p.sides().max(3);
            let a0 = rad(p.angle.unwrap_or(0.0));
            ((0..n).map(|i| {
                let a = a0 + 2.0 * PI * i as f64 / n as f64;
                [p.cx + p.r * a.cos(), p.cy + p.r * a.sin()]
            }).collect(), true)
        }
        Entity::Slot(s) => {
            let ang = (s.y2 - s.y1).atan2(s.x2 - s.x1);
            let n = 24;
            let mut pts = Vec::new();
            for i in 0..=n {
                let a = ang - PI / 2.0 + PI * i as f64 / n as f64;
                pts.push([s.x2 + s.r * a.cos(), s.y2 + s.r * a.sin()]);
            }
            for i in 0..=n {
                let a = ang + PI / 2.0 + PI * i as f64 / n as f64;
                pts.push([s.x1 + s.r * a.cos(), s.y1 + s.r * a.sin()]);
            }
            (pts, true)
        }
        Entity::Polyline(p) => (p.pts.clone(), p.is_closed()),
        Entity::Line(l) => (vec![l.p1, l.p2], false),
        Entity::Arc(_) => (arc_pts(e, 0), false),
        Entity::Point(_) => (Vec::new(), false),
    }
}

/// `entityOutline(e).pts`.
pub fn entity_outline(e: &Entity) -> Vec<P> {
    outline(e).0
}

fn arc_fields(e: &Entity) -> Option<&Arc> {
    if let Entity::Arc(a) = e { Some(a) } else { None }
}

/// Degrees counter-clockwise from `a0` to `a1`; a full turn when they coincide.
pub fn arc_sweep(e: &Entity) -> f64 {
    let Some(a) = arc_fields(e) else { return 0.0 };
    let s = ((a.a1 - a.a0) % 360.0 + 360.0) % 360.0;
    if s < 1e-9 { 360.0 } else { s }
}

/// `arcPts(e, n)`; `n == 0` is the default (`max(8, round(sweep / 4))`).
pub fn arc_pts(e: &Entity, n: usize) -> Vec<P> {
    let Some(a) = arc_fields(e) else { return Vec::new() };
    let sw = arc_sweep(e);
    let n = if n == 0 { 8.0_f64.max(js_round(sw / 4.0)) as usize } else { n };
    (0..=n).map(|i| {
        let t = rad(a.a0 + sw * i as f64 / n as f64);
        [a.cx + a.r * t.cos(), a.cy + a.r * t.sin()]
    }).collect()
}

/// The circle through three points as an arc from `p1` to `p3` passing `p2` (None if collinear).
pub fn arc_from3(p1: P, p2: P, p3: P) -> Option<Entity> {
    let ([ax, ay], [bx, by], [cx, cy]) = (p1, p2, p3);
    let d = 2.0 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by));
    if d.abs() < 1e-12 {
        return None;
    }
    let ux = ((ax * ax + ay * ay) * (by - cy) + (bx * bx + by * by) * (cy - ay) + (cx * cx + cy * cy) * (ay - by)) / d;
    let uy = ((ax * ax + ay * ay) * (cx - bx) + (bx * bx + by * by) * (ax - cx) + (cx * cx + cy * cy) * (bx - ax)) / d;
    let r = (ax - ux).hypot(ay - uy);
    let ang = |p: P| (p[1] - uy).atan2(p[0] - ux) * 180.0 / PI;
    let (mut a0, mut a1, am) = (ang(p1), ang(p3), ang(p2));
    let m360 = |v: f64| ((v % 360.0) + 360.0) % 360.0;
    let ccw = |x: f64, from: f64, to: f64| m360(x - from) <= m360(to - from);
    if !ccw(am, a0, a1) {
        std::mem::swap(&mut a0, &mut a1);
    }
    Some(Entity::Arc(Arc { id: None, cx: ux, cy: uy, r, a0, a1, construction: None, extra: Default::default() }))
}

/// Snap points of an entity, each with a name (`keyPoints`; the names are this port's).
pub fn key_points(e: &Entity) -> Vec<(String, P)> {
    let n = |k: &str, p: P| (k.to_string(), p);
    match e {
        Entity::Rect(r) => vec![
            n("c0", [r.x, r.y]), n("c1", [r.x + r.w, r.y]), n("c2", [r.x + r.w, r.y + r.h]), n("c3", [r.x, r.y + r.h]),
            n("c", [r.x + r.w / 2.0, r.y + r.h / 2.0]),
            n("mb", [r.x + r.w / 2.0, r.y]), n("mr", [r.x + r.w, r.y + r.h / 2.0]),
            n("mt", [r.x + r.w / 2.0, r.y + r.h]), n("ml", [r.x, r.y + r.h / 2.0]),
        ],
        Entity::Circle(c) => vec![
            n("c", [c.cx, c.cy]), n("e", [c.cx + c.r, c.cy]), n("w", [c.cx - c.r, c.cy]),
            n("n", [c.cx, c.cy + c.r]), n("s", [c.cx, c.cy - c.r]),
        ],
        Entity::Polygon(p) => {
            let mut v = vec![n("c", [p.cx, p.cy])];
            v.extend(entity_outline(e).into_iter().enumerate().map(|(i, q)| (format!("v{i}"), q)));
            v
        }
        Entity::Slot(s) => vec![n("s", [s.x1, s.y1]), n("e", [s.x2, s.y2])],
        Entity::Line(l) => vec![n("p1", l.p1), n("p2", l.p2), n("m", [(l.p1[0] + l.p2[0]) / 2.0, (l.p1[1] + l.p2[1]) / 2.0])],
        Entity::Arc(a) => {
            let p = arc_pts(e, 2);
            vec![n("s", p[0]), n("e", p[2]), n("m", p[1]), n("c", [a.cx, a.cy])]
        }
        Entity::Polyline(pl) => {
            let p = &pl.pts;
            let mut v: Vec<(String, P)> = p.iter().enumerate().map(|(i, q)| (format!("v{i}"), *q)).collect();
            for i in 0..p.len().saturating_sub(1) {
                v.push((format!("m{i}"), [(p[i][0] + p[i + 1][0]) / 2.0, (p[i][1] + p[i + 1][1]) / 2.0]));
            }
            v
        }
        Entity::Point(_) => Vec::new(),
    }
}

/// Distance from a point to a segment.
pub fn seg_dist(p: P, a: P, b: P) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l = dx * dx + dy * dy;
    let t = if l != 0.0 { ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l } else { 0.0 };
    let t = t.clamp(0.0, 1.0);
    (p[0] - (a[0] + t * dx)).hypot(p[1] - (a[1] + t * dy))
}

/// Distance from a point to an entity's outline (picking); infinity for an empty outline.
pub fn dist_to_entity(e: &Entity, p: P) -> f64 {
    let (pts, closed) = outline(e);
    let n = pts.len();
    let mut best = f64::INFINITY;
    let segs = if closed { n } else { n.saturating_sub(1) };
    for i in 0..segs {
        best = best.min(seg_dist(p, pts[i], pts[(i + 1) % n]));
    }
    if n == 1 {
        best = (p[0] - pts[0][0]).hypot(p[1] - pts[0][1]);
    }
    best
}

/// `ENTITY_LABEL`.
pub fn label(e: &Entity) -> &'static str {
    match e {
        Entity::Rect(_) => "Rectangle",
        Entity::Circle(_) => "Circle",
        Entity::Polygon(_) => "Polygon",
        Entity::Slot(_) => "Slot",
        Entity::Polyline(_) => "Polyline",
        Entity::Line(_) => "Line",
        Entity::Arc(_) => "Arc",
        Entity::Point(_) => "Point",
    }
}

/// JS `(+v).toFixed(2)`: exact decimal value, halves away from zero.
pub fn to_fixed2(v: f64) -> String {
    if !v.is_finite() {
        return if v.is_nan() { "NaN".into() } else if v > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    let a = v.abs();
    let exact = format!("{a:.40}");
    let (int, frac) = exact.split_once('.').unwrap_or((&exact, ""));
    let digits = format!("{int}{}", &frac[..2]);
    let mut n: u128 = digits.parse().unwrap_or(0);
    let rest = &frac[2..];
    if rest.starts_with('5') || rest.as_bytes().first().is_some_and(|c| *c > b'5') {
        n += 1; // 5 then anything: the half goes up; above half is more than half
    }
    let s = format!("{}.{:02}", n / 100, n % 100);
    if v < 0.0 && n != 0 { format!("-{s}") } else { s }
}

/// The 1.x `describe` number: `toFixed(2)` without trailing zeros.
fn f(v: f64) -> String {
    let s = to_fixed2(v);
    if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s }
}

pub fn describe(e: &Entity) -> String {
    match e {
        Entity::Rect(r) => format!("{} × {}", f(r.w.abs()), f(r.h.abs())),
        Entity::Circle(c) => format!("Ø{}", f(c.r * 2.0)),
        Entity::Polygon(p) => format!("{} sides, R{}", js_int(p.n), f(p.r)),
        Entity::Slot(s) => format!("L{} W{}", f((s.x2 - s.x1).hypot(s.y2 - s.y1)), f(s.r * 2.0)),
        Entity::Polyline(p) => format!("{} pts{}", p.pts.len(), if p.is_closed() { ", closed" } else { "" }),
        Entity::Line(l) => format!("L{}", f((l.p2[0] - l.p1[0]).hypot(l.p2[1] - l.p1[1]))),
        Entity::Arc(a) => format!("R{}, {}°", f(a.r), f(arc_sweep(e))),
        Entity::Point(_) => String::new(),
    }
}

/// A number as JS prints it in a template (`${e.n}`): whole numbers without a decimal point.
fn js_int(v: f64) -> String {
    format!("{v}")
}

/// One editable dimension: `{key, value, label, at, side}`.
#[derive(Clone, Debug, PartialEq)]
pub struct DimItem {
    pub key: String,
    pub value: f64,
    pub label: String,
    pub at: P,
    pub side: Option<&'static str>,
}

/// `Math.round(v * 100) / 100` printed as JS prints it.
fn g(v: f64) -> String {
    let r = js_round(v * 100.0) / 100.0;
    if r == 0.0 { "0".into() } else { format!("{r}") }
}

/// `dimensions(e)` with labels and positions.
pub fn dimension_items(e: &Entity) -> Vec<DimItem> {
    let item = |key: &str, value: f64, label: String, at: P, side| DimItem { key: key.into(), value, label, at, side };
    match e {
        Entity::Rect(r) => vec![
            item("w", r.w.abs(), g(r.w.abs()), [r.x + r.w / 2.0, r.y.min(r.y + r.h)], Some("below")),
            item("h", r.h.abs(), g(r.h.abs()), [r.x.max(r.x + r.w), r.y + r.h / 2.0], Some("right")),
        ],
        Entity::Circle(c) => vec![item("d", c.r * 2.0, format!("⌀{}", g(c.r * 2.0)), [c.cx + c.r * 0.7071, c.cy + c.r * 0.7071], None)],
        Entity::Arc(a) => {
            let m = arc_pts(e, 2)[1];
            vec![item("r", a.r, format!("R{}", g(a.r)), m, None)]
        }
        Entity::Polygon(p) => vec![item("r", p.r, format!("R{}", g(p.r)), [p.cx + p.r, p.cy], None)],
        Entity::Slot(s) => {
            let len = (s.x2 - s.x1).hypot(s.y2 - s.y1);
            vec![
                item("len", len, g(len), [(s.x1 + s.x2) / 2.0, (s.y1 + s.y2) / 2.0], None),
                item("w", s.r * 2.0, format!("⌀{}", g(s.r * 2.0)), [s.x2 + s.r, s.y2], None),
            ]
        }
        Entity::Line(l) => {
            let len = (l.p2[0] - l.p1[0]).hypot(l.p2[1] - l.p1[1]);
            vec![item("len", len, g(len), [(l.p1[0] + l.p2[0]) / 2.0, (l.p1[1] + l.p2[1]) / 2.0], None)]
        }
        _ => Vec::new(),
    }
}

/// The editable dimensions as `(key, value)`.
pub fn dimensions(e: &Entity) -> Vec<(String, f64)> {
    dimension_items(e).into_iter().map(|d| (d.key, d.value)).collect()
}

/// Apply an edited dimension, keeping the shape's anchor point fixed (`applyDimension`).
/// Nothing changes for `v <= 0` or a key the shape doesn't have.
pub fn apply_dimension(e: &mut Entity, key: &str, v: f64) {
    if !(v > 0.0) {
        return;
    }
    let sign = |x: f64| if x == 0.0 || x.is_nan() { 1.0 } else { x.signum() };
    match e {
        Entity::Rect(r) => {
            if key == "w" { r.w = sign(r.w) * v } else { r.h = sign(r.h) * v }
        }
        Entity::Circle(c) => c.r = v / 2.0,
        Entity::Arc(a) => a.r = v,
        Entity::Polygon(p) => p.r = v,
        Entity::Slot(s) => {
            if key == "w" {
                s.r = v / 2.0;
            } else {
                let l = (s.x2 - s.x1).hypot(s.y2 - s.y1);
                let k = v / if l == 0.0 { 1.0 } else { l };
                (s.x2, s.y2) = (s.x1 + (s.x2 - s.x1) * k, s.y1 + (s.y2 - s.y1) * k);
            }
        }
        Entity::Line(l) => {
            let len = (l.p2[0] - l.p1[0]).hypot(l.p2[1] - l.p1[1]);
            let k = v / if len == 0.0 { 1.0 } else { len };
            l.p2 = [l.p1[0] + (l.p2[0] - l.p1[0]) * k, l.p1[1] + (l.p2[1] - l.p1[1]) * k];
        }
        _ => {}
    }
}
