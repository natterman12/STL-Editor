// EarClip triangulator — extracted from polygon.rs
// Port of C++ ear-clipping algorithm with 2D KD-tree acceleration
// The hole keyholing (CutKeyhole, FindCloserBridge, JoinPolygons) is a second
// `impl EarClip` block in polygon_earclip_keyhole.rs.

use crate::linalg::Vec2;
use crate::types::{PolyVert, PolygonsIdx, Rect, K_PRECISION};
use std::collections::HashMap;

use super::{
    build_two_d_tree, ccw, determinant2x2, dot2d, safe_normalize_2d, IVec3Out, INVALID, K_BEST,
};

// ---------------------------------------------------------------------------
// Supporting types
// ---------------------------------------------------------------------------

pub(super) struct Vert {
    pub mesh_idx: i32,
    pub cost: f64,
    pub ear_version: u32,
    pub pos: Vec2,
    pub right_dir: Vec2,
    pub left: usize,
    pub right: usize,
}

impl Clone for Vert {
    fn clone(&self) -> Self {
        Vert {
            mesh_idx: self.mesh_idx,
            cost: self.cost,
            ear_version: self.ear_version,
            pos: self.pos,
            right_dir: self.right_dir,
            left: self.left,
            right: self.right,
        }
    }
}

/// Entry in the min-heap ear queue.
#[derive(Clone, Copy, PartialEq)]
struct EarEntry {
    cost: f64,
    idx: usize,
    version: u32,
    /// Monotonic insertion sequence. C++ uses a std::multiset, which keeps
    /// equal-cost entries in INSERTION order and pops the oldest first —
    /// equal costs are common (kBest = -inf for short ears), so this FIFO
    /// tie-break decides which degenerate ear clips first and must match.
    seq: u64,
}

impl Eq for EarEntry {}

impl PartialOrd for EarEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EarEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Min-heap: lower cost = higher priority (appears first in pop);
        // ties: lower seq (older insertion) = higher priority.
        // BinaryHeap is max-heap, so "greater" = higher priority.
        other
            .cost
            .partial_cmp(&self.cost)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(other.seq.cmp(&self.seq))
    }
}

/// The verts of one polygon in a 2D kd-tree, for the ear cost queries.
struct IdxCollider {
    points: Vec<PolyVert>,
    /// The polygon vert at each position of `points`.
    verts: Vec<usize>,
    /// Per position of `points`: false once that vert has been clipped. A
    /// query skips those positions without looking the vert up. Clipped verts
    /// stay in the tree, and late in a large polygon they are most of what a
    /// query finds.
    alive: Vec<bool>,
    /// Per position of `points`: where that vert's left and right neighbours
    /// in the ring are now. `triangulate_poly` keeps them current as it clips,
    /// so the ear cost of a vert that lies on an edge line (which looks at
    /// its neighbours) is worked out without following the ring.
    left_pos: Vec<Vec2>,
    right_pos: Vec<Vec2>,
}

