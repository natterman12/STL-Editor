//! Static KD-tree over 3D points: nearest distance (like scipy `cKDTree.query`) and
//! radius search (like `query_ball_point`, radius inclusive).

const LEAF: usize = 8;

enum Node {
    Leaf { start: usize, end: usize },
    Split { axis: usize, value: f64, left: usize, right: usize },
}

pub struct PointTree {
    pts: Vec<[f64; 3]>,
    /// Original index of the point stored at each slot of `pts`.
    ids: Vec<usize>,
    nodes: Vec<Node>,
}

#[inline]
fn d2(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}

impl PointTree {
    pub fn new(points: &[[f64; 3]]) -> Self {
        let mut order: Vec<usize> = (0..points.len()).collect();
        let mut nodes = Vec::new();
        if !points.is_empty() {
            build(points, &mut order, 0, points.len(), &mut nodes);
        }
        let pts = order.iter().map(|&i| points[i]).collect();
        Self { pts, ids: order, nodes }
    }

    /// Distance from `q` to the closest point (infinity for an empty tree).
    pub fn nearest_dist(&self, q: [f64; 3]) -> f64 {
        if self.nodes.is_empty() {
            return f64::INFINITY;
        }
        let mut best = f64::INFINITY;
        self.nearest(0, &q, &mut best);
        best.sqrt()
    }

    fn nearest(&self, node: usize, q: &[f64; 3], best: &mut f64) {
        match &self.nodes[node] {
            Node::Leaf { start, end } => {
                for p in &self.pts[*start..*end] {
                    let d = d2(p, q);
                    if d < *best {
                        *best = d;
                    }
                }
            }
            Node::Split { axis, value, left, right } => {
                let diff = q[*axis] - *value;
                let (near, far) = if diff <= 0.0 { (*left, *right) } else { (*right, *left) };
                self.nearest(near, q, best);
                if diff * diff < *best {
                    self.nearest(far, q, best);
                }
            }
        }
    }

    /// Indices of all points within distance `r` of `q` (inclusive), sorted ascending.
    pub fn within(&self, q: [f64; 3], r: f64) -> Vec<usize> {
        let mut out = Vec::new();
        if !self.nodes.is_empty() && r >= 0.0 {
            self.ball(0, &q, r * r, &mut out);
        }
        out.sort_unstable();
        out
    }

    fn ball(&self, node: usize, q: &[f64; 3], r2: f64, out: &mut Vec<usize>) {
        match &self.nodes[node] {
            Node::Leaf { start, end } => {
                for s in *start..*end {
                    if d2(&self.pts[s], q) <= r2 {
                        out.push(self.ids[s]);
                    }
                }
            }
            Node::Split { axis, value, left, right } => {
                let diff = q[*axis] - *value;
                if diff <= 0.0 || diff * diff <= r2 {
                    self.ball(*left, q, r2, out);
                }
                if diff >= 0.0 || diff * diff <= r2 {
                    self.ball(*right, q, r2, out);
                }
            }
        }
    }
}

/// Builds the subtree for `order[start..end]`; returns its node index.
fn build(points: &[[f64; 3]], order: &mut [usize], start: usize, end: usize, nodes: &mut Vec<Node>) -> usize {
    let me = nodes.len();
    if end - start <= LEAF {
        nodes.push(Node::Leaf { start, end });
        return me;
    }
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for &i in &order[start..end] {
        for a in 0..3 {
            lo[a] = lo[a].min(points[i][a]);
            hi[a] = hi[a].max(points[i][a]);
        }
    }
    let mut axis = 0;
    for a in 1..3 {
        if hi[a] - lo[a] > hi[axis] - lo[axis] {
            axis = a;
        }
    }
    if !(hi[axis] > lo[axis]) {
        // All points identical (or NaN): one leaf.
        nodes.push(Node::Leaf { start, end });
        return me;
    }
    let mid = start + (end - start) / 2;
    order[start..end].select_nth_unstable_by(mid - start, |&a, &b| points[a][axis].total_cmp(&points[b][axis]));
    let value = points[order[mid]][axis];
    nodes.push(Node::Leaf { start, end }); // placeholder
    // Left holds points with coordinate <= value, right holds >= value.
    let left = build(points, order, start, mid, nodes);
    let right = build(points, order, mid, end, nodes);
    nodes[me] = Node::Split { axis, value, left, right };
    me
}

