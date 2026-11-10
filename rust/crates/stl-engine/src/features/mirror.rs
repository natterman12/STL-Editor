use serde_json::{Map, Value};

use super::common::{mirror_matrix, repeat_features, transform_body};
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn string_list(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
        _ => Vec::new(),
    }
}

fn f_mirror(
    state: &mut State,
    fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let m = mirror_matrix(&state.frame(p.get("plane"))?);
    if p.get("kind").and_then(Value::as_str) == Some("features") {
        let ids = string_list(p.get("features"));
        return repeat_features(state, fid, &ids, &[m], res, "mirror");
    }
    let bids = string_list(p.get("bodies"));
    if bids.is_empty() {
        return Err(FeatureError("Choose a body to mirror.".into()));
    }
    let mode = p.get("mode").and_then(Value::as_str).unwrap_or("join");
    for (i, bid) in bids.iter().enumerate() {
        let b = state.body(bid)?.clone();
        let mb = transform_body(&b, m);
        match mode {
            "replace" => {
                state.bodies.insert(bid.clone(), mb);
                res.modified.push(bid.clone());
            }
            "join" => {
                let s = b.require_solid()?.clone();
                let mirrored = mb.require_solid()?.clone();
                let joined = s.union(&mirrored);
                state.bodies.insert(bid.clone(), crate::state::Body { solid: Some(joined), ..b });
                res.modified.push(bid.clone());
            }
            _ => {
                let nid = if bids.len() == 1 { fid.to_string() } else { format!("{fid}.{}", i + 1) };
                let mut mb = mb;
                mb.name = format!("{} (mirror)", b.name);
                state.bodies.insert(nid.clone(), mb);
                res.created.push(nid);
            }
        }
    }
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("mirror", f_mirror)
}
