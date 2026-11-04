// Mesh geometry helpers (port of stleditor/geometry.py): vectors, bodies, mesh IO, weld and
// quick repair, face regions, plane frames.
import { Manifold, CrossSection, Mesh, manifoldOf } from './wasm.js';

export class GeometryError extends Error {}

export const EPS = 1e-9;

// ---------------------------------------------------------------- small vector helpers
export const add = (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
export const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
export const mul = (a, k) => [a[0] * k, a[1] * k, a[2] * k];
export const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
export const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
export const norm = a => Math.hypot(a[0], a[1], a[2]);
export function unit(a) {
  const n = norm(a);
  if (n < EPS) throw new GeometryError('Zero-length direction.');
  return [a[0] / n, a[1] / n, a[2] / n];
}
export const num = (v, def) => {
  if (v === undefined || v === null || v === '') {
    if (def === undefined) throw new GeometryError('Missing value.');
    return +def;
  }
  const x = +v;
  if (!Number.isFinite(x)) throw new GeometryError('A value must be a number.');
  return x;
};

// 3x3 rotation (rows) + translation -> column-major Mat4 for Manifold.transform
export function mat4(R, t = [0, 0, 0]) {
  return [R[0][0], R[1][0], R[2][0], 0, R[0][1], R[1][1], R[2][1], 0, R[0][2], R[1][2], R[2][2], 0, t[0], t[1], t[2], 1];
}
// local (u, v, w) -> world o + u x + v y + w n
export function frameMatrix(o, x, y, n) {
  return [x[0], x[1], x[2], 0, y[0], y[1], y[2], 0, n[0], n[1], n[2], 0, o[0], o[1], o[2], 1];
}
// world -> local of a frame
export function worldToFrame(o, x, y, n) {
  const R = [x, y, n];
  const t = [-dot(x, o), -dot(y, o), -dot(n, o)];
  return mat4(R, t);
}
export const matMul3 = (A, B) => A.map(r => [0, 1, 2].map(j => r[0] * B[0][j] + r[1] * B[1][j] + r[2] * B[2][j]));
export const matVec3 = (A, v) => A.map(r => r[0] * v[0] + r[1] * v[1] + r[2] * v[2]);
export const eye3 = () => [[1, 0, 0], [0, 1, 0], [0, 0, 1]];
export function det3(A) {
  return A[0][0] * (A[1][1] * A[2][2] - A[1][2] * A[2][1]) - A[0][1] * (A[1][0] * A[2][2] - A[1][2] * A[2][0]) + A[0][2] * (A[1][0] * A[2][1] - A[1][1] * A[2][0]);
}
// apply an affine [R | t] (R rows) to a Float32Array of xyz
export function transformVerts(v, R, t) {
  const out = new Float32Array(v.length);
  for (let i = 0; i < v.length; i += 3) {
    const x = v[i], y = v[i + 1], z = v[i + 2];
    out[i] = R[0][0] * x + R[0][1] * y + R[0][2] * z + t[0];
    out[i + 1] = R[1][0] * x + R[1][1] * y + R[1][2] * z + t[1];
    out[i + 2] = R[2][0] * x + R[2][1] * y + R[2][2] * z + t[2];
  }
  return out;
}

export function planeFrame(origin, normal, xdir = null) {
  const n = unit(normal);
  if (!xdir || norm(xdir) < EPS || Math.abs(dot(unit(xdir), n)) > 0.999) {
    if (Math.abs(n[2]) < 0.9) {
      const y = unit(sub([0, 0, 1], mul(n, n[2])));
      const x = cross(y, n);
      return [origin.map(Number), unit(x), y, n];
    }
    xdir = [1, 0, 0];
  }
  const x = unit(sub(xdir, mul(n, dot(xdir, n))));
  const y = cross(n, x);
  return [origin.map(Number), x, y, n];
}

export const BASE_PLANES = {
  XY: [[0, 0, 0], [0, 0, 1], [1, 0, 0]],
  XZ: [[0, 0, 0], [0, -1, 0], [1, 0, 0]],
  YZ: [[0, 0, 0], [1, 0, 0], [0, 1, 0]],
};

// ---------------------------------------------------------------- bodies
let SERIAL = 1;

export class Body {
  constructor(name, { solid = null, verts = null, faces = null } = {}) {
    this.name = name;
    this.serial = SERIAL++;
    this.solid = solid;
    this._v = verts;
    this._f = faces;
    this._cache = null;
  }
  get isSolid() { return this.solid !== null; }
  mesh() {
    if (!this._cache) {
      if (this.solid) {
        const m = this.solid.getMesh();
        const np = m.numProp;
        let v = m.vertProperties;
        if (np !== 3) {
          const nv = v.length / np;
          const o = new Float32Array(nv * 3);
          for (let i = 0; i < nv; i++) { o[i * 3] = v[i * np]; o[i * 3 + 1] = v[i * np + 1]; o[i * 3 + 2] = v[i * np + 2]; }
          v = o;
        }
        this._cache = { v: Float32Array.from(v), f: Uint32Array.from(m.triVerts) };
      } else this._cache = { v: this._v, f: this._f };
    }
    return this._cache;
  }
  requireSolid() {
    if (!this.solid) throw new GeometryError(`"${this.name}" is not a watertight solid. Run Repair on it first.`);
    return this.solid;
  }
  copy({ name, solid } = {}) {
    if (solid) return new Body(name || this.name, { solid });
    const b = new Body(name || this.name, { solid: this.solid, verts: this._v, faces: this._f });
    b._cache = this._cache;
    return b;
  }
  bbox() {
    if (this.solid) { const b = this.solid.boundingBox(); return [b.min, b.max]; }
    return vertsBBox(this.mesh().v);
  }
  info() {
    const { v, f } = this.mesh();
    const [lo, hi] = v.length ? vertsBBox(v) : [[0, 0, 0], [0, 0, 0]];
    const d = { name: this.name, solid: this.isSolid, triangles: f.length / 3, vertices: v.length / 3, bbox: [lo, hi], size: sub(hi, lo) };
    if (this.solid) { d.volume = this.solid.volume(); d.area = this.solid.surfaceArea(); d.genus = this.solid.genus(); }
    else d.area = meshArea(v, f);
    return d;
  }
}

export function vertsBBox(v) {
  const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
  for (let i = 0; i < v.length; i += 3) {
    for (let k = 0; k < 3; k++) { const x = v[i + k]; if (x < lo[k]) lo[k] = x; if (x > hi[k]) hi[k] = x; }
  }
  return [lo, hi];
}

function meshArea(v, f) {
  let a = 0;
  for (let i = 0; i < f.length; i += 3) {
    const p = f[i] * 3, q = f[i + 1] * 3, r = f[i + 2] * 3;
    const u = [v[q] - v[p], v[q + 1] - v[p + 1], v[q + 2] - v[p + 2]], w = [v[r] - v[p], v[r + 1] - v[p + 1], v[r + 2] - v[p + 2]];
    a += norm(cross(u, w)) / 2;
  }
  return a;
}

/** Manifold from flat arrays, or null if not a closed 2-manifold. */
export function manifoldFromArrays(v, f) {
  if (!f.length) return null;
  try {
    const mesh = new Mesh({ numProp: 3, vertProperties: v instanceof Float32Array ? v : Float32Array.from(v), triVerts: f instanceof Uint32Array ? f : Uint32Array.from(f) });
    mesh.merge();
    const m = manifoldOf(mesh);
    if (m.status() !== 'NoError' || m.isEmpty()) return null;
    return m;
  } catch { return null; }
}

/** Merge coincident vertices, drop degenerate and duplicate triangles. */
export function weld(v, f, tol = 1e-5) {
  const map = new Map();
  const nv = [];
  const remap = new Int32Array(v.length / 3);
  for (let i = 0; i < v.length / 3; i++) {
    const kx = Math.round(v[i * 3] / tol), ky = Math.round(v[i * 3 + 1] / tol), kz = Math.round(v[i * 3 + 2] / tol);
    const key = kx + ',' + ky + ',' + kz;
    let j = map.get(key);
    if (j === undefined) { j = nv.length / 3; map.set(key, j); nv.push(v[i * 3], v[i * 3 + 1], v[i * 3 + 2]); }
    remap[i] = j;
  }
  // triangles on the same three vertices: duplicates facing the same way collapse to one, and
  // pairs facing opposite ways (the shared wall of two touching shells) cancel out
  const groups = new Map(), tris = [];
  for (let i = 0; i < f.length; i += 3) {
    const a = remap[f[i]], b = remap[f[i + 1]], c = remap[f[i + 2]];
    if (a === b || b === c || a === c) continue;
    const lo = Math.min(a, b, c), hi = Math.max(a, b, c), mid = a + b + c - lo - hi;
    const key = lo + ',' + mid + ',' + hi;
    // even permutation of (lo, mid, hi) = same orientation
    const sign = (a === lo && b === mid) || (b === lo && c === mid) || (c === lo && a === mid) ? 1 : -1;
    let g = groups.get(key);
    if (!g) { g = { bal: 0, first: { 1: -1, '-1': -1 } }; groups.set(key, g); }
    g.bal += sign;
    if (g.first[sign] < 0) g.first[sign] = tris.length / 3;
    tris.push(a, b, c);
  }
  const keep = [];
  for (const g of groups.values()) if (g.bal) keep.push(g.first[Math.sign(g.bal)]);
  keep.sort((x, y) => x - y);
  const nf = new Uint32Array(keep.length * 3);
  keep.forEach((t, i) => { nf[i * 3] = tris[t * 3]; nf[i * 3 + 1] = tris[t * 3 + 1]; nf[i * 3 + 2] = tris[t * 3 + 2]; });
  return { v: new Float32Array(nv), f: nf };
}

// ---------------------------------------------------------------- quick repair
function edgeKey(a, b) { return a < b ? a * 4294967296 + b : b * 4294967296 + a; }

/** Fix small defects without remeshing: drop faces on non-manifold edges, unify winding,
 *  cap holes with fans, fix inside-out shells. Returns a Manifold or null. */
export function localRepair(v0, f0) {
  let { v, f } = weld(v0, f0);
  const touching = touchingShells(v, f);
  if (touching) return touching;
  for (let iter = 0; iter < 3; iter++) {
    // faces on edges shared by more than two triangles go
    const cnt = new Map();
    for (let i = 0; i < f.length; i += 3) for (let k = 0; k < 3; k++) {
      const key = edgeKey(f[i + k], f[i + (k + 1) % 3]);
      cnt.set(key, (cnt.get(key) || 0) + 1);
    }
    const keep = [];
    for (let i = 0; i < f.length; i += 3) {
      let bad = false;
      for (let k = 0; k < 3; k++) if (cnt.get(edgeKey(f[i + k], f[i + (k + 1) % 3])) > 2) bad = true;
      if (!bad) keep.push(f[i], f[i + 1], f[i + 2]);
    }
    f = fixWinding(new Uint32Array(keep));
    const capped = capHoles(v, f);
    v = capped.v; f = capped.f;
    let man = manifoldFromArrays(v, f);
    if (man) {
      if (man.volume() < 0) man = manifoldFromArrays(v, flip(f));
      if (man) return mergeOverlappingShells(man).man;
    }
  }
  // last try: shell by shell, union of the shells that are (or became) closed
  const solids = [];
  for (const sh of shells(v, f)) {
    let m = manifoldFromArrays(sh.v, sh.f);
    if (!m) { const c = capHoles(sh.v, sh.f); m = manifoldFromArrays(c.v, c.f); }
    if (!m) return null;
    if (m.volume() < 0) m = manifoldFromArrays(sh.v, flip(sh.f));
    if (!m) return null;
    solids.push(m);
  }
  if (!solids.length) return null;
  return solids.length > 1 ? Manifold.union(solids) : solids[0];
}

/** Shells that touch each other (sharing a wall or an edge) make edges with more than two
 *  triangles. Split the mesh where that happens, close each piece and union the solid ones;
 *  flat leftovers (a shared wall, a stray fin) are dropped. Returns a Manifold or null. */
function touchingShells(v, f) {
  const nt = f.length / 3, edges = new Map();
  for (let t = 0; t < nt; t++) for (let k = 0; k < 3; k++) {
    const key = edgeKey(f[t * 3 + k], f[t * 3 + (k + 1) % 3]);
    const a = edges.get(key); if (a) a.push(t); else edges.set(key, [t]);
  }
  let crowded = false;
  for (const a of edges.values()) if (a.length > 2) { crowded = true; break; }
  if (!crowded) return null;
  const comp = new Int32Array(nt).fill(-1), pieces = [];
  for (let s = 0; s < nt; s++) {
    if (comp[s] >= 0) continue;
    const id = pieces.length, tris = [s], stack = [s];
    comp[s] = id;
    while (stack.length) {
      const t = stack.pop();
      for (let k = 0; k < 3; k++) {
        const a = edges.get(edgeKey(f[t * 3 + k], f[t * 3 + (k + 1) % 3]));
        if (a.length !== 2) continue;
        for (const u of a) if (comp[u] < 0) { comp[u] = id; tris.push(u); stack.push(u); }
      }
    }
    pieces.push(tris);
  }
  const [lo, hi] = vertsBBox(v), scale = Math.max(hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]) ** 3;
  const solids = [];
  for (const tris of pieces) {
    const remap = new Map(), vs = [], fs = [];
    for (const t of tris) for (let k = 0; k < 3; k++) {
      const w = f[t * 3 + k];
      let j = remap.get(w);
      if (j === undefined) { j = vs.length / 3; remap.set(w, j); vs.push(v[w * 3], v[w * 3 + 1], v[w * 3 + 2]); }
      fs.push(j);
    }
    const pv = new Float32Array(vs);
    let pf = fixWinding(new Uint32Array(fs));
    let m = manifoldFromArrays(pv, pf);
    if (!m) { const c = capHoles(pv, pf); pf = c.f; m = manifoldFromArrays(c.v, c.f); if (m && m.volume() < 0) m = manifoldFromArrays(c.v, flip(c.f)); }
    else if (m.volume() < 0) m = manifoldFromArrays(pv, flip(pf));
    if (!m) {
      // a flat piece can't be closed; anything else means this isn't a case of touching shells
      if (isFlat(pv)) continue;
      return null;
    }
    if (Math.abs(m.volume()) <= 1e-6 * scale) continue;
    solids.push(m);
  }
  if (!solids.length) return null;
  return solids.length > 1 ? Manifold.union(solids) : solids[0];
}

