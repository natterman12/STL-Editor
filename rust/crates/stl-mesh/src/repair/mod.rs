//! Repair, ported step by step from desktop 1.x (`stleditor/geometry.py`) and the trimesh
//! 5.2.0 / networkx 3.7 functions it calls. One file per step; each is checked against
//! `rust/tests/parity/repair_golden.json`.
//!
//! Meshes are flat arrays: `v` is x, y, z per vertex (f64, as in 1.x) and `f` is three vertex
//! indices per triangle. Where the 1.x result depends on a numpy, scipy or networkx ordering
//! (sort order, which face a traversal starts from), this port follows the same ordering; the
//! helpers for that live here.

mod cap_holes;
mod fill_holes;
mod inversion;
mod local;
mod process;
mod quick;
mod shells;
mod winding;

pub use cap_holes::cap_holes;
pub use fill_holes::fill_holes;
pub use inversion::fix_inversion;
pub use local::local_repair;
pub use process::process;
pub use quick::quick_repair;
pub use shells::{merge_overlapping_shells, touching_shells};
pub use winding::fix_winding;

use std::collections::HashMap;

use stl_kernel::{MeshArrays, Solid};

/// `geometry.manifold_from_arrays`: a solid, or `None` if the mesh is not a closed 2-manifold.
/// Positions go to the kernel as f32, as in 1.x.
pub(crate) fn to_solid(v: &[f64], f: &[u32]) -> Option<Solid> {
    if f.is_empty() {
        return None;
    }
    let m = MeshArrays { positions: v.iter().map(|&x| x as f32).collect(), triangles: f.to_vec() };
    Solid::from_mesh(&m).ok()
}

/// Every triangle with its vertex order reversed (`f[:, ::-1]`).
pub(crate) fn reversed(f: &[u32]) -> Vec<u32> {
    f.chunks_exact(3).flat_map(|t| [t[2], t[1], t[0]]).collect()
}

/// trimesh `faces_to_edges`: directed edge `k` of the mesh is edge `k % 3` of face `k / 3`,
/// the edges of a face being (0, 1), (1, 2), (2, 0).
pub(crate) fn edge(f: &[u32], k: usize) -> (u32, u32) {
    let t = k - k % 3;
    (f[t + k % 3], f[t + (k + 1) % 3])
}

/// The sort key of an undirected edge. trimesh packs a sorted row (lo, hi) into one integer as
/// `lo | hi << 32` and sorts those, so groups come out ordered by (hi, lo).
pub(crate) fn edge_key(e: (u32, u32)) -> (u32, u32) {
    (e.0.max(e.1), e.0.min(e.1))
}

/// trimesh `grouping.group_rows(edges_sorted, require_count=count)`: the directed-edge indices
/// of every undirected edge that occurs exactly `count` times, `count` indices per group, the
/// groups ordered by [`edge_key`]. Inside a group the lower index comes first (numpy's sort is
/// not stable there, but every caller is symmetric in the group's members).
pub(crate) fn edge_groups(f: &[u32], count: usize) -> Vec<usize> {
    let n = f.len() / 3 * 3;
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&k| (edge_key(edge(f, k)), k));
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        let key = edge_key(edge(f, order[i]));
        let mut j = i + 1;
        while j < n && edge_key(edge(f, order[j])) == key {
            j += 1;
        }
        if j - i == count {
            out.extend_from_slice(&order[i..j]);
        }
        i = j;
    }
    out
}

/// trimesh `graph.is_watertight`: (every edge is used by exactly two faces, every such pair of
/// uses runs in opposite directions).
pub(crate) fn is_watertight(f: &[u32]) -> (bool, bool) {
    let pairs = edge_groups(f, 2);
    let watertight = pairs.len() == f.len() / 3 * 3;
    let winding = pairs.chunks_exact(2).all(|p| edge(f, p[0]).1 == edge(f, p[1]).0);
    (watertight, winding)
}

/// trimesh `Trimesh.face_adjacency`: the pairs of faces that share an edge used by exactly
/// two faces, lower face first, in [`edge_groups`] order.
pub(crate) fn face_adjacency(f: &[u32]) -> Vec<(u32, u32)> {
    edge_groups(f, 2)
        .chunks_exact(2)
        .filter_map(|p| {
            let (a, b) = ((p[0] / 3) as u32, (p[1] / 3) as u32);
            (a != b).then_some((a.min(b), a.max(b)))
        })
        .collect()
}

/// numpy's pairwise summation (`pairwise_sum_DOUBLE`), which `ndarray.sum` uses along a
/// contiguous axis.
fn pairwise_sum(a: &[f64]) -> f64 {
    let n = a.len();
    if n < 8 {
        let mut res = 0.0;
        for &x in a {
            res += x;
        }
        res
    } else if n <= 128 {
        let mut r = [0.0f64; 8];
        r.copy_from_slice(&a[..8]);
        let mut i = 8;
        while i < n - n % 8 {
            for j in 0..8 {
                r[j] += a[i + j];
            }
            i += 8;
        }
        let mut res = ((r[0] + r[1]) + (r[2] + r[3])) + ((r[4] + r[5]) + (r[6] + r[7]));
        while i < n {
            res += a[i];
            i += 1;
        }
        res
    } else {
        let mut n2 = n / 2;
        n2 -= n2 % 8;
        pairwise_sum(&a[..n2]) + pairwise_sum(&a[n2..])
    }
}

