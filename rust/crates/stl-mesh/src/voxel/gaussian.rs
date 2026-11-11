//! 3D separable Gaussian filter (truncate = 4.0, mode = "reflect").
//!
//! Ported from SciPy v1.18.1 (`scipy/ndimage/_filters.py`, `scipy/ndimage/src/ni_filters.c`,
//! and `scipy/ndimage/src/ni_support.c`), Copyright (C) 2003-2005 Peter J. Verveer and SciPy
//! Developers, licensed under the BSD-3-Clause licence.

use super::Grid3;

/// Extend `buf[lw .. lw + len]` on both sides by `lw` samples using `NI_EXTEND_REFLECT`
/// (`d c b a | a b c d | d c b a`, repeating periodically when `lw > len`), matching
/// `NI_ExtendLine` in SciPy's `ni_support.c`.
fn extend_reflect(buf: &mut [f64], len: usize, lw: usize) {
    if lw == 0 {
        return;
    }
    let first = lw;
    let last = lw + len;

    let mut src = first;
    let mut dst = first;
    let mut rem = lw;
    while rem > 0 && src < last {
        dst -= 1;
        buf[dst] = buf[src];
        src += 1;
        rem -= 1;
    }
    src = last - 1;
    while rem > 0 {
        dst -= 1;
        buf[dst] = buf[src];
        src -= 1;
        rem -= 1;
    }

    let mut src_r = last as isize - 1;
    let mut dst_r = last;
    let mut rem_r = lw;
    while rem_r > 0 && src_r >= first as isize {
        buf[dst_r] = buf[src_r as usize];
        dst_r += 1;
        src_r -= 1;
        rem_r -= 1;
    }
    let mut src_f = first;
    while rem_r > 0 {
        buf[dst_r] = buf[src_f];
        dst_r += 1;
        src_f += 1;
        rem_r -= 1;
    }
}

/// Symmetric 1D correlation matching `NI_Correlate1D` (`symmetric > 0`) in `ni_filters.c`.
#[inline]
fn correlate_symmetric(buf: &[f64], len: usize, lw: usize, weights: &[f64], out: &mut [f32]) {
    let w0 = weights[lw];
    for (ll, slot) in out.iter_mut().enumerate().take(len) {
        let center = lw + ll;
        let mut acc = buf[center] * w0;
        for d in (1..=lw).rev() {
            acc += (buf[center - d] + buf[center + d]) * weights[lw - d];
        }
        *slot = acc as f32;
    }
}

/// 1D Gaussian kernel (`order = 0`, `truncate = 4.0`), matching `_gaussian_kernel1d` and
/// `gaussian_filter1d` in `scipy/ndimage/_filters.py`.
fn gaussian_kernel1d(sigma: f64) -> (usize, Vec<f64>) {
    let lw = (4.0 * sigma + 0.5) as usize;
    let size = 2 * lw + 1;
    let factor = -0.5 / (sigma * sigma);
    let mut weights = Vec::with_capacity(size);
    let mut sum = 0.0f64;
    for k in -(lw as isize)..=(lw as isize) {
        let v = (factor * ((k * k) as f64)).exp();
        weights.push(v);
        sum += v;
    }
    for w in &mut weights {
        *w /= sum;
    }
    weights.reverse();
    (lw, weights)
}

/// In-place 3D Gaussian filter matching `scipy.ndimage.gaussian_filter(field, sigma)` on `float32`
/// arrays (`truncate = 4.0`, `mode = "reflect"`). Separable 1D passes run along axes 0, 1, 2 in
/// `f64` line buffers, storing `f32` back to `field` after each axis pass.
pub fn gaussian_filter(field: &mut Grid3<f32>, sigma: f64) {
    let [n0, n1, n2] = field.n;
    if !(sigma > 1e-15) || n0 == 0 || n1 == 0 || n2 == 0 || field.data.is_empty() {
        return;
    }

    let (lw, weights) = gaussian_kernel1d(sigma);
    let max_len = n0.max(n1).max(n2);
    let mut buf = vec![0.0f64; max_len + 2 * lw];
    let mut out_line = vec![0.0f32; max_len];

    // Axis 0 (stride n1 * n2)
    let stride0 = n1 * n2;
    for j in 0..n1 {
        for k in 0..n2 {
            let base = j * n2 + k;
            for i in 0..n0 {
                buf[lw + i] = field.data[base + i * stride0] as f64;
            }
            extend_reflect(&mut buf, n0, lw);
            correlate_symmetric(&buf, n0, lw, &weights, &mut out_line[..n0]);
            for i in 0..n0 {
                field.data[base + i * stride0] = out_line[i];
            }
        }
    }

    // Axis 1 (stride n2)
    let plane_stride = n1 * n2;
    for i in 0..n0 {
        let i_base = i * plane_stride;
        for k in 0..n2 {
            let base = i_base + k;
            for j in 0..n1 {
                buf[lw + j] = field.data[base + j * n2] as f64;
            }
            extend_reflect(&mut buf, n1, lw);
            correlate_symmetric(&buf, n1, lw, &weights, &mut out_line[..n1]);
            for j in 0..n1 {
                field.data[base + j * n2] = out_line[j];
            }
        }
    }

    // Axis 2 (contiguous stride 1)
    for row in field.data.chunks_exact_mut(n2) {
        for (k, &v) in row.iter().enumerate() {
            buf[lw + k] = v as f64;
        }
        extend_reflect(&mut buf, n2, lw);
        correlate_symmetric(&buf, n2, lw, &weights, row);
    }
}
