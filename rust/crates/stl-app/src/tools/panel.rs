//! The floating tool panel (1.x `#tool-panel`, app.js `openTool` / `renderForm` / `fieldHTML` /
//! `onFormChange` / `setValue` / `params`) and the state of the tool that is open.
//!
//! [`OpenTool`] holds everything about the open tool and has no link to the engine: the app
//! (`app.rs`) sends the previews and the commits. Task 3 (pick fields), Task 4 (the gizmo writes the
//! Move / Scale fields) and Task 5 (Support Fins tools built at run time) use the pub methods:
//! [`OpenTool::set_value`], [`OpenTool::set_values`], `active_field`, `ok_label` and so on.

use std::collections::BTreeMap;

use egui::{Align, Align2, Area, CornerRadius, Frame, Id, Layout, Margin, Order, Rect, RichText, Sense, Stroke, Ui, Vec2};
use serde_json::{Map, Value};
use stl_engine::params::resolve_deep;

use super::fields::bool::truthy;
use super::fields::number::{expr_title, is_expr, num_value, parse_num};
use super::fields::{self, Bufs, FieldEnv, FORM_W};
use super::preview::{MsgKind, PreviewState, ToolMsg};
use super::{Ctx, Field, FieldKind, ToolDef, Values};
use crate::theme::Tokens;
use crate::ui::dialogs::button;

/// `#tool-panel { width: 290px; right: 8px; top: 8px }`.
pub const PANEL_W: f32 = 290.0;
pub const INSET: f32 = 8.0;
/// The view cube moves this far from the right edge while a tool is open (`right: 306px`, less the
/// 8 px the cube's own inset already has).
pub const VIEWCUBE_INSET: f32 = 298.0;

/// What a drawn panel asks the app to do.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct PanelOut {
    pub ok: bool,
    pub cancel: bool,
    /// The Preview button of a `noLivePreview` tool (`runPreview()` at once).
    pub preview_now: bool,
    /// The `"face"` highlight should go (a plane or face was removed).
    pub clear_face: bool,
    /// A value changed this frame (the preview is scheduled already).
    pub changed: bool,
    /// What the Support Fins form asked for this frame (Suggest, Fill gaps, material, ...).
    pub sf_events: Vec<crate::supports::PanelEvent>,
}

/// What the panel needs from the app to draw.
pub struct PanelEnv<'a> {
    pub tk: &'a Tokens,
    pub inch: bool,
    /// The project's parameters that have a value (`paramValues()`).
    pub params: &'a BTreeMap<String, f64>,
    pub ctx: &'a Ctx<'a>,
    /// The workspace (the viewport's rectangle).
    pub ws: Rect,
}

/// The tool that is open (`this.tool` in app.js).
pub struct OpenTool {
    pub def: &'static ToolDef,
    pub values: Values,
    /// The step being edited (`editId`).
    pub edit_id: Option<String>,
    /// The pick field a viewport click fills (`activeField`).
    pub active_field: Option<String>,
    /// The rollback marker before the tool opened, put back when it closes (`prevRollback`).
    pub prev_rollback: Option<usize>,
    /// The line under the form (`#tool-msg`).
    pub msg: Option<ToolMsg>,
    pub preview: PreviewState,
    /// The record the last preview came with (`pack.result`): Support Fins reads `paint`, `walls`,
    /// `pieces`, `support` here.
    pub last_result: Value,
    /// The gizmo is being dragged: a late preview must not snap the part back (`gizmo.tc.dragging`).
    pub dragging: bool,
    /// The OK and Cancel labels (`#tool-ok`): Support Fins and the Inspect tools change them.
    pub ok_label: String,
    pub cancel_label: String,
    /// The credit at the left of the buttons, with its hover text.
    pub credit: Option<(String, String)>,
    /// The keys `set_value` changed since the app last looked (`take_changed`).
    changed: Vec<String>,
    /// What the user is typing in the number and text boxes.
    pub(crate) bufs: Bufs,
    /// `t._lastSize`: the Scale tool's last size, to see which axis the user edited.
    last_size: Option<Vec<f64>>,
    /// What the viewport marks were last drawn for (the app redraws them when it changes).
    pub marks_sig: String,
    /// Support Fins and Draw Walls: the form is drawn by `supports::show` from this session.
    pub sf: Option<crate::supports::Session>,
    /// Draw Walls' hint line for the shape picked (the panel passes it to the Support Fins form).
    pub wall_hint: String,
    /// Move / Rotate / Scale: letting go of a handle added or edited the step already
    /// (`t.accepted`): the buttons say Done / Close.
    pub accepted: bool,
    /// A drop added the step and the timeline hasn't shown it yet: the ids that were there before.
    pub accepting: Option<Vec<String>>,
    /// A second drop came while the first was still being added (`_acceptAgain`).
    pub accept_again: bool,
    /// The newest preview arrived while a handle was held and was dropped: it is asked for again
    /// when the handle is let go, unless the drop brings a preview of its own.
    pub preview_dropped: bool,
    /// OK was pressed while a handle's Add was still on its way: OK goes out once that step is
    /// in (as an edit of it), instead of adding a second step.
    pub ok_after_add: bool,
}

