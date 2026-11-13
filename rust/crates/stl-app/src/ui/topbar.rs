//! The title bar (1.x `#topbar`): brand, File / Edit / View / Help, quick buttons, document name
//! and the tool search box.

use crate::icons;
use crate::theme::Tokens;
use egui::{Align2, Color32, CornerRadius, FontFamily, FontId, Id, Key, Modifiers, Pos2, Rangef, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

pub const HEIGHT: f32 = 30.0;
pub const MENUS: [(&str, &str); 4] = [("file", "File"), ("edit", "Edit"), ("view", "View"), ("help", "Help")];
pub const SEARCH_ID: &str = "tool-search";

/// What the title bar asks for this frame.
pub struct TopbarOut {
    /// A menu button was clicked: its key and rect.
    pub menu: Option<(&'static str, Rect)>,
    /// A quick button: `"save"`, `"undo"`, `"redo"`, `"insert"`, `"export"` (with its rect).
    pub quick: Option<(&'static str, Rect)>,
    /// The search text changed.
    pub search_changed: bool,
    pub search_rect: Rect,
    /// Arrow keys / Enter / Escape pressed in the search box.
    pub search_key: Option<Key>,
}

impl TopbarOut {
    pub fn empty() -> Self {
        TopbarOut { menu: None, quick: None, search_changed: false, search_rect: Rect::NOTHING, search_key: None }
    }
}

pub struct TopbarState<'a> {
    pub title: &'a str,
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub open_menu: Option<&'static str>,
    pub query: &'a mut String,
    pub focus_search: bool,
}

fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}

pub fn show(ui: &mut Ui, tk: &Tokens, st: &mut TopbarState) -> TopbarOut {
    let mut out = TopbarOut::empty();
    let bar = ui.available_rect_before_wrap();
    let bar = Rect::from_min_size(bar.min, Vec2::new(bar.width(), HEIGHT));
    ui.allocate_rect(bar, Sense::hover());
    ui.painter().rect_filled(bar, 0.0, tk.chrome);
    let cy = bar.center().y;
    let mut x = bar.min.x + 8.0;

    // brand: the logo is a 135-degree gradient square (#5fa8ff, #1f6fd1, #0e3f7d)
    let logo = Rect::from_center_size(Pos2::new(x + 8.0, cy), Vec2::splat(16.0));
    let mut mesh = egui::Mesh::default();
    let (a, m, z) = (Color32::from_rgb(0x5f, 0xa8, 0xff), Color32::from_rgb(0x1f, 0x6f, 0xd1), Color32::from_rgb(0x0e, 0x3f, 0x7d));
    for (p, c) in [(logo.left_top(), a), (logo.right_top(), m), (logo.right_bottom(), z), (logo.left_bottom(), m)] {
        mesh.colored_vertex(p, c);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(mesh);
    ui.painter().rect_stroke(logo, CornerRadius::same(4), Stroke::new(1.0, Color32::from_white_alpha(64)), StrokeKind::Inside);
    x += 16.0 + 7.0;
    let g = ui.painter().layout_no_wrap("STL Editor".into(), bold(12.0), tk.strong);
    let gw = g.size().x;
    ui.painter().galley(Pos2::new(x, cy - g.size().y / 2.0), g, tk.strong);
    x += gw + 10.0;

    // menus
    for (k, l) in MENUS {
        let tw = ui.painter().layout_no_wrap(l.into(), FontId::proportional(12.0), tk.chrome_text).size().x;
        let r = Rect::from_min_size(Pos2::new(x, cy - 12.0), Vec2::new(tw + 18.0, 24.0));
        let resp = ui.interact(r, ui.id().with(("menu-btn", k)), Sense::click());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, l));
        let open = st.open_menu == Some(k);
        let col = if open || resp.hovered() {
            ui.painter().rect_filled(r, CornerRadius::same(tk.radius_sm as u8), tk.chrome_hover);
            tk.strong
        } else {
            tk.chrome_text
        };
        ui.painter().text(r.center(), Align2::CENTER_CENTER, l, FontId::proportional(12.0), col);
        if resp.clicked() {
            out.menu = Some((k, r));
        }
        x += r.width() + 1.0;
    }
    x += 9.0;

    // quick buttons
    let quick: [(&'static str, &str, &str, bool); 5] = [
        ("save", "save", "Save (Ctrl+S)", true),
        ("undo", "undo", "Undo (Ctrl+Z)", st.can_undo),
        ("redo", "redo", "Redo (Ctrl+Y)", st.can_redo),
        ("insert", "import", "Insert Mesh (Ctrl+I)", true),
        ("export", "export", "Export", true),
    ];
    for (n, (key, ic, tip, enabled)) in quick.iter().enumerate() {
        if n == 3 {
            ui.painter().vline(x + 4.5, Rangef::new(cy - 8.0, cy + 8.0), Stroke::new(1.0, tk.line));
            x += 9.0;
        }
        let r = Rect::from_min_size(Pos2::new(x, cy - 12.0), Vec2::new(26.0, 24.0));
        let resp = ui.interact(r, ui.id().with(("quick", *key)), if *enabled { Sense::click() } else { Sense::hover() });
        let label = tip.split(" (").next().unwrap_or(tip);
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, *enabled, label));
        let hot = *enabled && resp.hovered();
        if hot {
            ui.painter().rect_filled(r, CornerRadius::same(tk.radius_sm as u8), tk.chrome_hover);
        }
        let mut col = if hot { tk.strong } else { tk.chrome_text };
        if !*enabled {
            col = col.gamma_multiply(0.35);
        }
        let img = icons::icon(ui.ctx(), ic, 16.0, col);
        egui::Image::new(img).paint_at(ui, Rect::from_center_size(r.center(), Vec2::splat(16.0)));
        if *enabled && resp.on_hover_text(*tip).clicked() {
            out.quick = Some((key, r));
        }
        x += 26.0 + 1.0;
    }
    x += 9.0;

    // search box (right)
    let sid = Id::new(SEARCH_ID);
    let focused = ui.memory(|m| m.has_focus(sid));
    let sw = if focused { 260.0 } else { 210.0 };
    let sr = Rect::from_min_size(Pos2::new(bar.max.x - 8.0 - sw, cy - 11.0), Vec2::new(sw, 22.0));
    out.search_rect = sr;

    // document name
    let title = format!("{}{}", st.title, if st.dirty { " •" } else { "" });
    let maxw = (sr.min.x - 10.0 - x).clamp(0.0, 320.0);
    if maxw > 30.0 {
        let g = ui.painter().layout(title, FontId::proportional(12.0), tk.strong, maxw - 20.0);
        let w = g.size().x + 20.0;
        let r = Rect::from_min_size(Pos2::new(x, cy - 10.0), Vec2::new(w, 20.0));
        ui.painter().rect_filled(r, CornerRadius::same(5), tk.doc_tab);
        ui.painter().galley(Pos2::new(r.min.x + 10.0, cy - g.size().y / 2.0), g, tk.strong);
    }

    // keys the search box owns while it has focus (before the text field sees them)
    // egui drops focus on Escape before this runs, so the box also counts as focused if it was last frame
    let was_focused = ui.data(|d| d.get_temp::<bool>(sid)).unwrap_or(false);
    if focused || was_focused {
        for k in [Key::ArrowDown, Key::ArrowUp, Key::Enter, Key::Escape] {
            if ui.input_mut(|i| i.consume_key(Modifiers::NONE, k)) {
                out.search_key = Some(k);
            }
        }
    }
    ui.painter().rect(sr, CornerRadius::same(11), tk.input, Stroke::new(1.0, if focused { tk.accent } else { tk.input_line }), StrokeKind::Inside);
    // the magnifier of the 1.x background image: 12px, circle (11,11) r6 and a handle, stroke 2 of 24
    let o = Pos2::new(sr.min.x + 8.0, sr.center().y - 6.0);
    let ms = Stroke::new(1.0, tk.muted);
    ui.painter().circle_stroke(o + Vec2::splat(5.5), 3.0, ms);
    ui.painter().line_segment([o + Vec2::splat(7.75), o + Vec2::splat(10.0)], ms);
    let inner = Rect::from_min_max(Pos2::new(sr.min.x + 25.0, sr.min.y + 1.0), Pos2::new(sr.max.x - 8.0, sr.max.y - 1.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::left_to_right(egui::Align::Center)));
    let te = egui::TextEdit::singleline(st.query)
        .id(sid)
        .hint_text(egui::RichText::new("Search tools  Ctrl+K").size(11.5).color(tk.muted))
        .font(FontId::proportional(11.5))
        .frame(egui::Frame::NONE)
        .desired_width(inner.width())
        .text_color(tk.text);
    let resp = child.add(te);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, "Search tools"));
    if st.focus_search {
        resp.request_focus();
    }
    let has_focus = resp.has_focus();
    ui.data_mut(|d| d.insert_temp(sid, has_focus));
    out.search_changed = resp.changed();
    out
}
