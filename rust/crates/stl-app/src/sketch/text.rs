//! Sketch text tool and the text dialog (1.x `sketchText`, `app.js` 1629–1677).

use egui::{RichText, Ui};
use serde_json::{json, Value};

use crate::theme::Tokens;
use crate::tools::fields::{self, Bufs, FieldEnv};
use crate::tools::fields::number::{fmt_len, parse_len, unit_name};
use crate::tools::panel::ctl_button;
use crate::viewport::overlays::SketchOutline;

use super::{SketchDialog, SketchEffect, SketchInput, SketchSession, SketchTool, ToolCx};

thread_local! {
    static PENDING_UV: std::cell::Cell<Option<[f64; 2]>> = const { std::cell::Cell::new(None) };
}

/// UV from the last text-tool click; Task 2 reads this when opening [`SketchDialog::Text`].
pub fn take_pending_uv() -> Option<[f64; 2]> {
    PENDING_UV.with(|c| {
        let v = c.get();
        c.set(None);
        v
    })
}

fn set_pending_uv(uv: [f64; 2]) {
    PENDING_UV.with(|c| c.set(Some(uv)));
}

/// Form fields matching 1.x `sketchText` / `sketch_op` `text`.
#[derive(Clone, Debug, PartialEq)]
pub struct TextForm {
    pub text: String,
    pub font: String,
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    pub spacing: f64,
    pub align: String,
    pub angle: f64,
    pub x: f64,
    pub y: f64,
}

impl Default for TextForm {
    fn default() -> Self {
        TextForm {
            text: "Text".into(),
            font: default_font_name(),
            size: 10.0,
            bold: false,
            italic: false,
            spacing: 0.0,
            align: "left".into(),
            angle: 0.0,
            x: 0.0,
            y: 0.0,
        }
    }
}

/// State of the text modal while it is open in the sketch panel.
#[derive(Default)]
pub struct TextDialogState {
    pub open: bool,
  /// When set, `sketch_op` replaces this text group (1.x `group` argument).
    pub edit_group: Option<String>,
    pub form: TextForm,
    pub bufs: Bufs,
    pub last_font: String,
    pub inch: bool,
}

pub fn tool_for(key: &str) -> Option<Box<dyn SketchTool>> {
    if key == "text" {
        Some(Box::new(TextTool))
    } else {
        None
    }
}

fn sketch_num(n: f64) -> Value {
    if n.is_finite() && n.fract().abs() < 1e-9 && n.abs() < 9e15 {
        json!(n as i64)
    } else {
        serde_json::Number::from_f64(n).map(Value::Number).unwrap_or(json!(0))
    }
}

/// The `data` object 1.x posts to `sketch_op` for `op: "text"` (entities and seed included).
pub fn sketch_op_data(s: &SketchSession, form: &TextForm, group: Option<&str>) -> Value {
    let entities: Vec<Value> = s.entities.iter().filter_map(|e| serde_json::to_value(e).ok()).collect();
    let mut data = json!({
        "op": "text",
        "entities": entities,
        "seed": s.seed,
        "text": form.text,
        "font": form.font,
        "size": sketch_num(form.size),
        "bold": form.bold,
        "italic": form.italic,
        "spacing": sketch_num(form.spacing),
        "align": form.align,
        "angle": sketch_num(form.angle),
        "x": sketch_num(form.x),
        "y": sketch_num(form.y),
    });
    if let Some(g) = group {
        data["group"] = json!(g);
    }
    data
}

/// Open a new text placement at `uv` (millimetres on the sketch plane).
pub fn open_new(state: &mut TextDialogState, uv: [f64; 2], inch: bool) {
    let mut form = TextForm::default();
    if !state.last_font.is_empty() {
        form.font = state.last_font.clone();
    }
    form.x = (uv[0] * 1000.0).round() / 1000.0;
    form.y = (uv[1] * 1000.0).round() / 1000.0;
    state.edit_group = None;
    state.form = form;
    state.bufs.clear();
    state.inch = inch;
    state.open = true;
}

