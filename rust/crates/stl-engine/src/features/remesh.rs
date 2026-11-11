//! Remesh (1.x `f_remesh`): even out the triangles to about one edge length.

use serde_json::{Map, Value};

use super::common::float;
use crate::state::Body;
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn f_remesh(
    state: &mut State,
    _fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let bid = p.get("body").and_then(Value::as_str).unwrap_or("").to_string();
    let b = state.body(&bid)?.clone();
    let s = b.require_solid()?;
    let edge = float(p, "edge", Some(1.0))?;
    let out = stl_mesh::remesh::remesh(s, edge, 5).map_err(|e| FeatureError(e.to_string()))?;
    state.bodies.insert(bid.clone(), Body { name: b.name, solid: Some(out), mesh: None });
    res.modified.push(bid);
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("remesh", f_remesh)
}
