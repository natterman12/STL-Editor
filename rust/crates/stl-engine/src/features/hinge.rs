//! Living hinge (1.x `f_hinge`): a slot across the whole block, leaving a thin web of
//! `thickness` at the bottom.

use serde_json::{Map, Value};
use stl_kernel::Solid;

use super::common::frames::dot;
use super::common::{apply_tool, float, place::place, strings, truthy};
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn hinge(
    state: &mut State,
    fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let slot = float(p, "slot", Some(1.0))?;
    let web = float(p, "thickness", Some(0.5))?;
    if slot <= 0.0 || web <= 0.0 {
        return Err(FeatureError("The slot width and the hinge thickness must be more than zero.".into()));
    }
    let targets = p.get("targets");
    let ids: Vec<String> = if truthy(targets) {
        strings(targets).into_iter().filter(|t| state.bodies.contains_key(t)).collect()
    } else {
        state.bodies.iter().filter(|(_, b)| b.solid.is_some()).map(|(k, _)| k.clone()).collect()
    };
    if ids.is_empty() {
        return Err(FeatureError("Choose the block to put the living hinge in.".into()));
    }
    let frame = state.frame(p.get("plane"))?;
    let (mut lo, mut hi, mut reach) = (f64::INFINITY, f64::NEG_INFINITY, 0.0_f64);
    for id in &ids {
        let b = state.body(id)?;
        let mesh;
        let m = match (&b.solid, &b.mesh) {
            (Some(s), _) => {
                mesh = s.to_mesh();
                &mesh
            }
            (None, Some(m)) => m.as_ref(),
            (None, None) => continue,
        };
        for c in m.positions.chunks_exact(3) {
            let rel = [c[0] as f64 - frame.o[0], c[1] as f64 - frame.o[1], c[2] as f64 - frame.o[2]];
            let d = dot(rel, frame.n);
            lo = lo.min(d);
            hi = hi.max(d);
            reach = reach.max(dot(rel, frame.x).abs()).max(dot(rel, frame.y).abs());
        }
    }
    if web >= hi - lo {
        return Err(FeatureError("The hinge is as thick as the block, so there is nothing to cut.".into()));
    }
    let bb = reach + 10.0;
    let strip = Solid::cube([slot, 2.0 * bb, hi - lo - web + 1.0], false)
        .translate([float(p, "x", Some(0.0))?, -bb, lo + web]);
    apply_tool(state, fid, place(&strip, &frame, [0.0; 3]), "cut", targets, "Living hinge", res)
}

pub fn register(r: Registry) -> Registry {
    r.with("hinge", hinge)
}