/// Open the dialog to edit an existing text group (`Edit text` in 1.x).
pub fn open_edit(state: &mut TextDialogState, group: &str, meta: &Value, inch: bool) {
    let mut form = TextForm::default();
    let read = |k: &str, def: &str| meta.get(k).and_then(Value::as_str).unwrap_or(def).to_string();
    let read_n = |k: &str, def: f64| meta.get(k).and_then(|v| v.as_f64()).unwrap_or(def);
    let read_b = |k: &str| meta.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    form.text = read("text", "Text");
    form.font = read("font", &default_font_name());
    form.size = read_n("size", 10.0);
    form.bold = read_b("bold");
    form.italic = read_b("italic");
    form.spacing = read_n("spacing", 0.0);
    form.align = read("align", "left");
    form.angle = read_n("angle", 0.0);
    form.x = read_n("x", 0.0);
    form.y = read_n("y", 0.0);
    state.edit_group = Some(group.into());
    state.form = form;
    state.bufs.clear();
    state.inch = inch;
    state.open = true;
}

/// 1.x `selectedText()`: every selected entity shares a `group` and carries `text` metadata.
pub fn selected_text(s: &SketchSession) -> Option<(String, Value)> {
    let sel: Vec<&Entity> = s
        .entities
        .iter()
        .filter(|e| e.id().is_some_and(|id| s.sel.contains(id)))
        .collect();
    if sel.is_empty() {
        return None;
    }
    let v0 = serde_json::to_value(sel[0]).ok()?;
    let group = v0.get("group")?.as_str()?.to_string();
    if !sel.iter().all(|e| {
        serde_json::to_value(e)
            .ok()
            .and_then(|v| v.get("group").and_then(|g| g.as_str()).map(|g| g == group))
            .unwrap_or(false)
    }) {
        return None;
    }
    v0.get("text").cloned().map(|meta| (group, meta))
}

use stl_sketch::Entity;

/// Returns an error message word for word when Add/Update cannot run (1.x toasts).
pub fn validate_for_commit(state: &TextDialogState) -> Result<(), &'static str> {
    if state.form.text.trim().is_empty() {
        return Err("Type some text first.");
    }
    let parsed = state
        .bufs
        .get("size")
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .and_then(|t| parse_len(t, state.inch));
    let size = match parsed {
        Some(v) if v > 0.0 => Some(v),
        Some(_) => None,
        None if state.bufs.get("size").is_some_and(|t| !t.trim().is_empty()) => None,
        None if state.form.size > 0.0 => Some(state.form.size),
        _ => None,
    };
    if size.is_none() {
        return Err("Enter a text height.");
    }
    Ok(())
}

