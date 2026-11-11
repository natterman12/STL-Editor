//! Occupancy grid for any (possibly broken) mesh (ported from 1.x `mesh_occupancy`).

use crate::MeshError;

use super::{Grid3, binary_dilation, binary_erosion, binary_fill_holes, check_grid, grid_for, surface_points};

/// Rasterise the surface, then fill the inside. Holes larger than a voxel are closed by growing
/// the surface before filling. Returns the grid and its origin (voxel centres at `origin + ijk * pitch`).
pub fn mesh_occupancy(
    verts: &[[f64; 3]],
    faces: &[[u32; 3]],
    pitch: f64,
) -> Result<(Grid3<bool>, [f64; 3]), MeshError> {
    if verts.is_empty() {
        return Err(MeshError::Message("Voxel rebuild produced an invalid mesh.".to_string()));
    }
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for v in verts {
        for a in 0..3 {
            lo[a] = lo[a].min(v[a]);
            hi[a] = hi[a].max(v[a]);
        }
    }
    let (origin, n) = grid_for(lo, hi, pitch, 12);
    check_grid(n)?;
    let pts = surface_points(verts, faces, pitch * 0.5);
    let mut shell = Grid3::try_new(n, false)?;
    for p in &pts {
        let mut c = [0usize; 3];
        for a in 0..3 {
            let r = ((p[a] - origin[a]) / pitch).round_ties_even();
            c[a] = (r as i64).clamp(0, n[a] as i64 - 1) as usize;
        }
        let id = shell.idx(c[0], c[1], c[2]);
        shell.data[id] = true;
    }
    let shell_count = shell.data.iter().filter(|&&b| b).count();
    for it in [0usize, 1, 2, 3, 5, 8, 11] {
        let grown_owned;
        let grown: &Grid3<bool> = if it > 0 {
            grown_owned = binary_dilation(&shell, it)?;
            &grown_owned
        } else {
            &shell
        };
        let mut filled = binary_fill_holes(grown)?;
        let inside = filled.data.iter().zip(&grown.data).filter(|&(&f, &g)| f && !g).count();
        if (inside as f64) > 0.2 * (shell_count as f64) {
            if it > 0 {
                filled = binary_erosion(&filled, it)?;
                for (f, &s) in filled.data.iter_mut().zip(&shell.data) {
                    *f |= s; // keep the original surface voxels
                }
            }
            return Ok((filled, origin));
        }
    }
    Ok((binary_fill_holes(&shell)?, origin))
}
