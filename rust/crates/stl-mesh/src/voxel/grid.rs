//! 3D voxel grid, 300M voxel cap, 2D even-odd scanline rasteriser, and watertight solid occupancy
//! (ported from 1.x `stleditor/geometry.py`).

use stl_kernel::Solid;

use crate::MeshError;

const VOXEL_CAP: f64 = 300e6;
const VOXEL_CAP_MSG: &str = "Resolution too fine for this model size. Use a coarser setting.";

fn cap_error() -> MeshError {
    MeshError::Message(VOXEL_CAP_MSG.to_string())
}

/// 3D voxel grid in C (row-major) order: cell `(i, j, k)` is at `(i * n[1] + j) * n[2] + k`.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid3<T> {
    pub n: [usize; 3],
    pub data: Vec<T>,
}

impl<T> Grid3<T> {
    #[inline]
    pub fn idx(&self, i: usize, j: usize, k: usize) -> usize {
        (i * self.n[1] + j) * self.n[2] + k
    }
}

impl<T: Clone> Grid3<T> {
    /// Allocate a `n[0] × n[1] × n[2]` grid filled with `fill`, checking the 300M voxel cap first
    /// and using `try_reserve` so allocation failures return the cap message instead of aborting.
    pub fn try_new(n: [usize; 3], fill: T) -> Result<Self, MeshError> {
        check_grid(n)?;
        let len = n[0]
            .checked_mul(n[1])
            .and_then(|ab| ab.checked_mul(n[2]))
            .ok_or_else(cap_error)?;
        let mut data = Vec::new();
        data.try_reserve(len).map_err(|_| cap_error())?;
        data.resize(len, fill);
        Ok(Self { n, data })
    }
}

/// 1.x `_grid_for`: grid origin and dimensions covering `[lo, hi]` at `pitch` with `pad` border cells.
pub fn grid_for(lo: [f64; 3], hi: [f64; 3], pitch: f64, pad: usize) -> ([f64; 3], [usize; 3]) {
    let pad_mm = (pad as f64) * pitch;
    let origin = [lo[0] - pad_mm, lo[1] - pad_mm, lo[2] - pad_mm];
    let dim = |a: usize| -> usize {
        let raw = ((hi[a] - lo[a]) / pitch).ceil();
        let base = if raw.is_nan() || raw <= 0.0 {
            0
        } else if !raw.is_finite() {
            usize::MAX
        } else {
            raw as usize
        };
        base.saturating_add(1).saturating_add(pad.saturating_mul(2))
    };
    (origin, [dim(0), dim(1), dim(2)])
}

/// 1.x `_check_grid`: reject grids over `300e6` voxels before allocating.
pub fn check_grid(n: [usize; 3]) -> Result<(), MeshError> {
    let prod = (n[0] as f64) * (n[1] as f64) * (n[2] as f64);
    if !(prod <= VOXEL_CAP) {
        return Err(cap_error());
    }
    Ok(())
}

/// 1.x `_raster_evenodd`: even-odd scanline fill of 2D polygons onto a row-major `(nx, ny)` grid
/// of sample points at `(ox + i * pitch, oy + j * pitch)`.
pub fn raster_evenodd(
    polys: &[Vec<[f64; 2]>],
    ox: f64,
    oy: f64,
    pitch: f64,
    nx: usize,
    ny: usize,
) -> Vec<bool> {
    let total_cells = nx.saturating_mul(ny);
    let mut out = vec![false; total_cells];
    if polys.is_empty() || nx == 0 || ny == 0 {
        return out;
    }

    let mut row_crossings: Vec<Vec<f64>> = vec![Vec::new(); nx];
    let nx_i64 = nx as i64;
    for poly in polys {
        let m = poly.len();
        if m < 2 {
            continue;
        }
        for idx in 0..m {
            let p0 = poly[idx];
            let p1 = poly[(idx + 1) % m];
            let gx0 = (p0[0] - ox) / pitch;
            let gy0 = (p0[1] - oy) / pitch;
            let gx1 = (p1[0] - ox) / pitch;
            let gy1 = (p1[1] - oy) / pitch;

            let lo = (gx0.min(gx1).ceil() as i64).max(0);
            let hi = (gx0.max(gx1).ceil() as i64).saturating_sub(1).min(nx_i64 - 1);
            if lo <= hi {
                let dx = gx1 - gx0;
                let denom = if dx == 0.0 { 1.0 } else { dx };
                let dy = gy1 - gy0;
                for r in lo..=hi {
                    let t = (r as f64 - gx0) / denom;
                    let yc = gy0 + t * dy;
                    row_crossings[r as usize].push(yc);
                }
            }
        }
    }

    let ny_i64 = ny as i64;
    for (i, yc) in row_crossings.iter_mut().enumerate() {
        if yc.is_empty() || yc.len() % 2 != 0 {
            continue;
        }
        yc.sort_by(f64::total_cmp);
        let row_out = &mut out[i * ny..(i + 1) * ny];
        for pair in yc.chunks_exact(2) {
            let ja = (pair[0].ceil() as i64).clamp(0, ny_i64) as usize;
            let jb = (pair[1].ceil() as i64).clamp(0, ny_i64) as usize;
            if ja < jb {
                row_out[ja..jb].fill(true);
            }
        }
    }

    out
}

/// 1.x `solid_occupancy`: exact inside/outside grid for a watertight solid from z-slices.
/// Voxel `(i, j, k)` has its centre at `origin + (i, j, k) * pitch`.
pub fn solid_occupancy(s: &Solid, pitch: f64) -> Result<(Grid3<bool>, [f64; 3]), MeshError> {
    let bb = s.bounding_box();
    let (origin, n) = grid_for(bb.min, bb.max, pitch, 2);
    check_grid(n)?;
    let mut occ = Grid3::try_new(n, false)?;
    let n2 = n[2];
    for k in 0..n2 {
        let z = origin[2] + (k as f64) * pitch;
        if z <= bb.min[2] || z >= bb.max[2] {
            continue;
        }
        let shape = s.slice(z);
        let plane = raster_evenodd(&shape.contours, origin[0], origin[1], pitch, n[0], n[1]);
        for (p, &inside) in plane.iter().enumerate() {
            if inside {
                occ.data[p * n2 + k] = true;
            }
        }
    }
    Ok((occ, origin))
}
