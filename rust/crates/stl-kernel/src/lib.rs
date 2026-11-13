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
pub use frame::{cross, dot, Frame, Vec3};
pub use mesh::{Aabb, MeshArrays};
pub use minkowski::{minkowski_batch_path_hits, with_minkowski_batch_threads, with_stock_minkowski};
pub use scratch::{release_scratch, touch as touch_scratch, scratch_held_bytes, set_idle_timeout, ScratchGuard, DEFAULT_IDLE_TIMEOUT};
#[cfg(target_arch = "wasm32")]
pub use scratch::set_clock;
pub use shape2d::Shape2d;
pub use solid::Solid;
pub use triangulate::triangulate;
