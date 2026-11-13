//! Helpers shared across tool definitions (1.x `tools.js` behaviours).

use serde_json::{json, Number, Value};
use serde_json::Map;

use crate::engine_link::{BodyView, TimelineItem};

use super::{Ctx, Values};

/// JS `Math.round`: halves go towards +infinity.
pub fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

pub fn r2(x: f64) -> f64 {
    js_round(x * 100.0) / 100.0
}

/// `selBody` in tools.js.
pub fn sel_body(ctx: &Ctx) -> Vec<String> {
    ctx.selection
        .iter()
        .filter(|id| ctx.bodies.iter().any(|b| b.id == **id))
        .cloned()
        .collect()
}

pub fn repeatable_features(features: &[TimelineItem]) -> Vec<&TimelineItem> {
    features
        .iter()
        .filter(|f| {
            super::tables::REPEATABLE.contains(&f.kind.as_str())
                && f.active
                && f.error.is_none()
                && !f.suppressed
        })
        .collect()
}

pub fn last_repeatable<'a>(ctx: &Ctx<'a>) -> Option<&'a str> {
    repeatable_features(ctx.features)
        .last()
        .map(|f| f.id.as_str())
}

/// Bbox from mesh positions (fixture / scale defaults).
pub fn body_bbox(b: &BodyView) -> Option<([f64; 3], [f64; 3])> {
    let p = &b.mesh.positions;
    if p.len() < 3 {
        return None;
    }
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for c in p.chunks_exact(3) {
        for i in 0..3 {
            lo[i] = lo[i].min(f64::from(c[i]));
            hi[i] = hi[i].max(f64::from(c[i]));
        }
    }
    Some((lo, hi))
}

pub fn size_from_bbox(b: &BodyView) -> [f64; 3] {
    body_bbox(b).map_or([10.0, 10.0, 10.0], |(lo, hi)| {
        lo.iter()
            .zip(hi.iter())
            .map(|(a, b)| js_to_fixed(b - a, 3).parse::<f64>().unwrap_or(0.0))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap()
    })
}

/// Build a [`Values`] map from ordered pairs (serde_json map order is undefined; callers use field order in tests).
pub fn values(pairs: &[(&str, Value)]) -> Values {
    let mut m = Map::new();
    for (k, v) in pairs {
        m.insert(k.to_string(), v.clone());
    }
    m
}

pub fn get_str<'a>(v: &'a Values, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

pub fn get_bool(v: &Values, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// JS `+v[key]`: NaN when missing or not a number (an `=expr` string is NaN, not 0).
pub fn get_f64(v: &Values, key: &str) -> f64 {
    v.get(key).map_or(f64::NAN, js_to_number)
}

/// Engine params after `toParams` + app.js `params()` cleanup (`_` keys and undefined).
pub fn params_cleanup(mut p: Value) -> Value {
    let Some(obj) = p.as_object_mut() else {
        return p;
    };
    let keys: Vec<String> = obj.keys().cloned().collect();
    for k in keys {
        if k.starts_with('_') {
            obj.remove(&k);
        }
    }
    p
}

pub fn to_params_default(v: &Values, custom: Option<Value>) -> Value {
    let p = custom.unwrap_or_else(|| Value::Object(v.clone()));
    params_cleanup(p)
}

pub fn js_num(n: f64) -> Value {
    if n == (n as i64) as f64 && (i64::MIN as f64) <= n && n <= (i64::MAX as f64) {
        json!(n as i64)
    } else {
        Number::from_f64(n).map_or(Value::Null, Value::Number) // JSON.stringify(NaN/±Infinity) is null
    }
}

pub fn json_f64(n: f64) -> Value {
    js_num(n)
}

/// JS `+v.tol || 0`
pub fn tol_or_zero(v: &Values) -> f64 {
    let t = get_f64(v, "tol");
    if t.is_nan() {
        0.0
    } else {
        t
    }
}

/// JS truthiness.
pub fn js_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

pub fn truthy(v: &Values, key: &str) -> bool {
    v.get(key).is_some_and(js_truthy)
}

/// JS `+x` (ToNumber).
pub fn js_to_number(v: &Value) -> f64 {
    match v {
        Value::Null => 0.0,
        Value::Bool(b) => f64::from(u8::from(*b)),
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        Value::String(s) => str_to_number(s),
        Value::Array(a) if a.is_empty() => 0.0,
        Value::Array(a) if a.len() == 1 => js_to_number(&Value::String(match &a[0] {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Null => String::new(),
            _ => return f64::NAN,
        })),
        _ => f64::NAN,
    }
}

fn str_to_number(s: &str) -> f64 {
    let s = s.trim_matches(|c: char| c.is_whitespace() || c == '﻿');
    if s.is_empty() {
        return 0.0;
    }
    match s {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    for (p, radix) in [("0x", 16), ("0X", 16), ("0b", 2), ("0B", 2), ("0o", 8), ("0O", 8)] {
        if let Some(rest) = s.strip_prefix(p) {
            return u64::from_str_radix(rest, radix).map_or(f64::NAN, |n| n as f64);
        }
    }
    if s.chars().all(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | 'e' | 'E')) {
        s.parse::<f64>().unwrap_or(f64::NAN)
    } else {
        f64::NAN
    }
}

/// JS `n.toFixed(digits)`: the exact binary value is rounded, halves go away from zero.
pub fn js_to_fixed(x: f64, digits: usize) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if x.abs() >= 1e21 {
        return format!("{x}");
    }
    let neg = x < 0.0;
    let ax = x.abs();
    let wide = format!("{:.*}", digits + 1 + 60, ax);
    let dot = wide.find('.').unwrap_or(wide.len());
    let dec = &wide[dot + 1..];
    let tie = dec.as_bytes()[digits] == b'5' && dec[digits + 1..].bytes().all(|b| b == b'0');
    let mut out = if tie {
        // round the exact half up (away from zero): bump the truncated digits
        let mut digs: Vec<u8> = wide[..dot].bytes().chain(dec[..digits].bytes()).collect();
        let mut i = digs.len();
        loop {
            if i == 0 {
                digs.insert(0, b'1');
                break;
            }
            i -= 1;
            if digs[i] == b'9' {
                digs[i] = b'0';
            } else {
                digs[i] += 1;
                break;
            }
        }
        let int_len = digs.len() - digits;
        let mut s = String::from_utf8(digs[..int_len].to_vec()).unwrap();
        if digits > 0 {
            s.push('.');
            s.push_str(std::str::from_utf8(&digs[int_len..]).unwrap());
        }
        s
    } else {
        format!("{:.*}", digits, ax)
    };
    if neg {
        out.insert(0, '-');
    }
    out
}

/// Set `key` only when `val` is present (JS drops `undefined` properties).
pub fn put_opt(m: &mut Map<String, Value>, key: &str, val: Option<&Value>) {
    if let Some(v) = val {
        m.insert(key.to_string(), v.clone());
    }
}

/// JS `arr[0]` of a body list: `None` (undefined) when empty or not a list.
pub fn first_of(v: &Values, key: &str) -> Option<Value> {
    match v.get(key) {
        Some(Value::Array(a)) => a.first().cloned(),
        _ => None,
    }
}

/// JS `p.x ? [p.x] : []`.
pub fn wrap_truthy(v: Option<&Value>) -> Value {
    match v {
        Some(x) if js_truthy(x) => Value::Array(vec![x.clone()]),
        _ => Value::Array(vec![]),
    }
}
