//! Toasts (1.x `#toasts`).

use egui::{Context, CornerRadius, RichText};
use web_time::{Duration, Instant};

use crate::engine_link::Level;
use crate::theme::Tokens;

#[derive(Clone, Debug)]
struct Toast {
    text: String,
    level: Level,
    until: Instant,
}

#[derive(Clone, Debug, Default)]
pub struct Toasts {
    queue: Vec<Toast>,
    next_wake: Option<Instant>,
}

impl Toasts {
    pub fn push(&mut self, level: Level, text: String) {
        let dur = if level == Level::Error {
            Duration::from_secs(7)
        } else {
            Duration::from_secs(4)
        };
        let until = Instant::now() + dur;
        self.queue.push(Toast { text, level, until });
        self.next_wake = Some(until);
    }

    /// Drop every toast and any wake they had scheduled.
    pub fn clear(&mut self) {
        self.queue.clear();
        self.next_wake = None;
    }
}

pub fn toast_colours(level: Level, tokens: &Tokens) -> (egui::Color32, egui::Color32) {
    match level {
        // white text: `.toast { color: #fff }`, app.css:378
        Level::Error => (tokens.danger, egui::Color32::WHITE),
        Level::Warning => (tokens.toast_warn, egui::Color32::WHITE),
        // 1.x `.toast` has no separate success rule; success uses default toast background.
        Level::Success | Level::Info => (tokens.toast, egui::Color32::WHITE),
    }
}

/// Draw stacked toasts at the bottom-right of the window.
pub fn show(ctx: &Context, tokens: &Tokens, state: &mut Toasts) {
    let now = Instant::now();
    state.queue.retain(|t| t.until > now);
    // The soonest toast still showing, not the one most recently pushed.
    state.next_wake = state.queue.iter().map(|t| t.until).min();
    if state.queue.is_empty() {
        return;
    }
    if let Some(wake) = state.next_wake {
        if wake > now {
            ctx.request_repaint_after(wake - now);
        }
    }

    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::new("toasts"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(screen.right() - 14.0, screen.bottom() - 56.0))
        .pivot(egui::Align2::RIGHT_BOTTOM)
        .show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                for (i, t) in state.queue.iter().enumerate() {
                    let (bg, fg) = toast_colours(t.level, tokens);
                    egui::Frame::NONE
                        .fill(bg)
                        .corner_radius(CornerRadius::same(tokens.radius as u8))
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .stroke(egui::Stroke::new(1.0, bg.gamma_multiply(1.1)))
                        .show(ui, |ui| {
                            ui.push_id(("toast", i), |ui| {
                                ui.label(RichText::new(&t.text).color(fg).size(12.5));
                            });
                        });
                }
            });
        });
}
