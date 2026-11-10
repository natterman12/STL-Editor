//! Isotropic remeshing (Botsch & Kobbelt 2004), a port of `remesh()` in
//! `web/engine/meshedit.js`: split long edges, collapse short ones, flip edges towards
//! valence 6, then relax vertices along the surface and project them back onto it. Sharp
//! edges (over 40 degrees) are kept: their vertices don't slide and their edges never flip.
//!
//! The input is the mesh *after* `refineToLength(edge)` (that step stays in the Manifold
//! kernel). The passes run in the same order, over the same edge lists, as the JavaScript,
//! so both give the same mesh apart from last-bit rounding (`Math.hypot` against `sqrt`).

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};
use wasm_bindgen::prelude::*;

/// Multiply-and-fold hasher for the integer keys used here (edges, grid cells).
#[derive(Default)]
struct FastHasher(u64);

impl Hasher for FastHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(b as u64);
        }
    }
    #[inline]
    fn write_u64(&mut self, n: u64) {
        let h = (self.0.rotate_left(5) ^ n).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        self.0 = h ^ (h >> 32);
    }
}

type Build = BuildHasherDefault<FastHasher>;
type FastMap<K, V> = HashMap<K, V, Build>;
type FastSet<K> = HashSet<K, Build>;

const NONE: u32 = u32::MAX;
const BROKEN_VERTEX: &str = "Remesh found a broken vertex.";

