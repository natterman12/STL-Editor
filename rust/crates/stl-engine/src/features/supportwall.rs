//! Draw Walls: hand-drawn Support Fins walls (1.x `f_supportwall`).

use serde_json::{json, Map, Value};
use stl_kernel::Solid;

use crate::features::common::fins::{add_supports, body_mesh, usable_walls, walls as fin_walls, Extras};
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

/// Runs the feature and returns what 1.x puts in the result besides `FeatureResult`'s fields.
pub fn run(
    state: &mut State,
    fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    ctx: &Ctx<'_>,
) -> Result<Extras, FeatureError> {
    let bid = match p.get("body") {
        Some(Value::String(s)) => s.clone(),
        // 1.x: f"Body {bid!r} no longer exists." with bid None
        None | Some(Value::Null) => return Err(FeatureError("Body None no longer exists.".into())),
        Some(other) => return Err(FeatureError(format!("Body {other} no longer exists."))),
    };
    let b = state.body(&bid)?.clone();
    let walls = usable_walls(p);
    if walls.is_empty() {
        return Err(FeatureError("Click on the part to draw a wall under an overhang.".into()));
    }
    let options = p.get("options").cloned().unwrap_or_else(|| json!({}));
    let mesh = body_mesh(&b);
    let r = fin_walls::walls(ctx.fins, &mesh, &walls, &options)?;
    let mut ex = Extras::default();
    ex.walls = Some(Value::Array(r.walls.iter().map(|w| json!({"ok": w.ok, "reason": w.reason})).collect()));
    let good: Vec<_> = r.walls.iter().filter(|w| w.ok).collect();
    let bad: Vec<(usize, _)> = r.walls.iter().enumerate().filter(|(_, w)| !w.ok || w.failed > 0).collect();
    if good.is_empty() {
        let why = r.walls[0].reason.clone().unwrap_or_else(|| "None".into());
        return Err(FeatureError(if walls.len() == 1 {
            format!("Support Fins couldn’t place that wall: {why}.")
        } else {
            format!("Support Fins couldn’t place any of the {} walls ({why}).", walls.len())
        }));
    }
    let vol: f64 = r.solids.iter().map(Solid::volume).sum();
    ex.support = Some(json!({"volume": (vol * 10.0).round_ties_even() / 10.0, "pieces": r.solids.len()}));
    let length: f64 = good.iter().map(|w| w.length).sum();
    let tines: i64 = good.iter().map(|w| w.tines).sum();
    let tall = good.iter().map(|w| w.height).fold(f64::NEG_INFINITY, f64::max);
    res.info = Some(if good.len() == 1 {
        format!(
            "Support Fins {}: wall {:.1} mm long, {:.1} mm tall, {} tines{}",
            r.version,
            length,
            tall,
            tines,
            if good[0].part_attached { ", standing on the part" } else { "" }
        )
    } else {
        format!("Support Fins {}: {} walls, {:.1} mm in all, {} tines", r.version, good.len(), length, tines)
    });
    if !bad.is_empty() {
        let text: Vec<String> = bad
            .iter()
            .map(|(i, w)| {
                format!(
                    "Wall {}{}: {}",
                    i + 1,
                    if w.ok { " (part of it)" } else { "" },
                    w.reason.clone().unwrap_or_else(|| "None".into())
                )
            })
            .collect();
        res.warning = Some(text.join(". ") + ".");
    }
    add_supports(state, fid, res, &mut ex, &bid, &r.solids, p.get("output").and_then(Value::as_str), "walls", ctx.preview)?;
    Ok(ex)
}

fn f_supportwall(
    state: &mut State,
    fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let ex = run(state, fid, p, res, ctx)?;
    crate::features::common::fins::apply_extras(res, ex);
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("supportwall", f_supportwall)
}
