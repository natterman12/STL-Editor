//! The Measure panel (1.x `startMeasure`, `measureClick`), and the floating panel frame that
//! Properties (`info_panel.rs`) uses too: the tool panel's look (`#tool-panel`), with a title, a
//! form and a Cancel / Done row.

use egui::{Align, Area, CornerRadius, Frame, Id, Layout, Margin, Order, Rect, RichText, Sense, Stroke, Ui, Vec2};

use crate::theme::Tokens;
use crate::tools::fields::number::{fmt_len, unit_name};
use crate::tools::fields::{FORM_W, row_raw};
use crate::tools::panel::{INSET, PANEL_W};
use crate::ui::dialogs::button;

/// What the panel's buttons asked for this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelAct {
    None,
    /// Done / Close / Cancel / the × box.
    Close,
}

/// One row of a readout: label, value, and whether the value is bold.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub label: String,
    pub value: String,
    pub bold: bool,
}

impl Row {
    pub fn new(label: &str, value: impl Into<String>) -> Row {
        Row {
            label: label.to_owned(),
            value: value.into(),
            bold: false,
        }
    }
    pub fn bold(mut self) -> Row {
        self.bold = true;
        self
    }
}

/// The text of the Measure panel for these picks (`measureClick`): "Point 1" after the first
/// click, Distance and the three differences after the second. Lengths are in the current units.
pub fn measure_rows(pts: &[[f64; 3]], inch: bool) -> Vec<Row> {
    let unit = unit_name(inch);
    let str_of = |mm: f64| format!("{} {unit}", fmt_len(mm, None, inch));
    match pts {
        [] => Vec::new(),
        [p] => vec![Row::new(
            "Point 1",
            format!(
                "{} {unit}",
                p.iter().map(|v| fmt_len(*v, None, inch)).collect::<Vec<_>>().join(", ")
            ),
        )],
        [a, b, ..] => {
            let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            vec![
                Row::new("Distance", str_of(l)).bold(),
                Row::new("ΔX", str_of(d[0].abs())),
                Row::new("ΔY", str_of(d[1].abs())),
                Row::new("ΔZ", str_of(d[2].abs())),
            ]
        }
    }
}

/// The Measure panel.
pub fn show(ctx: &egui::Context, tk: &Tokens, ws: Rect, pts: &[[f64; 3]], inch: bool) -> PanelAct {
    let rows = measure_rows(pts, inch);
    frame(ctx, tk, ws, "measure-panel", "Measure", "measure", "Done", |ui| {
        note(ui, tk, "Click two points on the model.");
        ui.add_space(6.0);
        readout(ui, tk, &rows);
    })
}

// ------------------------------------------------------------------ the shared pieces

