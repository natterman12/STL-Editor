//! Parameters panel: names, expressions and live values (1.x parameters modal table).

use std::collections::HashSet;

use egui::{RichText, Ui};
use stl_engine::params::{Param, resolve_params};

use crate::engine_link::{Command, ParamView};
use crate::theme::Tokens;

const HINT: &str = "Name a value once and use it in any number: type the name (or =width/2 + 3) in a tool's field. Change a value here and every step that uses it is rebuilt. Lengths are in mm, angles in degrees; a parameter can use others.";

#[derive(Clone, Debug)]
struct Row {
    name: String,
    expr: String,
    note: String,
}

#[derive(Clone, Debug, Default)]
pub struct Params {
    rows: Vec<Row>,
    /// Snapshot of the engine list we last synced from (`None` until the first `sync`).
    snap: Option<Vec<(String, String, String)>>,
    open: bool,
    /// Set when Apply would remove a parameter that steps still use (1.x asks first, app.js:1181).
    confirm: Option<String>,
}

impl Params {
    pub fn set_open(&mut self, open: bool) {
        if open && !self.open {
            self.confirm = None;
            self.snap = None; // start from the engine's list, not from edits that were cancelled
        }
        self.open = open;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn sync(&mut self, views: &[ParamView]) {
        let key: Vec<_> = views.iter().map(|p| (p.name.clone(), p.expr.clone(), p.note.clone())).collect();
        if self.snap.as_ref() != Some(&key) {
            self.rows = views
                .iter()
                .map(|p| Row { name: p.name.clone(), expr: p.expr.clone(), note: p.note.clone() })
                .collect();
            if self.rows.is_empty() {
                self.rows.push(Row { name: String::new(), expr: String::new(), note: String::new() });
            }
            self.snap = Some(key);
        }
    }
}

fn row_to_param(r: &Row) -> Param {
    Param { name: r.name.trim().to_string(), expr: r.expr.trim().to_string(), note: r.note.clone() }
}

/// 1.x's wording when Apply would remove parameters that steps use (`missing` is never empty).
fn confirm_text(missing: &[String]) -> String {
    let many = missing.len() > 1;
    format!(
        "{} {} used in the timeline. Remove {} anyway? Those steps will show an error until you fix them.",
        missing.join(", "),
        if many { "are" } else { "is" },
        if many { "them" } else { "it" }
    )
}

/// Used parameters (in the engine's list) that the table no longer has.
fn removed_but_used(rows: &[Row], views: &[ParamView], used: &HashSet<String>) -> Vec<String> {
    views
        .iter()
        .filter(|v| used.contains(&v.name) && !rows.iter().any(|r| r.name.trim() == v.name))
        .map(|v| v.name.clone())
        .collect()
}

fn fmt_num(v: f64) -> String {
    ((v * 1000.0).round() / 1000.0).to_string()
}

/// Draw the parameters table. Pushes `Command::SetParams` on Apply or Enter.
pub fn show(
    ui: &mut Ui,
    tokens: &Tokens,
    state: &mut Params,
    views: &[ParamView],
    used: &HashSet<String>,
    commands: &mut Vec<Command>,
) {
    if !state.open {
        return;
    }

    state.sync(views);

    if let Some(msg) = state.confirm.clone() {
        ui.label(RichText::new("STL Editor").strong().color(tokens.strong).size(14.0));
        ui.label(RichText::new(msg).color(tokens.text).size(12.0));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                state.confirm = None; // back to the table with the edits kept
            }
            if ui.button("OK").clicked() {
                commit_rows(&state.rows, commands);
                state.confirm = None;
                state.open = false;
            }
        });
        return;
    }

    ui.label(RichText::new("Parameters").strong().color(tokens.strong).size(14.0));
    ui.label(RichText::new(HINT).color(tokens.muted).size(11.5));
    ui.add_space(6.0);

    let list: Vec<Param> = state
        .rows
        .iter()
        .filter(|r| !r.name.trim().is_empty() || !r.expr.trim().is_empty())
        .map(row_to_param)
        .collect();
    let resolved = resolve_params(&list);

    egui::Grid::new("params_table")
        .num_columns(5)
        .spacing([4.0, 4.0])
        .show(ui, |ui| {
            ui.label(RichText::new("Name").color(tokens.muted).size(11.0).strong());
            ui.label(RichText::new("Value").color(tokens.muted).size(11.0).strong());
            ui.label("");
            ui.label(RichText::new("Note").color(tokens.muted).size(11.0).strong());
            ui.label("");
            ui.end_row();

            let mut remove = None;
            let mut commit = false;
            for (i, row) in state.rows.iter_mut().enumerate() {
                // `push_id(..).response` is the scope's own response; the field is `.inner`.
                let name_resp = ui
                    .push_id(("param_name", i), |ui| ui.add(egui::TextEdit::singleline(&mut row.name).desired_width(80.0)))
                    .inner;
                let expr_resp = ui
                    .push_id(("param_expr", i), |ui| {
                        ui.add(egui::TextEdit::singleline(&mut row.expr).desired_width(90.0).text_color(tokens.expr))
                    })
                    .inner;

                let n = row.name.trim();
                let pv = if n.is_empty() {
                    String::new()
                } else if let Some(err) = resolved.errors.get(n) {
                    err.clone()
                } else if let Some(v) = resolved.values.get(n) {
                    format!("= {}", fmt_num(*v))
                } else {
                    String::new()
                };
                let is_err = resolved.errors.contains_key(n);
                let mut job = egui::text::LayoutJob::default();
                let font = egui::FontId::proportional(12.0);
                job.append(&pv, 0.0, egui::TextFormat::simple(font.clone(), if is_err { tokens.danger } else { tokens.text }));
                if !is_err && !pv.is_empty() && used.contains(n) {
                    // 1.x greys only the "· used" part (a `.muted` span, app.js:1166)
                    job.append(" · used", 0.0, egui::TextFormat::simple(font, tokens.muted));
                }
                ui.add(egui::Label::new(job).truncate());

                ui.add(egui::TextEdit::singleline(&mut row.note).desired_width(80.0));

                if ui.small_button("×").clicked() {
                    remove = Some(i);
                }
                ui.end_row();

                if (name_resp.lost_focus() || expr_resp.lost_focus())
                    && ui.input(|inp| inp.key_pressed(egui::Key::Enter))
                {
                    commit = true;
                }
            }
            if commit {
                let missing = removed_but_used(&state.rows, views, used);
                if missing.is_empty() {
                    commit_rows(&state.rows, commands);
                } else {
                    state.confirm = Some(confirm_text(&missing));
                }
            }
            if let Some(i) = remove {
                // like 1.x, × only takes the row out of the table; Apply sends the list
                state.rows.remove(i);
                if state.rows.is_empty() {
                    state.rows.push(Row { name: String::new(), expr: String::new(), note: String::new() });
                }
            }
        });

    ui.horizontal(|ui| {
        if ui.button("+ Add parameter").clicked() {
            state.rows.push(Row { name: String::new(), expr: String::new(), note: String::new() });
        }
        if ui.button("Apply").clicked() {
            let missing = removed_but_used(&state.rows, views, used);
            if missing.is_empty() {
                commit_rows(&state.rows, commands);
                state.open = false;
            } else {
                state.confirm = Some(confirm_text(&missing));
            }
        }
        if ui.button("Cancel").clicked() {
            state.snap = None;
            state.sync(views);
            state.open = false;
        }
    });
}

fn commit_rows(rows: &[Row], commands: &mut Vec<Command>) {
    let list: Vec<Param> = rows
        .iter()
        .filter(|r| !r.name.trim().is_empty() || !r.expr.trim().is_empty())
        .map(row_to_param)
        .collect();
    commands.push(Command::SetParams(list));
}
