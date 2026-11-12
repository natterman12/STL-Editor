//! Round holes and pins: find the triangle nearest a click and recognise the cylinder around
//! it (1.x `meshtools.nearest_triangle` / `fit_cylinder`).

use std::collections::VecDeque;

use crate::refs::fit_circle;
use crate::MeshError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CylKind {
    Hole,
    Pin,
}

/// A fitted round hole or pin: the axis runs from `z0` to `z1` measured from `center`.
#[derive(Debug, Clone)]
pub struct Cylinder {
    pub center: [f64; 3],
    pub axis: [f64; 3],
    pub radius: f64,
    pub z0: f64,
    pub z1: f64,
    pub kind: CylKind,
    pub tris: Vec<usize>,
    pub cover: f64,
}

type V3 = [f64; 3];

fn p(v: &[[f32; 3]], i: u32) -> V3 {
    let a = v[i as usize];
    [a[0] as f64, a[1] as f64, a[2] as f64]
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn unit(a: V3) -> V3 {
    let n = dot(a, a).sqrt();
    if n == 0.0 {
        a
    } else {
        [a[0] / n, a[1] / n, a[2] / n]
    }
}

/// The index of the triangle whose centre is closest to `point` (the first, on a tie).
pub fn nearest_triangle(v: &[[f32; 3]], f: &[[u32; 3]], point: [f64; 3]) -> usize {
    let mut best = 0;
    let mut bd = f64::INFINITY;
    for (i, t) in f.iter().enumerate() {
        let (a, b, c) = (p(v, t[0]), p(v, t[1]), p(v, t[2]));
        let m = [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0, (a[2] + b[2] + c[2]) / 3.0];
        let d = sub(m, point);
        let d2 = dot(d, d);
        if d2 < bd {
            bd = d2;
            best = i;
        }
    }
    best
}

/// Eigen-decomposition of a symmetric 3x3 matrix (Jacobi): eigenvalues ascending, with the
/// matching unit eigenvectors as `vecs[k]`.
fn eigh(m: [[f64; 3]; 3]) -> ([f64; 3], [V3; 3]) {
    let mut a = m;
    let mut v = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..60 {
        let off = a[0][1].abs() + a[0][2].abs() + a[1][2].abs();
        if off < 1e-300 {
            break;
        }
        for (i, j) in [(0, 1), (0, 2), (1, 2)] {
            if a[i][j].abs() < 1e-300 {
                continue;
            }
            let theta = (a[j][j] - a[i][i]) / (2.0 * a[i][j]);
            let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
            let t = if theta == 0.0 { 1.0 } else { t };
            let c = 1.0 / (t * t + 1.0).sqrt();
            let s = t * c;
            for k in 0..3 {
                let (aki, akj) = (a[k][i], a[k][j]);
                a[k][i] = c * aki - s * akj;
                a[k][j] = s * aki + c * akj;
            }
            for k in 0..3 {
                let (aik, ajk) = (a[i][k], a[j][k]);
                a[i][k] = c * aik - s * ajk;
                a[j][k] = s * aik + c * ajk;
            }
            for k in 0..3 {
                let (vki, vkj) = (v[k][i], v[k][j]);
                v[k][i] = c * vki - s * vkj;
                v[k][j] = s * vki + c * vkj;
            }
        }
    }
    let mut idx = [0usize, 1, 2];
    idx.sort_by(|&x, &y| a[x][x].partial_cmp(&a[y][y]).unwrap_or(std::cmp::Ordering::Equal));
    let vals = [a[idx[0]][idx[0]], a[idx[1]][idx[1]], a[idx[2]][idx[2]]];
    let vecs = [0, 1, 2].map(|r| unit([v[0][idx[r]], v[1][idx[r]], v[2][idx[r]]]));
    (vals, vecs)
}

fn percentile(sorted: &[f64], q: f64) -> f64 {
    let n = sorted.len();
    if n == 0 {
        return 0.0;
    }
    let pos = q / 100.0 * (n - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = (lo + 1).min(n - 1);
    sorted[lo] + (sorted[hi] - sorted[lo]) * (pos - lo as f64)
}

fn msg(s: &str) -> MeshError {
    MeshError::Message(s.to_string())
}

/// Recognise the round hole or pin around triangle `tri`; surfaces whose normals turn by
/// more than `max_angle` degrees from one triangle to the next stop the search.
pub fn fit_cylinder(v: &[[f32; 3]], f: &[[u32; 3]], tri: usize, max_angle: f64) -> Result<Cylinder, MeshError> {
    let nf = f.len();
    let mut nrm = Vec::with_capacity(nf);
    let mut area = Vec::with_capacity(nf);
    for t in f {
        let (a, b, c) = (p(v, t[0]), p(v, t[1]), p(v, t[2]));
        let n = cross(sub(b, a), sub(c, a));
        let l = dot(n, n).sqrt();
        nrm.push(if l > 0.0 { [n[0] / l, n[1] / l, n[2] / l] } else { [0.0; 3] });
        area.push(l / 2.0);
    }
    // triangles that share an edge
    let mut edges: Vec<(u32, u32, usize)> = Vec::with_capacity(nf * 3);
    for (i, t) in f.iter().enumerate() {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            edges.push((a.min(b), a.max(b), i));
        }
    }
    edges.sort();
    let cos_t = max_angle.to_radians().cos();
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); nf];
    for w in edges.windows(2) {
        if w[0].0 == w[1].0 && w[0].1 == w[1].1 {
            let (a, b) = (w[0].2, w[1].2);
            if dot(nrm[a], nrm[b]) >= cos_t {
                adj[a].push(b);
                adj[b].push(a);
            }
        }
    }
    if tri >= nf {
        return Err(msg("Click the curved wall of a round hole or pin."));
    }
    let mut seen = vec![false; nf];
    let mut region = Vec::new();
    let mut q = VecDeque::new();
    seen[tri] = true;
    q.push_back(tri);
    while let Some(i) = q.pop_front() {
        region.push(i);
        for &j in &adj[i] {
            if !seen[j] {
                seen[j] = true;
                q.push_back(j);
            }
        }
    }
    if region.len() < 6 {
        return Err(msg("Click the curved wall of a round hole or pin."));
    }
    let mut m = [[0.0; 3]; 3];
    for &i in &region {
        for r in 0..3 {
            for c in 0..3 {
                m[r][c] += nrm[i][r] * area[i] * nrm[i][c];
            }
        }
    }
    let (evals, evecs) = eigh(m);
    let axis = evecs[0];
    if evals[0] > 0.08 * evals[1] || evals[1] < 0.25 * evals[2] {
        return Err(msg("That surface isn't round like a hole or pin. Click the curved wall itself."));
    }
    let rf = if axis[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let ex = unit(cross(axis, rf));
    let ey = cross(axis, ex);
    let mut vi: Vec<u32> = region.iter().flat_map(|&i| f[i]).collect();
    vi.sort_unstable();
    vi.dedup();
    let pts: Vec<V3> = vi.iter().map(|&i| p(v, i)).collect();
    let p2: Vec<[f64; 2]> = pts.iter().map(|q| [dot(*q, ex), dot(*q, ey)]).collect();
    let (cx, cy, r) = fit_circle(&p2);
    if r <= 0.0 {
        return Err(msg("Couldn't measure that surface."));
    }
    let mut dev: Vec<f64> = p2.iter().map(|q| ((q[0] - cx).hypot(q[1] - cy) - r).abs()).collect();
    dev.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if percentile(&dev, 90.0) > 0.05_f64.max(0.04 * r) {
        return Err(msg("That surface isn't a clean cylinder, so it can't be resized."));
    }
    let mut bins = [false; 40];
    for q in &p2 {
        let ang = (q[1] - cy).atan2(q[0] - cx);
        let b = ((ang + std::f64::consts::PI) / (2.0 * std::f64::consts::PI) * 36.0).floor() as usize;
        bins[b.min(39)] = true;
    }
    let cover = bins.iter().filter(|b| **b).count() as f64 / 36.0;
    if cover < 0.5 {
        return Err(msg("Only part of a round surface was found. Click the wall of a full hole or pin."));
    }
    let zs: Vec<f64> = pts.iter().map(|q| dot(*q, axis)).collect();
    let z0 = zs.iter().cloned().fold(f64::INFINITY, f64::min);
    let z1 = zs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let center = [cx * ex[0] + cy * ey[0], cx * ex[1] + cy * ey[1], cx * ex[2] + cy * ey[2]];
    let mut s = 0.0;
    for &i in &region {
        let t = f[i];
        let (a, b, c) = (p(v, t[0]), p(v, t[1]), p(v, t[2]));
        let tc = [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0, (a[2] + b[2] + c[2]) / 3.0];
        let h = dot(tc, axis);
        let radial = [tc[0] - h * axis[0] - center[0], tc[1] - h * axis[1] - center[1], tc[2] - h * axis[2] - center[2]];
        s += dot(radial, nrm[i]);
    }
    let kind = if s / (region.len() as f64) < 0.0 { CylKind::Hole } else { CylKind::Pin };
    region.sort_unstable();
    Ok(Cylinder { center, axis, radius: r, z0, z1, kind, tris: region, cover })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A square block with a round hole through it, as a triangle soup of just the wall.
    fn tube(r: f64, h: f64, n: usize, inward: bool) -> (Vec<[f32; 3]>, Vec<[u32; 3]>) {
        let mut v = Vec::new();
        for i in 0..n {
            let a = i as f64 / n as f64 * std::f64::consts::TAU;
            v.push([(r * a.cos()) as f32, (r * a.sin()) as f32, 0.0]);
            v.push([(r * a.cos()) as f32, (r * a.sin()) as f32, h as f32]);
        }
        let mut f = Vec::new();
        for i in 0..n {
            let j = (i + 1) % n;
            let (a, b, c, d) = (2 * i as u32, 2 * i as u32 + 1, 2 * j as u32, 2 * j as u32 + 1);
            if !inward {
                f.push([a, d, b]);
                f.push([a, c, d]);
            } else {
                f.push([a, b, d]);
                f.push([a, d, c]);
            }
        }
        (v, f)
    }

    #[test]
    fn finds_a_pin() {
        let (v, f) = tube(3.0, 10.0, 48, false);
        let t = nearest_triangle(&v, &f, [3.0, 0.0, 5.0]);
        let c = fit_cylinder(&v, &f, t, 35.0).unwrap();
        assert!((c.radius - 3.0).abs() < 1e-3, "{}", c.radius);
        assert!(((c.z1 - c.z0) - 10.0).abs() < 1e-3);
        assert_eq!(c.kind, CylKind::Pin);
        assert!(c.axis[2].abs() > 0.999);
    }

    #[test]
    fn finds_a_hole() {
        let (v, f) = tube(2.0, 6.0, 48, true);
        let c = fit_cylinder(&v, &f, 0, 35.0).unwrap();
        assert_eq!(c.kind, CylKind::Hole);
    }

    #[test]
    fn flat_is_refused() {
        let v = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let f = vec![[0, 1, 2]];
        assert!(fit_cylinder(&v, &f, 0, 35.0).is_err());
    }
}
