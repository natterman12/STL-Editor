//! SVG and DXF import (`web/js/import2d.js`).

pub mod dxf;
pub mod svg;

pub use dxf::import_dxf;
pub use svg::{
    import_svg, mm_per_from_svg, mm_per_unit, parse_float_js, svg_drawing_size_mm, svg_tag_attr,
};
