// 2D sketch entities -> profiles (port of stleditor/sketch.py, without Shapely: regions come
// from a small planar-arrangement builder here).
import { CrossSection, crossSection } from './wasm.js';
import { GeometryError } from './geom.js';

const segs = r => Math.floor(Math.min(256, Math.max(48, r * 10)));
const rad = d => d * Math.PI / 180;

function rotateRect(pts, e) {
  const ang = +(e.angle || 0);
  if (!ang) return pts;
  const cx = +e.x + +e.w / 2, cy = +e.y + +e.h / 2, a = rad(ang), c = Math.cos(a), s = Math.sin(a);
  return pts.map(([x, y]) => [cx + (x - cx) * c - (y - cy) * s, cy + (x - cx) * s + (y - cy) * c]);
}

export function arcPoints(e, n) {
  const cx = +e.cx, cy = +e.cy, r = +e.r;
  const a0 = rad(+e.a0), a1 = rad(+e.a1);
  let sweep = ((a1 - a0) % (2 * Math.PI) + 2 * Math.PI) % (2 * Math.PI);
  if (sweep < 1e-9) sweep = 2 * Math.PI;
  n = n || Math.max(8, Math.floor(segs(r) * sweep / (2 * Math.PI)));
  const out = [];
  for (let i = 0; i <= n; i++) { const a = a0 + sweep * i / n; out.push([cx + r * Math.cos(a), cy + r * Math.sin(a)]); }
  return out;
}

/** Closed contours (CCW) of a closed entity; [] for open / construction entities. */
export function entityContours(e) {
  const t = e.type;
  if (e.construction) return [];
  if (t === 'rect') {
    const x = +e.x, y = +e.y, w = +e.w, h = +e.h;
    if (Math.abs(w) < 1e-9 || Math.abs(h) < 1e-9) return [];
    const x0 = Math.min(x, x + w), x1 = Math.max(x, x + w), y0 = Math.min(y, y + h), y1 = Math.max(y, y + h);
    const r = Math.min(+(e.fillet || 0), (x1 - x0) / 2 - 1e-6, (y1 - y0) / 2 - 1e-6);
    let pts;
    if (r > 1e-6) {
      const n = Math.max(8, Math.floor(segs(r) / 4));
      pts = [];
      for (const [cx, cy, a0] of [[x1 - r, y0 + r, -90], [x1 - r, y1 - r, 0], [x0 + r, y1 - r, 90], [x0 + r, y0 + r, 180]])
        for (let i = 0; i <= n; i++) { const a = rad(a0 + 90 * i / n); pts.push([cx + r * Math.cos(a), cy + r * Math.sin(a)]); }
    } else pts = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    return [rotateRect(pts, e)];
  }
  if (t === 'circle') {
    const r = +e.r; if (r <= 0) return [];
    const n = segs(r), pts = [];
    for (let i = 0; i < n; i++) { const a = 2 * Math.PI * i / n; pts.push([+e.cx + r * Math.cos(a), +e.cy + r * Math.sin(a)]); }
    return [pts];
  }
  if (t === 'polygon') {
    const r = +e.r, n = Math.floor(+(e.n || 6));
    if (r <= 0 || n < 3) return [];
    const a0 = rad(+(e.angle || 0)), pts = [];
    for (let i = 0; i < n; i++) { const a = a0 + i * 2 * Math.PI / n; pts.push([+e.cx + r * Math.cos(a), +e.cy + r * Math.sin(a)]); }
    return [pts];
  }
  if (t === 'slot') {
    const x1 = +e.x1, y1 = +e.y1, x2 = +e.x2, y2 = +e.y2, r = +e.r;
    if (r <= 0) return [];
    const L = Math.hypot(x2 - x1, y2 - y1);
    if (L < 1e-9) return entityContours({ type: 'circle', cx: x1, cy: y1, r });
    const ang = Math.atan2(y2 - y1, x2 - x1), n = Math.max(12, Math.floor(segs(r) / 2)), pts = [];
    for (let i = 0; i <= n; i++) { const a = ang - Math.PI / 2 + Math.PI * i / n; pts.push([x2 + r * Math.cos(a), y2 + r * Math.sin(a)]); }
    for (let i = 0; i <= n; i++) { const a = ang + Math.PI / 2 + Math.PI * i / n; pts.push([x1 + r * Math.cos(a), y1 + r * Math.sin(a)]); }
    return [pts];
  }
  if (t === 'polyline') {
    const pts = (e.pts || []).map(p => [+p[0], +p[1]]);
    if (!e.closed || pts.length < 3) return [];
    return [pts];
  }
  return [];
}

