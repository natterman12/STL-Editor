//! Popup menus (1.x `#menu`): the top menus, ribbon dropdowns, the tool search results and context
//! menus (the browser's and timeline's right-click menus can use [`OpenMenu`] too).

use crate::icons;
use crate::theme::Tokens;
use egui::{Align2, Area, CornerRadius, Frame, Id, Margin, Order, Pos2, Rect, Sense, Stroke, Ui, Vec2};

#[derive(Clone, Debug, PartialEq)]
pub enum MenuItem {
    Sep,
    Hdr(String),
    Item { id: String, label: String, icon: Option<String>, key: Option<String>, dis: bool, title: Option<String> },
}

impl MenuItem {
    pub fn item(id: impl Into<String>, label: impl Into<String>) -> Self {
        MenuItem::Item { id: id.into(), label: label.into(), icon: None, key: None, dis: false, title: None }
    }
    pub fn key(mut self, k: impl Into<String>) -> Self {
        if let MenuItem::Item { key, .. } = &mut self {
            *key = Some(k.into());
        }
        self
    }
    pub fn icon(mut self, ic: impl Into<String>) -> Self {
        if let MenuItem::Item { icon, .. } = &mut self {
            *icon = Some(ic.into());
        }
        self
    }
    pub fn disabled(mut self, d: bool) -> Self {
        if let MenuItem::Item { dis, .. } = &mut self {
            *dis = d;
        }
        self
    }
    pub fn title(mut self, t: impl Into<String>) -> Self {
        if let MenuItem::Item { title, .. } = &mut self {
            *title = Some(t.into());
        }
        self
    }
}

#[derive(Clone, Debug)]
pub struct OpenMenu {
    /// Who opened it (`"top:file"`, `"drop:plane"`, `"search"`, `"more"`, a context menu's name).
    pub tag: String,
    pub items: Vec<MenuItem>,
    pub pos: Pos2,
    /// The highlighted row (index into `items`); the tool search moves it with the arrow keys.
    pub hot: Option<usize>,
    /// Clicks inside these rects (the button that opened it) don't close the menu.
    pub keep: Vec<Rect>,
    pub opened_at: Option<u64>,
}

impl OpenMenu {
    pub fn new(tag: impl Into<String>, items: Vec<MenuItem>, pos: Pos2) -> Self {
        OpenMenu { tag: tag.into(), items, pos, hot: None, keep: Vec::new(), opened_at: None }
    }
    pub fn keep(mut self, r: Rect) -> Self {
        self.keep.push(r);
        self
    }
    pub fn hot(mut self, h: Option<usize>) -> Self {
        self.hot = h;
        self
    }
}

pub enum MenuResult {
    None,
    /// A row was chosen: `(tag, item id)`.
    Pick(String, String),
    Close,
}

/// Draw the menu; call every frame while one is open.
pub fn show(ctx: &egui::Context, tk: &Tokens, m: &mut OpenMenu) -> MenuResult {
    let mut result = MenuResult::None;
    let frame_no = ctx.cumulative_pass_nr();
    let first = *m.opened_at.get_or_insert(frame_no);
    let area = Area::new(Id::new("stl-menu")).order(Order::Foreground).fixed_pos(m.pos).constrain(true).show(ctx, |ui| {
        Frame::new()
            .fill(tk.panel)
            .stroke(Stroke::new(1.0, tk.line))
            .corner_radius(CornerRadius::same(tk.radius as u8))
            .shadow(tk.shadow)
            .inner_margin(Margin::same(4))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                ui.set_min_width(200.0);
                rows(ui, tk, m, &mut result);
            });
    });
    let rect = area.response.rect;
    if matches!(result, MenuResult::None) {
        let (pressed, pos, esc) = ctx.input(|i| (i.pointer.any_pressed(), i.pointer.interact_pos(), i.key_pressed(egui::Key::Escape)));
        let outside = pressed && pos.is_some_and(|p| !rect.contains(p) && !m.keep.iter().any(|k| k.contains(p)));
        if (outside && frame_no > first) || (esc && m.tag != "search") {
            result = MenuResult::Close;
        }
    }
    result
}

