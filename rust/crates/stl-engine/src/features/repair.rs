//! Repair (1.x `f_repair`): make a broken mesh a watertight solid. Modes `auto` and `quick`
//! keep the original triangles; Rebuild (and `auto` falling back to it) needs the voxel tools.

use serde_json::{Map, Value};
use stl_kernel::{MeshArrays, Solid};

use crate::state::Body;
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

/// Temporary, until the voxel tools are ported (batch 3): 1.x calls `voxel_rebuild` here.
const NEEDS_VOXEL: &str = "Rebuild needs the voxel tools, which aren't ported yet.";

fn f_repair(
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
    // Body.mesh(): the solid's triangles, or the raw mesh
    let mesh = match (&b.solid, &b.mesh) {
        (Some(s), _) => s.to_mesh(),
        (None, Some(m)) => (**m).clone(),
        (None, None) => MeshArrays::default(),
    };
    let v: Vec<f64> = mesh.positions.iter().map(|&x| x as f64).collect();
    let f = mesh.triangles;
    let mode = match p.get("mode") {
        None => "auto",
        Some(m) => m.as_str().unwrap_or(""),
    };
    let mut man: Option<Solid> = None;
    if mode == "auto" || mode == "quick" {
        man = stl_mesh::local_repair(&v, &f);
        if man.is_none() {
            let (rv, rf) = stl_mesh::quick_repair(&v, &f);
            // manifold_from_arrays: positions as f32, None unless it is a closed solid
            if !rf.is_empty() {
                let arrays = MeshArrays { positions: rv.iter().map(|&x| x as f32).collect(), triangles: rf };
                man = Solid::from_mesh(&arrays).ok();
            }
        }
        if man.is_none() && mode == "quick" {
            return Err(FeatureError("Quick repair wasn't enough. Use Rebuild.".into()));
        }
    }
    let Some(man) = man else {
        return Err(FeatureError(NEEDS_VOXEL.into()));
    };
    state.bodies.insert(bid.to_string(), Body { name: b.name.clone(), solid: Some(man), mesh: None });
    res.modified.push(bid.to_string());
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("repair", f_repair)
}
