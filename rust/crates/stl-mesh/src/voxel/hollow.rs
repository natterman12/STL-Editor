//! `hollow` (ported from 1.x `stleditor/geometry.py`): shell a solid inward with a wall
//! thickness. The outer surface stays exact; the cavity comes from a voxel distance field.

use stl_kernel::Solid;

use super::{Grid3, distance_transform_edt, field_to_solid, solid_occupancy, SampleIndex, SurfaceSampler};
use crate::MeshError;

const CAP_MSG: &str = "Resolution too fine for this model size. Use a coarser setting.";
const TOO_THICK: &str = "Wall thickness is too large for this body; nothing would be left hollow.";

fn msg(m: &str) -> MeshError {
    MeshError::Message(m.to_string())
}

/// Exact distances from `centers` to the nearest surface sample, on threads where there are any.
pub fn sample_distances(index: &SampleIndex<'_>, centers: &[[f64; 3]]) -> Result<Vec<f64>, MeshError> {
    let mut out: Vec<f64> = Vec::new();
    out.try_reserve_exact(centers.len()).map_err(|_| msg(CAP_MSG))?;
    out.resize(centers.len(), 0.0);
    #[cfg(not(target_arch = "wasm32"))]
    {
        use rayon::prelude::*;
        out.par_iter_mut().zip(centers.par_iter()).for_each(|(o, &c)| *o = index.nearest_dist(c));
    }
    #[cfg(target_arch = "wasm32")]
    {
        for (o, &c) in out.iter_mut().zip(centers) {
            *o = index.nearest_dist(c);
        }
    }
    Ok(out)
}

/// 1.x `hollow`. With `cavity_only` the cavity itself is returned (Shell opens faces into it).
pub fn hollow(s: &Solid, thickness: f64, pitch: Option<f64>, cavity_only: bool) -> Result<Solid, MeshError> {
    if thickness <= 0.0 {
        return Err(msg("Wall thickness must be positive."));
    }
    // 1.x measures the size on the float32 mesh points
    let mm = s.to_mesh();
    let pts: Vec<[f32; 3]> = mm.positions.chunks_exact(3).map(|p| [p[0], p[1], p[2]]).collect();
    if pts.is_empty() {
        return Err(msg(TOO_THICK));
    }
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for p in &pts {
        for a in 0..3 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
    }
    let size = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    let size_min = size[0].min(size[1]).min(size[2]);
    if ((thickness * 2.0) as f32) >= size_min {
        return Err(msg(TOO_THICK));
    }
    let diag = f32_norm(size);
    let pitch = pitch.unwrap_or_else(|| (thickness / 3.0).min(diag / 120.0).max(diag / 400.0));

    let (occ, origin) = solid_occupancy(s, pitch)?;
    let mut field = distance_transform_edt(&occ)?;
    for d in field.data.iter_mut() {
        *d = *d * pitch - pitch * 0.5 - thickness;
    }

    // refine near the cavity wall with exact distances to the surface
    let [_, n1, n2] = occ.n;
    let mut band: Vec<usize> = Vec::new();
    for i in 0..field.data.len() {
        if occ.data[i] && field.data[i].abs() < 2.5 * pitch {
            band.try_reserve(1).map_err(|_| msg(CAP_MSG))?;
            band.push(i);
        }
    }
    if !band.is_empty() {
        let verts: Vec<[f64; 3]> = pts.iter().map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]).collect();
        let faces: Vec<[u32; 3]> = mm.triangles.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();
        let spacing = pitch / 3.0;
        let sampler = SurfaceSampler::new(&verts, &faces, spacing);
        let mut centers: Vec<[f64; 3]> = Vec::new();
        centers.try_reserve_exact(band.len()).map_err(|_| msg(CAP_MSG))?;
        centers.extend(band.iter().map(|&i| {
            let (k, ij) = (i % n2, i / n2);
            let (j, ii) = (ij % n1, ij / n1);
            [origin[0] + ii as f64 * pitch, origin[1] + j as f64 * pitch, origin[2] + k as f64 * pitch]
        }));
        let dd = match SampleIndex::new(&sampler) {
            Some(index) => sample_distances(&index, &centers)?,
            None => return Err(msg(CAP_MSG)),
        };
        for (&i, d) in band.iter().zip(dd) {
            field.data[i] = d - thickness;
        }
    }
    for (f, &inside) in field.data.iter_mut().zip(&occ.data) {
        if !inside {
            *f = -pitch;
        }
    }
    if !(field.data.iter().copied().fold(f64::NEG_INFINITY, f64::max) > 0.0) {
        return Err(msg(TOO_THICK));
    }
    let mut field32 = Grid3::try_new(occ.n, 0.0f32)?;
    for (o, &x) in field32.data.iter_mut().zip(&field.data) {
        *o = x as f32;
    }
    let cavity = field_to_solid(&field32, origin, pitch, 0.0)?;
    let cavity = cavity.simplify(pitch * 0.1); // more and the cavity drifts by up to 0.1 mm
    let r = if cavity_only { cavity } else { s.difference(&cavity) };
    Ok(r)
}

/// numpy `linalg.norm` of a float32 vector (`sqrt(dot(x, x))` in float32), as a float.
fn f32_norm(v: [f32; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn norm_of_float32_vector() {
        assert_eq!(f32_norm([3.0, 4.0, 0.0]), 5.0);
    }
}


