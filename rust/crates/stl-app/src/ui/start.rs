//! The start page (1.x `#empty-state`): shown over the viewport while the project has no steps.

use crate::theme::Tokens;
use egui::{Align2, Area, Color32, CornerRadius, Frame, Id, Margin, Order, Pos2, Rect, RichText, Sense, Stroke, Vec2};

pub const KOFI: &str = "https://ko-fi.com/natterman12";

#[derive(Clone, Debug, PartialEq)]
pub enum StartPick {
    Open,
    Box,
    Recent(String),
    OpenUrl(String),
}

fn bold(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name("bold".into()))
}

/// `recent` are file paths (the 1.x list shows the last six, by file name).
pub fn show(ctx: &egui::Context, tk: &Tokens, workspace: Rect, recent: &[String]) -> Option<StartPick> {
    let mut pick = None;
    let centre = Pos2::new(workspace.center().x, workspace.center().y - 22.0);
    Area::new(Id::new("start-page")).order(Order::Middle).pivot(Align2::CENTER_CENTER).fixed_pos(centre).show(ctx, |ui| {
        Frame::new()
            .fill(tk.panel)
            .stroke(Stroke::new(1.0, tk.line))
            .corner_radius(CornerRadius::same(10))
            .shadow(tk.shadow)
            .inner_margin(Margin { left: 28, right: 28, top: 22, bottom: 16 })
            .show(ui, |ui| {
                ui.set_width((if recent.is_empty() { 340.0 } else { 420.0 } - 56.0f32).min((workspace.width() - 32.0 - 56.0).max(200.0)));
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Open an STL to get started").font(bold(16.0)).color(tk.strong));
                    ui.add_space(6.0);
                    ui.label(RichText::new("Drop STL, 3MF or OBJ files here, or").size(12.0).color(tk.muted));
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        // centred pair of buttons, 8px apart
                        let w = 118.0 + 8.0 + 120.0;
                        ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                        ui.spacing_mut().item_spacing.x = 8.0;
                        if super::dialogs::button(ui, tk, "Open mesh…", true).clicked() {
                            pick = Some(StartPick::Open);
                        }
                        if super::dialogs::button(ui, tk, "Start with a box", false).clicked() {
                            pick = Some(StartPick::Box);
                        }
                    });
                });
                if !recent.is_empty() {
                    ui.add_space(14.0);
                    ui.label(RichText::new("RECENT").size(11.0).color(tk.muted));
                    ui.add_space(4.0);
                    for p in recent.iter().take(6) {
                        let name = p.rsplit(['/', '\\']).next().unwrap_or(p);
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width().min(420.0), 22.0), Sense::click());
                        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
                        if resp.hovered() {
                            ui.painter().rect_filled(r, CornerRadius::same(4), tk.hover);
                        }
                        let g = ui.painter().layout(name.to_owned(), egui::FontId::proportional(12.0), tk.accent, r.width() - 16.0);
                        ui.painter().galley(Pos2::new(r.min.x + 8.0, r.center().y - g.size().y / 2.0), g, tk.accent);
                        if resp.on_hover_text(p.as_str()).clicked() {
                            pick = Some(StartPick::Recent(p.clone()));
                        }
                    }
                }
                ui.add_space(18.0);
                let top = ui.cursor().min.y;
                ui.painter().hline(ui.max_rect().x_range(), top, Stroke::new(1.0, tk.divider));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.label(RichText::new("Free and open source · ").size(11.5).color(tk.muted));
                    let r = ui.add(egui::Label::new(RichText::new("Support STL Editor ").size(11.5).color(tk.muted)).sense(Sense::click()));
                    ui.add_space(3.0);
                    ui.label(RichText::new("♥").size(11.5).color(tk.heart));
                    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Link, true, "Support STL Editor"));
                    if r.clicked() {
                        pick = Some(StartPick::OpenUrl(KOFI.into()));
                    }
                });
            });
    });
    let _ = Color32::WHITE;
    pick
}
