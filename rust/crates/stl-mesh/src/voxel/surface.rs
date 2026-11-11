//! Barycentric surface-point sampling (ported from 1.x `stleditor/geometry.py` `_surface_points`).

const MAX_K: usize = 400;

#[inline]
fn edge_len(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// 1.x `_surface_points`: points covering every triangle no further than `spacing` apart,
/// emitted in the exact same order as 1.x (`verts` first, then triangles grouped by ascending
/// `kk in np.unique(k)` in original face order).
pub fn surface_points(verts: &[[f64; 3]], faces: &[[u32; 3]], spacing: f64) -> Vec<[f64; 3]> {
    let nv = verts.len();
    if faces.is_empty() {
        return verts.to_vec();
    }

    let mut counts = [0usize; MAX_K + 1];
    let mut face_k = Vec::with_capacity(faces.len());
    for f in faces {
        let (i0, i1, i2) = (f[0] as usize, f[1] as usize, f[2] as usize);
        if i0 >= nv || i1 >= nv || i2 >= nv {
            face_k.push(0u16);
            continue;
        }
        let (t0, t1, t2) = (verts[i0], verts[i1], verts[i2]);
        let edge = edge_len(t1, t0).max(edge_len(t2, t1)).max(edge_len(t0, t2));
        let raw = (edge / spacing).ceil();
        let kk = if raw.is_finite() {
            (raw as i64).clamp(1, MAX_K as i64) as usize
        } else {
            1
        };
        face_k.push(kk as u16);
        counts[kk] += 1;
    }

    let mut total_pts = nv + counts[1];
    for kk in 2..=MAX_K {
        if counts[kk] > 0 {
            let samples = (kk + 1) * (kk + 2) / 2;
            total_pts += counts[kk] * samples;
        }
    }

    // Group faces by kk in 1..=400 while preserving original face order within each group.
    let mut offsets = [0usize; MAX_K + 2];
    for kk in 1..=MAX_K {
        offsets[kk + 1] = offsets[kk] + counts[kk];
    }
    let mut write_pos = offsets;
    let mut grouped_faces = vec![0usize; offsets[MAX_K + 1]];
    for (fi, &kk_u16) in face_k.iter().enumerate() {
        let kk = kk_u16 as usize;
        if kk >= 1 {
            let pos = write_pos[kk];
            grouped_faces[pos] = fi;
            write_pos[kk] = pos + 1;
        }
    }

    let mut out = Vec::with_capacity(total_pts);
    out.extend_from_slice(verts);

    // kk == 1: centroid per triangle (`sel.mean(axis=1)`).
    for &fi in &grouped_faces[offsets[1]..offsets[2]] {
        let f = faces[fi];
        let (t0, t1, t2) = (verts[f[0] as usize], verts[f[1] as usize], verts[f[2] as usize]);
        out.push([
            (t0[0] + t1[0] + t2[0]) / 3.0,
            (t0[1] + t1[1] + t2[1]) / 3.0,
            (t0[2] + t1[2] + t2[2]) / 3.0,
        ]);
    }

    // kk >= 2: barycentric grid in `indexing="ij"` `(i + j) <= kk` order.
    let mut bary: Vec<[f64; 3]> = Vec::new();
    for kk in 2..=MAX_K {
        let start = offsets[kk];
        let end = offsets[kk + 1];
        if start == end {
            continue;
        }
        bary.clear();
        bary.reserve((kk + 1) * (kk + 2) / 2);
        let denom = kk as f64;
        for ii in 0..=kk {
            let a = (ii as f64) / denom;
            for jj in 0..=(kk - ii) {
                let bb = (jj as f64) / denom;
                let w0 = 1.0 - a - bb;
                bary.push([w0, a, bb]);
            }
        }
        for &fi in &grouped_faces[start..end] {
            let f = faces[fi];
            let (t0, t1, t2) = (verts[f[0] as usize], verts[f[1] as usize], verts[f[2] as usize]);
            for &[w0, a, bb] in &bary {
                out.push([
                    t0[0] * w0 + t1[0] * a + t2[0] * bb,
                    t0[1] * w0 + t1[1] * a + t2[1] * bb,
                    t0[2] * w0 + t1[2] * a + t2[2] * bb,
                ]);
            }
        }
    }

    out
}

/// The same sample points as [`surface_points`], produced one triangle at a time instead of
/// all at once (only the order differs). Hollow bins them straight into a grid, so the many
/// millions of points on a fine mesh never exist as one big list.
pub struct SurfaceSampler<'a> {
    verts: &'a [[f64; 3]],
    faces: &'a [[u32; 3]],
    face_k: Vec<u16>,
    /// `i / kk` for i in 0..=kk, for each subdivision count that some face uses (index = kk):
    /// the barycentric weights of a sample are built from these on the fly (a full table of
    /// weight triples for every count would be hundreds of MB).
    steps: Vec<Vec<f64>>,
}