function isFlat(v) {
  const [lo, hi] = vertsBBox(v), size = Math.max(hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]) || 1;
  // plane through the first triangle with any area
  for (let i = 3; i + 3 < v.length; i += 3) {
    const u = [v[i] - v[0], v[i + 1] - v[1], v[i + 2] - v[2]], w = [v[i + 3] - v[0], v[i + 4] - v[1], v[i + 5] - v[2]];
    const n = cross(u, w), l = norm(n);
    if (l < 1e-9 * size * size) continue;
    for (let j = 0; j < v.length; j += 3) if (Math.abs(((v[j] - v[0]) * n[0] + (v[j + 1] - v[1]) * n[1] + (v[j + 2] - v[2]) * n[2]) / l) > 1e-5 * size) return false;
    return true;
  }
  return true;
}

/** Split a mesh into its edge-connected pieces (like trimesh's split). */
export function shells(v, f) {
  const nt = f.length / 3, edges = new Map();
  for (let t = 0; t < nt; t++) for (let k = 0; k < 3; k++) {
    const key = edgeKey(f[t * 3 + k], f[t * 3 + (k + 1) % 3]);
    let a = edges.get(key); if (!a) edges.set(key, a = []); a.push(t);
  }
  const comp = new Int32Array(nt).fill(-1), out = [];
  for (let s = 0; s < nt; s++) {
    if (comp[s] >= 0) continue;
    const id = out.length, tris = [s], stack = [s];
    comp[s] = id;
    while (stack.length) {
      const t = stack.pop();
      for (let k = 0; k < 3; k++) for (const u of edges.get(edgeKey(f[t * 3 + k], f[t * 3 + (k + 1) % 3]))) {
        if (comp[u] < 0) { comp[u] = id; tris.push(u); stack.push(u); }
      }
    }
    const remap = new Map(), vs = [], fs = [];
    for (const t of tris) for (let k = 0; k < 3; k++) {
      const w = f[t * 3 + k];
      let j = remap.get(w);
      if (j === undefined) { j = vs.length / 3; remap.set(w, j); vs.push(v[w * 3], v[w * 3 + 1], v[w * 3 + 2]); }
      fs.push(j);
    }
    out.push({ v: new Float32Array(vs), f: new Uint32Array(fs) });
  }
  return out;
}

