// Which overhangs the support fins actually hold, and which are left bare.
//
// An overhang point counts as held when something holding it up is within `reach` mm: the top
// of a fin (a fin point within the support gap of the overhang), or a wall of the part going
// down from the overhang's edge (a ledge grows out from the wall layer by layer). Everything
// further away is bare: it would sag or fail. Plain arrays in, plain arrays out (no three.js),
// so the tests run it in Node.

const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const crs = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const len = a => Math.hypot(a[0], a[1], a[2]);

/** Distance from point p to triangle abc. */
export function pointTri(p, a, b, c) {
  const ab = sub(b, a), ac = sub(c, a), ap = sub(p, a);
  const d1 = dot(ab, ap), d2 = dot(ac, ap);
  if (d1 <= 0 && d2 <= 0) return len(ap);
  const bp = sub(p, b), d3 = dot(ab, bp), d4 = dot(ac, bp);
  if (d3 >= 0 && d4 <= d3) return len(bp);
  const vc = d1 * d4 - d3 * d2;
  if (vc <= 0 && d1 >= 0 && d3 <= 0) { const v = d1 / (d1 - d3); return len(sub(p, [a[0] + ab[0] * v, a[1] + ab[1] * v, a[2] + ab[2] * v])); }
  const cp = sub(p, c), d5 = dot(ab, cp), d6 = dot(ac, cp);
  if (d6 >= 0 && d5 <= d6) return len(cp);
  const vb = d5 * d2 - d1 * d6;
  if (vb <= 0 && d2 >= 0 && d6 <= 0) { const w = d2 / (d2 - d6); return len(sub(p, [a[0] + ac[0] * w, a[1] + ac[1] * w, a[2] + ac[2] * w])); }
  const va = d3 * d6 - d5 * d4;
  if (va <= 0 && d4 - d3 >= 0 && d5 - d6 >= 0) {
    const w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
    return len(sub(p, [b[0] + (c[0] - b[0]) * w, b[1] + (c[1] - b[1]) * w, b[2] + (c[2] - b[2]) * w]));
  }
  const n = crs(ab, ac), nl = len(n) || 1;
  return Math.abs(dot(ap, n)) / nl;
}

class Grid {
  constructor(cell) { this.c = cell; this.m = new Map(); }
  key(i, j, k) { return ((i * 73856093) ^ (j * 19349663) ^ (k * 83492791)) | 0; }
  cellOf(p) { return [Math.floor(p[0] / this.c), Math.floor(p[1] / this.c), Math.floor(p[2] / this.c)]; }
  add(i, j, k, v) { const key = this.key(i, j, k); let l = this.m.get(key); if (!l) this.m.set(key, l = []); l.push(v); }
  near(p, fn) {
    const [i, j, k] = this.cellOf(p);
    for (let a = -1; a <= 1; a++) for (let b = -1; b <= 1; b++) for (let d = -1; d <= 1; d++) {
      const l = this.m.get(this.key(i + a, j + b, k + d));
      if (l) for (const v of l) if (fn(v)) return true;
    }
    return false;
  }
}

/**
 * @param {object} o
 *   pos, idx: the part's mesh (flat arrays); over: the overhang triangles Support Fins found;
 *   fins: the fins as flat triangle-soup position arrays (9 numbers a triangle); gap: the support gap (mm); reach: how far from
 *   a support an overhang may be (mm).
 * @returns {{ total, held, tris: Float32Array, dist: Float32Array, spots: {area, span, far, at}[] }}
 *   (tris: the overhangs in small triangles; dist: each corner's distance to a support, mm;
 *   spots worst first; `far`: mm from the nearest support, null when it's more than 2.5 x reach)
 */
