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
    /// The font list, loaded when the dialog first draws (kept for the session, so the
    /// computer's fonts are asked for once, as 1.x).
    pub fonts: Option<FontList>,
    /// The open "Add a font file…" picker.
    pub font_pick: Option<crate::platform::Pending<Option<(crate::platform::PickedFile, Option<String>)>>>,
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
/// Without a platform the font list can't add font files; the app calls [`dialog_with`].
pub fn dialog(ui: &mut Ui, tk: &Tokens, s: &SketchSession, state: &mut TextDialogState) -> Option<SketchEffect> {
    dialog_inner(ui, tk, s, state, None)
}

/// [`dialog`] with the platform: "Add a font file…" opens a file picker and remembers the font
/// in settings.
pub fn dialog_with(ui: &mut Ui, tk: &Tokens, s: &SketchSession, state: &mut TextDialogState, platform: &dyn crate::platform::Platform) -> Option<SketchEffect> {
    dialog_inner(ui, tk, s, state, Some(platform))
}

/// The font box chose `value`: a family, or [`LOCAL`] / [`FILE`] (1.x `fs.onchange`). Returns a
/// toast to show, if any.
pub fn choose_font(state: &mut TextDialogState, value: &str, platform: Option<&dyn crate::platform::Platform>) -> Option<SketchEffect> {
    match value {
        #[cfg(not(target_arch = "wasm32"))]
        LOCAL => {
            let fonts = state.fonts.get_or_insert_with(|| platform.map_or_else(FontList::bundled_only, FontList::load));
            state.form.font = fonts.load_system().unwrap_or_else(default_font_name);
        }
        FILE => {
            if let Some(p) = platform {
                state.font_pick = Some(p.open_file_path(&[("Fonts", &["ttf", "otf"])]));
            }
        }
        f => state.form.font = f.to_string(),
    }
    None
}

/// A picked font file arrived: add it (1.x `addFontFiles`). Picks its family, or the default
/// with a warning when the file isn't a font.
pub fn font_file_picked(state: &mut TextDialogState, file: &crate::platform::PickedFile, path: Option<&str>, platform: &dyn crate::platform::Platform) -> Option<SketchEffect> {
    let fonts = state.fonts.get_or_insert_with(|| FontList::load(platform));
    match fonts.add_file(platform, &file.name, &file.bytes, path) {
        Ok(added) => {
            state.form.font = added.first().cloned().unwrap_or_else(default_font_name);
            Some(SketchEffect::Toast(crate::engine_link::Level::Info, added_message(&added)))
        }
        Err(e) => {
            state.form.font = default_font_name();
            Some(SketchEffect::Toast(crate::engine_link::Level::Warning, e))
        }
    }
}

fn dialog_inner(ui: &mut Ui, tk: &Tokens, s: &SketchSession, state: &mut TextDialogState, platform: Option<&dyn crate::platform::Platform>) -> Option<SketchEffect> {
    if !state.open {
        return None;
    }
    if state.fonts.is_none() {
        state.fonts = Some(platform.map_or_else(FontList::bundled_only, FontList::load));
    }
    let mut picked_toast = None;
    if let (Some(pending), Some(p)) = (state.font_pick.clone(), platform) {
        match pending.take() {
            Some(answer) => {
                state.font_pick = None;
                if let Some((file, path)) = answer {
                    picked_toast = font_file_picked(state, &file, path.as_deref(), p);
                }
            }
            None => ui.ctx().request_repaint(),
        }
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
            let entries = state.fonts.as_ref().map(FontList::entries).unwrap_or_default();
            let shown = state.form.font.clone();
            let mut chosen = None;
            fields::framed_combo(ui, tk, ("sk-text", "font"), w, &shown, |ui| {
                for (value, label) in &entries {
                    if ui.selectable_label(state.form.font == *value, label.as_str()).clicked() {
                        chosen = Some(value.clone());
                    }
                }
            });
            if let Some(v) = chosen {
                choose_font(state, &v, platform);
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
    effect.or(picked_toast)
}

fn align_label(v: &str) -> String {
    match v {
        "center" => "Centre".into(),
        "right" => "Right".into(),
        _ => "Left".into(),
    }
}

fn bundled_fonts() -> Vec<String> {
    BUNDLED.iter().map(|s| s.to_string()).collect()
}

/// The text fonts every edition has (1.x web `BUNDLED_FONTS`; the engine draws them itself).
pub const BUNDLED: [&str; 3] = ["DejaVu Sans", "DejaVu Serif", "DejaVu Sans Mono"];

/// The text dialog's default font: on the desktop the first of Arial, Segoe UI, DejaVu Sans,
/// Liberation Sans this computer has (1.x `MT.default_font()`); else DejaVu Sans (1.x web
/// `F.default`).
pub fn default_font_name() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let fonts = stl_sketch::text::list_fonts();
        for want in ["Arial", "Segoe UI", "DejaVu Sans", "Liberation Sans"] {
            if fonts.iter().any(|f| f.family == want) {
                return want.into();
            }
        }
    }
    BUNDLED[0].into()
}

