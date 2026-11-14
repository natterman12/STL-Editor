//! Typed sizes while drawing (`sketcher.js` `readTyped`, `addTypedDims`, `addDims`) and the
//! display units they are typed in (`web/js/units.js` `U`, `parseExpr`).

use serde_json::{json, Value};
use stl_sketch::entities::{point, Entity, P2};
use stl_sketch::solver::{is_redundant, measure, Constraint, Ref};

use super::SketchSession;

/// `U.factor`: millimetres per display unit.
pub fn factor(inch: bool) -> f64 {
    if inch { 25.4 } else { 1.0 }
}

/// `U.label`.
pub fn unit_label(inch: bool) -> &'static str {
    if inch { "in" } else { "mm" }
}

/// JS `Math.round` (halves up).
pub fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// A number as JS's `String(n)` prints it, for the values the HUD shows.
pub fn js_num(v: f64) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if v == 0.0 {
        return "0".into();
    }
    format!("{v}")
}

/// `U.fmt(mm)`: display units, 3 decimals (4 in inch mode), trailing zeros dropped.
pub fn fmt_len(mm: f64, inch: bool) -> String {
    let v = if mm.is_finite() { mm } else { 0.0 } / factor(inch);
    let k = if inch { 1e4 } else { 1e3 };
    js_num(js_round(v * k) / k)
}

/// sketcher.js `fmt`: 3 decimals (angles, side counts).
pub fn fmt3(v: f64) -> String {
    js_num(js_round(v * 1000.0) / 1000.0)
}

/// `U.parse`: typed text in display units to millimetres.
pub fn parse_len(text: &str, inch: bool) -> f64 {
    let v = parse_expr(text);
    if v.is_finite() { v * factor(inch) } else { f64::NAN }
}

/// `parseExpr`: "12", "1/4", "1 3/8", "2*3+1", "1,5"; NaN when it isn't a number.
pub fn parse_expr(text: &str) -> f64 {
    let s = text.trim().replacen(',', ".", 1);
    let s = strip_unit(&s);
    let s = s.trim();
    if s.is_empty() {
        return f64::NAN;
    }
    if let Some(v) = mixed_fraction(s) {
        return v;
    }
    if s.chars().all(|c| "0123456789+-*/(). ".contains(c)) {
        if let Some(r) = Arith::eval(s) {
            if r.is_finite() {
                return r;
            }
        }
    }
    let f = parse_float(s);
    if f.is_finite() { f } else { f64::NAN }
}

/// `.replace(/["″]|in$|mm$/i, '')`: the leftmost of a quote mark or a trailing "in" / "mm".
fn strip_unit(s: &str) -> String {
    let quote = s.char_indices().find(|(_, c)| *c == '"' || *c == '″').map(|(i, c)| (i, c.len_utf8()));
    let lower = s.to_ascii_lowercase();
    let suffix = (s.len() >= 2 && (lower.ends_with("in") || lower.ends_with("mm"))).then(|| (s.len() - 2, 2));
    let hit = match (quote, suffix) {
        (Some(q), Some(x)) => Some(if q.0 <= x.0 { q } else { x }),
        (q, x) => q.or(x),
    };
    match hit {
        Some((i, n)) => format!("{}{}", &s[..i], &s[i + n..]),
        None => s.to_string(),
    }
}

/// `^(-?\d+)\s+(\d+)\/(\d+)$`: `w + sign(w || 1) * (n / d)`.
fn mixed_fraction(s: &str) -> Option<f64> {
    let digits = |t: &str| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit());
    let ws = s.find(char::is_whitespace)?;
    let (w, rest) = (&s[..ws], s[ws..].trim_start());
    let wd = w.strip_prefix('-').unwrap_or(w);
    if !digits(wd) {
        return None;
    }
    let (n, d) = rest.split_once('/')?;
    if !digits(n) || !digits(d) {
        return None;
    }
    let w: f64 = w.parse().ok()?;
    let sign = if w == 0.0 { 1.0 } else { w.signum() };
    Some(w + sign * (n.parse::<f64>().ok()? / d.parse::<f64>().ok()?))
}

