//! Support Fins coverage — port of `web/js/sfcoverage.js` (held / bare overhangs).

use std::collections::{HashMap, HashSet};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
#[cfg(not(target_arch = "wasm32"))]
use std::thread;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

/// Default “Bare beyond” reach (mm), as 1.x `sfReach()`.
pub const DEFAULT_REACH: f64 = 5.0;

/// Inputs to [`compute`] (same fields as `coverage({ ... })` in 1.x).
#[derive(Clone, Debug)]
pub struct CoverageInput {
    pub positions: Vec<f32>,
    pub triangles: Vec<u32>,
    pub over_faces: Vec<u32>,
    pub fins: Vec<Vec<f32>>,
    pub gap: f64,
    pub reach: f64,
    pub max_sub: usize,
}

impl Default for CoverageInput {
    fn default() -> Self {
        Self {
            positions: Vec::new(),
            triangles: Vec::new(),
            over_faces: Vec::new(),
            fins: Vec::new(),
            gap: 0.2,
            reach: DEFAULT_REACH,
            max_sub: 250_000,
        }
    }
}

/// One bare region (worst-first order in [`CoverageOutput::spots`]).
#[derive(Clone, Debug, PartialEq)]
pub struct BareSpot {
    pub area: f64,
    pub span: f64,
    /// `None` when farther than 2.5 × reach from any support.
    pub far: Option<f64>,
    pub at: [f64; 3],
    /// Wall endpoints on the overhang face, when known.
    pub ends: Option<[[f64; 3]; 2]>,
}

/// Result of [`compute`].
#[derive(Clone, Debug, Default)]
pub struct CoverageOutput {
    pub total: f64,
    pub held: f64,
    pub tris: Vec<f32>,
    pub dist: Vec<f32>,
    pub spots: Vec<BareSpot>,
}

/// Centroids of bare spots (for parity tests).
pub fn bare_points(out: &CoverageOutput) -> Vec<[f64; 3]> {
    let mut v: Vec<_> = out.spots.iter().map(|s| s.at).collect();
    v.sort_by(|a, b| {
        a[0]
            .partial_cmp(&b[0])
            .unwrap()
            .then(a[1].partial_cmp(&b[1]).unwrap())
            .then(a[2].partial_cmp(&b[2]).unwrap())
    });
    v
}

/// UI readout while a check runs or after it — mirrors `t._cover` in 1.x.
#[derive(Clone, Debug, Default)]
pub struct CoverState {
    pub busy: bool,
    pub error: Option<String>,
    pub total: f64,
    pub held: f64,
    pub spots: Vec<BareSpot>,
    pub reach: f64,
    pub none: bool,
}

/// Cooperative cancel (shared with the worker thread on desktop).
#[derive(Clone)]
pub struct ComputeCancel {
    pub generation: Arc<AtomicU64>,
    pub seq: u64,
}

impl ComputeCancel {
    pub fn alive(&self) -> bool {
        self.generation.load(Ordering::Relaxed) == self.seq
    }
}

// --- geometry helpers (1.x sfcoverage.js) ---

#[inline]
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
fn crs(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[inline]
fn len(a: [f64; 3]) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

/// Distance from point `p` to triangle `abc` (exported for tests).
pub fn point_tri(p: [f64; 3], a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return len(ap);
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return len(bp);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return len(sub(
            p,
            [
                a[0] + ab[0] * v,
                a[1] + ab[1] * v,
                a[2] + ab[2] * v,
            ],
        ));
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return len(cp);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return len(sub(
            p,
            [
                a[0] + ac[0] * w,
                a[1] + ac[1] * w,
                a[2] + ac[2] * w,
            ],
        ));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return len(sub(
            p,
            [
                b[0] + (c[0] - b[0]) * w,
                b[1] + (c[1] - b[1]) * w,
                b[2] + (c[2] - b[2]) * w,
            ],
        ));
    }
    let n = crs(ab, ac);
    // 1.x `len(n) || 1`: a zero normal divides by 1, not by a tiny epsilon.
    (dot(ap, n).abs()) / len_or_one(n)
}

