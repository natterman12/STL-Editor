// Copyright 2026 Lars Brubaker
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// Collider — BVH-based broadphase collision detection.
// Uses a radix tree built from Morton codes for O(n log n) queries,
// matching the C++ Manifold implementation.

use crate::impl_mesh::ManifoldImpl;
use crate::linalg::{cross, distance2, dot, Mat3x4, Vec3};
use crate::sort::{get_face_box_morton, morton_code};
use crate::types::Box as BBox;

// Node encoding (matches C++):
// - Even indices are leaf nodes: leaf i -> node 2*i
// - Odd indices are internal nodes: internal i -> node 2*i + 1
// - Root is always node index 1 (internal node 0)
const K_ROOT: i32 = 1;

/// Leaf count from which the tree is built in parallel (C++ `kSeqThreshold`).
/// Both parallel passes give the sequential build's tree and boxes: see
/// `create_radix_tree` and `build_internal_boxes_par`.
const PAR_THRESHOLD: usize = 10_000;

/// Most leaves in a subtree that one task of the parallel box refit handles.
const REFIT_GRAIN: i32 = 4096;

fn leaf_to_node(leaf: i32) -> i32 {
    leaf * 2
}
fn node_to_leaf(node: i32) -> i32 {
    node / 2
}
fn internal_to_node(internal: i32) -> i32 {
    internal * 2 + 1
}
fn node_to_internal(node: i32) -> i32 {
    node / 2
}
fn is_leaf(node: i32) -> bool {
    node % 2 == 0
}
fn is_internal(node: i32) -> bool {
    node % 2 == 1
}

/// Face BVH matching the C++ Collider's storage layout exactly: one interleaved
/// node array (leaves at even indices, internals at odd), the radix-tree
/// topology, and parent links for bottom-up box refits. Leaf boxes live inside
/// `node_bbox` — no separate leaf copy, and the morton codes are consumed at
/// construction rather than stored, which matters because a collider is cached
/// on every ManifoldImpl.
#[derive(Debug, Default)]
pub struct Collider {
    node_bbox: Vec<BBox>,             // AABBs for all nodes (2*num_leaves - 1)
    node_parent: Vec<i32>,            // parent node per node (-1 at root)
    internal_children: Vec<[i32; 2]>, // child pairs for internal nodes (num_leaves - 1)
    /// The links were checked at construction to form a proper radix tree
    /// (see `create_radix_tree`), which the parallel box refit relies on.
    /// Sorted morton codes always give one; `false` keeps the sequential refit.
    proper: bool,
}

// The node arrays are recycled like a mesh's (see `crate::scratch`).
impl Clone for Collider {
    fn clone(&self) -> Self {
        use crate::scratch::copy_of;
        Collider {
            node_bbox: copy_of(&self.node_bbox),
            node_parent: copy_of(&self.node_parent),
            internal_children: copy_of(&self.internal_children),
            proper: self.proper,
        }
    }
}

impl Drop for Collider {
    fn drop(&mut self) {
        use crate::scratch::give_from;
        give_from(&mut self.node_bbox);
        give_from(&mut self.node_parent);
        give_from(&mut self.internal_children);
    }
}

impl Collider {
    /// Create a new Collider from leaf bounding boxes and morton codes.
    /// Like the C++ constructor, leaves must already be sorted by morton code
    /// — every production caller builds them from a morton-sorted source
    /// (sort_geometry sorts faces, merge sorts open verts).
    pub fn new(leaf_bbox: Vec<BBox>, leaf_morton: Vec<u32>) -> Self {
        debug_assert_eq!(leaf_bbox.len(), leaf_morton.len());
        Self::from_leaf_fn(&leaf_morton, |leaf| leaf_bbox[leaf])
    }

    /// [`Collider::new`] with the box of each leaf given by a function of its
    /// index. The boxes are written straight into the tree, in parallel for a
    /// large one, instead of into a list that is then copied.
    pub fn from_leaf_fn<F>(leaf_morton: &[u32], leaf_box: F) -> Self
    where
        F: Fn(usize) -> BBox + Sync + Send,
    {
        debug_assert!(
            leaf_morton.windows(2).all(|w| w[0] <= w[1]),
            "Collider leaves must be pre-sorted by morton code"
        );
        let n = leaf_morton.len();
        if n == 0 {
            return Self::default();
        }
        let num_nodes = 2 * n - 1;
        let node_bbox = crate::par::maybe_par_map(num_nodes, PAR_THRESHOLD, |node| {
            if node % 2 == 0 {
                leaf_box(node / 2)
            } else {
                BBox::default()
            }
        });
        let mut collider = Self {
            node_bbox,
            node_parent: crate::scratch::filled(num_nodes, -1),
            internal_children: Vec::new(),
            proper: false,
        };
        collider.create_radix_tree(leaf_morton);
        collider.build_internal_boxes();
        collider
    }

