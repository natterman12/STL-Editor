//! Import (1.x `f_import`): bring the parts of an imported file into the design, repairing
//! small defects; a part that still isn't watertight is kept as a raw mesh.

use std::sync::Arc;

use serde_json::{Map, Value};
use stl_kernel::{MeshArrays, Solid};

use crate::state::Body;
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

/// 1.x `manifold_from_arrays`: a solid, or `None` unless the mesh is a closed 2-manifold.
fn solid_from_arrays(v: &[f64], f: &[u32]) -> Option<Solid> {
    if f.is_empty() {
        return None;
    }
    let arrays = MeshArrays { positions: v.iter().map(|&x| x as f32).collect(), triangles: f.to_vec() };
    Solid::from_mesh(&arrays).ok().filter(|s| !s.is_empty())
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        None => true, // repair defaults to on
        Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

fn f_import(
    state: &mut State,
    fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let sid = p.get("source").and_then(Value::as_str).unwrap_or("");
    let src = ctx.source(sid)?;
    let parts = stl_io::load_mesh_file(&src.data, &src.name).map_err(|e| FeatureError(e.0))?;
    let repair = truthy(p.get("repair"));
    // "center": put the part on the print bed at the origin (bottom on Z=0, centred in X/Y).
    // "keep": the coordinates stored in the file. Older projects without the key keep them.
    let place = p.get("place").and_then(Value::as_str).unwrap_or("keep");
    let mut shift = [0.0f64; 3];
    if place == "center" {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for part in &parts {
            for v in part.positions.chunks_exact(3) {
                for i in 0..3 {
                    lo[i] = lo[i].min(v[i]);
                    hi[i] = hi[i].max(v[i]);
                }
            }
        }
        shift = [-(lo[0] + hi[0]) / 2.0, -(lo[1] + hi[1]) / 2.0, -lo[2]];
        // already something on the bed: put the new part next to it instead of inside it
        let mut ohi: Option<f64> = None;
        for b in state.bodies.values() {
            let solid_mesh;
            let mesh: &MeshArrays = match (&b.solid, &b.mesh) {
                (Some(s), _) => {
                    solid_mesh = s.to_mesh();
                    &solid_mesh
                }
                (None, Some(m)) => m,
                (None, None) => continue,
            };
            for v in mesh.positions.chunks_exact(3) {
                let x = v[0] as f64;
                ohi = Some(ohi.map_or(x, |h| h.max(x)));
            }
        }
        if let Some(ohi) = ohi {
            shift[0] = ohi + 10.0 - lo[0];
        }
    }
    let name_param = p.get("name").and_then(Value::as_str).filter(|s| !s.is_empty());
    let n = parts.len();
    let mut broken = 0usize;
    for (i, part) in parts.iter().enumerate() {
        let v: Vec<f64> = part.positions.chunks_exact(3).flat_map(|c| (0..3).map(move |k| c[k] + shift[k])).collect();
        let f = &part.triangles;
        let bid = if n == 1 { fid.to_string() } else { format!("{fid}.{}", i + 1) };
        let mut name = name_param.map_or_else(|| part.name.clone(), String::from);
        if n > 1 {
            name = match name_param {
                Some(nm) => format!("{nm} {}", i + 1),
                None => part.name.clone(),
            };
        }
        let mut man = solid_from_arrays(&v, f);
        if man.is_none() && repair {
            man = stl_mesh::local_repair(&v, f);
            if man.is_none() {
                let (rv, rf) = stl_mesh::quick_repair(&v, f);
                man = solid_from_arrays(&rv, &rf);
            }
            if man.is_some() {
                res.warning = Some(
                    "The mesh had small defects (open or doubled edges); they were fixed automatically.".into(),
                );
            }
        }
        if let Some(m) = man {
            let (m, merged) = stl_mesh::merge_overlapping_shells(&m);
            if merged > 0 {
                let rest = res.warning.as_ref().map(|w| format!(" {w}")).unwrap_or_default();
                res.warning = Some(format!("{merged} overlapping shells were merged into one solid.{rest}"));
            }
            state.bodies.insert(bid.clone(), Body { name, solid: Some(m), mesh: None });
        } else {
            let mesh = MeshArrays { positions: v.iter().map(|&x| x as f32).collect(), triangles: f.clone() };
            state.bodies.insert(bid.clone(), Body { name, solid: None, mesh: Some(Arc::new(mesh)) });
            broken += 1;
        }
        res.created.push(bid);
    }
    if broken > 0 {
        res.warning = Some(format!(
            "{broken} mesh(es) are not watertight. They display and export, but editing needs Repair (Rebuild) first."
        ));
    }
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("import", f_import)
}
