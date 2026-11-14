//! Field widgets, one per kind (1.x `fieldHTML`).
//!
//! The plain kinds (`number`, `select`, `bool`, `vec3`, `text`) are drawn here. The kinds picked in
//! the viewport go through [`pick_field`], which draws the label and hands the control to Task 3's
//! `bodies::show_pick`.

use std::collections::{BTreeMap, HashMap};

use egui::{Align, Color32, CornerRadius, Layout, Rect, RichText, Sense, Stroke, Ui, Vec2};
use serde_json::Value;

use crate::theme::Tokens;
use crate::tools::{Ctx, Field, Values};

pub mod number;
pub mod select;
pub mod r#bool;
pub mod vec3;
pub mod text;
pub mod bodies;
pub mod plane;
pub mod sketch;
pub mod regions;
pub mod features;
pub mod facepoints;
pub mod pface;
pub mod cylface;
pub mod points3;
pub mod axisline;

/// `.field { grid-template-columns: 92px 1fr; gap: 6px }`.
pub const LABEL_W: f32 = 92.0;
pub const GAP: f32 = 6.0;
/// The form's inner width: the panel is 290 wide with a 1 px border and 10 px padding each side.
pub const FORM_W: f32 = 268.0;
/// The control column of a normal row (`1fr`).
pub const CTL_W: f32 = FORM_W - LABEL_W - GAP;
/// A row's height: an input is 16 px of text, 4 px padding and a 1 px border on each side.
pub const ROW_H: f32 = 26.0;

/// A change a field asks for: set `values[key]`. The panel runs `onChange` and the preview.
#[derive(Clone, Debug, PartialEq)]
pub struct Edit {
    pub key: String,
    pub value: Value,
}

/// The text the user is typing in a box, by field (`"key"` or `"key:i"`).
pub type Bufs = HashMap<String, String>;

/// What a plain field needs to know about the app.
pub struct FieldEnv<'a> {
    pub tk: &'a Tokens,
    /// Inch mode: lengths are shown and typed in inches, and sent in mm.
    pub inch: bool,
    /// The project's parameters with a value (1.x `paramValues()`).
    pub params: &'a BTreeMap<String, f64>,
}

/// A field row: the label in a 92 px column (a `?` mark with the help text on hover), then the
/// control. `wide` has no label column. `ctl` gets the width left for the control.
pub fn row<R>(ui: &mut Ui, env: &FieldEnv, f: &Field, wide: bool, ctl: impl FnOnce(&mut Ui, f32) -> R) -> R {
    row_raw(ui, env.tk, f.label, f.help, wide, ctl)
}

/// [`row`] without a `Field` (the Support Fins form has its own field records).
pub fn row_raw<R>(ui: &mut Ui, tk: &Tokens, label: &str, help: Option<&str>, wide: bool, ctl: impl FnOnce(&mut Ui, f32) -> R) -> R {
    ui.allocate_ui_with_layout(Vec2::new(FORM_W, ROW_H), Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing = Vec2::new(GAP, 4.0);
        ui.set_min_height(ROW_H);
        if wide {
            let r = ctl(ui, FORM_W);
            if let Some(h) = help {
                ui.interact(ui.min_rect(), ui.scope_id().with("help"), Sense::hover()).on_hover_text(h);
            }
            return r;
        }
        // `grid-template-columns: 92px 1fr`: the label column is 92 px wide whatever the label says
        ui.allocate_ui_with_layout(Vec2::new(LABEL_W, ROW_H), Layout::left_to_right(Align::Center), |ui| {
            ui.set_width(LABEL_W);
            ui.set_min_height(ROW_H);
            // the label and its `?` flow like inline text: a long label sends the `?` to a second line
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(3.0, 2.0);
                if !label.is_empty() {
                    ui.add(egui::Label::new(RichText::new(label).size(12.0).color(tk.label)));
                }
                if let Some(h) = help {
                    help_mark(ui, tk, h);
                }
            });
        });
        ui.spacing_mut().item_spacing.x = 4.0; // .ctl { gap: 4px }
        ctl(ui, CTL_W)
    })
    .inner
}

/// `.field label .help`: a 14 px circle with a `?`, the help text on hover.
pub fn help_mark(ui: &mut Ui, tk: &Tokens, text: &str) {
    let (r, resp) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
    ui.painter().circle_stroke(r.center(), 6.5, Stroke::new(1.0, tk.line));
    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "?", egui::FontId::proportional(9.5), tk.muted);
    resp.on_hover_text(text);
}

