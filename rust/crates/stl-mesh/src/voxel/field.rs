//! 1.x `_field_to_manifold` (`stleditor/geometry.py`): marching cubes on a scalar field
//! (positive = inside), then a solid.

use stl_kernel::{MeshArrays, Solid};

use super::{Grid3, marching_cubes};
use crate::{MeshError, weld};

const INVALID_MSG: &str = "Voxel rebuild produced an invalid mesh.";

/// 1.x `manifold_from_arrays`: `None` when the mesh is not a closed 2-manifold.
fn manifold_from_arrays(v: &[f64], f: &[u32]) -> Option<Solid> {
    if f.is_empty() {
        return None;
    }
    let m = MeshArrays { positions: v.iter().map(|&x| x as f32).collect(), triangles: f.to_vec() };
    Solid::from_mesh(&m).ok()
}

fn reversed(f: &[u32]) -> Vec<u32> {
    f.chunks_exact(3).flat_map(|t| [t[2], t[1], t[0]]).collect()
}

/// Cell `(i, j, k)` of `field` sits at `origin + (i, j, k) * pitch`. Where skimage itself would
/// raise (level above the whole field, or no surface), the error carries skimage's message.
pub fn field_to_solid(field: &Grid3<f32>, origin: [f64; 3], pitch: f64, level: f32) -> Result<Solid, MeshError> {
    let [n0, n1, n2] = field.n;
    let fmin = field.data.iter().copied().fold(f32::INFINITY, f32::min);
    let fill = ((fmin.min(level)) as f64 - 1.0) as f32;
    let p = [n0 + 2, n1 + 2, n2 + 2];
    let mut pad = Grid3::try_new(p, fill)?;
    for i in 0..n0 {
        for j in 0..n1 {
            let src = field.idx(i, j, 0);
            let dst = pad.idx(i + 1, j + 1, 1);
            pad.data[dst..dst + n2].copy_from_slice(&field.data[src..src + n2]);
        }
    }
    let fmax = pad.data.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    if level > fmax {
        return Err(MeshError::Message("Surface level must be within volume data range.".into()));
    }
    let (verts, faces) = marching_cubes(&pad, level, pitch);
    if verts.is_empty() {
        return Err(MeshError::Message("No surface found at the given iso value.".into()));
    }
    let v: Vec<f64> = verts.iter().flat_map(|q| [0, 1, 2].map(|a| q[a] + origin[a] - pitch)).collect();
    // skimage winds faces inward for "positive inside" fields
    let f: Vec<u32> = faces.iter().flat_map(|t| [t[2], t[1], t[0]]).collect();
    let (v, f) = weld(&v, &f, pitch * 1e-4);
    solid_from_surface(&v, &f)
}

/// The end of `_field_to_manifold`: the solid, retrying with every face reversed when the mesh
/// is not accepted, and again when it comes out inside out (1.x's signed `volume() < 0`).
fn solid_from_surface(v: &[f64], f: &[u32]) -> Result<Solid, MeshError> {
    let invalid = || MeshError::Message(INVALID_MSG.into());
    let man = match manifold_from_arrays(v, f) {
        Some(m) => m,
        None => manifold_from_arrays(v, &reversed(f)).ok_or_else(invalid)?,
    };
    if man.signed_volume() < 0.0 {
        return manifold_from_arrays(v, &reversed(f)).ok_or_else(invalid);
    }
    Ok(man)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 10 mm cube with its triangles facing inwards.
    fn inside_out_cube() -> (Vec<f64>, Vec<u32>) {
        let mut v = Vec::new();
        for i in 0..8 {
            v.extend([(i & 1) as f64 * 10.0, ((i >> 1) & 1) as f64 * 10.0, ((i >> 2) & 1) as f64 * 10.0]);
        }
        let outward: [u32; 36] = [
            0, 2, 1, 1, 2, 3, 4, 5, 6, 5, 7, 6, 0, 1, 4, 1, 5, 4, 2, 6, 3, 3, 6, 7, 0, 4, 2, 2, 4, 6, 1, 3, 5, 3, 7,
            5,
        ];
        (v, reversed(&outward))
    }

    #[test]
    fn inside_out_surface_is_turned_the_right_way() {
        let (v, f) = inside_out_cube();
        assert!(manifold_from_arrays(&v, &f).unwrap().signed_volume() < 0.0);
        let s = solid_from_surface(&v, &f).expect("solid");
        assert!((s.signed_volume() - 1000.0).abs() < 1e-9, "signed volume {}", s.signed_volume());
    }
}
