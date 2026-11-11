//! Lattice (1.x `f_lattice`): hollow the body (solid wall kept outside) and fill the hollow
//! with a cubic grid of round struts. Grid lines are spread evenly across the hollow space,
//! about one per cell size.

use serde_json::{Map, Value};
use stl_kernel::Solid;

use super::common::float;
use super::common::pitch::hollow_pitch;
use crate::state::Body;
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn f_lattice(
    state: &mut State,
    _fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let bid = p.get("body").and_then(Value::as_str).unwrap_or("");
    if bid.is_empty() {
        return Err(FeatureError("Choose a body to fill with a lattice.".into()));
    }
    let b = state.body(bid)?.clone();
    let s = b.require_solid()?;
    let cell = float(p, "cell", Some(6.0))?;
    let d = float(p, "strut", Some(1.0))?;
    let t = float(p, "thickness", Some(2.0))?;
    if cell <= 0.0 || d <= 0.0 || t <= 0.0 {
        return Err(FeatureError("Cell size, strut diameter and wall thickness must all be above zero.".into()));
    }
    if d >= cell {
        return Err(FeatureError(
            "The struts are as wide as the cells, so there'd be no gaps. Use a thinner strut or a bigger cell.".into(),
        ));
    }
    let pitch = hollow_pitch(s, t, p.get("resolution").and_then(Value::as_str));
    let cavity = stl_mesh::voxel::hollow(s, t, Some(pitch), true).map_err(|e| FeatureError(e.to_string()))?;
    let bb = cavity.bounding_box();
    let (lo, hi) = (bb.min, bb.max);
    let size: [f64; 3] = std::array::from_fn(|i| hi[i] - lo[i]);
    // counted in f64: a tiny cell would overflow whole-number maths
    let nf: [f64; 3] = std::array::from_fn(|i| (size[i] / cell).round_ties_even().max(1.0));
    if nf[1] * nf[2] + nf[0] * nf[2] + nf[0] * nf[1] > 2500.0 {
        return Err(FeatureError("That's too many struts. Use a bigger cell size.".into()));
    }
    let n: [usize; 3] = std::array::from_fn(|i| nf[i] as usize);
    let pos: Vec<Vec<f64>> = (0..3)
        .map(|a| (0..n[a]).map(|i| lo[a] + (i as f64 + 0.5) * size[a] / n[a] as f64).collect())
        .collect();
    let r = d / 2.0;
    let mut struts: Vec<Solid> = Vec::new();
    for a in 0..3 {
        let others: Vec<usize> = (0..3).filter(|&k| k != a).collect();
        let (o1, o2) = (others[0], others[1]);
        let mut cyl = Solid::cylinder(size[a] + 2.0, r, r, 16, false);
        cyl = match a {
            0 => cyl.rotate_deg([0.0, 90.0, 0.0]),
            1 => cyl.rotate_deg([-90.0, 0.0, 0.0]),
            _ => cyl,
        };
        for &u in &pos[o1] {
            for &w in &pos[o2] {
                let mut off = [0.0; 3];
                off[a] = lo[a] - 1.0;
                off[o1] = u;
                off[o2] = w;
                struts.push(cyl.translate(off));
            }
        }
    }
    let grid = Solid::union_all(&struts);
    let out = s.difference(&cavity.difference(&grid));
    state.bodies.insert(bid.to_string(), Body { name: b.name.clone(), solid: Some(out), mesh: None });
    res.modified.push(bid.to_string());
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("lattice", f_lattice)
}