/** The desktop's second quick fix (trimesh's repairs): weld, unify winding, turn inside-out
 *  shells the right way round and fill holes of one triangle or quad. */
export function quickRepair(v0, f0) {
  let { v, f } = weld(v0, f0);
  f = fixWinding(f);
  // fix inversion shell by shell
  const parts = shells(v, f);
  const vs = [], fs = [];
  for (const sh of parts) {
    let vol = 0;
    for (let i = 0; i < sh.f.length; i += 3) {
      const a = sh.f[i] * 3, b = sh.f[i + 1] * 3, c = sh.f[i + 2] * 3, V = sh.v;
      vol += V[a] * (V[b + 1] * V[c + 2] - V[b + 2] * V[c + 1]) - V[a + 1] * (V[b] * V[c + 2] - V[b + 2] * V[c]) + V[a + 2] * (V[b] * V[c + 1] - V[b + 1] * V[c]);
    }
    const off = vs.length / 3, ff = vol < 0 ? flip(sh.f) : sh.f;
    vs.push(...sh.v); for (const i of ff) fs.push(i + off);
  }
  v = new Float32Array(vs); f = new Uint32Array(fs);
  // fill holes made of 3 or 4 edges
  const directed = new Set(), nxt = new Map();
  for (let i = 0; i < f.length; i += 3) for (let k = 0; k < 3; k++) directed.add(f[i + k] * 4294967296 + f[i + (k + 1) % 3]);
  for (let i = 0; i < f.length; i += 3) for (let k = 0; k < 3; k++) {
    const a = f[i + k], b = f[i + (k + 1) % 3];
    if (!directed.has(b * 4294967296 + a)) { const l = nxt.get(a); if (l) l.push(b); else nxt.set(a, [b]); }
  }
  const add = [], seen = new Set();
  for (const [a, outs] of nxt) {
    if (outs.length !== 1 || seen.has(a)) continue;
    const loop = [a];
    let cur = outs[0];
    while (cur !== a && loop.length <= 4) { const o = nxt.get(cur); if (!o || o.length !== 1) break; loop.push(cur); cur = o[0]; }
    if (cur !== a || loop.length < 3 || loop.length > 4) continue;
    for (const x of loop) seen.add(x);
    // boundary runs a -> b along existing faces, so the patch goes the other way
    if (loop.length === 3) add.push(loop[2], loop[1], loop[0]);
    else add.push(loop[3], loop[2], loop[1], loop[3], loop[1], loop[0]);
  }
  if (add.length) { const g = new Uint32Array(f.length + add.length); g.set(f); g.set(add, f.length); f = g; }
  let man = manifoldFromArrays(v, f);
  if (man && man.volume() < 0) man = manifoldFromArrays(v, flip(f));
  return man;
}

