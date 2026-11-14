//! Dimensions (`web/js/dims.js` plus `sketcher.js` `buildDims`, `placeDims`, `dimText`,
//! `dimClass`, `editDimension`, `placeDimension`, `setDimValue`, `setDimRef`, `parseDim`).
//!
//! Where a dimension sits is kept on the constraint, as 1.x: `off` / `slide` (linear: distance
//! from the measured points, and along them; angle: arc radius), `ang` / `out` (radial: direction
//! and how far past the circle the text is). Those keys live in `Constraint::extra`.

use std::cell::RefCell;
use std::collections::BTreeMap;

use egui::{pos2, vec2, Color32, FontId, Rect, Sense, Shape, Stroke, StrokeKind};
use serde_json::Value;
use stl_sketch::entities::{point, P2};
use stl_sketch::solver::{
    angle_to, is_redundant, line_meet, measure, refs_exist, Constraint, Ref, World,
};

use super::solve::{all_ents, fixed_ents};
use super::{Entity, SketchDialog, SketchEffect, SketchSession};
use crate::engine_link::Level;
use crate::tools::fields::number::{expr_of, expr_text, fmt_len, js_num, parse_expr, parse_len};
use crate::viewport::Viewport;

const D2R: f64 = std::f64::consts::PI / 180.0;

pub fn is_linear(kind: &str) -> bool {
    matches!(kind, "hdistance" | "vdistance" | "distance" | "length")
}
pub fn is_radial(kind: &str) -> bool {
    matches!(kind, "radius" | "diameter")
}
/// `CTYPES[c.type].dim`.
pub fn is_dim(kind: &str) -> bool {
    is_linear(kind) || is_radial(kind) || kind == "angle"
}

/// `CTYPES[c.type].label`.
pub fn type_label(kind: &str) -> &'static str {
    match kind {
        "coincident" => "Coincident",
        "onentity" => "Point on curve",
        "horizontal" => "Horizontal",
        "vertical" => "Vertical",
        "parallel" => "Parallel",
        "perpendicular" => "Perpendicular",
        "equal" => "Equal",
        "tangent" => "Tangent",
        "concentric" => "Concentric",
        "midpoint" => "Midpoint",
        "fix" => "Fixed",
        "symmetric" => "Symmetric",
        "distance" => "Distance",
        "hdistance" => "Horizontal distance",
        "vdistance" => "Vertical distance",
        "length" => "Length",
        "radius" => "Radius",
        "diameter" => "Diameter",
        "angle" => "Angle",
        _ => "",
    }
}

// ------------------------------------------------------------------ small vectors

fn sub(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}
fn add(a: P2, b: P2) -> P2 {
    [a[0] + b[0], a[1] + b[1]]
}
fn mul(a: P2, s: f64) -> P2 {
    [a[0] * s, a[1] * s]
}
fn dot(a: P2, b: P2) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn len(a: P2) -> f64 {
    a[0].hypot(a[1])
}
fn unit(a: P2) -> P2 {
    let l = len(a);
    if l > 1e-12 {
        [a[0] / l, a[1] / l]
    } else {
        [1.0, 0.0]
    }
}
/// JS `Math.sign(x) || 1`.
fn sign1(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        1.0
    }
}

// ------------------------------------------------------------------ constraint extras

/// A placement number kept on the constraint (`off`, `slide`, `ang`, `out`).
pub fn get(c: &Constraint, key: &str) -> Option<f64> {
    c.extra.get(key).and_then(Value::as_f64)
}
pub fn set(c: &mut Constraint, key: &str, v: f64) {
    if let Some(n) = serde_json::Number::from_f64(v) {
        c.extra.insert(key.to_string(), Value::Number(n));
    }
}
/// The parameter expression a dimension was typed as ("=width/2").
pub fn expr_of_dim(c: &Constraint) -> Option<&str> {
    c.extra.get("expr").and_then(Value::as_str).filter(|s| !s.is_empty())
}
fn is_ref_dim(c: &Constraint) -> bool {
    c.has_ref()
}

// ------------------------------------------------------------------ geometry (dims.js)

pub struct LinearBase {
    pub p: P2,
    pub q: P2,
    pub u: P2,
    /// Point to line: the line's ends.
    pub line: Option<(P2, P2)>,
}

