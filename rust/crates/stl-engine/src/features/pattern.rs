//! 1.x `f_pattern`: copies of bodies (or of earlier features) in a line or round an axis.

use serde_json::{Map, Value};
use stl_kernel::Solid;

use super::common::frames::{unit, Vec3};
use super::common::pose::par_map;
use super::common::{affine, float, repeat_features, strings, truthy};
use crate::state::Body;
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn vec3(p: &Map<String, Value>, k: &str, default: Vec3) -> Result<Vec3, FeatureError> {
    match p.get(k) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => {
            let bad = || FeatureError(format!("{k} must be three numbers."));
            let a = v.as_array().ok_or_else(bad)?;
            if a.len() != 3 {
                return Err(bad());
            }
            Ok([a[0].as_f64().ok_or_else(bad)?, a[1].as_f64().ok_or_else(bad)?, a[2].as_f64().ok_or_else(bad)?])
        }
    }
}

/// `int(p.get("count", 3))`, which must be at least 2.
fn count(p: &Map<String, Value>) -> Result<i64, FeatureError> {
    let c = float(p, "count", Some(3.0))?.trunc() as i64;
    if c < 2 {
        return Err(FeatureError("Count must be at least 2.".into()));
    }
    Ok(c)
}

/// The move for copy number `i` (1-based).
fn transform(p: &Map<String, Value>, count: i64, i: i64) -> Result<[[f64; 4]; 3], FeatureError> {
    let eye = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let kind = p.get("kind").and_then(Value::as_str);
    if p.get("kind").is_none() || kind == Some("linear") {
        let d = unit(vec3(p, "direction", [1.0, 0.0, 0.0])?)?;
        let sp = float(p, "spacing", Some(10.0))? * i as f64;
        return Ok(affine(eye, [d[0] * sp, d[1] * sp, d[2] * sp]));
    }
    let axis = unit(vec3(p, "axis", [0.0, 0.0, 1.0])?)?;
    let c = vec3(p, "center", [0.0; 3])?;
    let total = float(p, "angle", Some(360.0))?;
    let step = if total.abs() >= 360.0 { total / count as f64 } else { total / (count - 1) as f64 };
    let a = (step * i as f64).to_radians();
    let k = [[0.0, -axis[2], axis[1]], [axis[2], 0.0, -axis[0]], [-axis[1], axis[0], 0.0]];
    let mut r = [[0.0; 3]; 3];
    for x in 0..3 {
        for y in 0..3 {
            let kk = k[x][0] * k[0][y] + k[x][1] * k[1][y] + k[x][2] * k[2][y];
            r[x][y] = (if x == y { 1.0 } else { 0.0 }) + a.sin() * k[x][y] + (1.0 - a.cos()) * kk;
        }
    }
    let rc = [0, 1, 2].map(|x| r[x][0] * c[0] + r[x][1] * c[1] + r[x][2] * c[2]);
    Ok(affine(r, [c[0] - rc[0], c[1] - rc[1], c[2] - rc[2]]))
}

fn f_pattern(
    state: &mut State,
    fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    if p.get("source").and_then(Value::as_str) == Some("features") {
        let ids = strings(p.get("features"));
        let n = count(p)?;
        let ts = (1..n).map(|i| transform(p, n, i)).collect::<Result<Vec<_>, _>>()?;
        return repeat_features(state, fid, &ids, &ts, res, "pattern");
    }
    let bids = strings(p.get("bodies"));
    if bids.is_empty() {
        return Err(FeatureError("Choose a body to pattern.".into()));
    }
    let n = count(p)?;
    let join = p.get("join").map_or(true, |v| truthy(Some(v)));
    for bid in bids {
        let b = state.body(&bid)?.clone();
        let s = b.require_solid()?.clone();
        let ts = (1..n).map(|i| transform(p, n, i)).collect::<Result<Vec<_>, _>>()?;
        let mut copies = vec![s.clone()];
        copies.extend(par_map(ts.len(), |k| s.transform(ts[k])));
        if join {
            let u = Solid::union_all(&copies);
            state.bodies.insert(bid.clone(), Body { solid: Some(u), ..b });
            res.modified.push(bid);
        } else {
            for (i, cpy) in copies.into_iter().enumerate().skip(1) {
                let i = i + 1;
                let nid = format!("{fid}.{bid}.{i}");
                state.bodies.insert(nid.clone(), Body { name: format!("{} ({i})", b.name), solid: Some(cpy), mesh: None });
                res.created.push(nid);
            }
        }
    }
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("pattern", f_pattern)
}