function flip(f) {
  const o = new Uint32Array(f.length);
  for (let i = 0; i < f.length; i += 3) { o[i] = f[i]; o[i + 1] = f[i + 2]; o[i + 2] = f[i + 1]; }
  return o;
}

/** Make neighbouring triangles agree on winding (per connected shell). */
export function fixWinding(f) {
  const nt = f.length / 3;
  const edges = new Map();  // undirected edge -> [tri, ...]
  for (let t = 0; t < nt; t++) for (let k = 0; k < 3; k++) {
    const key = edgeKey(f[t * 3 + k], f[t * 3 + (k + 1) % 3]);
    let a = edges.get(key); if (!a) edges.set(key, a = []); a.push(t);
  }
  const out = Uint32Array.from(f);
  const done = new Uint8Array(nt);
  const has = (t, a, b) => { for (let k = 0; k < 3; k++) if (out[t * 3 + k] === a && out[t * 3 + (k + 1) % 3] === b) return true; return false; };
  for (let s = 0; s < nt; s++) {
    if (done[s]) continue;
    done[s] = 1;
    const stack = [s];
    while (stack.length) {
      const t = stack.pop();
      for (let k = 0; k < 3; k++) {
        const a = out[t * 3 + k], b = out[t * 3 + (k + 1) % 3];
        for (const u of edges.get(edgeKey(a, b)) || []) {
          if (u === t || done[u]) continue;
          // a consistent neighbour has the edge as b -> a
          if (has(u, a, b)) { const x = out[u * 3 + 1]; out[u * 3 + 1] = out[u * 3 + 2]; out[u * 3 + 2] = x; }
          done[u] = 1;
          stack.push(u);
        }
      }
    }
  }
  return out;
}