#[inline(always)]
fn nxt(c: u32) -> u32 {
    if c % 3 == 2 { c - 2 } else { c + 1 }
}
#[inline(always)]
fn prv(c: u32) -> u32 {
    if c % 3 == 0 { c + 2 } else { c - 1 }
}
#[inline(always)]
fn ekey(a: u32, b: u32) -> u64 {
    if a < b { ((a as u64) << 32) | b as u64 } else { ((b as u64) << 32) | a as u64 }
}
#[inline(always)]
fn norm(n: [f64; 3]) -> f64 {
    (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt()
}
#[inline(always)]
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Reused buffers, so the passes don't allocate per edge.
#[derive(Default)]
struct Scratch {
    ring_r: Vec<u32>,
    ring_k: Vec<u32>,
    nr: Vec<u32>,
    nk: Vec<u32>,
    tris: Vec<u32>,
    alive: Vec<u32>,
    outer: Vec<u32>,
    cs: Vec<u32>,
    fr: Vec<u32>,
}

/// Corner table: V[c] is the vertex at corner c, O[c] the corner opposite across the edge
/// facing c, VC[v] one corner at vertex v.
struct TriMesh {
    p: Vec<f64>,
    v: Vec<u32>,
    o: Vec<u32>,
    dead: Vec<bool>,
    vc: Vec<u32>,
    vdead: Vec<bool>,
    feat: FastSet<u64>,
}

impl TriMesh {
    fn new(p: Vec<f64>, f: &[u32]) -> Result<TriMesh, String> {
        let nv = p.len() / 3;
        let mut o = vec![NONE; f.len()];
        let mut vc = vec![NONE; nv];
        let mut m: FastMap<u64, u32> = FastMap::with_capacity_and_hasher(f.len() / 2 + 16, Build::default());
        let n = nv as u64;
        for c in 0..f.len() as u32 {
            let a = f[nxt(c) as usize] as u64;
            let b = f[prv(c) as usize] as u64;
            if let Some(other) = m.remove(&(b * n + a)) {
                o[c as usize] = other;
                o[other as usize] = c;
            } else {
                m.insert(a * n + b, c);
            }
            vc[f[c as usize] as usize] = c;
        }
        if !m.is_empty() {
            return Err("Remesh needs a closed solid.".into());
        }
        Ok(TriMesh { p, v: f.to_vec(), o, dead: vec![false; f.len() / 3], vc, vdead: vec![false; nv], feat: FastSet::default() })
    }

    #[inline(always)]
    fn n_t(&self) -> u32 {
        (self.v.len() / 3) as u32
    }
    #[inline(always)]
    fn n_v(&self) -> u32 {
        (self.p.len() / 3) as u32
    }
    #[inline(always)]
    fn pos(&self, a: u32) -> [f64; 3] {
        let i = a as usize * 3;
        [self.p[i], self.p[i + 1], self.p[i + 2]]
    }
    #[inline(always)]
    fn vert(&self, c: u32) -> u32 {
        self.v[c as usize]
    }
    #[inline(always)]
    fn opp(&self, c: u32) -> u32 {
        self.o[c as usize]
    }

    /// corners at v, going around it
    fn ring_into(&self, v: u32, out: &mut Vec<u32>) -> Result<(), String> {
        out.clear();
        let c0 = self.vc[v as usize];
        let mut c = c0;
        loop {
            out.push(c);
            c = nxt(self.opp(nxt(c)));
            if out.len() > 1000 {
                return Err(BROKEN_VERTEX.into());
            }
            if c == c0 {
                return Ok(());
            }
        }
    }

    fn valence(&self, v: u32) -> Result<usize, String> {
        let c0 = self.vc[v as usize];
        let (mut c, mut n) = (c0, 0usize);
        loop {
            n += 1;
            c = nxt(self.opp(nxt(c)));
            if n > 1000 {
                return Err(BROKEN_VERTEX.into());
            }
            if c == c0 {
                return Ok(n);
            }
        }
    }

    fn has_neighbor(&self, v: u32, w: u32) -> Result<bool, String> {
        let c0 = self.vc[v as usize];
        let (mut c, mut n, mut found) = (c0, 0usize, false);
        loop {
            n += 1;
            found |= self.vert(nxt(c)) == w;
            c = nxt(self.opp(nxt(c)));
            if n > 1000 {
                return Err(BROKEN_VERTEX.into());
            }
            if c == c0 {
                return Ok(found);
            }
        }
    }

    /// how many feature (sharp) edges meet at v
    fn feat_count(&self, v: u32) -> Result<usize, String> {
        let c0 = self.vc[v as usize];
        let (mut c, mut n, mut cnt) = (c0, 0usize, 0usize);
        let any = !self.feat.is_empty();
        loop {
            n += 1;
            if any && self.feat.contains(&ekey(v, self.vert(nxt(c)))) {
                cnt += 1;
            }
            c = nxt(self.opp(nxt(c)));
            if n > 1000 {
                return Err(BROKEN_VERTEX.into());
            }
            if c == c0 {
                return Ok(cnt);
            }
        }
    }

    #[inline]
    fn len(&self, a: u32, b: u32) -> f64 {
        let (pa, pb) = (self.pos(a), self.pos(b));
        norm([pa[0] - pb[0], pa[1] - pb[1], pa[2] - pb[2]])
    }

    /// normal of (a, b, c) with a at position pa
    #[inline]
    fn tri_normal_at(&self, pa: [f64; 3], b: u32, c: u32) -> [f64; 3] {
        let (pb, pc) = (self.pos(b), self.pos(c));
        let (ux, uy, uz) = (pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]);
        let (wx, wy, wz) = (pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]);
        [uy * wz - uz * wy, uz * wx - ux * wz, ux * wy - uy * wx]
    }
    #[inline]
    fn tri_normal(&self, a: u32, b: u32, c: u32) -> [f64; 3] {
        self.tri_normal_at(self.pos(a), b, c)
    }

    /// re-link opposite corners among `tris` (alive, changed) and the corners outside them
    fn relink(&mut self, tris: &[u32], outer: &[u32], cs: &mut Vec<u32>) -> Result<(), String> {
        cs.clear();
        for &t in tris {
            cs.extend_from_slice(&[3 * t, 3 * t + 1, 3 * t + 2]);
        }
        for &c in cs.iter() {
            let (a, b) = (self.vert(nxt(c)), self.vert(prv(c)));
            let hit = |o: &&u32| self.vert(nxt(**o)) == b && self.vert(prv(**o)) == a;
            let found = match cs.iter().find(hit).or_else(|| outer.iter().find(hit)) {
                Some(&o) => o,
                None => return Err("remesh relink failed".into()),
            };
            self.o[c as usize] = found;
            self.o[found as usize] = c;
        }
        for &c in cs.iter() {
            let w = self.vert(c);
            self.vc[w as usize] = c;
        }
        Ok(())
    }

    fn outer_of(&self, tris: &[u32], out: &mut Vec<u32>) {
        out.clear();
        for &t in tris {
            for i in 0..3 {
                let o = self.opp(3 * t + i);
                if !tris.contains(&(o / 3)) {
                    out.push(o);
                }
            }
        }
    }

    fn set_tri(&mut self, t: u32, a: u32, b: u32, c: u32) {
        let i = t as usize * 3;
        self.v[i] = a;
        self.v[i + 1] = b;
        self.v[i + 2] = c;
    }

    fn split(&mut self, s: &mut Scratch, c: u32) -> Result<(), String> {
        let d = self.opp(c);
        let (x, a, b, y) = (self.vert(c), self.vert(nxt(c)), self.vert(prv(c)), self.vert(d));
        let m = self.n_v();
        let (pa, pb) = (self.pos(a), self.pos(b));
        self.p.extend_from_slice(&[(pa[0] + pb[0]) / 2.0, (pa[1] + pb[1]) / 2.0, (pa[2] + pb[2]) / 2.0]);
        self.vc.push(NONE);
        self.vdead.push(false);
        let (t1, t2, t3) = (c / 3, d / 3, self.n_t());
        let t4 = t3 + 1;
        self.v.extend_from_slice(&[0; 6]);
        self.o.extend_from_slice(&[NONE; 6]);
        self.dead.extend_from_slice(&[false; 2]);
        self.outer_of(&[t1, t2], &mut s.outer);
        self.set_tri(t1, x, a, m);
        self.set_tri(t3, x, m, b);
        self.set_tri(t2, y, b, m);
        self.set_tri(t4, y, m, a);
        self.relink(&[t1, t2, t3, t4], &s.outer, &mut s.cs)?;
        if self.feat.remove(&ekey(a, b)) {
            self.feat.insert(ekey(a, m));
            self.feat.insert(ekey(m, b));
        }
        Ok(())
    }

    /// collapse the edge opposite corner c, removing vertex r and keeping k at position xp
    fn collapse(&mut self, s: &mut Scratch, c: u32, r: u32, k: u32, xp: [f64; 3], hi: f64) -> Result<bool, String> {
        let d = self.opp(c);
        let (x, y) = (self.vert(c), self.vert(d));
        if x == y {
            return Ok(false);
        }
        self.ring_into(r, &mut s.ring_r)?;
        self.ring_into(k, &mut s.ring_k)?;
        if s.ring_r.len() <= 3 || s.ring_k.len() <= 3 {
            return Ok(false);
        }
        s.nr.clear();
        s.nk.clear();
        for &cc in &s.ring_r {
            s.nr.push(self.vert(nxt(cc)));
        }
        for &cc in &s.ring_k {
            s.nk.push(self.vert(nxt(cc)));
        }
        if s.nr.iter().filter(|w| s.nk.contains(w)).count() != 2 {
            return Ok(false);
        }
        if self.valence(x)? <= 3 || self.valence(y)? <= 3 {
            return Ok(false);
        }
        // no flipped or stretched triangles around either end
        for (u, ring) in [(r, &s.ring_r), (k, &s.ring_k)] {
            for &cc in ring.iter() {
                let (b, e) = (self.vert(nxt(cc)), self.vert(prv(cc)));
                if b == r || b == k || e == r || e == k {
                    continue;
                }
                let (n0, n1) = (self.tri_normal(u, b, e), self.tri_normal_at(xp, b, e));
                let (l0, l1) = (norm(n0), norm(n1));
                if l1 < 1e-12 || dot(n0, n1) < 0.2 * l0 * l1 {
                    return Ok(false);
                }
                let pb = self.pos(b);
                if norm([xp[0] - pb[0], xp[1] - pb[1], xp[2] - pb[2]]) > hi {
                    return Ok(false);
                }
            }
        }
        s.tris.clear();
        for &cc in &s.ring_r {
            if !s.tris.contains(&(cc / 3)) {
                s.tris.push(cc / 3);
            }
        }
        self.outer_of(&s.tris, &mut s.outer);
        let (t1, t2) = (c / 3, d / 3);
        for &t in &s.tris {
            for i in 0..3 {
                let j = (3 * t + i) as usize;
                if self.v[j] == r {
                    self.v[j] = k;
                }
            }
        }
        self.dead[t1 as usize] = true;
        self.dead[t2 as usize] = true;
        s.alive.clear();
        s.alive.extend(s.tris.iter().copied().filter(|&t| t != t1 && t != t2));
        let dead = &self.dead;
        s.outer.retain(|&o| !dead[(o / 3) as usize]);
        // features: r's edges move to k
        if !self.feat.is_empty() {
            s.fr.clear();
            for &w in &s.nr {
                if self.feat.remove(&ekey(r, w)) {
                    s.fr.push(w);
                }
            }
            for &w in &s.fr {
                if w != k {
                    self.feat.insert(ekey(k, w));
                }
            }
        }
        self.relink(&s.alive, &s.outer, &mut s.cs)?;
        let i = k as usize * 3;
        self.p[i..i + 3].copy_from_slice(&xp);
        self.vdead[r as usize] = true;
        Ok(true)
    }

    fn flip(&mut self, s: &mut Scratch, c: u32) -> Result<bool, String> {
        let d = self.opp(c);
        let (x, a, b, y) = (self.vert(c), self.vert(nxt(c)), self.vert(prv(c)), self.vert(d));
        if x == y || (!self.feat.is_empty() && self.feat.contains(&ekey(a, b))) {
            return Ok(false);
        }
        let (va, vb) = (self.valence(a)? as i32, self.valence(b)? as i32);
        let (vx, vy) = (self.valence(x)? as i32, self.valence(y)? as i32);
        if va <= 3 || vb <= 3 {
            return Ok(false);
        }
        if self.has_neighbor(x, y)? {
            return Ok(false);
        }
        let dev = |p: i32, q: i32, r: i32, t: i32| (p - 6).abs() + (q - 6).abs() + (r - 6).abs() + (t - 6).abs();
        if dev(va - 1, vb - 1, vx + 1, vy + 1) >= dev(va, vb, vx, vy) {
            return Ok(false);
        }
        // the new triangles must face the same way as the old ones
        let (o1, o2) = (self.tri_normal(x, a, b), self.tri_normal(y, b, a));
        let on = [o1[0] + o2[0], o1[1] + o2[1], o1[2] + o2[2]];
        let lon = norm(on);
        for nn in [self.tri_normal(x, a, y), self.tri_normal(y, b, x)] {
            let l = norm(nn);
            if l < 1e-12 || dot(nn, on) < 0.5 * l * lon {
                return Ok(false);
            }
        }
        let (t1, t2) = (c / 3, d / 3);
        self.outer_of(&[t1, t2], &mut s.outer);
        self.set_tri(t1, x, a, y);
        self.set_tri(t2, y, b, x);
        self.relink(&[t1, t2], &s.outer, &mut s.cs)?;
        Ok(true)
    }

    /// one corner per edge, for alive triangles
    fn edges(&self) -> Vec<u32> {
        let mut out = Vec::with_capacity(self.v.len() / 2);
        for t in 0..self.n_t() {
            if self.dead[t as usize] {
                continue;
            }
            for c in 3 * t..3 * t + 3 {
                if c < self.opp(c) {
                    out.push(c);
                }
            }
        }
        out
    }
}

