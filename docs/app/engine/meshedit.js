// Sculpt brushes and Remesh for the web edition (stleditor/printing.py has the desktop versions).
import { GeometryError, manifoldFromArrays } from './geom.js';

function meshArrays(solid) {
  const m = solid.getMesh(), np = m.numProp, nv = m.vertProperties.length / np;
  const v = new Float64Array(nv * 3);
  for (let i = 0; i < nv; i++) { v[i * 3] = m.vertProperties[i * np]; v[i * 3 + 1] = m.vertProperties[i * np + 1]; v[i * 3 + 2] = m.vertProperties[i * np + 2]; }
  return { v, f: Uint32Array.from(m.triVerts) };
}

/** Unique neighbours of every vertex, as CSR arrays. */
function adjacency(f, nv) {
  const deg = new Int32Array(nv);
  for (let i = 0; i < f.length; i++) deg[f[i]] += 2;
  const start = new Int32Array(nv + 1);
  for (let i = 0; i < nv; i++) start[i + 1] = start[i] + deg[i];
  const nb = new Int32Array(start[nv]), fill = start.slice(0, nv);
  for (let t = 0; t < f.length; t += 3) for (let e = 0; e < 3; e++) {
    const a = f[t + e], b = f[t + (e + 1) % 3];
    nb[fill[a]++] = b; nb[fill[b]++] = a;
  }
  // dedupe each list in place
  const s2 = new Int32Array(nv + 1), out = new Int32Array(nb.length);
  let w = 0;
  for (let i = 0; i < nv; i++) {
    s2[i] = w;
    const list = Array.from(nb.subarray(start[i], start[i + 1])).sort((x, y) => x - y);
    for (let k = 0; k < list.length; k++) if (k === 0 || list[k] !== list[k - 1]) out[w++] = list[k];
  }
  s2[nv] = w;
  return { start: s2, nb: out.subarray(0, w) };
}

function vertexNormals(v, f) {
  const vn = new Float64Array(v.length);
  for (let t = 0; t < f.length; t += 3) {
    const a = f[t] * 3, b = f[t + 1] * 3, c = f[t + 2] * 3;
    const ux = v[b] - v[a], uy = v[b + 1] - v[a + 1], uz = v[b + 2] - v[a + 2];
    const wx = v[c] - v[a], wy = v[c + 1] - v[a + 1], wz = v[c + 2] - v[a + 2];
    const nx = uy * wz - uz * wy, ny = uz * wx - ux * wz, nz = ux * wy - uy * wx;
    for (const p of [a, b, c]) { vn[p] += nx; vn[p + 1] += ny; vn[p + 2] += nz; }
  }
  for (let i = 0; i < vn.length; i += 3) {
    const l = Math.hypot(vn[i], vn[i + 1], vn[i + 2]);
    if (l > 0) { vn[i] /= l; vn[i + 1] /= l; vn[i + 2] /= l; }
  }
  return vn;
}

function resample(points, spacing) {
  const P = (points || []).map(p => [+p[0], +p[1], +p[2]]);
  if (P.length < 2) return P;
  const out = [P[0]];
  for (let s = 0; s < P.length - 1; s++) {
    const a = P[s], b = P[s + 1], L = Math.hypot(b[0] - a[0], b[1] - a[1], b[2] - a[2]);
    const k = Math.floor(L / spacing);
    for (let i = 1; i <= k; i++) { const t = i * spacing / L; out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]); }
    out.push(b);
  }
  return out;
}

