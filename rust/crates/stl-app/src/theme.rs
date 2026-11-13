//! Colours and the egui style, ported from 1.x's web/css/app.css.
//!
//! The token block between BEGIN/END GENERATED is written by `assets/gen_theme.mjs` from the CSS
//! (`:root` is the dark theme, `html[data-theme=light]` the light one; tokens the light block does
//! not set fall back to the dark value, as in CSS). Re-run the script when the CSS changes; the
//! test `tokens_match_css` fails if the two drift apart.

use egui::epaint::Shadow;
use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, TextStyle, Vec2};

// BEGIN GENERATED (assets/gen_theme.mjs from web/css/app.css; do not edit by hand)
/// Every colour token of web/css/app.css, same names in snake_case.
#[derive(Clone, Debug, PartialEq)]
pub struct Tokens {
    pub chrome: Color32,
    pub chrome_text: Color32,
    pub chrome_hover: Color32,
    pub doc_tab: Color32,
    pub ribbon: Color32,
    pub ribbon_line: Color32,
    pub divider: Color32,
    pub bg: Color32,
    pub panel: Color32,
    pub panel_2: Color32,
    pub hover: Color32,
    pub line: Color32,
    pub line_2: Color32,
    pub input: Color32,
    pub input_line: Color32,
    pub text: Color32,
    pub strong: Color32,
    pub muted: Color32,
    pub label: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub primary: Color32,
    pub primary_hover: Color32,
    pub sel: Color32,
    pub sel_text: Color32,
    pub sel_line: Color32,
    pub sketch: Color32,
    pub sketch_fill: Color32,
    pub sketch_text: Color32,
    pub ok: Color32,
    pub danger: Color32,
    pub warn: Color32,
    pub err_bg: Color32,
    pub err_text: Color32,
    pub warn_bg: Color32,
    pub warn_text: Color32,
    pub info_bg: Color32,
    pub info_text: Color32,
    pub good_bg: Color32,
    pub good_text: Color32,
    pub ent_sel: Color32,
    pub chip: Color32,
    pub kbd: Color32,
    pub expr: Color32,
    pub toast: Color32,
    pub toast_warn: Color32,
    pub scrim: Color32,
    pub busy_scrim: Color32,
    pub spin_track: Color32,
    pub float: Color32,
    pub dim_ink: Color32,
    pub dim_text: Color32,
    pub dim_ref: Color32,
    pub dim_expr: Color32,
    pub dim_sel: Color32,
    pub dim_sel_text: Color32,
    pub dim_sel_line: Color32,
    pub dim_bad: Color32,
    pub dim_bad_line: Color32,
    pub dim_preview: Color32,
    pub dim_preview_line: Color32,
    pub dim_hover: Color32,
    pub cg_bg: Color32,
    pub cg_text: Color32,
    pub cg_line: Color32,
    pub cg_bad_bg: Color32,
    pub cg_bad_text: Color32,
    pub cg_bad_line: Color32,
    pub def_under: Color32,
    pub def_full: Color32,
    pub def_over: Color32,
    pub heart: Color32,
    /// `--shadow` (menus, panels) and `--shadow-lg` (dialogs).
    pub shadow: Shadow,
    pub shadow_lg: Shadow,
    /// `--radius` and `--radius-sm`, in points (CSS px).
    pub radius: f32,
    pub radius_sm: f32,
}

/// CSS names of the colour tokens, in the same order as the fields.
pub const COLOUR_NAMES: &[&str] = &["chrome", "chrome-text", "chrome-hover", "doc-tab", "ribbon", "ribbon-line", "divider", "bg", "panel", "panel-2", "hover", "line", "line-2", "input", "input-line", "text", "strong", "muted", "label", "accent", "accent-soft", "primary", "primary-hover", "sel", "sel-text", "sel-line", "sketch", "sketch-fill", "sketch-text", "ok", "danger", "warn", "err-bg", "err-text", "warn-bg", "warn-text", "info-bg", "info-text", "good-bg", "good-text", "ent-sel", "chip", "kbd", "expr", "toast", "toast-warn", "scrim", "busy-scrim", "spin-track", "float", "dim-ink", "dim-text", "dim-ref", "dim-expr", "dim-sel", "dim-sel-text", "dim-sel-line", "dim-bad", "dim-bad-line", "dim-preview", "dim-preview-line", "dim-hover", "cg-bg", "cg-text", "cg-line", "cg-bad-bg", "cg-bad-text", "cg-bad-line", "def-under", "def-full", "def-over", "heart"];

