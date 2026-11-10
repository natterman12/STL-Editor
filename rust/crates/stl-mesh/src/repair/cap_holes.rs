//! `geometry._cap_holes` from 1.x: close every boundary loop with a fan around its centroid.

use std::collections::{HashMap, HashSet};

use super::point;

/// Returns the vertices (one more per closed loop), the faces (the fans added at the end) and
/// the number of loops closed.
pub fn cap_holes(v: &[f64], f: &[u32]) -> (Vec<f64>, Vec<u32>, usize) {
    let nf = f.len() / 3;
    // the directed edges in numpy's order: the (0, 1) edge of every face, then the (1, 2)
    // edges, then the (2, 0) edges
    let edges: Vec<(u32, u32)> =
        (0..3).flat_map(|k| (0..nf).map(move |t| (f[3 * t + k], f[3 * t + (k + 1) % 3]))).collect();
    let all: HashSet<(u32, u32)> = edges.iter().copied().collect();
    // a boundary edge has no edge running back along it
    let boundary: Vec<(u32, u32)> = edges.into_iter().filter(|&(a, b)| !all.contains(&(b, a))).collect();
    let mut out_v = v.to_vec();
    let mut out_f = f.to_vec();
    if boundary.is_empty() {
        return (out_v, out_f, 0);
    }
    let mut nxt: HashMap<u32, Vec<u32>> = HashMap::new();
    for &(a, b) in &boundary {
        nxt.entry(a).or_default().push(b);
    }
    let mut loops = 0;
    let mut used: HashSet<(u32, u32)> = HashSet::new();
    for &(a0, b0) in &boundary {
        if used.contains(&(a0, b0)) {
            continue;
        }
        let mut lp = vec![a0];
        let mut cur = b0;
        used.insert((a0, b0));
        let mut ok = true;
        while cur != a0 {
            lp.push(cur);
            let next = nxt.get(&cur).and_then(|outs| outs.iter().copied().find(|&x| !used.contains(&(cur, x))));
            let Some(nx) = next else {
                ok = false;
                break;
            };
            if lp.len() > 200000 {
                ok = false;
                break;
            }
            used.insert((cur, nx));
            cur = nx;
        }
        if !ok || lp.len() < 3 {
            continue;
        }
        // the centroid of the loop's vertices (the original ones: a loop never uses a centroid)
        let mut c = [0.0f64; 3];
        for &i in &lp {
            let p = point(v, i);
            for k in 0..3 {
                c[k] += p[k];
            }
        }
        let n = lp.len() as f64;
        let ci = (out_v.len() / 3) as u32;
        out_v.extend_from_slice(&[c[0] / n, c[1] / n, c[2] / n]);
        // boundary edge (u -> w) belongs to an existing face, so the cap uses (w -> u)
        for i in 0..lp.len() {
            out_f.extend_from_slice(&[lp[(i + 1) % lp.len()], lp[i], ci]);
        }
        loops += 1;
    }
    (out_v, out_f, loops)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repair::is_watertight;

    #[test]
    fn caps_an_open_tetrahedron() {
        let v = [0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 3.0];
        let f = [0, 2, 1, 0, 1, 3, 2, 0, 3];
        let (cv, cf, loops) = cap_holes(&v, &f);
        assert_eq!(loops, 1);
        assert_eq!(cf.len() / 3, 6);
        assert_eq!(&cv[12..], &[1.0, 1.0, 1.0]);
        assert_eq!(is_watertight(&cf), (true, true));
    }

    #[test]
    fn closed_mesh_is_unchanged() {
        let v = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let f = [0, 2, 1, 0, 1, 3, 1, 2, 3, 2, 0, 3];
        let (cv, cf, loops) = cap_holes(&v, &f);
        assert_eq!((cv.len(), cf.len(), loops), (12, 12, 0));
    }
}