/// JS `parseFloat`: the longest number at the start, NaN if none.
pub(crate) fn parse_float(s: &str) -> f64 {
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    if s[i..].starts_with("Infinity") {
        return if b.first() == Some(&b'-') { f64::NEG_INFINITY } else { f64::INFINITY };
    }
    let start_digits = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let mut any = i > start_digits;
    if i < b.len() && b[i] == b'.' {
        let j = i + 1;
        let mut k = j;
        while k < b.len() && b[k].is_ascii_digit() {
            k += 1;
        }
        if k > j || any {
            any = any || k > j;
            i = k;
        }
    }
    if !any {
        return f64::NAN;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut k = i + 1;
        if k < b.len() && (b[k] == b'+' || b[k] == b'-') {
            k += 1;
        }
        let ds = k;
        while k < b.len() && b[k].is_ascii_digit() {
            k += 1;
        }
        if k > ds {
            i = k;
        }
    }
    s[..i].trim_end_matches('.').parse::<f64>().unwrap_or(f64::NAN)
}

/// `Function("return (s)")` for arithmetic text; `None` where JS would throw.
pub(crate) fn arith(s: &str) -> Option<f64> {
    Arith::eval(s)
}

/// The arithmetic JS's `Function("return (...)")` evaluates for `parseExpr`: numbers, `+ - * /`,
/// `**`, brackets. Anything that would be a JS syntax error gives `None`.
struct Arith<'a> {
    t: Vec<Tok<'a>>,
    i: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum Tok<'a> {
    Num(&'a str),
    Op(&'a str),
}

impl<'a> Arith<'a> {
    fn eval(s: &'a str) -> Option<f64> {
        let b = s.as_bytes();
        let mut t = Vec::new();
        let mut i = 0;
        while i < b.len() {
            let c = b[i];
            if c == b' ' {
                i += 1;
            } else if c.is_ascii_digit() || c == b'.' {
                let st = i;
                while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                    i += 1;
                }
                let n = &s[st..i];
                // one dot at most, not a lone dot, no legacy octal ("01" is an error in strict mode)
                if n.matches('.').count() > 1 || n == "." || (n.len() > 1 && n.starts_with('0') && n.as_bytes()[1].is_ascii_digit()) {
                    return None;
                }
                t.push(Tok::Num(n));
            } else if s[i..].starts_with("**") {
                t.push(Tok::Op("**"));
                i += 2;
            } else if s[i..].starts_with("++") || s[i..].starts_with("--") {
                return None; // increment / decrement on a literal
            } else {
                t.push(Tok::Op(&s[i..i + 1]));
                i += 1;
            }
        }
        let mut p = Arith { t, i: 0 };
        let v = p.expr()?;
        (p.i == p.t.len()).then_some(v)
    }

    fn peek(&self) -> Option<Tok<'a>> {
        self.t.get(self.i).copied()
    }

    fn expr(&mut self) -> Option<f64> {
        let mut v = self.term()?;
        while let Some(Tok::Op(o @ ("+" | "-"))) = self.peek() {
            self.i += 1;
            let r = self.term()?;
            v = if o == "+" { v + r } else { v - r };
        }
        Some(v)
    }

    fn term(&mut self) -> Option<f64> {
        let mut v = self.unary()?;
        while let Some(Tok::Op(o @ ("*" | "/"))) = self.peek() {
            self.i += 1;
            let r = self.unary()?;
            v = if o == "*" { v * r } else { v / r };
        }
        Some(v)
    }

    fn unary(&mut self) -> Option<f64> {
        match self.peek() {
            Some(Tok::Op("-")) => {
                self.i += 1;
                let v = self.unary()?;
                if self.peek() == Some(Tok::Op("**")) {
                    return None; // -a ** b is a syntax error
                }
                Some(-v)
            }
            Some(Tok::Op("+")) => {
                self.i += 1;
                let v = self.unary()?;
                if self.peek() == Some(Tok::Op("**")) {
                    return None;
                }
                Some(v)
            }
            _ => self.power(),
        }
    }

    fn power(&mut self) -> Option<f64> {
        let base = self.primary()?;
        if self.peek() == Some(Tok::Op("**")) {
            self.i += 1;
            let e = self.unary()?;
            return Some(base.powf(e));
        }
        Some(base)
    }

    fn primary(&mut self) -> Option<f64> {
        match self.peek()? {
            Tok::Num(n) => {
                self.i += 1;
                let n = if n.ends_with('.') { &n[..n.len() - 1] } else { n };
                let n = if n.starts_with('.') { format!("0{n}") } else { n.to_string() };
                n.parse().ok()
            }
            Tok::Op("(") => {
                self.i += 1;
                let v = self.expr()?;
                if self.peek() != Some(Tok::Op(")")) {
                    return None;
                }
                self.i += 1;
                Some(v)
            }
            _ => None,
        }
    }
}

