//! The `facepoints` field: points clicked on a face (1.x `'facepoints'`).

use egui::Ui;
use serde_json::Value;

use super::bodies::{Mark, PickOut, ShowOut, handle_box, is_empty_sel, pt_from, pt4, sel_box};
use super::place_refs::{self, FROM_PICK, POINT_REFS, SNAPPED};
use super::plane::face_info;
use crate::engine_link::Level;
use crate::theme::Tokens;
use crate::tools::{Ctx, Field, Values};
use crate::viewport::pick::Hit;

/// 1.x draws these markers in this blue.
pub const MARKER: u32 = 0x1f6fd1;

fn points(values: &Values, key: &str) -> Vec<Value> {
    values.get(key).and_then(Value::as_array).cloned().unwrap_or_default()
}

pub fn label(n: usize) -> String {
    format!("{n} point{}", if n > 1 { "s" } else { "" })
}

pub fn show(
    ui: &mut Ui,
    tk: &Tokens,
    f: &Field,
    values: &mut Values,
    active: &mut Option<String>,
    ctx: &Ctx,
) -> ShowOut {
    // a rebuild may have moved an edge a point is measured from: show (and later store) the point
    // the references give now, as the engine cuts it
    let mut out = ShowOut::default();
    if place_refs::refresh_points(values, f.key, ctx) {
        out.changed = true;
    }
    let n = points(values, f.key).len();
    let chips = if n > 0 { vec![label(n)] } else { vec![] };
    ui.vertical(|ui| {
        let b = sel_box(
            ui,
            tk,
            egui::Id::unique(("pick", f.key)),
            active.as_deref() == Some(f.key),
            &chips,
            "Click on a face",
        );
        handle_box(&b, f.key, active, &mut out, |_| {
            clear(values, f.key);
            true
        });
        // M7: the "From" rows (distances from part edges and sketch lines) for the last point
        if n > 0 && place_refs::show_from(ui, tk, values, f.key, active, ctx.inch) {
            out.changed = true;
        }
    });
    out
}

/// The cross: no points, no stored face normal and nothing they were measured from.
pub fn clear(values: &mut Values, key: &str) {
    values.insert(key.to_owned(), Value::Array(vec![]));
    values.insert("normal".to_owned(), Value::Null);
    values.remove(POINT_REFS);
    values.remove(FROM_PICK);
}

/// A click adds a point. The first one also stores the face normal (the flat face's, else the
/// clicked triangle's) and an empty `targets`.
///
/// M7: the click snaps to the visible sketch geometry near it (a point on a sketch line keeps
/// the line as its reference); while the "From" row is picking, the click picks an edge or
/// sketch line to measure the last point from instead.
pub fn feed_pick(f: &Field, values: &mut Values, hit: &Hit, ctx: &Ctx) -> PickOut {
    let Hit::Body {
        id,
        face,
        point,
        normal,
    } = hit
    else {
        return PickOut::used();
    };
    let tol = place_refs::tolerance(ctx, id);
    let face_n = || {
        ctx.bodies
            .iter()
            .find(|b| &b.id == id)
            .and_then(|b| face_info(&b.mesh, *face as usize, false))
            .filter(|fi| fi.planar)
            .map(|fi| fi.normal)
            .unwrap_or(*normal)
    };
    let mut pts = points(values, f.key);
    if values.get(FROM_PICK).and_then(Value::as_bool).unwrap_or(false) {
        values.remove(FROM_PICK);
        let (Some(at), Some(n)) = (pts.last().and_then(pt_from), values.get("normal").and_then(pt_from)) else {
            return PickOut::changed();
        };
        return match place_refs::pick_ref(ctx, id, *point, n, at, tol) {
            Some(r) => {
                place_refs::add_ref(values, f.key, r);
                PickOut::changed()
            }
            None => {
                let mut o = PickOut::said(
                    Level::Warning,
                    "There's no straight edge there. Click a straight edge of the part or a sketch line.",
                );
                o.changed = true;
                o
            }
        };
    }
    let n = values.get("normal").and_then(pt_from).unwrap_or_else(face_n);
    let snapped = place_refs::snap(*point, n, ctx, tol);
    let at = snapped.as_ref().map_or(*point, |s| s.p);
    pts.push(pt4(at));
    let count = pts.len();
    values.insert(f.key.to_owned(), Value::Array(pts));
    let mut lists = place_refs::ref_lists(values, POINT_REFS);
    lists.resize(count - 1, Vec::new());
    lists.push(snapped.and_then(|s| s.line_ref).into_iter().collect());
    place_refs::set_ref_lists(values, POINT_REFS, lists);
    if is_empty_sel(values.get("normal")) {
        let n = face_n();
        values.insert("normal".to_owned(), Value::from(vec![n[0], n[1], n[2]]));
    }
    if is_empty_sel(values.get("targets")) {
        values.insert("targets".to_owned(), Value::Array(vec![]));
    }
    PickOut::changed()
}

pub fn marks(f: &Field, values: &Values) -> Vec<Mark> {
    let mut out: Vec<Mark> = points(values, f.key)
        .iter()
        .enumerate()
        .filter_map(|(i, v)| {
            pt_from(v).map(|p| Mark::Marker {
                p,
                color: if place_refs::has_refs(values, i) {
                    SNAPPED
                } else {
                    MARKER
                },
            })
        })
        .collect();
    out.extend(place_refs::dim_marks(values, f.key));
    out
}
