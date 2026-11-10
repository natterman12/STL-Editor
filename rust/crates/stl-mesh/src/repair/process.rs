//! trimesh `Trimesh(vertices, faces, process=True, validate=True)`: drop duplicate and
//! degenerate faces, fix the winding and the facing, drop non-finite values, merge vertices.

use std::collections::{HashMap, HashSet};

use super::{fix_inversion, fix_winding, point, tri_cross};

/// trimesh `tol.merge`.
const TOL_MERGE: f64 = 1e-8;

/// trimesh `Trimesh.nondegenerate_faces()`: a face is kept when both sides of its 2D oriented
/// bounding box are longer than `tol.merge`.
pub(crate) fn nondegenerate_faces(v: &[f64], f: &[u32]) -> Vec<bool> {
    f.chunks_exact(3)
        .map(|t| {
            let c = tri_cross(v, t);
            let area = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt() / 2.0;
            let (p0, p1, p2) = (point(v, t[0]), point(v, t[1]), point(v, t[2]));
            let len = |p: [f64; 3]| {
                let d = [p[0] - p0[0], p[1] - p0[1], p[2] - p0[2]];
                (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
            };
            let side = |l: f64| if l > TOL_MERGE { (area * 2.0) / l } else { 0.0 };
            side(len(p1)) > TOL_MERGE && side(len(p2)) > TOL_MERGE
        })
        .collect()
}

/// trimesh `Trimesh.unique_faces()`: true for the first face on each set of three vertices.
pub(crate) fn unique_faces(f: &[u32]) -> Vec<bool> {
    let mut seen: HashSet<[u32; 3]> = HashSet::new();
    f.chunks_exact(3)
        .map(|t| {
            let mut k = [t[0], t[1], t[2]];
            k.sort_unstable();
            seen.insert(k)
        })
        .collect()
}

/// trimesh `Trimesh.update_faces(mask)`.
pub(crate) fn keep_faces(f: &[u32], mask: &[bool]) -> Vec<u32> {
    f.chunks_exact(3).zip(mask).filter(|(_, keep)| **keep).flat_map(|(t, _)| t.iter().copied()).collect()
}

/// trimesh `Trimesh.body_count`: connected groups of vertices (a vertex no face uses is a
/// group of its own).
fn body_count(nv: usize, f: &[u32]) -> usize {
    let mut parent: Vec<u32> = (0..nv as u32).collect();
    fn find(parent: &mut [u32], mut x: u32) -> u32 {
        while parent[x as usize] != x {
            parent[x as usize] = parent[parent[x as usize] as usize];
            x = parent[x as usize];
        }
        x
    }
    let mut count = nv;
    for t in f.chunks_exact(3) {
        for (a, b) in [(t[0], t[1]), (t[1], t[2])] {
            let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
            if ra != rb {
                parent[ra as usize] = rb;
                count -= 1;
            }
        }
    }
    count
}

/// `Trimesh(process=True, validate=True)` on an indexed mesh: the vertices and faces the
/// trimesh object ends up with. A face that names a vertex that is not there gives an empty
/// mesh (see `weld`).
pub fn process(v: &[f64], f: &[u32]) -> (Vec<f64>, Vec<u32>) {
    let nv = v.len() / 3;
    if f.iter().any(|&i| i as usize >= nv) {
        return (Vec::new(), Vec::new());
    }
    // an empty mesh is left as it is
    if nv == 0 || f.len() < 3 {
        return (v.to_vec(), f.to_vec());
    }

    // validate: only unique, non-degenerate faces, then fix_normals()
    let unique = unique_faces(f);
    let nondegenerate = nondegenerate_faces(v, f);
    let mask: Vec<bool> = unique.iter().zip(&nondegenerate).map(|(a, b)| *a && *b).collect();
    let mut f = keep_faces(f, &mask);
    if f.is_empty() {
        // trimesh treats the mesh as empty from here on and changes nothing else
        return (v.to_vec(), f);
    }
    let multibody = body_count(nv, &f) > 1;
    fix_winding(v, &mut f);
    fix_inversion(v, &mut f, multibody);

    // remove_infinite_values: faces on a vertex that is not finite go, then those vertices
    let finite: Vec<bool> = (0..nv).map(|i| v[3 * i..3 * i + 3].iter().all(|x| x.is_finite())).collect();
    let mut v = v.to_vec();
    if finite.iter().any(|ok| !ok) {
        let keep: Vec<bool> = f.chunks_exact(3).map(|t| t.iter().all(|&i| finite[i as usize])).collect();
        f = keep_faces(&f, &keep);
        if f.is_empty() {
            return (v, f);
        }
        let mut inverse = vec![0u32; nv];
        let mut nv2 = Vec::new();
        for i in 0..nv {
            if finite[i] {
                inverse[i] = (nv2.len() / 3) as u32;
                nv2.extend_from_slice(&v[3 * i..3 * i + 3]);
            }
        }
        for i in f.iter_mut() {
            *i = inverse[*i as usize];
        }
        v = nv2;
    }

    // merge_vertices: vertices that faces use, equal when rounded to 8 decimals, become one;
    // they keep the order of their first use by index, and unused vertices go
    let nv = v.len() / 3;
    let mut referenced = vec![false; nv];
    for &i in &f {
        referenced[i as usize] = true;
    }
    let quantise = |x: f64| {
        let r = (x * 1e8).round_ties_even();
        if r.abs() < 9.223372036854775e18 { r as i64 } else { i64::MIN }
    };
    let mut first: HashMap<[i64; 3], u32> = HashMap::new();
    let mut inverse = vec![0u32; nv];
    let mut out_v = Vec::new();
    for i in 0..nv {
        if !referenced[i] {
            continue;
        }
        let key = [quantise(v[3 * i]), quantise(v[3 * i + 1]), quantise(v[3 * i + 2])];
        let next = (out_v.len() / 3) as u32;
        let id = *first.entry(key).or_insert(next);
        if id == next {
            out_v.extend_from_slice(&v[3 * i..3 * i + 3]);
        }
        inverse[i] = id;
    }
    for i in f.iter_mut() {
        *i = inverse[*i as usize];
    }
    (out_v, f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_duplicate_and_degenerate_faces_and_unused_vertices() {
        // a unit square of two triangles, one duplicated, one collinear face, one unused vertex
        let v = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0, 2.0, 0.0, 0.0, 9.0, 9.0, 9.0];
        let f = [0, 1, 2, 0, 2, 3, 2, 1, 0, 0, 1, 4];
        let (pv, pf) = process(&v, &f);
        assert_eq!(pf, vec![0, 1, 2, 0, 2, 3]);
        assert_eq!(pv.len(), 12);
    }

    #[test]
    fn index_out_of_range_gives_an_empty_mesh() {
        let v = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        assert_eq!(process(&v, &[0, 1, 3]), (vec![], vec![]));
        assert_eq!(process(&[], &[0, 1, 2]), (vec![], vec![]));
    }

    #[test]
    fn empty_stays_empty() {
        let (pv, pf) = process(&[], &[]);
        assert!(pv.is_empty() && pf.is_empty());
    }
}
