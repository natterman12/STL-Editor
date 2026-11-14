//! The sketch panel (`app.js` `openSketchPanel`, `renderSketchPanel`) and the dimension box
//! (`sketcher.js` `openModify`). Fields, rows and the definition line carry AccessKit labels, so
//! tests find them by name.

use std::collections::{BTreeMap, HashMap};

use egui::{Align, Area, Color32, CornerRadius, Frame, Id, Key, Layout, Margin, Order, Rect, RichText, Sense, Stroke, Ui, Vec2, WidgetInfo};
use serde_json::{json, Value};

use super::geom::{describe, label as entity_label};
use super::session::SketchMode;
use super::solve::Def;
use super::{dims, Entity, SketchEffect, SketchSession};
use crate::theme::Tokens;
use crate::tools::fields::number::{fmt_len, parse_len, parse_num_text, to_disp, unit_name};
use crate::tools::fields::{FORM_W, GAP, LABEL_W};
use crate::tools::panel::{ctl_button, INSET, PANEL_W};

/// What the panel remembers between frames.
#[derive(Default)]
pub struct PanelState {
    /// Text being typed in a field, by field key.
    bufs: HashMap<String, String>,
    /// The dimension box that is open (`openModify`): the constraint's index and the text.
    pub modify: Option<Modify>,
    /// Grid snap (`snapGrid`, `snapStep`).
    pub snap_grid: bool,
    pub snap_step: f64,
    init: bool,
}

pub struct Modify {
    pub idx: usize,
    pub text: String,
    pub fresh: bool,
}

impl PanelState {
    fn ensure(&mut self) {
        if !self.init {
            self.init = true;
            self.snap_grid = true;
            self.snap_step = 1.0;
        }
    }

    /// Set Grid snap (on entering a sketch, or when the units change) and the drawing tools' grid.
    pub fn set_snap(&mut self, on: bool, step: f64) {
        self.init = true;
        self.snap_grid = on;
        self.snap_step = step;
        super::snap::set_grid(on, step);
    }

    /// Change only the snap step (the units changed), keeping Grid snap on or off.
    pub fn set_snap_step(&mut self, step: f64) {
        self.ensure();
        let on = self.snap_grid;
        self.set_snap(on, step);
    }
}

/// What the panel asks for this frame.
#[derive(Default)]
pub struct PanelOut {
    pub finish: bool,
    pub cancel: bool,
    pub effects: Vec<SketchEffect>,
    /// A toast (level, text) the panel wants shown.
    pub toasts: Vec<(crate::engine_link::Level, String)>,
    /// "Edit…" on a selected text: its group and settings.
    pub edit_text: Option<(String, Value)>,
}

/// The title: "Edit Sketch" for a step being edited, else "Sketch".
pub fn title(s: &SketchSession) -> &'static str {
    if s.feature_id.is_some() { "Edit Sketch" } else { "Sketch" }
}

pub const FINISH_LABEL: &str = "Finish Sketch";

/// A one-line text field whose text is kept while it has focus; returns the text when it was
/// committed (Enter or leaving the field) and differs from `cur`.
fn text_field(ui: &mut Ui, tk: &Tokens, st: &mut PanelState, key: &str, label: &str, cur: &str, width: f32) -> Option<String> {
    let id = Id::unique(("skp", key));
    let focused = ui.memory(|m| m.has_focus(id));
    let mut buf = if focused { st.bufs.get(key).cloned().unwrap_or_else(|| cur.to_owned()) } else { cur.to_owned() };
    let r = crate::tools::fields::input_box(ui, tk, &mut buf, id, width, egui::FontId::proportional(12.0), tk.text);
    r.widget_info(|| WidgetInfo::labeled(egui::Role::TextInput, true, label));
    if r.has_focus() || r.changed() {
        st.bufs.insert(key.to_owned(), buf.clone());
    }
    if r.lost_focus() {
        st.bufs.remove(key);
        return (buf != cur).then_some(buf);
    }
    None
}

/// `.field`: a label column and a control.
fn row(ui: &mut Ui, tk: &Tokens, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = GAP;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(LABEL_W, 24.0), Sense::hover());
        ui.painter().text(egui::pos2(rect.min.x, rect.center().y), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(12.0), tk.label);
        add(ui);
    });
    ui.add_space(6.0);
}

