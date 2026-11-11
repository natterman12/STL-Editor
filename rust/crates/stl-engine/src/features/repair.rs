//! Repair (1.x `f_repair`): make a broken mesh a watertight solid. Modes `auto` and `quick`
//! keep the original triangles; Rebuild (and `auto` falling back to it) goes through a voxel grid.

use serde_json::{Map, Value};
use stl_kernel::{MeshArrays, Solid};

use super::common::pitch::fmt_g3;
use super::common::{float, truthy};
use crate::state::Body;
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

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
    let man = match man {
        Some(m) => m,
        None => {
            let pitch = if truthy(p.get("voxel")) {
                float(p, "voxel", None)?
            } else {
                let target = match p.get("resolution").and_then(Value::as_str) {
                    Some("draft") => 120.0,
                    Some("fine") => 360.0,
                    _ => 220.0,
                };
                let pts: Vec<[f32; 3]> = mesh.positions.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
                stl_mesh::voxel::auto_pitch(&pts, target)
            };
            let verts: Vec<[f64; 3]> = v.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
            let faces: Vec<[u32; 3]> = f.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
            let m = stl_mesh::voxel::voxel_rebuild(&verts, &faces, pitch, 0.6)
                .map_err(|e| FeatureError(e.to_string()))?;
            res.warning = Some(format!("Rebuilt as a watertight solid on a {} mm grid.", fmt_g3(pitch)));
            m
        }
    };
    state.bodies.insert(bid.to_string(), Body { name: b.name.clone(), solid: Some(man), mesh: None });
    res.modified.push(bid.to_string());
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("repair", f_repair)
}
