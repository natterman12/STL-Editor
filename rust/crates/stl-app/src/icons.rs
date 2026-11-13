//! The icons of web/js/icons.js, rasterised from the same SVG with resvg.
//!
//! `assets/icons.json` holds the SVG of every key of `ICONS` (written by `assets/gen_icons.mjs`
//! from icons.js). They are 24x24 line icons that draw in `currentColor`, with a few fixed accent
//! fills; `currentColor` is replaced by the colour asked for. Textures are cached per
//! (name, size, colour, screen scale).

use std::collections::HashMap;
use std::sync::OnceLock;

use egui::load::SizedTexture;
use egui::{Color32, ColorImage, Context, Id, ImageSource, TextureHandle, TextureOptions};

static JSON: &str = include_str!("../assets/icons.json");

fn table() -> &'static HashMap<String, String> {
    static T: OnceLock<HashMap<String, String>> = OnceLock::new();
    T.get_or_init(|| {
        let mut t: HashMap<String, String> = serde_json::from_str(JSON).expect("assets/icons.json is valid");
        for (name, body) in GLYPHS {
            t.insert((*name).to_owned(), format!("{HEAD}{body}</svg>"));
        }
        t
    })
}

/// Same wrapper as `S()` in icons.js.
const HEAD: &str = r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">"#;

/// icons.js draws ten constraint icons as a text glyph (`<text>•</text>`, a font symbol).
/// resvg here has no text support and the glyphs would depend on the system font, so these are
/// the same symbols drawn as paths (our own drawings, 24x24, centred like the glyphs).
const GLYPHS: &[(&str, &str)] = &[
    ("c_coincident", r#"<circle cx="12" cy="12" r="3" fill="currentColor" stroke="none"/>"#),
    ("c_hv", r#"<path d="M8 6v12M8 12h9" stroke-width="2"/>"#),
    ("c_parallel", r#"<path d="M9.5 6v12M14.5 6v12" stroke-width="2"/>"#),
    ("c_perpendicular", r#"<path d="M6 18h12M12 18V6" stroke-width="2"/>"#),
    ("c_equal", r#"<path d="M6 9.5h12M6 14.5h12" stroke-width="2"/>"#),
    ("c_tangent", r#"<path d="M7 7h10M12 7v11" stroke-width="2"/>"#),
    ("c_concentric", r#"<circle cx="12" cy="12" r="3" stroke-width="1.8"/><circle cx="12" cy="12" r="7" stroke-width="1.8"/>"#),
    ("c_midpoint", r#"<path d="M6 17V7l6 6 6-6v10" stroke-width="2"/>"#),
    ("c_fix", r#"<rect x="7" y="11" width="10" height="8" rx="1.5" fill="currentColor" stroke="none"/><path d="M9 11V8a3 3 0 0 1 6 0v3" stroke-width="1.8"/>"#),
    ("c_symmetric", r#"<path d="M12 4v16" stroke-dasharray="2 2"/><circle cx="7" cy="12" r="2.2"/><circle cx="17" cy="12" r="2.2"/>"#),
];

/// All icon names (the keys of ICONS in icons.js), sorted.
pub fn names() -> Vec<&'static str> {
    let mut v: Vec<&str> = table().keys().map(String::as_str).collect();
    v.sort_unstable();
    v
}

/// The SVG text of an icon. Like icons.js, an unknown name gives the `primitive` icon.
pub fn svg(name: &str) -> &'static str {
    table().get(name).or_else(|| table().get("primitive")).map(String::as_str).unwrap_or("")
}

/// Draw an icon into `px` x `px` pixels: straight-alpha RGBA bytes.
pub fn render_rgba(name: &str, px: u32, color: Color32) -> Result<Vec<u8>, String> {
    let px = px.max(1);
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    let mut s = svg(name).replace("currentColor", &format!("rgb({r},{g},{b})"));
    if a < 255 {
        // Whole-icon opacity (the root <svg> element).
        s = s.replacen("<svg ", &format!("<svg opacity=\"{:.3}\" ", a as f32 / 255.0), 1);
    }
    let tree = resvg::usvg::Tree::from_str(&s, &resvg::usvg::Options::default()).map_err(|e| format!("{name}: {e}"))?;
    let mut pm = resvg::tiny_skia::Pixmap::new(px, px).ok_or("empty pixmap")?;
    let scale = px as f32 / tree.size().width();
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pm.as_mut());
    // tiny-skia stores premultiplied; hand back straight alpha.
    let mut out = Vec::with_capacity(pm.data().len());
    for p in pm.pixels() {
        let c = p.demultiply();
        out.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    Ok(out)
}

type Key = (String, u32, [u8; 4], u32);

#[derive(Clone, Default)]
struct Cache(HashMap<Key, TextureHandle>);

/// The icon as a texture, `size` points wide, drawn sharp at the current screen scale.
pub fn texture(ctx: &Context, name: &str, size: f32, color: Color32) -> TextureHandle {
    let ppp = ctx.pixels_per_point();
    let key: Key = (name.to_owned(), size.to_bits(), color.to_array(), ppp.to_bits());
    if let Some(t) = ctx.data_mut(|d| d.get_temp_mut_or_default::<Cache>(Id::NULL).0.get(&key).cloned()) {
        return t;
    }
    let px = (size * ppp).ceil().max(1.0) as u32;
    let rgba = render_rgba(name, px, color).unwrap_or_else(|_| vec![0; (px * px * 4) as usize]);
    let img = ColorImage::from_rgba_unmultiplied([px as usize, px as usize], &rgba);
    let tex = ctx.load_texture(format!("icon:{name}"), img, TextureOptions::LINEAR);
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Cache>(Id::NULL).0.insert(key, tex.clone()));
    tex
}

/// An icon for `egui::Image::new(...)` / `ui.add(egui::Image::new(...))`.
/// Names are the keys of ICONS in icons.js. (Takes the context: a texture has to be uploaded.)
pub fn icon(ctx: &Context, name: &str, size: f32, color: Color32) -> ImageSource<'static> {
    let t = texture(ctx, name, size, color);
    ImageSource::Texture(SizedTexture::new(t.id(), egui::vec2(size, size)))
}