fn section(ui: &mut Ui, tk: &Tokens, text: &str) {
    ui.add_space(12.0);
    ui.label(RichText::new(text.to_uppercase()).size(11.0).color(tk.muted));
    ui.add_space(4.0);
}

fn def_colour(tk: &Tokens, d: Def) -> Color32 {
    match d {
        Def::Under(_) => tk.def_under,
        Def::Full => tk.def_full,
        Def::Over => tk.def_over,
    }
}

/// The definition line (`#sk-def`): a dot and the text; red text when over defined.
fn def_line(ui: &mut Ui, tk: &Tokens, m: &SketchMode) {
    let text = super::solve::def_text(&m.state);
    if text.is_empty() {
        return;
    }
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(FORM_W, 16.0), Sense::hover());
    resp.widget_info(|| WidgetInfo::labeled(egui::Role::Label, true, text.clone()));
    let c = def_colour(tk, m.state.def);
    ui.painter().circle_filled(egui::pos2(rect.min.x + 4.5, rect.center().y), 4.5, c);
    let tc = if m.state.def == Def::Over { c } else { tk.text };
    ui.painter().text(egui::pos2(rect.min.x + 15.0, rect.center().y), egui::Align2::LEFT_CENTER, text, egui::FontId::proportional(12.0), tc);
    ui.add_space(8.0);
}

/// A relation's line in the list ("Distance: 20", "Parallel").
fn constraint_desc(s: &SketchSession, c: &super::Constraint, inch: bool) -> String {
    let label = dims::type_label(&c.kind);
    if !dims::is_dim(&c.kind) {
        return label.to_owned();
    }
    let ents = super::solve::all_ents(s);
    let val = if c.ref_.is_some() {
        format!("({})", dims::dim_text(c, &ents, inch, true))
    } else if c.kind.contains("angle") {
        format!("{}°", (c.value.unwrap_or(0.0) * 100.0).round() / 100.0)
    } else {
        format!("{} {}", fmt_len(c.value.unwrap_or(0.0), None, inch), unit_name(inch))
    };
    let expr = dims::expr_of_dim(c).map(|x| format!(" = {}", x.trim_start_matches('='))).unwrap_or_default();
    format!("{label}: {val}{expr}")
}

/// The fields of the selected shape: (key, label, is an angle or count).
fn entity_fields(e: &Entity) -> &'static [(&'static str, &'static str)] {
    match e {
        Entity::Rect(_) => &[("x", "Corner X"), ("y", "Corner Y"), ("w", "Width"), ("h", "Height"), ("fillet", "Corner radius"), ("angle", "Rotation °")],
        Entity::Circle(_) => &[("cx", "Centre X"), ("cy", "Centre Y"), ("d", "Diameter")],
        Entity::Polygon(_) => &[("cx", "Centre X"), ("cy", "Centre Y"), ("r", "Radius"), ("n", "Sides"), ("angle", "Rotation °")],
        Entity::Slot(_) => &[("x1", "Start X"), ("y1", "Start Y"), ("x2", "End X"), ("y2", "End Y"), ("w", "Width")],
        Entity::Line(_) => &[("p1x", "Start X"), ("p1y", "Start Y"), ("p2x", "End X"), ("p2y", "End Y"), ("len", "Length")],
        Entity::Arc(_) => &[("cx", "Centre X"), ("cy", "Centre Y"), ("r", "Radius"), ("a0", "Start angle °"), ("a1", "End angle °")],
        _ => &[],
    }
}

fn is_plain(k: &str) -> bool {
    matches!(k, "angle" | "a0" | "a1" | "n")
}

/// The number a field shows, in millimetres (angles and counts as they are).
fn field_value(e: &Entity, k: &str) -> f64 {
    let v = serde_json::to_value(e).unwrap_or(Value::Null);
    let n = |key: &str| v.get(key).and_then(Value::as_f64).unwrap_or(0.0);
    let pt = |key: &str, i: usize| v.get(key).and_then(Value::as_array).and_then(|a| a.get(i)).and_then(Value::as_f64).unwrap_or(0.0);
    match (k, e) {
        ("d", _) => n("r") * 2.0,
        ("w", Entity::Slot(_)) => n("r") * 2.0,
        ("p1x", _) => pt("p1", 0),
        ("p1y", _) => pt("p1", 1),
        ("p2x", _) => pt("p2", 0),
        ("p2y", _) => pt("p2", 1),
        ("len", _) => (pt("p2", 0) - pt("p1", 0)).hypot(pt("p2", 1) - pt("p1", 1)),
        _ => n(k),
    }
}

