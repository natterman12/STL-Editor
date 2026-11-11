//! Sketch text: font discovery (native) and glyph outlines matching 1.x `meshtools`.

#[cfg(not(target_arch = "wasm32"))]
pub mod fonts;
pub mod glyphs;

pub use glyphs::{text_contours, text_extent, TextOpts};

#[cfg(not(target_arch = "wasm32"))]
pub use fonts::{default_font, font_dirs, font_files, font_path, list_fonts, FontInfo};