/** Uniform hash grid over points for radius queries. */
class PointGrid {
  constructor(pts, count, h) {
    this.h = h; this.map = new Map(); this.pts = pts;
    for (let i = 0; i < count; i++) {
      const key = this.key(Math.floor(pts[i * 3] / h), Math.floor(pts[i * 3 + 1] / h), Math.floor(pts[i * 3 + 2] / h));
      let l = this.map.get(key); if (!l) this.map.set(key, l = []); l.push(i);
    }
  }
  key(i, j, k) { return ((i + 1048576) * 2097152 + (j + 1048576)) * 2097152 + (k + 1048576); }
  /** calls fn(index, distance²) for points within r of (x, y, z); r must be <= h */
  near(x, y, z, r, fn) {
    const h = this.h, ci = Math.floor(x / h), cj = Math.floor(y / h), ck = Math.floor(z / h), r2 = r * r, P = this.pts;
    for (let di = -1; di <= 1; di++) for (let dj = -1; dj <= 1; dj++) for (let dk = -1; dk <= 1; dk++) {
      const l = this.map.get(this.key(ci + di, cj + dj, ck + dk));
      if (!l) continue;
      for (const i of l) {
        const d2 = (P[i * 3] - x) ** 2 + (P[i * 3 + 1] - y) ** 2 + (P[i * 3 + 2] - z) ** 2;
        if (d2 <= r2) fn(i, d2);
      }
    }
  }
}

function eigSym3(A) {
  // Jacobi rotations; returns {values, vectors (columns)}
  const a = A.map(r => r.slice()), V = [[1, 0, 0], [0, 1, 0], [0, 0, 1]];
  for (let sweep = 0; sweep < 50; sweep++) {
    let off = 0;
    for (let p = 0; p < 3; p++) for (let q = p + 1; q < 3; q++) off += a[p][q] ** 2;
    if (off < 1e-30) break;
    for (let p = 0; p < 3; p++) for (let q = p + 1; q < 3; q++) {
      if (Math.abs(a[p][q]) < 1e-300) continue;
      const th = (a[q][q] - a[p][p]) / (2 * a[p][q]);
      const t = Math.sign(th || 1) / (Math.abs(th) + Math.sqrt(th * th + 1)), c = 1 / Math.sqrt(t * t + 1), s = t * c;
      for (let k = 0; k < 3; k++) { const x = a[k][p], y = a[k][q]; a[k][p] = c * x - s * y; a[k][q] = s * x + c * y; }
      for (let k = 0; k < 3; k++) { const x = a[p][k], y = a[q][k]; a[p][k] = c * x - s * y; a[q][k] = s * x + c * y; }
      for (let k = 0; k < 3; k++) { const x = V[k][p], y = V[k][q]; V[k][p] = c * x - s * y; V[k][q] = s * x + c * y; }
    }
  }
  return { values: [a[0][0], a[1][1], a[2][2]], vectors: V };
}

