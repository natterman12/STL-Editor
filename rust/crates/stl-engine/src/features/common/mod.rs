//! Helpers used by two or more features: planes and frames, placing and moving solids,
//! combining a tool with the design, and reading numbers from parameters.

pub mod connectors;
pub mod frames;
pub mod params;
pub mod place;
pub mod profiles;
pub mod pushpull_tool;
pub mod tool;

pub use frames::{base_planes, plane_frame, Frame, PlaneDef, Vec3};
pub use params::{float, pick, strings, truthy};
pub use profiles::{
    align, bbox, big_extent, ccw, cross2, dedupe, frame_matrix, make_cylinder, material_volume, mesh_solid, overlaps,
    path_points, path_self_crosses, profile_loop, resample, rot_between, sketch, SketchRec,
};
pub use pushpull_tool::pushpull_tool;
pub use place::{affine, mirror_matrix, pivot, place, rotation, transform_body};
pub use tool::{apply_tool, repeat_features, ToolRecord};
