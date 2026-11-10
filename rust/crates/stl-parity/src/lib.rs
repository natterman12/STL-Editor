//! Mesh stats and the matching rules used to compare 2.0 results with 1.x.

mod compare;
mod stats;

pub use compare::{Exactness, close, compare_mesh};
pub use stats::{MeshStats, mesh_stats};
