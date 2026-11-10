//! `geometry._touching_shells`, `merge_overlapping_shells` and `_is_flat` from 1.x.

use stl_kernel::Solid;

use super::{cap_holes, fix_winding, reversed, to_solid};

/// The eigenvalues of a symmetric 3x3 matrix, by Jacobi rotations.
fn symmetric_eigenvalues(mut a: [[f64; 3]; 3]) -> [f64; 3] {
    for _ in 0..64 {
        let off = a[0][1].abs() + a[0][2].abs() + a[1][2].abs();
        if off == 0.0 {
            break;
        }
        for (p, q) in [(0usize, 1usize), (0, 2), (1, 2)] {
            if a[p][q] == 0.0 {
                continue;
            }
            let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
            let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
            let t = if theta == 0.0 { 1.0 } else { t };
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
        }
    }
    [a[0][0], a[1][1], a[2][2]]
}

/// `geometry._is_flat`: the points lie in one plane, to within 1e-5 of their size. 1.x takes
/// the smallest singular value of the centred points; here it is the root of the smallest
/// eigenvalue of their 3x3 scatter matrix, which is the same number.
pub(crate) fn is_flat(v: &[f64]) -> bool {
    let n = v.len() / 3;
    if n == 0 {
        return true;
    }
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    let mut mean = [0.0f64; 3];
    for p in v.chunks_exact(3) {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
            mean[k] += p[k];
        }
    }
    let size = (0..3).map(|k| hi[k] - lo[k]).fold(0.0f64, f64::max);
    let size = if size == 0.0 { 1.0 } else { size };
    let mut s = [[0.0f64; 3]; 3];
    for p in v.chunks_exact(3) {
        let c = [p[0] - mean[0] / n as f64, p[1] - mean[1] / n as f64, p[2] - mean[2] / n as f64];
        for i in 0..3 {
            for j in 0..3 {
                s[i][j] += c[i] * c[j];
            }
        }
    }
    let smallest = symmetric_eigenvalues(s).into_iter().fold(f64::INFINITY, f64::min).max(0.0).sqrt();
    smallest / (n as f64).sqrt() <= 1e-5 * size
}

