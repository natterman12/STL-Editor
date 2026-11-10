//! Mesh readers. Port of 1.x `geometry.load_mesh_file`.
//!
//! Contract for every format module (`stl`, `obj`, `ply`, `off`, `threemf`, `gltf`):
//!
//! ```ignore
//! pub fn read(data: &[u8], base: &str) -> Result<Vec<Part>, ReadError>
//! ```
//!
//! * `base` is the file name without directory and extension (`os.path.splitext`).
//! * Return the parts UNWELDED (raw trimesh `vertices` / `faces`, `process=False`); `load.rs`
//!   welds each one with tolerance 1e-5. Positions are flat xyz `f64`, triangles flat `u32`.
//! * Names are final, as 1.x builds them. A file that loads as a single mesh gives one part
//!   named `base` (kept even when it has no faces). A file that loads as a scene
//!   (`Scene.dump()`: node transforms applied, in dump order) gives one part per geometry that
//!   has faces; geometries without faces are skipped but still count: the name is `base` when
//!   the dump has exactly one geometry, else `"{base} {i+1}"` with `i` the geometry's index in
//!   the dump, empties included.
//! * Return an empty `Vec` when nothing usable is found; `load.rs` then raises
//!   "No triangles found in this file." Fail with `ReadError(plain-English text)` otherwise.
//! * For `.glb` / `.gltf` with more than one part, `load.rs` re-sorts by welded minimum
//!   corner (rounded to 3 decimals) then triangle count and renames to `"{base} {n}"`, so
//!   readers need not do that.

pub mod gltf;
pub mod load;
pub mod obj;
pub mod off;
pub mod ply;
pub mod stl;
pub mod threemf;

pub use load::load_mesh_file;

pub const MESH_EXTS: [&str; 7] = [".stl", ".obj", ".3mf", ".ply", ".off", ".glb", ".gltf"];

/// One mesh read from a file.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub name: String,
    pub positions: Vec<f64>,
    pub triangles: Vec<u32>,
}

/// A plain-English message, shown to the user as is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadError(pub String);

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ReadError {}
