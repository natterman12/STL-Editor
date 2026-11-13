//! The `points3` field: three points clicked anywhere (1.x `'points3'`), for the 3-point plane.

use egui::Ui;
use serde_json::Value;

use super::bodies::{handle_box, is_empty_sel, pt4, pt_from, sel_box, Mark, PickOut, ShowOut};
use crate::theme::Tokens;
use crate::tools::{Ctx, Field, FieldKind, ToolDef, Values};
use crate::viewport::pick::Hit;

/// 1.x draws these markers in this orange.
pub const MARKER: u32 = 0xd9480f;
/// How many points (`f.count || 3`; every 1.x tool uses 3).
pub const COUNT: usize = 3;

fn points(values: &Values, key: &str) -> Vec<Value> {
    values.get(key).and_then(Value::as_array).cloned().unwrap_or_default()
}

pub fn show(ui: &mut Ui, tk: &Tokens, f: &Field, values: &mut Values, active: &mut Option<String>, _ctx: &Ctx) -> ShowOut {
    let n = points(values, f.key).len();
    let chips = if n > 0 { vec![format!("{n} of {COUNT}")] } else { vec![] };
    let b = sel_box(ui, tk, egui::Id::new(("pick", f.key)), active.as_deref() == Some(f.key), &chips, &format!("Click {COUNT} points"));
    let mut out = ShowOut::default();
    handle_box(&b, f.key, active, &mut out, |_| {
        values.insert(f.key.to_owned(), Value::Array(vec![]));
        true
    });
    out
}

/// A click adds a point; a fourth click starts over. A tool with a `body` bodies field takes
/// the body the point is on, when that is still empty.
pub fn feed_pick(tool: &ToolDef, f: &Field, values: &mut Values, hit: &Hit, _ctx: &Ctx) -> PickOut {
    let (point, body) = match hit {
        Hit::Body { point, id, .. } => (*point, Some(id)),
        Hit::Plane { point, .. } => (*point, None),
    };
    let mut pts = points(values, f.key);
    if pts.len() >= COUNT {
        pts.clear();
    }
    pts.push(pt4(point));
    values.insert(f.key.to_owned(), Value::Array(pts));
    if let Some(id) = body {
        if tool.fields.iter().any(|x| x.key == "body" && matches!(x.kind, FieldKind::Bodies { .. })) && is_empty_sel(values.get("body")) {
            values.insert("body".to_owned(), Value::Array(vec![Value::from(id.clone())]));
        }
    }
    PickOut::changed()
}

pub fn marks(f: &Field, values: &Values) -> Vec<Mark> {
    points(values, f.key).iter().filter_map(pt_from).map(|p| Mark::Marker { p, color: MARKER }).collect()
}
