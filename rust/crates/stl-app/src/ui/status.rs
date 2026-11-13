//! Status line in the timeline bar (`#status`).

use egui::{RichText, Ui};

use crate::theme::Tokens;

/// Draw the muted status text at the end of the timeline bar (ellipsis via layout, not byte truncation).
pub fn show(ui: &mut Ui, tokens: &Tokens, text: &str) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.set_max_width(420.0);
        ui.push_id("status_line", |ui| {
            ui.add(egui::Label::new(RichText::new(text).color(tokens.muted).size(11.5)).truncate());
        });
    });
}
