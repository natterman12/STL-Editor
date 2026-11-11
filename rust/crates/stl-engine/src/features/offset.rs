//! Offset (1.x `f_offset`): grow or shrink whole bodies by a distance.

use serde_json::{Map, Value};
use stl_mesh::offset::offset_solid;

use super::common::{float, strings, truthy};
use crate::state::Body;
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn f_offset(
    state: &mut State,
    _fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    if !truthy(p.get("bodies")) {
        return Err(FeatureError("Choose a body to offset.".into()));
    }
    let bids = strings(p.get("bodies"));
    let d = float(p, "distance", Some(0.2))?;
    let shrink = p.get("direction").and_then(Value::as_str).unwrap_or("grow") == "shrink";
    let d = if shrink { -d.abs() } else { d.abs() };
    if d.abs() < 1e-6 {
        return Err(FeatureError("Distance can't be zero.".into()));
    }
    for bid in bids {
        let b = state.body(&bid)?.clone();
        let out = offset_solid(b.require_solid()?, d, 12000).map_err(|e| FeatureError(e.to_string()))?;
        state.bodies.insert(bid.clone(), Body { name: b.name, solid: Some(out), mesh: None });
        res.modified.push(bid);
    }
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("offset", f_offset)
}
