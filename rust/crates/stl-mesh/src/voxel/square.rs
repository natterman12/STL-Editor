//! Square inside corners for Shell and Hollow (spec 2.7): the voxel hollow's inner surface is
//! snapped onto the part's flat faces moved inward by the wall thickness.
//!
//! The marching cubes surface cuts every crease and corner of the cavity by up to a voxel.
//! A CAD shell is the part's flat faces offset inward and meeting in exact edges and corners,
//! so each inner vertex that lies within one voxel of such an offset plane is moved onto it; a
//! vertex within a voxel of two planes goes onto their line, of three onto their corner.
//! Curved areas have no big flat face behind them and are left as they are.

use stl_kernel::MeshArrays;

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}
fn axpy(a: V3, k: f64, b: V3) -> V3 {
    [a[0] + k * b[0], a[1] + k * b[1], a[2] + k * b[2]]
}

/// One flat region of the outer surface: connected triangles on one plane `n . x = d`, `n` outward.
struct Region {
    n: V3,
    d: f64,
    tris: Vec<[V3; 3]>,
    lo: Vec<V3>,
    hi: Vec<V3>,
    lo_all: V3,
    hi_all: V3,
}

fn read_points(m: &MeshArrays) -> Vec<V3> {
    m.positions
        .chunks_exact(3)
        .map(|p| [p[0] as f64, p[1] as f64, p[2] as f64])
        .collect()
}

/// Distance from `p` to triangle `abc` (Ericson, Real-Time Collision Detection).
fn point_triangle(p: V3, a: V3, b: V3, c: V3) -> f64 {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return norm(ap);
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return norm(bp);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return norm(sub(p, axpy(a, d1 / (d1 - d3), ab)));
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return norm(cp);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return norm(sub(p, axpy(a, d2 / (d2 - d6), ac)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return norm(sub(p, axpy(b, w, sub(c, b))));
    }
    let den = 1.0 / (va + vb + vc);
    let q = axpy(axpy(a, vb * den, ab), vc * den, ac);
    norm(sub(p, q))
}

/// Coplanar, edge-connected triangle groups of the outer surface that cover at least
/// `min_area`. Deterministic: seeds and neighbours are visited in triangle order.
fn flat_regions(pts: &[V3], tris: &[[u32; 3]], pitch: f64, min_area: f64) -> Vec<Region> {
    let nt = tris.len();
    let mut normal: Vec<V3> = Vec::with_capacity(nt);
    let mut area: Vec<f64> = Vec::with_capacity(nt);
    for t in tris {
        let (a, b, c) = (pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]);
        let n = cross(sub(b, a), sub(c, a));
        let l = norm(n);
        area.push(0.5 * l);
        normal.push(if l > 0.0 {
            [n[0] / l, n[1] / l, n[2] / l]
        } else {
            [0.0; 3]
        });
    }
    // triangles sharing an edge
    let mut edges: Vec<(u32, u32, u32)> = Vec::with_capacity(nt * 3);
    for (i, t) in tris.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            edges.push((a.min(b), a.max(b), i as u32));
        }
    }
    edges.sort_unstable();
    let mut nbr: Vec<Vec<u32>> = vec![Vec::new(); nt];
    let mut i = 0;
    while i < edges.len() {
        let mut j = i;
        while j < edges.len() && edges[j].0 == edges[i].0 && edges[j].1 == edges[i].1 {
            j += 1;
        }
        for x in i..j {
            for y in i..j {
                if x != y && edges[x].2 != edges[y].2 {
                    nbr[edges[x].2 as usize].push(edges[y].2);
                }
            }
        }
        i = j;
    }
    let eps = (pitch * 1e-3).max(1e-6);
    let mut seen = vec![false; nt];
    let mut out = Vec::new();
    for seed in 0..nt {
        if seen[seed] || area[seed] <= 0.0 {
            continue;
        }
        let n = normal[seed];
        let d = dot(n, pts[tris[seed][0] as usize]);
        let mut members = vec![seed];
        seen[seed] = true;
        let mut head = 0;
        while head < members.len() {
            let cur = members[head];
            head += 1;
            for &o in &nbr[cur] {
                let o = o as usize;
                if seen[o] || area[o] <= 0.0 || dot(normal[o], n) < 1.0 - 1e-5 {
                    continue;
                }
                if tris[o].iter().all(|&v| (dot(n, pts[v as usize]) - d).abs() <= eps) {
                    seen[o] = true;
                    members.push(o);
                }
            }
        }
        let total: f64 = members.iter().map(|&m| area[m]).sum();
        if total < min_area {
            continue;
        }
        members.sort_unstable();
        let tr: Vec<[V3; 3]> = members
            .iter()
            .map(|&m| {
                [
                    pts[tris[m][0] as usize],
                    pts[tris[m][1] as usize],
                    pts[tris[m][2] as usize],
                ]
            })
            .collect();
        let mut lo_all = [f64::INFINITY; 3];
        let mut hi_all = [f64::NEG_INFINITY; 3];
        let mut lo = Vec::with_capacity(tr.len());
        let mut hi = Vec::with_capacity(tr.len());
        for t in &tr {
            let mut l = [f64::INFINITY; 3];
            let mut h = [f64::NEG_INFINITY; 3];
            for p in t {
                for a in 0..3 {
                    l[a] = l[a].min(p[a]);
                    h[a] = h[a].max(p[a]);
                }
            }
            for a in 0..3 {
                lo_all[a] = lo_all[a].min(l[a]);
                hi_all[a] = hi_all[a].max(h[a]);
            }
            lo.push(l);
            hi.push(h);
        }
        out.push(Region {
            n,
            d,
            tris: tr,
            lo,
            hi,
            lo_all,
            hi_all,
        });
    }
    out
}