/// `np.add.reduce` of a contiguous row: the first value, plus the pairwise sum of the rest.
pub(crate) fn numpy_sum(a: &[f64]) -> f64 {
    match a.split_first() {
        None => 0.0,
        Some((first, rest)) => first + pairwise_sum(rest),
    }
}

pub(crate) fn point(v: &[f64], i: u32) -> [f64; 3] {
    let k = i as usize * 3;
    [v[k], v[k + 1], v[k + 2]]
}

/// trimesh `triangles.cross`: (v1 - v0) x (v2 - v1).
pub(crate) fn tri_cross(v: &[f64], t: &[u32]) -> [f64; 3] {
    let (p0, p1, p2) = (point(v, t[0]), point(v, t[1]), point(v, t[2]));
    let a = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let b = [p2[0] - p1[0], p2[1] - p1[1], p2[2] - p1[2]];
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// The volume trimesh's `mass_properties` gives for the listed faces (all of them for `None`):
/// the sum of `cross.x * (x0 + x1 + x2)` over the triangles, divided by 6.
pub(crate) fn volume(v: &[f64], f: &[u32], faces: Option<&[u32]>) -> f64 {
    let term = |i: usize| {
        let t = &f[3 * i..3 * i + 3];
        let c = tri_cross(v, t);
        c[0] * (point(v, t[0])[0] + point(v, t[1])[0] + point(v, t[2])[0])
    };
    let terms: Vec<f64> = match faces {
        Some(idx) => idx.iter().map(|&i| term(i as usize)).collect(),
        None => (0..f.len() / 3).map(term).collect(),
    };
    numpy_sum(&terms) / 6.0
}

/// A networkx `Graph` as far as its orderings go: nodes in the order they were first seen,
/// and each node's neighbours in the order their edges were added.
pub(crate) struct Graph {
    pub nodes: Vec<u32>,
    slot: HashMap<u32, usize>,
    adj: Vec<Vec<u32>>,
}

impl Graph {
    /// `nx.from_edgelist`.
    pub fn from_edges(edges: impl IntoIterator<Item = (u32, u32)>) -> Graph {
        let mut g = Graph { nodes: Vec::new(), slot: HashMap::new(), adj: Vec::new() };
        for (u, w) in edges {
            let su = g.add_node(u);
            let sw = g.add_node(w);
            if !g.adj[su].contains(&w) {
                g.adj[su].push(w);
            }
            if !g.adj[sw].contains(&u) {
                g.adj[sw].push(u);
            }
        }
        g
    }

    fn add_node(&mut self, n: u32) -> usize {
        if let Some(&s) = self.slot.get(&n) {
            return s;
        }
        let s = self.nodes.len();
        self.nodes.push(n);
        self.slot.insert(n, s);
        self.adj.push(Vec::new());
        s
    }

    /// The node's position in `nodes`.
    pub fn slot(&self, n: u32) -> usize {
        self.slot[&n]
    }

    pub fn neighbors(&self, n: u32) -> &[u32] {
        &self.adj[self.slot[&n]]
    }
}

/// Groups of faces joined through [`face_adjacency`], each in face order, the groups ordered by
/// their lowest face (the label order of scipy's `connected_components`). With `all_faces`, a
/// face that has no neighbour is a group of its own; without it, such faces are left out.
pub(crate) fn face_components(f: &[u32], all_faces: bool) -> Vec<Vec<u32>> {
    let nf = f.len() / 3;
    let mut parent: Vec<u32> = (0..nf as u32).collect();
    fn find(parent: &mut [u32], mut x: u32) -> u32 {
        while parent[x as usize] != x {
            parent[x as usize] = parent[parent[x as usize] as usize];
            x = parent[x as usize];
        }
        x
    }
    let mut linked = vec![false; nf];
    for (a, b) in face_adjacency(f) {
        linked[a as usize] = true;
        linked[b as usize] = true;
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        if ra != rb {
            parent[ra.max(rb) as usize] = ra.min(rb);
        }
    }
    let mut label: HashMap<u32, usize> = HashMap::new();
    let mut out: Vec<Vec<u32>> = Vec::new();
    for i in 0..nf as u32 {
        if !all_faces && !linked[i as usize] {
            continue;
        }
        let r = find(&mut parent, i);
        let next = out.len();
        let l = *label.entry(r).or_insert(next);
        if l == next {
            out.push(Vec::new());
        }
        out[l].push(i);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numpy_sum_small_and_large() {
        assert_eq!(numpy_sum(&[]), 0.0);
        assert_eq!(numpy_sum(&[1.0, 2.0, 3.0]), 6.0);
        let a: Vec<f64> = (0..1000).map(|i| i as f64).collect();
        assert_eq!(numpy_sum(&a), 499500.0);
    }

    #[test]
    fn watertight_tetrahedron() {
        let f = [0, 2, 1, 0, 1, 3, 1, 2, 3, 2, 0, 3];
        assert_eq!(is_watertight(&f), (true, true));
        assert_eq!(is_watertight(&f[..9]), (false, true));
        let flipped = [0, 2, 1, 0, 1, 3, 1, 2, 3, 3, 0, 2];
        assert_eq!(is_watertight(&flipped), (true, false));
        assert_eq!(face_components(&f, true).len(), 1);
    }

    #[test]
    fn graph_keeps_insertion_order() {
        let g = Graph::from_edges([(5, 2), (2, 9), (5, 9), (2, 5)]);
        assert_eq!(g.nodes, vec![5, 2, 9]);
        assert_eq!(g.neighbors(2), &[5, 9]);
        assert_eq!(g.neighbors(5), &[2, 9]);
    }
}