/// The two HUD fields while drawing (`#dyn-a`, `#dyn-b`) and whether anything was typed.
#[derive(Clone, Debug, Default)]
pub struct Typed {
    pub a: String,
    pub b: String,
    /// The focused field: 0 = A, 1 = B; `None` when neither has focus.
    pub focus: Option<u8>,
    /// `this.typed`: the preview follows the fields, not the pointer.
    pub typed: bool,
    /// Tab selected the focused field's text: the next key replaces it.
    pub replace: bool,
}

impl Typed {
    /// Both fields emptied (`inA.value = ''; inB.value = ''`).
    pub fn clear(&mut self) {
        self.a.clear();
        self.b.clear();
    }

    /// `readTyped`: `(hasA, hasB)`, counting only what was typed.
    pub fn read(&self) -> (bool, bool) {
        let has = |t: &str| self.typed && !t.trim().is_empty() && parse_expr(t).is_finite();
        (has(&self.a), has(&self.b))
    }

    pub fn field_mut(&mut self) -> Option<&mut String> {
        match self.focus {
            Some(0) => Some(&mut self.a),
            Some(_) => Some(&mut self.b),
            None => None,
        }
    }
}

fn ent_by<'a>(all: &'a [Entity], id: &str) -> Option<&'a Entity> {
    all.iter().rev().find(|e| e.id() == Some(id))
}

fn sub(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}

fn unit(a: P2) -> P2 {
    let l = a[0].hypot(a[1]);
    if l > 1e-12 { [a[0] / l, a[1] / l] } else { [1.0, 0.0] }
}

/// dims.js `defaultPlacement` for the dimensions typed sizes make (`off`/`slide`, `ang`/`out`).
pub fn default_placement(c: &mut Constraint, all: &[Entity], px: f64) {
    let kind = c.kind.as_str();
    let Some(a) = c.a.clone() else { return };
    let Some(ea) = ent_by(all, &a.e) else { return };
    if matches!(kind, "hdistance" | "vdistance" | "distance" | "length") {
        let (p, q, u) = if kind == "length" {
            match ea {
                Entity::Slot(s) => {
                    let (p, q) = ([s.x1, s.y1], [s.x2, s.y2]);
                    (p, q, unit(sub(q, p)))
                }
                Entity::Line(l) => (l.p1, l.p2, unit(sub(l.p2, l.p1))),
                _ => return,
            }
        } else {
            let p = point(ea, a.k.as_deref().unwrap_or("")).unwrap_or([f64::NAN; 2]);
            let b = c.b.clone();
            let q = b
                .as_ref()
                .and_then(|b| ent_by(all, &b.e).and_then(|e| point(e, b.k.as_deref().unwrap_or(""))))
                .unwrap_or([f64::NAN; 2]);
            let u = match kind {
                "hdistance" => [1.0, 0.0],
                "vdistance" => [0.0, 1.0],
                _ => unit(sub(q, p)),
            };
            (p, q, u)
        };
        let n = [-u[1], u[0]];
        let m = [(p[0] + q[0]) * 0.5, (p[1] + q[1]) * 0.5];
        let mid = match ea {
            Entity::Rect(r) => [r.x + r.w / 2.0, r.y + r.h / 2.0],
            Entity::Circle(e) => [e.cx, e.cy],
            Entity::Arc(e) => [e.cx, e.cy],
            Entity::Polygon(e) => [e.cx, e.cy],
            _ => m,
        };
        let d = sub(m, mid);
        let dt = d[0] * n[0] + d[1] * n[1];
        let s = if dt > 0.0 { 1.0 } else if dt < 0.0 { -1.0 } else { 1.0 };
        c.extra.insert("off".into(), json!(s * 26.0 * px));
        c.extra.insert("slide".into(), json!(0));
    } else if matches!(kind, "radius" | "diameter") {
        c.extra.insert("ang".into(), json!(45));
        c.extra.insert("out".into(), json!(18.0 * px));
    } else if kind == "angle" {
        c.extra.insert("off".into(), json!(46.0 * px));
    }
}

