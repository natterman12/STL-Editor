//! trimesh `repair.fill_holes` (without fans): close the holes that are one triangle or one
//! quad in size.

use std::collections::HashSet;

use super::{Graph, edge, edge_groups, is_watertight, tri_cross};

/// trimesh `tol.zero` as `util.unitize` uses it: a new face with a smaller cross product has
/// no area and is not added.
const TOL_ZERO: f64 = 1e-13;

/// networkx 3.7 `cycle_basis(G)`: the cycles, each a list of nodes.
fn cycle_basis(g: &Graph) -> Vec<Vec<u32>> {
    let n = g.nodes.len();
    let mut cycles: Vec<Vec<u32>> = Vec::new();
    // gnodes: the nodes not yet in a walked component; popitem() takes the last one
    let mut alive = vec![true; n];
    let mut last = n;
    // per node slot: its predecessor, and the nodes it was reached or closed a cycle from
    let mut pred: Vec<Option<u32>> = vec![None; n];
    let mut used: Vec<Vec<u32>> = vec![Vec::new(); n];
    loop {
        while last > 0 && !alive[last - 1] {
            last -= 1;
        }
        if last == 0 {
            break;
        }
        let root = g.nodes[last - 1];
        let mut stack = vec![root];
        pred[g.slot(root)] = Some(root);
        alive[g.slot(root)] = false;
        while let Some(z) = stack.pop() {
            let zs = g.slot(z);
            for &nbr in g.neighbors(z) {
                let ns = g.slot(nbr);
                if pred[ns].is_none() {
                    // new node
                    pred[ns] = Some(z);
                    alive[ns] = false;
                    stack.push(nbr);
                    used[ns] = vec![z];
                } else if nbr == z {
                    // self loop
                    cycles.push(vec![z]);
                } else if !used[zs].contains(&nbr) {
                    // found a cycle
                    let mut cycle = vec![nbr, z];
                    let mut p = pred[zs].unwrap_or(z);
                    while !used[ns].contains(&p) {
                        cycle.push(p);
                        p = pred[g.slot(p)].unwrap_or(p);
                    }
                    cycle.push(p);
                    cycles.push(cycle);
                    used[ns].push(z);
                }
            }
        }
    }
    cycles
}

/// trimesh `repair.fill_holes(mesh)`, in place. Only faces are added; `v` is taken mutably to
/// match the trimesh call, which works on the whole mesh.
pub fn fill_holes(v: &mut Vec<f64>, f: &mut Vec<u32>) {
    if f.len() / 3 < 3 || is_watertight(f).0 {
        return;
    }
    // any edge that occurs once is on the boundary
    let boundary: Vec<(u32, u32)> = edge_groups(f, 1).into_iter().map(|k| edge(f, k)).collect();
    if boundary.len() < 3 {
        return;
    }
    let holes = cycle_basis(&Graph::from_edges(boundary.iter().copied()));

    // triangulate_quads(holes, use_fan=False): the triangles, then the first half of every
    // quad, then the second halves; bigger holes are skipped
    let mut new_faces: Vec<[u32; 3]> = Vec::new();
    for h in holes.iter().filter(|h| h.len() == 3) {
        new_faces.push([h[0], h[1], h[2]]);
    }
    for h in holes.iter().filter(|h| h.len() == 4) {
        new_faces.push([h[0], h[1], h[2]]);
    }
    for h in holes.iter().filter(|h| h.len() == 4) {
        new_faces.push([h[2], h[3], h[0]]);
    }
    if new_faces.is_empty() {
        return;
    }

    // a new face that runs along a boundary edge in the same direction is the wrong way round
    let old: HashSet<(u32, u32)> = boundary.into_iter().collect();
    for t in new_faces.iter_mut() {
        if (0..3).any(|k| old.contains(&(t[k], t[(k + 1) % 3]))) {
            t.swap(0, 2);
        }
    }

    // extend_faces: faces without area are left out
    for t in new_faces {
        let c = tri_cross(v, &t);
        if (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt() > TOL_ZERO {
            f.extend_from_slice(&t);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closes_a_missing_triangle() {
        let mut v = vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let mut f = vec![0, 2, 1, 0, 1, 3, 1, 2, 3];
        fill_holes(&mut v, &mut f);
        assert_eq!(f.len(), 12);
        assert_eq!(is_watertight(&f), (true, true));
    }

    #[test]
    fn fewer_than_three_faces_is_left_alone() {
        let mut v = vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let mut f = vec![0, 1, 2];
        fill_holes(&mut v, &mut f);
        assert_eq!(f, vec![0, 1, 2]);
    }
}
