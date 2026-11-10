//! 1.x `f_split`: cut a body by a plane, optionally with alignment connectors.

use serde_json::{Map, Value};
use stl_kernel::Solid;

use super::common::connectors::split_with_connectors;
use crate::state::Body;
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn split(
    state: &mut State,
    fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let bid = p.get("body").and_then(Value::as_str).unwrap_or("").to_string();
    if bid.is_empty() {
        // 1.x: "Body None no longer exists."; see EXCEPTIONS.md.
        return Err(FeatureError("Choose a body first.".into()));
    }
    let b = state.body(&bid)?.clone();
    let s = b.require_solid()?.clone();
    let f = state.frame(p.get("plane"))?;
    // 1.x: p.get("keep", "both") -- only a missing key means "both"; null or a non-string is
    // not "both" (so connectors are off) and keeps neither half specially.
    let keep_v = p.get("keep");
    let is_both = keep_v.map_or(true, |v| v.as_str() == Some("both"));
    let keep = match keep_v {
        None => "both".to_string(),
        Some(v) => v.as_str().unwrap_or("").to_string(),
    };
    let conn = if is_both {
        match p.get("connector") {
            None => "none",
            Some(Value::String(s)) => s.as_str(),
            Some(Value::Null) => "",
            Some(_) => "(invalid)", // 1.x fails only after the early return for an empty half
        }
    } else {
        "none"
    };
    let r = split_with_connectors(&s, f.o, f.n, conn, p.get("pin_d"), p.get("pin_len"), p.get("clearance"))?;
    if conn != "none" && r.skipped {
        res.warning = Some("The cut face is too thin for connectors here; split without them.".into());
    } else if conn != "none" {
        res.warning = None;
    }
    let (mut above, mut below) = (r.above, r.below);
    for (i, ex) in r.extras.into_iter().enumerate() {
        let id = format!("{fid}.pin{}", i + 1);
        state.bodies.insert(id.clone(), Body { name: format!("Pin {}", i + 1), solid: Some(ex), mesh: None });
        res.created.push(id);
    }
    match keep.as_str() {
        "above" => below = Solid::compose(&[]),
        "below" => above = Solid::compose(&[]),
        _ => {}
    }
    if above.is_empty() && below.is_empty() {
        return Err(FeatureError("Nothing left after the split.".into()));
    }
    if (above.is_empty() || below.is_empty()) && keep == "both" {
        return Err(FeatureError("The plane doesn't cut through this body.".into()));
    }
    let (first, second) = if !above.is_empty() { (above, below) } else { (below, Solid::compose(&[])) };
    let name = if second.is_empty() { b.name.clone() } else { format!("{} (1)", b.name) };
    state.bodies.insert(bid.clone(), Body { name, solid: Some(first), mesh: None });
    res.modified.push(bid);
    if !second.is_empty() {
        let nid = format!("{fid}.2");
        state.bodies.insert(nid.clone(), Body { name: format!("{} (2)", b.name), solid: Some(second), mesh: None });
        res.created.push(nid);
    }
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("split", split)
}
