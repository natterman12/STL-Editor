//! Busy overlay with optional progress and Cancel (engine cancel during rebuild).

use egui::{Context, CornerRadius, RichText};
use crate::engine_link::Command;
use crate::theme::Tokens;

#[derive(Clone, Debug, Default)]
pub struct Busy {
    pub busy: bool,
    pub label: String,
    pub done: usize,
    pub total: usize,
    /// egui time of the first frame drawn while busy; the overlay waits `DELAY` after that.
    since: Option<f64>,
}

/// 1.x shows the overlay only if the work takes longer than this (`web/js/api.js:24`: 350 ms).
const DELAY: f64 = 0.35;

impl Busy {
    pub fn set_busy(&mut self, on: bool) {
        self.busy = on;
        if !on {
            self.since = None;
        }
    }

    pub fn set_progress(&mut self, done: usize, total: usize, label: &str) {
        self.done = done;
        self.total = total;
        self.label = label.to_string();
        self.set_busy(true);
    }
}

/// Full-window scrim, spinner and Cancel when a rebuild is in progress.
pub fn show(ctx: &Context, tokens: &Tokens, state: &mut Busy, commands: &mut Vec<Command>) {
    if !state.busy {
        return;
    }
    let now = ctx.input(|i| i.time);
    let since = *state.since.get_or_insert(now);
    if now - since < DELAY {
        // nothing is drawn yet, so ask for the frame that will draw the overlay
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(DELAY - (now - since)));
        return;
    }
    ctx.request_repaint();

    let screen = ctx.content_rect();
    let display = if state.total > 0 {
        format!("{}\n{} / {}", state.label, state.done, state.total)
    } else if state.label.is_empty() {
        "Working…".to_string()
    } else {
        state.label.clone()
    };

    egui::Area::new(egui::Id::unique("busy_overlay"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.left_top())
        .show(ctx, |ui| {
            let (rect, _response) = ui.allocate_exact_size(screen.size(), egui::Sense::hover());
            ui.painter().rect_filled(rect, 0.0, tokens.busy_scrim);
            let center = rect.center();

            let spin_r = 17.0;
            let t = ctx.input(|i| i.time) as f32;
            let n = 32;
            for k in 0..n {
                let a0 = t * 4.5 + (k as f32 / n as f32) * std::f32::consts::TAU;
                let a1 = a0 + 0.15;
                let col = if k < 8 { tokens.accent } else { tokens.spin_track };
                let p0 = center + spin_r * egui::vec2(a0.cos(), a0.sin());
                let p1 = center + spin_r * egui::vec2(a1.cos(), a1.sin());
                ui.painter().line_segment([p0, p1], egui::Stroke::new(3.0, col));
            }

            egui::Area::new(egui::Id::unique("busy_pill"))
                .order(egui::Order::Foreground)
                .fixed_pos(center + egui::vec2(0.0, 40.0))
                .pivot(egui::Align2::CENTER_CENTER)
                .show(ctx, |ui| {
                    egui::Frame::NONE
                        .fill(tokens.panel)
                        .corner_radius(CornerRadius::same(14))
                        .inner_margin(egui::Margin::symmetric(12, 6))
                        .stroke(egui::Stroke::new(1.0, tokens.line))
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                ui.push_id("busy_msg", |ui| {
                                    ui.label(RichText::new(&display).color(tokens.text).size(12.5));
                                });
                                if ui.button("Cancel").clicked() {
                                    commands.push(Command::Cancel);
                                    state.set_busy(false);
                                }
                            });
                        });
                });
        });
}
