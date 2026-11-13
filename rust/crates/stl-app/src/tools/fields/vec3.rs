//! `vec3` field: three boxes (x, y, z), each a number or an expression.

use serde_json::Value;

use super::number::{expr_font, expr_of, expr_text, expr_title, input_font, is_expr, js_num, num_value, parse_len, parse_num_text, to_disp, unit_name};
use super::{row, text_commit, unit_label, Bufs, Edit, FieldEnv};
use crate::tools::Field;

/// What box `i` shows: `isLen ? U.toDisp(val[i] ?? 0) : (val[i] ?? 0)`.
fn shown(val: &Value, i: usize, is_len: bool, inch: bool) -> String {
    match val.get(i) {
        Some(Value::Number(n)) if is_len => js_num(to_disp(n.as_f64().unwrap_or(0.0), inch)),
        Some(Value::Number(n)) => js_num(n.as_f64().unwrap_or(0.0)),
        Some(Value::String(s)) if is_len => js_num(to_disp(parse_num_text(s), inch)),
        Some(Value::String(s)) => s.clone(),
        _ => "0".into(),
    }
}

pub fn show(ui: &mut egui::Ui, env: &FieldEnv, f: &Field, unit: Option<&str>, val: &Value, bufs: &mut Bufs) -> Option<Edit> {
    let is_len = unit == Some("mm");
    let tk = env.tk;
    row(ui, env, f, false, |ui, w| {
        let label = if is_len { unit_name(env.inch) } else { unit.unwrap_or("") };
        let uw = if label.is_empty() { 0.0 } else { 22.0 + 4.0 };
        let bw = ((w - uw - 8.0) / 3.0).max(30.0);
        let mut out = None;
        for i in 0..3 {
            let cell = val.get(i).cloned().unwrap_or(Value::Null);
            let (text, font, colour, tip) = if is_expr(&cell) {
                let s = cell.as_str().unwrap_or("").to_string();
                let tip = expr_title(&s, is_len, env.params);
                (s, expr_font(), tk.expr, Some(tip))
            } else {
                (shown(val, i, is_len, env.inch), input_font(), tk.text, None)
            };
            let key = format!("{}:{i}", f.key);
            if let Some(typed) = text_commit(ui, tk, bufs, &key, &text, bw, font, colour, tip) {
                let mut arr: Vec<Value> = match val {
                    Value::Array(a) => a.clone(),
                    _ => Vec::new(),
                };
                arr.resize(3, Value::from(0));
                // onFormChange: an expression is kept as text; a length is read in display units
                arr[i] = if expr_text(&typed) {
                    Value::String(expr_of(&typed))
                } else if is_len {
                    num_value(parse_len(&typed, env.inch).unwrap_or(0.0))
                } else {
                    num_value(parse_num_text(&typed))
                };
                out = Some(Edit { key: f.key.to_owned(), value: Value::Array(arr) });
            }
        }
        if !label.is_empty() {
            unit_label(ui, tk, label);
        }
        out
    })
}
