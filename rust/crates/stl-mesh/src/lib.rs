//! Mesh tools: remesh, simplify, voxelise and marching cubes, repair, sculpt, nearest-point search,
//! and a bounding-volume tree for picking.

pub mod bvh;
mod error;
pub mod fit;
pub mod nearest;
pub mod offset;
pub mod refs;
pub mod remesh;
pub mod repair;
pub mod sculpt;
pub mod simplify;
pub mod thin;
pub mod thread;
pub mod voxel;
mod weld;

pub use bvh::Bvh;
pub use error::MeshError;
pub use refs::{
    OutlineEntity, RefEntity, RefGeometry, as_circle, ents_from_polys, fit_circle, outline, outline_entities,
    ref_geometry, ref_geometry_max, round_py,
};
pub use repair::{
    cap_holes, fill_holes, fix_inversion, fix_winding, local_repair, merge_overlapping_shells, process, quick_repair,
    touching_shells,
};
pub use weld::weld;
