//! `number` field, and the number helpers every plain field uses: units (`web/js/units.js`),
//! `parseNum` (app.js:2823) and the expression checks of `expr.js`.

use std::collections::BTreeMap;

use egui::{FontFamily, FontId};
use serde_json::Value;
use stl_engine::params::{is_expr as engine_is_expr, resolve_deep};

use super::{row, text_commit, unit_label, Edit, FieldEnv, Bufs};
use crate::tools::Field;

// ------------------------------------------------------------------ JS-style numbers

/// `String(n)` for a finite number: 5 is "5", 0.25 is "0.25".
pub fn js_num(n: f64) -> String {
    if !n.is_finite() {
        return if n.is_nan() { "NaN".into() } else if n > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if n == 0.0 {
        return "0".into();
    }
    format!("{n}")
}

/// A JSON number, 0 when not finite (JSON has no NaN).
pub fn num_value(n: f64) -> Value {
    serde_json::Number::from_f64(n).map(Value::Number).unwrap_or_else(|| Value::from(0))
}

/// `Math.round(v * 1000) / 1000` as a string (`num` in app.js:33).
pub fn num3(v: f64) -> String {
    js_num((v * 1000.0).round() / 1000.0)
}

// ------------------------------------------------------------------ units.js

/// `U.factor`: millimetres per displayed unit.
pub fn factor(inch: bool) -> f64 {
    if inch { 25.4 } else { 1.0 }
}

/// `U.label`.
pub fn unit_name(inch: bool) -> &'static str {
    if inch { "in" } else { "mm" }
}

/// `U.toDisp`: mm to the display number (4 decimals, 5 in inches).
pub fn to_disp(mm: f64, inch: bool) -> f64 {
    let v = mm / factor(inch);
    let p = if inch { 1e5 } else { 1e4 };
    (v * p).round() / p
}

/// `U.fmt`: a short display string.
pub fn fmt_len(mm: f64, digits: Option<i32>, inch: bool) -> String {
    let v = mm / factor(inch);
    let d = digits.unwrap_or(if inch { 4 } else { 3 });
    let p = 10f64.powi(d);
    js_num((v * p).round() / p)
}

/// `U.parse`: typed text in display units to mm (`None` is NaN).
pub fn parse_len(text: &str, inch: bool) -> Option<f64> {
    parse_expr(text).map(|v| v * factor(inch))
}

/// A tiny arithmetic evaluator for the characters `[0-9+\-*/(). ]` (what `Function(...)` was used
/// for in 1.x). `None` when JavaScript would throw.
fn eval_arith(s: &str) -> Option<f64> {
    struct P<'a> {
        b: &'a [u8],
        i: usize,
    }
    impl P<'_> {
        fn ws(&mut self) {
            while self.i < self.b.len() && self.b[self.i] == b' ' {
                self.i += 1;
            }
        }
        fn eat(&mut self, c: u8) -> bool {
            self.ws();
            if self.b.get(self.i) == Some(&c) {
                self.i += 1;
                true
            } else {
                false
            }
        }
        fn expr(&mut self) -> Option<f64> {
            let mut v = self.term()?;
            loop {
                if self.eat(b'+') {
                    v += self.term()?;
                } else if self.eat(b'-') {
                    v -= self.term()?;
                } else {
                    return Some(v);
                }
            }
        }
        fn term(&mut self) -> Option<f64> {
            let mut v = self.pow()?;
            loop {
                self.ws();
                // `**` is a power in JavaScript, not two multiplications
                if self.b.get(self.i) == Some(&b'*') && self.b.get(self.i + 1) != Some(&b'*') {
                    self.i += 1;
                    v *= self.pow()?;
                } else if self.eat(b'/') {
                    v /= self.pow()?;
                } else {
                    return Some(v);
                }
            }
        }
        fn pow(&mut self) -> Option<f64> {
            let base = self.unary()?;
            self.ws();
            if self.b.get(self.i) == Some(&b'*') && self.b.get(self.i + 1) == Some(&b'*') {
                self.i += 2;
                let e = self.pow()?;
                return Some(base.powf(e));
            }
            Some(base)
        }
        fn unary(&mut self) -> Option<f64> {
            if self.eat(b'-') {
                return self.unary().map(|v| -v);
            }
            if self.eat(b'+') {
                return self.unary();
            }
            self.primary()
        }
        fn primary(&mut self) -> Option<f64> {
            self.ws();
            if self.eat(b'(') {
                let v = self.expr()?;
                return self.eat(b')').then_some(v);
            }
            let start = self.i;
            while self.i < self.b.len() && (self.b[self.i].is_ascii_digit() || self.b[self.i] == b'.') {
                self.i += 1;
            }
            let t = std::str::from_utf8(&self.b[start..self.i]).ok()?;
            if t.is_empty() || t.matches('.').count() > 1 || t == "." {
                return None;
            }
            t.parse::<f64>().ok()
        }
    }
    let mut p = P { b: s.as_bytes(), i: 0 };
    let v = p.expr()?;
    p.ws();
    (p.i == s.len() && v.is_finite()).then_some(v)
}

