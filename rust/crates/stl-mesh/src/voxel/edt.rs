//! Exact Euclidean distance transform, ported from scipy v1.18.1
//! (`scipy/ndimage/src/ni_morphology.c`: `NI_EuclideanFeatureTransform`, `_ComputeFT`,
//! `_VoronoiFT`; `scipy/ndimage/_morphology.py`: `distance_transform_edt`).
//! scipy is Copyright (c) 2001-2002 Enthought, Inc. 2003, SciPy Developers, BSD-3-Clause.
//!
//! The algorithm is C. R. Maurer, Jr., R. Qi, V. Raghavan, "A linear time algorithm for
//! computing exact euclidean distance transforms of binary images in arbitrary dimensions",
//! IEEE Trans. PAMI 25, 265-270, 2003.

use super::Grid3;

/// One `_VoronoiFT` pass along axis `d` over the cells `line(0..len)`. `feat` holds the
/// feature (nearest background cell) of every cell, with `-1` in the first component where
/// none is known yet. `coor` is the line's position on the other two axes.
fn voronoi_ft(
    feat: &mut [[i32; 3]],
    line: impl Fn(usize) -> usize,
    len: usize,
    coor: [i64; 3],
    d: usize,
    f: &mut [[i32; 3]],
    g: &mut [usize],
) {
    for ii in 0..len {
        f[ii] = feat[line(ii)];
    }
    let mut l: isize = -1;
    for ii in 0..len {
        if f[ii][0] >= 0 {
            let fd = f[ii][d] as f64;
            let mut w_r = 0.0f64;
            for jj in 0..3 {
                if jj != d {
                    let tw = f[ii][jj] as f64 - coor[jj] as f64;
                    w_r += tw * tw;
                }
            }
            while l >= 1 {
                let idx1 = g[l as usize];
                let f1 = f[idx1][d] as f64;
                let idx2 = g[l as usize - 1];
                let a = f1 - f[idx2][d] as f64;
                let b = fd - f1;
                let c = a + b;
                let (mut u_r, mut v_r) = (0.0f64, 0.0f64);
                for jj in 0..3 {
                    if jj != d {
                        let cc = coor[jj] as f64;
                        let tu = f[idx2][jj] as f64 - cc;
                        let tv = f[idx1][jj] as f64 - cc;
                        u_r += tu * tu;
                        v_r += tv * tv;
                    }
                }
                if c * v_r - b * u_r - a * w_r - a * b * c <= 0.0 {
                    break;
                }
                l -= 1;
            }
            l += 1;
            g[l as usize] = ii;
        }
    }
    let maxl = l;
    if maxl >= 0 {
        let maxl = maxl as usize;
        let dist2 = |fe: &[i32; 3], ii: usize| -> f64 {
            let mut s = 0.0f64;
            for jj in 0..3 {
                let t = if jj == d { fe[jj] as f64 - ii as f64 } else { fe[jj] as f64 - coor[jj] as f64 };
                s += t * t;
            }
            s
        };
        let mut l = 0usize;
        for ii in 0..len {
            let mut delta1 = dist2(&f[g[l]], ii);
            while l < maxl {
                let delta2 = dist2(&f[g[l + 1]], ii);
                if delta1 <= delta2 {
                    break;
                }
                delta1 = delta2;
                l += 1;
            }
            feat[line(ii)] = f[g[l]];
        }
    }
}

/// scipy `ndimage.distance_transform_edt(occ)`: for each true cell, the distance in voxels to
/// the nearest false cell (0 on false cells). If there is no false cell at all, the result is
/// what scipy gives then: every cell's feature stays `(-1, 0, 0)`, so cell `(i, j, k)` gets
/// `sqrt((i + 1)² + j² + k²)`.
pub fn distance_transform_edt(occ: &Grid3<bool>) -> Result<Grid3<f64>, crate::MeshError> {
    let n = occ.n;
    let mut feat_grid: Grid3<[i32; 3]> = Grid3::try_new(n, [0; 3])?;
    let feat = &mut feat_grid.data;
    let mx = n.iter().copied().max().unwrap_or(0);
    let mut f = vec![[0i32; 3]; mx];
    let mut g = vec![0usize; mx];
    let at = |i: usize, j: usize, k: usize| (i * n[1] + j) * n[2] + k;

    // `_ComputeFT` recurses from the last axis down; each pass only reads and writes its own
    // line, so running every axis-0 line, then every axis-1 line, then every axis-2 line gives
    // the same features.
    for j in 0..n[1] {
        for k in 0..n[2] {
            for i in 0..n[0] {
                let c = at(i, j, k);
                feat[c] = if occ.data[c] { [-1, 0, 0] } else { [i as i32, j as i32, k as i32] };
            }
            voronoi_ft(feat, |ii| at(ii, j, k), n[0], [0, j as i64, k as i64], 0, &mut f, &mut g);
        }
    }
    for k in 0..n[2] {
        for i in 0..n[0] {
            voronoi_ft(feat, |ii| at(i, ii, k), n[1], [i as i64, 0, k as i64], 1, &mut f, &mut g);
        }
    }
    for i in 0..n[0] {
        for j in 0..n[1] {
            voronoi_ft(feat, |ii| at(i, j, ii), n[2], [i as i64, j as i64, 0], 2, &mut f, &mut g);
        }
    }

    let mut out = Grid3::try_new(n, 0.0f64)?;
    for i in 0..n[0] {
        for j in 0..n[1] {
            for k in 0..n[2] {
                let c = at(i, j, k);
                let fe = feat[c];
                let d0 = (fe[0] - i as i32) as f64;
                let d1 = (fe[1] - j as i32) as f64;
                let d2 = (fe[2] - k as i32) as f64;
                out.data[c] = (d0 * d0 + d1 * d1 + d2 * d2).sqrt();
            }
        }
    }
    Ok(out)
}