/// `query_two_d_tree` over the positions of `points` whose `alive` flag is
/// set: calls `f` with the position of every such point inside `r`. The
/// traversal is the same; a point that is not alive is passed over.
fn query_alive<F: FnMut(usize)>(points: &[PolyVert], alive: &[bool], r: Rect, mut f: F) {
    if points.len() <= 8 {
        for (pos, p) in points.iter().enumerate() {
            if alive[pos] && r.contains_point(p.pos) {
                f(pos);
            }
        }
        return;
    }

    // (current_rect, start, len, level). A node halves its range, so the tree
    // of any slice is under 64 levels deep and the stack holds at most one
    // entry more than that.
    let inf_rect = Rect {
        min: Vec2::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
        max: Vec2::new(f64::INFINITY, f64::INFINITY),
    };
    let mut stack = [(inf_rect, 0usize, 0usize, 0i32); 66];
    let mut top = 0usize;
    stack[top] = (inf_rect, 0, points.len(), 0);
    top += 1;

    while top > 0 {
        top -= 1;
        let (current, start, len, level) = stack[top];
        if len <= 8 {
            for pos in start..start + len {
                if alive[pos] && r.contains_point(points[pos].pos) {
                    f(pos);
                }
            }
            continue;
        }

        let mid = len / 2;
        let middle = points[start + mid];

        let mut left = current;
        let mut right = current;
        if level % 2 == 0 {
            left.max.x = middle.pos.x;
            right.min.x = middle.pos.x;
        } else {
            left.max.y = middle.pos.y;
            right.min.y = middle.pos.y;
        }

        if alive[start + mid] && r.contains_point(middle.pos) {
            f(start + mid);
        }

        if left.does_overlap(&r) {
            stack[top] = (left, start, mid, level + 1);
            top += 1;
        }
        if right.does_overlap(&r) {
            stack[top] = (right, start + mid + 1, len - mid - 1, level + 1);
            top += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// EarClip
// ---------------------------------------------------------------------------

pub(super) struct EarClip {
    polygon: Vec<Vert>,
    holes: Vec<usize>,
    outers: Vec<usize>,
    /// Bounding box of each outer ring, parallel to `outers` and grown as
    /// holes join, so the bridge searches can skip rings. Not in C++.
    outer_bbox: Vec<Rect>,
    simples: Vec<usize>,
    hole2bbox: HashMap<usize, Rect>,
    ears_queue: std::collections::BinaryHeap<EarEntry>,
    /// Monotonic counter for EarEntry::seq (FIFO tie-break among equal costs).
    ear_seq: u64,
    /// The position of each vert in the kd-tree of the polygon being
    /// triangulated (`IdxCollider::points`); set by `vert_collider`.
    kd_pos: Vec<u32>,
    pub triangles: Vec<IVec3Out>,
    bbox: Rect,
    pub epsilon: f64,
}

impl EarClip {
    pub fn new(polys: &PolygonsIdx, epsilon: f64) -> Self {
        let num_vert: usize = polys.iter().map(|p| p.len()).sum();
        let mut ec = EarClip {
            polygon: Vec::with_capacity(num_vert + 2 * polys.len()),
            holes: Vec::new(),
            outers: Vec::new(),
            outer_bbox: Vec::new(),
            simples: Vec::new(),
            hole2bbox: HashMap::new(),
            ears_queue: std::collections::BinaryHeap::new(),
            ear_seq: 0,
            kd_pos: Vec::new(),
            triangles: Vec::with_capacity(num_vert + 2 * polys.len()),
            bbox: Rect::new(),
            epsilon,
        };

        let starts = ec.initialize(polys);

        // Clip degenerate verts
        for i in 0..ec.polygon.len() {
            ec.clip_if_degenerate(i);
        }

        // Find start/classify each polygon
        for start in starts {
            ec.find_start(start);
        }
        // C++ stores holes_ in a multiset ordered by MaxX. Keyholing from
        // right to left prevents a later bridge from crossing an earlier one.
        ec.holes.sort_by(|&left, &right| {
            ec.polygon[right]
                .pos
                .x
                .partial_cmp(&ec.polygon[left].pos.x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        ec
    }

    pub fn triangulate(mut self) -> (Vec<IVec3Out>, f64) {
        // Key-hole all holes into outer polygons
        let holes: Vec<usize> = self.holes.clone();
        for start in holes {
            self.cut_keyhole(start);
        }

        // Ear-clip each simple polygon
        let simples: Vec<usize> = self.simples.clone();
        for start in simples {
            self.triangulate_poly(start);
        }

        let eps = self.epsilon;
        (self.triangles, eps)
    }

    // -----------------------------------------------------------------------
    // Linked-list helpers
    // -----------------------------------------------------------------------

    fn clipped(&self, v: usize) -> bool {
        self.polygon[self.polygon[v].right].left != v
    }

    fn link(&mut self, left: usize, right: usize) {
        self.polygon[left].right = right;
        self.polygon[right].left = left;
        let dir = self.polygon[right].pos - self.polygon[left].pos;
        self.polygon[left].right_dir = safe_normalize_2d(dir);
    }

    /// The unclipped verts of the ring starting from `first`, or `None` if
    /// the ring is degenerate.
    fn loop_verts(&self, first: usize) -> Option<Vec<usize>> {
        let mut result = Vec::new();
        self.for_each_loop_vert(first, |v| result.push(v))
            .then_some(result)
    }

    /// Apply `f` to each vert `loop_verts` would return, without collecting
    /// them, as C++ `Loop` does. Returns `false` if the ring is degenerate,
    /// after `f` has already seen the verts before the degenerate one.
    fn for_each_loop_vert(&self, first: usize, mut f: impl FnMut(usize)) -> bool {
        let mut v = first;
        let mut cur_first = first;
        loop {
            if self.clipped(v) {
                cur_first = self.polygon[self.polygon[v].right].left;
                if !self.clipped(cur_first) {
                    v = cur_first;
                    if self.polygon[v].right == self.polygon[v].left {
                        return false;
                    }
                    f(v);
                }
            } else {
                if self.polygon[v].right == self.polygon[v].left {
                    return false;
                }
                f(v);
            }
            v = self.polygon[v].right;
            if v == cur_first {
                break;
            }
        }
        true
    }

    // -----------------------------------------------------------------------
    // Vert predicate methods (take vert index `v`)
    // -----------------------------------------------------------------------

    fn vert_is_short(&self, v: usize) -> bool {
        let edge = self.polygon[self.polygon[v].right].pos - self.polygon[v].pos;
        dot2d(edge, edge) * 4.0 < self.epsilon * self.epsilon
    }

    // NOTE: C++ `Vert::Interior` (polygon.cpp) is not ported: it is defined but
    // never called in the upstream reference either.

    /// Returns true if vert `v` is on the inside of the edge tail -> tail.right.
    /// to_left: walk v's polygon edges to the left (vs right).
    fn vert_inside_edge(&self, v: usize, tail: usize, to_left: bool) -> bool {
        let p2 = self.epsilon * self.epsilon;
        let mut next_l = self.polygon[self.polygon[v].left].right;
        let mut next_r = self.polygon[tail].right;
        let mut center = tail;
        let mut last = center;

        let v_stop = if to_left {
            self.polygon[v].right
        } else {
            self.polygon[v].left
        };

        loop {
            if next_l == next_r || tail == next_r || next_l == v_stop {
                break;
            }

            let edge_l = self.polygon[next_l].pos - self.polygon[center].pos;
            let l2 = dot2d(edge_l, edge_l);
            if l2 <= p2 {
                next_l = if to_left {
                    self.polygon[next_l].left
                } else {
                    self.polygon[next_l].right
                };
                continue;
            }

            let edge_r = self.polygon[next_r].pos - self.polygon[center].pos;
            let r2 = dot2d(edge_r, edge_r);
            if r2 <= p2 {
                next_r = self.polygon[next_r].right;
                continue;
            }

            let vec_lr = self.polygon[next_r].pos - self.polygon[next_l].pos;
            let lr2 = dot2d(vec_lr, vec_lr);
            if lr2 <= p2 {
                last = center;
                center = next_l;
                next_l = if to_left {
                    self.polygon[next_l].left
                } else {
                    self.polygon[next_l].right
                };
                if next_l == next_r {
                    break;
                }
                next_r = self.polygon[next_r].right;
                continue;
            }

            let mut convexity = ccw(
                self.polygon[next_l].pos,
                self.polygon[center].pos,
                self.polygon[next_r].pos,
                self.epsilon,
            );
            if center != last {
                convexity += ccw(
                    self.polygon[last].pos,
                    self.polygon[center].pos,
                    self.polygon[next_l].pos,
                    self.epsilon,
                ) + ccw(
                    self.polygon[next_r].pos,
                    self.polygon[center].pos,
                    self.polygon[last].pos,
                    self.epsilon,
                );
            }
            if convexity != 0 {
                return convexity > 0;
            }

            if l2 < r2 {
                center = next_l;
                next_l = if to_left {
                    self.polygon[next_l].left
                } else {
                    self.polygon[next_l].right
                };
            } else {
                center = next_r;
                next_r = self.polygon[next_r].right;
            }
            last = center;
        }
        true
    }

    fn vert_is_convex(&self, v: usize, epsilon: f64) -> bool {
        let left = self.polygon[v].left;
        let right = self.polygon[v].right;
        ccw(
            self.polygon[left].pos,
            self.polygon[v].pos,
            self.polygon[right].pos,
            epsilon,
        ) >= 0
    }

    fn vert_is_reflex(&self, v: usize) -> bool {
        let left = self.polygon[v].left;
        !self.vert_inside_edge(left, self.polygon[left].right, true)
    }

    /// x-value on this vert's right edge at start.y, or NaN if no crossing.
    fn vert_interp_y2x(&self, v: usize, start: Vec2, on_top: i32) -> f64 {
        let pos = self.polygon[v].pos;
        let rpos = self.polygon[self.polygon[v].right].pos;
        let eps = self.epsilon;
        if (pos.y - start.y).abs() <= eps {
            if rpos.y <= start.y + eps || on_top == 1 {
                f64::NAN
            } else {
                pos.x
            }
        } else if pos.y < start.y - eps {
            if rpos.y > start.y + eps {
                pos.x + (start.y - pos.y) * (rpos.x - pos.x) / (rpos.y - pos.y)
            } else if rpos.y < start.y - eps || on_top == -1 {
                f64::NAN
            } else {
                rpos.x
            }
        } else {
            f64::NAN
        }
    }

    /// Signed distance of vert `other` relative to the edge at `v` in direction `unit`.
    fn vert_signed_dist(&self, v: usize, other: usize, unit: Vec2) -> f64 {
        let eps = self.epsilon;
        let d = determinant2x2(unit, self.polygon[other].pos - self.polygon[v].pos);
        if d.abs() < eps {
            let d_r = determinant2x2(
                unit,
                self.polygon[self.polygon[other].right].pos - self.polygon[v].pos,
            );
            if d_r.abs() > eps {
                return d_r;
            }
            let d_l = determinant2x2(
                unit,
                self.polygon[self.polygon[other].left].pos - self.polygon[v].pos,
            );
            if d_l.abs() > eps {
                return d_l;
            }
        }
        d
    }

    /// Cost of vert `other` within ear `v`, where `open_side` is the unit vector left->right.
    fn vert_cost(&self, v: usize, other: usize, open_side: Vec2) -> f64 {
        let left = self.polygon[v].left;
        let right = self.polygon[v].right;
        let cost = self
            .vert_signed_dist(v, other, self.polygon[v].right_dir)
            .min(self.vert_signed_dist(left, other, self.polygon[left].right_dir));
        let open_cost =
            determinant2x2(open_side, self.polygon[other].pos - self.polygon[right].pos);
        cost.min(open_cost)
    }

    fn delaunay_cost(diff: Vec2, scale: f64, epsilon: f64) -> f64 {
        -epsilon - scale * dot2d(diff, diff)
    }

    fn vert_ear_cost(&self, v: usize, collider: &IdxCollider) -> f64 {
        let left = self.polygon[v].left;
        let right = self.polygon[v].right;
        let open_side_vec = self.polygon[left].pos - self.polygon[right].pos;
        let center = (self.polygon[left].pos + self.polygon[right].pos) * 0.5;
        let denom = dot2d(open_side_vec, open_side_vec);
        let scale = if denom > 0.0 { 4.0 / denom } else { 0.0 };
        let radius = denom.sqrt() * 0.5;
        let open_side = safe_normalize_2d(open_side_vec);

        let total_cost =
            dot2d(self.polygon[left].right_dir, self.polygon[v].right_dir) - 1.0 - self.epsilon;

        // Folded ears: clip first
        if ccw(
            self.polygon[v].pos,
            self.polygon[left].pos,
            self.polygon[right].pos,
            self.epsilon,
        ) == 0
        {
            return total_cost;
        }

        // Build ear bounding box, expanded to include pos, then by epsilon
        let cx = center.x;
        let cy = center.y;
        let mut ear_box = Rect::from_points(
            Vec2::new(cx - radius, cy - radius),
            Vec2::new(cx + radius, cy + radius),
        );
        ear_box.union_point(self.polygon[v].pos);
        ear_box.min = ear_box.min - Vec2::splat(self.epsilon);
        ear_box.max = ear_box.max + Vec2::splat(self.epsilon);

        let lid = self.polygon[left].mesh_idx;
        let rid = self.polygon[right].mesh_idx;
        let mid_id = self.polygon[v].mesh_idx;

        // `vert_cost` of every vert in the box, and the largest wins. The
        // cost is worked out here from the copies the tree keeps of the
        // vert's position and of its two neighbours' positions, with the
        // arithmetic of `vert_cost` and `vert_signed_dist`: the signed
        // distance is the first determinant unless that is within epsilon of
        // zero, and then the right neighbour's, or the left one's, if either
        // is clear of zero. Whether the vert counts at all (not clipped, not
        // a corner of this ear) is asked last, and only of a vert whose cost
        // would raise the result. The values and the result are those of
        // asking first and calling `vert_cost`; a large polygon just no
        // longer follows the ring's pointers for every vert in the box.
        let eps = self.epsilon;
        let v_pos = self.polygon[v].pos;
        let v_dir = self.polygon[v].right_dir;
        let l_pos = self.polygon[left].pos;
        let l_dir = self.polygon[left].right_dir;
        let r_pos = self.polygon[right].pos;
        let mut tc = total_cost;
        query_alive(&collider.points, &collider.alive, ear_box, |pos| {
            let p = collider.points[pos].pos;
            // `vert_signed_dist` once its first determinant `d` is known.
            let signed_dist = |unit: Vec2, origin: Vec2, d: f64| -> f64 {
                if d.abs() < eps {
                    let d_r = determinant2x2(unit, collider.right_pos[pos] - origin);
                    if d_r.abs() > eps {
                        return d_r;
                    }
                    let d_l = determinant2x2(unit, collider.left_pos[pos] - origin);
                    if d_l.abs() > eps {
                        return d_l;
                    }
                }
                d
            };
            let d_v = signed_dist(v_dir, v_pos, determinant2x2(v_dir, p - v_pos));
            let d_l = signed_dist(l_dir, l_pos, determinant2x2(l_dir, p - l_pos));
            let mut cost = d_v.min(d_l).min(determinant2x2(open_side, p - r_pos));
            debug_assert!(
                {
                    let reference = self.vert_cost(v, collider.verts[pos], open_side);
                    reference == cost || (reference.is_nan() && cost.is_nan())
                },
                "ear cost from the tree's copies differs from vert_cost"
            );
            if cost < -eps {
                cost = Self::delaunay_cost(p - center, scale, eps);
            }
            if cost > tc {
                let test = collider.verts[pos];
                if !self.clipped(test)
                    && self.polygon[test].mesh_idx != mid_id
                    && self.polygon[test].mesh_idx != lid
                    && self.polygon[test].mesh_idx != rid
                {
                    tc = cost;
                }
            }
        });
        tc
    }

    // -----------------------------------------------------------------------
    // Core algorithm methods
    // -----------------------------------------------------------------------

    /// Remove ear vert from polygon and emit a triangle.
    fn clip_ear(&mut self, ear: usize) {
        let left = self.polygon[ear].left;
        let right = self.polygon[ear].right;
        self.link(left, right);
        if self.polygon[left].mesh_idx != self.polygon[ear].mesh_idx
            && self.polygon[ear].mesh_idx != self.polygon[right].mesh_idx
            && self.polygon[right].mesh_idx != self.polygon[left].mesh_idx
        {
            self.triangles.push(IVec3Out::new(
                self.polygon[left].mesh_idx,
                self.polygon[ear].mesh_idx,
                self.polygon[right].mesh_idx,
            ));
        }
    }

    /// Clip degenerate ears (zero-area or very short edges).
    fn clip_if_degenerate(&mut self, ear: usize) {
        if self.clipped(ear) {
            return;
        }
        if self.polygon[ear].left == self.polygon[ear].right {
            return;
        }
        let right = self.polygon[ear].right;
        let left = self.polygon[ear].left;
        let is_short = self.vert_is_short(ear);
        let is_colinear_fold = {
            let lp = self.polygon[left].pos;
            let ep = self.polygon[ear].pos;
            let rp = self.polygon[right].pos;
            let diff_l = lp - ep;
            let diff_r = rp - ep;
            ccw(lp, ep, rp, self.epsilon) == 0 && dot2d(diff_l, diff_r) > 0.0
        };
        if is_short || is_colinear_fold {
            self.clip_ear(ear);
            let l = self.polygon[ear].left;
            let r = self.polygon[ear].right;
            self.clip_if_degenerate(l);
            self.clip_if_degenerate(r);
        }
    }

    /// Build the circular polygon list from input polygons. Returns start indices.
    fn initialize(&mut self, polys: &PolygonsIdx) -> Vec<usize> {
        let mut starts = Vec::new();
        for poly in polys {
            if poly.is_empty() {
                continue;
            }
            let first_idx = self.polygon.len();
            let vert = &poly[0];
            self.polygon.push(Vert {
                mesh_idx: vert.idx,
                cost: 0.0,
                ear_version: 0,
                pos: vert.pos,
                right_dir: Vec2::new(0.0, 0.0),
                left: INVALID,
                right: INVALID,
            });
            self.bbox.union_point(vert.pos);
            let mut last = first_idx;
            starts.push(first_idx);

            for vert in &poly[1..] {
                let next_idx = self.polygon.len();
                self.polygon.push(Vert {
                    mesh_idx: vert.idx,
                    cost: 0.0,
                    ear_version: 0,
                    pos: vert.pos,
                    right_dir: Vec2::new(0.0, 0.0),
                    left: INVALID,
                    right: INVALID,
                });
                self.bbox.union_point(vert.pos);
                self.link(last, next_idx);
                last = next_idx;
            }
            self.link(last, first_idx);
        }

        if self.epsilon < 0.0 {
            self.epsilon = self.bbox.scale() * K_PRECISION;
        }

        starts
    }

    /// Classify a polygon as hole or outer. For holes, find the rightmost reflex vert.
    fn find_start(&mut self, first: usize) {
        let origin = self.polygon[first].pos;
        let mut start = first;
        let mut max_x = f64::NEG_INFINITY;
        let mut bbox = Rect::new();
        // Kahan summation
        let mut area: f64 = 0.0;
        let mut area_comp: f64 = 0.0;

        let verts = match self.loop_verts(first) {
            None => return,
            Some(v) => v,
        };

        for &v in &verts {
            bbox.union_point(self.polygon[v].pos);
            let a1 = determinant2x2(
                self.polygon[v].pos - origin,
                self.polygon[self.polygon[v].right].pos - origin,
            );
            let t1 = area + a1;
            area_comp += (area - t1) + a1;
            area = t1;

            if self.polygon[v].pos.x > max_x && self.vert_is_reflex(v) {
                max_x = self.polygon[v].pos.x;
                start = v;
            }
        }

        area += area_comp;
        let size = bbox.size();
        let min_area = self.epsilon * size.x.max(size.y);

        if max_x.is_finite() && area < -min_area {
            // Hole (negative area)
            // Collected here and sorted by descending max X after classification.
            self.holes.push(start);
            self.hole2bbox.insert(start, bbox);
        } else {
            self.simples.push(start);
            if area > min_area {
                self.outers.push(start);
                self.outer_bbox.push(bbox);
            }
        }
    }

    /// Update ear queue entry for vert v.
    fn process_ear(&mut self, v: usize, collider: &IdxCollider) {
        // Lazy-delete existing queue entry
        self.polygon[v].ear_version = self.polygon[v].ear_version.wrapping_add(1);

        if self.vert_is_short(v) {
            self.polygon[v].cost = K_BEST;
            let version = self.polygon[v].ear_version;
            let seq = self.ear_seq;
            self.ear_seq += 1;
            self.ears_queue.push(EarEntry {
                cost: K_BEST,
                idx: v,
                version,
                seq,
            });
        } else if self.vert_is_convex(v, 2.0 * self.epsilon) {
            let cost = self.vert_ear_cost(v, collider);
            self.polygon[v].cost = cost;
            let version = self.polygon[v].ear_version;
            let seq = self.ear_seq;
            self.ear_seq += 1;
            self.ears_queue.push(EarEntry {
                cost,
                idx: v,
                version,
                seq,
            });
        } else {
            self.polygon[v].cost = 1.0; // reflex, not an ear
        }
    }

    /// Build a 2D KD-tree collider of all polygon verts for ear cost queries.
    fn vert_collider(&mut self, start: usize) -> IdxCollider {
        let loop_verts = match self.loop_verts(start) {
            None => {
                return IdxCollider {
                    points: Vec::new(),
                    verts: Vec::new(),
                    alive: Vec::new(),
                    left_pos: Vec::new(),
                    right_pos: Vec::new(),
                }
            }
            Some(v) => v,
        };

        // `idx` is the place in `loop_verts`.
        let mut points = Vec::with_capacity(loop_verts.len());
        for (k, &v) in loop_verts.iter().enumerate() {
            points.push(PolyVert {
                pos: self.polygon[v].pos,
                idx: k as i32,
            });
        }

        build_two_d_tree(&mut points);

        // Each vert of the loop is unclipped now; `triangulate_poly` clears
        // its flag when it clips it.
        let verts: Vec<usize> = points.iter().map(|p| loop_verts[p.idx as usize]).collect();
        if self.kd_pos.len() < self.polygon.len() {
            self.kd_pos.resize(self.polygon.len(), u32::MAX);
        }
        for (pos, &v) in verts.iter().enumerate() {
            self.kd_pos[v] = pos as u32;
        }
        let alive = vec![true; verts.len()];
        let left_pos = verts
            .iter()
            .map(|&v| self.polygon[self.polygon[v].left].pos)
            .collect();
        let right_pos = verts
            .iter()
            .map(|&v| self.polygon[self.polygon[v].right].pos)
            .collect();
        IdxCollider {
            points,
            verts,
            alive,
            left_pos,
            right_pos,
        }
    }

    /// Main ear-clipping loop for a simple polygon.
    fn triangulate_poly(&mut self, start: usize) {
        let mut collider = self.vert_collider(start);
        if collider.verts.is_empty() {
            return;
        }

        self.ears_queue.clear();
        let mut num_tri: i32 = -2;

        // Queue all verts and count
        let verts = match self.loop_verts(start) {
            None => return,
            Some(v) => v,
        };

        for &v in &verts {
            self.process_ear(v, &collider);
            num_tri += 1;
        }

        let mut last_v = *verts.last().unwrap();

        while num_tri > 0 {
            // Pop the cheapest valid ear
            let ear = loop {
                match self.ears_queue.pop() {
                    None => {
                        // Fallback: use last_v
                        break last_v;
                    }
                    Some(entry) => {
                        if entry.version == self.polygon[entry.idx].ear_version {
                            break entry.idx;
                        }
                        // Stale entry, try next
                    }
                }
            };

            self.clip_ear(ear);
            num_tri -= 1;
            // The ear is out of the ring for good: nothing links to it again,
            // so `clipped(ear)` holds from here on and the ear cost queries
            // can pass it over by its flag alone.
            debug_assert!(self.clipped(ear), "a clipped ear is still in its ring");
            collider.alive[self.kd_pos[ear] as usize] = false;

            let ear_left = self.polygon[ear].left;
            let ear_right = self.polygon[ear].right;
            // The two verts beside the ear are now each other's neighbours.
            collider.right_pos[self.kd_pos[ear_left] as usize] = self.polygon[ear_right].pos;
            collider.left_pos[self.kd_pos[ear_right] as usize] = self.polygon[ear_left].pos;
            self.process_ear(ear_left, &collider);
            self.process_ear(ear_right, &collider);
            last_v = ear_right;
        }
    }
}

#[path = "polygon_earclip_keyhole.rs"]
mod keyhole;

#[cfg(test)]
#[path = "polygon_earclip_tests.rs"]
mod tests;
