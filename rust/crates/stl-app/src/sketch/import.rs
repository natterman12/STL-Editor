//! SVG and DXF into the open sketch (`app.js` `sketchImport`).

use stl_sketch::entities::Entity;
use stl_sketch::import2d::{import_dxf, import_svg};

const NONE: &str = "No lines or curves found in that file.";

/// Shapes from an SVG or DXF file, chosen by the name's extension (anything else is read as SVG).
/// An empty drawing is the same sentence 1.x shows.
pub fn import_file(name: &str, bytes: &[u8]) -> Result<Vec<Entity>, String> {
    let text = String::from_utf8_lossy(bytes);
    let ents = if name.to_ascii_lowercase().ends_with(".dxf") {
        import_dxf(&text).map_err(|e| e.to_string())?
    } else {
        import_svg(&text).map_err(|e| e.to_string())?
    };
    if ents.is_empty() {
        return Err(NONE.to_string());
    }
    Ok(ents)
}
