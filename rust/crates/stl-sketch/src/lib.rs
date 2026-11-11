//! 2D sketches and dimensions, text and fonts, SVG and DXF import, 2D polygon operations.

pub mod contours;
pub mod entities;
pub mod error;
pub mod geos;
pub mod import2d;
pub mod ops;
pub mod polygon;
pub mod profile;
pub mod solver;
pub mod text;

pub use entities::{Entity, P2, point, point_keys};
pub use error::SketchError;
