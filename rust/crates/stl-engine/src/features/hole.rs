//! 1.x `f_hole`: cut a simple, counterbore, countersink or hex hole through or into a body.

use serde_json::{Map, Value};
use stl_kernel::Solid;

use super::common::frames::{Vec3, unit};
use super::common::placement::{lower_first, parse_refs, ref_body, resolve_point};
use super::common::{apply_tool, big_extent, float, make_cylinder, truthy};
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn vec3(v: &Value) -> Option<Vec3> {
    let a = v.as_array()?;
    Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?])
}

fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn mul(a: Vec3, s: f64) -> Vec3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn hole(
    state: &mut State,
    fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    // 1.x f_hole ignores `thread`; threads are a separate feature.
    let mut pts: Vec<Vec3> = match p.get("points") {
        Some(Value::Array(a)) if !a.is_empty() => a.iter().filter_map(vec3).collect(),
        _ => match p.get("point").and_then(vec3) {
            Some(pt) => vec![pt],
            None => Vec::new(),
        },
    };
    if pts.is_empty() {
        return Err(FeatureError("Click on a face to place the hole.".into()));
    }
    let targets = p.get("targets");
    let n = unit(p.get("normal").and_then(vec3).unwrap_or([0.0, 0.0, 1.0]))?;
    // M7: points measured from part edges or sketch lines are re-placed from those references;
    // without `point_refs` nothing here runs, so older projects build exactly as before
    let mut notes: Vec<String> = Vec::new();
    let refs = parse_refs(p.get("point_refs"));
    if refs.iter().any(|r| !r.is_empty()) {
        let raw = p.get("point_refs").and_then(Value::as_array);
        let first_target = targets
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .and_then(Value::as_str)
            .unwrap_or("");
        for (i, pt) in pts.iter_mut().enumerate() {
            let Some(list) = refs.get(i).filter(|l| !l.is_empty()) else {
                continue;
            };
            let body = ref_body(raw.and_then(|a| a.get(i))).unwrap_or_else(|| first_target.to_owned());
            let (np, warn) = resolve_point(state, &body, *pt, n, list)?;
            *pt = np;
            if let Some(w) = warn {
                notes.push(format!("Hole point {}: {}", i + 1, lower_first(&w)));
            }
        }
    }
    let d = float(p, "diameter", Some(5.0))?;
    let r = d / 2.0;
    let tbodies_extent = {
        let use_list = truthy(targets);
        if use_list {
            let ids: Vec<&str> = targets
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter(|t| state.bodies.contains_key(*t))
                .collect();
            big_extent(ids.iter().filter_map(|t| state.bodies.get(*t)))
        } else {
            big_extent(state.bodies.values().filter(|b| b.solid.is_some()))
        }
    };
    let through = match p.get("through") {
        None | Some(Value::Null) => true,
        Some(v) => truthy(Some(v)),
    };
    let depth = if through || !truthy(p.get("depth")) {
        tbodies_extent
    } else {
        float(p, "depth", Some(10.0))?
    };
    let kind = p.get("kind").and_then(Value::as_str).unwrap_or("simple");
    let tip = match p.get("tip") {
        None | Some(Value::Null) => true,
        Some(v) => truthy(Some(v)),
    };
    let eps = 0.01;
    let neg_n = mul(n, -1.0);
    let mut tools: Vec<Solid> = Vec::new();
    for pt in pts {
        let top = add(pt, mul(n, eps));
        tools.push(make_cylinder(top, neg_n, depth + eps, r, None, 96)?);
        if !through && truthy(p.get("depth")) && tip {
            let tip_h = r / (118.0_f64 / 2.0).to_radians().tan();
            tools.push(make_cylinder(
                add(pt, mul(n, -depth)),
                neg_n,
                tip_h,
                r,
                Some(0.001),
                96,
            )?);
        }
        match kind {
            "counterbore" => {
                let cbd = float(p, "cb_diameter", Some(d * 1.8))?;
                let cbh = float(p, "cb_depth", Some(d * 0.6))?;
                tools.push(make_cylinder(
                    add(pt, mul(n, 1.0)),
                    neg_n,
                    cbh + 1.0,
                    cbd / 2.0,
                    None,
                    96,
                )?);
            }
            "hex" => {
                let af = float(p, "hex_af", Some(d * 1.8))?;
                let hd = float(p, "hex_depth", Some(3.0))?;
                let rr = af / 3.0_f64.sqrt();
                tools.push(make_cylinder(add(pt, mul(n, 1.0)), neg_n, hd + 1.0, rr, Some(rr), 6)?);
            }
            "countersink" => {
                let csd = float(p, "cs_diameter", Some(d * 2.0))?;
                let ang = float(p, "cs_angle", Some(90.0))?;
                let h = (csd - d) / 2.0 / (ang / 2.0).to_radians().tan();
                let extra = 1.0;
                let r_top = csd / 2.0 + extra * (ang / 2.0).to_radians().tan();
                tools.push(make_cylinder(
                    add(pt, mul(n, extra)),
                    neg_n,
                    h + extra,
                    r_top,
                    Some(r),
                    96,
                )?);
            }
            _ => {}
        }
    }
    let tool = if tools.len() > 1 {
        Solid::union_all(&tools)
    } else {
        tools.remove(0)
    };
    res.set_cutter(ctx.preview, &tool);
    apply_tool(state, fid, tool, "cut", targets, "Hole", res)?;
    add_notes(res, notes);
    Ok(())
}

/// Put placement notes after any warning the cut already gave.
pub(crate) fn add_notes(res: &mut FeatureResult, notes: Vec<String>) {
    if notes.is_empty() {
        return;
    }
    let mut all: Vec<String> = res.warning.take().into_iter().collect();
    all.extend(notes);
    res.warning = Some(all.join(" "));
}

pub fn register(r: Registry) -> Registry {
    r.with("hole", hole)
}