/// A `bodies` field with a `placeholder` ("Auto", "Auto (bodies it touches)", "The part you draw
/// on") may stay empty: 1.x `validTool` skips it (`!f.placeholder`).
pub fn is_optional_bodies(f: &Field) -> bool {
    f.placeholder.is_some()
}

/// 1.x `isEmptySel`.
pub fn is_empty_sel(v: Option<&Value>) -> bool {
    fields::bodies::is_empty_sel(v)
}

fn arr_len(v: Option<&Value>) -> usize {
    v.and_then(Value::as_array).map_or(0, Vec::len)
}

impl OpenTool {
    pub fn new(def: &'static ToolDef, values: Values, edit_id: Option<String>, prev_rollback: Option<usize>) -> OpenTool {
        let active_field = fields::bodies::first_active(def, &values);
        OpenTool {
            def,
            values,
            edit_id,
            active_field,
            prev_rollback,
            msg: None,
            preview: PreviewState::default(),
            last_result: Value::Null,
            dragging: false,
            ok_label: "OK".into(),
            cancel_label: "Cancel".into(),
            credit: None,
            changed: Vec::new(),
            bufs: Bufs::new(),
            last_size: None,
            marks_sig: String::new(),
            sf: None,
            wall_hint: String::new(),
            accepted: false,
            accepting: None,
            accept_again: false,
            preview_dropped: false,
            ok_after_add: false,
        }
    }

    /// The panel's title: `Edit ` before the name when a step is edited.
    pub fn title(&self) -> String {
        format!("{}{}", if self.edit_id.is_some() { "Edit " } else { "" }, self.def.label)
    }

    /// `setValue(key, val)`: store it, run `onChange` and schedule a preview.
    pub fn set_value(&mut self, key: &str, val: Value, ctx: &Ctx) {
        self.values.insert(key.to_owned(), val);
        self.after_set(key, ctx);
        self.preview.schedule(super::preview::DELAY_MS, self.def.flags.no_live_preview);
    }

    /// `setValue` without `onChange` (the wall editor, the keys W / R and Support Fins' picks).
    pub fn set_value_plain(&mut self, key: &str, val: Value) {
        self.values.insert(key.to_owned(), val);
        self.changed.push(key.to_owned());
        self.preview.schedule(super::preview::DELAY_MS, self.def.flags.no_live_preview);
    }

    /// Several values at once (the gizmo writes `translate`, `rotate` or `size`): `onChange` runs for
    /// each key, then one preview is scheduled.
    pub fn set_values(&mut self, patch: Values, ctx: &Ctx) {
        for (k, v) in patch {
            self.values.insert(k.clone(), v);
            self.after_set(&k, ctx);
        }
        self.preview.schedule(super::preview::DELAY_MS, self.def.flags.no_live_preview);
    }

    fn after_set(&mut self, key: &str, ctx: &Ctx) {
        self.changed.push(key.to_owned());
        if let Some(oc) = self.def.on_change {
            if let Some(patch) = oc(key, &self.values, ctx) {
                for (k, v) in patch {
                    self.values.insert(k, v);
                }
            }
        }
        if self.def.key == "scale" && key == "size" {
            self.keep_proportions();
        }
    }