    pub(crate) fn num_leaves(&self) -> usize {
        if self.node_bbox.is_empty() {
            0
        } else {
            self.internal_children.len() + 1
        }
    }

    /// Build radix tree from sorted Morton codes (matches C++ CreateRadixTree).
    /// Only fills in topology (internal_children, node_parent); boxes are
    /// populated afterwards by update_boxes.
    fn create_radix_tree(&mut self, leaf_morton: &[u32]) {
        let num_leaves = leaf_morton.len();
        if num_leaves <= 1 {
            return;
        }
        let num_internal = num_leaves - 1;

        const K_INITIAL_LENGTH: i32 = 128;
        const K_LENGTH_MULTIPLE: i32 = 4;

        // Helper: count leading zeros of XOR of two morton codes
        let prefix_length = |i: i32, j: i32| -> i32 {
            if j < 0 || j >= num_leaves as i32 {
                return -1;
            }
            let mi = leaf_morton[i as usize];
            let mj = leaf_morton[j as usize];
            let xor = mi ^ mj;
            if xor == 0 {
                // Same morton code, use index as tiebreaker (matches C++ clz)
                32 + ((i as u32 ^ j as u32).leading_zeros() as i32)
            } else {
                xor.leading_zeros() as i32
            }
        };

        // RangeEnd: find the other end of the range for internal node i
        let range_end = |i: i32| -> i32 {
            let dir_val = prefix_length(i, i + 1) - prefix_length(i, i - 1);
            // A tie counts as -1, where the C++ sign would give 0.
            let dir = if dir_val > 0 { 1i32 } else { -1i32 };
            let common_prefix = prefix_length(i, i - dir);
            let mut max_length = K_INITIAL_LENGTH;
            while prefix_length(i, i + dir * max_length) > common_prefix {
                max_length *= K_LENGTH_MULTIPLE;
            }
            let mut length = 0i32;
            let mut step = max_length / 2;
            while step > 0 {
                if prefix_length(i, i + dir * (length + step)) > common_prefix {
                    length += step;
                }
                step /= 2;
            }
            i + dir * length
        };

        // FindSplit: find where the split occurs within [first, last]
        let find_split = |first: i32, last: i32| -> i32 {
            let common_prefix = prefix_length(first, last);
            let mut split = first;
            let mut step = last - first;
            loop {
                step = (step + 1) >> 1; // divide by 2, rounding up
                let new_split = split + step;
                if new_split < last {
                    let split_prefix = prefix_length(first, new_split);
                    if split_prefix > common_prefix {
                        split = new_split;
                    }
                }
                if step <= 1 {
                    break;
                }
            }
            split
        };

        // For each internal node, find its range and split point. A node's
        // children depend only on the morton codes, so the nodes are worked
        // out in parallel; the parent links are then written in node order.
        // `ranges[i]` is the first and last leaf under internal node `i`.
        let (children, ranges): (Vec<[i32; 2]>, Vec<[i32; 2]>) =
            crate::par::maybe_par_map_unzip(num_internal, PAR_THRESHOLD, |internal| {
            let i = internal as i32;
            let mut first = i;
            let mut last = range_end(i);
            if first > last {
                std::mem::swap(&mut first, &mut last);
            }

            let split = find_split(first, last);

            // Assign children (matches C++ exactly)
            let child1 = if split == first {
                leaf_to_node(split)
            } else {
                internal_to_node(split)
            };
            // C++ increments split before computing child2
            let split2 = split + 1;
            let child2 = if split2 == last {
                leaf_to_node(split2)
            } else if (split2 as usize) < num_internal {
                internal_to_node(split2)
            } else {
                // Degenerate case: split2 exceeds internal node range.
                // This mirrors C++ UB when dir=0 in RangeEnd (clz(0) is UB).
                // Make child2 = child1 so traversal still works.
                child1
            };

            ([child1, child2], [first, last])
        });
        let ranges = crate::scratch::Scratch::from_vec(ranges);
        for (internal, &[child1, child2]) in children.iter().enumerate() {
            let node = internal_to_node(internal as i32);
            self.node_parent[child1 as usize] = node;
            self.node_parent[child2 as usize] = node;
        }

        // Is this a proper radix tree? Each node sits at one end of its leaf
        // range, its children split that range in two at `split`, and each
        // child, if internal, records exactly its half and names this node as
        // its parent. With the root over all the leaves that makes every node
        // the child of one parent, in a tree whose ranges nest: what
        // `build_internal_boxes_par` relies on.
        let node_parent = &self.node_parent;
        let last_leaf = num_leaves as i32 - 1;
        let broken = crate::par::maybe_par_filter(num_internal, PAR_THRESHOLD, |internal| {
            let i = internal as i32;
            let [first, last] = ranges[internal];
            let [child1, child2] = children[internal];
            let split = child1 / 2;
            let ok = first >= 0
                && first < last
                && last <= last_leaf
                && (i == first || i == last)
                && child1 >= 0
                && child2 >= 0
                && child1 != child2
                && split >= first
                && split < last
                && child2 / 2 == split + 1
                && (if is_leaf(child1) {
                    split == first
                } else {
                    ranges[split as usize] == [first, split]
                })
                && (if is_leaf(child2) {
                    split + 1 == last
                } else {
                    ((split + 1) as usize) < num_internal
                        && ranges[(split + 1) as usize] == [split + 1, last]
                })
                && node_parent[child1 as usize] == internal_to_node(i)
                && node_parent[child2 as usize] == internal_to_node(i);
            !ok
        });
        self.proper = broken.is_empty()
            && ranges[0] == [0, last_leaf]
            && self.node_parent[K_ROOT as usize] == -1;
        crate::scratch::give(std::mem::replace(&mut self.internal_children, children));
    }