export function sculpt(solid, strokes) {
  const { v, f } = meshArrays(solid);
  const nv = v.length / 3;
  const { start, nb } = adjacency(f, nv);
  const mark = new Int32Array(nv).fill(-1);
  let strokeNo = 0;
  const laplace = (idx, w, rate, iters) => {
    const lap = new Float64Array(idx.length * 3);
    for (let it = 0; it < iters; it++) {
      for (let s = 0; s < idx.length; s++) {
        const i = idx[s], a = start[i], b = start[i + 1], d = Math.max(b - a, 1);
        let x = 0, y = 0, z = 0;
        for (let e = a; e < b; e++) { const j = nb[e] * 3; x += v[j]; y += v[j + 1]; z += v[j + 2]; }
        lap[s * 3] = x / d - v[i * 3]; lap[s * 3 + 1] = y / d - v[i * 3 + 1]; lap[s * 3 + 2] = z / d - v[i * 3 + 2];
      }
      for (let s = 0; s < idx.length; s++) {
        const i = idx[s] * 3, k = rate * w[s];
        v[i] += k * lap[s * 3]; v[i + 1] += k * lap[s * 3 + 1]; v[i + 2] += k * lap[s * 3 + 2];
      }
    }
  };
  for (const st of strokes) {
    const r = +(st.radius ?? 5), k = Math.min(Math.max(+(st.strength ?? 0.5), 0), 1), mode = st.mode || 'smooth';
    if (!['smooth', 'flatten', 'inflate', 'deflate'].includes(mode)) throw new GeometryError(`Unknown brush ${mode}.`);
    const samples = resample(st.points, r / 4);
    if (!samples.length || !(r > 0)) continue;
    strokeNo++;
    const grid = new PointGrid(v, nv, r);
    const idx = [];
    for (const s of samples) grid.near(s[0], s[1], s[2], r, i => { if (mark[i] !== strokeNo) { mark[i] = strokeNo; idx.push(i); } });
    if (!idx.length) continue;
    idx.sort((x, y) => x - y);
    const sp = new Float64Array(samples.flat()), sg = new PointGrid(sp, samples.length, r);
    const w = new Float64Array(idx.length);
    for (let s = 0; s < idx.length; s++) {
      const i = idx[s] * 3;
      let best = Infinity;
      sg.near(v[i], v[i + 1], v[i + 2], r, (_, d2) => { if (d2 < best) best = d2; });
      const q = Math.min(Math.sqrt(best) / r, 1);
      w[s] = (1 - q * q) ** 2 * k;
    }
    if (mode === 'smooth') laplace(idx, w, 0.6, 4);
    else if (mode === 'flatten') {
      let ws = 0; const c = [0, 0, 0];
      for (let s = 0; s < idx.length; s++) { const i = idx[s] * 3; ws += w[s]; c[0] += v[i] * w[s]; c[1] += v[i + 1] * w[s]; c[2] += v[i + 2] * w[s]; }
      ws = Math.max(ws, 1e-12); c[0] /= ws; c[1] /= ws; c[2] /= ws;
      const C = [[0, 0, 0], [0, 0, 0], [0, 0, 0]];
      for (let s = 0; s < idx.length; s++) {
        const i = idx[s] * 3, d = [v[i] - c[0], v[i + 1] - c[1], v[i + 2] - c[2]];
        for (let a = 0; a < 3; a++) for (let b = 0; b < 3; b++) C[a][b] += w[s] * d[a] * d[b];
      }
      const { values, vectors } = eigSym3(C);
      const m = values.indexOf(Math.min(...values)), n = [vectors[0][m], vectors[1][m], vectors[2][m]];
      for (let s = 0; s < idx.length; s++) {
        const i = idx[s] * 3, dist = (v[i] - c[0]) * n[0] + (v[i + 1] - c[1]) * n[1] + (v[i + 2] - c[2]) * n[2];
        v[i] -= w[s] * dist * n[0]; v[i + 1] -= w[s] * dist * n[1]; v[i + 2] -= w[s] * dist * n[2];
      }
    } else {
      const vn = vertexNormals(v, f), sgn = mode === 'inflate' ? 1 : -1;
      for (let s = 0; s < idx.length; s++) {
        const i = idx[s] * 3, d = sgn * w[s] * r * 0.12;
        v[i] += d * vn[i]; v[i + 1] += d * vn[i + 1]; v[i + 2] += d * vn[i + 2];
      }
      laplace(idx, w, 0.3, 1);
    }
  }
  const out = manifoldFromArrays(Float32Array.from(v), f);
  if (!out) throw new GeometryError('The brush strokes folded the surface over itself. Use a smaller strength or radius.');
  return out;
}

// ---------------------------------------------------------------- remesh
// Isotropic remeshing (Botsch & Kobbelt 2004): split long edges, collapse short ones, flip edges
// towards valence 6, then relax vertices along the surface and project them back onto it. Sharp
// edges (over 40°) are kept: their vertices don't slide and their edges are never flipped.
const nxt = c => (c % 3 === 2 ? c - 2 : c + 1), prv = c => (c % 3 === 0 ? c + 2 : c - 1);

