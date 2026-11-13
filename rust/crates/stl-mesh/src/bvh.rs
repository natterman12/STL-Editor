//! A bounding-volume tree over a triangle mesh.
//!
//! Pure geometry: no windowing or GPU types. The viewport builds one tree per mesh revision
//! and casts pick rays through it. [`ray`](Bvh::ray) is the closest hit from either side;
//! [`ray_front`](Bvh::ray_front) keeps the side three.js `FrontSide` keeps.

use stl_kernel::MeshArrays;

const LEAF: usize = 8;

struct Item {
    index: u32,
    min: [f32; 3],
    max: [f32; 3],
    centroid: [f32; 3],
}

struct Node {
    min: [f32; 3],
    max: [f32; 3],
    /// Leaf: first entry in [`Bvh::order`]. Inner: left child.
    a: u32,
    /// Leaf: triangle count. Inner: right child.
    b: u32,
    leaf: bool,
}

/// Median-split AABB tree. Triangle indices in a hit are the mesh's own triangle numbers.
pub struct Bvh {
    nodes: Vec<Node>,
    order: Vec<u32>,
}

impl Bvh {
    pub fn build(m: &MeshArrays) -> Self {
        let mut items = collect(m);
        if items.is_empty() {
            return Self { nodes: Vec::new(), order: Vec::new() };
        }
        let mut nodes = Vec::new();
        let mut order = Vec::with_capacity(items.len());
        build_range(&mut items, &mut nodes, &mut order);
        Self { nodes, order }
    }

    /// Closest intersection, either side of a triangle. `t` is the ray parameter
    /// (`origin + t * direction`), the same quantity three.js reports as `distance` when
    /// `direction` is a unit vector.
    pub fn ray(&self, m: &MeshArrays, origin: [f64; 3], direction: [f64; 3]) -> Option<(u32, f64)> {
        self.search(m, origin, direction, false, |_| true)
    }

    /// Closest front face: the winding three.js keeps for `side: FrontSide`.
    pub fn ray_front(&self, m: &MeshArrays, origin: [f64; 3], direction: [f64; 3]) -> Option<(u32, f64)> {
        self.ray_front_where(m, origin, direction, |_| true)
    }

    /// [`ray_front`](Self::ray_front), dropping hits `accept` rejects. A nearer rejected hit
    /// does not hide a farther accepted one (the section cut works this way).
    pub fn ray_front_where(
        &self,
        m: &MeshArrays,
        origin: [f64; 3],
        direction: [f64; 3],
        accept: impl Fn([f64; 3]) -> bool,
    ) -> Option<(u32, f64)> {
        self.search(m, origin, direction, true, accept)
    }

    fn search(
        &self,
        m: &MeshArrays,
        origin: [f64; 3],
        direction: [f64; 3],
        front_only: bool,
        accept: impl Fn([f64; 3]) -> bool,
    ) -> Option<(u32, f64)> {
        if self.nodes.is_empty() || dot(direction, direction) == 0.0 {
            return None;
        }
        let mut best_t = f64::INFINITY;
        let mut best_i = u32::MAX;
        let mut stack = Vec::with_capacity(64);
        stack.push(0u32);
        while let Some(ni) = stack.pop() {
            let node = &self.nodes[ni as usize];
            if aabb_entry(origin, direction, node, best_t).is_none() {
                continue;
            }
            if node.leaf {
                let start = node.a as usize;
                let end = start + node.b as usize;
                for &tri in &self.order[start..end] {
                    let Some([a, b, c]) = tri_points(m, tri) else { continue };
                    let Some(t) = ray_triangle(origin, direction, a, b, c, front_only) else { continue };
                    if !(t < best_t || (t == best_t && tri < best_i)) {
                        continue;
                    }
                    let p = [origin[0] + t * direction[0], origin[1] + t * direction[1], origin[2] + t * direction[2]];
                    if !accept(p) {
                        continue;
                    }
                    best_t = t;
                    best_i = tri;
                }
            } else {
                let left = node.a;
                let right = node.b;
                let tl = aabb_entry(origin, direction, &self.nodes[left as usize], best_t);
                let tr = aabb_entry(origin, direction, &self.nodes[right as usize], best_t);
                match (tl, tr) {
                    (Some(l), Some(r)) if l > r => {
                        stack.push(left);
                        stack.push(right);
                    }
                    (Some(_), Some(_)) => {
                        stack.push(right);
                        stack.push(left);
                    }
                    (Some(_), None) => stack.push(left),
                    (None, Some(_)) => stack.push(right),
                    (None, None) => {}
                }
            }
        }
        (best_i != u32::MAX).then_some((best_i, best_t))
    }
}

