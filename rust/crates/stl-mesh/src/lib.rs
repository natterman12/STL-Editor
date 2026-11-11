//! Mesh tools: remesh, simplify, voxelise and marching cubes, repair, sculpt, nearest-point search.

mod error;
pub mod nearest;
pub mod offset;
pub mod remesh;
pub mod repair;
pub mod sculpt;
pub mod simplify;
pub mod thin;
pub mod voxel;
pub mod refs;
mod weld;

pub use refs::{
    as_circle, ents_from_polys, fit_circle, outline, outline_entities, ref_geometry, ref_geometry_max,
    round_py, OutlineEntity, RefEntity, RefGeometry,
};
pub use repair::{
    cap_holes, fill_holes, fix_inversion, fix_winding, local_repair, merge_overlapping_shells, process,
    quick_repair, touching_shells,
};
pub use error::MeshError;
pub use weld::weld;