export function coverage({ pos, idx, over, fins = [], gap = 0.2, reach = 5, maxSub = 250000 }) {
  const P = i => [pos[i * 3], pos[i * 3 + 1], pos[i * 3 + 2]];
  const tri = f => [P(idx[f * 3]), P(idx[f * 3 + 1]), P(idx[f * 3 + 2])];
  const out = { total: 0, held: 0, tris: new Float32Array(0), bare: new Uint8Array(0), spots: [] };
  if (!over.length) return out;

  // --- what holds the overhangs up: fin tips touching them ...
  const touch = Math.max(0.8, gap + 0.6), cs = Math.max(2, touch * 2);
  const og = new Grid(cs);
  for (const f of over) {
    const [a, b, c] = tri(f);
    const lo = [0, 1, 2].map(k => Math.floor((Math.min(a[k], b[k], c[k]) - touch) / cs));
    const hi = [0, 1, 2].map(k => Math.floor((Math.max(a[k], b[k], c[k]) + touch) / cs));
    if ((hi[0] - lo[0] + 1) * (hi[1] - lo[1] + 1) * (hi[2] - lo[2] + 1) > 400000) continue;   // a vast face: skip rather than run out of memory
    for (let i = lo[0]; i <= hi[0]; i++) for (let j = lo[1]; j <= hi[1]; j++) for (let k = lo[2]; k <= hi[2]; k++) og.add(i, j, k, f);
  }
  const contacts = new Grid(reach);
  const seen = new Set();
  const qk = (p, q) => ((Math.round(p[0] / q) * 73856093) ^ (Math.round(p[1] / q) * 19349663) ^ (Math.round(p[2] / q) * 83492791)) | 0;
  const dq = Math.max(0.35, reach / 14);   // contacts closer than this together are one
  const addContact = p => {
    const key = qk(p, dq);
    if (seen.has(key)) return;
    seen.add(key);
    const [i, j, k] = contacts.cellOf(p); contacts.add(i, j, k, p);
  };
  // fins: walk the edges of every fin triangle near an overhang (a long wall's top edge has
  // vertices only at its ends, so the points in between are sampled)
  const tested = new Set(), fd = new Map();
  const face = f => {
    let d = fd.get(f);
    if (!d) {
      const [a, b, c] = tri(f), n = crs(sub(b, a), sub(c, a)), l = len(n) || 1;
      d = { a, b, c, n: [n[0] / l, n[1] / l, n[2] / l] }; d.d = dot(d.n, a); fd.set(f, d);
    }
    return d;
  };
  const test = p => {
    const key = qk(p, 0.25);
    if (tested.has(key)) return;
    tested.add(key);
    if (og.near(p, f => { const F = face(f); return Math.abs(dot(F.n, p) - F.d) <= touch && pointTri(p, F.a, F.b, F.c) <= touch; })) addContact(p);
  };
  const occupied = (lo, hi) => {
    for (let i = Math.floor(lo[0] / cs); i <= Math.floor(hi[0] / cs); i++) for (let j = Math.floor(lo[1] / cs); j <= Math.floor(hi[1] / cs); j++)
      for (let k = Math.floor(lo[2] / cs); k <= Math.floor(hi[2] / cs); k++) if (og.m.has(og.key(i, j, k))) return true;
    return false;
  };
  for (const fp of fins) {
    for (let v = 0; v + 8 < fp.length; v += 9) {
      const T = [[fp[v], fp[v + 1], fp[v + 2]], [fp[v + 3], fp[v + 4], fp[v + 5]], [fp[v + 6], fp[v + 7], fp[v + 8]]];
      const lo = [0, 1, 2].map(k => Math.min(T[0][k], T[1][k], T[2][k]) - touch), hi = [0, 1, 2].map(k => Math.max(T[0][k], T[1][k], T[2][k]) + touch);
      if ((hi[0] - lo[0]) * (hi[1] - lo[1]) * (hi[2] - lo[2]) < 1e5 && !occupied(lo, hi)) continue;
      for (let e = 0; e < 3; e++) {
        // fine steps near an overhang, big ones through empty space
        const a = T[e], b = T[(e + 1) % 3], L = len(sub(b, a));
        for (let d = 0; ; ) {
          const t = L > 0 ? Math.min(d / L, 1) : 1, p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
          const [ci, cj, ck] = og.cellOf(p);
          let busy = false;
          for (let x = -1; x <= 1 && !busy; x++) for (let y = -1; y <= 1 && !busy; y++) for (let z = -1; z <= 1 && !busy; z++) busy = og.m.has(og.key(ci + x, cj + y, ck + z));
          if (busy) test(p);
          if (t >= 1) break;
          d += busy ? 0.5 : cs * 0.5;
        }
      }
    }
  }
  // ... and the part's own walls going down from an overhang's edge
  const isOver = new Set(over), nv = pos.length / 3;
  const ek = (a, b) => (a < b ? a * nv + b : b * nv + a);
  const edgeOf = new Map();
  for (const f of over) for (let j = 0; j < 3; j++) { const a = idx[f * 3 + j], b = idx[f * 3 + (j + 1) % 3]; edgeOf.set(ek(a, b), [a, b]); }
  const nf = idx.length / 3;
  for (let f = 0; f < nf; f++) {
    if (isOver.has(f)) continue;
    for (let j = 0; j < 3; j++) {
      const a = idx[f * 3 + j], b = idx[f * 3 + (j + 1) % 3], e = edgeOf.get(ek(a, b));
      if (!e) continue;
      const third = P(idx[f * 3 + (j + 2) % 3]), pa = P(a), pb = P(b);
      if (third[2] > Math.min(pa[2], pb[2]) - 0.05) continue;   // the face goes up from the edge: it holds nothing
      const L = len(sub(pb, pa)), n = Math.max(1, Math.ceil(L / 0.5));
      for (let s = 0; s <= n; s++) { const t = s / n; addContact([pa[0] + (pb[0] - pa[0]) * t, pa[1] + (pb[1] - pa[1]) * t, pa[2] + (pb[2] - pa[2]) * t]); }
    }
  }
  // distance from a point to the nearest support, searched ring by ring through the contact
  // grid; anything beyond 2.5 x reach is just "far"
  const FAR = 2.5 * reach;
  const nearest = m => {
    const [ci, cj, ck] = contacts.cellOf(m);
    let best = Infinity;
    for (let k = 0; k <= 3; k++) {
      if (best <= (k - 1) * reach || (k === 3 && best <= FAR)) break;
      for (let a = -k; a <= k; a++) for (let b = -k; b <= k; b++) for (let d = -k; d <= k; d++) {
        if (Math.max(Math.abs(a), Math.abs(b), Math.abs(d)) !== k) continue;
        const l = contacts.m.get(contacts.key(ci + a, cj + b, ck + d));
        if (l) for (const q of l) { const x = Math.hypot(m[0] - q[0], m[1] - q[1], m[2] - q[2]); if (x < best) best = x; }
      }
    }
    return Math.min(best, FAR);
  };
  const memo = new Map(), mq = Math.max(0.15, reach / 33);
  const distAt = p => {
    const key = qk(p, mq);
    let v = memo.get(key);
    if (v === undefined) memo.set(key, v = nearest(p));
    return v;
  };

  // --- test the overhangs in small pieces (big flat faces can be partly held)
  let area = 0;
  for (const f of over) { const [a, b, c] = tri(f); area += len(crs(sub(b, a), sub(c, a))) / 2; }
  let h = Math.max(reach / 3, 0.5);
  if (area / (h * h * 0.43) > maxSub) h = Math.sqrt(area / (maxSub * 0.43));
  const tris = [], dist = [], bareC = [];
  const put = (p, q, r) => {
    const ar = len(crs(sub(q, p), sub(r, p))) / 2;
    const m = [(p[0] + q[0] + r[0]) / 3, (p[1] + q[1] + r[1]) / 3, (p[2] + q[2] + r[2]) / 3];
    out.total += ar;
    // each corner's distance is drawn, so the edge of a bare spot comes out smooth
    const dp = distAt(p), dq2 = distAt(q), dr = distAt(r), dm = distAt(m);
    if (dm <= reach) out.held += ar; else bareC.push([m, ar, dm]);
    tris.push(...p, ...q, ...r);
    dist.push(dp, dq2, dr);
  };
  // split each face at its longest edge until every piece is small (long thin faces stay
  // few pieces, unlike a regular grid)
  const h2 = h * h;
  for (const f of over) {
    const stack = [tri(f)];
    let guard = 0;
    while (stack.length) {
      const [p, q, r] = stack.pop();
      const e0 = len(sub(q, p)), e1 = len(sub(r, q)), e2 = len(sub(p, r)), L = Math.max(e0, e1, e2);
      if (L * L > h2 * 2 && guard++ < 200000) {
        const mid = (u, v) => [(u[0] + v[0]) / 2, (u[1] + v[1]) / 2, (u[2] + v[2]) / 2];
        if (L === e0) { const m = mid(p, q); stack.push([p, m, r], [m, q, r]); }
        else if (L === e1) { const m = mid(q, r); stack.push([p, q, m], [p, m, r]); }
        else { const m = mid(r, p); stack.push([p, q, m], [m, q, r]); }
        continue;
      }
      put(p, q, r);
    }
  }
  out.tris = new Float32Array(tris);
  out.dist = new Float32Array(dist);

  // --- bare spots: bare pieces next to each other
  const link = h * 2.2, g = new Grid(link), parent = bareC.map((_, i) => i);
  const find = x => { while (parent[x] !== x) x = parent[x] = parent[parent[x]]; return x; };
  bareC.forEach(([m], i) => {
    g.near(m, j => { if (Math.hypot(m[0] - bareC[j][0][0], m[1] - bareC[j][0][1], m[2] - bareC[j][0][2]) <= link) { const ra = find(i), rb = find(j); if (ra !== rb) parent[ra] = rb; } return false; });
    const [ci, cj, ck] = g.cellOf(m); g.add(ci, cj, ck, i);
  });
  const groups = new Map();
  bareC.forEach(([m, ar], i) => {
    const r = find(i);
    let s = groups.get(r);
    if (!s) groups.set(r, s = { area: 0, lo: [...m], hi: [...m], c: [0, 0, 0] });
    s.area += ar;
    for (let k = 0; k < 3; k++) { s.lo[k] = Math.min(s.lo[k], m[k]); s.hi[k] = Math.max(s.hi[k], m[k]); s.c[k] += m[k] * ar; }
  });
  // how far the worst point of each spot is from anything holding it up
  const far = new Map();
  bareC.forEach(([, , d], i) => { const r = find(i); far.set(r, Math.max(far.get(r) || 0, d)); });
  const minArea = Math.max(4, reach * reach / 2);   // slivers at corners don't count as spots
  out.spots = [...groups.entries()].filter(([, s]) => s.area >= minArea)
    .map(([r, s]) => ({ area: s.area, span: len(sub(s.hi, s.lo)) + h, far: contacts.m.size && far.get(r) < FAR ? far.get(r) : null, at: s.c.map(x => x / s.area) }))
    .sort((a, b) => (b.far ?? Infinity) - (a.far ?? Infinity) || b.area - a.area);
  return out;
}
