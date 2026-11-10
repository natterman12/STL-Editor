//! trimesh `repair.fix_winding`: walk the faces breadth-first and reverse any face whose shared
//! edge runs the same way as its neighbour's.
//!
//! Which face a walk starts from decides which way a whole shell ends up facing, so the start
//! is chosen exactly as trimesh 5.2.0 on networkx 3.7 chooses it. That includes the case where
//! networkx iterates a Python `set` of face numbers, whose order [`PySet`] reproduces.

use std::collections::VecDeque;

use super::{Graph, edge, edge_key, face_adjacency, is_watertight};

/// The iteration order of a CPython `set` of small non-negative ints that were added one at a
/// time (`setobject.c`: open addressing, 9 linear probes, then the perturbed jump).
pub(crate) struct PySet {
    table: Vec<Option<u32>>,
    fill: usize,
}

const LINEAR_PROBES: usize = 9;
const PERTURB_SHIFT: u32 = 5;

impl PySet {
    pub fn new() -> PySet {
        PySet { table: vec![None; 8], fill: 0 }
    }

    /// The slot `key` goes in: the first free one on its probe sequence, or the one it is
    /// already in.
    fn find(table: &[Option<u32>], key: u32) -> usize {
        let mask = table.len() - 1;
        let mut perturb = key as usize;
        let mut i = key as usize & mask;
        loop {
            let probes = if i + LINEAR_PROBES <= mask { LINEAR_PROBES } else { 0 };
            for j in 0..=probes {
                match table[i + j] {
                    None => return i + j,
                    Some(k) if k == key => return i + j,
                    _ => {}
                }
            }
            perturb >>= PERTURB_SHIFT;
            i = (i * 5 + 1 + perturb) & mask;
        }
    }

    pub fn add(&mut self, key: u32) {
        let slot = PySet::find(&self.table, key);
        if self.table[slot].is_some() {
            return;
        }
        self.table[slot] = Some(key);
        self.fill += 1;
        let mask = self.table.len() - 1;
        if self.fill * 5 < mask * 3 {
            return;
        }
        // set_table_resize: the entries go into the bigger table in the old table's order
        let minused = if self.fill > 50000 { self.fill * 2 } else { self.fill * 4 };
        let mut size = 8;
        while size <= minused {
            size <<= 1;
        }
        let mut table = vec![None; size];
        for k in self.table.iter().flatten() {
            let slot = PySet::find(&table, *k);
            table[slot] = Some(*k);
        }
        self.table = table;
    }

    pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.table.iter().flatten().copied()
    }
}

/// True when the faces `a` and `b` use their first shared edge in the same direction, which
/// means one of them has to be reversed. `None` when they share no edge exactly once each.
fn same_direction(f: &[u32], a: u32, b: u32) -> Option<bool> {
    // the six directed edges of the pair, as trimesh `faces_to_edges(faces[[a, b]])`
    let pair: Vec<(u32, u32)> =
        [a, b].iter().flat_map(|&t| (0..3).map(move |k| edge(f, 3 * t as usize + k))).collect();
    // group_rows(sorted, require_count=2)[0]: the first key, in key order, that occurs twice
    let mut order: Vec<usize> = (0..6).collect();
    order.sort_by_key(|&k| (edge_key(pair[k]), k));
    let mut i = 0;
    while i < 6 {
        let key = edge_key(pair[order[i]]);
        let mut j = i + 1;
        while j < 6 && edge_key(pair[order[j]]) == key {
            j += 1;
        }
        if j - i == 2 {
            return Some(pair[order[i]].0 == pair[order[i + 1]].0);
        }
        i = j;
    }
    None
}

/// trimesh `repair.fix_winding`, in place. `_v` is not needed (the faces alone decide) but
/// keeps the signature in line with the other steps.
pub fn fix_winding(_v: &[f64], f: &mut Vec<u32>) {
    // anything we would fix is already done
    if f.len() >= 3 && is_watertight(f).1 {
        return;
    }
    let graph = Graph::from_edges(face_adjacency(f));
    let total = graph.nodes.len();
    let mut seen = vec![false; total];
    let mut visited = vec![false; f.len() / 3];
    for s in 0..total {
        if seen[s] {
            continue;
        }
        // nx.connected_components: the component of the first node not yet seen, its members
        // in the order a breadth-first search adds them to a Python set
        let source = graph.nodes[s];
        seen[s] = true;
        let mut members = vec![source];
        let mut head = 0;
        while head < members.len() {
            let n = members[head];
            head += 1;
            for &w in graph.neighbors(n) {
                let ws = graph.slot(w);
                if !seen[ws] {
                    seen[ws] = true;
                    members.push(w);
                }
            }
        }
        // graph.subgraph(component): its nodes iterate in graph order (so the source is first)
        // unless the component is under half the graph, when networkx iterates its own set of
        // the nodes, built by iterating the component set
        let start = if 2 * members.len() < total {
            let mut component = PySet::new();
            for &m in &members {
                component.add(m);
            }
            let mut nodes = PySet::new();
            for m in component.iter() {
                nodes.add(m);
            }
            nodes.iter().next().unwrap_or(source)
        } else {
            source
        };

        // nx.bfs_edges from the start: each (parent, child) tree edge in turn
        visited[start as usize] = true;
        let mut queue = VecDeque::from([start]);
        while let Some(parent) = queue.pop_front() {
            for &child in graph.neighbors(parent) {
                if visited[child as usize] {
                    continue;
                }
                visited[child as usize] = true;
                queue.push_back(child);
                if same_direction(f, parent, child) == Some(true) {
                    let t = 3 * child as usize;
                    f.swap(t, t + 2);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(keys: &[u32]) -> Vec<u32> {
        let mut s = PySet::new();
        for &k in keys {
            s.add(k);
        }
        s.iter().collect()
    }

    // the expected orders are printed by CPython 3.14: list(s) after s.add(k) for each k
    #[test]
    fn pyset_order_matches_cpython() {
        assert_eq!(
            order(&[5, 13, 21, 29, 37, 45, 3, 100, 64, 8, 16]),
            vec![64, 3, 100, 37, 5, 8, 13, 45, 16, 21, 29]
        );
        assert_eq!(
            order(&[64, 3, 100, 37, 5, 8, 13, 45, 16, 21, 29]),
            vec![64, 3, 100, 5, 37, 8, 13, 45, 16, 21, 29]
        );
        let sevens: Vec<u32> = (0..300).step_by(7).collect();
        assert_eq!(
            order(&sevens),
            vec![
                0, 259, 133, 7, 266, 140, 14, 273, 147, 21, 280, 154, 28, 287, 161, 35, 294, 168, 42, 175, 49,
                182, 56, 189, 63, 196, 70, 203, 77, 210, 84, 217, 91, 224, 98, 231, 105, 238, 112, 245, 119,
                252, 126
            ]
        );
    }

    #[test]
    fn flips_the_odd_face_of_a_tetrahedron() {
        let v = [0.0; 12];
        let mut f = vec![0, 2, 1, 0, 1, 3, 1, 2, 3, 3, 0, 2];
        fix_winding(&v, &mut f);
        assert_eq!(is_watertight(&f), (true, true));
    }

    #[test]
    fn empty_and_single_triangle_are_left_alone() {
        let mut f: Vec<u32> = Vec::new();
        fix_winding(&[], &mut f);
        assert!(f.is_empty());
        let mut f = vec![0, 1, 2];
        fix_winding(&[0.0; 9], &mut f);
        assert_eq!(f, vec![0, 1, 2]);
    }
}