/// Uniform grid of triangles for closest-point queries (the surface to project back onto).
struct TriGrid<'a> {
    v: &'a [f64],
    f: &'a [u32],
    h: f64,
    cells: FastMap<u64, u32>,
    start: Vec<u32>,
    items: Vec<u32>,
    stamp: Vec<u32>,
    qid: u32,
}

#[inline(always)]
fn cell_key(i: i64, j: i64, k: i64) -> u64 {
    (i.wrapping_add(1048576).wrapping_mul(2097152).wrapping_add(j.wrapping_add(1048576)))
        .wrapping_mul(2097152)
        .wrapping_add(k.wrapping_add(1048576)) as u64
}

impl<'a> TriGrid<'a> {
    fn new(v: &'a [f64], f: &'a [u32], h: f64) -> TriGrid<'a> {
        let nt = f.len() / 3;
        let mut cells: FastMap<u64, u32> = FastMap::default();
        let mut counts: Vec<u32> = Vec::new();
        let mut ids: Vec<u32> = Vec::new(); // cell of every (triangle, cell) pair, in triangle order
        let mut ranges: Vec<[i64; 6]> = Vec::with_capacity(nt);
        for t in 0..nt {
            let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
            for e in 0..3 {
                let p = f[t * 3 + e] as usize * 3;
                for k in 0..3 {
                    lo[k] = lo[k].min(v[p + k]);
                    hi[k] = hi[k].max(v[p + k]);
                }
            }
            let r = [
                (lo[0] / h).floor() as i64, (hi[0] / h).floor() as i64,
                (lo[1] / h).floor() as i64, (hi[1] / h).floor() as i64,
                (lo[2] / h).floor() as i64, (hi[2] / h).floor() as i64,
            ];
            for i in r[0]..=r[1] {
                for j in r[2]..=r[3] {
                    for k in r[4]..=r[5] {
                        let next = counts.len() as u32;
                        let id = *cells.entry(cell_key(i, j, k)).or_insert(next);
                        if id == next {
                            counts.push(0);
                        }
                        counts[id as usize] += 1;
                        ids.push(id);
                    }
                }
            }
            ranges.push(r);
        }
        let mut start = vec![0u32; counts.len() + 1];
        for c in 0..counts.len() {
            start[c + 1] = start[c] + counts[c];
        }
        let mut fill: Vec<u32> = start[..counts.len()].to_vec();
        let mut items = vec![0u32; ids.len()];
        let mut n = 0usize;
        for (t, r) in ranges.iter().enumerate() {
            let cnt = ((r[1] - r[0] + 1) * (r[3] - r[2] + 1) * (r[5] - r[4] + 1)) as usize;
            for &id in &ids[n..n + cnt] {
                items[fill[id as usize] as usize] = t as u32;
                fill[id as usize] += 1;
            }
            n += cnt;
        }
        TriGrid { v, f, h, cells, start, items, stamp: vec![0; nt], qid: 0 }
    }

    /// nearest surface point to p among triangles within about h/2
    fn closest(&mut self, p: [f64; 3]) -> Option<[f64; 3]> {
        let h = self.h;
        // 2x2x2 block around the point
        let ci = (p[0] / h - 0.5).floor() as i64;
        let cj = (p[1] / h - 0.5).floor() as i64;
        let ck = (p[2] / h - 0.5).floor() as i64;
        self.qid += 1;
        let q = self.qid;
        let mut best = f64::INFINITY;
        let mut out = [0.0; 3];
        for di in 0..2 {
            for dj in 0..2 {
                for dk in 0..2 {
                    let id = match self.cells.get(&cell_key(ci + di, cj + dj, ck + dk)) {
                        Some(&id) => id as usize,
                        None => continue,
                    };
                    for m in self.start[id] as usize..self.start[id + 1] as usize {
                        let t = self.items[m] as usize;
                        if self.stamp[t] == q {
                            continue;
                        }
                        self.stamp[t] = q;
                        let (d2, cp) = closest_on_tri(p, self.v, self.f, t);
                        if d2 < best {
                            best = d2;
                            out = cp;
                        }
                    }
                }
            }
        }
        if best < f64::INFINITY { Some(out) } else { None }
    }
}