    /// Scale: with "uniform" on, an edit of one axis scales the others from `_size0` (app.js:826).
    fn keep_proportions(&mut self) {
        let nums = |v: Option<&Value>| -> Option<Vec<f64>> { v.and_then(Value::as_array).map(|a| a.iter().map(|x| x.as_f64().unwrap_or(f64::NAN)).collect()) };
        let Some(val) = nums(self.values.get("size")) else { return };
        let uniform = self.values.get("uniform").is_some_and(truthy);
        let Some(s0) = nums(self.values.get("_size0")) else { return };
        if !uniform {
            return;
        }
        let prev = self.last_size.clone().unwrap_or_else(|| s0.clone());
        if let Some(i) = val.iter().enumerate().position(|(j, x)| (x - prev.get(j).copied().unwrap_or(f64::NAN)).abs() > 1e-9) {
            if s0.get(i).copied().unwrap_or(0.0) > 0.0 {
                let k = val[i] / s0[i];
                let size: Vec<Value> = s0.iter().map(|x| num_value((x * k * 1e4).round() / 1e4)).collect();
                self.values.insert("size".into(), Value::Array(size));
            }
        }
        self.last_size = nums(self.values.get("size"));
    }

    /// The keys `set_value` / `set_values` changed since the last call (Support Fins watches
    /// `_mode`, `_load`, `body`, `sf:threshold`; Task 4 watches `_gizmo`).
    pub fn take_changed(&mut self) -> Vec<String> {
        std::mem::take(&mut self.changed)
    }

    /// `schedulePreview(delay)`.
    pub fn schedule_preview(&mut self, delay_ms: u64) {
        self.preview.schedule(delay_ms, self.def.flags.no_live_preview);
    }

    /// The fields that show for the current values.
    pub fn shown_fields(&self) -> impl Iterator<Item = &'static Field> + '_ {
        let v = &self.values;
        self.def.fields.iter().filter(move |f| f.show.is_none_or(|s| s(v)))
    }

    /// `validTool()`: everything that has to be picked is picked.
    pub fn valid(&self) -> bool {
        let v = &self.values;
        for f in self.shown_fields() {
            let val = v.get(f.key);
            let ok = match f.kind {
                FieldKind::Bodies { .. } => is_optional_bodies(f) || !is_empty_sel(val),
                FieldKind::Plane { .. } | FieldKind::Sketch => val.is_some_and(truthy),
                FieldKind::FacePoints => arr_len(val) > 0,
                FieldKind::Points3 => arr_len(val) == 3,
                FieldKind::Features => arr_len(val) > 0,
                FieldKind::PFace { .. } | FieldKind::CylFace => val.is_some_and(truthy),
                _ => true,
            };
            if !ok {
                return false;
            }
        }
        true
    }

    /// `params()`: the values as the engine gets them. Plain numbers are read (`parseNum`),
    /// expressions stay strings, `toParams` shapes them, and keys starting with `_` go.
    pub fn params(&self, ctx: &Ctx) -> Value {
        let mut v = self.values.clone();
        for f in &self.def.fields {
            if matches!(f.kind, FieldKind::Number { .. }) {
                if let Some(x) = v.get(f.key) {
                    if !is_expr(x) {
                        let n = parse_num(x);
                        v.insert(f.key.to_owned(), num_value(n));
                    }
                }
            }
        }
        let mut p = (self.def.to_params)(&v, ctx);
        if let Value::Object(m) = &mut p {
            m.retain(|k, _| !k.starts_with('_'));
        }
        p
    }

    /// An expression in the form that can't be worked out, with 1.x's wording (`resolveDeep`).
    /// OK waits for it to be fixed.
    pub fn expr_error(&self, ctx: &Ctx, params: &BTreeMap<String, f64>) -> Option<String> {
        let p = self.params(ctx);
        resolve_deep(&p, params).err().map(|e| e.0)
    }

    /// `fieldChanged` helper for the Preview button's tooltip is not needed; this is the tooltip of
    /// an expression box (`exprTitle`).
    pub fn expr_tip(x: &str, is_len: bool, params: &BTreeMap<String, f64>) -> String {
        expr_title(x, is_len, params)
    }