impl<'a> SurfaceSampler<'a> {
    pub fn new(verts: &'a [[f64; 3]], faces: &'a [[u32; 3]], spacing: f64) -> Self {
        let nv = verts.len();
        let mut face_k = Vec::with_capacity(faces.len());
        let mut used = vec![false; MAX_K + 1];
        for f in faces {
            let (i0, i1, i2) = (f[0] as usize, f[1] as usize, f[2] as usize);
            if i0 >= nv || i1 >= nv || i2 >= nv {
                face_k.push(0u16);
                continue;
            }
            let (t0, t1, t2) = (verts[i0], verts[i1], verts[i2]);
            let edge = edge_len(t1, t0).max(edge_len(t2, t1)).max(edge_len(t0, t2));
            let raw = (edge / spacing).ceil();
            let kk = if raw.is_finite() { (raw as i64).clamp(1, MAX_K as i64) as usize } else { 1 };
            face_k.push(kk as u16);
            used[kk] = true;
        }
        let mut steps: Vec<Vec<f64>> = vec![Vec::new(); MAX_K + 1];
        for kk in 2..=MAX_K {
            if used[kk] {
                let denom = kk as f64;
                steps[kk] = (0..=kk).map(|i| (i as f64) / denom).collect();
            }
        }
        Self { verts, faces, face_k, steps }
    }

    /// How many points the sampler produces in all.
    pub fn point_count(&self) -> usize {
        let per_face: usize = self.face_k.iter().map(|&k| match k {
            0 => 0,
            1 => 1,
            k => (k as usize + 1) * (k as usize + 2) / 2,
        }).sum();
        self.verts.len() + per_face
    }

    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    /// Calls `f` with every vertex of the mesh.
    pub fn each_vertex(&self, mut f: impl FnMut([f64; 3])) {
        for &v in self.verts {
            f(v);
        }
    }

    /// Calls `f` with the sample points of triangle `fi` (none for a triangle with a bad index).
    pub fn each_face_point(&self, fi: usize, mut f: impl FnMut([f64; 3])) {
        let kk = self.face_k[fi] as usize;
        if kk == 0 {
            return;
        }
        let tri = self.faces[fi];
        let (t0, t1, t2) = (self.verts[tri[0] as usize], self.verts[tri[1] as usize], self.verts[tri[2] as usize]);
        if kk == 1 {
            f([(t0[0] + t1[0] + t2[0]) / 3.0, (t0[1] + t1[1] + t2[1]) / 3.0, (t0[2] + t1[2] + t2[2]) / 3.0]);
            return;
        }
        let st = &self.steps[kk];
        for ii in 0..=kk {
            let a = st[ii];
            for jj in 0..=(kk - ii) {
                let bb = st[jj];
                let w0 = 1.0 - a - bb;
                f([
                    t0[0] * w0 + t1[0] * a + t2[0] * bb,
                    t0[1] * w0 + t1[1] * a + t2[1] * bb,
                    t0[2] * w0 + t1[2] * a + t2[2] * bb,
                ]);
            }
        }
    }
}

const LEAF_TRIS: usize = 4;
const BOX_PAD: f64 = 1e-7;

#[inline]
fn d2(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}

#[inline]
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

struct BNode {
    lo: [f64; 3],
    hi: [f64; 3],
    /// Leaf (`left == u32::MAX`): `a..b` is its range of `order`. Otherwise `left` and `a` are the
    /// two child nodes.
    a: u32,
    b: u32,
    left: u32,
}

/// Exact nearest-sample search that never holds the samples: a bounding-box tree over the
/// triangles and, for each triangle near the query, only the lattice points that could be the
/// closest are worked out (the lattice is the one [`SurfaceSampler`] uses). The answer is the
/// same `sqrt(min d^2)` over the same sample values as a [`crate::nearest::PointTree`] over
/// [`surface_points`]; memory is a few tens of bytes per triangle instead of 24 per sample.
pub struct SampleIndex<'a> {
    sampler: &'a SurfaceSampler<'a>,
    nodes: Vec<BNode>,
    order: Vec<u32>,
    /// All vertices (they are samples too, whether or not a valid triangle uses them).
    vtree: crate::nearest::PointTree,
}

