//! Hollow (1.x `f_hollow`): leave a wall of the given thickness and empty the inside.

use serde_json::{Map, Value};

use super::common::float;
use super::common::pitch::hollow_pitch;
use crate::state::Body;
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn f_hollow(
    state: &mut State,
    _fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let bid = p.get("body").and_then(Value::as_str).unwrap_or("");
    if bid.is_empty() {
        // 1.x: "Body None no longer exists."; see EXCEPTIONS.md.
        return Err(FeatureError("Choose a body first.".into()));
    }
    let b = state.body(bid)?.clone();
    let s = b.require_solid()?;
    let t = float(p, "thickness", Some(2.0))?;
    let pitch = hollow_pitch(s, t, p.get("resolution").and_then(Value::as_str));
    let out = stl_mesh::voxel::hollow(s, t, Some(pitch), false).map_err(|e| FeatureError(e.to_string()))?;
    state.bodies.insert(bid.to_string(), Body { name: b.name.clone(), solid: Some(out), mesh: None });
    res.modified.push(bid.to_string());
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("hollow", f_hollow)
}