    // ------------------------------------------------------------------ drawing

    /// Draw the panel over the workspace (right edge, 8 px in). Returns what was clicked.
    pub fn show(&mut self, egui_ctx: &egui::Context, env: &PanelEnv) -> PanelOut {
        let tk = env.tk;
        let mut out = PanelOut::default();
        let pos = egui::pos2(env.ws.max.x - INSET - PANEL_W, env.ws.min.y + INSET);
        let max_h = (env.ws.height() - 60.0).max(120.0);
        Area::new(Id::unique("tool-panel")).order(Order::Middle).fixed_pos(pos).show(egui_ctx, |ui| {
            Frame::new().fill(tk.panel).stroke(Stroke::new(1.0, tk.line)).corner_radius(CornerRadius::same(tk.radius as u8)).shadow(tk.shadow).show(ui, |ui| {
                ui.set_width(PANEL_W - 2.0);
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
                self.title_row(ui, egui_ctx, tk, &mut out);
                let msg_h = self.msg.as_ref().map_or(0.0, |_| 40.0);
                let form_h = (max_h - 36.0 - 46.0 - msg_h - 2.0).max(60.0);
                egui::ScrollArea::vertical().max_height(form_h).auto_shrink([false, true]).id_salt("tool-form").show(ui, |ui| {
                    // `#tool-form { padding: 8px 10px }`, then the first field's own 6 px margin
                    Frame::new().inner_margin(Margin { left: 10, right: 10, top: 8, bottom: 8 }).show(ui, |ui| {
                        ui.set_width(FORM_W);
                        self.form(ui, env, &mut out);
                    });
                });
                if let Some(m) = &self.msg {
                    Frame::new().inner_margin(Margin { left: 10, right: 10, top: 0, bottom: 0 }).show(ui, |ui| {
                        ui.set_width(FORM_W);
                        message(ui, tk, m);
                        ui.add_space(6.0);
                    });
                }
                self.actions(ui, tk, &mut out);
            });
        });
        out
    }