/// closest point of triangle t to p, with the squared distance
#[inline]
fn closest_on_tri(p: [f64; 3], v: &[f64], f: &[u32], t: usize) -> (f64, [f64; 3]) {
    let (ia, ib, ic) = (f[t * 3] as usize * 3, f[t * 3 + 1] as usize * 3, f[t * 3 + 2] as usize * 3);
    let a = [v[ia], v[ia + 1], v[ia + 2]];
    let b = [v[ib], v[ib + 1], v[ib + 2]];
    let c = [v[ic], v[ic + 1], v[ic + 2]];
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let ap = [p[0] - a[0], p[1] - a[1], p[2] - a[2]];
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    let q;
    if d1 <= 0.0 && d2 <= 0.0 {
        q = a;
    } else {
        let bp = [p[0] - b[0], p[1] - b[1], p[2] - b[2]];
        let (d3, d4) = (dot(ab, bp), dot(ac, bp));
        let cp = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
        let (d5, d6) = (dot(ab, cp), dot(ac, cp));
        let (vc, vb, va) = (d1 * d4 - d3 * d2, d5 * d2 - d1 * d6, d3 * d6 - d5 * d4);
        if d3 >= 0.0 && d4 <= d3 {
            q = b;
        } else if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
            let s = d1 / (d1 - d3);
            q = [a[0] + s * ab[0], a[1] + s * ab[1], a[2] + s * ab[2]];
        } else if d6 >= 0.0 && d5 <= d6 {
            q = c;
        } else if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
            let s = d2 / (d2 - d6);
            q = [a[0] + s * ac[0], a[1] + s * ac[1], a[2] + s * ac[2]];
        } else if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
            let s = (d4 - d3) / ((d4 - d3) + (d5 - d6));
            q = [b[0] + s * (c[0] - b[0]), b[1] + s * (c[1] - b[1]), b[2] + s * (c[2] - b[2])];
        } else {
            let den = 1.0 / (va + vb + vc);
            let (s, w) = (vb * den, vc * den);
            q = [a[0] + ab[0] * s + ac[0] * w, a[1] + ab[1] * s + ac[1] * w, a[2] + ab[2] * s + ac[2] * w];
        }
    }
    let d = [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
    (dot(d, d), q)
}