/** {pts, closed} outline of any entity (for linework, picking and trimming). */
export function entityPath(e) {
  const t = e.type;
  if (t === 'line') return { pts: [e.p1.map(Number), e.p2.map(Number)], closed: false };
  if (t === 'arc') return { pts: arcPoints(e), closed: false };
  if (t === 'polyline') return { pts: (e.pts || []).map(p => [+p[0], +p[1]]), closed: !!e.closed };
  const c = entityContours({ ...e, construction: false });
  return c.length ? { pts: c[0], closed: true } : { pts: [], closed: false };
}

// ---------------------------------------------------------------- planar arrangement
const Q = 1e-6;
const qk = (x, y) => Math.round(x / Q) + ',' + Math.round(y / Q);

function segIntersect(a, b, c, d) {
  // returns [t, u] params or null
  const rx = b[0] - a[0], ry = b[1] - a[1], sx = d[0] - c[0], sy = d[1] - c[1];
  const den = rx * sy - ry * sx;
  if (Math.abs(den) < 1e-14) return null;
  const qx = c[0] - a[0], qy = c[1] - a[1];
  const t = (qx * sy - qy * sx) / den, u = (qx * ry - qy * rx) / den;
  if (t < -1e-9 || t > 1 + 1e-9 || u < -1e-9 || u > 1 + 1e-9) return null;
  return [Math.min(1, Math.max(0, t)), Math.min(1, Math.max(0, u))];
}

function linework(entities, only) {
  const segsOut = [];
  for (const e of entities) {
    if (e.construction || (only && !only.includes(e.id))) continue;
    const { pts, closed } = entityPath(e);
    if (pts.length < 2) continue;
    const n = pts.length;
    for (let i = 0; i < (closed ? n : n - 1); i++) {
      const a = pts[i], b = pts[(i + 1) % n];
      if (Math.hypot(b[0] - a[0], b[1] - a[1]) > 1e-9) segsOut.push([a, b]);
    }
  }
  return segsOut;
}

function nodeSegments(segList) {
  // split every segment at all its crossings (sweep over x)
  const cuts = segList.map(() => [0, 1]);
  const order = segList.map((s, i) => [Math.min(s[0][0], s[1][0]), Math.max(s[0][0], s[1][0]), i]).sort((p, q) => p[0] - q[0]);
  for (let i = 0; i < order.length; i++) {
    const [, maxi, si] = order[i];
    const s = segList[si];
    const ylo = Math.min(s[0][1], s[1][1]), yhi = Math.max(s[0][1], s[1][1]);
    for (let j = i + 1; j < order.length && order[j][0] <= maxi + 1e-9; j++) {
      const sj = order[j][2], o = segList[sj];
      if (Math.max(o[0][1], o[1][1]) < ylo - 1e-9 || Math.min(o[0][1], o[1][1]) > yhi + 1e-9) continue;
      const r = segIntersect(s[0], s[1], o[0], o[1]);
      if (r) { cuts[si].push(r[0]); cuts[sj].push(r[1]); }
      else {
        // collinear overlap: cut each at the other's endpoints when they lie on it
        for (const [A, B, list, P] of [[s[0], s[1], cuts[si], o], [o[0], o[1], cuts[sj], s]]) {
          const dx = B[0] - A[0], dy = B[1] - A[1], L2 = dx * dx + dy * dy;
          for (const p of P) {
            const t = ((p[0] - A[0]) * dx + (p[1] - A[1]) * dy) / L2;
            if (t <= 1e-9 || t >= 1 - 1e-9) continue;
            const ex = A[0] + dx * t - p[0], ey = A[1] + dy * t - p[1];
            if (ex * ex + ey * ey < 1e-14) list.push(t);
          }
        }
      }
    }
  }
  const out = [];
  segList.forEach((s, i) => {
    const ts = [...new Set(cuts[i].map(t => Math.round(t * 1e9) / 1e9))].sort((a, b) => a - b);
    for (let k = 0; k + 1 < ts.length; k++) {
      const a = [s[0][0] + (s[1][0] - s[0][0]) * ts[k], s[0][1] + (s[1][1] - s[0][1]) * ts[k]];
      const b = [s[0][0] + (s[1][0] - s[0][0]) * ts[k + 1], s[0][1] + (s[1][1] - s[0][1]) * ts[k + 1]];
      out.push([a, b]);
    }
  });
  return out;
}