// ------------------------------------------------------------------------ the font list

/// The font list entry that adds the computer's own fonts (desktop only), word for word 1.x.
pub const MORE_FONTS: &str = "More: fonts on this computer…";
/// The font list entry that adds fonts from a file (desktop and web), word for word 1.x.
pub const ADD_FONT_FILE: &str = "Add a font file…";
/// Value of the [`MORE_FONTS`] entry in [`FontList::entries`] (1.x `__local__`).
pub const LOCAL: &str = "__local__";
/// Value of the [`ADD_FONT_FILE`] entry in [`FontList::entries`] (1.x `__file__`).
pub const FILE: &str = "__file__";
/// The setting that remembers added fonts. 1.x kept them in the browser's IndexedDB `fonts`
/// store and had no settings key.
pub const FONTS_KEY: &str = "text_fonts";
/// 1.x `font_info`'s message, without ".woff" (2.0 reads .ttf and .otf only).
pub const BAD_FONT: &str = "That file isn\u{2019}t a font STL Editor can read (use .ttf or .otf).";

/// 1.x's toast after adding font files: `Added ${added.join(', ')}`.
pub fn added_message(families: &[String]) -> String {
    format!("Added {}", families.join(", "))
}

/// Where a font that isn't bundled comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontSource {
    /// A file on this computer (a system font, or an added file on the desktop).
    Path(String),
    /// The bytes of an added file (the web edition has no path).
    Bytes(Vec<u8>),
}

/// One added font file: family, style (1.x codes: "", "b", "i", "bi") and where it is.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Added {
    family: String,
    style: String,
    source: FontSource,
}

/// The text dialog's font list (1.x `post('fonts')` with `canAskLocal` / `canAddFile`): the
/// bundled fonts, then the added ones, then this computer's fonts once asked for.
#[derive(Clone, Debug, Default)]
pub struct FontList {
    added: Vec<Added>,
    /// (family, file) once "More: fonts on this computer…" was chosen.
    system: Option<Vec<(String, std::path::PathBuf)>>,
    web: bool,
    /// "Add a font file…" is offered (there is a platform to pick the file and remember it).
    can_add: bool,
}

impl FontList {
    /// The list with the fonts remembered in settings. A remembered file that has gone is left out.
    pub fn load(p: &dyn crate::platform::Platform) -> FontList {
        let mut added: Vec<Added> = Vec::new();
        for v in p.setting_json(FONTS_KEY).and_then(|v| v.as_array().cloned()).unwrap_or_default() {
            let Some(family) = v.get("family").and_then(Value::as_str).filter(|f| !f.is_empty()) else { continue };
            let style = v.get("style").and_then(Value::as_str).unwrap_or("").to_string();
            let source = if let Some(path) = v.get("path").and_then(Value::as_str) {
                #[cfg(not(target_arch = "wasm32"))]
                if !std::path::Path::new(path).is_file() {
                    continue;
                }
                FontSource::Path(path.to_string())
            } else if let Some(bytes) = v.get("data").and_then(Value::as_str).and_then(base64_decode) {
                FontSource::Bytes(bytes)
            } else {
                continue;
            };
            if !added.iter().any(|a| a.family == family && a.style == style) {
                added.push(Added { family: family.to_string(), style, source });
            }
        }
        FontList { added, system: None, web: p.is_web(), can_add: true }
    }

