//! The modal dialog (1.x `#modal`): a prompt with a text input (Enter accepts, Escape cancels),
//! a confirm with Cancel / OK, and an alert with OK.
//!
//! egui has no promises, so a dialog carries a `tag` chosen by the caller and the result is read
//! with [`Dialogs::take_result`], e.g. `("rename-body", DialogResult::Text("Cube"))`.

use crate::theme::Tokens;
use egui::{Align, Area, Color32, CornerRadius, Frame, Id, Key, Layout, Margin, Modifiers, Order, RichText, Stroke, Vec2};

#[derive(Clone, Debug, PartialEq)]
pub enum DialogResult {
    /// A prompt's trimmed text.
    Text(String),
    /// Confirm: OK. Alert: OK.
    Yes,
    /// Cancel, Escape.
    Cancelled,
}

#[derive(Clone, Debug)]
enum Kind {
    Prompt { title: String, buf: String, fresh: bool },
    Confirm { msg: String },
    Alert { title: String, text: String, links: Vec<(String, String)>, rows: Vec<(String, String)> },
}

#[derive(Default)]
pub struct Dialogs {
    cur: Option<(String, Kind)>,
    done: Option<(String, DialogResult)>,
}

impl Dialogs {
    pub fn open_prompt(&mut self, tag: &str, title: &str, value: &str) {
        self.cur = Some((tag.into(), Kind::Prompt { title: title.into(), buf: value.into(), fresh: true }));
    }
    pub fn open_confirm(&mut self, tag: &str, msg: &str) {
        self.cur = Some((tag.into(), Kind::Confirm { msg: msg.into() }));
    }
    /// `links` are (label, url) pairs shown under the text.
    pub fn open_alert(&mut self, tag: &str, title: &str, text: &str, links: Vec<(String, String)>) {
        self.cur = Some((tag.into(), Kind::Alert { title: title.into(), text: text.into(), links, rows: Vec::new() }));
    }
    /// An alert that is a two-column list (1.x `.help-grid`: a key cap, then what it does).
    pub fn open_alert_rows(&mut self, tag: &str, title: &str, rows: &[(&str, &str)]) {
        let rows = rows.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        self.cur = Some((tag.into(), Kind::Alert { title: title.into(), text: String::new(), links: Vec::new(), rows }));
    }
    pub fn is_open(&self) -> bool {
        self.cur.is_some()
    }
    pub fn take_result(&mut self) -> Option<(String, DialogResult)> {
        self.done.take()
    }
}

/// A button as 1.x's `.modal-actions button` (and `.empty-actions button`).
pub fn button(ui: &mut egui::Ui, tk: &Tokens, label: &str, primary: bool) -> egui::Response {
    let (fill, hover_fill, stroke, text) = if primary { (tk.primary, tk.primary_hover, tk.primary, Color32::WHITE) } else { (tk.panel_2, tk.panel_2, tk.input_line, tk.text) };
    ui.scope(|ui| {
        let v = ui.visuals_mut();
        v.widgets.inactive.weak_bg_fill = fill;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, stroke);
        v.widgets.hovered.weak_bg_fill = hover_fill;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, if primary { tk.primary_hover } else { tk.accent });
        v.widgets.hovered.fg_stroke.color = if primary { Color32::WHITE } else { tk.strong };
        let b = egui::Button::new(RichText::new(label).size(12.0).color(text)).corner_radius(CornerRadius::same(tk.radius_sm as u8)).min_size(Vec2::new(0.0, 24.0));
        ui.add(b)
    })
    .inner
}

/// Draw the open dialog, if any. Returns a URL when a link in an alert was clicked.
pub fn show(ctx: &egui::Context, tk: &Tokens, d: &mut Dialogs) -> Option<String> {
    let (tag, kind) = d.cur.as_mut()?;
    let screen = ctx.content_rect();
    // the scrim blocks everything under it
    Area::new(Id::unique("modal-scrim")).order(Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
        let (r, _) = ui.allocate_exact_size(screen.size(), egui::Sense::click_and_drag());
        ui.painter().rect_filled(r, 0.0, tk.scrim);
    });
    let mut result: Option<DialogResult> = None;
    let mut url = None;
    // keys first, so the text field doesn't see them
    let enter = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
    let esc = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
    Area::new(Id::unique("modal-card")).order(Order::Tooltip).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
        Frame::new()
            .fill(tk.panel)
            .stroke(Stroke::new(1.0, tk.line))
            .corner_radius(CornerRadius::same(9))
            .shadow(tk.shadow_lg)
            .inner_margin(Margin { left: 20, right: 20, top: 18, bottom: 14 })
            .show(ui, |ui| {
                ui.set_min_width(300.0);
                ui.set_max_width(480.0);
                let title = |ui: &mut egui::Ui, t: &str| {
                    ui.label(RichText::new(t).font(egui::FontId::new(15.0, egui::FontFamily::Name("bold".into()))).color(tk.strong));
                    ui.add_space(8.0);
                };
                let primary_label = "OK";
                match kind {
                    Kind::Prompt { title: t, buf, fresh } => {
                        title(ui, t);
                        let id = Id::unique("modal-input");
                        let out = egui::TextEdit::singleline(buf).id(id).desired_width(f32::INFINITY).margin(Margin::symmetric(8, 6)).show(ui);
                        if *fresh {
                            *fresh = false;
                            out.response.request_focus();
                            let mut st = out.state.clone();
                            st.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(buf.chars().count()))));
                            st.store(ui.ctx(), id);
                        }
                        if enter {
                            result = Some(DialogResult::Text(buf.trim().to_owned()));
                        }
                    }
                    Kind::Confirm { msg } => {
                        title(ui, "STL Editor");
                        ui.label(RichText::new(msg.as_str()).size(12.0).color(tk.text));
                    }
                    Kind::Alert { title: t, text, links, rows } => {
                        title(ui, t);
                        if !text.is_empty() {
                            ui.label(RichText::new(text.as_str()).size(12.0).color(tk.text));
                        }
                        if !rows.is_empty() {
                            egui::Grid::new("help-grid").num_columns(2).spacing(Vec2::new(14.0, 4.0)).show(ui, |ui| {
                                for (k, v) in rows.iter() {
                                    Frame::new().fill(tk.panel_2).stroke(Stroke::new(1.0, tk.line)).corner_radius(CornerRadius::same(3)).inner_margin(Margin::symmetric(5, 0)).show(ui, |ui| {
                                        ui.label(RichText::new(k.as_str()).size(11.5).color(tk.text));
                                    });
                                    ui.label(RichText::new(v.as_str()).size(12.5).color(tk.text));
                                    ui.end_row();
                                }
                            });
                        }
                        for (label, u) in links.iter() {
                            if ui.link(RichText::new(label.as_str()).size(12.0).color(tk.accent)).clicked() {
                                url = Some(u.clone());
                            }
                        }
                    }
                }
                ui.add_space(14.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if button(ui, tk, primary_label, true).clicked() {
                        result = Some(match kind {
                            Kind::Prompt { buf, .. } => DialogResult::Text(buf.trim().to_owned()),
                            _ => DialogResult::Yes,
                        });
                    }
                    if !matches!(kind, Kind::Alert { .. }) && button(ui, tk, "Cancel", false).clicked() {
                        result = Some(DialogResult::Cancelled);
                    }
                });
            });
    });
    if esc && result.is_none() {
        result = Some(DialogResult::Cancelled);
    }
    if let Some(r) = result {
        d.done = Some((tag.clone(), r));
        d.cur = None;
    }
    url
}
