//! The `features` field: a tick-list of earlier cuts, holes and extrudes to repeat (1.x
//! `fieldHTML` `'features'`, `REPEATABLE`, and the timeline click that ticks one).

use egui::{RichText, Ui};
use serde_json::Value;

use super::bodies::ShowOut;
use crate::engine_link::TimelineItem;
use crate::theme::Tokens;
use crate::tools::{Ctx, Field, Values};

/// Features whose result can be repeated (mirrored / patterned).
pub const REPEATABLE: [&str; 4] = ["extrude", "revolve", "hole", "primitive"];

/// `repeatableFeatures(state)`: switched on, no error, not suppressed.
pub fn repeatable<'a>(ctx: &'a Ctx) -> Vec<&'a TimelineItem> {
    ctx.features.iter().filter(|f| REPEATABLE.contains(&f.kind.as_str()) && f.active && f.error.is_none() && !f.suppressed).collect()
}

fn ids(values: &Values, key: &str) -> Vec<String> {
    values.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect()).unwrap_or_default()
}

/// The right-hand word of a row: the step's operation (a hole cuts).
pub fn operation(item: &TimelineItem) -> String {
    item.params.get("operation").and_then(Value::as_str).map(str::to_owned).unwrap_or_else(|| if item.kind == "hole" { "cut".into() } else { String::new() })
}

/// Tick or untick a step (the checkbox, and a click on that step in the timeline).
pub fn toggle(values: &mut Values, key: &str, id: &str) {
    let mut cur = ids(values, key);
    if let Some(p) = cur.iter().position(|x| x == id) {
        cur.remove(p);
    } else {
        cur.push(id.to_owned());
    }
    values.insert(key.to_owned(), Value::Array(cur.into_iter().map(Value::from).collect()));
}

/// A wide field: it draws its own label above the list. `edit_id` is the step being edited
/// (it can't repeat itself).
pub fn show(ui: &mut Ui, tk: &Tokens, f: &Field, values: &mut Values, ctx: &Ctx, edit_id: Option<&str>) -> ShowOut {
    ui.label(RichText::new(f.label).size(11.0).color(tk.label));
    let sel = ids(values, f.key);
    let list: Vec<&TimelineItem> = repeatable(ctx).into_iter().filter(|x| Some(x.id.as_str()) != edit_id).collect();
    let mut out = ShowOut::default();
    if list.is_empty() {
        ui.label(RichText::new("No cuts, holes or extrudes yet").size(11.0).color(tk.muted));
    }
    for item in list {
        let mut on = sel.contains(&item.id);
        ui.horizontal(|ui| {
            let resp = super::check_box(ui, tk, &mut on);
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::CheckBox, true, item.label.clone()));
            ui.label(RichText::new(&item.label).size(12.0).color(tk.text));
            if resp.changed() {
                toggle(values, f.key, &item.id);
                out.changed = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(operation(item)).size(11.0).color(tk.muted));
            });
        });
    }
    out
}