/** Close every boundary loop with a fan around its centroid. */
export function capHoles(v, f) {
  const directed = new Set();
  for (let i = 0; i < f.length; i += 3) for (let k = 0; k < 3; k++) directed.add(f[i + k] + ',' + f[i + (k + 1) % 3]);
  const nxt = new Map();
  const boundary = [];
  for (let i = 0; i < f.length; i += 3) for (let k = 0; k < 3; k++) {
    const a = f[i + k], b = f[i + (k + 1) % 3];
    if (!directed.has(b + ',' + a)) { boundary.push([a, b]); let l = nxt.get(a); if (!l) nxt.set(a, l = []); l.push(b); }
  }
  if (!boundary.length) return { v, f, loops: 0 };
  const nv = Array.from(v), nf = Array.from(f);
  const used = new Set();
  let loops = 0;
  for (const [a0, b0] of boundary) {
    if (used.has(a0 + ',' + b0)) continue;
    const loop = [a0];
    used.add(a0 + ',' + b0);
    let cur = b0, ok = true;
    while (cur !== a0) {
      loop.push(cur);
      const outs = (nxt.get(cur) || []).filter(x => !used.has(cur + ',' + x));
      if (!outs.length || loop.length > 200000) { ok = false; break; }
      used.add(cur + ',' + outs[0]);
      cur = outs[0];
    }
    if (!ok || loop.length < 3) continue;
    const c = [0, 0, 0];
    for (const i of loop) { c[0] += v[i * 3]; c[1] += v[i * 3 + 1]; c[2] += v[i * 3 + 2]; }
    const ci = nv.length / 3;
    nv.push(c[0] / loop.length, c[1] / loop.length, c[2] / loop.length);
    for (let i = 0; i < loop.length; i++) nf.push(loop[(i + 1) % loop.length], loop[i], ci);
    loops++;
  }
  return { v: new Float32Array(nv), f: new Uint32Array(nf), loops };
}

