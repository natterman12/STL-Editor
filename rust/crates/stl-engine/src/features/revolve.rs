//! 1.x `f_revolve`: spin a sketch profile around an axis into a solid.

use serde_json::{Map, Value};
use stl_kernel::{cross, Shape2d, Solid};
use stl_sketch::ops::find_axis;
use stl_sketch::P2;

use super::common::{apply_tool, float, pick, sketch};
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn revolve(
    state: &mut State,
    fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let sk = sketch(&state.sketches, p.get("sketch").and_then(Value::as_str))?.clone();
    let (only, regions) = pick(p);
    let cs = stl_sketch::profile::profile_with(&sk.entities, only.as_deref(), regions.as_deref())
        .map_err(|e| FeatureError(e.0))?;
    let axis = match p.get("axis") {
        None => "y".to_string(),
        Some(v) => v.as_str().unwrap_or("").to_string(),
    };
    let (p0, d): (P2, P2) = match axis.as_str() {
        "x" => ([0.0, 0.0], [1.0, 0.0]),
        "y" => ([0.0, 0.0], [0.0, 1.0]),
        id => find_axis(&sk.entities, id).map_err(|e| FeatureError(e.0))?,
    };
    let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
    let e = [d[0] / len, d[1] / len];
    let f = [e[1], -e[0]];
    // 2D map: sketch (u,v) -> local (a,b): a along f (radial), b along e (axis)
    let local: Vec<Vec<P2>> = cs
        .contours
        .iter()
        .map(|c| {
            c.iter()
                .map(|q| {
                    let (u, v) = (q[0] - p0[0], q[1] - p0[1]);
                    [f[0] * u + f[1] * v, e[0] * u + e[1] * v]
                })
                .collect()
        })
        .collect();
    let ang = float(p, "angle", Some(360.0))?;
    let seg = float(p, "segments", Some(128.0))? as u32;
    // keep each side of the axis; mirror the negative side so it revolves too (the kernel's
    // revolve drops whatever is at negative x), as 1.x does with a clip rectangle
    let pos = Shape2d { contours: local };
    let parts: Vec<Solid> = pos.revolve_halves().iter().map(|part| part.revolve(seg, ang)).collect();
    if parts.is_empty() {
        return Err(FeatureError("The profile is empty.".into()));
    }
    let man = if parts.len() > 1 { Solid::union_all(&parts) } else { parts[0].clone() };
    let fr = sk.frame;
    let ff = [0, 1, 2].map(|i| f[0] * fr.x[i] + f[1] * fr.y[i]);
    let ee = [0, 1, 2].map(|i| e[0] * fr.x[i] + e[1] * fr.y[i]);
    let gd = cross(ee, ff);
    let origin = [0, 1, 2].map(|i| fr.o[i] + p0[0] * fr.x[i] + p0[1] * fr.y[i]);
    let m = [0, 1, 2].map(|i| [ff[i], gd[i], ee[i], origin[i]]);
    let name = match p.get("body_name") {
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        _ => "Revolve".to_string(),
    };
    let op = match p.get("operation") {
        None => "new".to_string(),
        Some(v) => v.as_str().unwrap_or("").to_string(),
    };
    apply_tool(state, fid, man.transform(m), &op, p.get("targets"), &name, res)
}

pub fn register(r: Registry) -> Registry {
    r.with("revolve", revolve)
}