class TriMesh {
  constructor(v, f) {
    this.nV = v.length / 3; this.nT = f.length / 3;
    this.P = new Float64Array(Math.max(v.length * 2, 64)); this.P.set(v);
    this.V = new Int32Array(Math.max(f.length * 2, 64)); this.V.set(f);
    this.O = new Int32Array(this.V.length).fill(-1);
    this.dead = new Uint8Array(this.V.length / 3);
    this.VC = new Int32Array(this.P.length / 3).fill(-1);
    this.vdead = new Uint8Array(this.P.length / 3);
    this.feat = new Set();
    const m = new Map(), nv = this.nV;
    for (let c = 0; c < f.length; c++) {
      const a = f[nxt(c)], b = f[prv(c)];
      const o = m.get(b * nv + a);
      if (o !== undefined) { this.O[c] = o; this.O[o] = c; m.delete(b * nv + a); } else m.set(a * nv + b, c);
      this.VC[f[c]] = c;
    }
    if (m.size) throw new GeometryError('Remesh needs a closed solid.');
  }
  ekey(a, b) { return a < b ? a * 67108864 + b : b * 67108864 + a; }
  grow() {
    if ((this.nT + 2) * 3 > this.V.length) {
      const n = this.V.length * 2;
      const V = new Int32Array(n); V.set(this.V); this.V = V;
      const O = new Int32Array(n).fill(-1); O.set(this.O); this.O = O;
      const d = new Uint8Array(n / 3); d.set(this.dead); this.dead = d;
    }
    if ((this.nV + 1) * 3 > this.P.length) {
      const P = new Float64Array(this.P.length * 2); P.set(this.P); this.P = P;
      const VC = new Int32Array(this.P.length / 3).fill(-1); VC.set(this.VC); this.VC = VC;
      const vd = new Uint8Array(this.P.length / 3); vd.set(this.vdead); this.vdead = vd;
    }
  }
  ring(v) {   // corners at v, going around it
    const out = [], c0 = this.VC[v];
    let c = c0;
    do { out.push(c); c = nxt(this.O[nxt(c)]); if (out.length > 1000) throw new GeometryError('Remesh found a broken vertex.'); } while (c !== c0);
    return out;
  }
  neighbors(v) { return this.ring(v).map(c => this.V[nxt(c)]); }
  featCount(v) { let n = 0; for (const w of this.neighbors(v)) if (this.feat.has(this.ekey(v, w))) n++; return n; }
  len(a, b) { const P = this.P; return Math.hypot(P[a * 3] - P[b * 3], P[a * 3 + 1] - P[b * 3 + 1], P[a * 3 + 2] - P[b * 3 + 2]); }
  triNormal(a, b, c, pa) {   // normal of (a, b, c) with a optionally moved to pa
    const P = this.P, ax = pa ? pa[0] : P[a * 3], ay = pa ? pa[1] : P[a * 3 + 1], az = pa ? pa[2] : P[a * 3 + 2];
    const ux = P[b * 3] - ax, uy = P[b * 3 + 1] - ay, uz = P[b * 3 + 2] - az, wx = P[c * 3] - ax, wy = P[c * 3 + 1] - ay, wz = P[c * 3 + 2] - az;
    return [uy * wz - uz * wy, uz * wx - ux * wz, ux * wy - uy * wx];
  }
  /** re-link opposite corners among `tris` (alive, changed) and the corners outside them */
  relink(tris, outer) {
    const cs = [];
    for (const t of tris) cs.push(3 * t, 3 * t + 1, 3 * t + 2);
    const all = cs.concat(outer), V = this.V;
    for (const c of cs) {
      const a = V[nxt(c)], b = V[prv(c)];
      let found = -1;
      for (const o of all) if (V[nxt(o)] === b && V[prv(o)] === a) { found = o; break; }
      if (found < 0) throw new Error('remesh relink failed');
      this.O[c] = found; this.O[found] = c;
    }
    for (const c of cs) this.VC[V[c]] = c;
  }
  outerOf(tris) {
    const set = new Set(tris), out = [];
    for (const t of tris) for (let i = 0; i < 3; i++) { const o = this.O[3 * t + i]; if (!set.has(Math.floor(o / 3))) out.push(o); }
    return out;
  }
  split(c) {
    const d = this.O[c], V = this.V;
    const x = V[c], a = V[nxt(c)], b = V[prv(c)], y = V[d];
    this.grow();
    const P = this.P, m = this.nV++;
    for (let k = 0; k < 3; k++) P[m * 3 + k] = (P[a * 3 + k] + P[b * 3 + k]) / 2;
    this.vdead[m] = 0;
    const t1 = Math.floor(c / 3), t2 = Math.floor(d / 3), t3 = this.nT++, t4 = this.nT++;
    const outer = this.outerOf([t1, t2]);
    V.set([x, a, m], 3 * t1); V.set([x, m, b], 3 * t3); V.set([y, b, m], 3 * t2); V.set([y, m, a], 3 * t4);
    this.dead[t3] = 0; this.dead[t4] = 0;
    this.relink([t1, t2, t3, t4], outer);
    const k = this.ekey(a, b);
    if (this.feat.has(k)) { this.feat.delete(k); this.feat.add(this.ekey(a, m)); this.feat.add(this.ekey(m, b)); }
  }
  /** collapse the edge opposite corner c, removing vertex r and keeping k at position X */
  collapse(c, r, k, X, hi) {
    const d = this.O[c], V = this.V;
    const x = V[c], y = V[d];
    if (x === y) return false;
    const nr = this.neighbors(r), nk = this.neighbors(k);
    if (nr.length <= 3 || nk.length <= 3) return false;
    const setK = new Set(nk);
    let common = 0;
    for (const w of nr) if (setK.has(w)) common++;
    if (common !== 2) return false;
    if (this.neighbors(x).length <= 3 || this.neighbors(y).length <= 3) return false;
    // no flipped or stretched triangles around either end
    for (const [u, ring] of [[r, this.ring(r)], [k, this.ring(k)]]) {
      for (const cc of ring) {
        const b = V[nxt(cc)], e = V[prv(cc)];
        if (b === r || b === k || e === r || e === k) continue;
        const n0 = this.triNormal(u, b, e), n1 = this.triNormal(u, b, e, X);
        const l0 = Math.hypot(...n0), l1 = Math.hypot(...n1);
        if (l1 < 1e-12 || (n0[0] * n1[0] + n0[1] * n1[1] + n0[2] * n1[2]) < 0.2 * l0 * l1) return false;
        const P = this.P;
        if (Math.hypot(X[0] - P[b * 3], X[1] - P[b * 3 + 1], X[2] - P[b * 3 + 2]) > hi) return false;
      }
    }
    const tris = [...new Set(this.ring(r).map(cc => Math.floor(cc / 3)))];
    const outer = this.outerOf(tris);
    const t1 = Math.floor(c / 3), t2 = Math.floor(d / 3);
    for (const t of tris) for (let i = 0; i < 3; i++) if (V[3 * t + i] === r) V[3 * t + i] = k;
    this.dead[t1] = 1; this.dead[t2] = 1;
    const alive = tris.filter(t => t !== t1 && t !== t2);
    const outer2 = outer.filter(o => !this.dead[Math.floor(o / 3)]);
    // features: r's edges move to k
    const fr = nr.filter(w => this.feat.has(this.ekey(r, w)));
    for (const w of nr) this.feat.delete(this.ekey(r, w));
    for (const w of fr) if (w !== k) this.feat.add(this.ekey(k, w));
    this.relink(alive, outer2);
    this.P[k * 3] = X[0]; this.P[k * 3 + 1] = X[1]; this.P[k * 3 + 2] = X[2];
    this.vdead[r] = 1;
    return true;
  }
  flip(c) {
    const d = this.O[c], V = this.V;
    const x = V[c], a = V[nxt(c)], b = V[prv(c)], y = V[d];
    if (x === y || this.feat.has(this.ekey(a, b))) return false;
    const va = this.neighbors(a).length, vb = this.neighbors(b).length, vx = this.neighbors(x).length, vy = this.neighbors(y).length;
    if (va <= 3 || vb <= 3) return false;
    if (this.neighbors(x).includes(y)) return false;
    const dev = (p, q, r, s) => Math.abs(p - 6) + Math.abs(q - 6) + Math.abs(r - 6) + Math.abs(s - 6);
    if (dev(va - 1, vb - 1, vx + 1, vy + 1) >= dev(va, vb, vx, vy)) return false;
    // the new triangles must face the same way as the old ones
    const o1 = this.triNormal(x, a, b), o2 = this.triNormal(y, b, a), n1 = this.triNormal(x, a, y), n2 = this.triNormal(y, b, x);
    const on = [o1[0] + o2[0], o1[1] + o2[1], o1[2] + o2[2]];
    for (const nn of [n1, n2]) {
      const l = Math.hypot(...nn);
      if (l < 1e-12 || nn[0] * on[0] + nn[1] * on[1] + nn[2] * on[2] < 0.5 * l * Math.hypot(...on)) return false;
    }
    const t1 = Math.floor(c / 3), t2 = Math.floor(d / 3);
    const outer = this.outerOf([t1, t2]);
    V.set([x, a, y], 3 * t1); V.set([y, b, x], 3 * t2);
    this.relink([t1, t2], outer);
    return true;
  }
  edges() {   // one corner per edge, for alive triangles
    const out = [];
    for (let t = 0; t < this.nT; t++) if (!this.dead[t]) for (let i = 0; i < 3; i++) { const c = 3 * t + i; if (c < this.O[c]) out.push(c); }
    return out;
  }
}

