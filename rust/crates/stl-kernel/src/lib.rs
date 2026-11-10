//! Our own solid and 2D-shape interface: booleans, refine, hull, slice, cross-sections, transforms, mesh import and export as arrays. The only crate that uses manifold-rust.
//!
//! Nothing from manifold-rust is visible outside this crate, so the kernel can be replaced
//! or forked without changes elsewhere. Lengths are millimetres, angles are degrees.

mod error;
mod mesh;
mod shape2d;
mod solid;

pub use error::KernelError;
pub use mesh::{Aabb, MeshArrays};
pub use shape2d::Shape2d;
pub use solid::Solid;
