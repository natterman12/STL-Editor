//! 1.x `printing.rotation_to`, `best_rest_normal` and `lay_flat_matrix`: standing a part on
//! its most stable flat side.

use std::collections::BTreeMap;

use stl_kernel::Solid;

use super::frames::{cross, dot, unit, Vec3};
use crate::FeatureError;

/// Rotation matrix taking unit vector `a` onto unit vector `b`.
pub fn rotation_to(a: Vec3, b: Vec3) -> Result<[[f64; 3]; 3], FeatureError> {
    let (a, b) = (unit(a)?, unit(b)?);
    let v = cross(a, b);
    let c = dot(a, b);
    if c < -0.999999 {
        // opposite: half turn about any axis perpendicular to a
        let axis = if a[0].abs() < 0.9 { unit(cross(a, [1.0, 0.0, 0.0]))? } else { unit(cross(a, [0.0, 1.0, 0.0]))? };
        let mut r = [[0.0; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                r[i][j] = 2.0 * axis[i] * axis[j] - if i == j { 1.0 } else { 0.0 };
            }
        }
        return Ok(r);
    }
    let k = [[0.0, -v[2], v[1]], [v[2], 0.0, -v[0]], [-v[1], v[0], 0.0]];
    let mut r = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            let mut kk = 0.0;
            for m in 0..3 {
                kk += k[i][m] * k[m][j];
            }
            r[i][j] = (if i == j { 1.0 } else { 0.0 }) + k[i][j] + kk / (1.0 + c);
        }
    }
    Ok(r)
}

/// Outward normal of the largest flat area of the convex hull: the most stable way to stand
/// the part on the bed.
pub fn best_rest_normal(solid: &Solid) -> Result<Vec3, FeatureError> {
    let hull = solid.hull().to_mesh();
    let v: Vec<Vec3> = hull.positions.chunks_exact(3).map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]).collect();
    // np.unique(np.round(n, 3)) sorts the keys lexicographically; a BTreeMap on the same
    // rounded values keeps that order. (key -> (total area, triangle indices))
    let mut groups: BTreeMap<[i64; 3], (f64, Vec<(Vec3, f64)>)> = BTreeMap::new();
    for t in hull.triangles.chunks_exact(3) {
        let (p0, p1, p2) = (v[t[0] as usize], v[t[1] as usize], v[t[2] as usize]);
        let c = cross([p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]], [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]]);
        let area = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt() / 2.0;
        let n = if area > 0.0 { [c[0] / (2.0 * area), c[1] / (2.0 * area), c[2] / (2.0 * area)] } else { [0.0; 3] };
        let key = [0, 1, 2].map(|i| (n[i] * 1000.0).round_ties_even() as i64);
        let g = groups.entry(key).or_insert((0.0, Vec::new()));
        g.0 += area;
        g.1.push((n, area));
    }
    let mut best: Option<&(f64, Vec<(Vec3, f64)>)> = None;
    for g in groups.values() {
        if best.map_or(true, |b| g.0 > b.0) {
            best = Some(g);
        }
    }
    let Some(best) = best else {
        return Err(FeatureError("Zero-length direction.".into()));
    };
    let mut sum = [0.0; 3];
    for (n, a) in &best.1 {
        for i in 0..3 {
            sum[i] += n[i] * a;
        }
    }
    unit(sum)
}

/// 3x4 transform: turn `normal` straight down, keep the part's X/Y centre, rest it on Z=0.
pub fn lay_flat_matrix(solid: &Solid, normal: Vec3) -> Result<[[f64; 4]; 3], FeatureError> {
    let r = rotation_to(normal, [0.0, 0.0, -1.0])?;
    let b = solid.bounding_box();
    let c = [0, 1, 2].map(|i| (b.min[i] + b.max[i]) / 2.0);
    let mut m = [[0.0; 4]; 3];
    for i in 0..3 {
        let rc = r[i][0] * c[0] + r[i][1] * c[1] + r[i][2] * c[2];
        m[i] = [r[i][0], r[i][1], r[i][2], c[i] - rc];
    }
    let nb = solid.transform(m).bounding_box();
    let nc = [0, 1, 2].map(|i| (nb.min[i] + nb.max[i]) / 2.0);
    m[0][3] += c[0] - nc[0];
    m[1][3] += c[1] - nc[1];
    m[2][3] += -nb.min[2];
    Ok(m)
}

/// `(0..n).map(f).collect()`, run on several threads where the platform has them (not wasm32).
/// For a few big, independent jobs such as moving each copy of a large body; results come back
/// in index order, so the answer does not depend on the threads.
pub fn par_map<R: Send>(n: usize, f: impl Fn(usize) -> R + Sync) -> Vec<R> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let workers = std::thread::available_parallelism().map_or(1, |p| p.get()).min(n);
        if workers > 1 {
            use std::sync::atomic::{AtomicUsize, Ordering};
            let next = AtomicUsize::new(0);
            let mut out: Vec<Option<R>> = (0..n).map(|_| None).collect();
            let done = std::sync::Mutex::new(&mut out);
            std::thread::scope(|s| {
                for _ in 0..workers {
                    s.spawn(|| loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= n {
                            break;
                        }
                        let r = f(i);
                        done.lock().unwrap()[i] = Some(r);
                    });
                }
            });
            return out.into_iter().map(|r| r.unwrap()).collect();
        }
    }
    (0..n).map(f).collect()
}