/// `parseFloat`: the number at the start of the text.
pub fn parse_float(s: &str) -> Option<f64> {
    let s = s.trim_start();
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let ds = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }
    if i == ds || (i == ds + 1 && b[ds] == b'.') {
        return None;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let es = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > es {
            i = j;
        }
    }
    s[..i].parse::<f64>().ok().filter(|v| v.is_finite())
}

fn arith_chars(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || "+-*/(). ".contains(c))
}

/// `parseExpr` (units.js): "12", "1/4", "1 3/8", "2*3+1", "1,5", "25mm", `3"`. `None` is NaN.
pub fn parse_expr(text: &str) -> Option<f64> {
    let mut s = text.trim().replacen(',', ".", 1);
    // .replace(/["″]|in$|mm$/i, '') removes the first match only
    let lower = s.to_lowercase();
    let mut cut: Option<(usize, usize)> = s.char_indices().find(|(_, c)| *c == '"' || *c == '″').map(|(i, c)| (i, c.len_utf8()));
    for suf in ["in", "mm"] {
        if lower.ends_with(suf) && lower.len() == s.len() {
            let at = s.len() - 2;
            if cut.map_or(true, |(i, _)| at < i) {
                cut = Some((at, 2));
            }
        }
    }
    if let Some((i, n)) = cut {
        s.replace_range(i..i + n, "");
    }
    let s = s.trim().to_string();
    if s.is_empty() {
        return None;
    }
    // ^(-?\d+)\s+(\d+)\/(\d+)$
    if let Some(v) = mixed_fraction(&s) {
        return Some(v);
    }
    if arith_chars(&s) {
        if let Some(v) = eval_arith(&s) {
            return Some(v);
        }
    }
    parse_float(&s)
}

fn mixed_fraction(s: &str) -> Option<f64> {
    let mut it = s.split_whitespace();
    let (w, f) = (it.next()?, it.next()?);
    if it.next().is_some() || s.trim() != s {
        return None;
    }
    let wd = w.strip_prefix('-').unwrap_or(w);
    if wd.is_empty() || !wd.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let (a, b) = f.split_once('/')?;
    if a.is_empty() || b.is_empty() || !a.bytes().all(|c| c.is_ascii_digit()) || !b.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let wn: f64 = w.parse().ok()?;
    let sign = if wn == 0.0 { 1.0 } else { wn.signum() };
    Some(wn + sign * (a.parse::<f64>().ok()? / b.parse::<f64>().ok()?))
}

/// `parseNum` (app.js:2823): a number as typed, 0 when it isn't one.
pub fn parse_num_text(text: &str) -> f64 {
    let s = text.trim().replacen(',', ".", 1);
    if s.is_empty() {
        return 0.0;
    }
    if arith_chars(&s) {
        if let Some(v) = eval_arith(&s) {
            return v;
        }
    }
    parse_float(&s).unwrap_or(0.0)
}

/// `parseNum` of a value: a number stays, a string is read, anything else is 0.
pub fn parse_num(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => parse_num_text(s),
        Value::Bool(b) => f64::from(u8::from(*b)),
        _ => 0.0,
    }
}

