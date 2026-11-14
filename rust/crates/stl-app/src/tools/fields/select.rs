//! `select` field: a segmented control when every option fits on one line, otherwise a list.

use egui::FontId;
use serde_json::Value;

use super::{framed_combo, row, seg_strip, Edit, FieldEnv, CTL_W, FORM_W};
use crate::tools::Field;

/// `segFits`: at most 4 options, each label at most 16 characters, and all of them on one line.
pub fn seg_fits(ui: &egui::Ui, options: &[(String, String)], wide: bool) -> bool {
    if options.len() > 4 || options.iter().any(|(_, l)| l.chars().count() > 16) {
        return false;
    }
    let avail = if wide { FORM_W } else { CTL_W };
    let font = FontId::proportional(11.0);
    let need: f32 = options.iter().map(|(_, l)| text_w(ui, l, &font) + 7.0).sum::<f32>() + 2.0;
    need <= avail
}

fn text_w(ui: &egui::Ui, text: &str, font: &FontId) -> f32 {
    ui.fonts_mut(|f| f.layout_no_wrap(text.to_owned(), font.clone(), egui::Color32::WHITE).size().x)
}

fn current_label(all: &[(String, String)], val: &str) -> String {
    all.iter().find(|(k, _)| k == val).map(|(_, l)| l.clone()).unwrap_or_default()
}

pub fn value_str(val: &Value) -> String {
    match val {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => super::number::js_num(n.as_f64().unwrap_or(0.0)),
        other => other.to_string(),
    }
}

fn group_rows(options: &[(String, String, Option<String>)]) -> (Vec<(String, String)>, Vec<(String, Vec<(String, String)>)>) {
    let flat: Vec<(String, String)> = options.iter().map(|(a, b, _)| (a.clone(), b.clone())).collect();
    let mut order: Vec<String> = Vec::new();
    let mut by: std::collections::BTreeMap<String, Vec<(String, String)>> = std::collections::BTreeMap::new();
    for (id, label, g) in options {
        let g = g.clone().unwrap_or_default();
        if !order.contains(&g) {
            order.push(g.clone());
        }
        by.entry(g).or_default().push((id.clone(), label.clone()));
    }
    let groups = order
        .into_iter()
        .map(|g| {
            let items = by.remove(&g).unwrap_or_default();
            (g, items)
        })
        .collect();
    (flat, groups)
}

/// `options` are `(id, label, group)`; when `grouped`, combo headings use `group`.
pub fn show(
    ui: &mut egui::Ui,
    env: &FieldEnv,
    f: &Field,
    options: &[(String, String, Option<String>)],
    grouped: bool,
    val: &Value,
) -> Option<Edit> {
    let (pairs, groups) = if grouped {
        group_rows(options)
    } else {
        let pairs: Vec<(String, String)> = options.iter().map(|(a, b, _)| (a.clone(), b.clone())).collect();
        (pairs, Vec::new())
    };
    let tk = env.tk;
    let cur = value_str(val);
    let mut pick: Option<String> = None;
    let seg = groups.is_empty() && seg_fits(ui, &pairs, false);
    row(ui, env, f, false, |ui, w| {
        if seg {
            pick = seg_strip(ui, tk, ui.scope_id().with(("seg", f.key)), w, &pairs, &cur);
        } else {
            let mut all: Vec<(String, String)> = pairs.clone();
            for (_, items) in &groups {
                all.extend(items.iter().cloned());
            }
            let text = current_label(&all, &cur);
            framed_combo(ui, tk, ("tool-select", f.key), w, &text, |ui| {
                if !grouped {
                    for (k, l) in &pairs {
                        if ui.selectable_label(*k == cur, l).clicked() {
                            pick = Some(k.clone());
                        }
                    }
                }
                for (g, items) in &groups {
                    if !g.is_empty() {
                        ui.label(egui::RichText::new(g).size(11.0).strong().color(tk.muted));
                    }
                    for (k, l) in items {
                        if ui.selectable_label(*k == cur, l).clicked() {
                            pick = Some(k.clone());
                        }
                    }
                }
            });
        }
    });
    pick.filter(|k| *k != cur).map(|k| Edit { key: f.key.to_owned(), value: Value::String(k) })
}