/// The remesh itself. `positions` (x, y, z per vertex) and `faces` (three vertex indices per
/// triangle) describe a closed mesh already refined to `edge`. Returns the new positions and
/// faces, or a plain-English message.
pub fn remesh_core(positions: &[f32], faces: &[u32], edge: f64, iterations: u32) -> Result<(Vec<f32>, Vec<u32>), String> {
    if !(edge > 0.0) {
        return Err("Edge length must be positive.".into());
    }
    if positions.len() % 3 != 0 || faces.len() % 3 != 0 {
        return Err("Remesh needs three numbers per vertex and three corners per triangle.".into());
    }
    let nv = positions.len() / 3;
    if faces.iter().any(|&i| i as usize >= nv) {
        return Err("Remesh found a triangle that uses a missing vertex.".into());
    }
    let ref_v: Vec<f64> = positions.iter().map(|&x| x as f64).collect();
    let mut m = TriMesh::new(ref_v.clone(), faces)?;
    let mut s = Scratch::default();

    // sharp edges of the original shape
    let cos40 = (40.0 * std::f64::consts::PI / 180.0).cos();
    for c in m.edges() {
        let d = m.opp(c);
        let (a, b) = (m.vert(nxt(c)), m.vert(prv(c)));
        let (n1, n2) = (m.tri_normal(m.vert(c), a, b), m.tri_normal(m.vert(d), b, a));
        let (l1, l2) = (norm(n1), norm(n2));
        if l1 > 0.0 && l2 > 0.0 && dot(n1, n2) / (l1 * l2) < cos40 {
            m.feat.insert(ekey(a, b));
        }
    }

    let mut grid = TriGrid::new(&ref_v, faces, edge);
    let (hi, lo) = (edge * 4.0 / 3.0, edge * 4.0 / 5.0);
    let mut np: Vec<f64> = Vec::new();
    let mut fixed: Vec<bool> = Vec::new();
    for _ in 0..iterations {
        // 1. split long edges
        for c in m.edges() {
            if m.dead[(c / 3) as usize] {
                continue;
            }
            if m.len(m.vert(nxt(c)), m.vert(prv(c))) > hi {
                m.split(&mut s, c)?;
            }
        }
        // 2. collapse short edges
        for c in m.edges() {
            if m.dead[(c / 3) as usize] {
                continue;
            }
            let (a, b) = (m.vert(nxt(c)), m.vert(prv(c)));
            if m.vdead[a as usize] || m.vdead[b as usize] || m.len(a, b) >= lo {
                continue;
            }
            let (fa, fb) = (m.feat_count(a)?, m.feat_count(b)?);
            let on_feat = m.feat.contains(&ekey(a, b));
            let (r, k, mid) = if fa == 0 && fb == 0 {
                (a, b, true)
            } else if fa != 0 && fb == 0 {
                (b, a, false)
            } else if fb != 0 && fa == 0 {
                (a, b, false)
            } else if on_feat && fa == 2 {
                (a, b, false)
            } else if on_feat && fb == 2 {
                (b, a, false)
            } else {
                continue;
            };
            let xp = if mid {
                let (pa, pb) = (m.pos(a), m.pos(b));
                [(pa[0] + pb[0]) / 2.0, (pa[1] + pb[1]) / 2.0, (pa[2] + pb[2]) / 2.0]
            } else {
                m.pos(k)
            };
            m.collapse(&mut s, c, r, k, xp, hi)?;
        }
        // 3. flip towards valence 6
        for c in m.edges() {
            if !m.dead[(c / 3) as usize] {
                m.flip(&mut s, c)?;
            }
        }
        // 4. tangential relaxation, then back onto the surface
        let n = m.n_v() as usize;
        np.clear();
        np.extend_from_slice(&m.p[..n * 3]);
        fixed.clear();
        fixed.resize(n, false);
        for &key in &m.feat {
            fixed[(key >> 32) as usize] = true;
            fixed[(key & 0xFFFF_FFFF) as usize] = true;
        }
        for i in 0..n {
            if fixed[i] || m.vdead[i] || m.vc[i] == NONE {
                continue;
            }
            let pi = m.pos(i as u32);
            let (mut cx, mut cy, mut cz, mut nx, mut ny, mut nz) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
            let mut cnt = 0u32;
            let c0 = m.vc[i];
            let mut c = c0;
            loop {
                let (w, e) = (m.pos(m.vert(nxt(c))), m.pos(m.vert(prv(c))));
                cx += w[0];
                cy += w[1];
                cz += w[2];
                let (ux, uy, uz) = (w[0] - pi[0], w[1] - pi[1], w[2] - pi[2]);
                let (wx, wy, wz) = (e[0] - pi[0], e[1] - pi[1], e[2] - pi[2]);
                nx += uy * wz - uz * wy;
                ny += uz * wx - ux * wz;
                nz += ux * wy - uy * wx;
                cnt += 1;
                c = nxt(m.opp(nxt(c)));
                if c == c0 || cnt >= 1000 {
                    break;
                }
            }
            let cf = cnt as f64;
            cx /= cf;
            cy /= cf;
            cz /= cf;
            let mut ln = norm([nx, ny, nz]);
            if ln == 0.0 || ln.is_nan() {
                ln = 1.0;
            }
            nx /= ln;
            ny /= ln;
            nz /= ln;
            let (mut dx, mut dy, mut dz) = (cx - pi[0], cy - pi[1], cz - pi[2]);
            let dn = dx * nx + dy * ny + dz * nz;
            dx -= dn * nx;
            dy -= dn * ny;
            dz -= dn * nz;
            let t = [pi[0] + 0.6 * dx, pi[1] + 0.6 * dy, pi[2] + 0.6 * dz];
            let q = grid.closest(t).unwrap_or(t);
            np[i * 3..i * 3 + 3].copy_from_slice(&q);
        }
        m.p[..n * 3].copy_from_slice(&np);
    }

    // compact
    let mut remap = vec![NONE; m.n_v() as usize];
    let mut vs: Vec<f32> = Vec::new();
    let mut fs: Vec<u32> = Vec::new();
    for t in 0..m.n_t() {
        if m.dead[t as usize] {
            continue;
        }
        for c in 3 * t..3 * t + 3 {
            let w = m.vert(c) as usize;
            if remap[w] == NONE {
                remap[w] = (vs.len() / 3) as u32;
                vs.extend_from_slice(&[m.p[w * 3] as f32, m.p[w * 3 + 1] as f32, m.p[w * 3 + 2] as f32]);
            }
            fs.push(remap[w]);
        }
    }
    Ok((vs, fs))
}

