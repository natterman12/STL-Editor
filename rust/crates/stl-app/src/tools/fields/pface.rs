//! The `pface` field: one flat face of a body (1.x `'pface'`). With `anyFace` a curved face
//! gives a point on it instead.

use egui::Ui;
use serde_json::{json, Value};

use super::bodies::{body_name, handle_box, sel_box, PickOut, ShowOut};
use super::plane::face_info;
use crate::engine_link::Level;
use crate::theme::Tokens;
use crate::tools::{Ctx, Field, ToolDef, Values};
use crate::viewport::pick::Hit;

pub fn show(ui: &mut Ui, tk: &Tokens, f: &Field, values: &mut Values, active: &mut Option<String>, _ctx: &Ctx) -> ShowOut {
    let chips: Vec<String> = values.get(f.key).and_then(|v| v.get("label")).and_then(Value::as_str).map(|s| vec![s.to_owned()]).unwrap_or_default();
    let b = sel_box(ui, tk, egui::Id::new(("pick", f.key)), active.as_deref() == Some(f.key), &chips, "Click a flat face");
    let mut out = ShowOut::default();
    handle_box(&b, f.key, active, &mut out, |_| {
        values.insert(f.key.to_owned(), Value::Array(vec![]));
        true
    });
    out.clear_face = out.changed;
    out
}

/// Whether the field takes a curved face too (`anyFace`).
pub fn any_face(_tool: &ToolDef, f: &Field) -> bool {
    matches!(f.kind, crate::tools::FieldKind::PFace { any_face: true })
}

/// A click on a body: the face under it, its outline and highlight. Not flat and `anyFace`
/// off: "That face isn't flat. Pick a flat face." (error line, nothing stored).
pub fn feed_pick(tool: &ToolDef, f: &Field, values: &mut Values, hit: &Hit, ctx: &Ctx) -> PickOut {
    let Hit::Body { id, face, point, normal } = hit else { return PickOut::used() };
    let Some(b) = ctx.bodies.iter().find(|b| &b.id == id) else { return PickOut::used() };
    let Some(fi) = face_info(&b.mesh, *face as usize, true) else { return PickOut::used() };
    if !fi.planar && !any_face(tool, f) {
        return PickOut::said(Level::Error, "That face isn't flat. Pick a flat face.");
    }
    let v = if fi.planar {
        json!({ "origin": fi.origin, "normal": fi.normal, "xdir": fi.xdir, "contours": fi.contours, "label": format!("Face of {}", body_name(ctx, id)) })
    } else {
        json!({ "origin": point, "normal": normal, "contours": [], "label": format!("Point on {}", body_name(ctx, id)) })
    };
    values.insert(f.key.to_owned(), v);
    values.insert("body".to_owned(), Value::from(id.clone()));
    let mut out = PickOut::changed();
    out.clear_face = true;
    out.highlight = Some((id.clone(), fi.tris));
    out.advance = true;
    out
}