/// The floating panel (`#tool-panel`: right edge, 8 px in): title row, `body`, Cancel and `done`.
pub fn frame(
    ctx: &egui::Context,
    tk: &Tokens,
    ws: Rect,
    id: &'static str,
    title: &str,
    icon: &str,
    done: &str,
    body: impl FnOnce(&mut Ui),
) -> PanelAct {
    let mut act = PanelAct::None;
    let pos = egui::pos2(ws.max.x - INSET - PANEL_W, ws.min.y + INSET);
    let max_h = (ws.height() - 60.0).max(120.0);
    Area::new(Id::unique(id))
        .order(Order::Middle)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            Frame::new()
                .fill(tk.panel)
                .stroke(Stroke::new(1.0, tk.line))
                .corner_radius(CornerRadius::same(tk.radius as u8))
                .shadow(tk.shadow)
                .show(ui, |ui| {
                    ui.set_width(PANEL_W - 2.0);
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    // title row
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(PANEL_W - 2.0, 33.0), Sense::hover());
                    let cy = rect.center().y;
                    let mut x = rect.min.x + 10.0;
                    let img = crate::icons::icon(ctx, icon, 17.0, tk.accent);
                    egui::Image::new(img)
                        .paint_at(ui, Rect::from_center_size(egui::pos2(x + 8.5, cy), Vec2::splat(17.0)));
                    x += 17.0 + 7.0;
                    let font = egui::FontId::new(12.0, egui::FontFamily::Name("bold".into()));
                    let g = ui.painter().layout_no_wrap(title.to_owned(), font, tk.strong);
                    ui.painter().galley(egui::pos2(x, cy - g.size().y / 2.0), g, tk.strong);
                    let close = Rect::from_center_size(egui::pos2(rect.max.x - 8.0 - 11.0, cy), Vec2::splat(22.0));
                    let resp = ui.interact(close, Id::unique("measure-close"), Sense::click());
                    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Close panel"));
                    if resp.hovered() {
                        ui.painter().rect_filled(close, CornerRadius::same(4), tk.hover);
                    }
                    ui.painter().text(
                        close.center(),
                        egui::Align2::CENTER_CENTER,
                        "×",
                        egui::FontId::proportional(17.0),
                        if resp.hovered() { tk.strong } else { tk.muted },
                    );
                    if resp.on_hover_text("Close (Esc)").clicked() {
                        act = PanelAct::Close;
                    }
                    ui.painter()
                        .hline(rect.x_range(), rect.max.y, Stroke::new(1.0, tk.divider));
                    // the form
                    egui::ScrollArea::vertical()
                        .max_height((max_h - 36.0 - 46.0).max(60.0))
                        .auto_shrink([false, true])
                        .id_salt((id, "form"))
                        .show(ui, |ui| {
                            Frame::new()
                                .inner_margin(Margin {
                                    left: 10,
                                    right: 10,
                                    top: 8,
                                    bottom: 8,
                                })
                                .show(ui, |ui| {
                                    ui.set_width(FORM_W);
                                    ui.add_space(6.0);
                                    body(ui);
                                });
                        });
                    // Cancel and the main button
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(PANEL_W - 2.0, 41.0), Sense::hover());
                    ui.painter()
                        .hline(rect.x_range(), rect.min.y, Stroke::new(1.0, tk.divider));
                    let inner = Rect::from_min_max(
                        egui::pos2(rect.min.x + 10.0, rect.min.y + 8.0),
                        egui::pos2(rect.max.x - 10.0, rect.max.y - 9.0),
                    );
                    let mut right = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(inner)
                            .layout(Layout::right_to_left(Align::Center)),
                    );
                    right.spacing_mut().item_spacing.x = 8.0;
                    if button(&mut right, tk, done, true).clicked() | button(&mut right, tk, "Cancel", false).clicked()
                    {
                        act = PanelAct::Close;
                    }
                });
        });
    act
}

/// `.msg.info`: a coloured box with a line of text.
pub fn note(ui: &mut Ui, tk: &Tokens, text: &str) {
    Frame::new()
        .fill(tk.info_bg)
        .corner_radius(CornerRadius::same(tk.radius_sm as u8))
        .inner_margin(Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_width(FORM_W - 16.0);
            ui.add(egui::Label::new(RichText::new(text).size(12.0).color(tk.info_text)).wrap());
        });
}

/// `.section-label`: a small heading over a group of rows.
pub fn section_label(ui: &mut Ui, tk: &Tokens, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).size(11.0).strong().color(tk.muted)).wrap());
    ui.add_space(4.0);
}

/// Label-and-value rows (`.field` with a plain `.ctl`).
pub fn readout(ui: &mut Ui, tk: &Tokens, rows: &[Row]) {
    for r in rows {
        value_row(ui, tk, &r.label, &r.value, r.bold, tk.text);
        ui.add_space(6.0);
    }
}

/// One row with the value in `color`.
pub fn value_row(ui: &mut Ui, tk: &Tokens, label: &str, value: &str, bold: bool, color: egui::Color32) {
    row_raw(ui, tk, label, None, false, |ui, _w| {
        let mut t = RichText::new(value).size(12.0).color(color);
        if bold {
            t = t.family(egui::FontFamily::Name("bold".into()));
        }
        ui.add(egui::Label::new(t).wrap());
    });
}