    /// Refit internal boxes bottom-up from the leaf boxes already present in
    /// node_bbox, using the parent links (matches C++ BuildInternalBoxes).
    fn build_internal_boxes(&mut self) {
        let num_leaves = self.num_leaves();
        let num_internal = self.internal_children.len();
        if num_internal == 0 {
            return;
        }
        if self.proper && num_leaves >= PAR_THRESHOLD && cfg!(feature = "parallel") {
            self.build_internal_boxes_par();
            return;
        }
        let mut counter = vec![0u32; num_internal];

        for leaf in 0..num_leaves {
            let mut node = leaf_to_node(leaf as i32);
            loop {
                let parent = self.node_parent[node as usize];
                if parent < 0 {
                    break; // at root
                }
                let internal = node_to_internal(parent);
                if internal < 0 || internal >= num_internal as i32 {
                    break;
                }
                let idx = internal as usize;
                counter[idx] += 1;
                if counter[idx] < 2 {
                    break; // wait for second child
                }
                // Both children ready, compute union
                let [c1, c2] = self.internal_children[idx];
                let b1 = self.node_bbox[c1 as usize];
                let b2 = self.node_bbox[c2 as usize];
                self.node_bbox[parent as usize] = b1.union_box(&b2);
                node = parent;
            }
        }
    }

