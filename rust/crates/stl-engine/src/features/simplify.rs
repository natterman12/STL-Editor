//! Simplify (1.x `f_simplify`): fewer triangles, by a share to keep or a tolerance.

use std::sync::Arc;

use serde_json::{Map, Value};
use stl_kernel::MeshArrays;
use stl_mesh::simplify::{simplify, Simplified};

use super::common::{float, truthy};
use crate::state::Body;
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn f_simplify(
    state: &mut State,
    _fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let bid = p.get("body").and_then(Value::as_str).unwrap_or("").to_string();
    let b = state.body(&bid)?.clone();
    // 1.x: `float(ratio) if ratio else None`, so 0 means "not given"
    let ratio = if truthy(p.get("ratio")) { Some(float(p, "ratio", None)?) } else { None };
    let tolerance = match p.get("tolerance") {
        None | Some(Value::Null) => None,
        Some(_) => Some(float(p, "tolerance", None)?),
    };
    let mesh = match (&b.solid, &b.mesh) {
        (Some(s), _) => s.to_mesh(),
        (None, Some(m)) => (**m).clone(),
        (None, None) => MeshArrays::default(),
    };
    let v: Vec<[f32; 3]> = mesh.positions.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
    let f: Vec<[u32; 3]> = mesh.triangles.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
    match simplify(&v, &f, b.solid.as_ref(), ratio, tolerance) {
        Simplified::Raw(nv, nf) => {
            let arrays = MeshArrays {
                positions: nv.iter().flat_map(|p| p.iter().map(|&x| x as f32)).collect(),
                triangles: nf.iter().flatten().copied().collect(),
            };
            state.bodies.insert(bid.clone(), Body { name: b.name, solid: None, mesh: Some(Arc::new(arrays)) });
            res.warning = Some("Simplified mesh isn't watertight; run Repair to edit it.".into());
        }
        Simplified::Solid(s) => {
            state.bodies.insert(bid.clone(), Body { name: b.name, solid: Some(s), mesh: None });
        }
    }
    res.modified.push(bid);
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("simplify", f_simplify)
}