impl Tokens {
    /// A colour token by its CSS name (`"panel-2"`, without the leading `--`).
    pub fn colour(&self, name: &str) -> Option<Color32> {
        Some(match name {
            "chrome" => self.chrome,
            "chrome-text" => self.chrome_text,
            "chrome-hover" => self.chrome_hover,
            "doc-tab" => self.doc_tab,
            "ribbon" => self.ribbon,
            "ribbon-line" => self.ribbon_line,
            "divider" => self.divider,
            "bg" => self.bg,
            "panel" => self.panel,
            "panel-2" => self.panel_2,
            "hover" => self.hover,
            "line" => self.line,
            "line-2" => self.line_2,
            "input" => self.input,
            "input-line" => self.input_line,
            "text" => self.text,
            "strong" => self.strong,
            "muted" => self.muted,
            "label" => self.label,
            "accent" => self.accent,
            "accent-soft" => self.accent_soft,
            "primary" => self.primary,
            "primary-hover" => self.primary_hover,
            "sel" => self.sel,
            "sel-text" => self.sel_text,
            "sel-line" => self.sel_line,
            "sketch" => self.sketch,
            "sketch-fill" => self.sketch_fill,
            "sketch-text" => self.sketch_text,
            "ok" => self.ok,
            "danger" => self.danger,
            "warn" => self.warn,
            "err-bg" => self.err_bg,
            "err-text" => self.err_text,
            "warn-bg" => self.warn_bg,
            "warn-text" => self.warn_text,
            "info-bg" => self.info_bg,
            "info-text" => self.info_text,
            "good-bg" => self.good_bg,
            "good-text" => self.good_text,
            "ent-sel" => self.ent_sel,
            "chip" => self.chip,
            "kbd" => self.kbd,
            "expr" => self.expr,
            "toast" => self.toast,
            "toast-warn" => self.toast_warn,
            "scrim" => self.scrim,
            "busy-scrim" => self.busy_scrim,
            "spin-track" => self.spin_track,
            "float" => self.float,
            "dim-ink" => self.dim_ink,
            "dim-text" => self.dim_text,
            "dim-ref" => self.dim_ref,
            "dim-expr" => self.dim_expr,
            "dim-sel" => self.dim_sel,
            "dim-sel-text" => self.dim_sel_text,
            "dim-sel-line" => self.dim_sel_line,
            "dim-bad" => self.dim_bad,
            "dim-bad-line" => self.dim_bad_line,
            "dim-preview" => self.dim_preview,
            "dim-preview-line" => self.dim_preview_line,
            "dim-hover" => self.dim_hover,
            "cg-bg" => self.cg_bg,
            "cg-text" => self.cg_text,
            "cg-line" => self.cg_line,
            "cg-bad-bg" => self.cg_bad_bg,
            "cg-bad-text" => self.cg_bad_text,
            "cg-bad-line" => self.cg_bad_line,
            "def-under" => self.def_under,
            "def-full" => self.def_full,
            "def-over" => self.def_over,
            "heart" => self.heart,
            _ => return None,
        })
    }
}

