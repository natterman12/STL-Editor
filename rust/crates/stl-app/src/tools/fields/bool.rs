//! `bool` field: a checkbox.

use serde_json::Value;

use super::{row, Edit, FieldEnv};
use crate::tools::Field;

/// JavaScript truthiness of a stored value.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map_or(false, |x| x != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

pub fn show(ui: &mut egui::Ui, env: &FieldEnv, f: &Field, val: &Value) -> Option<Edit> {
    let mut on = truthy(val);
    let changed = row(ui, env, f, false, |ui, _| super::check_box(ui, env.tk, &mut on).changed());
    changed.then(|| Edit { key: f.key.to_owned(), value: Value::Bool(on) })
}