const signedArea = p => { let a = 0; for (let i = 0; i < p.length; i++) { const q = p[(i + 1) % p.length]; a += p[i][0] * q[1] - q[0] * p[i][1]; } return a / 2; };

export function pointInRing(pt, ring) {
  let inside = false;
  for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
    const [xi, yi] = ring[i], [xj, yj] = ring[j];
    if ((yi > pt[1]) !== (yj > pt[1]) && pt[0] < (xj - xi) * (pt[1] - yi) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}

/** Minimal enclosed regions of the sketch: [{outer, holes, area}] (outer CCW, holes CW). */
export function faces(entities, only) {
  const lw = nodeSegments(linework(entities, only));
  if (!lw.length) return [];
  const verts = new Map(), P = [];
  const vid = p => { const k = qk(p[0], p[1]); let i = verts.get(k); if (i === undefined) { i = P.length; verts.set(k, i); P.push(p); } return i; };
  const edges = new Set(), adj = new Map();
  for (const [a, b] of lw) {
    const i = vid(a), j = vid(b);
    if (i === j) continue;
    const key = i < j ? i + ':' + j : j + ':' + i;
    if (edges.has(key)) continue;
    edges.add(key);
    (adj.get(i) || adj.set(i, new Set()).get(i)).add(j);
    (adj.get(j) || adj.set(j, new Set()).get(j)).add(i);
  }
  // drop dangling edges
  let changed = true;
  while (changed) {
    changed = false;
    for (const [i, s] of adj) if (s.size < 2) { for (const j of s) adj.get(j)?.delete(i); adj.delete(i); changed = true; }
  }
  if (!adj.size) return [];
  const sorted = new Map();
  for (const [i, s] of adj) {
    const p = P[i];
    sorted.set(i, [...s].sort((a, b) => Math.atan2(P[a][1] - p[1], P[a][0] - p[0]) - Math.atan2(P[b][1] - p[1], P[b][0] - p[0])));
  }
  const used = new Set(), cycles = [];
  for (const [i, nbrs] of sorted) for (const j of nbrs) {
    if (used.has(i + '>' + j)) continue;
    const cyc = [];
    let u = i, v = j, guard = 0;
    while (!used.has(u + '>' + v) && guard++ < 1e6) {
      used.add(u + '>' + v);
      cyc.push(P[u]);
      // at v, turn to the next edge clockwise from the way back (face stays on the left)
      const list = sorted.get(v);
      const k = list.indexOf(u);
      const w = list[(k - 1 + list.length) % list.length];
      u = v; v = w;
    }
    if (cyc.length >= 3) cycles.push(cyc);
  }
  const pos = [], neg = [];
  for (const c of cycles) { const a = signedArea(c); if (a > 1e-10) pos.push({ outer: c, holes: [], area: a }); else if (a < -1e-10) neg.push(c); }
  // a negative cycle is the outside of a connected piece; if it sits inside a face of
  // another piece, it is a hole of the smallest such face
  for (const c of neg) {
    const pt = c[0];
    let best = null;
    for (const f of pos) {
      if (f.outer.some(q => qk(q[0], q[1]) === qk(pt[0], pt[1]))) continue;
      if (pointInRing(pt, f.outer) && (!best || f.area < best.area)) best = f;
    }
    if (best) best.holes.push(c);
  }
  for (const f of pos) for (const h of f.holes) f.area -= Math.abs(signedArea(h));
  return pos.filter(f => f.area > 1e-10);
}

const faceContains = (f, pt) => pointInRing(pt, f.outer) && !f.holes.some(h => pointInRing(pt, h));

function csFromFaces(fs) {
  const contours = [];
  for (const f of fs) { contours.push(f.outer); for (const h of f.holes) contours.push(h); }
  if (!contours.length) throw new GeometryError('No closed region there.');
  // faces don't overlap, so even-odd of all their rings is their union
  return crossSection(contours, 'EvenOdd');
}

/** The area to extrude or revolve (CrossSection). */
export function profile(entities, only = null, regions = null) {
  let cs;
  if (regions && regions.length) {
    const fs = faces(entities, only);
    const picked = fs.filter(f => regions.some(([u, v]) => faceContains(f, [+u, +v])));
    if (!picked.length) throw new GeometryError('None of the picked points is inside a closed region of the sketch.');
    cs = csFromFaces(picked);
  } else {
    const contours = [];
    for (const e of entities) {
      if (only && !only.includes(e.id)) continue;
      contours.push(...entityContours(e));
    }
    const loose = entities.filter(e => e.type === 'line' || e.type === 'arc' || (e.type === 'polyline' && !e.closed));
    if (loose.length) for (const f of faces(loose, only)) contours.push(f.outer);
    if (!contours.length) throw new GeometryError('The sketch has no closed profile to use. Draw a rectangle, circle, polygon, slot, or a closed loop of lines and arcs.');
    cs = crossSection(contours, 'EvenOdd');
  }
  if (cs.isEmpty() || cs.area() <= 1e-12) throw new GeometryError('The sketch profile has no area.');
  return cs;
}

// ---------------------------------------------------------------- editing operations
const nid = (base, k) => `${base}_${k}`;

export function explode(e) {
  const t = e.type, c = { construction: !!e.construction };
  if (t === 'line' || t === 'arc' || t === 'circle') return [e];
  if (t === 'rect' && !+(e.fillet || 0) && !+(e.angle || 0)) {
    const x = +e.x, y = +e.y, w = +e.w, h = +e.h, P = [[x, y], [x + w, y], [x + w, y + h], [x, y + h]];
    return P.map((p, i) => ({ ...c, id: nid(e.id, i), type: 'line', p1: [...p], p2: [...P[(i + 1) % 4]] }));
  }
  if (t === 'slot') {
    const x1 = +e.x1, y1 = +e.y1, x2 = +e.x2, y2 = +e.y2, r = +e.r;
    const ang = Math.atan2(y2 - y1, x2 - x1) * 180 / Math.PI;
    const nx = -Math.sin(rad(ang)) * r, ny = Math.cos(rad(ang)) * r;
    return [{ ...c, id: nid(e.id, 0), type: 'line', p1: [x1 + nx, y1 + ny], p2: [x2 + nx, y2 + ny] },
      { ...c, id: nid(e.id, 1), type: 'line', p1: [x2 - nx, y2 - ny], p2: [x1 - nx, y1 - ny] },
      { ...c, id: nid(e.id, 2), type: 'arc', cx: x2, cy: y2, r, a0: ang - 90, a1: ang + 90 },
      { ...c, id: nid(e.id, 3), type: 'arc', cx: x1, cy: y1, r, a0: ang + 90, a1: ang + 270 }];
  }
  const { pts, closed } = entityPath(e);
  const n = pts.length, out = [];
  for (let i = 0; i < (closed ? n : n - 1); i++) out.push({ ...c, id: nid(e.id, i), type: 'line', p1: [...pts[i]], p2: [...pts[(i + 1) % n]] });
  return out;
}

function distToPath(pt, { pts, closed }) {
  let best = Infinity;
  const n = pts.length;
  for (let i = 0; i < (closed ? n : n - 1); i++) {
    const a = pts[i], b = pts[(i + 1) % n], dx = b[0] - a[0], dy = b[1] - a[1], L = dx * dx + dy * dy;
    let t = L ? ((pt[0] - a[0]) * dx + (pt[1] - a[1]) * dy) / L : 0;
    t = Math.max(0, Math.min(1, t));
    best = Math.min(best, Math.hypot(pt[0] - a[0] - t * dx, pt[1] - a[1] - t * dy));
  }
  return best;
}

function param(prim, pt) {
  if (prim.type === 'line') {
    const a = prim.p1, b = prim.p2, dx = b[0] - a[0], dy = b[1] - a[1];
    return ((pt[0] - a[0]) * dx + (pt[1] - a[1]) * dy) / Math.max(dx * dx + dy * dy, 1e-18);
  }
  return Math.atan2(pt[1] - +prim.cy, pt[0] - +prim.cx) * 180 / Math.PI;
}

function crossings(prim, others) {
  // points where `prim` (line, arc or circle) meets the outlines of `others`
  const out = [];
  const segsOf = o => { const { pts, closed } = entityPath(o); const s = []; for (let i = 0; i < (closed ? pts.length : pts.length - 1); i++) s.push([pts[i], pts[(i + 1) % pts.length]]); return s; };
  if (prim.type === 'line') {
    for (const o of others) for (const [c, d] of segsOf(o)) { const r = segIntersect(prim.p1, prim.p2, c, d); if (r) out.push(r[0]); }
    return out;
  }
  const cx = +prim.cx, cy = +prim.cy, R = +prim.r;
  for (const o of others) for (const [a, b] of segsOf(o)) {
    const dx = b[0] - a[0], dy = b[1] - a[1], fx = a[0] - cx, fy = a[1] - cy;
    const A = dx * dx + dy * dy, B = 2 * (fx * dx + fy * dy), C = fx * fx + fy * fy - R * R;
    const disc = B * B - 4 * A * C;
    if (disc < 0 || A < 1e-18) continue;
    for (const s of [-1, 1]) {
      const t = (-B + s * Math.sqrt(disc)) / (2 * A);
      if (t >= -1e-9 && t <= 1 + 1e-9) out.push(Math.atan2(a[1] + t * dy - cy, a[0] + t * dx - cx) * 180 / Math.PI);
      if (disc === 0) break;
    }
  }
  return out;
}

/** Remove the piece of entity `eid` under `point`, between its nearest crossings. */
export function trim(entities, eid, point) {
  const target = entities.find(e => e.id === eid);
  if (!target) throw new GeometryError('Nothing to trim there.');
  const others = entities.filter(e => e.id !== eid && !e.construction);
  const prims = explode(target);
  const pt = [+point[0], +point[1]];
  let gi = 0, bd = Infinity;
  prims.forEach((p, i) => { const d = distToPath(pt, entityPath(p)); if (d < bd) { bd = d; gi = i; } });
  const prim = prims[gi];
  const rest = prims.filter((_, i) => i !== gi);
  const cuts = crossings(prim, [...others, ...rest]);
  const tp = param(prim, pt);
  const out = [];
  if (prim.type === 'line') {
    const cs = [...new Set(cuts.filter(c => c > 1e-6 && c < 1 - 1e-6).map(c => Math.round(c * 1e9) / 1e9))].sort((a, b) => a - b);
    const lo = Math.max(0, ...cs.filter(c => c <= tp)), hi = Math.min(1, ...cs.filter(c => c >= tp));
    const a = prim.p1, b = prim.p2, at = t => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
    if (lo > 1e-6) out.push({ ...prim, id: prim.id + 'a', p1: [...a], p2: at(lo) });
    if (hi < 1 - 1e-6) out.push({ ...prim, id: prim.id + 'b', p1: at(hi), p2: [...b] });
  } else {
    const full = prim.type === 'circle';
    const a0 = full ? 0 : +prim.a0;
    const sweep = full ? 360 : (((+prim.a1 - a0) % 360 + 360) % 360) || 360;
    const rel = ang => ((ang - a0) % 360 + 360) % 360;
    let cs = full ? [...new Set(cuts.map(c => Math.round(rel(c) * 1e7) / 1e7 % 360))].sort((a, b) => a - b)
      : [...new Set(cuts.map(rel).filter(c => c > 1e-5 && c < sweep - 1e-5).map(c => Math.round(c * 1e7) / 1e7))].sort((a, b) => a - b);
    const t = rel(tp);
    const base = { cx: prim.cx, cy: prim.cy, r: prim.r, construction: !!prim.construction };
    if (full) {
      if (cs.length >= 2) {
        const below = cs.filter(c => c <= t), above = cs.filter(c => c >= t);
        const lo = below.length ? Math.max(...below) : cs[cs.length - 1] - 360;
        const hi = above.length ? Math.min(...above) : cs[0] + 360;
        out.push({ ...base, id: prim.id + 'a', type: 'arc', a0: a0 + hi, a1: a0 + lo + 360 });
      }
    } else {
      const below = cs.filter(c => c <= t), above = cs.filter(c => c >= t);
      const lo = below.length ? Math.max(...below) : 0, hi = above.length ? Math.min(...above) : sweep;
      if (lo > 1e-5) out.push({ ...base, id: prim.id + 'a', type: 'arc', a0, a1: a0 + lo });
      if (hi < sweep - 1e-5) out.push({ ...base, id: prim.id + 'b', type: 'arc', a0: a0 + hi, a1: a0 + sweep });
    }
  }
  const res = [];
  for (const e of entities) { if (e.id === eid) res.push(...rest, ...out); else res.push(e); }
  return res;
}

function chainLoose(entities) {
  // join open pieces end to end into polylines (closed when they come back)
  const pieces = entities.map(e => entityPath(e).pts.map(p => [...p])).filter(p => p.length >= 2);
  const chains = [];
  const close = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1]) < 1e-6;
  while (pieces.length) {
    let ch = pieces.shift();
    let grew = true;
    while (grew) {
      grew = false;
      for (let i = 0; i < pieces.length; i++) {
        const p = pieces[i];
        if (close(ch.at(-1), p[0])) ch = ch.concat(p.slice(1));
        else if (close(ch.at(-1), p.at(-1))) ch = ch.concat(p.slice(0, -1).reverse());
        else if (close(ch[0], p.at(-1))) ch = p.concat(ch.slice(1));
        else if (close(ch[0], p[0])) ch = p.slice().reverse().concat(ch.slice(1));
        else continue;
        pieces.splice(i, 1); grew = true; break;
      }
    }
    const closed = ch.length > 3 && close(ch[0], ch.at(-1));
    chains.push({ pts: closed ? ch.slice(0, -1) : ch, closed });
  }
  return chains;
}

