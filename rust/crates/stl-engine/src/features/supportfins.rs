//! Support Fins (github.com/gittrahan/support-fins): breakaway fins for the part as posed now.
//! The engine runs on the document's `FinsHost`; this is 1.x `f_supportfins`.

use serde_json::{json, Map, Value};
use stl_kernel::Solid;

use crate::features::common::fins::{
    add_supports, body_mesh, results, usable_walls, walls as fin_walls, Extras, GHOST_COLOR, SUPPORT_COLOR,
};
use crate::state::Body;
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn ids_of(p: &Map<String, Value>) -> Vec<String> {
    let strs = |a: &Vec<Value>| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect::<Vec<_>>();
    if let Some(Value::Array(a)) = p.get("bodies") {
        if !a.is_empty() {
            return strs(a);
        }
    }
    match p.get("body") {
        Some(Value::String(s)) if !s.is_empty() => vec![s.clone()],
        _ => Vec::new(),
    }
}

/// Python's `str(None)` for an optional reason.
fn reason_text(r: &Option<String>) -> String {
    r.clone().unwrap_or_else(|| "None".into())
}

fn union(list: &[Solid]) -> Solid {
    if list.len() == 1 { list[0].clone() } else { Solid::union_all(list) }
}

/// Runs the feature and returns what 1.x puts in the result besides `FeatureResult`'s fields.
pub fn run(
    state: &mut State,
    fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    ctx: &Ctx<'_>,
) -> Result<Extras, FeatureError> {
    let ids = ids_of(p);
    if ids.is_empty() {
        return Err(FeatureError("Choose the body to support.".into()));
    }
    let options = p.get("options").cloned().unwrap_or_else(|| json!({}));
    let output = p.get("output").and_then(Value::as_str);
    let mut notes: Vec<String> = Vec::new();
    let mut ex = Extras::default();
    let preview = ctx.preview;
    for bid in &ids {
        let b = state.body(bid)?.clone();
        let mesh = body_mesh(&b);
        let r = results::fins(ctx.fins, &mesh, &options)?;
        ex.paint = Some(json!({"body": bid, "over": r.over, "small": r.small}));
        // fins you clicked away, and walls you drew by hand (Support Fins' "+ Add walls by hand")
        let (gone, missing) = results::match_removed(&r.pieces, p.get("removed"));
        let hand = usable_walls(p);
        let wr = if hand.is_empty() { None } else { Some(fin_walls::walls(ctx.fins, &mesh, &hand, &options)?) };
        if let Some(wr) = &wr {
            ex.walls = Some(Value::Array(
                wr.walls.iter().map(|w| json!({"ok": w.ok, "reason": w.reason})).collect(),
            ));
        }
        let drawn = wr.as_ref().map(|wr| wr.walls.iter().filter(|w| w.ok).count());
        let extra = [
            (!gone.is_empty()).then(|| format!("{} removed", gone.len())),
            drawn.map(|n| format!("{n} drawn by hand")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ");
        res.info = Some(
            format!("Support Fins {}: {}", r.version, r.report) + &if extra.is_empty() { String::new() } else { format!(" ({extra})") },
        );
        if missing > 0 {
            notes.push(format!("{missing} fin(s) you removed aren't there any more in this build, so nothing was removed for them."));
        }
        if let Some(wr) = &wr {
            for (i, w) in wr.walls.iter().enumerate() {
                if !w.ok || w.failed > 0 {
                    notes.push(format!(
                        "Hand wall {}{}: {}.",
                        i + 1,
                        if w.ok { " (part of it)" } else { "" },
                        reason_text(&w.reason)
                    ));
                }
            }
        }
        let kept: Vec<Solid> = r.solids.iter().enumerate().filter(|(i, _)| !gone.contains(i)).map(|(_, s)| s.clone()).collect();
        let mut every = kept;
        if let Some(wr) = &wr {
            every.extend(wr.solids.iter().cloned());
        }
        // the plastic the supports cost (the site's grams receipt; the panel turns it into grams)
        let vol: f64 = every.iter().map(Solid::volume).sum();
        ex.support = Some(json!({"volume": (vol * 10.0).round_ties_even() / 10.0, "pieces": every.len()}));
        if preview {
            // every fin a body of its own beside the untouched part, so one can be clicked to
            // remove it; removed ones stay as see-through ghosts to click back
            res.preview_only = true;
            let mut pieces = Map::new();
            let mut put = |state: &mut State, res: &mut FeatureResult, ex: &mut Extras, nid: String, solid: Solid, info: Value| {
                state.bodies.insert(nid.clone(), Body { name: format!("{} supports", b.name), solid: Some(solid), mesh: None });
                res.created.push(nid.clone());
                let removed = info.get("removed").and_then(Value::as_bool).unwrap_or(false);
                pieces.insert(nid.clone(), info);
                ex.colors.insert(nid.clone(), Value::String(if removed { GHOST_COLOR } else { SUPPORT_COLOR }.into()));
                if removed {
                    ex.ghosts.push(nid);
                }
            };
            for (i, sd) in r.solids.iter().enumerate() {
                let pc = &r.pieces[i.min(r.pieces.len().saturating_sub(1))];
                let info = json!({"kind": pc.kind, "c": pc.c, "size": pc.size, "removed": gone.contains(&i)});
                put(state, res, &mut ex, format!("{fid}~p{i}"), sd.clone(), info);
            }
            if let Some(wr) = &wr {
                for (i, lst) in wr.by_wall.iter().enumerate() {
                    if !lst.is_empty() {
                        put(state, res, &mut ex, format!("{fid}~w{i}"), union(lst), json!({"kind": "hand", "wall": i}));
                    }
                }
            }
            ex.pieces = Some(pieces);
            let overhangs = crate::features::common::fins::truthy(r.stats.get("overhangRegions"));
            if every.is_empty() && !overhangs && hand.is_empty() {
                notes.push(format!(
                    "\"{}\" has no overhangs in this position. Turn it the way you want to print it (drag the rings), then add fins.",
                    b.name
                ));
            }
            continue;
        }
        if every.is_empty() {
            let overhangs = crate::features::common::fins::truthy(r.stats.get("overhangRegions"));
            notes.push(if overhangs || !hand.is_empty() {
                format!(
                    "\"{}\": {}. Nothing could be placed; try Full coverage, draw walls by hand, or tip the part differently.",
                    b.name, r.report
                )
            } else {
                format!(
                    "\"{}\" has no overhangs in this position. Rotate it to the way you want to print it, then add fins.",
                    b.name
                )
            });
            continue;
        }
        if r.skipped > 0 {
            notes.push(format!("{} support piece(s) weren't closed solids and were left out.", r.skipped));
        }
        add_supports(state, fid, res, &mut ex, bid, &every, output, "supports", preview)?;
    }
    if !notes.is_empty() {
        res.warning = Some(notes.join(" "));
    }
    Ok(ex)
}

fn f_supportfins(
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
    r.with("supportfins", f_supportfins)
}