/// The patch `onFormChange` makes from a typed value (keys of the entity's JSON).
fn patch_for(e: &Entity, k: &str, v: f64) -> Value {
    let jv = serde_json::to_value(e).unwrap_or(Value::Null);
    let pt = |key: &str| jv.get(key).and_then(Value::as_array).map(|a| [a[0].as_f64().unwrap_or(0.0), a[1].as_f64().unwrap_or(0.0)]).unwrap_or([0.0; 2]);
    match (k, e) {
        ("d", _) => json!({ "r": v.abs() / 2.0 }),
        ("w", Entity::Slot(_)) => json!({ "r": v.abs() / 2.0 }),
        ("p1x" | "p1y" | "p2x" | "p2y", _) => {
            let key = &k[..2];
            let mut p = pt(key);
            p[if k.ends_with('x') { 0 } else { 1 }] = v;
            json!({ key: p })
        }
        ("len", _) => {
            let (p1, p2) = (pt("p1"), pt("p2"));
            let l = (p2[0] - p1[0]).hypot(p2[1] - p1[1]);
            let l = if l == 0.0 { 1.0 } else { l };
            json!({ "p2": [p1[0] + (p2[0] - p1[0]) * v / l, p1[1] + (p2[1] - p1[1]) * v / l] })
        }
        ("n", _) => json!({ "n": v.round().max(3.0) }),
        _ => json!({ k: v }),
    }
}

/// `updateEntity(id, patch)`: apply the patch, hold what was typed as locks, solve.
pub fn update_entity(s: &mut SketchSession, id: &str, patch: &Value) -> Vec<SketchEffect> {
    let Some(i) = s.entities.iter().position(|e| e.id() == Some(id)) else { return Vec::new() };
    s.push();
    let mut v = serde_json::to_value(&s.entities[i]).unwrap_or(Value::Null);
    if let (Some(o), Some(p)) = (v.as_object_mut(), patch.as_object()) {
        for (k, x) in p {
            o.insert(k.clone(), x.clone());
        }
    }
    if let Ok(e) = serde_json::from_value::<Entity>(v) {
        s.entities[i] = e;
    }
    if !s.constraints.is_empty() {
        // hold what was just typed, let the relations move everything else
        s.locks.clear();
        s.warn_unkept = true;
        if let Some(p) = patch.as_object() {
            for (k, x) in p {
                if let Some(n) = x.as_f64() {
                    s.locks.push(stl_sketch::solver::Lock { e: id.to_owned(), k: None, at: None, path: Some(k.clone()), value: Some(n) });
                } else if let Some(a) = x.as_array() {
                    for (j, n) in a.iter().enumerate() {
                        if let Some(n) = n.as_f64() {
                            s.locks.push(stl_sketch::solver::Lock { e: id.to_owned(), k: None, at: None, path: Some(format!("{k}.{j}")), value: Some(n) });
                        }
                    }
                }
            }
        }
    }
    vec![SketchEffect::Changed]
}

/// Draw the panel over the viewport's right edge. `ws` is the viewport's rectangle.
pub fn show(ctx: &egui::Context, tk: &Tokens, m: &mut SketchMode, ws: Rect, inch: bool) -> PanelOut {
    m.panel.ensure();
    let mut out = PanelOut::default();
    let pos = egui::pos2(ws.max.x - INSET - PANEL_W, ws.min.y + INSET);
    let max_h = (ws.height() - 60.0).max(120.0);
    Area::new(Id::unique("tool-panel")).order(Order::Middle).fixed_pos(pos).show(ctx, |ui| {
        Frame::new().fill(tk.panel).stroke(Stroke::new(1.0, tk.line)).corner_radius(CornerRadius::same(tk.radius as u8)).shadow(tk.shadow).show(ui, |ui| {
            ui.set_width(PANEL_W - 2.0);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            title_row(ui, ctx, tk, &m.s, &mut out);
            egui::ScrollArea::vertical().max_height((max_h - 36.0 - 41.0).max(60.0)).auto_shrink([false, true]).id_salt("sketch-form").show(ui, |ui| {
                Frame::new().inner_margin(Margin { left: 10, right: 10, top: 8, bottom: 8 }).show(ui, |ui| {
                    ui.set_width(FORM_W);
                    form(ui, tk, m, inch, &mut out);
                });
            });
            actions(ui, tk, &mut out);
        });
    });
    out
}

