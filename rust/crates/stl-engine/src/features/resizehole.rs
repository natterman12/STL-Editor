//! Resize Hole (1.x `f_resizehole`): change a round hole or pin to a new diameter, or fill a hole.

use serde_json::{Map, Value};
use stl_mesh::fit::{fit_cylinder, nearest_triangle, CylKind};

use super::common::round::resize_round;
use super::common::{float, truthy};
use crate::state::Body;
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn resizehole(
    state: &mut State,
    _fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let bid = p.get("body").and_then(Value::as_str).unwrap_or("");
    if bid.is_empty() || !truthy(p.get("point")) {
        return Err(FeatureError("Click the wall of a round hole or pin.".into()));
    }
    let b = state.body(bid)?.clone();
    let s = b.require_solid()?;
    let point = p
        .get("point")
        .and_then(Value::as_array)
        .and_then(|a| Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?]))
        .ok_or_else(|| FeatureError("point must be a number.".into()))?;
    let m = s.to_mesh();
    let v: Vec<[f32; 3]> = m.positions.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
    let f: Vec<[u32; 3]> = m.triangles.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
    if f.is_empty() {
        return Err(FeatureError("Click the curved wall of a round hole or pin.".into()));
    }
    let tri = nearest_triangle(&v, &f, point);
    let cyl = fit_cylinder(&v, &f, tri, 35.0).map_err(|e| FeatureError(e.to_string()))?;
    // Python round(x, 3): formatting rounds the exact binary value, ties to even
    let r3 = |x: f64| format!("{x:.3}").parse::<f64>().unwrap_or(x);
    res.extra.insert(
        "info".into(),
        serde_json::json!({
            "kind": if cyl.kind == CylKind::Hole { "hole" } else { "pin" },
            "diameter": r3(2.0 * cyl.radius),
            "length": r3(cyl.z1 - cyl.z0),
        }),
    );
    let fill = p.get("mode").and_then(Value::as_str) == Some("fill");
    let diameter = if fill { None } else { Some(float(p, "diameter", Some(2.0 * cyl.radius))?) };
    if fill && cyl.kind != CylKind::Hole {
        return Err(FeatureError(
            "Fill works on holes. To remove a pin, cut it off with Split or Extrude › Cut.".into(),
        ));
    }
    if !fill && !diameter.is_some_and(|d| d > 0.0) {
        return Err(FeatureError("Enter the new diameter.".into()));
    }
    let out = resize_round(s, &cyl, diameter, fill);
    if out.is_empty() {
        return Err(FeatureError("The result is empty.".into()));
    }
    state.bodies.insert(bid.to_string(), Body { name: b.name, solid: Some(out), mesh: None });
    res.modified.push(bid.to_string());
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("resizehole", resizehole)
}
