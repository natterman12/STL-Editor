//! Measure (1.x `startMeasure`, `measureClick`) and Properties (`startInfo`, `showInfo`). A child
//! of `app.rs`; the panels are `ui/measure_panel.rs` and `ui/info_panel.rs`.
//!
//! Both use the whole part: the section cut changes only what is seen, and the engine's
//! `BodyInfo` answer is about the body, not the picture.

use egui::Color32;

use super::*;
use crate::ui::measure_panel::PanelAct;

/// `0x1f6fd1`: the markers and the line.
const BLUE: Color32 = Color32::from_rgb(0x1f, 0x6f, 0xd1);
/// The overlay tag of the markers and the line.
const TAG: &str = "measure";

/// The Measure panel: its picks.
#[derive(Default)]
pub(super) struct MeasureState {
    open: bool,
    /// The points clicked: one, or two (a third click starts again).
    pts: Vec<[f64; 3]>,
}

/// The Properties panel: the body shown and the engine's answer.
#[derive(Default)]
pub(super) struct InfoState {
    open: bool,
    /// The body shown.
    body: Option<String>,
    /// Its `BodyView::revision` when asked: a later one asks again (the model changed).
    rev: u64,
    /// The engine's answer for `body` (`Body.info()` plus the name).
    info: Option<serde_json::Value>,
    /// The `seq` of the latest request; an older answer is dropped.
    seq: u64,
    /// The first selected body the last time the panel looked: a new one is shown at once.
    seen_sel: Option<String>,
}

impl StlApp {
    /// The ribbon's Measure button.
    pub fn open_measure(&mut self) {
        self.leave_modes();
        self.measure.open = true;
        self.measure.pts.clear();
        self.enter_mode("measure", "Click two points to measure the distance between them");
    }

    /// The ribbon's Properties button; `id`: show that body at once (else the selected one).
    pub fn open_info(&mut self, id: Option<String>) {
        self.leave_modes();
        self.info.open = true;
        let id = id.or_else(|| self.shell.selection.first().cloned());
        self.info.seen_sel = self.shell.selection.first().cloned();
        self.enter_mode("info", if id.is_some() { "" } else { "Click a body" });
        if let Some(id) = id {
            self.request_info(&id);
        }
    }

    /// Another tool or mode is going away: close what was open before this one starts.
    fn leave_modes(&mut self) {
        self.close_tool(false);
        self.close_section();
        self.close_measure();
    }

    fn enter_mode(&mut self, key: &str, hint: &str) {
        self.shell.hint = hint.to_owned();
        self.shell.active_tool = Some(key.to_owned());
        self.viewport.set_viewcube_inset(crate::tools::panel::VIEWCUBE_INSET);
    }

    /// Ask the engine about a body (`showInfo`).
    fn request_info(&mut self, id: &str) {
        self.info.seq += 1;
        self.info.body = Some(id.to_owned());
        self.info.rev = self.bodies.iter().find(|b| b.id == id).map_or(0, |b| b.revision);
        self.link.send(Command::BodyInfo { seq: self.info.seq, id: id.to_owned() });
    }

    /// The answer to `Command::BodyInfo`.
    pub(super) fn on_body_info(&mut self, seq: u64, result: Result<serde_json::Value, String>) {
        if !self.info.open || seq != self.info.seq {
            return; // closed, or an older question
        }
        match result {
            Ok(v) => self.info.info = Some(v),
            Err(e) => {
                self.info.info = None;
                self.info.body = None;
                self.toasts.push(Level::Error, e);
            }
        }
    }