pub fn tokens(dark: bool) -> Tokens {
    if dark {
        Tokens {
            chrome: Color32::from_rgb(0x17, 0x19, 0x1e),
            chrome_text: Color32::from_rgb(0xc9, 0xce, 0xd8),
            chrome_hover: Color32::from_rgba_unmultiplied(255, 255, 255, 20),
            doc_tab: Color32::from_rgb(0x2b, 0x2f, 0x38),
            ribbon: Color32::from_rgb(0x1d, 0x20, 0x26),
            ribbon_line: Color32::from_rgb(0x12, 0x14, 0x18),
            divider: Color32::from_rgb(0x2c, 0x30, 0x38),
            bg: Color32::from_rgb(0x2a, 0x2e, 0x37),
            panel: Color32::from_rgb(0x22, 0x25, 0x2c),
            panel_2: Color32::from_rgb(0x2a, 0x2e, 0x36),
            hover: Color32::from_rgb(0x2b, 0x2f, 0x38),
            line: Color32::from_rgb(0x33, 0x37, 0x3f),
            line_2: Color32::from_rgb(0x2c, 0x30, 0x38),
            input: Color32::from_rgb(0x1a, 0x1c, 0x21),
            input_line: Color32::from_rgb(0x36, 0x3a, 0x42),
            text: Color32::from_rgb(0xd7, 0xdc, 0xe4),
            strong: Color32::from_rgb(0xee, 0xf1, 0xf5),
            muted: Color32::from_rgb(0x8b, 0x93, 0xa0),
            label: Color32::from_rgb(0xc3, 0xcb, 0xd4),
            accent: Color32::from_rgb(0x4c, 0x8d, 0xf6),
            accent_soft: Color32::from_rgba_unmultiplied(76, 141, 246, 41),
            primary: Color32::from_rgb(0x3b, 0x7b, 0xe8),
            primary_hover: Color32::from_rgb(0x4c, 0x8d, 0xf6),
            sel: Color32::from_rgb(0x2c, 0x4a, 0x78),
            sel_text: Color32::from_rgb(0xff, 0xff, 0xff),
            sel_line: Color32::from_rgb(0x3a, 0x62, 0x99),
            sketch: Color32::from_rgb(0xf0, 0x8a, 0x4b),
            sketch_fill: Color32::from_rgba_unmultiplied(240, 138, 75, 61),
            sketch_text: Color32::from_rgb(0xff, 0xd9, 0xc2),
            ok: Color32::from_rgb(0x5f, 0xcf, 0x8f),
            danger: Color32::from_rgb(0xe5, 0x53, 0x4b),
            warn: Color32::from_rgb(0xe0, 0xa0, 0x30),
            err_bg: Color32::from_rgb(0x3a, 0x1f, 0x22),
            err_text: Color32::from_rgb(0xff, 0x9d, 0x9d),
            warn_bg: Color32::from_rgb(0x3a, 0x30, 0x20),
            warn_text: Color32::from_rgb(0xf1, 0xc2, 0x6b),
            info_bg: Color32::from_rgb(0x26, 0x2b, 0x33),
            info_text: Color32::from_rgb(0xb9, 0xc3, 0xcd),
            good_bg: Color32::from_rgb(0x1d, 0x33, 0x28),
            good_text: Color32::from_rgb(0x7f, 0xdc, 0xaa),
            ent_sel: Color32::from_rgb(0x4a, 0x2e, 0x1f),
            chip: Color32::from_rgb(0x2f, 0x34, 0x3d),
            kbd: Color32::from_rgb(0x2a, 0x2f, 0x37),
            expr: Color32::from_rgb(0xc3, 0xa6, 0xff),
            toast: Color32::from_rgb(0x34, 0x39, 0x44),
            toast_warn: Color32::from_rgb(0x7a, 0x54, 0x14),
            scrim: Color32::from_rgba_unmultiplied(8, 10, 14, 115),
            busy_scrim: Color32::from_rgba_unmultiplied(10, 12, 14, 89),
            spin_track: Color32::from_rgb(0x3a, 0x40, 0x4a),
            float: Color32::from_rgba_unmultiplied(34, 37, 44, 240),
            dim_ink: Color32::from_rgb(0xce, 0xd4, 0xda),
            dim_text: Color32::from_rgb(0xe9, 0xec, 0xef),
            dim_ref: Color32::from_rgb(0x90, 0x9a, 0xa4),
            dim_expr: Color32::from_rgb(0xb1, 0x97, 0xfc),
            dim_sel: Color32::from_rgb(0x38, 0xd9, 0xa9),
            dim_sel_text: Color32::from_rgb(0x8c, 0xc0, 0xff),
            dim_sel_line: Color32::from_rgb(0x4c, 0x8d, 0xf6),
            dim_bad: Color32::from_rgb(0xff, 0x6b, 0x6b),
            dim_bad_line: Color32::from_rgb(0xa3, 0x3d, 0x3d),
            dim_preview: Color32::from_rgb(0xff, 0x92, 0x2b),
            dim_preview_line: Color32::from_rgb(0x8a, 0x54, 0x20),
            dim_hover: Color32::from_rgb(0x5c, 0x64, 0x70),
            cg_bg: Color32::from_rgb(0x1f, 0x35, 0x50),
            cg_text: Color32::from_rgb(0x9c, 0xc5, 0xf5),
            cg_line: Color32::from_rgb(0x2f, 0x5a, 0x8a),
            cg_bad_bg: Color32::from_rgb(0x4a, 0x22, 0x24),
            cg_bad_text: Color32::from_rgb(0xff, 0xa8, 0xa8),
            cg_bad_line: Color32::from_rgb(0x8a, 0x3a, 0x3a),
            def_under: Color32::from_rgb(0x4c, 0x8d, 0xf6),
            def_full: Color32::from_rgb(0xe9, 0xec, 0xef),
            def_over: Color32::from_rgb(0xe5, 0x53, 0x4b),
            heart: Color32::from_rgb(0xff, 0x5e, 0x5b),
            shadow: Shadow { offset: [0, 6], blur: 20, spread: 0, color: Color32::from_rgba_unmultiplied(0, 0, 0, 89) },
            shadow_lg: Shadow { offset: [0, 10], blur: 40, spread: 0, color: Color32::from_rgba_unmultiplied(0, 0, 0, 115) },
            radius: 7.0,
            radius_sm: 5.0,
        }
    } else {
        Tokens {
            chrome: Color32::from_rgb(0xf3, 0xf4, 0xf6),
            chrome_text: Color32::from_rgb(0x39, 0x41, 0x4d),
            chrome_hover: Color32::from_rgba_unmultiplied(0, 0, 0, 15),
            doc_tab: Color32::from_rgb(0xe3, 0xe6, 0xeb),
            ribbon: Color32::from_rgb(0xfa, 0xfb, 0xfc),
            ribbon_line: Color32::from_rgb(0xdf, 0xe3, 0xe8),
            divider: Color32::from_rgb(0xe3, 0xe6, 0xea),
            bg: Color32::from_rgb(0xe6, 0xe9, 0xee),
            panel: Color32::from_rgb(0xff, 0xff, 0xff),
            panel_2: Color32::from_rgb(0xf1, 0xf3, 0xf6),
            hover: Color32::from_rgb(0xee, 0xf1, 0xf5),
            line: Color32::from_rgb(0xd9, 0xdd, 0xe3),
            line_2: Color32::from_rgb(0xe6, 0xe9, 0xee),
            input: Color32::from_rgb(0xff, 0xff, 0xff),
            input_line: Color32::from_rgb(0xcf, 0xd5, 0xdc),
            text: Color32::from_rgb(0x1f, 0x25, 0x30),
            strong: Color32::from_rgb(0x0f, 0x14, 0x1b),
            muted: Color32::from_rgb(0x6b, 0x73, 0x80),
            label: Color32::from_rgb(0x3b, 0x46, 0x52),
            accent: Color32::from_rgb(0x2f, 0x6f, 0xe0),
            accent_soft: Color32::from_rgb(0xe3, 0xed, 0xfc),
            primary: Color32::from_rgb(0x2f, 0x6f, 0xe0),
            primary_hover: Color32::from_rgb(0x24, 0x5f, 0xc9),
            sel: Color32::from_rgb(0xdc, 0xe8, 0xfb),
            sel_text: Color32::from_rgb(0x1d, 0x4f, 0x9e),
            sel_line: Color32::from_rgb(0xb5, 0xcf, 0xf0),
            sketch: Color32::from_rgb(0xd9, 0x48, 0x0f),
            sketch_fill: Color32::from_rgb(0xfd, 0xe8, 0xdc),
            sketch_text: Color32::from_rgb(0xa8, 0x38, 0x0b),
            ok: Color32::from_rgb(0x2f, 0x85, 0x5a),
            danger: Color32::from_rgb(0xc2, 0x3b, 0x3b),
            warn: Color32::from_rgb(0xb7, 0x79, 0x1f),
            err_bg: Color32::from_rgb(0xfd, 0xec, 0xec),
            err_text: Color32::from_rgb(0xc2, 0x3b, 0x3b),
            warn_bg: Color32::from_rgb(0xfd, 0xf5, 0xe6),
            warn_text: Color32::from_rgb(0x9a, 0x65, 0x16),
            info_bg: Color32::from_rgb(0xee, 0xf3, 0xf8),
            info_text: Color32::from_rgb(0x3b, 0x46, 0x52),
            good_bg: Color32::from_rgb(0xe8, 0xf6, 0xee),
            good_text: Color32::from_rgb(0x1f, 0x6f, 0x43),
            ent_sel: Color32::from_rgb(0xfd, 0xe8, 0xdc),
            chip: Color32::from_rgb(0xe4, 0xe9, 0xef),
            kbd: Color32::from_rgb(0xee, 0xf1, 0xf4),
            expr: Color32::from_rgb(0x6a, 0x3f, 0xb5),
            toast: Color32::from_rgb(0x26, 0x30, 0x3b),
            toast_warn: Color32::from_rgb(0x8a, 0x5a, 0x12),
            scrim: Color32::from_rgba_unmultiplied(20, 30, 40, 77),
            busy_scrim: Color32::from_rgba_unmultiplied(240, 242, 245, 89),
            spin_track: Color32::from_rgb(0xc8, 0xd3, 0xdf),
            float: Color32::from_rgba_unmultiplied(255, 255, 255, 240),
            dim_ink: Color32::from_rgb(0x34, 0x3a, 0x40),
            dim_text: Color32::from_rgb(0x21, 0x25, 0x29),
            dim_ref: Color32::from_rgb(0x86, 0x8e, 0x96),
            dim_expr: Color32::from_rgb(0x67, 0x41, 0xd9),
            dim_sel: Color32::from_rgb(0x0c, 0xa6, 0x78),
            dim_sel_text: Color32::from_rgb(0x08, 0x7f, 0x5b),
            dim_sel_line: Color32::from_rgb(0x0c, 0xa6, 0x78),
            dim_bad: Color32::from_rgb(0xe0, 0x31, 0x31),
            dim_bad_line: Color32::from_rgb(0xff, 0xa8, 0xa8),
            dim_preview: Color32::from_rgb(0xd9, 0x48, 0x0f),
            dim_preview_line: Color32::from_rgb(0xff, 0xc0, 0x78),
            dim_hover: Color32::from_rgb(0xad, 0xb5, 0xbd),
            cg_bg: Color32::from_rgb(0xe9, 0xf1, 0xfb),
            cg_text: Color32::from_rgb(0x1f, 0x5f, 0xae),
            cg_line: Color32::from_rgb(0x9c, 0xc0, 0xea),
            cg_bad_bg: Color32::from_rgb(0xfd, 0xe3, 0xe3),
            cg_bad_text: Color32::from_rgb(0xb4, 0x23, 0x23),
            cg_bad_line: Color32::from_rgb(0xe7, 0xa1, 0xa1),
            def_under: Color32::from_rgb(0x1c, 0x6f, 0xd6),
            def_full: Color32::from_rgb(0x1d, 0x24, 0x33),
            def_over: Color32::from_rgb(0xe0, 0x31, 0x31),
            heart: Color32::from_rgb(0xff, 0x5e, 0x5b),
            shadow: Shadow { offset: [0, 6], blur: 20, spread: 0, color: Color32::from_rgba_unmultiplied(20, 30, 40, 31) },
            shadow_lg: Shadow { offset: [0, 10], blur: 40, spread: 0, color: Color32::from_rgba_unmultiplied(20, 30, 40, 56) },
            radius: 7.0,
            radius_sm: 5.0,
        }
    }
}
// END GENERATED