/// `addDims`: dimensions at the sizes the shapes have now, placed beside them; any that
/// measure zero or that the sketch already sets are skipped.
pub fn add_dims(s: &mut SketchSession, specs: Vec<Constraint>, px: f64) {
    let fixed = super::snap::fixed_ents(s);
    for sp in specs {
        let mut c = sp;
        c.id = Some(Value::String(s.new_id()));
        let mut all = s.entities.clone();
        all.extend(fixed.iter().cloned());
        let v = measure(&c, &all);
        c.value = v;
        if !v.is_some_and(|v| v > 1e-9) {
            continue;
        }
        default_placement(&mut c, &all, px);
        if is_redundant(&s.entities, &s.constraints, &c, &fixed) {
            continue;
        }
        s.constraints.push(c);
    }
}

/// A dimension spec: `{type, a: {e, k?}, b?, sign?}`.
pub fn spec(kind: &str, a: Ref, b: Option<Ref>, sign: Option<f64>) -> Constraint {
    let mut c = Constraint::new(kind).with(Some(a), b);
    c.sign = sign;
    c
}

/// `addTypedDims`: the sizes typed while drawing a shape become its driving dimensions.
/// `slot_len` is the draw's `slotLen` (a length typed at the slot's first click).
pub fn add_typed_dims(s: &mut SketchSession, e: &Entity, t: &Typed, slot_len: bool, px: f64) {
    let (ha, hb) = t.read();
    let Some(id) = e.id().map(str::to_string) else { return };
    let sgn = |x: f64| if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { 1.0 };
    let mut specs = Vec::new();
    match e {
        Entity::Rect(r) => {
            if ha {
                specs.push(spec("hdistance", Ref::pt(&id, "c0"), Some(Ref::pt(&id, "c1")), Some(sgn(r.w))));
            }
            if hb {
                specs.push(spec("vdistance", Ref::pt(&id, "c1"), Some(Ref::pt(&id, "c2")), Some(sgn(r.h))));
            }
        }
        Entity::Circle(_) if ha => specs.push(spec("diameter", Ref::ent(&id), None, None)),
        Entity::Arc(_) | Entity::Polygon(_) if ha => specs.push(spec("radius", Ref::ent(&id), None, None)),
        Entity::Slot(_) => {
            if slot_len {
                specs.push(spec("length", Ref::ent(&id), None, None));
            }
            if ha {
                specs.push(spec("diameter", Ref::ent(&id), None, None));
            }
        }
        _ => {}
    }
    add_dims(s, specs, px);
}

/// `this.px()`: mm per screen point at the plane's origin, 0.1 when unknown.
pub fn plane_px(s: &SketchSession, vp: &crate::viewport::Viewport) -> f64 {
    let p = vp.pixel_size(s.frame.origin);
    if p.is_finite() && p != 0.0 { p } else { 0.1 }
}