/// Is `q` (on the region's plane) within `margin` of the region's triangles?
fn near_region(r: &Region, q: V3, margin: f64) -> bool {
    for a in 0..3 {
        if q[a] < r.lo_all[a] - margin || q[a] > r.hi_all[a] + margin {
            return false;
        }
    }
    for (k, t) in r.tris.iter().enumerate() {
        if (0..3).any(|a| q[a] < r.lo[k][a] - margin || q[a] > r.hi[k][a] + margin) {
            continue;
        }
        if point_triangle(q, t[0], t[1], t[2]) <= margin {
            return true;
        }
    }
    false
}

fn det3(a: V3, b: V3, c: V3) -> f64 {
    dot(a, cross(b, c))
}

/// Move `v` onto the planes `(n, k)` (`n . x = k`): the nearest point on all of them.
fn project(v: V3, planes: &[(V3, f64)]) -> Option<V3> {
    match planes {
        [(n, k)] => Some(axpy(v, k - dot(*n, v), *n)),
        [(n1, k1), (n2, k2)] => {
            let g = dot(*n1, *n2);
            let det = 1.0 - g * g;
            if det < 1e-9 {
                return None;
            }
            let (r1, r2) = (k1 - dot(*n1, v), k2 - dot(*n2, v));
            let a = (r1 - g * r2) / det;
            let b = (r2 - g * r1) / det;
            Some(axpy(axpy(v, a, *n1), b, *n2))
        }
        [(n1, k1), (n2, k2), (n3, k3)] => {
            let det = det3(*n1, *n2, *n3);
            if det.abs() < 1e-6 {
                return None;
            }
            let c23 = cross(*n2, *n3);
            let c31 = cross(*n3, *n1);
            let c12 = cross(*n1, *n2);
            let p = [
                (k1 * c23[0] + k2 * c31[0] + k3 * c12[0]) / det,
                (k1 * c23[1] + k2 * c31[1] + k3 * c12[1]) / det,
                (k1 * c23[2] + k2 * c31[2] + k3 * c12[2]) / det,
            ];
            Some(p)
        }
        _ => None,
    }
}

/// Snap `inner`'s vertices onto `outer`'s flat regions moved inward by `t`: onto one plane, the
/// line of two, or the corner of three, when within about one `pitch` (1.5). Curved areas are left as they
/// are. Only positions change, so the surface stays closed.
pub fn square_inner(inner: &mut MeshArrays, outer: &MeshArrays, t: f64, pitch: f64) {
    if !(t > 0.0 && pitch > 0.0 && t.is_finite() && pitch.is_finite()) {
        return;
    }
    let opts = read_points(outer);
    let otris: Vec<[u32; 3]> = outer.triangles.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
    if otris.iter().any(|t| t.iter().any(|&i| i as usize >= opts.len())) {
        return;
    }
    let side = 3.0 * pitch;
    let regions = flat_regions(&opts, &otris, pitch, side * side);
    if regions.is_empty() {
        return;
    }
    // a vertex on a rounded corner of the cavity projects up to a wall thickness past the face
    let margin = 1.5 * t + pitch;
    // marching cubes leaves vertices up to about 1.2 voxels from a crease, so the snap radius
    // is a little over the voxel the spec names
    let snap = 1.5 * pitch;
    let reach = 3.5 * pitch;

    let verts = read_points(inner);
    let mut moved: Vec<V3> = verts.clone();
    for (vi, &v) in verts.iter().enumerate() {
        let mut cand: Vec<(f64, usize)> = Vec::new();
        for (ri, r) in regions.iter().enumerate() {
            let sd = dot(r.n, v) - (r.d - t);
            if sd.abs() > snap {
                continue;
            }
            let q = axpy(v, r.d - dot(r.n, v), r.n);
            if near_region(r, q, margin) {
                cand.push((sd.abs(), ri));
            }
        }
        if cand.is_empty() {
            continue;
        }
        cand.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let mut planes: Vec<(V3, f64)> = Vec::new();
        let mut best = v;
        for &(_, ri) in &cand {
            let r = &regions[ri];
            let ok = match planes.len() {
                0 => true,
                1 => dot(planes[0].0, r.n).abs() < 0.995,
                _ => det3(planes[0].0, planes[1].0, r.n).abs() > 0.1,
            };
            if !ok {
                continue;
            }
            planes.push((r.n, r.d - t));
            match project(v, &planes) {
                Some(p) if norm(sub(p, v)) <= reach => best = p,
                _ => {
                    planes.pop();
                }
            }
            if planes.len() == 3 {
                break;
            }
        }
        moved[vi] = best;
    }
    for (p, m) in inner.positions.chunks_exact_mut(3).zip(&moved) {
        p[0] = m[0] as f32;
        p[1] = m[1] as f32;
        p[2] = m[2] as f32;
    }
}