/// Nearest-point search over a very large point set that is produced on demand (the surface
/// samples Hollow uses, tens of millions on a fine mesh). The points are binned into a uniform
/// grid in two passes (count, then fill), so the set never exists as a plain list and building
/// it is fast on threads. Distances are the same exact `sqrt(min d²)` a [`PointTree`] gives.
pub struct PointGrid {
    lo: [f64; 3],
    inv: f64,
    cell: f64,
    n: [usize; 3],
    /// Start of each cell's points in `pts` (cells run along z first); one extra entry at the end.
    start: Vec<usize>,
    pts: Vec<[f64; 3]>,
    /// Points outside the grid bounds (rare: rounding at the very edge); checked one by one.
    strays: Vec<[f64; 3]>,
    /// Margin added to every cell's bounds: a point is binned with `floor((p - lo) * inv)`, which
    /// can disagree with `lo + i * cell` by a few ulps.
    slack: f64,
}

struct SharedOut(*mut [f64; 3]);
// Each slot is written by exactly one thread (slots come from an atomic cursor).
unsafe impl Send for SharedOut {}
unsafe impl Sync for SharedOut {}

impl PointGrid {
    /// `lo`/`hi` should bound every point (a point outside them is kept apart and checked on
    /// every query, so the answer stays exact). `emit_all` must call the given function once for
    /// every point, from any threads, and give the same set both times it is run. `expected` is
    /// roughly how many points that is. `None` when the cells or points would not fit in memory.
    pub fn build<E>(lo: [f64; 3], hi: [f64; 3], cell: f64, expected: usize, emit_all: E) -> Option<Self>
    where
        E: Fn(&(dyn Fn([f64; 3]) + Sync)),
    {
        use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
        let mut cell = cell.max(1e-9);
        let dims =
            |c: f64| -> [usize; 3] { std::array::from_fn(|a| (((hi[a] - lo[a]) / c).floor().max(0.0) as usize) + 1) };
        let mut n = dims(cell);
        // keep the cell count in step with the point count: a few points per cell is plenty, and
        // a sparse set (a plain box) must not pay for millions of empty cells
        let max_cells = (expected / 2).clamp(1 << 16, 32_000_000);
        while n[0].saturating_mul(n[1]).saturating_mul(n[2]) > max_cells {
            cell *= 1.5;
            n = dims(cell);
        }
        let inv = 1.0 / cell;
        let cells = n[0] * n[1] * n[2];
        // None for a point outside the grid; those are kept apart so a cell's bounds stay exact
        let id = move |p: &[f64; 3]| -> Option<usize> {
            let mut c = [0usize; 3];
            for a in 0..3 {
                let r = ((p[a] - lo[a]) * inv).floor();
                if !(r >= 0.0 && r < n[a] as f64) {
                    return None;
                }
                c[a] = r as usize;
            }
            Some((c[0] * n[1] + c[1]) * n[2] + c[2])
        };
        let mut counts: Vec<AtomicUsize> = Vec::new();
        counts.try_reserve_exact(cells + 1).ok()?;
        counts.extend((0..=cells).map(|_| AtomicUsize::new(0)));
        emit_all(&|p| {
            if let Some(i) = id(&p) {
                counts[i].fetch_add(1, Relaxed);
            }
        });
        let mut start: Vec<usize> = Vec::new();
        start.try_reserve_exact(cells + 1).ok()?;
        let mut total = 0usize;
        for c in &counts[..cells] {
            start.push(total);
            total += c.load(Relaxed);
        }
        start.push(total);
        let mut pts: Vec<[f64; 3]> = Vec::new();
        pts.try_reserve_exact(total).ok()?;
        let out = SharedOut(pts.as_mut_ptr());
        let out = &out;
        for (c, s) in counts.iter().zip(&start) {
            c.store(*s, Relaxed);
        }
        let strays: std::sync::Mutex<Vec<[f64; 3]>> = std::sync::Mutex::new(Vec::new());
        let stray_failed = std::sync::atomic::AtomicBool::new(false);
        emit_all(&|p| match id(&p) {
            Some(i) => {
                let slot = counts[i].fetch_add(1, Relaxed);
                // SAFETY: `slot` is below `total` (same point set as the count pass) and unique.
                unsafe { out.0.add(slot).write(p) };
            }
            None => {
                let mut v = strays.lock().unwrap();
                if v.try_reserve(1).is_ok() {
                    v.push(p);
                } else {
                    stray_failed.store(true, Relaxed);
                }
            }
        });
        if stray_failed.load(Relaxed) {
            return None; // `pts` is dropped with length 0, which is fine for plain numbers
        }
        // SAFETY: every one of the `total` slots was written exactly once.
        unsafe { pts.set_len(total) };
        let reach = (0..3).map(|a| lo[a].abs().max((lo[a] + n[a] as f64 * cell).abs())).fold(0.0, f64::max);
        let slack = cell * 1e-9 + 16.0 * f64::EPSILON * reach;
        Some(Self { lo, inv, cell, n, start, pts, strays: strays.into_inner().unwrap(), slack })
    }