    /// The refit of `build_internal_boxes` for a proper radix tree
    /// (`self.proper`), one task per subtree.
    ///
    /// In a radix tree the node over leaves `first..=last` sits at index
    /// `first` or `last`, its left child (index `split`) is over
    /// `first..=split` and its right child (index `split + 1`) over
    /// `split + 1..=last`. So a subtree whose root is at `first` holds the
    /// internal nodes `first..last`, and one whose root is at `last` holds
    /// `first + 1..=last`. With leaf `i` stored at `2i` and internal node `i`
    /// at `2i + 1`, the boxes of a subtree lie in one run of `node_bbox`
    /// (`2 first..=2 last`, or one more for a root at `last`), and the runs of
    /// two subtrees over different leaves do not overlap.
    ///
    /// The tree is cut, top down, into subtrees of at most `REFIT_GRAIN`
    /// leaves. Each is refitted in its own run by the sequential algorithm on
    /// its own leaves; the nodes above the subtrees are then refitted children
    /// first. Every internal box is `box(child1).union_box(box(child2))` of
    /// its two finished children, the one union the sequential refit makes for
    /// it, so the boxes are the same bit for bit.
    fn build_internal_boxes_par(&mut self) {
        let num_leaves = self.num_leaves() as i32;

        // Cut the tree: `upper` nodes span more than REFIT_GRAIN leaves and are
        // listed parents first; `subtrees` are the roots below them, as
        // (root node, first leaf, last leaf).
        let mut upper: Vec<i32> = Vec::new();
        let mut subtrees: Vec<(i32, i32, i32)> = Vec::new();
        let mut stack: Vec<(i32, i32, i32)> = vec![(K_ROOT, 0, num_leaves - 1)];
        while let Some((node, first, last)) = stack.pop() {
            if is_leaf(node) {
                continue;
            }
            if last - first < REFIT_GRAIN {
                subtrees.push((node, first, last));
                continue;
            }
            let [child1, child2] = self.internal_children[node_to_internal(node) as usize];
            let split = child1 / 2;
            upper.push(node);
            stack.push((child1, first, split));
            stack.push((child2, split + 1, last));
        }
        subtrees.sort_unstable_by_key(|s| s.1);

        // Hand each subtree its run of the boxes.
        struct Subtree<'a> {
            root: i32,
            first: i32,
            last: i32,
            /// `node_bbox[2 * first..]`, as far as the subtree's last node.
            boxes: &'a mut [BBox],
        }
        let Collider {
            node_bbox,
            node_parent,
            internal_children,
            ..
        } = self;
        let node_parent: &[i32] = node_parent;
        let internal_children: &[[i32; 2]] = internal_children;
        let mut tasks: Vec<Subtree> = Vec::with_capacity(subtrees.len());
        let mut rest: &mut [BBox] = node_bbox;
        let mut rest_start = 0usize;
        for &(root, first, last) in &subtrees {
            let start = 2 * first as usize;
            let end = if node_to_internal(root) == last {
                2 * last as usize + 2
            } else {
                2 * last as usize + 1
            };
            let (_, tail) = std::mem::take(&mut rest).split_at_mut(start - rest_start);
            let (boxes, tail) = tail.split_at_mut(end - start);
            rest = tail;
            rest_start = end;
            tasks.push(Subtree {
                root,
                first,
                last,
                boxes,
            });
        }

        crate::par::maybe_par_for_each(tasks, 2, |subtree| {
            let Subtree {
                root,
                first,
                last,
                boxes,
            } = subtree;
            let base = 2 * first;
            // Children seen so far, per internal node `first + slot`.
            let mut counter = vec![0u8; (last - first + 1) as usize];
            for leaf in first..=last {
                let mut node = leaf_to_node(leaf);
                while node != root {
                    let parent = node_parent[node as usize];
                    let internal = node_to_internal(parent);
                    let slot = (internal - first) as usize;
                    counter[slot] += 1;
                    if counter[slot] < 2 {
                        break; // wait for the second child
                    }
                    let [c1, c2] = internal_children[internal as usize];
                    let b1 = boxes[(c1 - base) as usize];
                    let b2 = boxes[(c2 - base) as usize];
                    boxes[(parent - base) as usize] = b1.union_box(&b2);
                    node = parent;
                }
            }
        });