    /// Without a platform: the bundled fonts, and (desktop) the computer's fonts on request.
    pub fn bundled_only() -> FontList {
        FontList { web: cfg!(target_arch = "wasm32"), ..FontList::default() }
    }

    /// 1.x `canAskLocal`: the desktop, and the computer's fonts aren't in the list yet.
    pub fn can_ask_local(&self) -> bool {
        !self.web && self.system.is_none()
    }

    /// 1.x `canAddFile`.
    pub fn can_add_file(&self) -> bool {
        self.can_add
    }

    /// Families in list order, as 1.x's `fonts` route: the bundled ones, then the added ones
    /// (sorted), then the computer's (sorted); each family once.
    pub fn families(&self) -> Vec<String> {
        let mut out = bundled_fonts();
        let mut added: Vec<&str> = self.added.iter().map(|a| a.family.as_str()).collect();
        added.sort_by_key(|a| a.to_lowercase());
        let mut local: Vec<&str> = self.system.iter().flatten().map(|(f, _)| f.as_str()).collect();
        local.sort_by_key(|a| a.to_lowercase());
        for f in added.into_iter().chain(local) {
            if !out.iter().any(|o| o == f) {
                out.push(f.to_string());
            }
        }
        out
    }

    /// (value, label) for the font box: the families, then [`MORE_FONTS`] and [`ADD_FONT_FILE`]
    /// when they apply (values [`LOCAL`] and [`FILE`]).
    pub fn entries(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = self.families().into_iter().map(|f| (f.clone(), f)).collect();
        if self.can_ask_local() {
            out.push((LOCAL.into(), MORE_FONTS.into()));
        }
        if self.can_add_file() {
            out.push((FILE.into(), ADD_FONT_FILE.into()));
        }
        out
    }

    /// The computer's fonts join the list. Returns the family to pick: Arial when there is
    /// one (1.x `fams.includes('Arial') ? 'Arial' : null`).
    pub fn set_system(&mut self, fonts: Vec<(String, std::path::PathBuf)>) -> Option<String> {
        let arial = fonts.iter().any(|(f, _)| f == "Arial");
        self.system = Some(fonts);
        arial.then(|| "Arial".to_string())
    }