fn title_row(ui: &mut Ui, ctx: &egui::Context, tk: &Tokens, s: &SketchSession, out: &mut PanelOut) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(PANEL_W - 2.0, 33.0), Sense::hover());
    let cy = rect.center().y;
    let img = crate::icons::icon(ctx, "sketch", 17.0, tk.accent);
    egui::Image::new(img).paint_at(ui, Rect::from_center_size(egui::pos2(rect.min.x + 18.5, cy), Vec2::splat(17.0)));
    let font = egui::FontId::new(12.0, egui::FontFamily::Name("bold".into()));
    let t = title(s);
    let galley = ui.painter().layout_no_wrap(t.to_owned(), font, tk.strong);
    ui.painter().galley(egui::pos2(rect.min.x + 34.0, cy - galley.size().y / 2.0), galley, tk.strong);
    let close = Rect::from_center_size(egui::pos2(rect.max.x - 19.0, cy), Vec2::splat(22.0));
    let resp = ui.interact(close, Id::unique("sketch-close"), Sense::click());
    resp.widget_info(|| WidgetInfo::labeled(egui::Role::Button, true, "Close"));
    if resp.hovered() {
        ui.painter().rect_filled(close, CornerRadius::same(4), tk.hover);
    }
    ui.painter().text(close.center(), egui::Align2::CENTER_CENTER, "×", egui::FontId::proportional(17.0), if resp.hovered() { tk.strong } else { tk.muted });
    if resp.on_hover_text("Cancel (Esc)").clicked() {
        out.cancel = true;
    }
    ui.painter().hline(rect.x_range(), rect.max.y, Stroke::new(1.0, tk.divider));
}

fn actions(ui: &mut Ui, tk: &Tokens, out: &mut PanelOut) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(PANEL_W - 2.0, 41.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.min.y, Stroke::new(1.0, tk.divider));
    let inner = Rect::from_min_max(egui::pos2(rect.min.x + 10.0, rect.min.y + 8.0), egui::pos2(rect.max.x - 10.0, rect.max.y - 9.0));
    let mut right = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(Layout::right_to_left(Align::Center)));
    right.spacing_mut().item_spacing.x = 8.0;
    if crate::ui::dialogs::button(&mut right, tk, FINISH_LABEL, true).clicked() {
        out.finish = true;
    }
    if crate::ui::dialogs::button(&mut right, tk, "Cancel", false).clicked() {
        out.cancel = true;
    }
}

