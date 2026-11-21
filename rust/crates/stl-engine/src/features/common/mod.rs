//! Helpers used by two or more features: planes and frames, placing and moving solids,
//! combining a tool with the design, and reading numbers from parameters.

pub mod bed;
pub mod connector_place;
pub mod connectors;
pub mod dovetail;
pub mod fins;
pub mod frames;
pub mod params;
pub mod pitch;
pub mod place;
pub mod placement;
pub mod pose;
pub mod printers;
pub mod profiles;
pub mod pushpull_tool;
pub mod round;
pub mod tool;

pub use frames::{Frame, PlaneDef, Vec3, base_planes, plane_frame};
pub use params::{float, pick, strings, truthy};
pub use place::{affine, mirror_matrix, pivot, place, rotation, transform_body};
pub use profiles::{
    SketchRec, align, bbox, big_extent, ccw, cross2, dedupe, frame_matrix, make_cylinder, material_volume, mesh_solid,
    overlaps, path_points, path_self_crosses, profile_loop, resample, rot_between, sketch,
};
pub use pushpull_tool::pushpull_tool;
pub use tool::{ToolRecord, apply_tool, repeat_features};
