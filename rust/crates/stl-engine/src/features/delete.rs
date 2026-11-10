use serde_json::{Map, Value};

use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn string_list(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
        _ => Vec::new(),
    }
}

fn f_delete(
    state: &mut State,
    _fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    for bid in string_list(p.get("bodies")) {
        if state.bodies.shift_remove(&bid).is_some() {
            res.removed.push(bid);
        }
    }
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("delete", f_delete)
}