fn form(ui: &mut Ui, tk: &Tokens, m: &mut SketchMode, inch: bool, out: &mut PanelOut) {
    def_line(ui, tk, m);
    let unit = unit_name(inch);
    // the selected dimension: its value (or parameter), and whether it drives the size
    if let Some(i) = m.s.sel_c {
        if let Some(c) = m.s.constraints.get(i).cloned().filter(|c| dims::is_dim(&c.kind)) {
            let is_ang = c.kind.contains("angle");
            section(ui, tk, dims::type_label(&c.kind));
            let is_ref = c.ref_.is_some();
            let ents = super::solve::all_ents(&m.s);
            let cur = if is_ref {
                dims::dim_text(&c, &ents, inch, true)
            } else if let Some(x) = dims::expr_of_dim(&c) {
                x.to_owned()
            } else if is_ang {
                ((c.value.unwrap_or(0.0)) * 1e4).round().div_euclid(1.0).to_string().parse::<f64>().map(|v| (v / 1e4).to_string()).unwrap_or_default()
            } else {
                fmt_len(c.value.unwrap_or(0.0), Some(4), inch)
            };
            let mut typed = None;
            row(ui, tk, "Value", |ui| {
                ui.add_enabled_ui(!is_ref, |ui| {
                    typed = text_field(ui, tk, &mut m.panel, "skdim-value", "Value", &cur, FORM_W - LABEL_W - GAP - 28.0);
                });
                ui.label(RichText::new(if is_ang { "°" } else { unit }).size(11.0).color(tk.muted));
            });
            if let Some(t) = typed {
                match dims::parse_dim(&t, is_ang, inch, &BTreeMap::new()) {
                    Err(e) => out.toasts.push((crate::engine_link::Level::Warning, e)),
                    Ok((v, _)) if !(v > 0.0) => out.toasts.push((crate::engine_link::Level::Warning, "A dimension needs a size above zero.".into())),
                    Ok((v, expr)) => out.effects.extend(dims::set_dim_value(&mut m.s, i, v, expr)),
                }
            }
            row(ui, tk, "Reference", |ui| {
                let mut r = is_ref;
                let cb = crate::tools::fields::check_box(ui, tk, &mut r);
                cb.widget_info(|| WidgetInfo::selected(egui::Role::CheckBox, true, is_ref, "Reference"));
                if cb.changed() {
                    out.effects.extend(dims::set_dim_ref(&mut m.s, i, r));
                }
                if ctl_button(ui, tk, "Delete", true).clicked() {
                    out.effects.extend(super::relations::delete_constraint(&mut m.s, i));
                }
            });
        }
    }
    section(ui, tk, "Shapes");
    shapes_list(ui, tk, m, out);
    if let Some((group, meta)) = super::text::selected_text(&m.s) {
        section(ui, tk, "Text");
        let t = meta.get("text").and_then(Value::as_str).unwrap_or("").to_owned();
        row(ui, tk, "Text", |ui| {
            ui.add(egui::Label::new(RichText::new(format!("“{t}”")).size(12.0).color(tk.text)).truncate());
            if ctl_button(ui, tk, "Edit…", true).clicked() {
                out.edit_text = Some((group.clone(), meta.clone()));
            }
        });
        let font = meta.get("font").and_then(Value::as_str).unwrap_or("");
        let size = meta.get("size").and_then(Value::as_f64).unwrap_or(10.0);
        row(ui, tk, "Font", |ui| {
            ui.label(RichText::new(format!("{font} · {}", fmt_len(size, None, inch))).size(12.0).color(tk.text));
        });
    } else if m.s.sel.len() == 1 {
        let id = m.s.sel.iter().next().cloned().unwrap_or_default();
        if let Some(e) = m.s.entities.iter().find(|e| e.id() == Some(id.as_str())).cloned() {
            section(ui, tk, &format!("{} · {}", entity_label(&e), describe(&e)));
            for (k, l) in entity_fields(&e) {
                let v = field_value(&e, k);
                // 1.x `num(U.toDisp(v))`: rounded to 3 decimals
                let shown = super::typed::fmt3(if is_plain(k) { v } else { to_disp(v, inch) });
                let mut typed = None;
                row(ui, tk, l, |ui| {
                    typed = text_field(ui, tk, &mut m.panel, &format!("ent-{k}"), l, &shown, FORM_W - LABEL_W - GAP);
                });
                if let Some(t) = typed {
                    let n = if is_plain(k) { Some(parse_num_text(&t)) } else { parse_len(&t, inch) };
                    if let Some(n) = n.filter(|n| n.is_finite()) {
                        let patch = patch_for(&e, k, n);
                        out.effects.extend(update_entity(&mut m.s, &id, &patch));
                    }
                }
            }
            if let Entity::Polyline(p) = &e {
                if p.pts.len() > 24 {
                    let extra = if p.extra.get("projected").is_some() { " (projected outline)" } else { "" };
                    row(ui, tk, "Points", |ui| {
                        ui.label(RichText::new(format!("{}{extra} · use Offset, Trim or Explode to edit", p.pts.len())).size(12.0).color(tk.text));
                    });
                } else {
                    for (i, pt) in p.pts.iter().enumerate() {
                        let mut typed: Option<(usize, String)> = None;
                        row(ui, tk, &format!("Point {}", i + 1), |ui| {
                            for j in 0..2 {
                                let shown = crate::tools::fields::number::js_num(to_disp(pt[j], inch));
                                if let Some(t) = text_field(ui, tk, &mut m.panel, &format!("pt-{i}-{j}"), &format!("Point {} {}", i + 1, if j == 0 { "X" } else { "Y" }), &shown, (FORM_W - LABEL_W - GAP) / 2.0 - 3.0) {
                                    typed = Some((j, t));
                                }
                            }
                        });
                        if let Some((j, t)) = typed {
                            if let Some(n) = parse_len(&t, inch).filter(|n| n.is_finite()) {
                                let mut pts: Vec<[f64; 2]> = p.pts.clone();
                                pts[i][j] = n;
                                out.effects.extend(update_entity(&mut m.s, &id, &json!({ "pts": pts })));
                            }
                        }
                    }
                }
            }
            row(ui, tk, "Construction", |ui| {
                let mut on = e.construction();
                let cb = crate::tools::fields::check_box(ui, tk, &mut on);
                cb.widget_info(|| WidgetInfo::selected(egui::Role::CheckBox, true, e.construction(), "Construction"));
                if cb.changed() {
                    out.effects.extend(update_entity(&mut m.s, &id, &json!({ "construction": on })));
                }
            });
        }
    }
    relations_list(ui, tk, m, inch, out);
    section(ui, tk, "Snapping");
    row(ui, tk, "Grid snap", |ui| {
        let mut on = m.panel.snap_grid;
        let cb = crate::tools::fields::check_box(ui, tk, &mut on);
        cb.widget_info(|| WidgetInfo::selected(egui::Role::CheckBox, true, m.panel.snap_grid, "Grid snap"));
        m.panel.snap_grid = on;
        let steps: &[(&str, f64)] = if inch {
            &[("1/64\"", 25.4 / 64.0), ("1/32\"", 25.4 / 32.0), ("1/16\"", 25.4 / 16.0), ("1/8\"", 25.4 / 8.0), ("1/4\"", 25.4 / 4.0), ("1/2\"", 12.7), ("1\"", 25.4)]
        } else {
            &[("0.1", 0.1), ("0.5", 0.5), ("1", 1.0), ("2", 2.0), ("5", 5.0), ("10", 10.0)]
        };
        let cur = steps.iter().find(|(_, v)| (v - m.panel.snap_step).abs() < 1e-9).map(|(l, _)| *l).unwrap_or(steps[0].0);
        crate::tools::fields::framed_combo(ui, tk, "snap-step", 70.0, cur, |ui| {
            for (l, v) in steps {
                ui.selectable_value(&mut m.panel.snap_step, *v, *l);
            }
        });
        ui.label(RichText::new(unit).size(11.0).color(tk.muted));
    });
    // the drawing tools read the grid from here (`snapGrid`, `snapStep`)
    super::snap::set_grid(m.panel.snap_grid, m.panel.snap_step);
    ui.add_space(2.0);
    let info = "Shapes inside other shapes become holes; lines and arcs that close a loop make a shape too. Use Smart Dimension (D) to add sizes; double-click one to change it (a parameter name works too). Black shapes are fully defined, blue ones can still move; orange lines are the part's edges, there to dimension from. When extruding you can pick single regions under Profiles.";
    Frame::new().fill(tk.info_bg).corner_radius(CornerRadius::same(tk.radius_sm as u8)).inner_margin(Margin::symmetric(8, 6)).show(ui, |ui| {
        ui.set_width(FORM_W - 16.0);
        ui.add(egui::Label::new(RichText::new(info).size(12.0).color(tk.info_text)).wrap());
    });
}