/// Draw the text modal in the sketch tool panel. Returns an effect when the user commits or cancels.
pub fn dialog(ui: &mut Ui, tk: &Tokens, s: &SketchSession, state: &mut TextDialogState) -> Option<SketchEffect> {
    if !state.open {
        return None;
    }
    let _env = FieldEnv { tk, inch: state.inch, params: &Default::default() };
    let editing = state.edit_group.is_some();
    let mut effect = None;
    ui.vertical(|ui| {
        ui.label(
            RichText::new(if editing { "Edit text" } else { "Text" })
                .font(egui::FontId::new(15.0, egui::FontFamily::Name("bold".into())))
                .color(tk.strong),
        );
        ui.add_space(8.0);
        fields::row_raw(ui, tk, "", None, true, |ui, w| {
            ui.scope(|ui| {
                // `.field textarea`: the same input colours and border as the one-line boxes
                let v = ui.visuals_mut();
                v.extreme_bg_color = tk.input;
                v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, tk.input_line);
                v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, tk.input_line);
                v.widgets.active.bg_stroke = egui::Stroke::new(1.0, tk.accent);
                v.selection.stroke = egui::Stroke::new(1.0, tk.accent);
                v.override_text_color = Some(tk.text);
                ui.add(
                    egui::TextEdit::multiline(&mut state.form.text)
                        .desired_width(w)
                        .desired_rows(2)
                        .margin(egui::Margin::symmetric(6, 6)),
                );
            });
            None::<()>
        });
        fields::row_raw(ui, tk, "Font", None, false, |ui, w| {
            let families = font_choices();
            let shown = state.form.font.clone();
            let mut chosen = None;
            fields::framed_combo(ui, tk, ("sk-text", "font"), w, &shown, |ui| {
                for f in &families {
                    if ui.selectable_label(state.form.font == *f, f.as_str()).clicked() {
                        chosen = Some(f.clone());
                    }
                }
            });
            if let Some(f) = chosen {
                state.form.font = f;
            }
            None::<()>
        });
        fields::row_raw(ui, tk, "Height", None, false, |ui, w| {
            let shown = fmt_len(state.form.size, Some(3), state.inch);
            if let Some(t) = fields::text_commit(ui, tk, &mut state.bufs, "size", &shown, w - 22.0, fields::number::input_font(), tk.text, None)
            {
                if let Some(mm) = parse_len(&t, state.inch).filter(|v| *v > 0.0) {
                    state.form.size = mm;
                }
            }
            fields::unit_label(ui, tk, unit_name(state.inch));
            None::<()>
        });
        fields::row_raw(ui, tk, "Style", None, false, |ui, _w| {
            ui.horizontal(|ui| {
                for (on, name) in [(&mut state.form.bold, "Bold"), (&mut state.form.italic, "Italic")] {
                    fields::check_box(ui, tk, on);
                    ui.label(RichText::new(name).size(12.0).color(tk.text));
                }
            });
            None::<()>
        });
        fields::row_raw(ui, tk, "Align", None, false, |ui, w| {
            let pairs = [("left", "Left"), ("center", "Centre"), ("right", "Right")];
            let label = align_label(&state.form.align);
            let mut chosen = None;
            fields::framed_combo(ui, tk, ("sk-text", "align"), w, &label, |ui| {
                for (v, l) in pairs {
                    if ui.selectable_label(state.form.align == v, l).clicked() {
                        chosen = Some(v);
                    }
                }
            });
            if let Some(v) = chosen {
                state.form.align = v.into();
            }
            None::<()>
        });
        fields::row_raw(ui, tk, "Letter spacing", None, false, |ui, w| {
            let shown = fmt_len(state.form.spacing, Some(3), state.inch);
            if let Some(t) = fields::text_commit(ui, tk, &mut state.bufs, "spacing", &shown, w - 22.0, fields::number::input_font(), tk.text, None)
            {
                state.form.spacing = parse_len(&t, state.inch).unwrap_or(0.0);
            }
            fields::unit_label(ui, tk, unit_name(state.inch));
            None::<()>
        });
        fields::row_raw(ui, tk, "Position", None, false, |ui, w| {
            let bw = (w - 22.0 - 4.0) / 2.0;
            let sx = fmt_len(state.form.x, Some(3), state.inch);
            let sy = fmt_len(state.form.y, Some(3), state.inch);
            if let Some(t) = fields::text_commit(ui, tk, &mut state.bufs, "x", &sx, bw, fields::number::input_font(), tk.text, None) {
                state.form.x = parse_len(&t, state.inch).unwrap_or(0.0);
            }
            if let Some(t) = fields::text_commit(ui, tk, &mut state.bufs, "y", &sy, bw, fields::number::input_font(), tk.text, None) {
                state.form.y = parse_len(&t, state.inch).unwrap_or(0.0);
            }
            fields::unit_label(ui, tk, unit_name(state.inch));
            None::<()>
        });
        fields::row_raw(ui, tk, "Rotation", None, false, |ui, w| {
            let shown = format!("{}", state.form.angle);
            if let Some(t) = fields::text_commit(ui, tk, &mut state.bufs, "angle", &shown, w - 22.0, fields::number::input_font(), tk.text, None) {
                state.form.angle = t.parse().unwrap_or(0.0);
            }
            fields::unit_label(ui, tk, "°");
            None::<()>
        });
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "Height is the font's full height (capitals come out a little smaller). Letters with holes, like O and A, keep their holes. Extrude the text to emboss it, or cut to engrave it.",
            )
            .size(12.0)
            .color(tk.muted),
        );
        ui.add_space(10.0);
        let mut cancel = false;
        let mut ok = false;
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if ctl_button(ui, tk, if editing { "Update" } else { "Add text" }, true).clicked() {
                    ok = true;
                }
                if ctl_button(ui, tk, "Cancel", false).clicked() {
                    cancel = true;
                }
            });
        });
        if cancel {
            // closes the dialog only; the sketch stays open
            state.open = false;
        } else if ok {
            if let Err(msg) = validate_for_commit(state) {
                effect = Some(SketchEffect::Toast(crate::engine_link::Level::Warning, msg.into()));
            } else {
                if let Some(mm) = state
                    .bufs
                    .get("size")
                    .and_then(|t| parse_len(t.trim(), state.inch))
                    .filter(|v| *v > 0.0)
                {
                    state.form.size = mm;
                }
                state.last_font = state.form.font.clone();
                state.open = false;
                let group = state.edit_group.as_deref();
                let data = sketch_op_data(s, &state.form, group);
                effect = Some(SketchEffect::SketchOp { op: "text".into(), data });
            }
        }
    });
    effect
}

