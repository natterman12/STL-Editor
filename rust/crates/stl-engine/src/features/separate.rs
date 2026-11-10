use serde_json::{Map, Value};

use crate::state::Body;
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn f_separate(
    state: &mut State,
    fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    // 1.x says "Body None no longer exists." here; 2.0 says what is wrong (see EXCEPTIONS.md).
    let bid = match p.get("body") {
        Some(Value::String(s)) => s.clone(),
        _ => return Err(FeatureError("Choose a body first.".into())),
    };
    let b = state.body(&bid)?.clone();
    let pieces = b.require_solid()?.decompose();
    if pieces.len() < 2 {
        return Err(FeatureError("This body is already one piece.".into()));
    }
    // Solid::decompose already sorts largest volume first (1.x sorts the same way).
    state.bodies.insert(
        bid.to_string(),
        Body { name: b.name.clone(), solid: Some(pieces[0].clone()), mesh: None },
    );
    res.modified.push(bid.to_string());
    for (i, pc) in pieces.into_iter().enumerate().skip(1) {
        let n = i + 1; // pieces[1] -> 2, as Python enumerate(..., 2)
        let nid = format!("{fid}.{n}");
        state.bodies.insert(nid.clone(), Body { name: format!("{} ({n})", b.name), solid: Some(pc), mesh: None });
        res.created.push(nid);
    }
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("separate", f_separate)
}