/** Uniform grid of triangles for closest-point queries (the surface to project back onto). */
class TriGrid {
  constructor(v, f, h) {
    this.v = v; this.f = f; this.h = h; this.map = new Map();
    this.stamp = new Int32Array(f.length / 3).fill(-1); this.qid = 0;
    for (let t = 0; t < f.length / 3; t++) {
      const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
      for (let e = 0; e < 3; e++) for (let k = 0; k < 3; k++) { const x = v[f[t * 3 + e] * 3 + k]; lo[k] = Math.min(lo[k], x); hi[k] = Math.max(hi[k], x); }
      for (let i = Math.floor(lo[0] / h); i <= Math.floor(hi[0] / h); i++)
        for (let j = Math.floor(lo[1] / h); j <= Math.floor(hi[1] / h); j++)
          for (let k = Math.floor(lo[2] / h); k <= Math.floor(hi[2] / h); k++) {
            const key = this.key(i, j, k); let l = this.map.get(key); if (!l) this.map.set(key, l = []); l.push(t);
          }
    }
  }
  key(i, j, k) { return ((i + 1048576) * 2097152 + (j + 1048576)) * 2097152 + (k + 1048576); }
  /** nearest surface point to (x, y, z) among triangles within about h/2, into out */
  closest(x, y, z, out) {
    const h = this.h, gx = x / h - 0.5, gy = y / h - 0.5, gz = z / h - 0.5;
    const ci = Math.floor(gx), cj = Math.floor(gy), ck = Math.floor(gz);   // 2x2x2 block around the point
    const q = ++this.qid, v = this.v, f = this.f;
    let best = Infinity;
    for (let di = 0; di <= 1; di++) for (let dj = 0; dj <= 1; dj++) for (let dk = 0; dk <= 1; dk++) {
      const l = this.map.get(this.key(ci + di, cj + dj, ck + dk));
      if (!l) continue;
      for (let m = 0; m < l.length; m++) {
        const t = l[m];
        if (this.stamp[t] === q) continue;
        this.stamp[t] = q;
        const d2 = closestOnTri(x, y, z, v, f, t, CP);
        if (d2 < best) { best = d2; out[0] = CP[0]; out[1] = CP[1]; out[2] = CP[2]; }
      }
    }
    return best < Infinity;
  }
}