/// `.ent-list`: a bordered, scrolling list of rows.
fn list_frame(ui: &mut Ui, tk: &Tokens, add: impl FnOnce(&mut Ui)) {
    Frame::new().fill(tk.input).stroke(Stroke::new(1.0, tk.input_line)).corner_radius(CornerRadius::same(tk.radius_sm as u8)).show(ui, |ui| {
        ui.set_width(FORM_W - 2.0);
        egui::ScrollArea::vertical().max_height(180.0).id_salt(ui.next_auto_id()).auto_shrink([false, true]).show(ui, |ui| {
            ui.set_width(FORM_W - 2.0);
            add(ui);
        });
    });
}

/// One `.ent` row: left text, right muted text; returns the row's response.
fn ent_row(ui: &mut Ui, tk: &Tokens, selected: bool, left: &str, right: &str, name: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(FORM_W - 2.0, 22.0), Sense::click());
    resp.widget_info(|| WidgetInfo::labeled(egui::Role::Button, true, name));
    if selected {
        ui.painter().rect_filled(rect, 0.0, tk.ent_sel);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 0.0, tk.hover);
    }
    ui.painter().text(egui::pos2(rect.min.x + 8.0, rect.center().y), egui::Align2::LEFT_CENTER, left, egui::FontId::proportional(12.0), tk.text);
    ui.painter().text(egui::pos2(rect.max.x - 8.0, rect.center().y), egui::Align2::RIGHT_CENTER, right, egui::FontId::proportional(11.0), tk.muted);
    resp
}