/** Several closed shells pushed into each other: union them into one clean solid. */
export function mergeOverlappingShells(man) {
  const parts = man.decompose();
  if (parts.length < 2 || parts.some(p => p.volume() <= 0)) return { man, merged: 0 };
  const total = parts.reduce((s, p) => s + p.volume(), 0);
  const merged = Manifold.union(parts);
  if (merged.volume() < total * (1 - 1e-6)) return { man: merged, merged: parts.length };
  return { man, merged: 0 };
}

// ---------------------------------------------------------------- faces and regions
export function triNormals(v, f) {
  const nt = f.length / 3;
  const n = new Float64Array(nt * 3), area = new Float64Array(nt);
  for (let t = 0; t < nt; t++) {
    const p = f[t * 3] * 3, q = f[t * 3 + 1] * 3, r = f[t * 3 + 2] * 3;
    const ux = v[q] - v[p], uy = v[q + 1] - v[p + 1], uz = v[q + 2] - v[p + 2];
    const wx = v[r] - v[p], wy = v[r + 1] - v[p + 1], wz = v[r + 2] - v[p + 2];
    const cx = uy * wz - uz * wy, cy = uz * wx - ux * wz, cz = ux * wy - uy * wx;
    const L = Math.hypot(cx, cy, cz);
    area[t] = L / 2;
    if (L > 0) { n[t * 3] = cx / L; n[t * 3 + 1] = cy / L; n[t * 3 + 2] = cz / L; }
  }
  return { n, area };
}

/** Triangle adjacency through shared edges: Map(tri -> [neighbour tris]) built lazily. */
export function triAdjacency(f) {
  const nt = f.length / 3;
  const edges = new Map();
  for (let t = 0; t < nt; t++) for (let k = 0; k < 3; k++) {
    const key = edgeKey(f[t * 3 + k], f[t * 3 + (k + 1) % 3]);
    const e = edges.get(key);
    if (e === undefined) edges.set(key, t);
    else if (Array.isArray(e)) e.push(t);
    else edges.set(key, [e, t]);
  }
  const adj = Array.from({ length: nt }, () => []);
  for (const e of edges.values()) {
    if (!Array.isArray(e)) continue;
    for (let i = 0; i < e.length; i++) for (let j = i + 1; j < e.length; j++) { adj[e[i]].push(e[j]); adj[e[j]].push(e[i]); }
  }
  return adj;
}

