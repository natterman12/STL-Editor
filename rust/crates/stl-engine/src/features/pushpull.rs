//! 1.x `f_pushpull`: push a flat face out (join) or pull it in (cut).

use serde_json::{Map, Value};
use stl_sketch::P2;

use super::common::{apply_tool, float, plane_frame, pushpull_tool, Vec3};
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn vec3(v: &Value) -> Option<Vec3> {
    let a = v.as_array()?;
    Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?])
}

fn contours(v: &Value) -> Option<Vec<Vec<P2>>> {
    let a = v.as_array()?;
    let mut out = Vec::with_capacity(a.len());
    for loop_v in a {
        let pts = loop_v.as_array()?;
        let mut loop_pts = Vec::with_capacity(pts.len());
        for p in pts {
            let u = p.as_array()?;
            loop_pts.push([u.first()?.as_f64()?, u.get(1)?.as_f64()?]);
        }
        out.push(loop_pts);
    }
    Some(out)
}

fn pushpull(
    state: &mut State,
    fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let bid = p.get("body").and_then(Value::as_str).unwrap_or("");
    let b = state.body(bid)?;
    b.require_solid()?;
    let face = match p.get("face") {
        Some(Value::Object(m)) => m,
        _ => {
            return Err(FeatureError("Click a flat face to push or pull.".into()));
        }
    };
    let cont = match face.get("contours").and_then(contours) {
        Some(c) if !c.is_empty() => c,
        _ => return Err(FeatureError("Click a flat face to push or pull.".into())),
    };
    let origin = face
        .get("origin")
        .and_then(vec3)
        .ok_or_else(|| FeatureError("Click a flat face to push or pull.".into()))?;
    let normal = face
        .get("normal")
        .and_then(vec3)
        .ok_or_else(|| FeatureError("Click a flat face to push or pull.".into()))?;
    let xdir = face.get("xdir").and_then(vec3);
    let fr = plane_frame(origin, normal, xdir)?;
    let d = float(p, "distance", Some(5.0))?;
    let tool = pushpull_tool(&cont, &fr, d)?;
    let op = if d > 0.0 { "join" } else { "cut" };
    let targets = Value::Array(vec![Value::String(bid.to_string())]);
    apply_tool(state, fid, tool, op, Some(&targets), "Push/Pull", res)
}

pub fn register(r: Registry) -> Registry {
    r.with("pushpull", pushpull)
}
