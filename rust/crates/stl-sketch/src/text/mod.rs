//! Sketch text: font discovery (native) and glyph outlines matching 1.x `meshtools`.

#[cfg(not(target_arch = "wasm32"))]
pub mod fonts;
pub mod glyphs;

pub use glyphs::{TextOpts, text_contours, text_extent};

#[cfg(not(target_arch = "wasm32"))]
pub use fonts::{FontInfo, default_font, font_dirs, font_files, font_path, list_fonts};
