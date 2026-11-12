//! 1.x `f_thread`: build an external screw or cut an internal tapped hole on a plane.

use serde_json::{Map, Value};
use stl_mesh::thread::thread_tool;

use super::common::{apply_tool, float, place, truthy, Vec3};
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

/// Python `f"{x:g}"` (6 significant digits).
fn fmt_g(x: f64) -> String {
    if x == 0.0 {
        return "0".into();
    }
    if !x.is_finite() {
        return if x.is_nan() {
            "nan".into()
        } else if x < 0.0 {
            "-inf".into()
        } else {
            "inf".into()
        };
    }
    let e = format!("{:.5e}", x);
    let (mant, exp) = e.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    if exp < -4 || exp >= 6 {
        let mant = if mant.contains('.') {
            mant.trim_end_matches('0').trim_end_matches('.')
        } else {
            mant
        };
        return format!("{}e{}{:02}", mant, if exp < 0 { '-' } else { '+' }, exp.abs());
    }
    let s = format!("{:.*}", (5 - exp).max(0) as usize, x);
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

fn vec3_offset(p: &Map<String, Value>) -> Result<Vec3, FeatureError> {
    let bad = || FeatureError("offset must be a number.".into());
    match p.get("offset") {
        None | Some(Value::Null) => Ok([0.0; 3]),
        Some(Value::Array(a)) if a.len() == 3 => {
            let elem = |v: &Value| -> Result<f64, FeatureError> {
                match v {
                    Value::Number(n) => n.as_f64().ok_or_else(bad),
                    Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
                    Value::String(s) => s.trim().parse::<f64>().map_err(|_| bad()),
                    _ => Err(bad()),
                }
            };
            Ok([elem(&a[0])?, elem(&a[1])?, elem(&a[2])?])
        }
        _ => Err(bad()),
    }
}

fn f_thread(
    state: &mut State,
    fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let kind = match p.get("kind") {
        None => "internal",
        Some(v) => v.as_str().unwrap_or(""),
    };
    let d = float(p, "diameter", Some(8.0))?;
    let pitch = float(p, "pitch", Some(1.25))?;
    let l = float(p, "length", Some(10.0))?;
    let tol = match p.get("tolerance") {
        None => 0.2,
        Some(v) if !truthy(Some(v)) => 0.0,
        Some(_) => float(p, "tolerance", Some(0.0))?,
    };
    let hand = match p.get("hand") {
        None | Some(Value::Null) => "right",
        Some(Value::String(s)) => s.as_str(),
        Some(_) => "",
    };
    let chamfer = match p.get("chamfer") {
        None => true,
        Some(v) => truthy(Some(v)),
    };
    let tool = thread_tool(kind, d, pitch, l, tol, hand, chamfer).map_err(|e| FeatureError(e.to_string()))?;
    let frame = state.frame(p.get("plane"))?;
    let off = vec3_offset(p)?;
    let tool = place(&tool, &frame, off);
    let op = match p.get("operation").and_then(Value::as_str) {
        Some(s) if !s.is_empty() => s,
        _ => {
            if kind == "internal" {
                "cut"
            } else {
                "new"
            }
        }
    };
    let default_name = if kind == "external" {
        format!("M{} screw", fmt_g(d))
    } else {
        "Thread".into()
    };
    let name = match p.get("body_name").and_then(Value::as_str) {
        Some(s) if !s.is_empty() => s,
        _ => default_name.as_str(),
    };
    apply_tool(state, fid, tool, op, p.get("targets"), name, res)
}

pub fn register(r: Registry) -> Registry {
    r.with("thread", f_thread)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_g_format() {
        assert_eq!(fmt_g(8.0), "8");
        assert_eq!(fmt_g(2.5), "2.5");
        assert_eq!(fmt_g(6.35), "6.35");
        assert_eq!(fmt_g(1.0 / 3.0), "0.333333");
        assert_eq!(fmt_g(123456.0), "123456");
        assert_eq!(fmt_g(1234567.0), "1.23457e+06");
        assert_eq!(fmt_g(0.0001), "0.0001");
        assert_eq!(fmt_g(0.00001), "1e-05");
    }

    #[test]
    fn null_kind_uses_internal_tool_and_new_operation() {
        use std::collections::BTreeMap;
        use crate::NullHost;

        let sources = BTreeMap::new();
        let fins = NullHost;
        let ctx = Ctx { sources: &sources, preview: false, fins: &fins };

        let mut state = State::default();
        let mut res = FeatureResult::default();
        let p: Map<String, Value> = serde_json::from_value(serde_json::json!({
            "kind": null,
            "diameter": 8,
            "pitch": 1.25,
            "length": 6
        }))
        .unwrap();

        f_thread(&mut state, "f1", &p, &mut res, &ctx).unwrap();
        assert_eq!(res.created, vec!["f1"]);
        let b = state.bodies.get("f1").unwrap();
        assert_eq!(b.name, "Thread");
        let bb = b.solid.as_ref().unwrap().bounding_box();
        // internal tool goes from z = -6 up to z = 0.6
        assert!((bb.min[2] - (-6.0)).abs() < 1e-4 && (bb.max[2] - 0.6).abs() < 1e-4, "{bb:?}");
    }
}
