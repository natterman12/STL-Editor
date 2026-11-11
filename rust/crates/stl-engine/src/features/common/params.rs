use serde_json::{Map, Value};

use stl_sketch::P2;

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

/// Python truthiness of a parameter value.
pub fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(m)) => !m.is_empty(),
    }
}

/// A list of strings from a parameter (anything else gives an empty list).
pub fn strings(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
        _ => Vec::new(),
    }
}

/// 1.x `S.profile(entities, only, regions)` arguments from the params.
pub fn pick(p: &Map<String, Value>) -> (Option<Vec<String>>, Option<Vec<P2>>) {
    let only = Some(strings(p.get("entities"))).filter(|v| !v.is_empty());
    let regions = match p.get("regions") {
        Some(Value::Array(a)) if !a.is_empty() => Some(
            a.iter()
                .filter_map(|x| {
                    let u = x.as_array()?;
                    Some([u.first()?.as_f64()?, u.get(1)?.as_f64()?])
                })
                .collect::<Vec<P2>>(),
        ),
        _ => None,
    };
    (only, regions)
}