impl<'a> SampleIndex<'a> {
    /// `None` when the index would not fit in memory.
    pub fn new(sampler: &'a SurfaceSampler<'a>) -> Option<Self> {
        let verts = sampler.verts;
        let nf = sampler.faces.len();
        let mut face_box: Vec<([f64; 3], [f64; 3])> = Vec::new();
        face_box.try_reserve_exact(nf).ok()?;
        let mut order: Vec<u32> = Vec::new();
        order.try_reserve_exact(nf).ok()?;
        for (fi, tri) in sampler.faces.iter().enumerate() {
            if sampler.face_k[fi] == 0 {
                face_box.push(([0.0; 3], [0.0; 3]));
                continue;
            }
            let (a, b, c) = (verts[tri[0] as usize], verts[tri[1] as usize], verts[tri[2] as usize]);
            let lo = std::array::from_fn(|k| a[k].min(b[k]).min(c[k]) - BOX_PAD);
            let hi = std::array::from_fn(|k| a[k].max(b[k]).max(c[k]) + BOX_PAD);
            face_box.push((lo, hi));
            order.push(fi as u32);
        }
        let mut nodes: Vec<BNode> = Vec::new();
        let n = order.len();
        nodes.try_reserve(n / 2 + 1).ok()?;
        if n > 0 {
            build_bvh(&face_box, &mut order, 0, n, &mut nodes)?;
        }
        let vtree = crate::nearest::PointTree::new(verts);
        Some(Self { sampler, nodes, order, vtree })
    }

    /// Distance from `q` to the closest sample (infinity when there are none).
    pub fn nearest_dist(&self, q: [f64; 3]) -> f64 {
        let vd = self.vtree.nearest_dist(q);
        // a slightly generous bound for pruning; `best` only drops below it for a real sample
        let mut best = if vd.is_finite() { vd * vd * (1.0 + 1e-12) } else { f64::INFINITY };
        let mut stack: Vec<u32> = Vec::with_capacity(64);
        if !self.nodes.is_empty() {
            stack.push(0);
        }
        while let Some(ni) = stack.pop() {
            let node = &self.nodes[ni as usize];
            if box_d2(node, &q) >= best {
                continue;
            }
            if node.left == u32::MAX {
                for &fi in &self.order[node.a as usize..node.b as usize] {
                    self.tri_min(fi as usize, &q, &mut best);
                }
            } else {
                let (l, r) = (&self.nodes[node.left as usize], &self.nodes[node.a as usize]);
                // the nearer child is visited first
                if box_d2(l, &q) <= box_d2(r, &q) {
                    stack.push(node.a);
                    stack.push(node.left);
                } else {
                    stack.push(node.left);
                    stack.push(node.a);
                }
            }
        }
        vd.min(best.sqrt())
    }