const CP = new Float64Array(3);
/** closest point of triangle t to p, into out; returns the squared distance */
function closestOnTri(px, py, pz, v, f, t, out) {
  const A = f[t * 3] * 3, B = f[t * 3 + 1] * 3, C = f[t * 3 + 2] * 3;
  const ax = v[A], ay = v[A + 1], az = v[A + 2];
  const abx = v[B] - ax, aby = v[B + 1] - ay, abz = v[B + 2] - az, acx = v[C] - ax, acy = v[C + 1] - ay, acz = v[C + 2] - az;
  const apx = px - ax, apy = py - ay, apz = pz - az;
  const d1 = abx * apx + aby * apy + abz * apz, d2 = acx * apx + acy * apy + acz * apz;
  let qx, qy, qz;
  if (d1 <= 0 && d2 <= 0) { qx = ax; qy = ay; qz = az; } else {
    const bpx = px - v[B], bpy = py - v[B + 1], bpz = pz - v[B + 2];
    const d3 = abx * bpx + aby * bpy + abz * bpz, d4 = acx * bpx + acy * bpy + acz * bpz;
    const cpx = px - v[C], cpy = py - v[C + 1], cpz = pz - v[C + 2];
    const d5 = abx * cpx + aby * cpy + abz * cpz, d6 = acx * cpx + acy * cpy + acz * cpz;
    const vc = d1 * d4 - d3 * d2, vb = d5 * d2 - d1 * d6, va = d3 * d6 - d5 * d4;
    if (d3 >= 0 && d4 <= d3) { qx = v[B]; qy = v[B + 1]; qz = v[B + 2]; }
    else if (vc <= 0 && d1 >= 0 && d3 <= 0) { const s = d1 / (d1 - d3); qx = ax + s * abx; qy = ay + s * aby; qz = az + s * abz; }
    else if (d6 >= 0 && d5 <= d6) { qx = v[C]; qy = v[C + 1]; qz = v[C + 2]; }
    else if (vb <= 0 && d2 >= 0 && d6 <= 0) { const s = d2 / (d2 - d6); qx = ax + s * acx; qy = ay + s * acy; qz = az + s * acz; }
    else if (va <= 0 && d4 - d3 >= 0 && d5 - d6 >= 0) {
      const s = (d4 - d3) / ((d4 - d3) + (d5 - d6));
      qx = v[B] + s * (v[C] - v[B]); qy = v[B + 1] + s * (v[C + 1] - v[B + 1]); qz = v[B + 2] + s * (v[C + 2] - v[B + 2]);
    } else {
      const den = 1 / (va + vb + vc), s = vb * den, w = vc * den;
      qx = ax + abx * s + acx * w; qy = ay + aby * s + acy * w; qz = az + abz * s + acz * w;
    }
  }
  out[0] = qx; out[1] = qy; out[2] = qz;
  return (px - qx) ** 2 + (py - qy) ** 2 + (pz - qz) ** 2;
}