fn align_label(v: &str) -> String {
    match v {
        "center" => "Centre".into(),
        "right" => "Right".into(),
        _ => "Left".into(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn font_choices() -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for f in stl_sketch::text::list_fonts() {
        if seen.insert(f.family.clone()) {
            out.push(f.family);
        }
    }
    if out.is_empty() {
        bundled_fonts()
    } else {
        out
    }
}

#[cfg(target_arch = "wasm32")]
fn font_choices() -> Vec<String> {
    bundled_fonts()
}

fn bundled_fonts() -> Vec<String> {
    vec!["DejaVu Sans".into(), "DejaVu Serif".into(), "DejaVu Sans Mono".into()]
}

fn default_font_name() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        for want in ["Arial", "Segoe UI", "DejaVu Sans", "Liberation Sans"] {
            if stl_sketch::text::list_fonts().iter().any(|f| f.family == want) {
                return want.into();
            }
        }
    }
    "DejaVu Sans".into()
}

struct TextTool;

impl SketchTool for TextTool {
    fn key(&self) -> &'static str {
        "text"
    }

    fn hint(&self, _s: &SketchSession) -> String {
        "Click where the text should start".into()
    }

    fn input(&mut self, cx: &mut ToolCx, ev: &SketchInput) -> bool {
        let SketchInput::Up { uv, .. } = ev else {
            return false;
        };
        set_pending_uv(*uv);
        cx.s.tool = "select".into();
        cx.out.push(SketchEffect::Dialog(SketchDialog::Text { group: None }));
        cx.out.push(SketchEffect::Hint(String::new()));
        true
    }

    fn cancel(&mut self, _cx: &mut ToolCx) {}

    fn picks_geometry(&self) -> bool {
        true
    }

    fn preview(&self, _s: &SketchSession) -> Vec<SketchOutline> {
        Vec::new()
    }

    fn sketch_op_result(&mut self, cx: &mut ToolCx, op: &str, result: &Value) {
        if op != "text" {
            return;
        }
        cx.s.tool = "select".into();
        if let Some(added) = result.get("added").and_then(|a| a.as_array()) {
            cx.s.sel.clear();
            for id in added.iter().filter_map(|v| v.as_str()) {
                cx.s.sel.insert(id.to_owned());
            }
        }
    }
}