/// Text sizes in points (1 CSS px = 1 egui point). Each is measured from 1.x's CSS.
pub mod size {
    /// `html, body { font: 12px/1.35 }` (app.css:119): body text, buttons, panel text.
    pub const BODY: f32 = 12.0;
    /// `.rgroup-label`, `.tool-btn`, `.tl-label` secondary text: 11px (app.css:177, :182).
    pub const SMALL: f32 = 11.0;
    /// `.rtab`, `#tool-search`, `#status`, chips: 11.5px (app.css:166, :146, :360).
    pub const MEDIUM: f32 = 11.5;
    /// `.modal-card h3`: 15px (app.css:389). `#empty-state h2` is 16px (:270).
    pub const HEADING: f32 = 15.0;
    /// Line height: `line-height: 1.35` on the body.
    pub const LINE: f32 = 1.35;
}

/// The egui style built from the tokens.
pub fn apply(ctx: &egui::Context, t: &Tokens) {
    let dark = t.bg.r() < 0x80;
    ctx.all_styles_mut(|s| {
        let v = &mut s.visuals;
        *v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.dark_mode = dark;
        v.override_text_color = None;
        v.panel_fill = t.panel;
        v.window_fill = t.panel;
        v.extreme_bg_color = t.input; // text edits, sliders' rails
        v.faint_bg_color = t.panel_2;
        v.code_bg_color = t.kbd;
        v.hyperlink_color = t.accent;
        v.warn_fg_color = t.warn;
        v.error_fg_color = t.danger;
        v.window_stroke = Stroke::new(1.0, t.line);
        v.window_shadow = t.shadow_lg;
        v.popup_shadow = t.shadow;
        // .panel, #menu and (menu_corner_radius) use --radius (7px); .modal-card is 9px; buttons --radius-sm (5px).
        v.window_corner_radius = CornerRadius::same(t.radius as u8);
        v.menu_corner_radius = CornerRadius::same(t.radius as u8);
        v.selection.bg_fill = t.sel;
        v.selection.stroke = Stroke::new(1.0, t.sel_line);

        let rsm = CornerRadius::same(t.radius_sm as u8);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = t.panel;
        w.noninteractive.weak_bg_fill = t.panel;
        w.noninteractive.bg_stroke = Stroke::new(1.0, t.divider); // separators
        w.noninteractive.fg_stroke = Stroke::new(1.0, t.text);
        w.noninteractive.corner_radius = rsm;
        // Buttons are flat until hovered (.icon-btn, .tool-btn: no background, 1px transparent border).
        w.inactive.bg_fill = t.input;
        w.inactive.weak_bg_fill = Color32::TRANSPARENT;
        w.inactive.bg_stroke = Stroke::new(1.0, Color32::TRANSPARENT);
        w.inactive.fg_stroke = Stroke::new(1.0, t.text);
        w.inactive.corner_radius = rsm;
        // :hover { background: var(--hover); border-color: var(--line) }
        w.hovered.bg_fill = t.hover;
        w.hovered.weak_bg_fill = t.hover;
        w.hovered.bg_stroke = Stroke::new(1.0, t.line);
        w.hovered.fg_stroke = Stroke::new(1.0, t.strong);
        w.hovered.corner_radius = rsm;
        // .active { background: var(--sel); border-color: var(--sel-line); color: var(--sel-text) }
        w.active.bg_fill = t.sel;
        w.active.weak_bg_fill = t.sel;
        w.active.bg_stroke = Stroke::new(1.0, t.sel_line);
        w.active.fg_stroke = Stroke::new(1.0, t.sel_text);
        w.active.corner_radius = rsm;
        w.open.bg_fill = t.hover;
        w.open.weak_bg_fill = t.hover;
        w.open.bg_stroke = Stroke::new(1.0, t.line);
        w.open.fg_stroke = Stroke::new(1.0, t.strong);
        w.open.corner_radius = rsm;

        // Spacing measured from the CSS: ribbon items gap 2px (app.css:176), .menu-btn padding 4px 9px
        // (:134), inputs padding 2px 4px (:419), panel padding 8-10px (:218).
        let sp = &mut s.spacing;
        sp.item_spacing = Vec2::new(6.0, 4.0);
        sp.button_padding = Vec2::new(9.0, 4.0);
        sp.menu_margin = Margin::same(4);
        sp.window_margin = Margin::same(10);
        sp.interact_size = Vec2::new(26.0, 22.0); // the narrowest button (.icon-btn is 28x28, 26x24 in the title bar, drawn by hand in ui/topbar.rs); inputs 22px high
        sp.indent = 12.0; // .tnode .tw width
        sp.icon_width = 14.0;

        s.text_styles = [
            (TextStyle::Small, FontId::new(size::SMALL, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(size::BODY, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(size::BODY, FontFamily::Proportional)),
            (TextStyle::Heading, FontId::new(size::HEADING, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(size::BODY, FontFamily::Monospace)),
        ]
        .into();
    });
}