fn shapes_list(ui: &mut Ui, tk: &Tokens, m: &mut SketchMode, out: &mut PanelOut) {
    let rows: Vec<(String, String, String, bool)> = m
        .s
        .entities
        .iter()
        .map(|e| {
            let id = e.id().unwrap_or("").to_owned();
            let l = format!("{}{}", entity_label(e), if e.construction() { " (construction)" } else { "" });
            (id.clone(), l, describe(e), m.s.sel.contains(&id))
        })
        .collect();
    let mut clicked: Option<String> = None;
    list_frame(ui, tk, |ui| {
        if rows.is_empty() {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(FORM_W - 2.0, 22.0), Sense::hover());
            ui.painter().text(egui::pos2(rect.min.x + 8.0, rect.center().y), egui::Align2::LEFT_CENTER, "No shapes yet — pick a tool above.", egui::FontId::proportional(11.0), tk.muted);
        }
        for (id, l, d, sel) in &rows {
            if ent_row(ui, tk, *sel, l, d, &format!("{l} {d}")).clicked() {
                clicked = Some(id.clone());
            }
        }
    });
    if let Some(id) = clicked {
        // text is stored as one outline per letter part; they select together
        let group = m.s.entities.iter().find(|e| e.id() == Some(id.as_str())).and_then(|e| serde_json::to_value(e).ok()).and_then(|v| v.get("group").and_then(Value::as_str).map(str::to_owned));
        m.s.sel.clear();
        match group {
            Some(g) => {
                for e in &m.s.entities {
                    if serde_json::to_value(e).ok().and_then(|v| v.get("group").and_then(Value::as_str).map(|x| x == g)).unwrap_or(false) {
                        if let Some(i) = e.id() {
                            m.s.sel.insert(i.to_owned());
                        }
                    }
                }
            }
            None => {
                m.s.sel.insert(id);
            }
        }
        m.s.sel_c = None;
        m.dirty = true;
        let _ = out;
    }
}

fn relations_list(ui: &mut Ui, tk: &Tokens, m: &mut SketchMode, inch: bool, out: &mut PanelOut) {
    if m.s.constraints.is_empty() {
        return;
    }
    section(ui, tk, &format!("Dimensions & relations ({})", m.s.constraints.len()));
    let rows: Vec<(usize, String, bool, bool)> = m.s.constraints.iter().enumerate().map(|(i, c)| (i, constraint_desc(&m.s, c, inch), c.extra.get("auto").is_some_and(|v| v.as_bool() == Some(true)), m.s.sel_c == Some(i))).collect();
    let (mut select, mut delete) = (None, None);
    list_frame(ui, tk, |ui| {
        for (i, d, auto, sel) in &rows {
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(FORM_W - 2.0, 22.0), Sense::click());
            resp.widget_info(|| WidgetInfo::labeled(egui::Role::Button, true, d.clone()));
            if *sel {
                ui.painter().rect_filled(rect, 0.0, tk.ent_sel);
            } else if resp.hovered() {
                ui.painter().rect_filled(rect, 0.0, tk.hover);
            }
            let g = ui.painter().layout_no_wrap(d.clone(), egui::FontId::proportional(12.0), tk.text);
            let w = g.size().x;
            ui.painter().galley(egui::pos2(rect.min.x + 8.0, rect.center().y - g.size().y / 2.0), g, tk.text);
            if *auto {
                ui.painter().text(egui::pos2(rect.min.x + 14.0 + w, rect.center().y), egui::Align2::LEFT_CENTER, "auto", egui::FontId::proportional(11.0), tk.muted);
            }
            let x = Rect::from_center_size(egui::pos2(rect.max.x - 14.0, rect.center().y), Vec2::splat(18.0));
            let xr = ui.interact(x, Id::unique(("sk-cdel", *i)), Sense::click());
            xr.widget_info(|| WidgetInfo::labeled(egui::Role::Button, true, format!("Remove {d}")));
            ui.painter().text(x.center(), egui::Align2::CENTER_CENTER, "✕", egui::FontId::proportional(11.0), tk.muted);
            if xr.on_hover_text("Remove").clicked() {
                delete = Some(*i);
            } else if resp.clicked() {
                select = Some(*i);
            }
        }
    });
    if let Some(i) = delete {
        out.effects.extend(super::relations::delete_constraint(&mut m.s, i));
    } else if let Some(i) = select {
        m.s.sel_c = if m.s.sel_c == Some(i) { None } else { Some(i) };
        m.s.sel.clear();
        m.dirty = true;
    }
}