/// [`Bvh::ray_front_where`] without a tree: every triangle is tested. Same hit rules (front
/// faces only, `accept` filter, the lower triangle index wins a tie), for a mesh whose tree is
/// not built yet.
pub fn ray_front_direct(
    m: &MeshArrays,
    origin: [f64; 3],
    direction: [f64; 3],
    accept: impl Fn([f64; 3]) -> bool,
) -> Option<(u32, f64)> {
    direct(m, origin, direction, true, accept)
}

fn direct(
    m: &MeshArrays,
    origin: [f64; 3],
    direction: [f64; 3],
    front_only: bool,
    accept: impl Fn([f64; 3]) -> bool,
) -> Option<(u32, f64)> {
    if dot(direction, direction) == 0.0 {
        return None;
    }
    let mut best_t = f64::INFINITY;
    let mut best_i = u32::MAX;
    for tri in 0..(m.triangles.len() / 3) as u32 {
        let Some([a, b, c]) = tri_points(m, tri) else { continue };
        if !a.iter().chain(b.iter()).chain(c.iter()).all(|v| v.is_finite()) {
            continue;
        }
        let Some(t) = ray_triangle(origin, direction, a, b, c, front_only) else { continue };
        if !(t < best_t) {
            continue;
        }
        let p = [origin[0] + t * direction[0], origin[1] + t * direction[1], origin[2] + t * direction[2]];
        if !accept(p) {
            continue;
        }
        best_t = t;
        best_i = tri;
    }
    (best_i != u32::MAX).then_some((best_i, best_t))
}

/// three.js `Ray.intersectTriangle`. `front_only` is `backfaceCulling` (`FrontSide`).
/// Returns the ray parameter, or `None` when the ray misses or starts behind the triangle.
pub fn ray_triangle(
    origin: [f64; 3],
    direction: [f64; 3],
    a: [f64; 3],
    b: [f64; 3],
    c: [f64; 3],
    front_only: bool,
) -> Option<f64> {
    let e1 = sub(b, a);
    let e2 = sub(c, a);
    let normal = cross(e1, e2);
    let mut ddn = dot(direction, normal);
    let sign = if ddn > 0.0 {
        if front_only {
            return None;
        }
        1.0
    } else if ddn < 0.0 {
        ddn = -ddn;
        -1.0
    } else {
        return None;
    };
    let diff = sub(origin, a);
    let ddqxe2 = sign * dot(direction, cross(diff, e2));
    if ddqxe2 < 0.0 {
        return None;
    }
    let dde1xq = sign * dot(direction, cross(e1, diff));
    if dde1xq < 0.0 {
        return None;
    }
    if ddqxe2 + dde1xq > ddn {
        return None;
    }
    let qdn = -sign * dot(diff, normal);
    if qdn < 0.0 {
        return None;
    }
    Some(qdn / ddn)
}

fn collect(m: &MeshArrays) -> Vec<Item> {
    let ntri = m.triangles.len() / 3;
    let nv = m.positions.len() / 3;
    let mut out = Vec::with_capacity(ntri);
    for i in 0..ntri {
        let ia = m.triangles[i * 3] as usize;
        let ib = m.triangles[i * 3 + 1] as usize;
        let ic = m.triangles[i * 3 + 2] as usize;
        if ia >= nv || ib >= nv || ic >= nv {
            continue;
        }
        let a = corner(m, ia);
        let b = corner(m, ib);
        let c = corner(m, ic);
        if !a.iter().chain(b.iter()).chain(c.iter()).all(|v| v.is_finite()) {
            continue;
        }
        out.push(Item {
            index: i as u32,
            min: [a[0].min(b[0]).min(c[0]), a[1].min(b[1]).min(c[1]), a[2].min(b[2]).min(c[2])],
            max: [a[0].max(b[0]).max(c[0]), a[1].max(b[1]).max(c[1]), a[2].max(b[2]).max(c[2])],
            centroid: [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0, (a[2] + b[2] + c[2]) / 3.0],
        });
    }
    out
}

fn corner(m: &MeshArrays, i: usize) -> [f32; 3] {
    let s = i * 3;
    [m.positions[s], m.positions[s + 1], m.positions[s + 2]]
}