    fn title_row(&mut self, ui: &mut Ui, ctx: &egui::Context, tk: &Tokens, out: &mut PanelOut) {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(PANEL_W - 2.0, 33.0), Sense::hover());
        let mut x = rect.min.x + 10.0;
        let cy = rect.center().y;
        if !self.def.icon.is_empty() {
            let img = crate::icons::icon(ctx, self.def.icon, 17.0, tk.accent);
            egui::Image::new(img).paint_at(ui, Rect::from_center_size(egui::pos2(x + 8.5, cy), Vec2::splat(17.0)));
            x += 17.0 + 7.0;
        }
        let font = egui::FontId::new(12.0, egui::FontFamily::Name("bold".into()));
        let close = Rect::from_center_size(egui::pos2(rect.max.x - 8.0 - 11.0, cy), Vec2::splat(22.0));
        let title = self.title();
        let galley = ui.painter().layout(title.clone(), font, tk.strong, (close.min.x - 7.0 - x).max(10.0));
        ui.painter().galley(egui::pos2(x, cy - galley.size().y / 2.0), galley, tk.strong);
        let resp = ui.interact(close, Id::unique("tool-close"), Sense::click());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Close"));
        if resp.hovered() {
            ui.painter().rect_filled(close, CornerRadius::same(4), tk.hover);
        }
        ui.painter().text(close.center(), Align2::CENTER_CENTER, "×", egui::FontId::proportional(17.0), if resp.hovered() { tk.strong } else { tk.muted });
        resp.on_hover_text("Cancel (Esc)");
        if ui.interact(close, Id::unique("tool-close"), Sense::click()).clicked() {
            out.cancel = true;
        }
        ui.painter().hline(rect.x_range(), rect.max.y, Stroke::new(1.0, tk.divider));
    }

    /// Support Fins and Draw Walls: Support Fins' own settings, drawn by `supports::show`.
    fn sf_form(&mut self, ui: &mut Ui, env: &PanelEnv, out: &mut PanelOut) {
        let Some(mut sess) = self.sf.take() else { return };
        ui.add_space(6.0);
        // what the viewport picks, the gizmo and the walls wrote since the last frame
        sess.set_values(self.values.clone());
        sess.body_names = env.ctx.bodies.iter().map(|b| (b.id.clone(), b.name.clone())).collect();
        sess.body_active = self.active_field.as_deref() == Some("body");
        let was_active = sess.body_active;
        let events = crate::supports::show(ui, env.tk, &mut sess, &self.wall_hint);
        if sess.body_active != was_active {
            self.active_field = sess.body_active.then(|| "body".to_owned());
        }
        if events.iter().any(|e| matches!(e, crate::supports::PanelEvent::Edited(_))) {
            for e in &events {
                if let crate::supports::PanelEvent::Edited(k) = e {
                    self.changed.push(k.clone());
                }
            }
            self.values = sess.values.clone();
            self.preview.schedule(super::preview::DELAY_MS, self.def.flags.no_live_preview);
            out.changed = true;
        }
        out.sf_events = events;
        self.sf = Some(sess);
    }

    fn form(&mut self, ui: &mut Ui, env: &PanelEnv, out: &mut PanelOut) {
        if self.sf.is_some() {
            return self.sf_form(ui, env, out);
        }
        let tk = env.tk;
        let fenv = FieldEnv { tk, inch: env.inch, params: env.params };
        let def = self.def;
        let mut edits: Vec<fields::Edit> = Vec::new();
        ui.add_space(6.0);
        for f in def.fields.iter() {
            if let Some(s) = f.show {
                if !s(&self.values) {
                    continue;
                }
            }
            let val = self.values.get(f.key).cloned().unwrap_or(Value::Null);
            let edit = match &f.kind {
                FieldKind::Number { unit } => fields::number::show(ui, &fenv, f, *unit, &val, &mut self.bufs),
                FieldKind::Select { options, grouped } => fields::select::show(ui, &fenv, f, options, *grouped, &val),
                FieldKind::Bool => fields::bool::show(ui, &fenv, f, &val),
                FieldKind::Vec3 { unit } => fields::vec3::show(ui, &fenv, f, *unit, &val, &mut self.bufs),
                FieldKind::Text { placeholder } => fields::text::show(ui, &fenv, f, placeholder.as_deref(), &val, &mut self.bufs),
                _ => {
                    let before = self.values.clone();
                    if let Some(o) = fields::pick_field(ui, &fenv, f, &mut self.values, &mut self.active_field, env.ctx, self.edit_id.as_deref()) {
                        if o.changed || before != self.values {
                            self.preview.schedule(super::preview::DELAY_MS, def.flags.no_live_preview);
                            out.changed = true;
                        }
                        out.clear_face |= o.clear_face;
                    }
                    None
                }
            };
            if let Some(e) = edit {
                edits.push(e);
            }
            ui.add_space(6.0);
        }
        if def.extra == Some("dropToBed") {
            ui.horizontal_top(|ui| {
                ui.add_space(fields::LABEL_W + fields::GAP);
                // `.ctl` shrinks its buttons to the column and wraps their text
                let bw = (fields::CTL_W - 4.0) / 2.0;
                if ctl_button_w(ui, tk, "Drop to bed", bw).clicked() {
                    if let Some(tr) = drop_to_bed(&self.values, env.ctx) {
                        edits.push(fields::Edit { key: "translate".into(), value: tr });
                    }
                }
                ui.add_space(4.0);
                if ctl_button_w(ui, tk, "Centre on origin", bw).clicked() {
                    if let Some(tr) = centre_on_origin(&self.values, env.ctx) {
                        edits.push(fields::Edit { key: "translate".into(), value: tr });
                    }
                }
            });
            ui.add_space(6.0);
        }
        if def.flags.no_live_preview {
            ui.horizontal(|ui| {
                ui.add_space(fields::LABEL_W + fields::GAP);
                if ctl_button(ui, tk, "Preview", true).clicked() {
                    out.preview_now = true;
                }
            });
            ui.add_space(6.0);
        }
        if def.flags.sculpt {
            let strokes = self.values.get("strokes").and_then(Value::as_array).cloned().unwrap_or_default();
            let body = self.values.get("body").map(|b| match b {
                Value::Array(a) => a.first().cloned().unwrap_or(Value::Null),
                o => o.clone(),
            });
            let name = body.as_ref().and_then(Value::as_str).filter(|s| !s.is_empty()).map(|id| fields::bodies::body_name(env.ctx, id)).unwrap_or_else(|| "—".to_owned());
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(RichText::new("Strokes").size(12.0).color(tk.label)));
                ui.add_space(fields::LABEL_W - 40.0 + fields::GAP);
                ui.label(RichText::new(format!("{} on {}", strokes.len(), name)).size(12.0).color(tk.text));
                ui.add_space(4.0);
                if ctl_button(ui, tk, "Undo stroke", !strokes.is_empty()).clicked() {
                    let mut s = strokes.clone();
                    s.pop();
                    edits.push(fields::Edit { key: "strokes".into(), value: Value::Array(s) });
                }
            });
        }
        if !edits.is_empty() {
            out.changed = true;
        }
        for e in edits {
            self.set_value(&e.key, e.value, env.ctx);
        }
    }

    fn actions(&mut self, ui: &mut Ui, tk: &Tokens, out: &mut PanelOut) {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(PANEL_W - 2.0, 41.0), Sense::hover());
        ui.painter().hline(rect.x_range(), rect.min.y, Stroke::new(1.0, tk.divider));
        let inner = Rect::from_min_max(egui::pos2(rect.min.x + 10.0, rect.min.y + 8.0), egui::pos2(rect.max.x - 10.0, rect.max.y - 9.0));
        let mut right = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(Layout::right_to_left(Align::Center)));
        right.spacing_mut().item_spacing.x = 8.0;
        if button(&mut right, tk, &self.ok_label, true).clicked() {
            out.ok = true;
        }
        if button(&mut right, tk, &self.cancel_label, false).clicked() {
            out.cancel = true;
        }
        if let Some((text, tip)) = &self.credit {
            // `#tool-credit`: wraps in the room the buttons leave (they are laid out first)
            let room = (right.min_rect().min.x - 10.0 - inner.min.x).max(40.0);
            let g = ui.painter().layout(text.clone(), egui::FontId::proportional(10.5), tk.muted, room);
            let r = Rect::from_min_size(egui::pos2(inner.min.x, inner.center().y - g.size().y / 2.0), g.size());
            ui.painter().galley(r.min, g, tk.muted);
            ui.interact(r, Id::unique("tool-credit"), Sense::hover()).on_hover_text(tip);
        }
    }
}