function offsetOpen(pts, d) {
  // parallel curve to the left with mitre joins
  const out = [];
  const nrm = (a, b) => { const dx = b[0] - a[0], dy = b[1] - a[1], L = Math.hypot(dx, dy) || 1; return [-dy / L, dx / L]; };
  for (let i = 0; i < pts.length; i++) {
    const n0 = i > 0 ? nrm(pts[i - 1], pts[i]) : null, n1 = i < pts.length - 1 ? nrm(pts[i], pts[i + 1]) : null;
    let n;
    if (n0 && n1) {
      const mx = n0[0] + n1[0], my = n0[1] + n1[1], L = Math.hypot(mx, my);
      const cosHalf = L / 2;
      n = L < 1e-9 ? n1 : [mx / L / Math.max(cosHalf, 0.2), my / L / Math.max(cosHalf, 0.2)];
    } else n = n0 || n1;
    out.push([pts[i][0] + n[0] * d, pts[i][1] + n[1] * d]);
  }
  return out;
}

const ringPolys = cs => cs.toPolygons().map(p => p.map(q => [q[0], q[1]]));

/** Offset copies of the chosen shapes (positive = outward / to the left of lines). */
export function offset(entities, ids, d, newId) {
  const out = [], loose = [];
  for (const e of entities) {
    if (!ids.includes(e.id)) continue;
    const t = e.type, c = { id: newId(), construction: !!e.construction };
    if (t === 'rect') {
      const f = +(e.fillet || 0);
      const x0 = Math.min(+e.x, +e.x + +e.w), x1 = Math.max(+e.x, +e.x + +e.w), y0 = Math.min(+e.y, +e.y + +e.h), y1 = Math.max(+e.y, +e.y + +e.h);
      if (x1 - x0 + 2 * d <= 0 || y1 - y0 + 2 * d <= 0) throw new GeometryError('Offset is larger than the shape.');
      out.push({ ...e, ...c, x: x0 - d, y: y0 - d, w: x1 - x0 + 2 * d, h: y1 - y0 + 2 * d, fillet: f ? Math.max(f + d, 0) : 0 });
    } else if (t === 'circle') {
      if (+e.r + d <= 0) throw new GeometryError('Offset is larger than the circle.');
      out.push({ ...e, ...c, r: +e.r + d });
    } else if (t === 'arc') out.push({ ...e, ...c, r: Math.max(+e.r + d, 1e-6) });
    else if (t === 'polygon') out.push({ ...e, ...c, r: +e.r + d / Math.cos(Math.PI / Math.floor(+(e.n || 6))) });
    else if (t === 'slot') out.push({ ...e, ...c, r: +e.r + d });
    else if (t === 'polyline' && e.closed) {
      const cs = crossSection([e.pts.map(p => [+p[0], +p[1]])], 'Positive').offset(d, 'Miter', 4);
      for (const ring of ringPolys(cs)) out.push({ ...c, type: 'polyline', pts: ring, closed: true });
    } else loose.push(e);
  }
  if (loose.length) {
    for (const ch of chainLoose(loose)) {
      if (ch.closed) {
        let ring = ch.pts;
        if (signedArea(ring) < 0) ring = ring.slice().reverse();
        const cs = crossSection([ring], 'Positive').offset(d, 'Miter', 4);
        for (const r of ringPolys(cs)) out.push({ id: newId(), type: 'polyline', pts: r, closed: true });
      } else {
        const pts = offsetOpen(ch.pts, d);
        if (pts.length >= 2) out.push({ id: newId(), type: 'polyline', pts, closed: false });
      }
    }
  }
  if (!out.length) throw new GeometryError('Select the shapes to offset first.');
  return entities.concat(out);
}