/** Grow the coplanar region around triangle `tri`. Returns {region, normal, centroid}. */
export function faceRegion(v, f, tri, angleTolDeg = 1.0) {
  const nt = f.length / 3;
  if (tri < 0 || tri >= nt) throw new GeometryError('No face there.');
  const { n, area } = triNormals(v, f);
  const [lo, hi] = vertsBBox(v);
  const distTol = Math.max(norm(sub(hi, lo)) * 1e-5, 1e-5);
  const n0 = [n[tri * 3], n[tri * 3 + 1], n[tri * 3 + 2]];
  const p0 = f[tri * 3] * 3;
  const d0 = n0[0] * v[p0] + n0[1] * v[p0 + 1] + n0[2] * v[p0 + 2];
  const cosT = Math.cos(angleTolDeg * Math.PI / 180);
  const ok = t => {
    if (n[t * 3] * n0[0] + n[t * 3 + 1] * n0[1] + n[t * 3 + 2] * n0[2] < cosT) return false;
    for (let k = 0; k < 3; k++) {
      const p = f[t * 3 + k] * 3;
      if (Math.abs(n0[0] * v[p] + n0[1] * v[p + 1] + n0[2] * v[p + 2] - d0) >= distTol) return false;
    }
    return true;
  };
  const adj = triAdjacency(f);
  const seen = new Uint8Array(nt);
  const region = [];
  const stack = [tri];
  seen[tri] = 1;
  while (stack.length) {
    const t = stack.pop();
    region.push(t);
    for (const u of adj[t]) if (!seen[u]) { seen[u] = 1; if (ok(u)) stack.push(u); }
  }
  let nx = 0, ny = 0, nz = 0, cx = 0, cy = 0, cz = 0, W = 0;
  for (const t of region) {
    const w = area[t];
    nx += n[t * 3] * w; ny += n[t * 3 + 1] * w; nz += n[t * 3 + 2] * w;
    for (let k = 0; k < 3; k++) { const p = f[t * 3 + k] * 3; cx += v[p] * w / 3; cy += v[p + 1] * w / 3; cz += v[p + 2] * w / 3; }
    W += w;
  }
  const normal = unit([nx, ny, nz]);
  const W2 = Math.max(W, EPS);
  return { region, normal, centroid: [cx / W2, cy / W2, cz / W2] };
}

/** Directed boundary edges of a triangle region, as vertex pairs. */
export function regionBoundary(f, region) {
  const cnt = new Map();
  for (const t of region) for (let k = 0; k < 3; k++) {
    const key = edgeKey(f[t * 3 + k], f[t * 3 + (k + 1) % 3]);
    cnt.set(key, (cnt.get(key) || 0) + 1);
  }
  const out = [];
  for (const t of region) for (let k = 0; k < 3; k++) {
    const a = f[t * 3 + k], b = f[t * 3 + (k + 1) % 3];
    if (cnt.get(edgeKey(a, b)) === 1) out.push([a, b]);
  }
  return out;
}

/** Cylinder or cone from point p0 along direction for `length`. */
export function makeCylinder(p0, direction, length, r0, r1 = null, segments = 96) {
  const c = Manifold.cylinder(+length, +r0, r1 === null ? +r0 : +r1, segments);
  const [o, x, y, n] = planeFrame(p0, direction);
  return c.transform(frameMatrix(o, x, y, n));
}

export function bigExtent(bodies) {
  const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
  for (const b of bodies) {
    const [l, h] = b.bbox();
    for (let k = 0; k < 3; k++) { lo[k] = Math.min(lo[k], l[k]); hi[k] = Math.max(hi[k], h[k]); }
  }
  if (!Number.isFinite(lo[0])) return 1000;
  return norm(sub(hi, lo)) * 2 + 10 + Math.max(...lo.map(Math.abs), ...hi.map(Math.abs));
}

export { Manifold, CrossSection };