    /// A click in the viewport. True when Measure or Properties used it.
    pub(super) fn measure_click(&mut self, pos: Pos2) -> bool {
        if !self.measure.open && !self.info.open {
            return false;
        }
        let Some(hit) = self.viewport.pick(pos, true, false) else { return true };
        if self.info.open {
            // `mode === 'info'`: a body is selected and shown
            if let crate::viewport::pick::Hit::Body { id, .. } = hit {
                self.shell.selection = vec![id.clone()];
                self.viewport.set_selection(&self.shell.selection);
                self.info.seen_sel = Some(id.clone());
                self.shell.hint.clear();
                self.request_info(&id);
            }
            return true;
        }
        let crate::viewport::pick::Hit::Body { point, .. } = hit else { return true };
        if self.measure.pts.len() >= 2 {
            self.measure.pts.clear();
            self.viewport.clear_overlay(TAG);
        }
        self.measure.pts.push(point);
        self.viewport.add_marker(point, BLUE, TAG, None);
        if let [a, b] = self.measure.pts[..] {
            self.viewport.add_line(a, b, BLUE, TAG);
        }
        true
    }

    /// Draw the panels (each frame).
    pub(super) fn measure_frame(&mut self, ctx: &egui::Context, tk: &Tokens, ws: Rect) {
        if !self.measure.open && !self.info.open {
            return;
        }
        // Escape and Enter close the panel like Cancel and Done (`panelCancel`, `panelOk`)
        let typing = ctx.egui_wants_keyboard_input();
        let key = !typing && ctx.input(|i| i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Enter));
        let inch = self.shell.inch;
        let act = if self.measure.open {
            crate::ui::measure_panel::show(ctx, tk, ws, &self.measure.pts, inch)
        } else {
            self.refresh_info();
            crate::ui::info_panel::show(ctx, tk, ws, self.info.info.as_ref(), inch)
        };
        if act == PanelAct::Close || key {
            self.close_measure();
        }
    }

    /// Properties follows the selection and the model: a body picked in the browser is shown, and a
    /// body that changed (or went) is asked about again.
    fn refresh_info(&mut self) {
        let sel = self.shell.selection.first().cloned();
        if sel != self.info.seen_sel {
            self.info.seen_sel = sel.clone();
            if let Some(id) = sel {
                self.shell.hint.clear();
                self.request_info(&id);
                return;
            }
        }
        let Some(id) = self.info.body.clone() else { return };
        match self.bodies.iter().find(|b| b.id == id) {
            Some(b) if b.revision != self.info.rev => self.request_info(&id),
            Some(_) => {}
            None => {
                self.info.body = None;
                self.info.info = None;
                self.info.seq += 1; // an answer still on its way is out of date
            }
        }
    }

    /// Close both panels (another tool or mode opened, or Done).
    pub(super) fn close_measure(&mut self) {
        if !self.measure.open && !self.info.open {
            return;
        }
        if self.measure.open {
            self.viewport.clear_overlay(TAG);
        }
        self.measure = Default::default();
        self.info = InfoState { seq: self.info.seq + 1, ..Default::default() };
        if self.shell.active_tool.as_deref().is_some_and(|k| k == "measure" || k == "info") {
            self.shell.active_tool = None;
        }
        self.shell.hint.clear();
        self.viewport.set_viewcube_inset(0.0);
    }

    // ------------------------------------------------------------------ for the tests

    /// Measure is open.
    pub fn measure_open(&self) -> bool {
        self.measure.open
    }

    /// Properties is open.
    pub fn info_open(&self) -> bool {
        self.info.open
    }

    /// The points Measure holds, in millimetres.
    pub fn measure_points(&self) -> &[[f64; 3]] {
        &self.measure.pts
    }

    /// What the Measure panel shows: (label, value) rows, in the current units.
    pub fn measure_readout(&self) -> Vec<(String, String)> {
        crate::ui::measure_panel::measure_rows(&self.measure.pts, self.shell.inch).into_iter().map(|r| (r.label, r.value)).collect()
    }

    /// What the Properties panel shows for the body: its name and (label, value) rows, in the
    /// current units. `None` while no body is shown (or its answer is on its way).
    pub fn info_readout(&self) -> Option<(String, Vec<(String, String)>)> {
        let (name, rows) = crate::ui::info_panel::info_rows(self.info.info.as_ref()?, self.shell.inch)?;
        Some((name, rows.into_iter().map(|(r, _)| (r.label, r.value)).collect()))
    }
}
