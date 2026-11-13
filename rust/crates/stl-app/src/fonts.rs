//! Fonts. 1.x uses the system UI font (`--font: "Segoe UI", system-ui, ...`, app.css:59).
//!
//! - Desktop: at run time load the system UI font (Segoe UI on Windows; never shipped) and put it
//!   first in the proportional family.
//! - Web, or a machine without it: the bundled Selawik (SIL Open Font License 1.1, Microsoft;
//!   `assets/fonts/Selawik-OFL.txt`). Selawik is Microsoft's metric-compatible stand-in for Segoe
//!   UI, so text widths and line breaks stay the same as with the real font.

use egui::{FontData, FontDefinitions, FontFamily};
use std::sync::Arc;

const SELAWIK: &[u8] = include_bytes!("../assets/fonts/selawk.ttf");
const SELAWIK_BOLD: &[u8] = include_bytes!("../assets/fonts/selawkb.ttf");

/// System UI font files to try, in order (desktop only).
#[cfg(not(target_arch = "wasm32"))]
fn system_candidates() -> Vec<std::path::PathBuf> {
    use std::path::PathBuf;
    if cfg!(windows) {
        let root = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        vec![root.join("Fonts").join("segoeui.ttf")]
    } else if cfg!(target_os = "macos") {
        ["/System/Library/Fonts/SFNS.ttf", "/System/Library/Fonts/Helvetica.ttc"].map(PathBuf::from).to_vec()
    } else {
        ["/usr/share/fonts/truetype/ubuntu/Ubuntu-R.ttf", "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf", "/usr/share/fonts/TTF/DejaVuSans.ttf"]
            .map(PathBuf::from)
            .to_vec()
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn system_font() -> Option<Vec<u8>> {
    system_candidates().into_iter().find_map(|p| std::fs::read(p).ok())
}
#[cfg(target_arch = "wasm32")]
fn system_font() -> Option<Vec<u8>> {
    None
}

/// The font definitions: system UI font first when present, then Selawik, then egui's defaults
/// (which keep the symbol and emoji glyphs).
pub fn definitions() -> FontDefinitions {
    let mut f = FontDefinitions::default();
    f.font_data.insert("selawik".into(), Arc::new(FontData::from_static(SELAWIK)));
    f.font_data.insert("selawik-bold".into(), Arc::new(FontData::from_static(SELAWIK_BOLD)));
    let prop = f.families.entry(FontFamily::Proportional).or_default();
    prop.insert(0, "selawik".into());
    if let Some(bytes) = system_font() {
        f.font_data.insert("system-ui".into(), Arc::new(FontData::from_owned(bytes)));
        f.families.entry(FontFamily::Proportional).or_default().insert(0, "system-ui".into());
    }
    f.families.insert(FontFamily::Name("bold".into()), vec!["selawik-bold".into(), "selawik".into()]);
    f
}

pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(definitions());
}