    /// "More: fonts on this computer…": list them now (desktop).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_system(&mut self) -> Option<String> {
        self.set_system(system_fonts())
    }

    /// Read a font file the person picked, add it and remember it in settings. Returns the
    /// families added (for [`added_message`]); [`BAD_FONT`] when it isn't a readable font.
    pub fn add_file(&mut self, p: &dyn crate::platform::Platform, _name: &str, bytes: &[u8], path: Option<&str>) -> Result<Vec<String>, String> {
        let (family, style) = font_info(bytes)?;
        let source = match path.filter(|s| !s.is_empty()) {
            Some(path) => FontSource::Path(path.to_string()),
            None => FontSource::Bytes(bytes.to_vec()),
        };
        self.added.retain(|a| !(a.family == family && a.style == style));
        self.added.push(Added { family: family.clone(), style, source });
        p.set_setting_json(FONTS_KEY, self.saved());
        Ok(vec![family])
    }

    fn saved(&self) -> Value {
        Value::Array(
            self.added
                .iter()
                .map(|a| match &a.source {
                    FontSource::Path(path) => json!({"family": a.family, "style": a.style, "path": path}),
                    FontSource::Bytes(b) => json!({"family": a.family, "style": a.style, "data": base64_encode(b)}),
                })
                .collect(),
        )
    }

    /// Where the engine finds a family that isn't bundled, with 1.x's style fallback
    /// (`fileFontData`: the wanted style, then Bold or Regular, then Regular, then any).
    /// `None` for the bundled fonts and for families not in the list.
    pub fn font_source(&self, family: &str, bold: bool, italic: bool) -> Option<FontSource> {
        if BUNDLED.contains(&family) {
            return None;
        }
        let styles: Vec<&Added> = self.added.iter().filter(|a| a.family == family).collect();
        if !styles.is_empty() {
            let want = format!("{}{}", if bold { "b" } else { "" }, if italic { "i" } else { "" });
            let pick = [want.as_str(), if bold { "b" } else { "" }, ""]
                .iter()
                .find_map(|w| styles.iter().find(|a| a.style == *w))
                .or(styles.first())?;
            return Some(pick.source.clone());
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some((_, regular)) = self.system.iter().flatten().find(|(f, _)| f == family) {
            // another style of a listed family: from the computer's full list, as 1.x `_font_path`
            let all = stl_sketch::text::list_fonts();
            let styled = (bold || italic)
                .then(|| all.iter().any(|i| i.family == family))
                .filter(|&has| has)
                .and_then(|_| stl_sketch::text::fonts::font_path_among(&all, Some(family), bold, italic).ok());
            return Some(FontSource::Path(styled.unwrap_or_else(|| regular.clone()).to_string_lossy().into_owned()));
        }
        None
    }

    /// The bytes of [`FontList::font_source`] (a path is read now).
    pub fn font_bytes(&self, family: &str, bold: bool, italic: bool) -> Option<Vec<u8>> {
        match self.font_source(family, bold, italic)? {
            FontSource::Bytes(b) => Some(b),
            #[cfg(not(target_arch = "wasm32"))]
            FontSource::Path(p) => std::fs::read(p).ok(),
            #[cfg(target_arch = "wasm32")]
            FontSource::Path(_) => None,
        }
    }
}

/// This computer's outline fonts as (family, file), one per family (its Regular style when it
/// has one): the folders 1.x searched (`%WINDIR%\Fonts`, the per-user
/// `%LOCALAPPDATA%\Microsoft\Windows\Fonts`, the Linux and Mac font folders).
#[cfg(not(target_arch = "wasm32"))]
pub fn system_fonts() -> Vec<(String, std::path::PathBuf)> {
    one_per_family(stl_sketch::text::list_fonts())
}

/// [`system_fonts`] in these folders only.
#[cfg(not(target_arch = "wasm32"))]
pub fn system_fonts_in(dirs: &[std::path::PathBuf]) -> Vec<(String, std::path::PathBuf)> {
    one_per_family(stl_sketch::text::fonts::list_fonts_in(dirs))
}

#[cfg(not(target_arch = "wasm32"))]
fn one_per_family(fonts: Vec<stl_sketch::text::FontInfo>) -> Vec<(String, std::path::PathBuf)> {
    let mut out: Vec<(String, std::path::PathBuf)> = Vec::new();
    for f in &fonts {
        if out.iter().any(|(fam, _)| *fam == f.family) {
            continue;
        }
        let path = stl_sketch::text::fonts::font_path_among(&fonts, Some(&f.family), false, false).unwrap_or_else(|_| f.path.clone());
        out.push((f.family.clone(), path));
    }
    out
}

/// Family and style code ("", "b", "i", "bi") of a .ttf / .otf file, as 1.x `font_info`
/// (family: typographic, else font family; style from the subfamily name). [`BAD_FONT`] when
/// the sketch text can't read it.
pub fn font_info(bytes: &[u8]) -> Result<(String, String), String> {
    let bad = || BAD_FONT.to_string();
    let names = sfnt_names(bytes).ok_or_else(bad)?;
    // the glyph reader the sketch text uses must accept it too
    stl_sketch::text::text_contours(bytes, "A", 10.0, &stl_sketch::text::TextOpts::default()).map_err(|_| bad())?;
    let pick = |ids: &[u16]| ids.iter().find_map(|id| best_name(&names, *id));
    let family = pick(&[16, 1]).unwrap_or_else(|| "Font".into());
    let sub = pick(&[17, 2]).unwrap_or_else(|| "Regular".into()).to_lowercase();
    let mut style = String::new();
    if ["bold", "black", "heavy"].iter().any(|w| sub.contains(w)) {
        style.push('b');
    }
    if sub.contains("italic") || sub.contains("oblique") {
        style.push('i');
    }
    Ok((family, style))
}