/// `#tool-msg .msg`: a coloured box, 12 px text (a spinner in front of a busy note).
fn message(ui: &mut Ui, tk: &Tokens, m: &ToolMsg) {
    let (bg, fg) = match m.kind {
        MsgKind::Err => (tk.err_bg, tk.err_text),
        MsgKind::Warn => (tk.warn_bg, tk.warn_text),
        MsgKind::Info | MsgKind::Busy => (tk.info_bg, tk.info_text),
    };
    Frame::new().fill(bg).corner_radius(CornerRadius::same(tk.radius_sm as u8)).inner_margin(Margin::symmetric(8, 6)).show(ui, |ui| {
        ui.set_width(FORM_W - 16.0);
        ui.horizontal_wrapped(|ui| {
            if m.kind == MsgKind::Busy {
                ui.add(egui::Spinner::new().size(12.0).color(tk.accent));
                ui.add_space(8.0);
            }
            ui.add(egui::Label::new(RichText::new(&m.text).size(12.0).color(fg)).wrap());
        });
    });
}

/// `.field .ctl button`: a small bordered button inside a form row.
pub fn ctl_button(ui: &mut Ui, tk: &Tokens, text: &str, enabled: bool) -> egui::Response {
    ui.scope(|ui| {
        let v = ui.visuals_mut();
        v.widgets.inactive.weak_bg_fill = tk.input;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, tk.input_line);
        v.widgets.hovered.weak_bg_fill = tk.input;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, tk.accent);
        ui.add_enabled(enabled, egui::Button::new(RichText::new(text).size(12.0).color(tk.text)).corner_radius(CornerRadius::same(tk.radius_sm as u8)).min_size(Vec2::new(0.0, 24.0)))
    })
    .inner
}

