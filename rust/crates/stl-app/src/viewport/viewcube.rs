//! 1.x's "view cube": the small toolbar at the viewport's top right (`#viewcube` in
//! `web/index.html`, styled in `web/css/app.css`): the standard views, Fit, and Section.

use egui::{Align2, CornerRadius, FontId, Rect, Sense, Stroke, Vec2};

/// `data-view` and label of each button, in 1.x's order.
pub const VIEWS: [(&str, &str); 8] = [
    ("top", "Top"),
    ("front", "Front"),
    ("right", "Right"),
    ("iso", "Iso"),
    ("left", "Left"),
    ("back", "Back"),
    ("bottom", "Bottom"),
    ("fit", "Fit"),
];

pub const FIT_TIP: &str = "Fit all (F)";
pub const SECTION_TIP: &str = "Section view: cut the view open to look inside (X)";

/// What was clicked this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewCubeOut {
    pub view: Option<&'static str>,
    pub section_clicked: bool,
}

/// Draw the toolbar inside `viewport`, `8 + right_inset` points from its right edge (1.x moves
/// it left of an open tool panel: `right: 306px`).
pub fn show(ui: &mut egui::Ui, viewport: Rect, right_inset: f32, section_on: bool, dark: bool) -> ViewCubeOut {
    let mut out = ViewCubeOut::default();
    let t = crate::theme::tokens(dark);
    // #viewcube button: font-size 11px, line-height 18px, padding 2px 7px; 1px gaps, 2px padding
    let font = FontId::proportional(11.0);
    let bold = FontId::new(11.0, egui::FontFamily::Proportional);
    let (text, strong, hover, line, accent) = (t.text, t.strong, t.hover, t.line, t.accent);
    let painter = ui.painter_at(viewport);
    let width = |s: &str, f: &FontId| painter.layout_no_wrap(s.to_owned(), f.clone(), text).size().x + 14.0;
    let section_w = width("Section", &font) + 3.0;
    let widths: Vec<f32> = VIEWS.iter().map(|(k, l)| width(l, if *k == "fit" { &bold } else { &font })).collect();
    let total = widths.iter().sum::<f32>() + (VIEWS.len() as f32) * 1.0 + section_w + 4.0;
    let h = 18.0 + 4.0;
    let max = egui::pos2(viewport.max.x - 8.0 - right_inset, viewport.min.y + 8.0);
    let bar = Rect::from_min_size(egui::pos2(max.x - total, max.y), Vec2::new(total, h));
    let radius = CornerRadius::same(t.radius.round() as u8);
    painter.add(t.shadow.as_shape(bar, radius));
    painter.rect(bar, radius, t.panel, Stroke::new(1.0, line), egui::StrokeKind::Inside);
    let mut x = bar.min.x + 2.0;
    for (i, (key, label)) in VIEWS.iter().enumerate() {
        let r = Rect::from_min_size(egui::pos2(x, bar.min.y + 2.0), Vec2::new(widths[i], 18.0));
        let id = ui.scope_id().with(("viewcube", *key));
        let resp = ui.interact(r, id, Sense::click());
        let resp = if *key == "fit" { resp.on_hover_text(FIT_TIP) } else { resp };
        let hovered = resp.hovered();
        if hovered {
            painter.rect_filled(r, CornerRadius::same(4), hover);
        }
        let (color, f) = if *key == "fit" { (accent, &bold) } else if hovered { (strong, &font) } else { (text, &font) };
        painter.text(r.center(), Align2::CENTER_CENTER, *label, f.clone(), color);
        if *key == "fit" {
            // font-weight 600: a second pass a hair to the right
            painter.text(r.center() + Vec2::new(0.4, 0.0), Align2::CENTER_CENTER, *label, f.clone(), color);
        }
        if resp.clicked() {
            out.view = Some(key);
        }
        x += widths[i] + 1.0;
    }
    // #btn-section: 3px left margin and a divider line
    x += 2.0;
    painter.vline(x, (bar.min.y + 2.0)..=(bar.max.y - 2.0), Stroke::new(1.0, t.divider));
    let r = Rect::from_min_size(egui::pos2(x + 1.0, bar.min.y + 2.0), Vec2::new(section_w - 3.0, 18.0));
    let resp = ui.interact(r, ui.scope_id().with("viewcube-section"), Sense::click()).on_hover_text(SECTION_TIP);
    let rr = CornerRadius { nw: 0, sw: 0, ne: 4, se: 4 };
    let color = if section_on {
        // .on: var(--sel) behind var(--sel-text)
        painter.rect_filled(r, rr, t.sel);
        t.sel_text
    } else if resp.hovered() {
        painter.rect_filled(r, rr, hover);
        strong
    } else {
        text
    };
    painter.text(r.center(), Align2::CENTER_CENTER, "Section", font, color);
    out.section_clicked = resp.clicked();
    out
}