/// What `remesh` returns: read `positions` (Float32Array) and `faces` (Uint32Array), then
/// call `free()`.
#[wasm_bindgen]
pub struct RemeshResult {
    positions: Vec<f32>,
    faces: Vec<u32>,
}

#[wasm_bindgen]
impl RemeshResult {
    #[wasm_bindgen(getter)]
    pub fn positions(&self) -> Vec<f32> {
        self.positions.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn faces(&self) -> Vec<u32> {
        self.faces.clone()
    }
    #[wasm_bindgen(getter, js_name = triangleCount)]
    pub fn triangle_count(&self) -> u32 {
        (self.faces.len() / 3) as u32
    }
}

/// Isotropic remesh of a closed mesh that was already refined to `edge` (millimetres).
/// `iterations` defaults to 5. Throws an Error with a plain-English message.
#[wasm_bindgen]
pub fn remesh(positions: &[f32], faces: &[u32], edge: f64, iterations: Option<u32>) -> Result<RemeshResult, JsError> {
    match remesh_core(positions, faces, edge, iterations.unwrap_or(5)) {
        Ok((positions, faces)) => Ok(RemeshResult { positions, faces }),
        Err(msg) => Err(JsError::new(&msg)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(s: f32) -> (Vec<f32>, Vec<u32>) {
        let v = vec![0., 0., 0., s, 0., 0., s, s, 0., 0., s, 0., 0., 0., s, s, 0., s, s, s, s, 0., s, s];
        let f = vec![0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 1, 2, 6, 1, 6, 5, 2, 3, 7, 2, 7, 6, 3, 0, 4, 3, 4, 7];
        (v, f)
    }

    fn watertight(f: &[u32]) -> bool {
        let mut m: HashMap<(u32, u32), i32> = HashMap::new();
        for t in f.chunks(3) {
            for i in 0..3 {
                let (a, b) = (t[i], t[(i + 1) % 3]);
                *m.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
        m.values().all(|&n| n == 2)
    }

    fn volume(v: &[f32], f: &[u32]) -> f64 {
        let p = |i: u32| [v[i as usize * 3] as f64, v[i as usize * 3 + 1] as f64, v[i as usize * 3 + 2] as f64];
        f.chunks(3)
            .map(|t| {
                let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
                (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])) / 6.0
            })
            .sum()
    }

    #[test]
    fn cube_stays_closed_and_keeps_its_volume() {
        let (v, f) = cube(10.0);
        let (nv, nf) = remesh_core(&v, &f, 2.0, 5).unwrap();
        assert!(nf.len() / 3 > 200, "triangles: {}", nf.len() / 3);
        assert!(watertight(&nf));
        assert!((volume(&nv, &nf) - 1000.0).abs() < 1e-3, "volume {}", volume(&nv, &nf));
    }

    #[test]
    fn open_mesh_is_refused() {
        let (v, f) = cube(10.0);
        assert_eq!(remesh_core(&v, &f[..33], 2.0, 5).unwrap_err(), "Remesh needs a closed solid.");
        assert_eq!(remesh_core(&v, &f, 0.0, 5).unwrap_err(), "Edge length must be positive.");
    }
}
