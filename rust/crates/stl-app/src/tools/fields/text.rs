//! `text` field.

use serde_json::Value;

use super::number::input_font;
use super::{row, text_commit, Bufs, Edit, FieldEnv};
use crate::tools::Field;

/// The grey text in an empty box: 1.x always shows "optional" (app.js:735), whatever the tool says.
pub fn placeholder_text(_tool_says: Option<&str>) -> &'static str {
    "optional"
}

/// 1.x always shows `placeholder="optional"`, whatever the tool says.
pub fn show(ui: &mut egui::Ui, env: &FieldEnv, f: &Field, placeholder: Option<&str>, val: &Value, bufs: &mut Bufs) -> Option<Edit> {
    let tk = env.tk;
    let shown = match val {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    };
    row(ui, env, f, false, |ui, w| {
        let typed = text_commit(ui, tk, bufs, f.key, &shown, w, input_font(), tk.text, None);
        // the placeholder, drawn over an empty box that isn't being typed in
        let id = egui::Id::unique(("tool-field", f.key));
        let empty = bufs.get(f.key).map_or(shown.is_empty(), |b| b.is_empty());
        if empty && !ui.memory(|m| m.has_focus(id)) {
            let r = ui.min_rect();
            let at = egui::pos2(r.max.x - w + 6.0, r.center().y);
            ui.painter().text(at, egui::Align2::LEFT_CENTER, placeholder_text(placeholder), input_font(), tk.muted);
        }
        typed.map(|t| Edit { key: f.key.to_owned(), value: Value::String(t) })
    })
}