/// `.field select`: a bordered box in the input colours with a chevron; the list opens under it.
pub fn framed_combo(ui: &mut Ui, tk: &Tokens, salt: impl std::hash::Hash + std::fmt::Debug, width: f32, selected: &str, list: impl FnOnce(&mut Ui)) {
    let (input, line, accent, text, muted) = (tk.input, tk.input_line, tk.accent, tk.text, tk.muted);
    ui.scope(|ui| {
        let sp = ui.spacing_mut();
        sp.button_padding = Vec2::new(6.0, 4.0);
        sp.interact_size.y = 24.0;
        let v = ui.visuals_mut();
        let rr = CornerRadius::same(tk.radius_sm as u8);
        for (w, stroke) in [(&mut v.widgets.inactive, line), (&mut v.widgets.hovered, line), (&mut v.widgets.active, accent), (&mut v.widgets.open, accent)] {
            w.weak_bg_fill = input;
            w.bg_fill = input;
            w.bg_stroke = Stroke::new(1.0, stroke);
            w.corner_radius = rr;
            w.expansion = 0.0;
        }
        egui::ComboBox::from_id_salt(salt)
            .width(width)
            .truncate()
            .selected_text(RichText::new(selected).size(12.0).color(text))
            .icon(move |ui, rect, _vis, open| {
                // a small chevron at the right edge, like the browser's select arrow
                let c = rect.center();
                let d = if open { -1.0 } else { 1.0 };
                let st = Stroke::new(1.4, muted);
                ui.painter().line_segment([c + Vec2::new(-3.5, -1.5 * d), c + Vec2::new(0.0, 1.5 * d)], st);
                ui.painter().line_segment([c + Vec2::new(0.0, 1.5 * d), c + Vec2::new(3.5, -1.5 * d)], st);
            })
            .show_ui(ui, list);
    });
}

/// `input[type=checkbox]`: a 14 px box, accent-filled with a white tick when on.
pub fn check_box(ui: &mut Ui, tk: &Tokens, on: &mut bool) -> egui::Response {
    let (r, mut resp) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let b = Rect::from_center_size(r.center(), Vec2::splat(14.0));
    let rr = CornerRadius::same(3);
    if *on {
        ui.painter().rect_filled(b, rr, tk.accent);
        let c = b.center();
        let st = Stroke::new(1.8, Color32::WHITE);
        ui.painter().line_segment([c + Vec2::new(-3.5, 0.0), c + Vec2::new(-1.0, 2.8)], st);
        ui.painter().line_segment([c + Vec2::new(-1.0, 2.8), c + Vec2::new(3.8, -2.8)], st);
    } else {
        ui.painter().rect(b, rr, tk.input, Stroke::new(1.0, if resp.hovered() { tk.accent } else { tk.input_line }), egui::StrokeKind::Inside);
    }
    let v = *on;
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, v, ""));
    resp
}

/// `.seg`: one bordered strip of buttons that share the width by their text. `Some(key)` when a
/// button was clicked.
pub fn seg_strip(ui: &mut Ui, tk: &Tokens, id: egui::Id, w: f32, pairs: &[(String, String)], cur: &str) -> Option<String> {
    let font = egui::FontId::proportional(11.0);
    let nat: Vec<f32> = pairs.iter().map(|(_, l)| ui.fonts_mut(|f| f.layout_no_wrap(l.clone(), font.clone(), Color32::WHITE).size().x) + 7.0).collect();
    let extra = ((w - 2.0 - nat.iter().sum::<f32>()) / pairs.len().max(1) as f32).max(0.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 24.0), Sense::hover());
    let rr = CornerRadius::same(tk.radius_sm as u8);
    ui.painter().rect_filled(rect, rr, tk.input);
    let mut x = rect.min.x + 1.0;
    let mut pick = None;
    for (i, (k, l)) in pairs.iter().enumerate() {
        let bw = nat[i] + extra;
        let br = Rect::from_min_size(egui::pos2(x, rect.min.y + 1.0), Vec2::new(bw, 22.0));
        let resp = ui.interact(br, id.with(k.as_str()), Sense::click());
        let on = *k == cur;
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, l.clone()));
        let fill = if on { Some(tk.sel) } else if resp.hovered() { Some(tk.hover) } else { None };
        if let Some(c) = fill {
            ui.painter().rect_filled(br, 0.0, c);
        }
        ui.painter().text(br.center(), egui::Align2::CENTER_CENTER, l, font.clone(), if on { tk.sel_text } else { tk.text });
        if i + 1 < pairs.len() {
            ui.painter().vline(br.max.x, br.y_range(), Stroke::new(1.0, tk.input_line));
        }
        if resp.clicked() {
            pick = Some(k.clone());
        }
        x += bw;
    }
    ui.painter().rect_stroke(rect, rr, Stroke::new(1.0, tk.input_line), egui::StrokeKind::Inside);
    pick
}