struct Grid {
    cell: f64,
    map: HashMap<i32, Vec<usize>>,
    ids: Vec<[f64; 3]>,
}

impl Grid {
    fn new(cell: f64) -> Self {
        Self {
            cell,
            map: HashMap::new(),
            ids: Vec::new(),
        }
    }

    fn key(i: i32, j: i32, k: i32) -> i32 {
        i.wrapping_mul(73_856_093) ^ j.wrapping_mul(19_349_663) ^ k.wrapping_mul(83_492_791)
    }

    fn cell_of(&self, p: [f64; 3]) -> [i32; 3] {
        [
            (p[0] / self.cell).floor() as i32,
            (p[1] / self.cell).floor() as i32,
            (p[2] / self.cell).floor() as i32,
        ]
    }

    fn add_idx(&mut self, i: i32, j: i32, k: i32, idx: usize) {
        let key = Self::key(i, j, k);
        self.map.entry(key).or_default().push(idx);
    }

    fn add_point(&mut self, p: [f64; 3]) -> usize {
        let idx = self.ids.len();
        self.ids.push(p);
        let [i, j, k] = self.cell_of(p);
        self.add_idx(i, j, k, idx);
        idx
    }

    fn near<F: FnMut(usize) -> bool>(&self, p: [f64; 3], mut f: F) -> bool {
        let [i, j, k] = self.cell_of(p);
        for a in -1..=1 {
            for b in -1..=1 {
                for d in -1..=1 {
                    if let Some(list) = self.map.get(&Self::key(i + a, j + b, k + d)) {
                        for &v in list {
                            if f(v) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    fn len(&self) -> usize {
        self.ids.len()
    }

    fn get(&self, idx: usize) -> [f64; 3] {
        self.ids[idx]
    }
}

struct FaceGrid {
    cell: f64,
    map: HashMap<i32, Vec<u32>>,
}

impl FaceGrid {
    fn new(cell: f64) -> Self {
        Self {
            cell,
            map: HashMap::new(),
        }
    }

    fn key(i: i32, j: i32, k: i32) -> i32 {
        Grid::key(i, j, k)
    }

    fn add(&mut self, i: i32, j: i32, k: i32, f: u32) {
        self.map.entry(Self::key(i, j, k)).or_default().push(f);
    }

    fn has_cell(&self, i: i32, j: i32, k: i32) -> bool {
        self.map.contains_key(&Self::key(i, j, k))
    }

    fn near<F: FnMut(u32) -> bool>(&self, p: [f64; 3], mut f: F) -> bool {
        let [i, j, k] = [p[0], p[1], p[2]].map(|x| (x / self.cell).floor() as i32);
        for a in -1..=1 {
            for b in -1..=1 {
                for d in -1..=1 {
                    if let Some(list) = self.map.get(&Self::key(i + a, j + b, k + d)) {
                        for &v in list {
                            if f(v) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }
}

/// `Math.round`: `floor(x + 0.5)` for every finite `x`, so `Math.round(-2.5)` is `-2`.
#[inline]
fn js_round(x: f64) -> i32 {
    (x + 0.5).floor() as i32
}

/// `len(n) || 1` in 1.x (a zero vector is treated as length 1).
#[inline]
fn len_or_one(a: [f64; 3]) -> f64 {
    let l = len(a);
    if l == 0.0 { 1.0 } else { l }
}

fn qk(p: [f64; 3], q: f64) -> i32 {
    let a = js_round(p[0] / q);
    let b = js_round(p[1] / q);
    let c = js_round(p[2] / q);
    a.wrapping_mul(73_856_093) ^ b.wrapping_mul(19_349_663) ^ c.wrapping_mul(83_492_791)
}

/// Run the coverage check (same algorithm as 1.x `coverage()`).
pub fn compute(input: &CoverageInput) -> CoverageOutput {
    compute_cancellable(input, None).expect("coverage")
}

/// Like [`compute`], but stops when `cancel` is no longer [`ComputeCancel::alive`].
pub fn compute_cancellable(input: &CoverageInput, cancel: Option<&ComputeCancel>) -> Option<CoverageOutput> {
    let alive = || cancel.is_none_or(|c| c.alive());
    let pos = &input.positions;
    let idx = &input.triangles;
    let over = &input.over_faces;
    let gap = input.gap;
    let reach = input.reach;
    let max_sub = input.max_sub;

    let p = |vi: u32| -> [f64; 3] {
        let i = vi as usize * 3;
        [pos[i] as f64, pos[i + 1] as f64, pos[i + 2] as f64]
    };
    let tri = |f: u32| -> [[f64; 3]; 3] {
        [
            p(idx[f as usize * 3]),
            p(idx[f as usize * 3 + 1]),
            p(idx[f as usize * 3 + 2]),
        ]
    };

    let mut out = CoverageOutput::default();
    if over.is_empty() {
        return Some(out);
    }
    if !alive() {
        return None;
    }

    let touch = (gap + 0.6).max(0.8);
    let cs = (touch * 2.0).max(2.0);
    let mut og = FaceGrid::new(cs);
    for &f in over {
        let [a, b, c] = tri(f);
        let lo = [0, 1, 2].map(|k| ((a[k].min(b[k]).min(c[k]) - touch) / cs).floor() as i32);
        let hi = [0, 1, 2].map(|k| ((a[k].max(b[k]).max(c[k]) + touch) / cs).floor() as i32);
        let cells = (hi[0] - lo[0] + 1) * (hi[1] - lo[1] + 1) * (hi[2] - lo[2] + 1);
        if cells > 400_000 {
            continue;
        }
        for i in lo[0]..=hi[0] {
            for j in lo[1]..=hi[1] {
                for k in lo[2]..=hi[2] {
                    og.add(i, j, k, f);
                }
            }
        }
    }

    let mut contacts = Grid::new(reach);
    let mut seen = HashSet::new();
    let dq = (reach / 14.0).max(0.35);
    let mut add_contact = |pnt: [f64; 3]| {
        let key = qk(pnt, dq);
        if seen.contains(&key) {
            return;
        }
        seen.insert(key);
        contacts.add_point(pnt);
    };

    let mut fd: HashMap<u32, ([f64; 3], [f64; 3], [f64; 3], [f64; 3], f64)> = HashMap::new();
    let mut tested = HashSet::new();

    let face = |f: u32, fd: &mut HashMap<_, _>, tri: &dyn Fn(u32) -> [[f64; 3]; 3]| {
        if let Some(&d) = fd.get(&f) {
            return d;
        }
        let t = tri(f);
        let a = t[0];
        let b = t[1];
        let c = t[2];
        let n = crs(sub(b, a), sub(c, a));
        let l = len_or_one(n);
        let n = [n[0] / l, n[1] / l, n[2] / l];
        let d = dot(n, a);
        let entry = (a, b, c, n, d);
        fd.insert(f, entry);
        entry
    };

    let mut test_pt = |pnt: [f64; 3], og: &FaceGrid, fd: &mut HashMap<_, _>, tri: &dyn Fn(u32) -> [[f64; 3]; 3]| {
        let key = qk(pnt, 0.25);
        if tested.contains(&key) {
            return;
        }
        tested.insert(key);
        let hit = og.near(pnt, |f| {
            let (a, b, c, n, d) = face(f, fd, tri);
            (dot(n, pnt) - d).abs() <= touch && point_tri(pnt, a, b, c) <= touch
        });
        if hit {
            add_contact(pnt);
        }
    };

    let occupied = |lo: [f64; 3], hi: [f64; 3], og: &FaceGrid| -> bool {
        let i0 = (lo[0] / cs).floor() as i32;
        let i1 = (hi[0] / cs).floor() as i32;
        let j0 = (lo[1] / cs).floor() as i32;
        let j1 = (hi[1] / cs).floor() as i32;
        let k0 = (lo[2] / cs).floor() as i32;
        let k1 = (hi[2] / cs).floor() as i32;
        for i in i0..=i1 {
            for j in j0..=j1 {
                for k in k0..=k1 {
                    if og.has_cell(i, j, k) {
                        return true;
                    }
                }
            }
        }
        false
    };

    for fp in &input.fins {
        let mut v = 0;
        while v + 8 < fp.len() {
            let t = [
                [fp[v] as f64, fp[v + 1] as f64, fp[v + 2] as f64],
                [fp[v + 3] as f64, fp[v + 4] as f64, fp[v + 5] as f64],
                [fp[v + 6] as f64, fp[v + 7] as f64, fp[v + 8] as f64],
            ];
            let lo = [0, 1, 2].map(|k| t[0][k].min(t[1][k]).min(t[2][k]) - touch);
            let hi = [0, 1, 2].map(|k| t[0][k].max(t[1][k]).max(t[2][k]) + touch);
            let vol = (hi[0] - lo[0]) * (hi[1] - lo[1]) * (hi[2] - lo[2]);
            if vol < 1e5 && !occupied(lo, hi, &og) {
                v += 9;
                continue;
            }
            for e in 0..3 {
                let a = t[e];
                let b = t[(e + 1) % 3];
                let l = len(sub(b, a));
                let mut d = 0.0;
                loop {
                    let t_param = if l > 0.0 { (d / l).min(1.0) } else { 1.0 };
                    let pnt = [
                        a[0] + (b[0] - a[0]) * t_param,
                        a[1] + (b[1] - a[1]) * t_param,
                        a[2] + (b[2] - a[2]) * t_param,
                    ];
                    let ci = (pnt[0] / cs).floor() as i32;
                    let cj = (pnt[1] / cs).floor() as i32;
                    let ck = (pnt[2] / cs).floor() as i32;
                    let mut busy = false;
                    'cells: for x in -1..=1 {
                        for y in -1..=1 {
                            for z in -1..=1 {
                                if og.has_cell(ci + x, cj + y, ck + z) {
                                    busy = true;
                                    break 'cells;
                                }
                            }
                        }
                    }
                    if busy {
                        test_pt(pnt, &og, &mut fd, &tri);
                    }
                    if t_param >= 1.0 {
                        break;
                    }
                    d += if busy { 0.5 } else { cs * 0.5 };
                }
            }
            v += 9;
            if !alive() {
                return None;
            }
        }
        if !alive() {
            return None;
        }
    }

    let is_over: HashSet<u32> = over.iter().copied().collect();
    let nv = pos.len() / 3;
    let ek = |a: u32, b: u32| -> u64 {
        if a < b {
            a as u64 * nv as u64 + b as u64
        } else {
            b as u64 * nv as u64 + a as u64
        }
    };
    let mut edge_of: HashMap<u64, (u32, u32)> = HashMap::new();
    for &f in over {
        for j in 0..3 {
            let a = idx[f as usize * 3 + j];
            let b = idx[f as usize * 3 + (j + 1) % 3];
            edge_of.insert(ek(a, b), (a, b));
        }
    }
    let nf = idx.len() / 3;
    for f in 0..nf {
        if f & 63 == 0 && !alive() {
            return None;
        }
        let f = f as u32;
        if is_over.contains(&f) {
            continue;
        }
        for j in 0..3 {
            let a = idx[f as usize * 3 + j];
            let b = idx[f as usize * 3 + (j + 1) % 3];
            if edge_of.get(&ek(a, b)).is_none() {
                continue;
            }
            let third = p(idx[f as usize * 3 + (j + 2) % 3]);
            let pa = p(a);
            let pb = p(b);
            if third[2] > pa[2].min(pb[2]) - 0.05 {
                continue;
            }
            let l = len(sub(pb, pa));
            let n = ((l / 0.5).ceil() as usize).max(1);
            for s in 0..=n {
                let t = s as f64 / n as f64;
                add_contact([
                    pa[0] + (pb[0] - pa[0]) * t,
                    pa[1] + (pb[1] - pa[1]) * t,
                    pa[2] + (pb[2] - pa[2]) * t,
                ]);
            }
        }
    }

    let far_lim = 2.5 * reach;
    let nearest = |m: [f64; 3], contacts: &Grid| -> f64 {
        let [ci, cj, ck] = contacts.cell_of(m);
        let mut best = f64::INFINITY;
        for k in 0..=3 {
            if best <= (k as f64 - 1.0) * reach || (k == 3 && best <= far_lim) {
                break;
            }
            for a in -(k as i32)..=(k as i32) {
                for b in -(k as i32)..=(k as i32) {
                    for d in -(k as i32)..=(k as i32) {
                        if a.abs().max(b.abs()).max(d.abs()) != k as i32 {
                            continue;
                        }
                        let key = Grid::key(ci + a, cj + b, ck + d);
                        if let Some(list) = contacts.map.get(&key) {
                            for &qi in list {
                                let q = contacts.get(qi);
                                let x = ((m[0] - q[0]).powi(2) + (m[1] - q[1]).powi(2) + (m[2] - q[2]).powi(2)).sqrt();
                                if x < best {
                                    best = x;
                                }
                            }
                        }
                    }
                }
            }
        }
        best.min(far_lim)
    };

    let mq = (reach / 33.0).max(0.15);
    let mut memo: HashMap<i32, f64> = HashMap::new();
    let dist_at = |pnt: [f64; 3], contacts: &Grid, memo: &mut HashMap<i32, f64>| -> f64 {
        let key = qk(pnt, mq);
        if let Some(&v) = memo.get(&key) {
            return v;
        }
        let v = nearest(pnt, contacts);
        memo.insert(key, v);
        v
    };

    let mut area = 0.0;
    for &f in over {
        let t = tri(f);
        area += len(crs(sub(t[1], t[0]), sub(t[2], t[0]))) / 2.0;
    }
    let mut h = (reach / 3.0).max(0.5);
    if area / (h * h * 0.43) > max_sub as f64 {
        h = (area / (max_sub as f64 * 0.43)).sqrt();
    }

    let mut tris = Vec::new();
    let mut dist = Vec::new();
    let mut bare_c: Vec<([f64; 3], f64, f64, Vec<[f64; 3]>)> = Vec::new();
    let h2 = h * h;

    let mut put = |p: [f64; 3], q: [f64; 3], r: [f64; 3]| {
        let ar = len(crs(sub(q, p), sub(r, p))) / 2.0;
        let m = [(p[0] + q[0] + r[0]) / 3.0, (p[1] + q[1] + r[1]) / 3.0, (p[2] + q[2] + r[2]) / 3.0];
        out.total += ar;
        let dp = dist_at(p, &contacts, &mut memo);
        let dq2 = dist_at(q, &contacts, &mut memo);
        let dr = dist_at(r, &contacts, &mut memo);
        let dm = dist_at(m, &contacts, &mut memo);
        if dm <= reach {
            out.held += ar;
        } else {
            bare_c.push((m, ar, dm, vec![p, q, r]));
        }
        tris.extend_from_slice(&[p[0] as f32, p[1] as f32, p[2] as f32]);
        tris.extend_from_slice(&[q[0] as f32, q[1] as f32, q[2] as f32]);
        tris.extend_from_slice(&[r[0] as f32, r[1] as f32, r[2] as f32]);
        dist.extend_from_slice(&[dp as f32, dq2 as f32, dr as f32]);
    };

    for &f in over {
        let mut stack = vec![tri(f)];
        let mut guard = 0u32;
        while let Some([p, q, r]) = stack.pop() {
            if guard & 511 == 0 && !alive() {
                return None;
            }
            let e0 = len(sub(q, p));
            let e1 = len(sub(r, q));
            let e2 = len(sub(p, r));
            let l = e0.max(e1).max(e2);
            if l * l > h2 * 2.0 && guard < 200_000 {
                guard += 1;
                let mid = |u: [f64; 3], v: [f64; 3]| {
                    [(u[0] + v[0]) / 2.0, (u[1] + v[1]) / 2.0, (u[2] + v[2]) / 2.0]
                };
                if l == e0 {
                    let m = mid(p, q);
                    stack.push([p, m, r]);
                    stack.push([m, q, r]);
                } else if l == e1 {
                    let m = mid(q, r);
                    stack.push([p, q, m]);
                    stack.push([p, m, r]);
                } else {
                    let m = mid(r, p);
                    stack.push([p, q, m]);
                    stack.push([m, q, r]);
                }
                continue;
            }
            put(p, q, r);
        }
    }
    out.tris = tris;
    out.dist = dist;

    let link = h * 2.2;
    let mut g = Grid::new(link);
    let mut parent: Vec<usize> = (0..bare_c.len()).collect();
    let mut find = |mut x: usize, parent: &mut [usize]| -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    };
    for (i, (m, _, _, _)) in bare_c.iter().enumerate() {
        g.near(*m, |j| {
            if ((m[0] - bare_c[j].0[0]).powi(2) + (m[1] - bare_c[j].0[1]).powi(2) + (m[2] - bare_c[j].0[2]).powi(2))
                .sqrt()
                <= link
            {
                let ra = find(i, &mut parent);
                let rb = find(j, &mut parent);
                if ra != rb {
                    parent[ra] = rb;
                }
            }
            false
        });
        let [ci, cj, ck] = g.cell_of(*m);
        g.add_idx(ci, cj, ck, i);
    }

    let mut groups: HashMap<usize, (f64, [f64; 3], [f64; 3], [f64; 3], Vec<[f64; 3]>)> = HashMap::new();
    for (i, (m, ar, _, t)) in bare_c.iter().enumerate() {
        let r = find(i, &mut parent);
        // 1.x starts `area` at 0 and then adds this piece. Seeding with `ar` counts it twice
        // and shifts the centroid (mushroom: 39.9993 instead of 40).
        let s = groups.entry(r).or_insert((0.0, *m, *m, [0.0; 3], Vec::new()));
        s.0 += *ar;
        s.4.extend(t.iter().copied());
        for k in 0..3 {
            s.1[k] = s.1[k].min(m[k]);
            s.2[k] = s.2[k].max(m[k]);
            s.3[k] += m[k] * ar;
        }
    }

    let mut far_m: HashMap<usize, f64> = HashMap::new();
    let mut worst: HashMap<usize, Vec<[f64; 3]>> = HashMap::new();
    for (i, (_, _, d, t)) in bare_c.iter().enumerate() {
        let r = find(i, &mut parent);
        if *d > *far_m.get(&r).unwrap_or(&0.0) {
            worst.insert(r, t.clone());
            far_m.insert(r, *d);
        }
    }

    let ends = |vs: &[[f64; 3]], abc: &[[f64; 3]; 3]| -> Option<[[f64; 3]; 2]> {
        let a = abc[0];
        let b = abc[1];
        let c = abc[2];
        let n = crs(sub(b, a), sub(c, a));
        let nl = len_or_one(n);
        let tv = if (n[2] / nl).abs() > 0.98 {
            [1.0, 0.0, 0.0]
        } else {
            crs(n, [0.0, 0.0, 1.0])
        };
        let tl = len_or_one(tv);
        let t = [tv[0] / tl, tv[1] / tl, tv[2] / tl];
        let mut lo = None;
        let mut hi = None;
        let mut pl = f64::INFINITY;
        let mut ph = f64::NEG_INFINITY;
        for v in vs {
            let d = dot(*v, t);
            if d < pl {
                pl = d;
                lo = Some(*v);
            }
            if d > ph {
                ph = d;
                hi = Some(*v);
            }
        }
        if ph > pl {
            Some([lo?, hi?])
        } else {
            None
        }
    };

    let min_area = (reach * reach / 2.0).max(4.0);
    let has_contacts = contacts.len() > 0;
    out.spots = groups
        .into_iter()
        .filter(|(_, s)| s.0 >= min_area)
        .map(|(r, s)| {
            let c = s.3.map(|x| x / s.0);
            let f = far_m.get(&r).copied().unwrap_or(0.0);
            let far = if has_contacts && f < far_lim { Some(f) } else { None };
            let abc: [[f64; 3]; 3] = worst
                .get(&r)
                .and_then(|v| {
                    if v.len() >= 3 {
                        Some([v[0], v[1], v[2]])
                    } else {
                        None
                    }
                })
                .unwrap_or([[0.0; 3]; 3]);
            BareSpot {
                area: s.0,
                span: len(sub(s.2, s.1)) + h,
                far,
                at: c,
                ends: ends(&s.4, &abc),
            }
        })
        .collect();
    out.spots.sort_by(|a, b| {
        let fa = a.far.unwrap_or(f64::INFINITY);
        let fb = b.far.unwrap_or(f64::INFINITY);
        fb.partial_cmp(&fa)
            .unwrap()
            .then(b.area.partial_cmp(&a.area).unwrap())
    });

    if !alive() {
        return None;
    }
    Some(out)
}

/// Result of a background coverage job (a cancelled run never produces a result).
#[derive(Clone, Debug)]
pub enum CoverageJobResult {
    Done(CoverageOutput),
    Error(String),
}

/// Slot entry is `(seq, result)`. [`CoverageJob::poll`] drops a result whose seq is no longer current.
pub(crate) type JobSlot = Mutex<Option<(u64, CoverageJobResult)>>;

/// Publish only while `seq` is still the current generation, under the slot lock.
pub(crate) fn publish_result(slot: &JobSlot, generation: &AtomicU64, seq: u64, result: CoverageJobResult) {
    let mut guard = slot.lock().expect("slot");
    if generation.load(Ordering::SeqCst) == seq {
        *guard = Some((seq, result));
    }
}

/// Runs coverage off the UI thread; a newer [`bump_generation`](Self::bump_generation) cancels in-flight work.
pub struct CoverageJob {
    pub generation: Arc<AtomicU64>,
    slot: Arc<JobSlot>,
    #[cfg(not(target_arch = "wasm32"))]
    worker_thread: Arc<Mutex<Option<std::thread::ThreadId>>>,
    /// Tests park the worker after it has started and before `compute`.
    #[cfg(not(target_arch = "wasm32"))]
    enter_hold: Arc<AtomicBool>,
    #[cfg(not(target_arch = "wasm32"))]
    entered: Arc<AtomicBool>,
    #[cfg(target_arch = "wasm32")]
    web: crate::web::coverage_job::WasmCoverageSide,
}

impl Default for CoverageJob {
    fn default() -> Self {
        Self::new()
    }
}

impl CoverageJob {
    pub fn new() -> Self {
        Self {
            generation: Arc::new(AtomicU64::new(0)),
            slot: Arc::new(Mutex::new(None)),
            #[cfg(not(target_arch = "wasm32"))]
            worker_thread: Arc::new(Mutex::new(None)),
            #[cfg(not(target_arch = "wasm32"))]
            enter_hold: Arc::new(AtomicBool::new(false)),
            #[cfg(not(target_arch = "wasm32"))]
            entered: Arc::new(AtomicBool::new(false)),
            #[cfg(target_arch = "wasm32")]
            web: crate::web::coverage_job::WasmCoverageSide::new(),
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// Same as 1.x `sfCoverageStop`: invalidate in-flight checks (never blocks on them).
    pub fn bump_generation(&self) -> u64 {
        let g = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        #[cfg(target_arch = "wasm32")]
        self.web.stop_worker();
        g
    }

    /// Park the next worker after it has started and before it computes. Tests cancel that run.
    #[cfg(not(target_arch = "wasm32"))]
    #[doc(hidden)]
    pub fn hold_worker_at_enter(&self) {
        self.entered.store(false, Ordering::SeqCst);
        self.enter_hold.store(true, Ordering::SeqCst);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[doc(hidden)]
    pub fn worker_entered(&self) -> bool {
        self.entered.load(Ordering::SeqCst)
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[doc(hidden)]
    pub fn release_enter_hold(&self) {
        self.enter_hold.store(false, Ordering::SeqCst);
    }

    /// Start a check for `seq` (must equal current [`Self::generation`]).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn start(&self, seq: u64, input: CoverageInput) {
        *self.slot.lock().expect("slot") = None;
        let generation = self.generation.clone();
        let slot = self.slot.clone();
        let worker_thread = self.worker_thread.clone();
        let enter_hold = self.enter_hold.clone();
        let entered = self.entered.clone();
        thread::spawn(move || {
            let tid = thread::current().id();
            {
                let mut guard = worker_thread.lock().expect("worker_thread");
                // A newer start may already own the id. Don't replace it, and don't compute.
                if generation.load(Ordering::SeqCst) != seq {
                    return;
                }
                *guard = Some(tid);
            }
            entered.store(true, Ordering::SeqCst);
            while enter_hold.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(1));
            }
            let cancel = ComputeCancel {
                generation: generation.clone(),
                seq,
            };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                compute_cancellable(&input, Some(&cancel))
            }));
            {
                let mut guard = worker_thread.lock().expect("worker_thread");
                // Only the thread that still owns the id may clear it.
                if *guard == Some(tid) {
                    *guard = None;
                }
            }
            let published = match result {
                Ok(Some(out)) => Some(CoverageJobResult::Done(out)),
                Ok(None) => None,
                Err(_) => Some(CoverageJobResult::Error("coverage panic".into())),
            };
            if let Some(res) = published {
                publish_result(&slot, &generation, seq, res);
            }
        });
    }

    #[cfg(target_arch = "wasm32")]
    pub fn start(&self, seq: u64, input: CoverageInput) {
        *self.slot.lock().expect("slot") = None;
        self.web
            .start(self.generation.clone(), self.slot.clone(), seq, input);
    }

    /// Thread running coverage (desktop only), after [`Self::start`].
    #[cfg(not(target_arch = "wasm32"))]
    pub fn worker_thread_id(&self) -> Option<std::thread::ThreadId> {
        *self.worker_thread.lock().expect("worker_thread")
    }

    pub fn poll(&self) -> Option<CoverageJobResult> {
        let mut guard = self.slot.lock().ok()?;
        let current = self.generation.load(Ordering::SeqCst);
        match guard.take() {
            Some((seq, result)) if seq == current => Some(result),
            _ => None,
        }
    }
}

#[cfg(test)]
mod js_round_tests {
    #[test]
    fn matches_math_round_on_halves() {
        assert_eq!(super::js_round(2.5), 3);
        assert_eq!(super::js_round(-2.5), -2);
        assert_eq!(super::js_round(-1.5), -1);
        assert_eq!(super::js_round(0.5), 1);
        assert_eq!(super::js_round(-0.5), 0);
    }
}