    /// Distance from `q` to the closest point (infinity when there are none).
    pub fn nearest_dist(&self, q: [f64; 3]) -> f64 {
        if self.pts.is_empty() && self.strays.is_empty() {
            return f64::INFINITY;
        }
        let mut c = [0usize; 3];
        for a in 0..3 {
            let r = ((q[a] - self.lo[a]) * self.inv).floor();
            c[a] = if r > 0.0 { (r as usize).min(self.n[a] - 1) } else { 0 };
        }
        // Walk outwards ring by ring (cells at the same step count from q's own cell), skipping any
        // cell that cannot beat the best point so far. Cells further than r - 1 steps away are
        // more than (r - 1) * cell from q, so the walk ends when the best is closer than that.
        let maxr = self.n[0].max(self.n[1]).max(self.n[2]);
        let lower = |axis: usize, cell: usize| -> f64 {
            // distance along one axis from q to the slab of that cell (0 when q is inside it)
            let edge_lo = self.lo[axis] + cell as f64 * self.cell - self.slack;
            let edge_hi = self.lo[axis] + (cell + 1) as f64 * self.cell + self.slack;
            if q[axis] < edge_lo {
                edge_lo - q[axis]
            } else if q[axis] > edge_hi {
                q[axis] - edge_hi
            } else {
                0.0
            }
        };
        let mut best = f64::INFINITY;
        for p in &self.strays {
            best = best.min(d2(p, &q));
        }
        for r in 0..=maxr {
            if r >= 1 {
                let safe = ((r - 1) as f64 * self.cell - 2.0 * self.slack).max(0.0);
                if best <= safe * safe {
                    break;
                }
            }
            let (x0, x1) = (c[0].saturating_sub(r), (c[0] + r).min(self.n[0] - 1));
            let (y0, y1) = (c[1].saturating_sub(r), (c[1] + r).min(self.n[1] - 1));
            for x in x0..=x1 {
                let gx = lower(0, x);
                if gx * gx >= best {
                    continue;
                }
                for y in y0..=y1 {
                    let gy = lower(1, y);
                    let gxy = gx * gx + gy * gy;
                    if gxy >= best {
                        continue;
                    }
                    let base = (x * self.n[1] + y) * self.n[2];
                    let face = x.abs_diff(c[0]) == r || y.abs_diff(c[1]) == r;
                    let scan = |z0: usize, z1: usize, best: &mut f64| {
                        for p in &self.pts[self.start[base + z0]..self.start[base + z1 + 1]] {
                            let d = d2(p, &q);
                            if d < *best {
                                *best = d;
                            }
                        }
                    };
                    if face {
                        // whole column run; trim it to the cells a closer point could be in
                        let mut z0 = c[2].saturating_sub(r);
                        let mut z1 = (c[2] + r).min(self.n[2] - 1);
                        if best.is_finite() {
                            let zl = (best - gxy).sqrt() + 2.0 * self.slack;
                            let lo_z = ((q[2] - zl - self.lo[2]) * self.inv).floor();
                            let hi_z = ((q[2] + zl - self.lo[2]) * self.inv).floor();
                            if hi_z < z0 as f64 || lo_z > z1 as f64 {
                                continue;
                            }
                            if lo_z > z0 as f64 {
                                z0 = lo_z as usize;
                            }
                            if hi_z < z1 as f64 {
                                z1 = hi_z as usize;
                            }
                        }
                        scan(z0, z1, &mut best);
                    } else {
                        // inner column: only its two end cells belong to this ring
                        if r <= c[2] {
                            let z = c[2] - r;
                            let gz = lower(2, z);
                            if gxy + gz * gz < best {
                                scan(z, z, &mut best);
                            }
                        }
                        if r > 0 && c[2] + r < self.n[2] {
                            let z = c[2] + r;
                            let gz = lower(2, z);
                            if gxy + gz * gz < best {
                                scan(z, z, &mut best);
                            }
                        }
                    }
                }
            }
        }
        best.sqrt()
    }
}
