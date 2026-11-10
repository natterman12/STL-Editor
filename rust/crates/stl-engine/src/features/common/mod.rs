//! Helpers used by two or more features: planes and frames, placing and moving solids,
//! combining a tool with the design, and reading numbers from parameters.

pub mod connectors;
pub mod frames;
pub mod params;
pub mod place;
pub mod tool;

pub use frames::{base_planes, plane_frame, Frame, PlaneDef, Vec3};
pub use params::float;
pub use place::{affine, mirror_matrix, pivot, place, rotation, transform_body};
pub use tool::{apply_tool, repeat_features, ToolRecord};
