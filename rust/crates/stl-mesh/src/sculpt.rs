//! Sculpt brushes (1.x `printing.sculpt`): smooth, flatten, inflate and deflate strokes that move
//! the vertices near a path over the surface.

use serde::Deserialize;
use stl_kernel::{MeshArrays, Solid};

use crate::nearest::PointTree;
use crate::MeshError;

fn d_radius() -> f64 {
    5.0
}
fn d_strength() -> f64 {
    0.5
}
fn d_mode() -> String {
    "smooth".into()
}

/// One brush stroke, as 1.x stores it.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Stroke {
    #[serde(default)]
    pub points: Vec<[f64; 3]>,
    #[serde(default = "d_radius")]
    pub radius: f64,
    #[serde(default = "d_strength")]
    pub strength: f64,
    #[serde(default = "d_mode")]
    pub mode: String,
}

/// Distinct neighbours of every vertex (the 0/1 adjacency matrix of 1.x), sorted.
fn adjacency(f: &[[usize; 3]], n: usize) -> Vec<Vec<usize>> {
    let mut adj = vec![Vec::new(); n];
    for t in f {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            adj[a].push(b);
            adj[b].push(a);
        }
    }
    let tidy = |l: &mut Vec<usize>| {
        l.sort_unstable();
        l.dedup();
    };
    #[cfg(not(target_arch = "wasm32"))]
    {
        use rayon::prelude::*;
        adj.par_iter_mut().for_each(tidy);
    }
    #[cfg(target_arch = "wasm32")]
    adj.iter_mut().for_each(tidy);
    adj
}

fn vertex_normals(v: &[[f64; 3]], f: &[[usize; 3]]) -> Vec<[f64; 3]> {
    let mut vn = vec![[0.0; 3]; v.len()];
    for t in f {
        let (a, b, c) = (v[t[0]], v[t[1]], v[t[2]]);
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let w = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]];
        for &i in t {
            for k in 0..3 {
                vn[i][k] += n[k];
            }
        }
    }
    for n in &mut vn {
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if l > 0.0 {
            for k in 0..3 {
                n[k] /= l;
            }
        } else {
            *n = [0.0; 3];
        }
    }
    vn
}

/// Python's `a // b` for positive floats.
fn floordiv(a: f64, b: f64) -> f64 {
    let m = a % b;
    let div = (a - m) / b;
    if div == 0.0 {
        return 0.0;
    }
    let fl = div.floor();
    if div - fl > 0.5 { fl + 1.0 } else { fl }
}

fn resample(p: &[[f64; 3]], spacing: f64) -> Vec<[f64; 3]> {
    if p.len() < 2 {
        return p.to_vec();
    }
    let mut out = vec![p[0]];
    for w in p.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let k = floordiv(l, spacing) as i64;
        for i in 1..=k {
            let s = i as f64 * spacing / l;
            out.push([a[0] + d[0] * s, a[1] + d[1] * s, a[2] + d[2] * s]);
        }
        out.push(b);
    }
    out
}

/// Eigenvector of the smallest eigenvalue of a symmetric 3x3 matrix (cyclic Jacobi). Its sign
/// is arbitrary, which does not matter to the Flatten brush.
fn smallest_eigvec(mut a: [[f64; 3]; 3]) -> [f64; 3] {
    let mut v = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..60 {
        let off = a[0][1].abs() + a[0][2].abs() + a[1][2].abs();
        if off < 1e-300 {
            break;
        }
        for (p, q) in [(0, 1), (0, 2), (1, 2)] {
            if a[p][q] == 0.0 {
                continue;
            }
            let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
            let t = if theta == 0.0 { 1.0 } else { theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt()) };
            let c = 1.0 / (t * t + 1.0).sqrt();
            let s = t * c;
            for k in 0..3 {
                let (akp, akq) = (a[k][p], a[k][q]);
                a[k][p] = c * akp - s * akq;
                a[k][q] = s * akp + c * akq;
            }
            for k in 0..3 {
                let (apk, aqk) = (a[p][k], a[q][k]);
                a[p][k] = c * apk - s * aqk;
                a[q][k] = s * apk + c * aqk;
            }
            for k in 0..3 {
                let (vkp, vkq) = (v[k][p], v[k][q]);
                v[k][p] = c * vkp - s * vkq;
                v[k][q] = s * vkp + c * vkq;
            }
        }
    }
    let mut m = 0;
    for i in 1..3 {
        if a[i][i] < a[m][m] {
            m = i;
        }
    }
    [v[0][m], v[1][m], v[2][m]]
}