        for &node in upper.iter().rev() {
            let [c1, c2] = self.internal_children[node_to_internal(node) as usize];
            let b1 = self.node_bbox[c1 as usize];
            let b2 = self.node_bbox[c2 as usize];
            self.node_bbox[node as usize] = b1.union_box(&b2);
        }
    }

    /// Run a single query box against the BVH, invoking `record(query_idx,
    /// leaf_idx)` per overlap. Same traversal as `collisions_fn` for one
    /// index — the per-query entry point for parallel callers (`&self` only,
    /// so queries can run concurrently with thread-local recording).
    pub fn collisions_one<R: FnMut(usize, usize)>(
        &self,
        query: &BBox,
        query_idx: usize,
        mut record: R,
    ) {
        if query.is_empty() {
            return;
        }
        // No internal nodes (0-1 leaves): no collisions, matching C++
        // Collisions' early return on internalChildren_.empty().
        if self.internal_children.is_empty() {
            return;
        }
        self.traverse_bvh(query, query_idx, false, &mut record);
    }

    /// BVH-accelerated collision query with function-generated query boxes.
    /// For each query index 0..n, calls query_box_fn(i) to get the query AABB,
    /// then traverses the BVH to find overlapping leaves.
    pub fn collisions_fn<F, R>(&self, query_box_fn: F, n: usize, mut record: R)
    where
        F: Fn(usize) -> BBox,
        R: FnMut(usize, usize),
    {
        if self.internal_children.is_empty() {
            return;
        }

        for query_idx in 0..n {
            let query = query_box_fn(query_idx);
            if query.is_empty() {
                continue;
            }
            self.traverse_bvh(&query, query_idx, false, &mut record);
        }
    }

    /// BVH-accelerated collision query with pre-computed query boxes.
    pub fn collisions_with_boxes<F: FnMut(usize, usize)>(
        &self,
        queries: &[BBox],
        self_collision: bool,
        mut record: F,
    ) {
        if self.internal_children.is_empty() {
            return;
        }

        for (query_idx, query) in queries.iter().enumerate() {
            if query.is_empty() {
                continue;
            }
            self.traverse_bvh(query, query_idx, self_collision, &mut record);
        }
    }

    /// Point-based collision query using BVH.
    pub fn collisions_point<F, R>(&self, point_fn: F, n: usize, mut record: R)
    where
        F: Fn(usize) -> Vec3,
        R: FnMut(usize, usize),
    {
        if self.internal_children.is_empty() {
            return;
        }

        for query_idx in 0..n {
            let pt = point_fn(query_idx);
            let query = BBox::from_point(pt);
            self.traverse_bvh(&query, query_idx, false, &mut record);
        }
    }

    /// Stack-based depth-first BVH traversal (matches C++ FindCollision).
    fn traverse_bvh<F: FnMut(usize, usize)>(
        &self,
        query: &BBox,
        query_idx: usize,
        self_collision: bool,
        record: &mut F,
    ) {
        let mut stack = [0i32; 64];
        let mut top: i32 = -1;
        let mut node = K_ROOT;

        loop {
            let internal = node_to_internal(node);
            if internal < 0 || internal as usize >= self.internal_children.len() {
                if top < 0 {
                    break;
                }
                node = stack[top as usize];
                top -= 1;
                continue;
            }
            let [child1, child2] = self.internal_children[internal as usize];

            let traverse1 = self.check_node(query, child1, query_idx, self_collision, record);
            let traverse2 = self.check_node(query, child2, query_idx, self_collision, record);

            if !traverse1 && !traverse2 {
                if top < 0 {
                    break;
                }
                node = stack[top as usize];
                top -= 1;
            } else {
                node = if traverse1 { child1 } else { child2 };
                if traverse1 && traverse2 {
                    top += 1;
                    debug_assert!((top as usize) < 64, "BVH stack overflow");
                    stack[top as usize] = child2;
                }
            }
        }
    }

    /// Check if a node's AABB overlaps the query. If it's a leaf, record the hit.
    /// Returns true if the node is internal and overlaps (should traverse deeper).
    #[inline]
    fn check_node<F: FnMut(usize, usize)>(
        &self,
        query: &BBox,
        node: i32,
        query_idx: usize,
        self_collision: bool,
        record: &mut F,
    ) -> bool {
        if node < 0 || node as usize >= self.node_bbox.len() {
            return false;
        }
        let node_box = &self.node_bbox[node as usize];
        let overlaps = query.does_overlap_box(node_box);
        if overlaps && is_leaf(node) {
            // Leaves are stored in tree order == input order (pre-sorted by
            // morton), so the leaf index IS the caller's index — no mapping.
            let leaf_idx = node_to_leaf(node) as usize;
            if !self_collision || leaf_idx != query_idx {
                record(query_idx, leaf_idx);
            }
        }
        overlaps && is_internal(node)
    }

    /// Replace the leaf boxes (given in leaf/tree order — for a face collider
    /// that is face order, since faces are morton-sorted) and refit the
    /// internal boxes. Tree topology is untouched (C++ UpdateBoxes).
    pub fn update_boxes(&mut self, leaf_bbox: Vec<BBox>) {
        debug_assert_eq!(leaf_bbox.len(), self.num_leaves());
        for (i, bbox) in leaf_bbox.into_iter().enumerate() {
            self.node_bbox[leaf_to_node(i as i32) as usize] = bbox;
        }
        self.build_internal_boxes();
    }

    /// Map every node box through an axis-aligned transform (C++
    /// Collider::Transform) — no refit needed since axis-aligned transforms
    /// map AABBs to exact AABBs.
    pub fn transform(&mut self, transform: &Mat3x4) {
        debug_assert!(Self::is_axis_aligned(transform));
        for bbox in &mut self.node_bbox {
            *bbox = bbox.transform(transform);
        }
    }

    pub fn morton_code(position: Vec3, bbox: &BBox) -> u32 {
        morton_code(position, bbox)
    }

    pub fn is_axis_aligned(transform: &Mat3x4) -> bool {
        for row in 0..3 {
            let mut zero_count = 0;
            for col in 0..3 {
                if transform[col][row] == 0.0 {
                    zero_count += 1;
                }
            }
            if zero_count != 2 {
                return false;
            }
        }
        true
    }
}