/// The three corners of triangle `tri`, in f64. `None` when the triangle or one of its vertex
/// indices is out of range.
pub fn tri_points(m: &MeshArrays, tri: u32) -> Option<[[f64; 3]; 3]> {
    let base = tri as usize * 3;
    if base + 2 >= m.triangles.len() {
        return None;
    }
    let nv = m.positions.len() / 3;
    let mut out = [[0.0; 3]; 3];
    for k in 0..3 {
        let i = m.triangles[base + k] as usize;
        if i >= nv {
            return None;
        }
        let s = i * 3;
        out[k] = [m.positions[s] as f64, m.positions[s + 1] as f64, m.positions[s + 2] as f64];
    }
    Some(out)
}

fn build_range(tris: &mut [Item], nodes: &mut Vec<Node>, order: &mut Vec<u32>) -> u32 {
    let index = nodes.len() as u32;
    nodes.push(Node { min: [0.0; 3], max: [0.0; 3], a: 0, b: 0, leaf: true });
    let (tight_min, tight_max) = tight_bounds(tris);
    let (min, max) = pad(tight_min, tight_max);
    if tris.len() <= LEAF {
        let start = order.len() as u32;
        for t in tris.iter() {
            order.push(t.index);
        }
        nodes[index as usize] = Node { min, max, a: start, b: tris.len() as u32, leaf: true };
        return index;
    }
    let axis = longest(tight_min, tight_max);
    let mid = tris.len() / 2;
    tris.select_nth_unstable_by(mid, |p, q| p.centroid[axis].total_cmp(&q.centroid[axis]));
    let (left, right) = tris.split_at_mut(mid);
    let l = build_range(left, nodes, order);
    let r = build_range(right, nodes, order);
    nodes[index as usize] = Node { min, max, a: l, b: r, leaf: false };
    index
}

fn tight_bounds(tris: &[Item]) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for t in tris {
        for a in 0..3 {
            min[a] = min[a].min(t.min[a]);
            max[a] = max[a].max(t.max[a]);
        }
    }
    (min, max)
}

/// A little slack so an f64 hit on an f32 vertex is not rejected by the box test.
fn pad(min: [f32; 3], max: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let mut lo = min;
    let mut hi = max;
    for a in 0..3 {
        let e = (max[a] - min[a]).abs().max(1.0) * 1.0e-5 + 1.0e-4;
        lo[a] -= e;
        hi[a] += e;
    }
    (lo, hi)
}

fn longest(min: [f32; 3], max: [f32; 3]) -> usize {
    let mut axis = 0;
    let mut best = max[0] - min[0];
    for a in 1..3 {
        let e = max[a] - min[a];
        if e > best {
            best = e;
            axis = a;
        }
    }
    axis
}