// ------------------------------------------------------------------ expr.js

/// `isExpr`: a string starting with "=".
pub fn is_expr(v: &Value) -> bool {
    engine_is_expr(v)
}

/// `exprText`: typed text that names a parameter (or starts with "="), not a plain number like
/// "12", "1 3/8" or "25mm".
pub fn expr_text(text: &str) -> bool {
    let t = text.trim();
    if t.starts_with('=') {
        return t.chars().count() > 1;
    }
    // /^[0-9\s.,+\-*\/()]*\s*(mm|in|"|″)?$/i
    let lower = t.to_lowercase();
    let mut body = t;
    for suf in ["mm", "in"] {
        if lower.ends_with(suf) && lower.len() == t.len() {
            body = &t[..t.len() - 2];
            break;
        }
    }
    if let Some(b) = body.strip_suffix('"').or_else(|| body.strip_suffix('″')) {
        body = b;
    }
    if body.chars().all(|c| c.is_ascii_digit() || c.is_whitespace() || ".,+-*/()".contains(c)) {
        return false;
    }
    t.chars().any(|c| c.is_ascii_alphabetic() || c == '_')
}

/// `exprOf`: the stored form of a typed expression, "=width/2".
pub fn expr_of(text: &str) -> String {
    format!("={}", text.trim().trim_start_matches('=').trim())
}

/// `exprTitle`: what an expression comes to, or why it doesn't work.
pub fn expr_title(x: &str, is_len: bool, params: &BTreeMap<String, f64>) -> String {
    match resolve_deep(&Value::String(x.to_string()), params) {
        Ok(v) => format!("{} = {}{}", x.trim().trim_start_matches('=').to_string(), num3(v.as_f64().unwrap_or(0.0)), if is_len { " mm" } else { "" }),
        Err(e) => e.0,
    }
}

// ------------------------------------------------------------------ the field

/// What a number field shows for its value (not an expression).
pub fn shown_number(val: &Value, is_len: bool, inch: bool) -> String {
    match val {
        Value::Number(n) if is_len => js_num(to_disp(n.as_f64().unwrap_or(0.0), inch)),
        Value::Number(n) => js_num(n.as_f64().unwrap_or(0.0)),
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// What typing `text` into a number box stores (`onFormChange`): an expression as the string
/// "=...", a length as mm, anything else as the typed text.
pub fn commit_number(text: &str, is_len: bool, inch: bool) -> Value {
    if expr_text(text) {
        Value::String(expr_of(text))
    } else if is_len {
        num_value(parse_len(text, inch).unwrap_or(0.0))
    } else {
        Value::String(text.to_string())
    }
}

pub fn expr_font() -> FontId {
    FontId::new(12.0, FontFamily::Monospace)
}

pub fn input_font() -> FontId {
    FontId::proportional(12.0)
}

pub fn show(ui: &mut egui::Ui, env: &FieldEnv, f: &Field, unit: Option<&str>, val: &Value, bufs: &mut Bufs) -> Option<Edit> {
    let is_len = unit == Some("mm");
    let tk = env.tk;
    row(ui, env, f, false, |ui, w| {
        let expr = is_expr(val);
        let label = if is_len { unit_name(env.inch) } else { unit.unwrap_or("") };
        let uw = if label.is_empty() { 0.0 } else { 22.0 + 4.0 };
        let (shown, font, colour, tip) = if expr {
            let s = val.as_str().unwrap_or("").to_string();
            let tip = expr_title(&s, is_len, env.params);
            (s, expr_font(), tk.expr, Some(tip))
        } else {
            (shown_number(val, is_len, env.inch), input_font(), tk.text, None)
        };
        let typed = text_commit(ui, tk, bufs, f.key, &shown, w - uw, font, colour, tip);
        if expr {
            unit_label(ui, tk, "mm");
        } else if !label.is_empty() {
            unit_label(ui, tk, label);
        }
        typed.map(|t| Edit { key: f.key.to_owned(), value: commit_number(&t, is_len, env.inch) })
    })
}
