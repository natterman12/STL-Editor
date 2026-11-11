//! `voxel_rebuild` and `auto_pitch` (ported from 1.x `stleditor/geometry.py`).

use stl_kernel::Solid;

use super::{Grid3, field_to_solid, gaussian_filter, mesh_occupancy};
use crate::MeshError;

/// Rebuild any mesh (open, self-intersecting, multi-shell) as one watertight solid.
/// 1.x passes `smooth = 0.6` from Repair.
pub fn voxel_rebuild(verts: &[[f64; 3]], faces: &[[u32; 3]], pitch: f64, smooth: f64) -> Result<Solid, MeshError> {
    let (occ, origin) = mesh_occupancy(verts, faces, pitch)?;
    let mut field = Grid3::try_new(occ.n, 0.0f32)?;
    for (f, &b) in field.data.iter_mut().zip(&occ.data) {
        *f = if b { 1.0 } else { 0.0 };
    }
    if smooth > 0.0 {
        gaussian_filter(&mut field, smooth);
    }
    field_to_solid(&field, origin, pitch, 0.5)
}

/// 1.x `auto_pitch`. It gets the float32 mesh points and measures in float32, as numpy does.
pub fn auto_pitch(verts: &[[f32; 3]], target: f64) -> f64 {
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for p in verts {
        for a in 0..3 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
    }
    let d = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    let diag = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() as f64;
    (diag / target).max(0.05)
}
