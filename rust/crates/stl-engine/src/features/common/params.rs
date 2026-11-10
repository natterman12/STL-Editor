use serde_json::{Map, Value};

use crate::FeatureError;

/// 1.x `_float`: a number from a parameter, with an optional default. Python's `float()` also
/// takes numeric strings and booleans, so those work here too.
pub fn float(p: &Map<String, Value>, k: &str, default: Option<f64>) -> Result<f64, FeatureError> {
    let v = p.get(k);
    let empty = match v {
        None | Some(Value::Null) => true,
        Some(Value::String(s)) => s.is_empty(),
        _ => false,
    };
    if empty {
        return match default {
            None => Err(FeatureError(format!("Missing value: {k}."))),
            Some(d) => Ok(d),
        };
    }
    let bad = || FeatureError(format!("{k} must be a number."));
    match v.unwrap() {
        Value::Number(n) => n.as_f64().ok_or_else(bad),
        Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        Value::String(s) => parse_py_float(s).ok_or_else(bad),
        _ => Err(bad()),
    }
}

/// Python `float(str)`: surrounding spaces and digit-separating underscores are allowed.
fn parse_py_float(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() || s.starts_with('_') || s.ends_with('_') || s.contains("__") {
        return None;
    }
    let t = s.replace('_', "");
    let body = t.trim_start_matches(['+', '-']);
    let lower = body.to_ascii_lowercase();
    if matches!(lower.as_str(), "inf" | "infinity" | "nan") {
        return t.parse().ok();
    }
    if !body.chars().next().is_some_and(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    t.parse().ok()
}
