//! 1.x `f_sketch`: store a sketch (its plane, entities and part-edge references) for later
//! features to use.

use serde_json::{Map, Value};
use stl_sketch::entities::parse_entities;

use super::common::SketchRec;
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn sketch(
    state: &mut State,
    fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let frame = state.frame(p.get("plane"))?;
    let name = match p.get("name") {
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        _ => "Sketch".to_string(),
    };
    let list: Vec<Value> = match p.get("entities") {
        Some(Value::Array(a)) => a.clone(),
        _ => Vec::new(),
    };
    let entities = parse_entities(&list).map_err(FeatureError)?;
    let refs: Vec<Value> = match p.get("refs") {
        Some(Value::Array(a)) => a.clone(),
        _ => Vec::new(),
    };
    state.sketches.insert(fid.to_string(), SketchRec { name, frame, entities, refs });
    res.created_sketch = Some(fid.to_string());
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("sketch", sketch)
}