fn rows(ui: &mut Ui, tk: &Tokens, m: &mut OpenMenu, result: &mut MenuResult) {
    let n = m.items.len();
    for i in 0..n {
        match m.items[i].clone() {
            MenuItem::Sep => {
                let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 9.0), Sense::hover());
                ui.painter().hline(r.shrink2(Vec2::new(2.0, 0.0)).x_range(), r.center().y, Stroke::new(1.0, tk.divider));
            }
            MenuItem::Hdr(text) => {
                let g = ui.painter().layout_no_wrap(text.to_uppercase(), egui::FontId::proportional(11.0), tk.muted);
                let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), g.size().y + 8.0), Sense::hover());
                ui.painter().galley(r.min + Vec2::new(10.0, 4.0), g, tk.muted);
            }
            MenuItem::Item { id, label, icon, key, dis, title } => {
                // "✓ Light": the fonts have no check glyph, so the mark is drawn
                let (checked, label) = match label.strip_prefix("✓ ") {
                    Some(l) => (true, l.to_owned()),
                    None => (false, label),
                };
                let key_w = key.as_ref().map(|k| ui.painter().layout_no_wrap(k.clone(), egui::FontId::proportional(11.5), tk.muted).size().x + 18.0).unwrap_or(0.0);
                let label_g = ui.painter().layout_no_wrap(label.clone(), egui::FontId::proportional(12.0), tk.text);
                let w = (10.0 + if checked { 12.0 } else { 0.0 } + if icon.is_some() { 25.0 } else { 0.0 } + label_g.size().x + key_w + 12.0).max(ui.available_width());
                let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 26.0), if dis { Sense::hover() } else { Sense::click() });
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, !dis, label.clone()));
                let hot = !dis && (resp.hovered() || m.hot == Some(i));
                let (fg, k_fg) = if hot { (tk.sel_text, tk.sel_text.gamma_multiply(0.8)) } else { (tk.text, tk.muted) };
                let alpha = if dis { 0.4 } else { 1.0 };
                if hot {
                    ui.painter().rect_filled(r, CornerRadius::same(4), tk.sel);
                }
                let mut x = r.min.x + 10.0;
                if checked {
                    let c = Pos2::new(x + 4.0, r.center().y);
                    ui.painter().line(vec![c + Vec2::new(-3.5, 0.0), c + Vec2::new(-1.0, 3.0), c + Vec2::new(3.5, -3.0)], Stroke::new(1.5, fg.gamma_multiply(alpha)));
                    x += 12.0;
                }
                if let Some(ic) = icon {
                    let col = if hot { tk.sel_text } else { tk.accent };
                    let img = icons::icon(ui.ctx(), &ic, 17.0, col.gamma_multiply(alpha));
                    egui::Image::new(img).paint_at(ui, Rect::from_center_size(Pos2::new(x + 8.5, r.center().y), Vec2::splat(17.0)));
                    x += 25.0;
                }
                let lc = fg.gamma_multiply(alpha);
                ui.painter().galley(Pos2::new(x, r.center().y - label_g.size().y / 2.0), label_g, lc);
                if let Some(k) = key {
                    ui.painter().text(Pos2::new(r.max.x - 12.0, r.center().y), Align2::RIGHT_CENTER, k, egui::FontId::proportional(11.5), k_fg.gamma_multiply(alpha));
                }
                if let Some(t) = title {
                    if !t.is_empty() {
                        resp.clone().on_hover_text(t);
                    }
                }
                if resp.hovered() && m.tag != "search" {
                    m.hot = Some(i);
                }
                if resp.clicked() {
                    *result = MenuResult::Pick(m.tag.clone(), id);
                }
            }
        }
    }
}
