//! The `cylface` field: the wall of a round hole or pin (1.x `'cylface'`), measured with
//! `fit_cylinder`.

use egui::Ui;
use serde_json::Value;
use stl_mesh::fit::{fit_cylinder, CylKind};

use super::bodies::{handle_box, pt4, sel_box, PickOut, ShowOut};
use crate::engine_link::Level;
use crate::theme::Tokens;
use crate::tools::{Ctx, Field, Values};
use crate::viewport::pick::Hit;

pub fn show(ui: &mut Ui, tk: &Tokens, f: &Field, values: &mut Values, active: &mut Option<String>, _ctx: &Ctx) -> ShowOut {
    let chips: Vec<String> = values.get(f.key).and_then(|v| v.get("label")).and_then(Value::as_str).map(|s| vec![s.to_owned()]).unwrap_or_default();
    let b = sel_box(ui, tk, egui::Id::unique(("pick", f.key)), active.as_deref() == Some(f.key), &chips, "Click the wall of a round hole or pin");
    let mut out = ShowOut::default();
    handle_box(&b, f.key, active, &mut out, |_| {
        values.insert(f.key.to_owned(), Value::Array(vec![]));
        true
    });
    out.clear_face = out.changed;
    out
}

/// `U.fmt(mm)` in millimetre mode: rounded to 3 places, no trailing zeros.
pub fn fmt_mm(x: f64) -> String {
    let r = (x * 1e3).round() / 1e3;
    format!("{r}")
}

/// `U.str(mm)`: the length in the display unit with its suffix ("5 mm", "0.1969 in").
pub fn u_str(mm: f64, inch: bool) -> String {
    format!("{} {}", super::number::fmt_len(mm, None, inch), super::number::unit_name(inch))
}

/// A click on a body: measure the round feature under it. No cylinder there: the engine's
/// message as an error line.
pub fn feed_pick(f: &Field, values: &mut Values, hit: &Hit, ctx: &Ctx) -> PickOut {
    let Hit::Body { id, face, point, .. } = hit else { return PickOut::used() };
    let Some(b) = ctx.bodies.iter().find(|b| &b.id == id) else { return PickOut::used() };
    let m = &b.mesh;
    let v: Vec<[f32; 3]> = m.positions.chunks_exact(3).map(|p| [p[0], p[1], p[2]]).collect();
    let t: Vec<[u32; 3]> = m.triangles.chunks_exact(3).map(|p| [p[0], p[1], p[2]]).collect();
    let cyl = match fit_cylinder(&v, &t, *face as usize, 35.0) {
        Ok(c) => c,
        Err(e) => return PickOut::said(Level::Error, e.to_string()),
    };
    let pin = cyl.kind == CylKind::Pin;
    let what = if pin { "Pin" } else { "Hole" };
    let diameter = ((2.0 * cyl.radius) * 1e3).round() / 1e3;
    let length = ((cyl.z1 - cyl.z0) * 1e3).round() / 1e3;
    // app.js: `${what} Ø${U.str(cyl.diameter)} × ${U.str(cyl.length)} deep`
    values.insert(f.key.to_owned(), serde_json::json!({ "label": format!("{what} Ø{} × {} deep", u_str(diameter, ctx.inch), u_str(length, ctx.inch)) }));
    values.insert("body".to_owned(), Value::from(id.clone()));
    values.insert("point".to_owned(), pt4(*point));
    values.insert("measured".to_owned(), Value::from(diameter));
    values.insert("kind".to_owned(), Value::from(if pin { "pin" } else { "hole" }));
    if !values.get("_sized").and_then(Value::as_bool).unwrap_or(false) {
        values.insert("diameter".to_owned(), Value::from(diameter));
        values.insert("size".to_owned(), Value::from("custom"));
    }
    let tris: Vec<u32> = cyl.tris.iter().take(200_000).map(|&t| t as u32).collect();
    let mut out = PickOut::changed();
    out.clear_face = true;
    out.highlight = Some((id.clone(), tris));
    out.message = Some((
        Level::Info,
        format!("Measured {} Ø{}. Type the new diameter{}.", what.to_lowercase(), u_str(diameter, ctx.inch), if pin { "" } else { " or choose Fill" }),
    ));
    out.advance = true;
    out
}
