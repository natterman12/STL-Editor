use serde_json::Value;
use stl_kernel::{Aabb, Solid};

use crate::state::Body;
use crate::{FeatureError, FeatureResult, State};

/// The tool solid a feature used (extrude, revolve, hole, shapes), so mirror and pattern can
/// repeat the feature itself rather than a whole body.
#[derive(Clone)]
pub struct ToolRecord {
    pub solid: Solid,
    pub operation: String,
    pub targets: Vec<String>,
    pub name: String,
}

const OPS: [&str; 4] = ["new", "join", "cut", "intersect"];

fn overlaps(a: &Aabb, b: &Aabb) -> bool {
    let tol = 1e-6;
    (0..3).all(|i| a.min[i] <= b.max[i] + tol && b.min[i] <= a.max[i] + tol)
}

fn string_list(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
        _ => Vec::new(),
    }
}

/// 1.x `apply_tool`: combine a tool solid with the design: new body / join / cut / intersect.
/// A body that a cut or intersect empties is removed and listed in `removed`.
pub fn apply_tool(
    state: &mut State,
    fid: &str,
    tool: Solid,
    operation: &str,
    targets: Option<&Value>,
    name: &str,
    res: &mut FeatureResult,
) -> Result<(), FeatureError> {
    if tool.is_empty() {
        return Err(FeatureError("The feature produced no geometry.".into()));
    }
    let op = if operation.is_empty() { "new" } else { operation };
    if !OPS.contains(&op) {
        return Err(FeatureError(format!("Unknown operation {op}.")));
    }
    let targets = string_list(targets);
    state.tools.insert(
        fid.to_string(),
        ToolRecord { solid: tool.clone(), operation: op.to_string(), targets: targets.clone(), name: name.to_string() },
    );
    if op == "new" {
        state.bodies.insert(fid.to_string(), Body { name: name.to_string(), solid: Some(tool), mesh: None });
        res.created.push(fid.to_string());
        return Ok(());
    }
    let tb = tool.bounding_box();
    let tids: Vec<String> = if !targets.is_empty() {
        let t: Vec<String> = targets.iter().filter(|t| state.bodies.contains_key(*t)).cloned().collect();
        if t.is_empty() {
            return Err(FeatureError("The target body no longer exists.".into()));
        }
        t
    } else {
        state
            .bodies
            .iter()
            .filter(|(_, b)| b.solid.as_ref().is_some_and(|s| overlaps(&s.bounding_box(), &tb)))
            .map(|(k, _)| k.clone())
            .collect()
    };
    if tids.is_empty() {
        if op == "join" {
            state.bodies.insert(fid.to_string(), Body { name: name.to_string(), solid: Some(tool), mesh: None });
            res.created.push(fid.to_string());
            res.warning = Some("Nothing to join to: made a new body.".into());
            return Ok(());
        }
        return Err(FeatureError(format!("Nothing to {op}: the tool doesn't touch any body.")));
    }
    if op == "join" {
        let first = tids[0].clone();
        let mut solids = vec![state.body(&first)?.require_solid()?.clone(), tool];
        for t in &tids[1..] {
            solids.push(state.body(t)?.require_solid()?.clone());
        }
        let joined = Solid::union_all(&solids);
        let b = state.body(&first)?.clone();
        state.bodies.insert(first.clone(), Body { solid: Some(joined), ..b });
        for t in &tids[1..] {
            state.bodies.shift_remove(t);
        }
        res.modified.push(first);
        return Ok(());
    }
    let mut changed = 0;
    for t in &tids {
        let b = state.body(t)?.clone();
        let s = b.require_solid()?;
        if !overlaps(&s.bounding_box(), &tb) {
            if op == "intersect" {
                state.bodies.shift_remove(t);
            }
            continue;
        }
        let out = if op == "cut" { s.difference(&tool) } else { s.intersection(&tool) };
        if out.is_empty() {
            state.bodies.shift_remove(t);
            res.removed.push(t.clone());
        } else {
            state.bodies.insert(t.clone(), Body { solid: Some(out), ..b });
            res.modified.push(t.clone());
        }
        changed += 1;
    }
    if changed == 0 {
        return Err(FeatureError(format!("Nothing to {op}: the tool doesn't touch the target body.")));
    }
    Ok(())
}

/// 1.x `_repeat_features`: apply copies of earlier features' tools, each moved by one of
/// `transforms`.
pub fn repeat_features(
    state: &mut State,
    fid: &str,
    ids: &[String],
    transforms: &[[[f64; 4]; 3]],
    res: &mut FeatureResult,
    verb: &str,
) -> Result<(), FeatureError> {
    if ids.is_empty() {
        return Err(FeatureError(format!("Choose the feature(s) to {verb}.")));
    }
    let mut done = 0;
    let mut failed: Vec<String> = Vec::new();
    let mut k = 0;
    for src in ids {
        let Some(rec) = state.tools.get(src).cloned() else {
            return Err(FeatureError(format!(
                "A chosen feature can't be {verb}ed (it isn't an extrude, revolve, hole or shape that comes earlier in the timeline, or it failed)."
            )));
        };
        for m in transforms {
            k += 1;
            let id = format!("{fid}.{k}");
            let mut sub = FeatureResult::default();
            let live: Vec<Value> = rec
                .targets
                .iter()
                .filter(|t| state.bodies.contains_key(*t))
                .map(|t| Value::String(t.clone()))
                .collect();
            let targets = if live.is_empty() { None } else { Some(Value::Array(live)) };
            match apply_tool(state, &id, rec.solid.transform(*m), &rec.operation, targets.as_ref(), &rec.name, &mut sub) {
                Ok(()) => {
                    res.created.extend(sub.created);
                    res.removed.extend(sub.removed);
                    for x in sub.modified {
                        if !res.modified.contains(&x) {
                            res.modified.push(x);
                        }
                    }
                    done += 1;
                }
                Err(e) => failed.push(e.0),
            }
            // a copy's tool isn't a feature to repeat again
            state.tools.shift_remove(&id);
        }
    }
    if done == 0 {
        return Err(FeatureError(failed.into_iter().next().unwrap_or_else(|| "Nothing changed.".into())));
    }
    if !failed.is_empty() {
        let n = failed.len();
        res.warning = Some(format!("{n} cop{} didn't touch the part and were skipped.", if n == 1 { "y" } else { "ies" }));
    }
    Ok(())
}