export function findAxis(entities, axisId) {
  for (const e of entities) {
    if (e.id !== axisId) continue;
    if (e.type === 'polyline' && e.pts.length >= 2) return [e.pts[0].map(Number), [e.pts[1][0] - e.pts[0][0], e.pts[1][1] - e.pts[0][1]]];
    if (e.type === 'line') return [e.p1.map(Number), [e.p2[0] - e.p1[0], e.p2[1] - e.p1[1]]];
  }
  throw new GeometryError('Pick a straight sketch line (or the sketch X/Y axis) as the revolve axis.');
}

/** Mirrored copies of the chosen shapes across a sketch line. */
export function mirror(entities, ids, axisId, newId) {
  const [p0, d0] = findAxis(entities, axisId);
  const L = Math.hypot(d0[0], d0[1]), d = [d0[0] / L, d0[1] / L];
  const th = Math.atan2(d[1], d[0]) * 180 / Math.PI;
  const m = p => { const x = +p[0] - p0[0], y = +p[1] - p0[1], k = 2 * (x * d[0] + y * d[1]); return [p0[0] + k * d[0] - x, p0[1] + k * d[1] - y]; };
  const out = [];
  for (const e of entities) {
    if (!ids.includes(e.id) || e.id === axisId) continue;
    const t = e.type, c = { id: newId() };
    if (t === 'line') out.push({ ...e, ...c, p1: m(e.p1), p2: m(e.p2) });
    else if (t === 'polyline') out.push({ ...e, ...c, pts: e.pts.map(m).reverse() });
    else if (t === 'circle' || t === 'polygon') {
      const [cx, cy] = m([e.cx, e.cy]);
      const ne = { ...e, ...c, cx, cy };
      if (t === 'polygon') ne.angle = 2 * th - +(e.angle || 0);
      out.push(ne);
    } else if (t === 'arc') { const [cx, cy] = m([e.cx, e.cy]); out.push({ ...e, ...c, cx, cy, a0: 2 * th - +e.a1, a1: 2 * th - +e.a0 }); }
    else if (t === 'slot') { const [x1, y1] = m([e.x1, e.y1]), [x2, y2] = m([e.x2, e.y2]); out.push({ ...e, ...c, x1, y1, x2, y2 }); }
    else if (t === 'rect') {
      const cxy = m([+e.x + +e.w / 2, +e.y + +e.h / 2]), w = Math.abs(+e.w), h = Math.abs(+e.h);
      out.push({ ...e, ...c, x: cxy[0] - w / 2, y: cxy[1] - h / 2, w, h, angle: 2 * th - +(e.angle || 0) });
    }
  }
  if (!out.length) throw new GeometryError('Select the shapes to mirror (not the mirror line itself).');
  return entities.concat(out);
}

export { CrossSection };