/// (platform, encoding, language, name id, text) of every name record of an outline font
/// (TrueType `glyf` or CFF). `None` when the file isn't one.
fn sfnt_names(d: &[u8]) -> Option<Vec<(u16, u16, u16, u16, String)>> {
    // every offset comes from the file: checked sums, so a crafted file can't overflow (wasm32)
    let span = |o: usize, n: usize| d.get(o..o.checked_add(n)?);
    let u16_at = |o: usize| span(o, 2).map(|b| u16::from_be_bytes([b[0], b[1]]));
    let u32_at = |o: usize| span(o, 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let add = |a: usize, b: usize| a.checked_add(b);
    let mul = |a: usize, b: usize| a.checked_mul(b);
    let mut base = 0usize;
    if span(0, 4)? == b"ttcf" {
        base = usize::try_from(u32_at(12)?).ok()?;
    }
    let tag = span(base, 4)?;
    if tag != [0, 1, 0, 0] && tag != b"OTTO" && tag != b"true" {
        return None;
    }
    let count = u16_at(add(base, 4)?)? as usize;
    let mut name = None;
    let mut outlines = false;
    for i in 0..count {
        let r = add(add(base, 12)?, mul(16, i)?)?;
        let t = span(r, 4)?;
        let off = usize::try_from(u32_at(add(r, 8)?)?).ok()?;
        let len = usize::try_from(u32_at(add(r, 12)?)?).ok()?;
        span(off, len)?;
        match t {
            b"name" => name = Some(off),
            b"glyf" | b"CFF " | b"CFF2" => outlines = true,
            _ => {}
        }
    }
    let n = name.filter(|_| outlines)?;
    let records = u16_at(add(n, 2)?)? as usize;
    let strings = add(n, u16_at(add(n, 4)?)? as usize)?;
    let mut out = Vec::new();
    for i in 0..records {
        let r = add(add(n, 6)?, mul(12, i)?)?;
        let (plat, enc, lang, id) = (u16_at(r)?, u16_at(add(r, 2)?)?, u16_at(add(r, 4)?)?, u16_at(add(r, 6)?)?);
        let (len, off) = (u16_at(add(r, 8)?)? as usize, u16_at(add(r, 10)?)? as usize);
        let Some(raw) = add(strings, off).and_then(|o| span(o, len)) else { continue };
        let text = match plat {
            0 | 3 => {
                let units: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                String::from_utf16(&units).ok()
            }
            1 if enc == 0 => Some(raw.iter().map(|&b| if b < 128 { b as char } else { '\u{FFFD}' }).collect()),
            _ => None,
        };
        if let Some(t) = text.filter(|t| !t.trim().is_empty()) {
            out.push((plat, enc, lang, id, t));
        }
    }
    Some(out)
}

/// The name stl-sketch's font listing would pick: Windows English Unicode, then Windows
/// English, then Mac English, then any.
fn best_name(names: &[(u16, u16, u16, u16, String)], id: u16) -> Option<String> {
    let of_id = || names.iter().filter(move |n| n.3 == id);
    of_id()
        .find(|n| n.0 == 3 && n.1 == 1 && n.2 == 0x409)
        .or_else(|| of_id().find(|n| n.0 == 3 && n.2 == 0x409))
        .or_else(|| of_id().find(|n| n.0 == 1 && n.2 == 0))
        .or_else(|| of_id().last())
        .map(|n| n.4.clone())
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(b: &[u8]) -> String {
    let mut out = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            if k <= c.len() {
                out.push(B64[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.trim_end_matches('=');
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for ch in s.bytes() {
        let v = B64.iter().position(|&c| c == ch)? as u32;
        acc = acc << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits & 0xff) as u8);
        }
    }
    Some(out)
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