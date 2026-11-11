//! Sculpt (1.x `f_sculpt`): brush strokes that smooth, flatten, inflate or deflate.

use serde_json::{Map, Value};
use stl_mesh::sculpt::{sculpt, Stroke};

use super::common::truthy;
use crate::state::Body;
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn f_sculpt(
    state: &mut State,
    _fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let bid = p.get("body").and_then(Value::as_str).unwrap_or("").to_string();
    let b = state.body(&bid)?.clone();
    if !truthy(p.get("strokes")) {
        return Err(FeatureError("Drag over the model to sculpt.".into()));
    }
    let strokes: Vec<Stroke> = serde_json::from_value(p["strokes"].clone())
        .map_err(|_| FeatureError("The sculpt strokes are not valid.".into()))?;
    let out = sculpt(b.require_solid()?, &strokes).map_err(|e| FeatureError(e.to_string()))?;
    state.bodies.insert(bid.clone(), Body { name: b.name, solid: Some(out), mesh: None });
    res.modified.push(bid);
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("sculpt", f_sculpt)
}