pub fn edge_edge_dist(p: Vec3, a: Vec3, q: Vec3, b: Vec3) -> (Vec3, Vec3) {
    let t_vec = q - p;
    let a_dot_a = dot(a, a);
    let b_dot_b = dot(b, b);
    let a_dot_b = dot(a, b);
    let a_dot_t = dot(a, t_vec);
    let b_dot_t = dot(b, t_vec);

    let denom = a_dot_a * b_dot_b - a_dot_b * a_dot_b;
    let mut t = if denom != 0.0 {
        ((a_dot_t * b_dot_b - b_dot_t * a_dot_b) / denom).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let u = if b_dot_b != 0.0 {
        let u = (t * a_dot_b - b_dot_t) / b_dot_b;
        if u < 0.0 {
            t = if a_dot_a != 0.0 {
                (a_dot_t / a_dot_a).clamp(0.0, 1.0)
            } else {
                0.0
            };
            0.0
        } else if u > 1.0 {
            t = if a_dot_a != 0.0 {
                ((a_dot_b + a_dot_t) / a_dot_a).clamp(0.0, 1.0)
            } else {
                0.0
            };
            1.0
        } else {
            u
        }
    } else {
        t = if a_dot_a != 0.0 {
            (a_dot_t / a_dot_a).clamp(0.0, 1.0)
        } else {
            0.0
        };
        0.0
    };

    (p + a * t, q + b * u)
}

pub fn distance_triangle_triangle_squared(p: [Vec3; 3], q: [Vec3; 3]) -> f64 {
    let sv = [p[1] - p[0], p[2] - p[1], p[0] - p[2]];
    let tv = [q[1] - q[0], q[2] - q[1], q[0] - q[2]];

    let mut shown_disjoint = false;
    let mut mindd = f64::MAX;

    for i in 0..3 {
        for j in 0..3 {
            let (cp, cq) = edge_edge_dist(p[i], sv[i], q[j], tv[j]);
            let v = cq - cp;
            let dd = dot(v, v);

            if dd <= mindd {
                mindd = dd;

                let mut id = i + 2;
                if id >= 3 {
                    id -= 3;
                }
                let z = p[id] - cp;
                let mut a = dot(z, v);

                id = j + 2;
                if id >= 3 {
                    id -= 3;
                }
                let z = q[id] - cq;
                let mut b = dot(z, v);

                if a <= 0.0 && b >= 0.0 {
                    return dot(v, v);
                }

                if a <= 0.0 {
                    a = 0.0;
                } else if b > 0.0 {
                    b = 0.0;
                }

                if mindd - a + b > 0.0 {
                    shown_disjoint = true;
                }
            }
        }
    }

    let sn = cross(sv[0], sv[1]);
    let snl = dot(sn, sn);
    if snl > 1e-15 {
        let tp = Vec3::new(
            dot(p[0] - q[0], sn),
            dot(p[0] - q[1], sn),
            dot(p[0] - q[2], sn),
        );
        let mut index = None;
        if tp.x > 0.0 && tp.y > 0.0 && tp.z > 0.0 {
            let mut idx = if tp.x < tp.y { 0 } else { 1 };
            if tp.z < tp[idx] {
                idx = 2;
            }
            index = Some(idx);
        } else if tp.x < 0.0 && tp.y < 0.0 && tp.z < 0.0 {
            let mut idx = if tp.x > tp.y { 0 } else { 1 };
            if tp.z > tp[idx] {
                idx = 2;
            }
            index = Some(idx);
        }

        if let Some(index) = index {
            shown_disjoint = true;
            let q_index = q[index];
            let v = q_index - p[0];
            let z = cross(sn, sv[0]);
            if dot(v, z) > 0.0 {
                let v = q_index - p[1];
                let z = cross(sn, sv[1]);
                if dot(v, z) > 0.0 {
                    let v = q_index - p[2];
                    let z = cross(sn, sv[2]);
                    if dot(v, z) > 0.0 {
                        let cp = q_index + sn * (tp[index] / snl);
                        let cq = q_index;
                        return dot(cp - cq, cp - cq);
                    }
                }
            }
        }
    }

    let tn = cross(tv[0], tv[1]);
    let tnl = dot(tn, tn);
    if tnl > 1e-15 {
        let sp = Vec3::new(
            dot(q[0] - p[0], tn),
            dot(q[0] - p[1], tn),
            dot(q[0] - p[2], tn),
        );
        let mut index = None;
        if sp.x > 0.0 && sp.y > 0.0 && sp.z > 0.0 {
            let mut idx = if sp.x < sp.y { 0 } else { 1 };
            if sp.z < sp[idx] {
                idx = 2;
            }
            index = Some(idx);
        } else if sp.x < 0.0 && sp.y < 0.0 && sp.z < 0.0 {
            let mut idx = if sp.x > sp.y { 0 } else { 1 };
            if sp.z > sp[idx] {
                idx = 2;
            }
            index = Some(idx);
        }

        if let Some(index) = index {
            shown_disjoint = true;
            let p_index = p[index];
            let v = p_index - q[0];
            let z = cross(tn, tv[0]);
            if dot(v, z) > 0.0 {
                let v = p_index - q[1];
                let z = cross(tn, tv[1]);
                if dot(v, z) > 0.0 {
                    let v = p_index - q[2];
                    let z = cross(tn, tv[2]);
                    if dot(v, z) > 0.0 {
                        let cp = p_index;
                        let cq = p_index + tn * (sp[index] / tnl);
                        return dot(cp - cq, cp - cq);
                    }
                }
            }
        }
    }

    if shown_disjoint {
        mindd
    } else {
        0.0
    }
}

pub fn ray_triangle_intersection(origin: Vec3, direction: Vec3, tri: [Vec3; 3]) -> Option<f64> {
    let eps = 1e-9;
    let edge1 = tri[1] - tri[0];
    let edge2 = tri[2] - tri[0];
    let h = cross(direction, edge2);
    let a = dot(edge1, h);
    if a.abs() < eps {
        return None;
    }
    let f = 1.0 / a;
    let s = origin - tri[0];
    let u = f * dot(s, h);
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = cross(s, edge1);
    let v = f * dot(direction, q);
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = f * dot(edge2, q);
    if t > eps {
        Some(t)
    } else {
        None
    }
}

impl ManifoldImpl {
    pub fn is_self_intersecting(&self) -> bool {
        let ep = 2.0 * self.epsilon;
        let epsilon_sq = ep * ep;
        // Fresh face boxes for the queries; the tree itself is the cached
        // collider (C++ SelfIntersecting queries collider_ the same way).
        let (face_box, _face_morton) = get_face_box_morton(self);
        let collider = &self.collider;
        let mut intersecting = false;

        collider.collisions_with_boxes(&face_box, true, |tri0, tri1| {
            if intersecting {
                return;
            }
            let tri_verts0 = self.face_triangle_vertices(tri0);
            let tri_verts1 = self.face_triangle_vertices(tri1);

            for a in &tri_verts0 {
                for b in &tri_verts1 {
                    if distance2(*a, *b) <= epsilon_sq {
                        return;
                    }
                }
            }

            if distance_triangle_triangle_squared(tri_verts0, tri_verts1) == 0.0 {
                let mut tmp0 = tri_verts0;
                let mut tmp1 = tri_verts1;
                for i in 0..3 {
                    tmp0[i] = tri_verts0[i] + self.face_normal[tri1] * ep;
                }
                if distance_triangle_triangle_squared(tmp0, tri_verts1) > 0.0 {
                    return;
                }
                for i in 0..3 {
                    tmp0[i] = tri_verts0[i] - self.face_normal[tri1] * ep;
                }
                if distance_triangle_triangle_squared(tmp0, tri_verts1) > 0.0 {
                    return;
                }
                for i in 0..3 {
                    tmp1[i] = tri_verts1[i] + self.face_normal[tri0] * ep;
                }
                if distance_triangle_triangle_squared(tri_verts0, tmp1) > 0.0 {
                    return;
                }
                for i in 0..3 {
                    tmp1[i] = tri_verts1[i] - self.face_normal[tri0] * ep;
                }
                if distance_triangle_triangle_squared(tri_verts0, tmp1) > 0.0 {
                    return;
                }
                intersecting = true;
            }
        });

        intersecting
    }

    pub fn min_gap(&self, other: &ManifoldImpl, search_length: f64) -> f64 {
        let (mut other_box, _) = get_face_box_morton(other);
        for bbox in &mut other_box {
            bbox.min = bbox.min - Vec3::splat(search_length);
            bbox.max = bbox.max + Vec3::splat(search_length);
        }

        // Query self's cached face BVH (C++ MinGap queries collider_).
        let collider = &self.collider;
        let mut min_distance = f64::INFINITY;
        collider.collisions_with_boxes(&other_box, false, |tri_other, tri| {
            let p = self.face_triangle_vertices(tri);
            let q = other.face_triangle_vertices(tri_other);
            min_distance = min_distance.min(distance_triangle_triangle_squared(p, q));
        });

        min_distance.min(search_length * search_length).sqrt()
    }

    fn face_triangle_vertices(&self, tri: usize) -> [Vec3; 3] {
        [
            self.vert_pos[self.halfedge[3 * tri].start_vert as usize],
            self.vert_pos[self.halfedge[3 * tri + 1].start_vert as usize],
            self.vert_pos[self.halfedge[3 * tri + 2].start_vert as usize],
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linalg::{mat4_to_mat3x4, translation_matrix};

    #[test]
    fn test_collider_box_overlap() {
        let boxes = vec![
            BBox::from_points(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0)),
            BBox::from_points(Vec3::new(2.0, 2.0, 2.0), Vec3::new(3.0, 3.0, 3.0)),
        ];
        let collider = Collider::new(boxes.clone(), vec![0, 1]);
        let mut hits = Vec::new();
        collider.collisions_with_boxes(&boxes, true, |a, b| hits.push((a, b)));
        assert!(hits.is_empty());

        let queries = vec![BBox::from_points(
            Vec3::new(0.5, 0.5, 0.5),
            Vec3::new(2.5, 2.5, 2.5),
        )];
        collider.collisions_with_boxes(&queries, false, |a, b| hits.push((a, b)));
        assert_eq!(hits, vec![(0, 0), (0, 1)]);
    }

    #[test]
    fn test_ray_triangle_intersection() {
        let tri = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        let hit =
            ray_triangle_intersection(Vec3::new(0.25, 0.25, -1.0), Vec3::new(0.0, 0.0, 1.0), tri);
        assert!(hit.is_some());
    }

    #[test]
    fn test_triangle_triangle_distance_zero_for_intersection() {
        let a = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        let b = [
            Vec3::new(0.25, 0.25, -1.0),
            Vec3::new(0.25, 0.25, 1.0),
            Vec3::new(0.75, 0.25, 0.0),
        ];
        assert_eq!(distance_triangle_triangle_squared(a, b), 0.0);
    }

    #[test]
    fn test_cube_not_self_intersecting() {
        let m = ManifoldImpl::cube(&Mat3x4::identity());
        assert!(!m.is_self_intersecting());
    }

    #[test]
    fn test_min_gap_between_cubes() {
        let a = ManifoldImpl::cube(&Mat3x4::identity());
        let b = ManifoldImpl::cube(&mat4_to_mat3x4(translation_matrix(Vec3::new(
            2.0, 0.0, 0.0,
        ))));
        let gap = a.min_gap(&b, 5.0);
        assert!((gap - 1.0).abs() < 1e-8, "gap = {}", gap);
    }

    #[test]
    fn test_bvh_many_boxes() {
        // Create many non-overlapping boxes and verify BVH finds the right pairs
        let n = 100;
        let mut boxes = Vec::new();
        let mut mortons = Vec::new();
        for i in 0..n {
            let x = i as f64 * 3.0;
            boxes.push(BBox::from_points(
                Vec3::new(x, 0.0, 0.0),
                Vec3::new(x + 1.0, 1.0, 1.0),
            ));
            mortons.push(i as u32);
        }
        let collider = Collider::new(boxes.clone(), mortons);

        // Query that overlaps box 50
        let query = vec![BBox::from_points(
            Vec3::new(150.5, 0.5, 0.5),
            Vec3::new(150.6, 0.6, 0.6),
        )];
        let mut hits = Vec::new();
        collider.collisions_with_boxes(&query, false, |a, b| hits.push((a, b)));
        assert_eq!(hits, vec![(0, 50)]);
    }
}
