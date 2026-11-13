//! The `axisline` field: the sketch's own X or Y axis, or one of its open lines (1.x
//! `fieldHTML` `'axisline'`). The lines come from the sketch step's entities, because the
//! sketch view carries outlines without ids.

use egui::Ui;
use serde_json::Value;

use super::bodies::ShowOut;
use crate::theme::Tokens;
use crate::tools::{Ctx, Field, Values};

/// `[value, label]` rows: the two axes, then each unclosed polyline and line of the sketch
/// `values.sketch` ("Line 1", "Construction line 2", ...).
pub fn options(values: &Values, ctx: &Ctx) -> Vec<(String, String)> {
    let mut out = vec![("x".to_owned(), "Sketch X axis".to_owned()), ("y".to_owned(), "Sketch Y axis".to_owned())];
    let sk = values.get("sketch").and_then(Value::as_str).unwrap_or("");
    let ents = ctx.features.iter().find(|f| f.id == sk).and_then(|f| f.params.get("entities")).and_then(Value::as_array);
    let lines = ents.into_iter().flatten().filter(|e| {
        let t = e.get("type").and_then(Value::as_str);
        (t == Some("polyline") && !e.get("closed").and_then(Value::as_bool).unwrap_or(false)) || t == Some("line")
    });
    for (i, e) in lines.enumerate() {
        let id = e.get("id").and_then(Value::as_str).unwrap_or("");
        let constr = e.get("construction").and_then(Value::as_bool).unwrap_or(false);
        out.push((id.to_owned(), format!("{} {}", if constr { "Construction line" } else { "Line" }, i + 1)));
    }
    out
}

/// The label of the current value (an unknown value shows the first row, like a `<select>`).
pub fn selected_label(values: &Values, key: &str, ctx: &Ctx) -> String {
    let cur = values.get(key).and_then(Value::as_str).unwrap_or("");
    let rows = options(values, ctx);
    rows.iter().find(|r| r.0 == cur).or(rows.first()).map(|r| r.1.clone()).unwrap_or_default()
}

pub fn show(ui: &mut Ui, tk: &Tokens, f: &Field, values: &mut Values, _active: &mut Option<String>, ctx: &Ctx) -> ShowOut {
    let cur = values.get(f.key).and_then(Value::as_str).unwrap_or("").to_owned();
    let rows = options(values, ctx);
    let mut pick: Option<String> = None;
    let shown = selected_label(values, f.key, ctx);
    super::framed_combo(ui, tk, ("sel", f.key), ui.available_width(), &shown, |ui| {
        for (k, l) in &rows {
            if ui.selectable_label(&cur == k, l).clicked() {
                pick = Some(k.clone());
            }
        }
    });
    let mut out = ShowOut::default();
    if let Some(k) = pick.filter(|k| *k != cur) {
        values.insert(f.key.to_owned(), Value::from(k));
        out.changed = true;
    }
    out
}