fn aabb_entry(origin: [f64; 3], dir: [f64; 3], node: &Node, best_t: f64) -> Option<f64> {
    let mut tmin = 0.0;
    let mut tmax = best_t;
    for a in 0..3 {
        let mn = node.min[a] as f64;
        let mx = node.max[a] as f64;
        if dir[a].abs() <= 1e-18 {
            if origin[a] < mn || origin[a] > mx {
                return None;
            }
            continue;
        }
        let inv = 1.0 / dir[a];
        let (mut t1, mut t2) = ((mn - origin[a]) * inv, (mx - origin[a]) * inv);
        if t1 > t2 {
            std::mem::swap(&mut t1, &mut t2);
        }
        if t1 > tmin {
            tmin = t1;
        }
        if t2 < tmax {
            tmax = t2;
        }
        if tmin > tmax {
            return None;
        }
    }
    Some(tmin)
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tri(verts: [[f32; 3]; 3]) -> MeshArrays {
        MeshArrays { positions: verts.into_iter().flatten().collect(), triangles: vec![0, 1, 2] }
    }

    fn brute(m: &MeshArrays, origin: [f64; 3], dir: [f64; 3], front: bool) -> Option<(u32, f64)> {
        direct(m, origin, dir, front, |_| true)
    }

    #[test]
    fn one_triangle_front_and_back() {
        // CCW from +Z. A ray 5 mm above the centre runs straight down onto it.
        let m = tri([[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]]);
        let bvh = Bvh::build(&m);
        let (face, t) = bvh.ray(&m, [0.5, 0.5, 5.0], [0.0, 0.0, -1.0]).unwrap();
        assert_eq!(face, 0);
        assert!((t - 5.0).abs() < 1e-9, "{t}");
        assert!(bvh.ray_front(&m, [0.5, 0.5, 5.0], [0.0, 0.0, -1.0]).is_some());
        // the same triangle from below is a back face
        assert!(bvh.ray(&m, [0.5, 0.5, -5.0], [0.0, 0.0, 1.0]).is_some());
        assert!(bvh.ray_front(&m, [0.5, 0.5, -5.0], [0.0, 0.0, 1.0]).is_none());
        assert!(bvh.ray(&m, [3.0, 3.0, 5.0], [0.0, 0.0, -1.0]).is_none());
        assert!(bvh.ray(&m, [0.5, 0.5, 5.0], [0.0, 0.0, 1.0]).is_none());
    }

    #[test]
    fn closer_triangle_wins_and_a_filter_can_skip_it() {
        let m = MeshArrays {
            positions: vec![0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0, 2.0, 0.0, 2.0, 0.0, 2.0, 2.0],
            triangles: vec![0, 1, 2, 3, 4, 5],
        };
        let bvh = Bvh::build(&m);
        let (face, t) = bvh.ray(&m, [0.4, 0.4, 5.0], [0.0, 0.0, -1.0]).unwrap();
        assert_eq!(face, 1);
        assert!((t - 3.0).abs() < 1e-9, "{t}");
        // reject the z = 2 sheet; the z = 0 sheet is the next hit
        let (face, t) = bvh.ray_front_where(&m, [0.4, 0.4, 5.0], [0.0, 0.0, -1.0], |p| p[2] < 1.0).unwrap();
        assert_eq!(face, 0);
        assert!((t - 5.0).abs() < 1e-9, "{t}");
    }

    #[test]
    fn empty_and_broken_meshes_miss() {
        let empty = MeshArrays::default();
        assert!(Bvh::build(&empty).ray(&empty, [0.0; 3], [0.0, 0.0, -1.0]).is_none());
        let broken = MeshArrays { positions: vec![0.0, 0.0, 0.0], triangles: vec![0, 1, 2] };
        assert!(Bvh::build(&broken).ray(&broken, [0.0, 0.0, 1.0], [0.0, 0.0, -1.0]).is_none());
    }

    #[test]
    fn ray_matches_every_triangle() {
        let mut seed = 0x5a17_u64;
        let mut rnd = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((seed >> 11) as f64) * (1.0 / ((1u64 << 53) as f64))
        };
        let mut positions = Vec::new();
        let mut triangles = Vec::new();
        for i in 0..80u32 {
            for _ in 0..3 {
                positions.push((rnd() * 4.0 - 2.0) as f32);
                positions.push((rnd() * 4.0 - 2.0) as f32);
                positions.push((rnd() * 4.0 - 2.0) as f32);
            }
            triangles.extend_from_slice(&[i * 3, i * 3 + 1, i * 3 + 2]);
        }
        let m = MeshArrays { positions, triangles };
        let bvh = Bvh::build(&m);
        let mut hits = 0u32;
        for _ in 0..400 {
            let origin = [rnd() * 8.0 - 4.0, rnd() * 8.0 - 4.0, rnd() * 8.0 - 4.0];
            let target = [rnd() * 2.0 - 1.0, rnd() * 2.0 - 1.0, rnd() * 2.0 - 1.0];
            let d = sub(target, origin);
            let len = dot(d, d).sqrt();
            if len < 1e-8 {
                continue;
            }
            let dir = [d[0] / len, d[1] / len, d[2] / len];
            let got = bvh.ray(&m, origin, dir);
            let expect = brute(&m, origin, dir, false);
            assert_eq!(got.map(|h| h.0), expect.map(|h| h.0));
            if let (Some((_, t)), Some((_, u))) = (got, expect) {
                assert!((t - u).abs() < 1e-9, "{t} vs {u}");
            }
            let got_f = bvh.ray_front(&m, origin, dir);
            assert_eq!(got_f.map(|h| h.0), ray_front_direct(&m, origin, dir, |_| true).map(|h| h.0));
            // a filter that drops the upper half: the tree and the direct test still agree
            let lower = |p: [f64; 3]| p[2] < 0.0;
            assert_eq!(bvh.ray_front_where(&m, origin, dir, lower), ray_front_direct(&m, origin, dir, lower));
            if got.is_some() {
                hits += 1;
            }
        }
        assert!(hits > 30, "only {hits} rays hit; the comparison did not exercise the tree");
    }
}