// ------------------------------------------------------------------ the dimension box

/// `openModify(id)`: the value box with ✓ and ✗. Returns effects when applied.
pub fn modify_box(ctx: &egui::Context, tk: &Tokens, m: &mut SketchMode, ws: Rect, inch: bool, params: &BTreeMap<String, f64>, anchor: Option<egui::Pos2>) -> (Vec<SketchEffect>, Vec<(crate::engine_link::Level, String)>) {
    let mut effects = Vec::new();
    let mut toasts = Vec::new();
    let Some(md) = m.panel.modify.as_mut() else { return (effects, toasts) };
    let Some(c) = m.s.constraints.get(md.idx).cloned() else {
        m.panel.modify = None;
        return (effects, toasts);
    };
    let is_ang = c.kind.contains("angle");
    let (mut apply, mut cancel) = (false, false);
    let id = Id::unique("sk-modify-input");
    // 1.x: centred over the dimension's label, 12 px above it (`translate(-50%, calc(-100% - 12px))`)
    let area = Area::new(Id::unique("sk-modify")).order(Order::Foreground);
    let area = match anchor {
        Some(p) => area.pivot(egui::Align2::CENTER_BOTTOM).fixed_pos(egui::pos2(p.x, p.y - 12.0)),
        None => area.fixed_pos(egui::pos2(ws.center().x - 90.0, ws.min.y + 60.0)),
    };
    let was_fresh = md.fresh;
    let shown = area.show(ctx, |ui| {
        Frame::new().fill(tk.panel).stroke(Stroke::new(1.0, tk.accent)).corner_radius(CornerRadius::same(tk.radius as u8)).shadow(tk.shadow).inner_margin(Margin::symmetric(8, 6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                let r = crate::tools::fields::input_box(ui, tk, &mut md.text, id, 110.0, egui::FontId::proportional(12.0), tk.text);
                r.widget_info(|| WidgetInfo::labeled(egui::Role::TextInput, true, "Dimension value"));
                if md.fresh {
                    md.fresh = false;
                    r.request_focus();
                    if let Some(mut st) = egui::TextEdit::load_state(ctx, id) {
                        st.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(md.text.chars().count()))));
                        st.store(ctx, id);
                    }
                }
                ui.label(RichText::new(if is_ang { "°" } else { unit_name(inch) }).size(11.0).color(tk.muted));
                let ok = ui.button("✓");
                ok.widget_info(|| WidgetInfo::labeled(egui::Role::Button, true, "Apply"));
                if ok.on_hover_text("Apply (Enter)").clicked() {
                    apply = true;
                }
                let x = ui.button("✗");
                x.widget_info(|| WidgetInfo::labeled(egui::Role::Button, true, "Leave it"));
                if x.on_hover_text("Leave it (Esc)").clicked() {
                    cancel = true;
                }
                if r.has_focus() {
                    if ui.input(|i| i.key_pressed(Key::Enter)) {
                        apply = true;
                    }
                    if ui.input(|i| i.key_pressed(Key::Escape)) {
                        cancel = true;
                    }
                }
            });
        });
    });
    // 1.x `openModify`: a pointer press outside the box applies it (Esc / ✗ cancel)
    if !was_fresh && !apply && !cancel {
        let outside = ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !shown.response.rect.contains(p)));
        if outside {
            apply = true;
        }
    }
    if apply {
        let text = md.text.clone();
        let idx = md.idx;
        match dims::parse_dim(&text, is_ang, inch, params) {
            Err(e) => toasts.push((crate::engine_link::Level::Warning, e)),
            Ok((v, expr)) => {
                if !(v > 0.0) || (is_ang && v > 180.0) {
                    toasts.push((crate::engine_link::Level::Warning, if is_ang { "An angle between 0° and 180°.".to_owned() } else { "A dimension needs a size above zero.".to_owned() }));
                } else {
                    m.panel.modify = None;
                    let cur = c.value.unwrap_or(0.0);
                    if (v - cur).abs() > 1e-9 || expr.as_deref() != dims::expr_of_dim(&c) {
                        effects.extend(dims::set_dim_value(&mut m.s, idx, v, expr));
                    }
                }
            }
        }
    } else if cancel {
        m.panel.modify = None;
    }
    (effects, toasts)
}
