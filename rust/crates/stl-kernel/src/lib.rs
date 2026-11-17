//! Our own solid and 2D-shape interface: booleans, refine, hull, slice, cross-sections, transforms, mesh import and export as arrays. The only crate that uses manifold-rust.
//!
//! Nothing from manifold-rust is visible outside this crate, so the kernel can be replaced
//! or forked without changes elsewhere. Lengths are millimetres, angles are degrees.

mod error;
mod frame;
mod mesh;
mod minkowski;
mod scratch;
mod shape2d;
mod solid;
mod triangulate;

pub use error::KernelError;
pub use frame::{Frame, Vec3, cross, dot};
pub use mesh::{Aabb, MeshArrays};
pub use minkowski::{minkowski_batch_path_hits, with_minkowski_batch_threads, with_stock_minkowski};
#[cfg(target_arch = "wasm32")]
pub use scratch::set_clock;
pub use scratch::{
    DEFAULT_IDLE_TIMEOUT, ScratchGuard, release_scratch, scratch_held_bytes, set_idle_timeout, touch as touch_scratch,
};
pub use shape2d::Shape2d;
pub use solid::Solid;
pub use triangulate::triangulate;