export function remesh(solid, edge, iterations = 5) {
  if (!(edge > 0)) throw new GeometryError('Edge length must be positive.');
  const area = solid.surfaceArea();
  if (area / (0.433 * edge * edge) > 3e6) throw new GeometryError('That triangle size would make more than 3 million triangles. Use a bigger size.');
  const fine = solid.refineToLength(edge);
  const ref = meshArrays(fine);
  const M = new TriMesh(ref.v, ref.f);
  // sharp edges of the original shape
  {
    const V = M.V;
    for (const c of M.edges()) {
      const d = M.O[c], a = V[nxt(c)], b = V[prv(c)];
      const n1 = M.triNormal(V[c], a, b), n2 = M.triNormal(V[d], b, a);
      const l1 = Math.hypot(...n1), l2 = Math.hypot(...n2);
      if (l1 > 0 && l2 > 0 && (n1[0] * n2[0] + n1[1] * n2[1] + n1[2] * n2[2]) / (l1 * l2) < Math.cos(40 * Math.PI / 180)) M.feat.add(M.ekey(a, b));
    }
  }
  const grid = new TriGrid(ref.v, ref.f, edge);
  const hi = edge * 4 / 3, lo = edge * 4 / 5;
  for (let it = 0; it < iterations; it++) {
    // 1. split long edges
    for (const c of M.edges()) {
      if (M.dead[Math.floor(c / 3)]) continue;
      if (M.len(M.V[nxt(c)], M.V[prv(c)]) > hi) M.split(c);
    }
    // 2. collapse short edges
    for (const c of M.edges()) {
      if (M.dead[Math.floor(c / 3)]) continue;
      const a = M.V[nxt(c)], b = M.V[prv(c)];
      if (M.vdead[a] || M.vdead[b] || M.len(a, b) >= lo) continue;
      const fa = M.featCount(a), fb = M.featCount(b), onFeat = M.feat.has(M.ekey(a, b));
      const P = M.P;
      let r, k, X;
      if (!fa && !fb) { r = a; k = b; X = [(P[a * 3] + P[b * 3]) / 2, (P[a * 3 + 1] + P[b * 3 + 1]) / 2, (P[a * 3 + 2] + P[b * 3 + 2]) / 2]; }
      else if (fa && !fb) { r = b; k = a; }
      else if (fb && !fa) { r = a; k = b; }
      else if (onFeat && fa === 2) { r = a; k = b; }
      else if (onFeat && fb === 2) { r = b; k = a; }
      else continue;
      if (!X) X = [P[k * 3], P[k * 3 + 1], P[k * 3 + 2]];
      M.collapse(c, r, k, X, hi);
    }
    // 3. flip towards valence 6
    for (const c of M.edges()) if (!M.dead[Math.floor(c / 3)]) M.flip(c);
    // 4. tangential relaxation, then back onto the surface
    const n = M.nV, P = M.P, V = M.V, O = M.O, np = new Float64Array(n * 3), q = [0, 0, 0];
    const fixed = new Uint8Array(n);
    for (const key of M.feat) { fixed[Math.floor(key / 67108864)] = 1; fixed[key % 67108864] = 1; }
    for (let i = 0; i < n; i++) {
      const i3 = i * 3;
      np[i3] = P[i3]; np[i3 + 1] = P[i3 + 1]; np[i3 + 2] = P[i3 + 2];
      if (fixed[i] || M.vdead[i] || M.VC[i] < 0) continue;
      let cx = 0, cy = 0, cz = 0, nx = 0, ny = 0, nz = 0, cnt = 0;
      const c0 = M.VC[i];
      let c = c0;
      do {
        const w = V[nxt(c)] * 3, e = V[prv(c)] * 3;
        cx += P[w]; cy += P[w + 1]; cz += P[w + 2];
        const ux = P[w] - P[i3], uy = P[w + 1] - P[i3 + 1], uz = P[w + 2] - P[i3 + 2];
        const wx = P[e] - P[i3], wy = P[e + 1] - P[i3 + 1], wz = P[e + 2] - P[i3 + 2];
        nx += uy * wz - uz * wy; ny += uz * wx - ux * wz; nz += ux * wy - uy * wx;
        cnt++;
        c = nxt(O[nxt(c)]);
      } while (c !== c0 && cnt < 1000);
      cx /= cnt; cy /= cnt; cz /= cnt;
      const ln = Math.hypot(nx, ny, nz) || 1; nx /= ln; ny /= ln; nz /= ln;
      let dx = cx - P[i3], dy = cy - P[i3 + 1], dz = cz - P[i3 + 2];
      const dn = dx * nx + dy * ny + dz * nz;
      dx -= dn * nx; dy -= dn * ny; dz -= dn * nz;
      const tx = P[i3] + 0.6 * dx, ty = P[i3 + 1] + 0.6 * dy, tz = P[i3 + 2] + 0.6 * dz;
      if (grid.closest(tx, ty, tz, q)) { np[i3] = q[0]; np[i3 + 1] = q[1]; np[i3 + 2] = q[2]; }
      else { np[i3] = tx; np[i3 + 1] = ty; np[i3 + 2] = tz; }
    }
    P.set(np.subarray(0, n * 3));
  }
  // compact
  const remap = new Int32Array(M.nV).fill(-1), vs = [], fs = [];
  for (let t = 0; t < M.nT; t++) {
    if (M.dead[t]) continue;
    for (let i = 0; i < 3; i++) {
      const w = M.V[3 * t + i];
      if (remap[w] < 0) { remap[w] = vs.length / 3; vs.push(M.P[w * 3], M.P[w * 3 + 1], M.P[w * 3 + 2]); }
      fs.push(remap[w]);
    }
  }
  const out = manifoldFromArrays(Float32Array.from(vs), Uint32Array.from(fs));
  return out || fine;
}
