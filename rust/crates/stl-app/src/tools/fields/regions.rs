//! The `regions` field: single closed regions of a sketch, clicked on the sketch plane (1.x
//! `'regions'`, `drawRegionOverlay`, and the regions branch of the viewport click handler).

use egui::{Pos2, Ui};
use serde_json::Value;
use stl_sketch::entities::{parse_entities, Entity};
use stl_sketch::polygon::Face;
use stl_sketch::profile::faces;

use super::bodies::{handle_box, sel_box, Mark, PickOut, ShowOut};
use crate::engine_link::Level;
use crate::theme::Tokens;
use crate::tools::{Ctx, Field, Values};
use crate::viewport::{PlaneFrame, Viewport};

fn points(values: &Values, key: &str) -> Vec<[f64; 2]> {
    values.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(|p| Some([p.get(0)?.as_f64()?, p.get(1)?.as_f64()?])).collect()).unwrap_or_default()
}

pub fn label(n: usize) -> String {
    format!("{n} region{}", if n > 1 { "s" } else { "" })
}

/// The entities of the sketch step `values.sketch`.
pub fn entities(values: &Values, ctx: &Ctx) -> Vec<Entity> {
    let id = values.get("sketch").and_then(Value::as_str).unwrap_or("");
    let list = ctx.features.iter().find(|f| f.id == id).and_then(|f| f.params.get("entities")).and_then(Value::as_array);
    list.map(|l| parse_entities(l).unwrap_or_default()).unwrap_or_default()
}

/// All closed regions of that sketch (`sketchFaces`).
pub fn sketch_faces(values: &Values, ctx: &Ctx) -> Vec<Face> {
    faces(&entities(values, ctx), None)
}

fn frame(values: &Values, ctx: &Ctx) -> Option<PlaneFrame> {
    let id = values.get("sketch").and_then(Value::as_str)?;
    let s = ctx.sketches.iter().find(|s| s.id == id)?;
    Some(PlaneFrame { origin: s.frame[0], x: s.frame[1], y: s.frame[2], normal: s.frame[3] })
}

pub fn show(ui: &mut Ui, tk: &Tokens, f: &Field, values: &mut Values, active: &mut Option<String>, _ctx: &Ctx) -> ShowOut {
    let n = points(values, f.key).len();
    let chips = if n > 0 { vec![label(n)] } else { vec![] };
    let b = sel_box(ui, tk, egui::Id::unique(("pick", f.key)), active.as_deref() == Some(f.key), &chips, "All (click to pick)");
    let mut out = ShowOut::default();
    handle_box(&b, f.key, active, &mut out, |_| {
        values.insert(f.key.to_owned(), Value::Array(vec![]));
        true
    });
    out
}

/// Where the pointer meets the sketch plane, in sketch coordinates.
pub fn sketch_uv(vp: &Viewport, pos: Pos2, values: &Values, ctx: &Ctx) -> Option<[f64; 2]> {
    let fr = frame(values, ctx)?;
    let p = vp.ray_to_plane(pos, &fr)?;
    let d = [p[0] - fr.origin[0], p[1] - fr.origin[1], p[2] - fr.origin[2]];
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    Some([dot(d, fr.x), dot(d, fr.y)])
}

/// A click at `uv` on the sketch plane: a region not yet picked is added, one already picked is
/// taken out again, a click outside every region says so.
pub fn feed_click(f: &Field, values: &mut Values, uv: [f64; 2], ctx: &Ctx) -> PickOut {
    let fs = sketch_faces(values, ctx);
    let Some(fi) = fs.iter().position(|fc| fc.contains(uv)) else {
        return PickOut::said(Level::Warning, "Click inside a closed region of the sketch.");
    };
    let mut cur = points(values, f.key);
    // clicking a picked region again un-picks it
    match cur.iter().position(|q| fs.iter().position(|fc| fc.contains(*q)) == Some(fi)) {
        Some(same) => {
            cur.remove(same);
        }
        None => cur.push(uv),
    }
    values.insert(f.key.to_owned(), Value::Array(cur.iter().map(|p| Value::from(vec![p[0], p[1]])).collect()));
    PickOut::changed()
}

/// The sketch's lines (orange, 0.9) and the picked regions (`drawRegionOverlay`).
pub fn marks(f: &Field, values: &Values, ctx: &Ctx) -> Vec<Mark> {
    let id = values.get("sketch").and_then(Value::as_str).unwrap_or("");
    let (Some(sk), Some(fr)) = (ctx.sketches.iter().find(|s| s.id == id), frame(values, ctx)) else { return Vec::new() };
    let mut out = vec![Mark::SketchLines { frame: fr.clone(), outlines: sk.outlines.clone() }];
    let pts = points(values, f.key);
    if !pts.is_empty() {
        let picked: Vec<Vec<Vec<[f64; 2]>>> = sketch_faces(values, ctx)
            .into_iter()
            .filter(|fc| pts.iter().any(|q| fc.contains(*q)))
            .map(|fc| std::iter::once(fc.exterior.clone()).chain(fc.holes.iter().cloned()).collect())
            .collect();
        if !picked.is_empty() {
            out.push(Mark::Regions { frame: fr, faces: picked });
        }
    }
    out
}
