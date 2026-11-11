//! Combine (1.x `f_combine`): join, cut or intersect solid bodies into a target body.

use serde_json::{Map, Value};
use stl_kernel::Solid;

use super::common::float;
use crate::state::Body;
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

fn f_combine(
    state: &mut State,
    _fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let tgt = p.get("target").and_then(Value::as_str).unwrap_or("").to_string();
    let tools: Vec<String> = match p.get("tools") {
        Some(Value::Array(a)) => {
            a.iter().filter_map(|x| x.as_str()).filter(|t| !t.is_empty() && *t != tgt).map(String::from).collect()
        }
        _ => Vec::new(),
    };
    if tgt.is_empty() || tools.is_empty() {
        return Err(FeatureError("Choose a target body and at least one tool body.".into()));
    }
    let b = state.body(&tgt)?.clone();
    let s = b.require_solid()?.clone();
    let mut ts: Vec<Solid> = Vec::with_capacity(tools.len());
    for t in &tools {
        ts.push(state.body(t)?.require_solid()?.clone());
    }
    let op = p.get("operation").and_then(Value::as_str).unwrap_or("join");
    let out = match op {
        "join" => {
            let mut all = vec![s];
            all.extend(ts);
            Solid::union_all(&all)
        }
        "cut" => {
            let clearance = float(p, "clearance", Some(0.0))?;
            if clearance > 0.0 {
                // 1.x grows each tool first (meshtools.offset_solid)
                let mut grown = Vec::with_capacity(ts.len());
                for t in &ts {
                    grown.push(
                        stl_mesh::offset::offset_solid(t, clearance, 12000)
                            .map_err(|e| FeatureError(e.to_string()))?,
                    );
                }
                ts = grown;
            }
            if ts.len() > 1 {
                s.difference(&Solid::union_all(&ts))
            } else {
                s.difference(&ts[0])
            }
        }
        "intersect" => {
            let mut out = s;
            for t in &ts {
                out = out.intersection(t);
            }
            out
        }
        other => return Err(FeatureError(format!("Unknown operation {other}."))),
    };
    if out.is_empty() {
        return Err(FeatureError("The result is empty.".into()));
    }
    state.bodies.insert(tgt.clone(), Body { name: b.name, solid: Some(out), mesh: None });
    res.modified.push(tgt);
    if !truthy(p.get("keep_tools")) {
        for t in &tools {
            state.bodies.shift_remove(t);
        }
    }
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("combine", f_combine)
}