/// The two measured points and the measuring direction of a linear dimension.
pub fn linear_base(c: &Constraint, w: &World) -> Option<LinearBase> {
    let ar = c.a.as_ref()?;
    let a = w.get(&ar.e)?;
    if c.kind == "length" {
        let (p, q) = match a {
            Entity::Slot(s) => ([s.x1, s.y1], [s.x2, s.y2]),
            Entity::Line(l) => (l.p1, l.p2),
            _ => return None,
        };
        return Some(LinearBase { p, q, u: unit(sub(q, p)), line: None });
    }
    let p = point(a, ar.k.as_deref().unwrap_or(""))?;
    let br = c.b.as_ref()?;
    if c.kind == "distance" && !br.has_k() {
        let Entity::Line(l) = w.get(&br.e)? else { return None };
        let d = unit(sub(l.p2, l.p1));
        let q = add(l.p1, mul(d, dot(sub(p, l.p1), d)));
        let u = if len(sub(q, p)) > 1e-12 { unit(sub(q, p)) } else { [-d[1], d[0]] };
        return Some(LinearBase { p, q, u, line: Some((l.p1, l.p2)) });
    }
    let q = point(w.get(&br.e)?, br.k.as_deref().unwrap_or(""))?;
    let u = match c.kind.as_str() {
        "hdistance" => [1.0, 0.0],
        "vdistance" => [0.0, 1.0],
        _ => unit(sub(q, p)),
    };
    Some(LinearBase { p, q, u, line: None })
}

/// Centre, radius and the entity a radius / diameter dimension measures.
pub fn round_base<'a>(c: &Constraint, w: &'a World) -> Option<(P2, f64, &'a Entity)> {
    let a = w.get(&c.a.as_ref()?.e)?;
    Some(match a {
        Entity::Slot(s) => ([s.x2, s.y2], s.r, a),
        Entity::Arc(x) => ([x.cx, x.cy], x.r, a),
        Entity::Circle(x) => ([x.cx, x.cy], x.r, a),
        Entity::Polygon(x) => ([x.cx, x.cy], x.r, a),
        _ => return None,
    })
}

/// Default placement for a dimension that wasn't placed by hand (typed while drawing):
/// the `(key, value)` pairs to store on the constraint.
pub fn default_placement(c: &Constraint, w: &World, px: f64) -> Vec<(&'static str, f64)> {
    if is_linear(&c.kind) {
        let Some(b) = linear_base(c, w) else { return vec![] };
        let n = [-b.u[1], b.u[0]];
        let m = mul(add(b.p, b.q), 0.5);
        // away from the middle of the shape it belongs to
        let mid = match w.get(&c.a.as_ref().unwrap().e) {
            Some(Entity::Rect(r)) => [r.x + r.w / 2.0, r.y + r.h / 2.0],
            Some(Entity::Arc(x)) => [x.cx, x.cy],
            Some(Entity::Circle(x)) => [x.cx, x.cy],
            Some(Entity::Polygon(x)) => [x.cx, x.cy],
            _ => m,
        };
        let s = sign1(dot(sub(m, mid), n));
        return vec![("off", s * 26.0 * px), ("slide", 0.0)];
    }
    if is_radial(&c.kind) {
        return vec![("ang", 45.0), ("out", 18.0 * px)];
    }
    if c.kind == "angle" {
        return vec![("off", 46.0 * px)];
    }
    vec![]
}