/// `geometry._touching_shells`. Shells that touch each other (sharing a wall or an edge) make
/// edges with more than two triangles. Split the mesh where that happens, close each piece and
/// union the solid ones; flat leftovers (a shared wall, a stray fin) are dropped. `None` when
/// no edge has more than two triangles, or a piece cannot be made solid.
pub fn touching_shells(v: &[f64], f: &[u32]) -> Option<Solid> {
    let nt = f.len() / 3;
    if nt == 0 {
        return None;
    }
    // undirected edges with how many triangles use each
    let key = |k: usize| {
        let (a, b) = (f[k], f[k - k % 3 + (k + 1) % 3]);
        (a.min(b), a.max(b))
    };
    let mut order: Vec<usize> = (0..3 * nt).collect();
    order.sort_by_key(|&k| key(k));
    let mut parent: Vec<u32> = (0..nt as u32).collect();
    fn find(parent: &mut [u32], mut x: u32) -> u32 {
        while parent[x as usize] != x {
            parent[x as usize] = parent[parent[x as usize] as usize];
            x = parent[x as usize];
        }
        x
    }
    let mut any_over_two = false;
    let mut i = 0;
    while i < order.len() {
        let mut j = i + 1;
        while j < order.len() && key(order[j]) == key(order[i]) {
            j += 1;
        }
        if j - i > 2 {
            any_over_two = true;
        } else if j - i == 2 {
            // triangles stay together only across edges that exactly two of them share
            let (ra, rb) = (find(&mut parent, (order[i] / 3) as u32), find(&mut parent, (order[i + 1] / 3) as u32));
            if ra != rb {
                parent[ra.max(rb) as usize] = ra.min(rb);
            }
        }
        i = j;
    }
    if !any_over_two {
        return None;
    }
    // the pieces, in the order of their first triangle (scipy's component labels)
    let mut piece_of_root = vec![usize::MAX; nt];
    let mut pieces: Vec<Vec<usize>> = Vec::new();
    for t in 0..nt {
        let r = find(&mut parent, t as u32) as usize;
        if piece_of_root[r] == usize::MAX {
            piece_of_root[r] = pieces.len();
            pieces.push(Vec::new());
        }
        pieces[piece_of_root[r]].push(t);
    }

    // np.ptp(v, axis=0).max() ** 3. A coordinate that is not a number makes the size NaN, as
    // in numpy, and then no piece counts as too small.
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    let mut nan = false;
    for p in v.chunks_exact(3) {
        for k in 0..3 {
            nan |= p[k].is_nan();
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let extent = (0..3).map(|k| hi[k] - lo[k]).fold(f64::NEG_INFINITY, f64::max);
    // inf - inf on one axis is NaN too, and `f64::max` would hide it
    let nan = nan || (0..3).any(|k| (hi[k] - lo[k]).is_nan());
    let size = if nan { f64::NAN } else { extent.powi(3) };

    let mut solids: Vec<Solid> = Vec::new();
    for piece in pieces {
        // the piece as its own mesh: its vertices in index order
        let mut used: Vec<u32> = piece.iter().flat_map(|&t| f[3 * t..3 * t + 3].iter().copied()).collect();
        used.sort_unstable();
        used.dedup();
        let pv: Vec<f64> = used.iter().flat_map(|&i| v[3 * i as usize..3 * i as usize + 3].iter().copied()).collect();
        let mut pf: Vec<u32> = piece
            .iter()
            .flat_map(|&t| f[3 * t..3 * t + 3].iter().map(|i| used.binary_search(i).unwrap_or(0) as u32))
            .collect();
        fix_winding(&pv, &mut pf);
        let mut m = to_solid(&pv, &pf);
        match &m {
            None => {
                let (cv, cf, _) = cap_holes(&pv, &pf);
                m = to_solid(&cv, &cf);
                if m.as_ref().is_some_and(|s| s.signed_volume() < 0.0) {
                    m = to_solid(&cv, &reversed(&cf));
                }
            }
            Some(s) if s.signed_volume() < 0.0 => m = to_solid(&pv, &reversed(&pf)),
            Some(_) => {}
        }
        let Some(m) = m else {
            if is_flat(&pv) {
                continue;
            }
            return None;
        };
        if m.signed_volume().abs() <= 1e-6 * size {
            continue;
        }
        solids.push(m);
    }
    match solids.len() {
        0 => None,
        1 => solids.pop(),
        _ => Some(Solid::union_all(&solids)),
    }
}

/// `geometry.merge_overlapping_shells`. Many STLs are several closed shells pushed into each
/// other. Union them into one clean solid (shells that are cavities, i.e. inside-out, are left
/// alone). Returns the solid and how many shells were merged (0 when nothing changed).
pub fn merge_overlapping_shells(s: &Solid) -> (Solid, usize) {
    let parts = s.decompose_in_kernel_order();
    if parts.len() < 2 || parts.iter().any(|p| p.signed_volume() <= 0.0) {
        return (s.clone(), 0);
    }
    let total: f64 = parts.iter().map(|p| p.signed_volume()).sum();
    let merged = Solid::union_all(&parts);
    if merged.signed_volume() < total * (1.0 - 1e-6) {
        return (merged, parts.len());
    }
    (s.clone(), 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_and_not_flat() {
        let square = [0.0, 0.0, 5.0, 10.0, 0.0, 5.0, 10.0, 10.0, 5.0, 0.0, 10.0, 5.0];
        assert!(is_flat(&square));
        let mut tent = square.to_vec();
        tent.extend_from_slice(&[5.0, 5.0, 6.0]);
        assert!(!is_flat(&tent));
        // a tilted plane is still flat
        let tilted = [0.0, 0.0, 0.0, 10.0, 0.0, 10.0, 10.0, 10.0, 20.0, 0.0, 10.0, 10.0];
        assert!(is_flat(&tilted));
    }

    #[test]
    fn overlapping_cubes_merge_and_apart_cubes_do_not() {
        let a = Solid::cube([10.0, 10.0, 10.0], false);
        let both = Solid::compose(&[a.clone(), a.translate([5.0, 5.0, 5.0])]);
        let (merged, n) = merge_overlapping_shells(&both);
        assert_eq!(n, 2);
        assert!((merged.volume() - 1875.0).abs() < 1e-6);
        let apart = Solid::compose(&[a.clone(), a.translate([20.0, 0.0, 0.0])]);
        assert_eq!(merge_overlapping_shells(&apart).1, 0);
        assert_eq!(merge_overlapping_shells(&a).1, 0);
    }
}