/// Move the vertices under each stroke. Errors with 1.x's messages for an unknown brush or a
/// result that no longer closes up.
pub fn sculpt(s: &Solid, strokes: &[Stroke]) -> Result<Solid, MeshError> {
    let mm = s.to_mesh();
    let mut v: Vec<[f64; 3]> = mm.positions.chunks_exact(3).map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]).collect();
    let f: Vec<[usize; 3]> =
        mm.triangles.chunks_exact(3).map(|t| [t[0] as usize, t[1] as usize, t[2] as usize]).collect();
    let adj = adjacency(&f, v.len());
    let lap = |v: &[[f64; 3]], idx: &[usize]| -> Vec<[f64; 3]> {
        idx.iter()
            .map(|&i| {
                let deg = adj[i].len().max(1) as f64;
                let mut sum = [0.0; 3];
                for &j in &adj[i] {
                    for k in 0..3 {
                        sum[k] += v[j][k];
                    }
                }
                [sum[0] / deg - v[i][0], sum[1] / deg - v[i][1], sum[2] / deg - v[i][2]]
            })
            .collect()
    };
    for st in strokes {
        let r = st.radius;
        let k = st.strength.clamp(0.0, 1.0);
        let samples = resample(&st.points, r / 4.0);
        if samples.is_empty() || r <= 0.0 {
            continue;
        }
        let tree = PointTree::new(&v);
        let mut idx: Vec<usize> = samples.iter().flat_map(|q| tree.within(*q, r)).collect();
        idx.sort_unstable();
        idx.dedup();
        if idx.is_empty() {
            continue;
        }
        let stree = PointTree::new(&samples);
        let w: Vec<f64> = idx
            .iter()
            .map(|&i| {
                let dd = stree.nearest_dist(v[i]);
                let x = (dd / r).clamp(0.0, 1.0);
                let y = 1.0 - x * x;
                y * y * k
            })
            .collect();
        match st.mode.as_str() {
            "smooth" => {
                for _ in 0..4 {
                    let l = lap(&v, &idx);
                    for (n, &i) in idx.iter().enumerate() {
                        for c in 0..3 {
                            v[i][c] += 0.6 * w[n] * l[n][c];
                        }
                    }
                }
            }
            "flatten" => {
                let wsum: f64 = w.iter().sum();
                let mut c = [0.0; 3];
                for (n, &i) in idx.iter().enumerate() {
                    for a in 0..3 {
                        c[a] += v[i][a] * w[n];
                    }
                }
                for a in 0..3 {
                    c[a] /= wsum.max(1e-12);
                }
                let mut cov = [[0.0; 3]; 3];
                for (n, &i) in idx.iter().enumerate() {
                    let d = [v[i][0] - c[0], v[i][1] - c[1], v[i][2] - c[2]];
                    for a in 0..3 {
                        for b in 0..3 {
                            cov[a][b] += d[a] * d[b] * w[n];
                        }
                    }
                }
                let nrm = smallest_eigvec(cov);
                let dists: Vec<f64> = idx
                    .iter()
                    .map(|&i| (v[i][0] - c[0]) * nrm[0] + (v[i][1] - c[1]) * nrm[1] + (v[i][2] - c[2]) * nrm[2])
                    .collect();
                for (n, &i) in idx.iter().enumerate() {
                    for a in 0..3 {
                        v[i][a] -= w[n] * dists[n] * nrm[a];
                    }
                }
            }
            "inflate" | "deflate" => {
                let vn = vertex_normals(&v, &f);
                let sgn = if st.mode == "inflate" { 1.0 } else { -1.0 };
                for (n, &i) in idx.iter().enumerate() {
                    for a in 0..3 {
                        v[i][a] += sgn * (w[n] * r * 0.12) * vn[i][a];
                    }
                }
                let l = lap(&v, &idx);
                for (n, &i) in idx.iter().enumerate() {
                    for a in 0..3 {
                        v[i][a] += 0.3 * w[n] * l[n][a];
                    }
                }
            }
            other => return Err(MeshError::Message(format!("Unknown brush {other}."))),
        }
    }
    let arrays = MeshArrays {
        positions: v.iter().flat_map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect(),
        triangles: mm.triangles.clone(),
    };
    Solid::from_mesh(&arrays).map_err(|_| {
        MeshError::Message("The brush strokes folded the surface over itself. Use a smaller strength or radius.".into())
    })
}
