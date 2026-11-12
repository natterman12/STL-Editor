//! Snap-fit (1.x `f_snapfit`): a cantilever clip, an arm standing on the plate with a hook at
//! its tip pointing inwards.

use serde_json::{Map, Value};
use stl_kernel::Solid;

use super::common::{apply_tool, float, place::place};
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn snapfit(
    state: &mut State,
    fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let l = float(p, "length", Some(12.0))?;
    let w = float(p, "width", Some(6.0))?;
    let t = float(p, "thickness", Some(1.5))?;
    let h = float(p, "hook_height", Some(1.5))?;
    if l.min(w).min(t).min(h) <= 0.0 {
        return Err(FeatureError("Arm length, width, thickness and hook height must all be more than zero.".into()));
    }
    if h >= l {
        return Err(FeatureError("The hook must be shorter than the arm.".into()));
    }
    let sink = 0.5;
    let arm = Solid::cube([t, w, l + sink], false).translate([0.0, 0.0, -sink]);
    let hook = Solid::cube([t, w, h], false).translate([t, 0.0, l - h]);
    let tool = Solid::union_all(&[arm, hook]);
    let frame = state.frame(p.get("plane"))?;
    let tool = place(&tool, &frame, [float(p, "x", Some(0.0))?, float(p, "y", Some(0.0))?, 0.0]);
    let op = p.get("operation").and_then(Value::as_str).filter(|s| !s.is_empty()).unwrap_or("join");
    let name = p.get("body_name").and_then(Value::as_str).filter(|s| !s.is_empty()).unwrap_or("Snap-fit");
    apply_tool(state, fid, tool, op, p.get("targets"), name, res)
}

pub fn register(r: Registry) -> Registry {
    r.with("snapfit", snapfit)
}
