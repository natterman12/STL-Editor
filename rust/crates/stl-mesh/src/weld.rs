//! `geometry.weld` from 1.x: merge coincident vertices, drop degenerate triangles, collapse
//! duplicate faces and cancel opposite-facing pairs. Output order follows numpy exactly.

/// numpy `np.round(v / tol).astype(np.int64)`: ties go to even, non-finite or out-of-range
/// values become `i64::MIN` (what x86 numpy gives).
fn quantise(v: f64, tol: f64) -> i64 {
    let r = (v / tol).round_ties_even();
    if r.is_finite() && r.abs() < 9.223372036854775e18 { r as i64 } else { i64::MIN }
}

/// `positions` is flat xyz, `triangles` flat vertex indices. Returns the welded positions
/// (the original coordinates of the first vertex in each cell, cells in `np.unique` order)
/// and the surviving triangles. A triangle that names a vertex that is not there makes the
/// whole mesh unusable: the result is then empty, which every caller treats as "no solid".
pub fn weld(positions: &[f64], triangles: &[u32], tol: f64) -> (Vec<f64>, Vec<u32>) {
    let nv = positions.len() / 3;
    if triangles.iter().any(|&i| i as usize >= nv) {
        return (Vec::new(), Vec::new());
    }
    let q: Vec<[i64; 3]> = (0..nv)
        .map(|i| {
            [
                quantise(positions[3 * i], tol),
                quantise(positions[3 * i + 1], tol),
                quantise(positions[3 * i + 2], tol),
            ]
        })
        .collect();
    // np.unique(axis=0): lexicographic order, first occurrence represents the cell (stable sort)
    let mut order: Vec<usize> = (0..nv).collect();
    order.sort_by(|&a, &b| q[a].cmp(&q[b]));
    let mut inv = vec![0u32; nv];
    let mut out_pos: Vec<f64> = Vec::new();
    let mut cells = 0u32;
    let mut k = 0;
    while k < order.len() {
        let first = order[k];
        let mut j = k;
        while j < order.len() && q[order[j]] == q[first] {
            inv[order[j]] = cells;
            j += 1;
        }
        out_pos.extend_from_slice(&positions[3 * first..3 * first + 3]);
        cells += 1;
        k = j;
    }

    // remap and drop degenerate triangles
    let mut nf: Vec<[u32; 3]> = Vec::with_capacity(triangles.len() / 3);
    for t in triangles.chunks_exact(3) {
        let a = inv[t[0] as usize];
        let b = inv[t[1] as usize];
        let c = inv[t[2] as usize];
        if a != b && b != c && a != c {
            nf.push([a, b, c]);
        }
    }
    if nf.is_empty() {
        return (out_pos, Vec::new());
    }

    // group triangles by their sorted vertex triple (np.unique order, first occurrence)
    let keys: Vec<[u32; 3]> = nf
        .iter()
        .map(|t| {
            let mut s = *t;
            s.sort_unstable();
            s
        })
        .collect();
    let mut ord: Vec<usize> = (0..nf.len()).collect();
    ord.sort_by(|&a, &b| keys[a].cmp(&keys[b]));
    let mut group = vec![0usize; nf.len()];
    let mut counts: Vec<usize> = Vec::new();
    let mut k = 0;
    while k < ord.len() {
        let mut j = k;
        let g = counts.len();
        while j < ord.len() && keys[ord[j]] == keys[ord[k]] {
            group[ord[j]] = g;
            j += 1;
        }
        counts.push(j - k);
        k = j;
    }
    let keep: Vec<usize> = if counts.iter().any(|&c| c > 1) {
        // sign of each triangle: parity of its vertex order against sorted order
        let sign: Vec<i64> = nf
            .iter()
            .map(|t| {
                // argsort of the row (values are distinct)
                let mut r = [0usize, 1, 2];
                r.sort_by_key(|&i| t[i]);
                let odd = (r[0] > r[1]) as i32 + (r[0] > r[2]) as i32 + (r[1] > r[2]) as i32;
                if odd % 2 == 1 { -1 } else { 1 }
            })
            .collect();
        let mut sum = vec![0i64; counts.len()];
        for (i, &g) in group.iter().enumerate() {
            sum[g] += sign[i];
        }
        let mut taken = vec![false; counts.len()];
        let mut keep = Vec::new();
        for i in 0..nf.len() {
            let g = group[i];
            let win = sum[g].signum();
            if win != 0 && sign[i] == win && !taken[g] {
                taken[g] = true;
                keep.push(i);
            }
        }
        keep
    } else {
        (0..nf.len()).collect()
    };
    let mut tris = Vec::with_capacity(keep.len() * 3);
    for i in keep {
        tris.extend_from_slice(&nf[i]);
    }
    (out_pos, tris)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_and_orders_lexicographically() {
        let p = [
            1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0,
        ];
        let (v, f) = weld(&p, &[0, 1, 2, 3, 4, 5], 1e-5);
        assert_eq!(v.len(), 12);
        assert_eq!(&v[0..3], &[0.0, 0.0, 0.0]);
        assert_eq!(f.len(), 6);
    }

    #[test]
    fn index_out_of_range_gives_an_empty_mesh() {
        let p = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        assert_eq!(weld(&p, &[0, 1, 3], 1e-5), (vec![], vec![]));
        assert_eq!(weld(&p, &[0, 1, u32::MAX], 1e-5), (vec![], vec![]));
        assert_eq!(weld(&[], &[0, 1, 2], 1e-5), (vec![], vec![]));
        // two stray values after the last whole vertex are not a vertex
        assert_eq!(weld(&p[..8], &[0, 1, 2], 1e-5), (vec![], vec![]));
    }

    #[test]
    fn opposite_pair_cancels_and_duplicate_collapses() {
        let p = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let (_, f) = weld(&p, &[0, 1, 2, 2, 1, 0], 1e-5);
        assert!(f.is_empty());
        let (_, f) = weld(&p, &[0, 1, 2, 1, 2, 0], 1e-5);
        assert_eq!(f, vec![0, 2, 1]); // vertices are re-ordered lexicographically
    }
}
