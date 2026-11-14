//! The `facepoints` field: points clicked on a face (1.x `'facepoints'`).

use egui::Ui;
use serde_json::Value;

use super::bodies::{handle_box, is_empty_sel, pt4, pt_from, sel_box, Mark, PickOut, ShowOut};
use super::plane::face_info;
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

pub fn show(ui: &mut Ui, tk: &Tokens, f: &Field, values: &mut Values, active: &mut Option<String>, _ctx: &Ctx) -> ShowOut {
    let n = points(values, f.key).len();
    let chips = if n > 0 { vec![label(n)] } else { vec![] };
    let b = sel_box(ui, tk, egui::Id::unique(("pick", f.key)), active.as_deref() == Some(f.key), &chips, "Click on a face");
    let mut out = ShowOut::default();
    handle_box(&b, f.key, active, &mut out, |_| {
        clear(values, f.key);
        true
    });
    out
}

/// The cross: no points and no stored face normal.
pub fn clear(values: &mut Values, key: &str) {
    values.insert(key.to_owned(), Value::Array(vec![]));
    values.insert("normal".to_owned(), Value::Null);
}

/// A click adds a point. The first one also stores the face normal (the flat face's, else the
/// clicked triangle's) and an empty `targets`.
pub fn feed_pick(f: &Field, values: &mut Values, hit: &Hit, ctx: &Ctx) -> PickOut {
    let Hit::Body { id, face, point, normal } = hit else { return PickOut::used() };
    let mut pts = points(values, f.key);
    pts.push(pt4(*point));
    values.insert(f.key.to_owned(), Value::Array(pts));
    if is_empty_sel(values.get("normal")) {
        let n = ctx.bodies.iter().find(|b| &b.id == id).and_then(|b| face_info(&b.mesh, *face as usize, false)).filter(|fi| fi.planar).map(|fi| fi.normal).unwrap_or(*normal);
        values.insert("normal".to_owned(), Value::from(vec![n[0], n[1], n[2]]));
    }
    if is_empty_sel(values.get("targets")) {
        values.insert("targets".to_owned(), Value::Array(vec![]));
    }
    PickOut::changed()
}

pub fn marks(f: &Field, values: &Values) -> Vec<Mark> {
    points(values, f.key).iter().filter_map(pt_from).map(|p| Mark::Marker { p, color: MARKER }).collect()
}