    /// Lowers `best` (a squared distance) to the closest sample of triangle `fi` if that is closer.
    fn tri_min(&self, fi: usize, q: &[f64; 3], best: &mut f64) {
        let sm = self.sampler;
        let kk = sm.face_k[fi] as usize;
        if kk == 0 {
            return;
        }
        let tri = sm.faces[fi];
        let (t0, t1, t2) = (sm.verts[tri[0] as usize], sm.verts[tri[1] as usize], sm.verts[tri[2] as usize]);
        if kk == 1 {
            let p = [(t0[0] + t1[0] + t2[0]) / 3.0, (t0[1] + t1[1] + t2[1]) / 3.0, (t0[2] + t1[2] + t2[2]) / 3.0];
            let d = d2(&p, q);
            if d < *best {
                *best = d;
            }
            return;
        }
        let st = &sm.steps[kk];
        // exactly the arithmetic `each_face_point` uses
        let sample = |ii: usize, jj: usize| -> f64 {
            let (a, bb) = (st[ii], st[jj]);
            let w0 = 1.0 - a - bb;
            d2(
                &[
                    t0[0] * w0 + t1[0] * a + t2[0] * bb,
                    t0[1] * w0 + t1[1] * a + t2[1] * bb,
                    t0[2] * w0 + t1[2] * a + t2[2] * bb,
                ],
                q,
            )
        };
        let k = kk as f64;
        let e1 = [(t1[0] - t0[0]) / k, (t1[1] - t0[1]) / k, (t1[2] - t0[2]) / k];
        let e2 = [(t2[0] - t0[0]) / k, (t2[1] - t0[1]) / k, (t2[2] - t0[2]) / k];
        let q0 = [q[0] - t0[0], q[1] - t0[1], q[2] - t0[2]];
        let g22 = dot(e2, e2);
        if !(g22 > 0.0 && g22.is_finite()) {
            // a collapsed edge: no useful geometry to prune with, try every sample
            for ii in 0..=kk {
                for jj in 0..=(kk - ii) {
                    let d = sample(ii, jj);
                    if d < *best {
                        *best = d;
                    }
                }
            }
            return;
        }
        // A sample is at the real-number point t0 + ii*e1 + jj*e2; its float value differs from that
        // by a few ulps of the coordinates, so everything within `tol` of the best must be tried.
        let scale = 1.0 + dot(*q, *q) + dot(t0, t0) + dot(t1, t1) + dot(t2, t2);
        let tol = 1e-12 * scale;
        // The distance from q to lattice row ii, minimised along the row, is a convex quadratic
        // in ii: |u - ii*v|^2, with u and v the parts of q0 and e1 across e2.
        let c0 = dot(q0, e2) / g22;
        let c1 = dot(e1, e2) / g22;
        let u = [q0[0] - c0 * e2[0], q0[1] - c0 * e2[1], q0[2] - c0 * e2[2]];
        let v = [e1[0] - c1 * e2[0], e1[1] - c1 * e2[1], e1[2] - c1 * e2[2]];
        let vv = dot(v, v);
        let (mut i_lo, mut i_hi) = (0usize, kk);
        if vv > 0.0 && vv.is_finite() {
            let i0 = dot(u, v) / vv;
            let r = [u[0] - i0 * v[0], u[1] - i0 * v[1], u[2] - i0 * v[2]];
            if !best.is_finite() {
                // no bound yet: start from the lattice point nearest the projection of q
                let ii = i0.round().clamp(0.0, k) as usize;
                let rem = [q0[0] - ii as f64 * e1[0], q0[1] - ii as f64 * e1[1], q0[2] - ii as f64 * e1[2]];
                let jj = (dot(rem, e2) / g22).round().clamp(0.0, (kk - ii) as f64) as usize;
                *best = best.min(sample(ii, jj));
            }
            let rad2 = *best + tol - dot(r, r);
            if rad2 < 0.0 {
                return;
            }
            let w = (rad2 / vv).sqrt() + 2.0;
            i_lo = (i0 - w).ceil().max(0.0) as usize;
            i_hi = (i0 + w).floor().min(k).max(0.0) as usize;
        } else if !best.is_finite() {
            *best = best.min(sample(0, 0));
        }
        for ii in i_lo..=i_hi {
            let fii = ii as f64;
            let qr = [q0[0] - fii * e1[0], q0[1] - fii * e1[1], q0[2] - fii * e1[2]];
            let cx = [
                qr[1] * e2[2] - qr[2] * e2[1],
                qr[2] * e2[0] - qr[0] * e2[2],
                qr[0] * e2[1] - qr[1] * e2[0],
            ];
            let rad2 = *best + tol - dot(cx, cx) / g22;
            if rad2 < 0.0 {
                continue;
            }
            let jopt = dot(qr, e2) / g22;
            let w = (rad2 / g22).sqrt() + 2.0;
            let j0 = (jopt - w).ceil().max(0.0) as usize;
            let j1 = (jopt + w).floor().min((kk - ii) as f64).max(0.0) as usize;
            for jj in j0..=j1 {
                let d = sample(ii, jj);
                if d < *best {
                    *best = d;
                }
            }
        }
    }
}

fn box_d2(n: &BNode, q: &[f64; 3]) -> f64 {
    let mut s = 0.0;
    for a in 0..3 {
        let d = (n.lo[a] - q[a]).max(q[a] - n.hi[a]).max(0.0);
        s += d * d;
    }
    s
}

/// Builds the subtree over `order[start..end]`; returns its node index (`None` on out of memory).
fn build_bvh(
    boxes: &[([f64; 3], [f64; 3])],
    order: &mut [u32],
    start: usize,
    end: usize,
    nodes: &mut Vec<BNode>,
) -> Option<u32> {
    let me = nodes.len();
    nodes.try_reserve(1).ok()?;
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    let mut clo = [f64::INFINITY; 3];
    let mut chi = [f64::NEG_INFINITY; 3];
    for &fi in &order[start..end] {
        let (l, h) = &boxes[fi as usize];
        for a in 0..3 {
            lo[a] = lo[a].min(l[a]);
            hi[a] = hi[a].max(h[a]);
            let c = 0.5 * (l[a] + h[a]);
            clo[a] = clo[a].min(c);
            chi[a] = chi[a].max(c);
        }
    }
    if end - start <= LEAF_TRIS || !(0..3).any(|a| chi[a] > clo[a]) {
        nodes.push(BNode { lo, hi, a: start as u32, b: end as u32, left: u32::MAX });
        return Some(me as u32);
    }
    let axis = (0..3).fold(0, |b, a| if chi[a] - clo[a] > chi[b] - clo[b] { a } else { b });
    let mid = start + (end - start) / 2;
    order[start..end].select_nth_unstable_by(mid - start, |&p, &q| {
        let (bp, bq) = (&boxes[p as usize], &boxes[q as usize]);
        (bp.0[axis] + bp.1[axis]).total_cmp(&(bq.0[axis] + bq.1[axis]))
    });
    nodes.push(BNode { lo, hi, a: 0, b: 0, left: 0 });
    let left = build_bvh(boxes, order, start, mid, nodes)?;
    let right = build_bvh(boxes, order, mid, end, nodes)?;
    nodes[me].left = left;
    nodes[me].a = right;
    Some(me as u32)
}
