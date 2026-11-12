//! Fit Test (1.x `f_fittest`): one plate with pegs of a fixed diameter and matching holes at
//! stepped clearances.

use serde_json::{Map, Value};
use stl_kernel::Solid;

use super::common::{apply_tool, float};
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

/// Python's `int(p.get("count", 6))`.
fn count_of(p: &Map<String, Value>) -> Result<i64, FeatureError> {
    let bad = || FeatureError("count must be a whole number.".into());
    match p.get("count") {
        None => Ok(6),
        Some(Value::Number(n)) => n.as_f64().map(|x| x.trunc() as i64).ok_or_else(bad),
        Some(Value::Bool(b)) => Ok(*b as i64),
        Some(Value::String(s)) => s.trim().parse::<i64>().map_err(|_| bad()),
        _ => Err(bad()),
    }
}

fn fittest(
    state: &mut State,
    fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let d = float(p, "diameter", Some(8.0))?;
    let start = float(p, "start", Some(0.05))?;
    let step = float(p, "step", Some(0.05))?;
    let count = count_of(p)?;
    let t = float(p, "thickness", Some(3.0))?;
    let gap = float(p, "gap", Some(4.0))?;
    if !(d > 0.0) {
        return Err(FeatureError("Peg diameter must be more than zero.".into()));
    }
    if start < 0.0 {
        return Err(FeatureError("Clearance start can't be negative.".into()));
    }
    if !(step > 0.0) {
        return Err(FeatureError("Clearance step must be more than zero.".into()));
    }
    if count < 1 {
        return Err(FeatureError("Count must be at least 1.".into()));
    }
    if !(t > 0.0) {
        return Err(FeatureError("Plate thickness must be more than zero.".into()));
    }
    if !(gap > 0.0) {
        return Err(FeatureError("Gap must be more than zero.".into()));
    }
    let clearances: Vec<f64> = (0..count).map(|i| start + i as f64 * step).collect();
    let max_hole = d + clearances[clearances.len() - 1];
    let cf = count as f64;
    let l = cf * max_hole + (cf + 1.0) * gap;
    let w = max_hole + d + 3.0 * gap;
    let seg = 64;
    let mut man = Solid::cube([l, w, t], false).translate([-l / 2.0, -w / 2.0, 0.0]);
    for (i, c) in clearances.iter().enumerate() {
        let x = -l / 2.0 + gap + max_hole / 2.0 + i as f64 * (max_hole + gap);
        let y_hole = -w / 2.0 + gap + max_hole / 2.0;
        let y_peg = -w / 2.0 + gap + max_hole + gap + d / 2.0;
        let hr = (d + c) / 2.0;
        man = man.difference(&Solid::cylinder(t + 2.0, hr, hr, seg, false).translate([x, y_hole, -1.0]));
        man = man.union(&Solid::cylinder(t, d / 2.0, d / 2.0, seg, false).translate([x, y_peg, t]));
    }
    let name = p.get("body_name").and_then(Value::as_str).filter(|s| !s.is_empty()).unwrap_or("Fit Test");
    apply_tool(state, fid, man, "new", None, name, res)?;
    res.info = Some(format!(
        "{count} pair{}, peg \u{00d8}{} mm, clearances {} mm.",
        if count != 1 { "s" } else { "" },
        fmt_g(d),
        clearances.iter().map(|c| fmt_g(*c)).collect::<Vec<_>>().join(", ")
    ));
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("fittest", fittest)
}