/// A bordered text box in the form's style (`.field input`). Returns the response.
pub fn input_box(ui: &mut Ui, tk: &Tokens, buf: &mut String, id: egui::Id, width: f32, font: egui::FontId, colour: Color32) -> egui::Response {
    let focused = ui.memory(|m| m.has_focus(id));
    ui.scope(|ui| {
        let v = ui.visuals_mut();
        v.extreme_bg_color = tk.input;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, tk.input_line);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, tk.input_line);
        v.widgets.active.bg_stroke = Stroke::new(1.0, tk.accent);
        v.selection.stroke = Stroke::new(1.0, tk.accent);
        v.override_text_color = Some(colour);
        let resp = ui.add_sized(
            [width, 24.0],
            egui::TextEdit::singleline(buf).id(id).font(font).margin(egui::Margin::symmetric(5, 4)).text_color(colour).align(egui::Align2::LEFT_CENTER),
        );
        if focused {
            // :focus { border-color: accent; box-shadow: 0 0 0 2px accent-soft }
            let r = resp.rect;
            ui.painter().rect_stroke(r.expand(1.0), CornerRadius::same(tk.radius_sm as u8 + 1), Stroke::new(2.0, tk.accent_soft), egui::StrokeKind::Outside);
            ui.painter().rect_stroke(r, CornerRadius::same(tk.radius_sm as u8), Stroke::new(1.0, tk.accent), egui::StrokeKind::Inside);
        }
        resp
    })
    .inner
}

/// A text box over `shown`: a change fires when the box loses focus (1.x's `change` event, which
/// is also what Enter does). Returns the typed text when it should be taken.
pub fn text_commit(ui: &mut Ui, tk: &Tokens, bufs: &mut Bufs, key: &str, shown: &str, width: f32, font: egui::FontId, colour: Color32, tip: Option<String>) -> Option<String> {
    let id = egui::Id::unique(("tool-field", key));
    let focused = ui.memory(|m| m.has_focus(id));
    let buf = bufs.entry(key.to_owned()).or_default();
    if !focused {
        buf.clear();
        buf.push_str(shown);
    }
    let resp = input_box(ui, tk, buf, id, width, font, colour);
    let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    let out = (resp.lost_focus() && (enter || buf.as_str() != shown)).then(|| buf.clone());
    if let Some(t) = tip {
        resp.on_hover_text(t);
    }
    out
}

/// A small muted unit label (`.field .unit`: 11 px, 22 px wide).
pub fn unit_label(ui: &mut Ui, tk: &Tokens, text: &str) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(22.0, 16.0), Sense::hover());
    ui.painter().text(Rect::from_min_size(r.min, Vec2::new(22.0, 16.0)).left_center(), egui::Align2::LEFT_CENTER, text, egui::FontId::proportional(11.0), tk.muted);
}

/// A pick-kind field (Task 3's `bodies::show_pick` draws the control). The label column is drawn
/// here, as `fieldHTML` does; the `features` checklist is a wide field that draws its own label.
/// `None` when the field is not one of the ten pick kinds.
pub fn pick_field(ui: &mut Ui, env: &FieldEnv, f: &Field, values: &mut Values, active: &mut Option<String>, ctx: &Ctx, edit_id: Option<&str>) -> Option<bodies::ShowOut> {
    use crate::tools::FieldKind as K;
    if !matches!(
        f.kind,
        K::Bodies { .. }
            | K::Plane { .. }
            | K::Sketch
            | K::Regions
            | K::Features
            | K::FacePoints
            | K::PFace { .. }
            | K::CylFace
            | K::Points3
            | K::AxisLine
    ) {
        return None;
    }
    if matches!(f.kind, K::Features) {
        return bodies::show_pick(ui, env.tk, f, values, active, ctx, edit_id);
    }
    row(ui, env, f, false, |ui, _| bodies::show_pick(ui, env.tk, f, values, active, ctx, edit_id))
}
