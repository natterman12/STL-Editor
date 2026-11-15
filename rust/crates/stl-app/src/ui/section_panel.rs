//! The Section View panel (1.x `startSection`, `renderSection`), spec 2.3: Plane (X, Y, Z, or a
//! picked plane or flat face), Distance, Flip, Hatch, 1.x's note, Remove cut, and Done.
//! It only draws and reports; `app_section.rs` acts on what it reports.

use egui::text::{LayoutJob, TextFormat};
use egui::{Align, Area, CornerRadius, FontFamily, FontId, Frame, Id, Layout, Margin, Order, Rect, Sense, Stroke, Ui, Vec2};

use crate::theme::Tokens;
use crate::tools::fields::number::{expr_font, input_font, unit_name};
use crate::tools::fields::{check_box, row_raw, seg_strip, text_commit, unit_label, Bufs, FORM_W};
use crate::tools::panel::{ctl_button, INSET, PANEL_W};
use crate::ui::dialogs::button;

/// The panel's title (`#tool-title`).
pub const TITLE: &str = "Section View";
/// The main button (`#tool-ok`).
pub const DONE: &str = "Done";
/// The hint while Section is open: 1.x's, with the arrow and Distance in place of its slider.
pub const HINT: &str = "Drag the arrow or change Distance to look inside · only the view changes · the cut stays when you close this; X turns it off and on";
/// 1.x's note, word for word; the parts in `**` are bold.
pub const NOTE: &str = "The cut only changes what you see: export, Properties and Measure use the whole part. It stays on after you click Done; turn it off and on with **Section** in the view bar or the **X** key.";
/// Below the Plane buttons.
pub const PICK_HINT: &str = "Or click a flat face or a plane.";
/// A click on a face that is not flat.
pub const NOT_FLAT: &str = "That face isn't flat. Click a flat face or a plane.";

/// What the panel shows.
#[derive(Clone, Debug, PartialEq)]
pub struct SectionView {
    /// The axis button that is on (0 X, 1 Y, 2 Z), or `None` for a picked plane or face.
    pub axis: Option<usize>,
    /// A picked plane's or face's name.
    pub picked: Option<String>,
    /// The Distance box's text (an `=` expression as typed, else the number in the current units).
    pub distance: String,
    pub expr: bool,
    /// A tip on the Distance box (an expression's value).
    pub tip: Option<String>,
    pub flip: bool,
    pub hatch: bool,
    pub inch: bool,
    /// A message under the plane (a face that isn't flat, an expression that doesn't work).
    pub msg: Option<String>,
}

/// What the panel's controls asked for this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum SectionAct {
    None,
    Axis(usize),
    /// The Distance box was committed with this text.
    Distance(String),
    Flip(bool),
    Hatch(bool),
    Remove,
    Done,
    Cancel,
}

/// The Distance box's key in the panel's text buffers.
pub const DISTANCE_KEY: &str = "section-distance";

/// Draw the panel.
pub fn show(ctx: &egui::Context, tk: &Tokens, ws: Rect, v: &SectionView, bufs: &mut Bufs) -> SectionAct {
    let mut act = SectionAct::None;
    let pos = egui::pos2(ws.max.x - INSET - PANEL_W, ws.min.y + INSET);
    let max_h = (ws.height() - 60.0).max(120.0);
    Area::new(Id::unique("section-panel")).order(Order::Middle).fixed_pos(pos).show(ctx, |ui| {
        Frame::new().fill(tk.panel).stroke(Stroke::new(1.0, tk.line)).corner_radius(CornerRadius::same(tk.radius as u8)).shadow(tk.shadow).show(ui, |ui| {
            ui.set_width(PANEL_W - 2.0);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            title_row(ui, ctx, tk, &mut act);
            egui::ScrollArea::vertical().max_height((max_h - 36.0 - 46.0).max(60.0)).auto_shrink([false, true]).id_salt("section-form").show(ui, |ui| {
                Frame::new().inner_margin(Margin { left: 10, right: 10, top: 8, bottom: 8 }).show(ui, |ui| {
                    ui.set_width(FORM_W);
                    ui.add_space(6.0);
                    form(ui, tk, v, bufs, &mut act);
                });
            });
            let (rect, _) = ui.allocate_exact_size(Vec2::new(PANEL_W - 2.0, 41.0), Sense::hover());
            ui.painter().hline(rect.x_range(), rect.min.y, Stroke::new(1.0, tk.divider));
            let inner = Rect::from_min_max(egui::pos2(rect.min.x + 10.0, rect.min.y + 8.0), egui::pos2(rect.max.x - 10.0, rect.max.y - 9.0));
            let mut right = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(Layout::right_to_left(Align::Center)));
            right.spacing_mut().item_spacing.x = 8.0;
            if button(&mut right, tk, DONE, true).clicked() {
                act = SectionAct::Done;
            }
            if button(&mut right, tk, "Cancel", false).clicked() {
                act = SectionAct::Cancel;
            }
        });
    });
    act
}

