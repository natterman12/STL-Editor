//! 1.x `f_layflat`: stand each body on its most stable flat side, or on a chosen face.

use serde_json::{Map, Value};

use super::common::pose::{best_rest_normal, lay_flat_matrix};
use super::common::{strings, truthy};
use crate::state::Body;
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn f_layflat(
    state: &mut State,
    _fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let mut bids = strings(p.get("bodies"));
    if !truthy(p.get("bodies")) {
        bids.clear();
        if let Some(b) = p.get("body").and_then(Value::as_str).filter(|b| !b.is_empty()) {
            bids.push(b.to_string());
        }
    }
    if bids.is_empty() {
        return Err(FeatureError("Choose a body to lay flat.".into()));
    }
    for bid in bids {
        let b = state.body(&bid)?.clone();
        let s = b.require_solid()?.clone();
        let normal = if truthy(p.get("auto")) || !truthy(p.get("normal")) {
            best_rest_normal(&s)?
        } else {
            let bad = || FeatureError("normal must be three numbers.".into());
            let a = p.get("normal").and_then(Value::as_array).ok_or_else(bad)?;
            if a.len() != 3 {
                return Err(bad());
            }
            [a[0].as_f64().ok_or_else(bad)?, a[1].as_f64().ok_or_else(bad)?, a[2].as_f64().ok_or_else(bad)?]
        };
        let m = lay_flat_matrix(&s, normal)?;
        state.bodies.insert(bid.clone(), Body { solid: Some(s.transform(m)), ..b });
        res.modified.push(bid);
    }
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("layflat", f_layflat)
}