/// [`ctl_button`] of a given width, its text wrapping inside it.
pub fn ctl_button_w(ui: &mut Ui, tk: &Tokens, text: &str, width: f32) -> egui::Response {
    ui.scope(|ui| {
        ui.set_max_width(width);
        let v = ui.visuals_mut();
        v.widgets.inactive.weak_bg_fill = tk.input;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, tk.input_line);
        v.widgets.hovered.weak_bg_fill = tk.input;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, tk.accent);
        ui.add(egui::Button::new(RichText::new(text).size(12.0).color(tk.text)).wrap().corner_radius(CornerRadius::same(tk.radius_sm as u8)).min_size(Vec2::new(width, 24.0)))
    })
    .inner
}

// ------------------------------------------------------------------ Move: Drop to bed / Centre on origin

/// A body's box from its mesh (the engine's `bbox`).
fn bbox(b: &crate::engine_link::BodyView) -> Option<([f64; 3], [f64; 3])> {
    let p = &b.mesh.positions;
    if p.len() < 3 {
        return None;
    }
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for c in p.chunks_exact(3) {
        for i in 0..3 {
            lo[i] = lo[i].min(f64::from(c[i]));
            hi[i] = hi[i].max(f64::from(c[i]));
        }
    }
    Some((lo, hi))
}

fn picked_boxes(values: &Values, ctx: &Ctx) -> Vec<([f64; 3], [f64; 3])> {
    let ids: Vec<&str> = values.get("bodies").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    ctx.bodies.iter().filter(|b| ids.contains(&b.id.as_str())).filter_map(bbox).collect()
}

fn r4(x: f64) -> f64 {
    let r = (x * 1e4).round() / 1e4;
    if r == 0.0 { 0.0 } else { r }
}

fn drop_to_bed(values: &Values, ctx: &Ctx) -> Option<Value> {
    let bs = picked_boxes(values, ctx);
    if bs.is_empty() {
        return None;
    }
    let landed = values.get("bed").is_some_and(truthy)
        && values.get("rotate").and_then(Value::as_array).is_some_and(|a| a.iter().any(|x| parse_num(x).abs() > 1e-9));
    let zmin = bs.iter().map(|b| b.0[2]).fold(f64::INFINITY, f64::min);
    let mut tr: Vec<Value> = values.get("translate").and_then(Value::as_array).cloned().unwrap_or_else(|| vec![Value::from(0); 3]);
    tr.resize(3, Value::from(0));
    tr[2] = num_value(if landed { 0.0 } else { r4(-zmin) });
    Some(Value::Array(tr))
}

fn centre_on_origin(values: &Values, ctx: &Ctx) -> Option<Value> {
    let bs = picked_boxes(values, ctx);
    if bs.is_empty() {
        return None;
    }
    let lo: Vec<f64> = (0..3).map(|i| bs.iter().map(|b| b.0[i]).fold(f64::INFINITY, f64::min)).collect();
    let hi: Vec<f64> = (0..3).map(|i| bs.iter().map(|b| b.1[i]).fold(f64::NEG_INFINITY, f64::max)).collect();
    Some(Value::Array(vec![num_value(r4(-(lo[0] + hi[0]) / 2.0)), num_value(r4(-(lo[1] + hi[1]) / 2.0)), num_value(r4(-lo[2]))]))
}

/// `toolKeyFor(f)` (app.js:2842): the tool that edits a timeline step.
pub fn tool_key_for(kind: &str, params: &Value) -> Option<String> {
    match kind {
        "primitive" => Some(format!("primitive_{}", params.get("shape").and_then(Value::as_str).unwrap_or("undefined"))),
        "plane" => {
            let mode = params.get("mode").and_then(Value::as_str)?;
            Some(
                match mode {
                    "offset" | "face" => "plane_offset",
                    "angle" => "plane_angle",
                    "three_points" => "plane_3pt",
                    "midplane" => "plane_mid",
                    _ => return None,
                }
                .to_owned(),
            )
        }
        "supportfins" | "supportwall" => Some(kind.to_owned()),
        k => super::tool(k).map(|t| t.key.to_owned()),
    }
}

/// An empty values map.
pub fn no_values() -> Map<String, Value> {
    Map::new()
}