fn title_row(ui: &mut Ui, ctx: &egui::Context, tk: &Tokens, act: &mut SectionAct) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(PANEL_W - 2.0, 33.0), Sense::hover());
    let cy = rect.center().y;
    let mut x = rect.min.x + 10.0;
    let img = crate::icons::icon(ctx, "section", 17.0, tk.accent);
    egui::Image::new(img).paint_at(ui, Rect::from_center_size(egui::pos2(x + 8.5, cy), Vec2::splat(17.0)));
    x += 17.0 + 7.0;
    let font = FontId::new(12.0, FontFamily::Name("bold".into()));
    let g = ui.painter().layout_no_wrap(TITLE.to_owned(), font, tk.strong);
    ui.painter().galley(egui::pos2(x, cy - g.size().y / 2.0), g, tk.strong);
    let close = Rect::from_center_size(egui::pos2(rect.max.x - 8.0 - 11.0, cy), Vec2::splat(22.0));
    let resp = ui.interact(close, Id::unique("section-close"), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Close panel"));
    if resp.hovered() {
        ui.painter().rect_filled(close, CornerRadius::same(4), tk.hover);
    }
    ui.painter().text(close.center(), egui::Align2::CENTER_CENTER, "×", FontId::proportional(17.0), if resp.hovered() { tk.strong } else { tk.muted });
    if resp.on_hover_text("Close (Esc)").clicked() {
        *act = SectionAct::Cancel;
    }
    ui.painter().hline(rect.x_range(), rect.max.y, Stroke::new(1.0, tk.divider));
}

fn form(ui: &mut Ui, tk: &Tokens, v: &SectionView, bufs: &mut Bufs, act: &mut SectionAct) {
    // Plane: X / Y / Z (1.x's Axis), or the picked plane or face
    let pairs: Vec<(String, String)> = ["X", "Y", "Z"].iter().map(|a| (a.to_string(), a.to_string())).collect();
    let cur = v.axis.map(|i| ["X", "Y", "Z"][i]).unwrap_or("");
    row_raw(ui, tk, "Plane", None, false, |ui, w| {
        if let Some(k) = seg_strip(ui, tk, Id::unique("section-axis"), w, &pairs, cur) {
            *act = SectionAct::Axis("XYZ".find(k.as_str()).unwrap_or(2));
        }
    });
    ui.add_space(4.0);
    let below = match &v.picked {
        Some(name) => name.clone(),
        None => PICK_HINT.to_owned(),
    };
    row_raw(ui, tk, "", None, false, |ui, _| {
        let colour = if v.picked.is_some() { tk.text } else { tk.muted };
        ui.add(egui::Label::new(egui::RichText::new(below).size(11.5).color(colour)).wrap());
    });
    if let Some(m) = &v.msg {
        ui.add_space(4.0);
        row_raw(ui, tk, "", None, false, |ui, _| {
            ui.add(egui::Label::new(egui::RichText::new(m).size(11.5).color(tk.err_text)).wrap());
        });
    }
    ui.add_space(6.0);

    // Distance: a normal number box (an `=` expression, inch mode)
    row_raw(ui, tk, "Distance", None, false, |ui, w| {
        let (font, colour) = if v.expr { (expr_font(), tk.expr) } else { (input_font(), tk.text) };
        let uw = 22.0 + 4.0;
        if let Some(t) = text_commit(ui, tk, bufs, DISTANCE_KEY, &v.distance, w - uw, font, colour, v.tip.clone()) {
            *act = SectionAct::Distance(t);
        }
        unit_label(ui, tk, if v.expr { "mm" } else { unit_name(v.inch) });
    });
    ui.add_space(6.0);

    let mut flip = v.flip;
    row_raw(ui, tk, "Flip", None, false, |ui, _| {
        let resp = check_box(ui, tk, &mut flip);
        let on = flip;
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, on, "Flip"));
        if resp.changed() {
            *act = SectionAct::Flip(flip);
        }
    });
    ui.add_space(6.0);
    let mut hatch = v.hatch;
    row_raw(ui, tk, "Hatch", None, false, |ui, _| {
        let resp = check_box(ui, tk, &mut hatch);
        let on = hatch;
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, on, "Hatch"));
        if resp.changed() {
            *act = SectionAct::Hatch(hatch);
        }
    });
    ui.add_space(6.0);

    note(ui, tk, NOTE);
    ui.add_space(6.0);
    row_raw(ui, tk, "", None, false, |ui, _| {
        if ctl_button(ui, tk, "Remove cut", true).clicked() {
            *act = SectionAct::Remove;
        }
    });
}

/// `.msg.info` with 1.x's `<b>` parts in bold.
fn note(ui: &mut Ui, tk: &Tokens, text: &str) {
    let mut job = LayoutJob::default();
    for (i, part) in text.split("**").enumerate() {
        let family = if i % 2 == 1 { FontFamily::Name("bold".into()) } else { FontFamily::Proportional };
        job.append(part, 0.0, TextFormat { font_id: FontId::new(12.0, family), color: tk.info_text, ..Default::default() });
    }
    Frame::new().fill(tk.info_bg).corner_radius(CornerRadius::same(tk.radius_sm as u8)).inner_margin(Margin::symmetric(8, 6)).show(ui, |ui| {
        ui.set_width(FORM_W - 16.0);
        ui.add(egui::Label::new(job).wrap());
    });
}

/// The note as plain text (for the tests): 1.x's words without the bold marks.
pub fn note_text() -> String {
    NOTE.replace("**", "")
}