/// Move a dimension so its text sits at `cur` (and, for an angle, measure that sector).
pub fn place_at(c: &mut Constraint, w: &World, cur: P2) {
    if is_linear(&c.kind) {
        let Some(b) = linear_base(c, w) else { return };
        let n = [-b.u[1], b.u[0]];
        let m = mul(add(b.p, b.q), 0.5);
        let s = dot(sub(cur, m), b.u);
        let half = dot(sub(b.q, b.p), b.u).abs() / 2.0;
        set(c, "off", dot(sub(cur, m), n));
        set(c, "slide", if s.abs() <= half { 0.0 } else { s });
    } else if is_radial(&c.kind) {
        let Some((cc, r, _)) = round_base(c, w) else { return };
        let d = sub(cur, cc);
        set(c, "ang", d[1].atan2(d[0]) / D2R);
        set(c, "out", len(d) - r);
    } else if c.kind == "angle" {
        let (Some(a), Some(b)) = (c.a.as_ref().and_then(|r| w.get(&r.e)), c.b.as_ref().and_then(|r| w.get(&r.e))) else {
            return;
        };
        if let Some(v) = line_meet(a, b) {
            set(c, "off", len(sub(cur, v)).max(1e-6));
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Arrow {
    pub at: P2,
    /// The way it points.
    pub d: P2,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArcSeg {
    pub c: P2,
    pub r: f64,
    pub a0: f64,
    pub a1: f64,
    pub thin: bool,
}

/// Lines, extension lines, arrows and arcs for one dimension (`dims.js layout`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub lines: Vec<[P2; 2]>,
    pub thin: Vec<[P2; 2]>,
    pub arrows: Vec<Arrow>,
    pub arcs: Vec<ArcSeg>,
    pub text: P2,
}

/// `layout(c, E, px)`; `None` where 1.x would throw (the label is then left out).
pub fn layout(c: &Constraint, w: &World, px: f64) -> Option<Layout> {
    let mut out = Layout::default();
    let (gap, over, arrow) = (4.0 * px, 7.0 * px, 10.0 * px);
    if is_linear(&c.kind) {
        let LinearBase { p, q, u, line } = linear_base(c, w)?;
        let n = [-u[1], u[0]];
        let m = mul(add(p, q), 0.5);
        let off = get(c, "off").unwrap_or(26.0 * px);
        let level = dot(m, n) + off;
        let a = add(p, mul(n, level - dot(p, n)));
        let b = add(q, mul(n, level - dot(q, n)));
        for (x, y) in [(p, a), (q, b)] {
            let s = level - dot(x, n);
            let sg = sign1(s);
            if s.abs() > gap {
                out.thin.push([add(x, mul(n, sg * gap)), add(y, mul(n, sg * over))]);
            }
        }
        // to a line: carry the line on to the foot of the perpendicular if it stops short
        if let Some((l1, l2)) = line {
            let d = unit(sub(l2, l1));
            let t1 = dot(sub(l1, q), d);
            let t2 = dot(sub(l2, q), d);
            if t1 > gap && t2 > gap {
                out.thin.push([q, add(q, mul(d, t1.min(t2)))]);
            } else if t1 < -gap && t2 < -gap {
                out.thin.push([q, add(q, mul(d, t1.max(t2)))]);
            }
        }
        let wv = dot(sub(b, a), u);
        let dir_ab = sign1(wv);
        let uu = mul(u, dir_ab);
        let t = add(mul(add(a, b), 0.5), mul(u, get(c, "slide").filter(|s| *s != 0.0).unwrap_or(0.0)));
        out.text = t;
        out.lines.push([a, b]);
        if wv.abs() > 2.6 * arrow + 14.0 * px {
            out.arrows.push(Arrow { at: a, d: mul(uu, -1.0) });
            out.arrows.push(Arrow { at: b, d: uu });
        } else {
            // too short for arrows inside: put them outside, pointing in
            out.arrows.push(Arrow { at: a, d: uu });
            out.arrows.push(Arrow { at: b, d: mul(uu, -1.0) });
            out.lines.push([a, sub(a, mul(uu, 2.2 * arrow))]);
            out.lines.push([b, add(b, mul(uu, 2.2 * arrow))]);
        }
        // text off the end: run the dimension line out to it
        let ta = dot(sub(t, a), uu);
        let tb = dot(sub(t, b), uu);
        if ta < 0.0 {
            out.lines.push([a, t]);
        }
        if tb > 0.0 {
            out.lines.push([b, t]);
        }
        return Some(out);
    }
    if is_radial(&c.kind) {
        let (cc, r, ent) = round_base(c, w)?;
        let ang = get(c, "ang").unwrap_or(45.0);
        let th = ang * D2R;
        let d = [th.cos(), th.sin()];
        let o = get(c, "out").unwrap_or(18.0 * px);
        let t = add(cc, mul(d, (r + o).max(0.0)));
        let rr = add(cc, mul(d, r));
        out.text = t;
        if c.kind == "diameter" {
            let r2 = sub(cc, mul(d, r));
            out.lines.push([r2, if o >= 0.0 { t } else { rr }]);
            out.arrows.push(Arrow { at: rr, d });
            out.arrows.push(Arrow { at: r2, d: mul(d, -1.0) });
        } else {
            out.lines.push([cc, if o >= 0.0 { t } else { rr }]);
            out.arrows.push(Arrow { at: rr, d });
        }
        // an arc that doesn't reach the leader: carry it round, thin
        if let Entity::Arc(arc) = ent {
            let deg = |x: f64| ((x % 360.0) + 360.0) % 360.0;
            let sw = {
                let s = deg(arc.a1 - arc.a0);
                if s == 0.0 { 360.0 } else { s }
            };
            let at = deg(ang);
            let rel = deg(at - arc.a0);
            if rel > sw {
                let to_start = 360.0 - rel;
                let to_end = rel - sw;
                let (a0, a1) = if to_end < to_start { (arc.a1, at) } else { (at, arc.a0) };
                out.arcs.push(ArcSeg { c: cc, r, a0: a0 * D2R, a1: (a0 + deg(a1 - a0)) * D2R, thin: true });
            }
        }
        return Some(out);
    }
    if c.kind == "angle" {
        let (Some(Entity::Line(la)), Some(Entity::Line(lb))) =
            (c.a.as_ref().and_then(|r| w.get(&r.e)), c.b.as_ref().and_then(|r| w.get(&r.e)))
        else {
            return None;
        };
        let (ea, eb) = (&Entity::Line(la.clone()), &Entity::Line(lb.clone()));
        let Some(v) = line_meet(ea, eb) else {
            let ma = point(ea, "m")?;
            let mb = point(eb, "m")?;
            out.text = mul(add(ma, mb), 0.5);
            return Some(out);
        };
        let or1 = |x: Option<f64>| x.filter(|x| *x != 0.0 && !x.is_nan()).unwrap_or(1.0);
        let ra = mul(unit(sub(la.p2, la.p1)), or1(c.ra));
        let rb = mul(unit(sub(lb.p2, lb.p1)), or1(c.rb));
        let rad = get(c, "off").unwrap_or(46.0 * px);
        let mut a0 = ra[1].atan2(ra[0]);
        let mut a1 = rb[1].atan2(rb[0]);
        if or1(c.sign) < 0.0 {
            std::mem::swap(&mut a0, &mut a1);
        }
        while a1 < a0 {
            a1 += 2.0 * std::f64::consts::PI;
        }
        out.arcs.push(ArcSeg { c: v, r: rad, a0, a1, thin: false });
        out.arrows.push(Arrow { at: add(v, mul([a0.cos(), a0.sin()], rad)), d: [a0.sin(), -a0.cos()] });
        out.arrows.push(Arrow { at: add(v, mul([a1.cos(), a1.sin()], rad)), d: [-a1.sin(), a1.cos()] });
        let mid = (a0 + a1) / 2.0;
        out.text = add(v, mul([mid.cos(), mid.sin()], rad));
        // extension lines from the lines out to the arc
        for (l, r) in [(la, ra), (lb, rb)] {
            let t1 = dot(sub(l.p1, v), r);
            let t2 = dot(sub(l.p2, v), r);
            let (hi, lo) = (t1.max(t2), t1.min(t2));
            if rad > hi + gap {
                out.thin.push([add(v, mul(r, hi + gap)), add(v, mul(r, rad + over))]);
            } else if rad < lo - gap {
                out.thin.push([add(v, mul(r, rad - over)), add(v, mul(r, lo - gap))]);
            }
        }
        return Some(out);
    }
    Some(out)
}

// ------------------------------------------------------------------ text

/// `dimText`: the label, in the display unit; reference dimensions in brackets (not when `bare`).
pub fn dim_text(c: &Constraint, ents: &[Entity], inch: bool, bare: bool) -> String {
    let v = match c.value {
        Some(v) if !is_ref_dim(c) && v.is_finite() => v,
        _ => measure(c, ents).unwrap_or(0.0),
    };
    let t = if c.kind == "angle" {
        format!("{}°", js_num((v * 100.0).round() / 100.0))
    } else {
        let pre = match c.kind.as_str() {
            "diameter" => "⌀",
            "radius" => "R",
            _ => "",
        };
        format!("{pre}{}", fmt_len(v, None, inch))
    };
    if bare || !is_ref_dim(c) {
        t
    } else {
        format!("({t})")
    }
}

/// `dimClass`: the class names 1.x gives the label.
pub fn dim_class(c: &Constraint, selected: bool, bad: bool, preview: bool) -> String {
    if preview {
        return "preview".into();
    }
    let base = if is_ref_dim(c) {
        "ref"
    } else if expr_of_dim(c).is_some() {
        "expr"
    } else {
        "driving"
    };
    [base, if selected { "sel" } else { "" }, if bad { "bad" } else { "" }].join(" ")
}

// ------------------------------------------------------------------ editing

/// `parseDim`: a value as typed, in display units (degrees for angles), or a parameter
/// expression. Returns the value (mm or degrees) and the expression, if it was one.
pub fn parse_dim(text: &str, is_ang: bool, inch: bool, params: &BTreeMap<String, f64>) -> Result<(f64, Option<String>), String> {
    let t = text.trim();
    if expr_text(t) {
        let expr = expr_of(t);
        return match stl_engine::params::resolve_deep(&Value::String(expr.clone()), params) {
            Ok(v) => Ok((v.as_f64().unwrap_or(0.0), Some(expr))),
            Err(e) => Err(e.0),
        };
    }
    let v = if is_ang { parse_expr(t) } else { parse_len(t, inch) };
    match v {
        Some(v) if v.is_finite() => Ok((v, None)),
        _ => Err("Type a number, or a parameter (like width or =width/2).".to_string()),
    }
}

fn warn(msg: &str) -> SketchEffect {
    SketchEffect::Toast(Level::Warning, msg.to_string())
}

/// `setDimValue`: change a dimension's value (or the parameter expression that sets it), then
/// solve. A size the sketch can't take is undone with the 1.x message.
pub fn set_dim_value(s: &mut SketchSession, idx: usize, v: f64, expr: Option<String>) -> Vec<SketchEffect> {
    if idx >= s.constraints.len() {
        return vec![];
    }
    s.push();
    let old = s.constraints[idx].clone();
    {
        let c = &mut s.constraints[idx];
        c.value = Some(v);
        match &expr {
            Some(x) => {
                c.extra.insert("expr".into(), Value::String(x.clone()));
            }
            None => {
                c.extra.remove("expr");
            }
        }
    }
    let fixed = fixed_ents(s);
    let r = stl_sketch::solver::solve(&s.entities, &s.constraints, &[], &fixed);
    if !r.ok {
        s.constraints[idx] = old;
        s.history.pop();
        return vec![warn("The sketch can't take that size: it conflicts with its other dimensions and relations.")];
    }
    s.entities = r.entities;
    vec![SketchEffect::Changed]
}

/// `setDimRef`: reference (driven) dimensions show a size; driving ones set it.
pub fn set_dim_ref(s: &mut SketchSession, idx: usize, is_ref: bool) -> Vec<SketchEffect> {
    let Some(c) = s.constraints.get(idx).cloned() else { return vec![] };
    if c.has_ref() == is_ref {
        return vec![];
    }
    if !is_ref {
        let ents = all_ents(s);
        let mut test = c.clone();
        test.ref_ = None;
        test.value = measure(&c, &ents);
        let others: Vec<Constraint> = s.constraints.iter().enumerate().filter(|(i, _)| *i != idx).map(|(_, x)| x.clone()).collect();
        if is_redundant(&s.entities, &others, &test, &fixed_ents(s)) {
            return vec![
                warn("That would over-define the sketch: other dimensions and relations already set this size."),
                SketchEffect::Changed,
            ];
        }
        s.push();
        let c = &mut s.constraints[idx];
        c.value = test.value;
        c.ref_ = None;
        c.extra.remove("expr");
    } else {
        s.push();
        let c = &mut s.constraints[idx];
        c.ref_ = Some(Value::Bool(true));
        c.extra.remove("expr");
    }
    vec![SketchEffect::Changed]
}

/// `editDimension`: select it, then open the Modify box (a reference dimension only says so).
pub fn edit_dimension(s: &mut SketchSession, idx: usize) -> Vec<SketchEffect> {
    let Some(c) = s.constraints.get(idx) else { return vec![] };
    let is_ref = c.has_ref();
    s.sel_c = Some(idx);
    if is_ref {
        return vec![
            SketchEffect::Changed,
            SketchEffect::Toast(
                Level::Info,
                "A reference dimension follows the sketch. Untick Reference in the panel to make it set the size.".into(),
            ),
        ];
    }
    vec![SketchEffect::Dialog(SketchDialog::DimValue(idx))]
}

/// `prepDim`: a candidate follows the pointer, placed there, with its value measured.
pub fn prep_dim(s: &SketchSession, r: &Constraint, uv: P2) -> Constraint {
    let ents = all_ents(s);
    let w = World::new(&ents);
    let mut c = r.clone();
    if c.kind == "angle" {
        c = angle_to(&c, &ents, uv);
    }
    place_at(&mut c, &w, uv);
    c.value = Some(measure(&c, &ents).unwrap_or(0.0));
    c
}

/// `placeDimension`: add the dimension; when it over-defines the sketch, ask first
/// (`SketchDialog::OverDefined`; [`add_reference`] is the "Add as reference" answer).
pub fn place_dimension(s: &mut SketchSession, c0: &Constraint, hint: String) -> Vec<SketchEffect> {
    let mut c = c0.clone();
    c.id = Some(Value::String(s.new_id()));
    let ents = all_ents(s);
    c.value = Some(measure(&c, &ents).unwrap_or(0.0));
    if !(c.value.unwrap_or(0.0) > 1e-9) {
        return vec![warn("That measures zero: use a Coincident relation instead.")];
    }
    let mut out = vec![SketchEffect::Hint(hint)];
    if is_redundant(&s.entities, &s.constraints, &c, &fixed_ents(s)) {
        out.push(SketchEffect::Dialog(SketchDialog::OverDefined(Box::new(c))));
        return out;
    }
    s.push();
    s.constraints.push(c);
    let i = s.constraints.len() - 1;
    s.sel_c = Some(i);
    out.push(SketchEffect::Changed);
    out.push(SketchEffect::Dialog(SketchDialog::DimValue(i)));
    out
}

/// "Add as reference" of the Over defined question: the dimension goes in as a driven one.
pub fn add_reference(s: &mut SketchSession, mut c: Constraint) -> Vec<SketchEffect> {
    c.ref_ = Some(Value::Bool(true));
    s.push();
    s.constraints.push(c);
    s.sel_c = Some(s.constraints.len() - 1);
    vec![SketchEffect::Changed]
}

/// Where the label of dimension `idx` is on screen (the Modify box opens over it).
pub fn label_screen(s: &SketchSession, vp: &Viewport, idx: usize) -> Option<egui::Pos2> {
    let c = s.constraints.get(idx)?;
    let ents = all_ents(s);
    let w = World::new(&ents);
    let l = layout(c, &w, px_of(s, vp))?;
    let f = &s.frame;
    let p3 = [0, 1, 2].map(|i| f.origin[i] + f.x[i] * l.text[0] + f.y[i] * l.text[1]);
    Some(vp.world_to_screen(p3))
}

// ------------------------------------------------------------------ pending dimension / picks

/// What the relation tools have picked, and the Smart Dimension candidate; the tools write it and
/// the painters read it (`cpicks`, `dimPend`).
#[derive(Clone, Debug, Default)]
pub struct Pend {
    pub picks: Vec<Ref>,
    pub cand: Option<Constraint>,
    /// A draw tool is mid-draw: dimension labels let clicks through.
    pub drawing: bool,
}

thread_local! {
    static PEND: RefCell<Pend> = RefCell::new(Pend::default());
}

pub fn pending() -> Pend {
    PEND.with(|p| p.borrow().clone())
}
pub fn set_pending(p: Pend) {
    PEND.with(|x| *x.borrow_mut() = p);
}
/// Draw tools say whether a drawing is in progress (`!!this.draw`).
pub fn set_drawing(on: bool) {
    PEND.with(|x| x.borrow_mut().drawing = on);
}

/// `pickingTool()` minus the draw tools' own state: do clicks pick geometry, so labels must not
/// take them?
pub fn passive(s: &SketchSession) -> bool {
    let p = pending();
    if s.tool == "c_dimension" {
        return !p.picks.is_empty() || p.drawing;
    }
    s.tool.starts_with("c_") || matches!(s.tool.as_str(), "trim" | "sk_mirror" | "sk_useedge") || p.drawing
}

// ------------------------------------------------------------------ painting

/// Text the bundled fonts can draw: a character with no glyph (egui panics on those) becomes a
/// plain stand-in. The fonts task should add `⌀ ƒ • ⊙ ∥ ⊥ ◎ ⌯ 🔒`; until then the labels still show.
pub fn drawable(ctx: &egui::Context, font: &FontId, text: &str) -> String {
    ctx.fonts_mut(|f| {
        text.chars()
            .map(|c| {
                if f.has_glyph(font, c) {
                    return c.to_string();
                }
                match c {
                    '⌀' => "Ø".into(),
                    'ƒ' => "f".into(),
                    '•' | '·' => ".".into(),
                    '⊙' | '◎' => "o".into(),
                    '∥' => "//".into(),
                    '⊥' => "_|".into(),
                    '⌯' => "S".into(),
                    '🔒' => "F".into(),
                    _ => "?".into(),
                }
            })
            .collect()
    })
}

fn px_of(s: &SketchSession, vp: &Viewport) -> f64 {
    let px = vp.pixel_size(s.frame.origin);
    if px.is_finite() && px > 0.0 { px } else { 0.1 }
}

/// Dimension colours by class: (line, label text, label border).
fn colours(t: &crate::theme::Tokens, c: &Constraint, sel: bool, bad: bool, preview: bool) -> (Color32, Color32, Option<Color32>) {
    let (mut line, mut text, mut border) = (t.dim_ink, t.dim_text, None);
    if is_ref_dim(c) {
        line = t.dim_ref;
        text = t.dim_ref;
    }
    if expr_of_dim(c).is_some() {
        line = t.dim_expr;
        text = t.dim_expr;
    }
    if sel {
        line = t.dim_sel;
        text = t.dim_sel_text;
        border = Some(t.dim_sel_line);
    }
    if bad {
        line = t.dim_bad;
        text = t.dim_bad;
        border = Some(t.dim_bad_line);
    }
    if preview {
        line = t.dim_preview;
        text = t.dim_preview;
        border = Some(t.dim_preview_line);
    }
    (line, text, border)
}

fn arrow_head(sc: &impl Fn(P2) -> egui::Pos2, a: &Arrow, px: f64, col: Color32) -> Shape {
    let tip = sc(a.at);
    let base = sc([a.at[0] - a.d[0] * px * 10.0, a.at[1] - a.d[1] * px * 10.0]);
    let mut u = tip - base;
    let l = u.length();
    if l > 0.0 {
        u /= l;
    } else {
        u = vec2(1.0, 0.0);
    }
    let (k, w) = (9.0, 3.2);
    let c = tip - u * k;
    Shape::convex_polygon(vec![tip, pos2(c.x - u.y * w, c.y + u.x * w), pos2(c.x + u.y * w, c.y - u.x * w)], col, Stroke::NONE)
}

fn arc_points(sc: &impl Fn(P2) -> egui::Pos2, a: &ArcSeg) -> Vec<egui::Pos2> {
    let n = 6.max(((a.a1 - a.a0) / 0.06).ceil() as usize);
    (0..=n)
        .map(|i| {
            let t = a.a0 + (a.a1 - a.a0) * i as f64 / n as f64;
            sc([a.c[0] + a.r * t.cos(), a.c[1] + a.r * t.sin()])
        })
        .collect()
}

/// The dimension labels, arrows and extension lines; drag a label to place it, click to select,
/// double-click to change. Reference dimensions are in brackets; conflicting ones red.
pub fn paint(ui: &egui::Ui, s: &mut SketchSession, vp: &Viewport, inch: bool, params: &BTreeMap<String, f64>) -> Vec<SketchEffect> {
    let _ = params;
    let mut out = Vec::new();
    let t = crate::theme::tokens(ui.visuals().dark_mode);
    let painter = ui.painter().with_clip_rect(vp.rect());
    let ents = all_ents(s);
    let w = World::new(&ents);
    let px = px_of(s, vp);
    let st = super::solve::last();
    let pend = pending();
    let is_passive = passive(s);
    let frame = s.frame;
    let to3 = |p: P2| -> [f64; 3] { [0, 1, 2].map(|i| frame.origin[i] + frame.x[i] * p[0] + frame.y[i] * p[1]) };
    let sc = |p: P2| vp.world_to_screen(to3(p));

    let mut items: Vec<(Option<usize>, Constraint)> = Vec::new();
    for (i, c) in s.constraints.iter().enumerate() {
        if is_dim(&c.kind) && refs_exist(c, &w) {
            items.push((Some(i), c.clone()));
        }
    }
    if let Some(c) = pend.cand.clone() {
        items.push((None, c));
    }

    for (idx, c) in items {
        let preview = idx.is_none();
        let sel = idx.is_some() && s.sel_c == idx;
        let bad = idx.is_some_and(|i| st.conflicts.contains(&i));
        let Some(l) = layout(&c, &w, px) else { continue };
        let (line_col, text_col, border) = colours(&t, &c, sel, bad, preview);
        let thick = Stroke::new(1.1, line_col);
        let thin = Stroke::new(0.8, line_col.gamma_multiply(0.75));
        for [a, b] in &l.lines {
            painter.line_segment([sc(*a), sc(*b)], thick);
        }
        for a in l.arcs.iter().filter(|a| !a.thin) {
            painter.add(Shape::line(arc_points(&sc, a), thick));
        }
        for [a, b] in &l.thin {
            painter.line_segment([sc(*a), sc(*b)], thin);
        }
        for a in l.arcs.iter().filter(|a| a.thin) {
            painter.add(Shape::line(arc_points(&sc, a), thin));
        }
        for a in &l.arrows {
            painter.add(arrow_head(&sc, a, px, line_col));
        }
        // the label
        let mut label = dim_text(&c, &ents, inch, false);
        let shown = if expr_of_dim(&c).is_some() { format!("ƒ {label}") } else { label.clone() };
        let font = FontId::proportional(11.5);
        let shown = drawable(ui.ctx(), &font, &shown);
        let galley = painter.layout_no_wrap(shown, font, text_col);
        let at = sc(l.text);
        let rect = Rect::from_center_size(at, galley.size() + vec2(10.0, 2.0)).expand2(vec2(0.0, 1.0));
        let (mut hover, mut resp) = (false, None);
        if !preview {
            let i = idx.unwrap();
            let sense = if is_passive { Sense::hover() } else { Sense::click_and_drag() };
            let r = ui.interact(rect, ui.scope_id().with(("sk_dim", i)), sense);
            label = dim_text(&c, &ents, inch, false);
            let name = label.clone();
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name.clone()));
            let tip = if is_ref_dim(&c) {
                "Reference dimension (driven)".to_string()
            } else if let Some(x) = expr_of_dim(&c) {
                format!("{} = {}", &x[1..], dim_text(&c, &ents, inch, true))
            } else {
                format!("{} · double-click to change, drag to move", type_label(&c.kind))
            };
            hover = r.hovered();
            resp = Some((i, r, tip));
        }
        painter.rect_filled(rect, 3.0, t.float);
        let stroke = match (border, hover) {
            (Some(b), _) => Stroke::new(1.0, b),
            (None, true) => Stroke::new(1.0, t.dim_hover),
            _ => Stroke::NONE,
        };
        if stroke.width > 0.0 {
            painter.rect_stroke(rect, 3.0, stroke, StrokeKind::Inside);
        }
        painter.galley(rect.center() - galley.size() / 2.0, galley, text_col);
        let Some((i, r, tip)) = resp else { continue };
        let r = r.on_hover_text(tip);
        if r.drag_started() {
            s.push();
        }
        if r.dragged() {
            if let Some(pos) = r.interact_pointer_pos() {
                if let Some(p3) = vp.ray_to_plane(pos, &s.frame) {
                    let uv = s.to_uv(p3);
                    let mut c2 = s.constraints[i].clone();
                    place_at(&mut c2, &w, uv);
                    if c2.kind == "angle" && c2.has_ref() {
                        let n = angle_to(&c2, &ents, uv);
                        c2.ra = n.ra;
                        c2.rb = n.rb;
                        c2.sign = n.sign;
                    }
                    s.constraints[i] = c2;
                }
            }
        }
        if r.drag_stopped() {
            out.push(SketchEffect::Changed);
        } else if r.double_clicked() {
            out.extend(edit_dimension(s, i));
        } else if r.clicked() {
            s.sel_c = Some(i);
            s.sel.clear();
            out.push(SketchEffect::Changed);
        }
    }
    out
}
