//! Mesh tools: remesh, simplify, voxelise and marching cubes, repair, sculpt, nearest-point search.

pub mod repair;
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
pub use weld::weld;
