//! Thin-wall Check (1.x `f_thincheck`): reports walls, pins and ribs thinner than a minimum;
//! changes nothing.

use serde_json::{Map, Value};
use stl_mesh::thin::thin_scan;

use super::common::float;
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

/// Python's `f"{x:g}"`.
fn fmt_g(x: f64) -> String {
    if x == 0.0 {
        return if x.is_sign_negative() { "-0".into() } else { "0".into() };
    }
    let e = format!("{:.5e}", x);
    let (mant, exp) = e.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    if exp < -4 || exp >= 6 {
        let mant = if mant.contains('.') { mant.trim_end_matches('0').trim_end_matches('.') } else { mant };
        return format!("{}e{}{:02}", mant, if exp < 0 { '-' } else { '+' }, exp.abs());
    }
    let s = format!("{:.*}", (5 - exp).max(0) as usize, x);
    if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s }
}

fn f_thincheck(
    state: &mut State,
    _fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let bid = p.get("body").and_then(Value::as_str).unwrap_or("");
    if bid.is_empty() {
        return Err(FeatureError("Choose a body to check.".into()));
    }
    let t = float(p, "min", Some(0.8))?;
    if t <= 0.0 {
        return Err(FeatureError("The minimum thickness must be more than zero.".into()));
    }
    let r = thin_scan(state.body(bid)?.require_solid()?, t);
    res.thin = Some(serde_json::json!({"min": r.min, "count": r.count, "spots": r.spots}));
    if r.count == 0 {
        res.info = Some(format!("Nothing thinner than {} mm found.", fmt_g(t)));
        return Ok(());
    }
    let near: Vec<String> = r
        .spots
        .iter()
        .take(3)
        .map(|s| format!("X {}, Y {}, Z {}", fmt_g(s[0]), fmt_g(s[1]), fmt_g(s[2])))
        .collect();
    res.warning = Some(format!(
        "{} thin spot{} under {} mm. Thinnest: about {} mm. Near {} (mm).",
        r.count,
        if r.count != 1 { "s" } else { "" },
        fmt_g(t),
        fmt_g(r.min.unwrap_or(0.0)),
        near.join("; ")
    ));
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("thincheck", f_thincheck)
}
