//! The `sketch` pick field: a list of the document's sketches (1.x `fieldHTML` `'sketch'`).

use egui::Ui;
use serde_json::Value;

use super::bodies::ShowOut;
use crate::theme::Tokens;
use crate::tools::{Ctx, Field, Values};

pub const NONE_LABEL: &str = "— pick a sketch —";

/// The chosen sketch's name, or the "pick a sketch" line.
pub fn selected_label(values: &Values, key: &str, ctx: &Ctx) -> String {
    let id = values.get(key).and_then(Value::as_str).unwrap_or("");
    ctx.sketches.iter().find(|s| s.id == id).map(|s| s.name.clone()).unwrap_or_else(|| NONE_LABEL.to_owned())
}

/// Choosing a sketch (an empty id is "none").
pub fn choose(values: &mut Values, key: &str, id: &str) {
    values.insert(key.to_owned(), Value::from(id));
}

pub fn show(ui: &mut Ui, tk: &Tokens, f: &Field, values: &mut Values, _active: &mut Option<String>, ctx: &Ctx) -> ShowOut {
    let cur = values.get(f.key).and_then(Value::as_str).unwrap_or("").to_owned();
    let mut pick: Option<String> = None;
    let shown = selected_label(values, f.key, ctx);
    super::framed_combo(ui, tk, ("sel", f.key), ui.available_width(), &shown, |ui| {
        if ui.selectable_label(cur.is_empty(), NONE_LABEL).clicked() {
            pick = Some(String::new());
        }
        for s in ctx.sketches {
            if ui.selectable_label(cur == s.id, &s.name).clicked() {
                pick = Some(s.id.clone());
            }
        }
    });
    let mut out = ShowOut::default();
    if let Some(id) = pick.filter(|id| *id != cur) {
        choose(values, f.key, &id);
        out.changed = true;
    }
    out
}
