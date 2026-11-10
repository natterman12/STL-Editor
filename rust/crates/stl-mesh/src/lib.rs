//! Mesh tools: remesh, simplify, voxelise and marching cubes, repair, sculpt, nearest-point search.

pub mod repair;
mod weld;

pub use repair::{
    cap_holes, fill_holes, fix_inversion, fix_winding, local_repair, merge_overlapping_shells, process,
    quick_repair, touching_shells,
};
pub use weld::weld;
