// Sketch constraints and a small numerical solver.
//
// Constraints refer to entities ({e}) or to points on them ({e, k}). The solver moves the
// entity parameters as little as possible (minimum-norm Gauss-Newton steps) until every
// constraint holds. It runs in the browser so dragging and editing feel immediate.

const D2R = Math.PI / 180;

// ------------------------------------------------------------------ parameters
const PARAMS = {
  line: ['p1.0', 'p1.1', 'p2.0', 'p2.1'],
  arc: ['cx', 'cy', 'r', 'a0', 'a1'],
  circle: ['cx', 'cy', 'r'],
  rect: ['x', 'y', 'w', 'h'],
  polygon: ['cx', 'cy', 'r'],
  slot: ['x1', 'y1', 'x2', 'y2', 'r'],
  point: ['x', 'y'],
};
function paramsOf(e) {
  if (e.type === 'polyline') return (e.pts || []).flatMap((_, i) => [`pts.${i}.0`, `pts.${i}.1`]);
  return PARAMS[e.type] || [];
}
function getP(e, path) { return path.split('.').reduce((o, k) => o[k], e); }
function setP(e, path, v) {
  const ks = path.split('.'); let o = e;
  for (let i = 0; i < ks.length - 1; i++) o = o[ks[i]];
  o[ks.at(-1)] = v;
}

// ------------------------------------------------------------------ points on entities
export function pointKeys(e) {
  switch (e.type) {
    case 'line': return ['p1', 'p2', 'm'];
    case 'arc': return ['s', 'e', 'c'];
    case 'circle': return ['c'];
    case 'rect': return ['c0', 'c1', 'c2', 'c3', 'c'];
    case 'polygon': return ['c'];
    case 'slot': return ['s', 'e'];
    case 'polyline': return (e.pts || []).map((_, i) => 'v' + i);
    case 'point': return ['p'];
  }
  return [];
}
export function point(e, k) {
  switch (e.type) {
    case 'line': return k === 'p1' ? e.p1 : k === 'p2' ? e.p2 : [(e.p1[0] + e.p2[0]) / 2, (e.p1[1] + e.p2[1]) / 2];
    case 'arc': {
      if (k === 'c') return [e.cx, e.cy];
      const a = (k === 's' ? e.a0 : e.a1) * D2R;
      return [e.cx + e.r * Math.cos(a), e.cy + e.r * Math.sin(a)];
    }
    case 'circle': case 'polygon': return [e.cx, e.cy];
    case 'rect': {
      const { x, y, w, h } = e;
      return { c0: [x, y], c1: [x + w, y], c2: [x + w, y + h], c3: [x, y + h], c: [x + w / 2, y + h / 2] }[k];
    }
    case 'slot': return k === 's' ? [e.x1, e.y1] : [e.x2, e.y2];
    case 'polyline': return e.pts[+k.slice(1)];
    case 'point': return [e.x, e.y];
  }
  return [0, 0];
}
const isLine = e => e && e.type === 'line';
const isRound = e => e && (e.type === 'circle' || e.type === 'arc');
const center = e => [e.cx, e.cy];
const sub = (a, b) => [a[0] - b[0], a[1] - b[1]];
const dot = (a, b) => a[0] * b[0] + a[1] * b[1];
const crs = (a, b) => a[0] * b[1] - a[1] * b[0];
const len = a => Math.hypot(a[0], a[1]);
const dir = e => sub(e.p2, e.p1);
const wrap = a => { while (a > Math.PI) a -= 2 * Math.PI; while (a < -Math.PI) a += 2 * Math.PI; return a; };

// ------------------------------------------------------------------ constraint types
export const CTYPES = {
  coincident: { glyph: '•', label: 'Coincident' },
  onentity: { glyph: '⊙', label: 'Point on curve' },
  horizontal: { glyph: 'H', label: 'Horizontal' },
  vertical: { glyph: 'V', label: 'Vertical' },
  parallel: { glyph: '∥', label: 'Parallel' },
  perpendicular: { glyph: '⊥', label: 'Perpendicular' },
  equal: { glyph: '=', label: 'Equal' },
  tangent: { glyph: 'T', label: 'Tangent' },
  concentric: { glyph: '◎', label: 'Concentric' },
  midpoint: { glyph: 'M', label: 'Midpoint' },
  fix: { glyph: '🔒', label: 'Fixed' },
  symmetric: { glyph: '⌯', label: 'Symmetric' },
  distance: { label: 'Distance', dim: true },
  hdistance: { label: 'Horizontal distance', dim: true },
  vdistance: { label: 'Vertical distance', dim: true },
  length: { label: 'Length', dim: true },
  radius: { label: 'Radius', dim: true },
  diameter: { label: 'Diameter', dim: true },
  angle: { label: 'Angle', dim: true, angle: true },
};

// residuals of one constraint (array of numbers that should all be 0)
function residual(c, E) {
  const ent = r => E[r.e];
  const P = r => point(E[r.e], r.k);
  const A = c.a && ent(c.a), B = c.b && ent(c.b);
  if (!A || (c.b && !B) || (c.c && !E[c.c.e])) return null;
  switch (c.type) {
    case 'coincident': { const d = sub(P(c.a), P(c.b)); return [d[0], d[1]]; }
    case 'onentity': {
      const p = P(c.a);
      if (isLine(B)) { const d = dir(B); return [crs(sub(p, B.p1), d) / (len(d) || 1)]; }
      if (isRound(B)) return [len(sub(p, center(B))) - B.r];
      return [0];
    }
    case 'horizontal': return c.b ? [P(c.a)[1] - P(c.b)[1]] : [A.p2[1] - A.p1[1]];
    case 'vertical': return c.b ? [P(c.a)[0] - P(c.b)[0]] : [A.p2[0] - A.p1[0]];
    case 'parallel': { const a = dir(A), b = dir(B); return [crs(a, b) / ((len(a) * len(b)) || 1)]; }
    case 'perpendicular': { const a = dir(A), b = dir(B); return [dot(a, b) / ((len(a) * len(b)) || 1)]; }
    case 'equal':
      if (isLine(A) && isLine(B)) return [len(dir(A)) - len(dir(B))];
      if (isRound(A) && isRound(B)) return [A.r - B.r];
      return [0];
    case 'tangent': {
      if (isLine(A) && isRound(B)) { const d = dir(A); return [Math.abs(crs(sub(center(B), A.p1), d)) / (len(d) || 1) - B.r]; }
      if (isRound(A) && isLine(B)) { const d = dir(B); return [Math.abs(crs(sub(center(A), B.p1), d)) / (len(d) || 1) - A.r]; }
      if (isRound(A) && isRound(B)) { const dd = len(sub(center(A), center(B))); return [c.inner ? dd - Math.abs(A.r - B.r) : dd - (A.r + B.r)]; }
      return [0];
    }
    case 'concentric': { const d = sub(center(A), center(B)); return [d[0], d[1]]; }
    case 'midpoint': { const p = P(c.a), m = point(B, 'm'); return [p[0] - m[0], p[1] - m[1]]; }
    case 'fix':
      if (c.a.k) { const p = P(c.a); return [p[0] - c.at[0], p[1] - c.at[1]]; }
      return paramsOf(A).map((path, i) => getP(A, path) - c.at[i]);
    case 'symmetric': {
      const p = P(c.a), q = P(c.b), L = E[c.c.e], d = dir(L), m = [(p[0] + q[0]) / 2, (p[1] + q[1]) / 2];
      return [crs(sub(m, L.p1), d) / (len(d) || 1), dot(sub(q, p), d) / (len(d) || 1)];
    }
    case 'distance': {
      if (c.b && c.b.k === undefined && isLine(B)) { const d = dir(B); return [Math.abs(crs(sub(P(c.a), B.p1), d)) / (len(d) || 1) - c.value]; }
      return [len(sub(P(c.a), P(c.b))) - c.value];
    }
    case 'hdistance': return [(P(c.b)[0] - P(c.a)[0]) * (c.sign || 1) - c.value];
    case 'vdistance': return [(P(c.b)[1] - P(c.a)[1]) * (c.sign || 1) - c.value];
    case 'length':
      if (isLine(A)) return [len(dir(A)) - c.value];
      if (A.type === 'slot') return [Math.hypot(A.x2 - A.x1, A.y2 - A.y1) - c.value];
      return [0];
    case 'radius': return [A.r - c.value];
    case 'diameter': return [2 * A.r - c.value];
    case 'angle': {
      // ra / rb pick which way along each line the angle is measured (the sector it was placed in)
      const a = dir(A).map(x => x * (c.ra || 1)), b = dir(B).map(x => x * (c.rb || 1));
      return [wrap(Math.atan2(crs(a, b), dot(a, b)) - (c.sign || 1) * c.value * D2R)];
    }
  }
  return null;
}

// current measured value of a dimension (for defaults and labels)
export function measure(c, entities) {
  const E = Object.fromEntries(entities.map(e => [e.id, e]));
  const r = residual({ ...c, value: 0, sign: 1 }, E);
  if (!r) return null;
  if (c.type === 'angle') return Math.abs(r[0]) / D2R;
  if (c.type === 'hdistance' || c.type === 'vdistance') return Math.abs(r[0]);
  return Math.abs(r[0]);
}

// ------------------------------------------------------------------ solver
function linsolve(A, b) {
  const n = b.length, M = A.map((row, i) => [...row, b[i]]);
  for (let col = 0; col < n; col++) {
    let piv = col;
    for (let r = col + 1; r < n; r++) if (Math.abs(M[r][col]) > Math.abs(M[piv][col])) piv = r;
    if (Math.abs(M[piv][col]) < 1e-14) continue;
    [M[col], M[piv]] = [M[piv], M[col]];
    for (let r = 0; r < n; r++) {
      if (r === col) continue;
      const f = M[r][col] / M[col][col];
      if (!f) continue;
      for (let k = col; k <= n; k++) M[r][k] -= f * M[col][k];
    }
  }
  return M.map((row, i) => (Math.abs(row[i]) < 1e-14 ? 0 : row[n] / row[i]));
}

function run(E, vars, eqs, iters = 60) {
  // eqs: functions returning arrays; vars: [{e, path}]
  const n = vars.length;
  const x = vars.map(v => getP(E[v.e], v.path));
  const set = xx => vars.forEach((v, i) => setP(E[v.e], v.path, xx[i]));
  const F = () => eqs.flatMap(f => f() || []);
  let f = F(), norm = Math.hypot(...f, 0);
  for (let it = 0; it < iters && norm > 1e-10; it++) {
    const m = f.length, h = 1e-7, J = Array.from({ length: m }, () => new Array(n).fill(0));
    for (let j = 0; j < n; j++) {
      const old = x[j]; x[j] = old + h; set(x);
      const fp = F();
      for (let i = 0; i < m; i++) J[i][j] = (fp[i] - f[i]) / h;
      x[j] = old;
    }
    set(x);
    let lambda = 1e-9, improved = false;
    for (let tries = 0; tries < 8 && !improved; tries++) {
      const A = J.map((ri, i) => J.map((rk, k) => ri.reduce((s, v, j) => s + v * rk[j], 0) + (i === k ? lambda : 0)));
      const y = linsolve(A, f);
      const dx = new Array(n).fill(0);
      for (let i = 0; i < m; i++) if (y[i]) for (let j = 0; j < n; j++) dx[j] -= J[i][j] * y[i];
      const xn = x.map((v, j) => v + dx[j]);
      set(xn);
      const fn = F(), nn = Math.hypot(...fn, 0);
      if (nn < norm || nn < 1e-10) { for (let j = 0; j < n; j++) x[j] = xn[j]; f = fn; norm = nn; improved = true; }
      else { set(x); lambda *= 100; }
    }
    if (!improved) break;
  }
  set(x);
  return norm;
}

/**
 * Solve the sketch. `locks` pull points or parameters toward targets (while dragging or after
 * typing a value); constraints always win over locks.
 * Returns { entities, ok, error } with new entity objects (inputs are not modified).
 */
export function solve(entities, constraints, locks = [], fixed = []) {
  const E = Object.fromEntries(entities.map(e => [e.id, structuredClone(e)]));
  for (const f of fixed) if (!E[f.id]) E[f.id] = f;
  const isFixed = new Set(fixed.map(f => f.id));
  const valid = constraints.filter(c => !c.ref && refsExist(c, E));
  if (!valid.length && !locks.length) return { entities: entities.map(e => E[e.id]), ok: true };
  const touched = new Set();
  for (const c of valid) for (const r of [c.a, c.b, c.c]) if (r) touched.add(r.e);
  for (const l of locks) touched.add(l.e);
  // only entities connected to a constraint or lock move
  const vars = [];
  for (const id of touched) if (E[id] && !isFixed.has(id)) for (const path of paramsOf(E[id])) vars.push({ e: id, path });
  const ceqs = valid.map(c => () => residual(c, E));
  const leqs = locks.map(l => () => {
    const e = E[l.e]; if (!e) return [];
    if (l.k) { const p = point(e, l.k); return [(p[0] - l.at[0]) * 0.05, (p[1] - l.at[1]) * 0.05]; }
    return [(getP(e, l.path) - l.value) * 0.05];
  });
  if (locks.length) run(E, vars, [...ceqs, ...leqs], 40);
  const err = ceqs.length ? run(E, vars, ceqs, 80) : 0;
  // keep shapes sane
  for (const id of touched) { const e = E[id]; if (e && !isFixed.has(id) && 'r' in e && e.r < 0) e.r = -e.r; }
  const ok = err < 1e-6;
  return { entities: entities.map(e => E[e.id]), ok, error: err };
}

export function refsExist(c, E) {
  for (const r of [c.a, c.b, c.c]) {
    if (!r) continue;
    const e = E[r.e]; if (!e) return false;
    if (r.k && !pointKeys(e).includes(r.k)) return false;
  }
  return true;
}

// ------------------------------------------------------------------ building constraints from picks
// picks: [{e, k?}] in click order. Returns a constraint (without id) or a string message.
export function makeConstraint(tool, picks, entities) {
  const E = Object.fromEntries(entities.map(e => [e.id, e]));
  const kind = p => (p.k ? 'point' : E[p.e]?.type);
  const [p, q, r] = picks;
  const need = n => (picks.length < n ? null : true);
  switch (tool) {
    case 'c_coincident':
      if (!need(2)) return null;
      if (p.k && q.k) return { type: 'coincident', a: p, b: q };
      if (p.k && (isLine(E[q.e]) || isRound(E[q.e]))) return { type: 'onentity', a: p, b: { e: q.e } };
      if (q.k && (isLine(E[p.e]) || isRound(E[p.e]))) return { type: 'onentity', a: q, b: { e: p.e } };
      return 'Pick two points, or a point and a line / circle.';
    case 'c_hv': {
      if (p && !p.k && isLine(E[p.e])) { const d = dir(E[p.e]); return { type: Math.abs(d[0]) >= Math.abs(d[1]) ? 'horizontal' : 'vertical', a: { e: p.e } }; }
      if (!need(2)) return p && !p.k ? 'Pick a line, or two points.' : null;
      if (p.k && q.k) { const d = sub(point(E[q.e], q.k), point(E[p.e], p.k)); return { type: Math.abs(d[0]) >= Math.abs(d[1]) ? 'horizontal' : 'vertical', a: p, b: q }; }
      return 'Pick a line, or two points.';
    }
    case 'c_parallel': case 'c_perpendicular':
      if (!need(2)) return null;
      if (isLine(E[p.e]) && isLine(E[q.e]) && !p.k && !q.k) return { type: tool.slice(2), a: { e: p.e }, b: { e: q.e } };
      return 'Pick two lines.';
    case 'c_equal':
      if (!need(2)) return null;
      if ((isLine(E[p.e]) && isLine(E[q.e])) || (isRound(E[p.e]) && isRound(E[q.e]))) return { type: 'equal', a: { e: p.e }, b: { e: q.e } };
      return 'Pick two lines, or two circles / arcs.';
    case 'c_tangent': {
      if (!need(2)) return null;
      const A = E[p.e], B = E[q.e];
      if ((isLine(A) && isRound(B)) || (isRound(A) && isLine(B))) return { type: 'tangent', a: { e: p.e }, b: { e: q.e } };
      if (isRound(A) && isRound(B)) {
        const dd = len(sub(center(A), center(B)));
        return { type: 'tangent', a: { e: p.e }, b: { e: q.e }, inner: Math.abs(dd - Math.abs(A.r - B.r)) < Math.abs(dd - (A.r + B.r)) };
      }
      return 'Pick a line and a circle / arc, or two circles / arcs.';
    }
    case 'c_concentric':
      if (!need(2)) return null;
      if (isRound(E[p.e]) && isRound(E[q.e])) return { type: 'concentric', a: { e: p.e }, b: { e: q.e } };
      return 'Pick two circles or arcs.';
    case 'c_midpoint':
      if (!need(2)) return null;
      if (p.k && isLine(E[q.e]) && !q.k) return { type: 'midpoint', a: p, b: { e: q.e } };
      if (q.k && isLine(E[p.e]) && !p.k) return { type: 'midpoint', a: q, b: { e: p.e } };
      return 'Pick a point and a line.';
    case 'c_fix':
      if (!p) return null;
      if (p.k) return { type: 'fix', a: p, at: [...point(E[p.e], p.k)] };
      return { type: 'fix', a: { e: p.e }, at: paramsOf(E[p.e]).map(path => getP(E[p.e], path)) };
    case 'c_symmetric':
      if (!need(3)) return null;
      if (p.k && q.k && isLine(E[r.e])) return { type: 'symmetric', a: p, b: q, c: { e: r.e } };
      return 'Pick two points, then the line of symmetry.';
    case 'c_dimension': return inferDimension(picks, entities);
  }
  return 'Unknown constraint tool.';
}

export const CTOOLS = {
  c_coincident: { label: 'Coincident', hint: 'Click two points (or a point, then a line or circle) to join them' },
  c_hv: { label: 'Horiz / Vert', hint: 'Click a line (or two points) to make it horizontal or vertical' },
  c_parallel: { label: 'Parallel', hint: 'Click two lines' },
  c_perpendicular: { label: 'Perpendicular', hint: 'Click two lines' },
  c_equal: { label: 'Equal', hint: 'Click two lines (same length) or two circles / arcs (same size)' },
  c_tangent: { label: 'Tangent', hint: 'Click a line and an arc / circle, or two arcs / circles' },
  c_concentric: { label: 'Concentric', hint: 'Click two circles or arcs' },
  c_midpoint: { label: 'Midpoint', hint: 'Click a point, then a line' },
  c_fix: { label: 'Fix', hint: 'Click a point or shape to lock it in place' },
  c_symmetric: { label: 'Symmetric', hint: 'Click two points, then the line they mirror about' },
  c_dimension: { label: 'Smart Dimension', hint: 'Click a line, circle or arc (or two points, a point and a line, or two lines), move to place it, click, then type the value' },
};

// where to draw a constraint's glyph(s): [[u, v], ...]
export function glyphSpots(c, entities) {
  const E = Object.fromEntries(entities.map(e => [e.id, e]));
  const at = r => {
    const e = E[r.e]; if (!e) return null;
    if (r.k) return point(e, r.k);
    if (isLine(e)) return point(e, 'm');
    if (e.type === 'arc') { const a = ((e.a0 + ((e.a1 - e.a0 + 360) % 360) / 2)) * D2R; return [e.cx + e.r * Math.cos(a), e.cy + e.r * Math.sin(a)]; }
    if ('cx' in e) return [e.cx + e.r * 0.7, e.cy - e.r * 0.7];
    if (e.type === 'rect') return [e.x + e.w / 2, e.y + e.h / 2];
    if (e.type === 'slot') return [(e.x1 + e.x2) / 2, (e.y1 + e.y2) / 2];
    return null;
  };
  return [c.a, c.b].filter(Boolean).map(at).filter(Boolean);
}

// ------------------------------------------------------------------ Smart Dimension
// What to dimension from what was picked, like SOLIDWORKS: a line gives its length, a circle
// its diameter, an arc its radius; two points or a point and a line the distance between them;
// two lines their angle (or their distance when parallel). Where the pointer is decides
// horizontal, vertical or aligned, and which side of an angle is measured.
// picks: [{e, k?, side?}] (side: which edge of a rectangle). `fixed`: ids that can't move
// (the origin, the axes, part edges). Returns a constraint without value, null to wait for
// another pick, or a message.
export function inferDimension(picks, entities, cursor = null, fixed = new Set()) {
  const E = Object.fromEntries(entities.map(e => [e.id, e]));
  const cur = cursor || [0, 0];
  if (picks.length && picks.every(p => fixed.has(p.e)) && picks.length >= 2)
    return 'Pick at least one shape of the sketch: the origin, the axes and the part edges don\'t move.';
  const zone = (P, Q) => {
    const dx = Math.abs(Q[0] - P[0]), dy = Math.abs(Q[1] - P[1]), L = Math.hypot(dx, dy) || 1;
    if (dy / L < 1e-6) return 'h';
    if (dx / L < 1e-6) return 'v';
    const inX = cur[0] > Math.min(P[0], Q[0]) && cur[0] < Math.max(P[0], Q[0]);
    const inY = cur[1] > Math.min(P[1], Q[1]) && cur[1] < Math.max(P[1], Q[1]);
    return inX && !inY ? 'h' : inY && !inX ? 'v' : 'a';
  };
  const linear = (a, b, P, Q, aligned = 'distance') => {
    const o = zone(P, Q);
    if (o === 'h') return { type: 'hdistance', a, b, sign: Math.sign(Q[0] - P[0]) || 1 };
    if (o === 'v') return { type: 'vdistance', a, b, sign: Math.sign(Q[1] - P[1]) || 1 };
    return aligned === 'length' ? { type: 'length', a: { e: a.e } } : { type: 'distance', a, b };
  };
  const RS = [['c0', 'c1'], ['c1', 'c2'], ['c2', 'c3'], ['c3', 'c0']];
  const kind = p => {
    const e = E[p.e]; if (!e) return null;
    if (p.k) return 'pt';
    if (e.type === 'rect' && p.side != null) return 'rs';
    if (e.type === 'line') return 'ln';
    if (e.type === 'circle' || e.type === 'arc') return 'rd';
    if (e.type === 'point') return 'pt';
    return e.type;
  };
  const ptOf = p => { const e = E[p.e]; if (p.k) return p; if (e.type === 'point') return { e: p.e, k: 'p' }; if (e.type === 'circle' || e.type === 'arc') return { e: p.e, k: 'c' }; return null; };
  const P = r => point(E[r.e], r.k);
  const [p, q] = picks;
  if (picks.length === 1) {
    const e = E[p.e], k = kind(p);
    if (k === 'pt' || fixed.has(p.e)) return null;
    if (k === 'ln') {
      const o = zone(e.p1, e.p2);
      if (o === 'h' && Math.abs(e.p2[1] - e.p1[1]) > 1e-9) return { type: 'hdistance', a: { e: p.e, k: 'p1' }, b: { e: p.e, k: 'p2' }, sign: Math.sign(e.p2[0] - e.p1[0]) || 1 };
      if (o === 'v' && Math.abs(e.p2[0] - e.p1[0]) > 1e-9) return { type: 'vdistance', a: { e: p.e, k: 'p1' }, b: { e: p.e, k: 'p2' }, sign: Math.sign(e.p2[1] - e.p1[1]) || 1 };
      return { type: 'length', a: { e: p.e } };
    }
    if (k === 'rs') {
      const [k1, k2] = RS[p.side], A = point(e, k1), B = point(e, k2), horiz = p.side % 2 === 0;
      return { type: horiz ? 'hdistance' : 'vdistance', a: { e: p.e, k: k1 }, b: { e: p.e, k: k2 }, sign: Math.sign(horiz ? B[0] - A[0] : B[1] - A[1]) || 1 };
    }
    if (e.type === 'circle') return { type: 'diameter', a: { e: p.e } };
    if (e.type === 'arc' || e.type === 'polygon') return { type: 'radius', a: { e: p.e } };
    if (e.type === 'slot') return { type: 'length', a: { e: p.e } };
    if (e.type === 'rect') return 'Click one edge of the rectangle.';
    return 'That shape can\'t take a dimension: pick lines, circles, arcs or points.';
  }
  if (p.e === q.e && p.k === q.k && p.side === q.side) return 'That\'s the same item twice.';
  const ka = kind(p), kb = kind(q);
  // point-like: a point, or the centre of a circle / arc
  const isPt = k => k === 'pt' || k === 'rd';
  if (isPt(ka) && isPt(kb)) {
    const pa = ptOf(p), pb = ptOf(q);
    const A = P(pa), B = P(pb);
    if (Math.hypot(B[0] - A[0], B[1] - A[1]) < 1e-9) return 'Those two points are in the same place.';
    return linear(pa, pb, A, B);
  }
  const lineDir = r => { const e = E[r.e]; if (kind(r) === 'ln') return sub(e.p2, e.p1); const [k1, k2] = RS[r.side]; return sub(point(e, k2), point(e, k1)); };
  const ptLine = (pt, ln) => {
    if (kind(ln) === 'rs') {
      const e = E[ln.e], horiz = ln.side % 2 === 0, k1 = RS[ln.side][0], A = P(pt), B = point(e, k1);
      return { type: horiz ? 'vdistance' : 'hdistance', a: pt, b: { e: ln.e, k: k1 }, sign: Math.sign(horiz ? B[1] - A[1] : B[0] - A[0]) || 1 };
    }
    const L = E[ln.e], d = dir(L);
    if (Math.abs(crs(sub(P(pt), L.p1), d)) / (len(d) || 1) < 1e-9) return 'That point is on the line.';
    return { type: 'distance', a: pt, b: { e: ln.e } };
  };
  const isLn = k => k === 'ln' || k === 'rs';
  if (isPt(ka) && isLn(kb)) return ptLine(ptOf(p), q);
  if (isLn(ka) && isPt(kb)) return ptLine(ptOf(q), p);
  if (isLn(ka) && isLn(kb)) {
    const a = lineDir(p), b = lineDir(q);
    const par = Math.abs(crs(a, b)) / ((len(a) * len(b)) || 1) < 0.0175;
    if (par) {
      // the distance between them: from a point of one (one that can move) to the other
      const [m, o] = fixed.has(p.e) ? [q, p] : [p, q];
      const mp = kind(m) === 'ln' ? { e: m.e, k: 'm' } : { e: m.e, k: RS[m.side][0] };
      const r = ptLine(mp, o);
      return typeof r === 'string' ? 'Those lines are on top of each other.' : r;
    }
    if (ka === 'rs' || kb === 'rs') return 'Angles to a rectangle\'s sides can\'t be set: its sides are always square. Draw it with lines to angle it.';
    return angleTo({ type: 'angle', a: { e: p.e }, b: { e: q.e } }, E, cur);
  }
  return 'Pick a line, circle or arc; or two points; a point and a line; or two lines.';
}

/** Point the angle's rays at the sector the pointer is in (sets ra, rb, sign). */
export function angleTo(c, E, cur) {
  const A = E[c.a.e], B = E[c.b.e], a = dir(A), b = dir(B);
  const V = lineMeet(A, B) || point(A, 'm');
  const w = sub(cur, V), det = crs(a, b);
  const ra = Math.sign(crs(w, b) / det) || 1, rb = Math.sign(crs(a, w) / det) || 1;
  const sa = a.map(x => x * ra), sb = b.map(x => x * rb);
  return { ...c, ra, rb, sign: Math.sign(crs(sa, sb)) || 1 };
}

/** Where two lines (taken as endless) cross, or null when they're parallel. */
export function lineMeet(A, B) {
  const a = dir(A), b = dir(B), d = crs(a, b);
  if (Math.abs(d) < 1e-12 * (len(a) * len(b) || 1)) return null;
  const t = crs(sub(B.p1, A.p1), b) / d;
  return [A.p1[0] + a[0] * t, A.p1[1] + a[1] * t];
}

// ------------------------------------------------------------------ how defined the sketch is
// Rank of the constraints' Jacobian: what is left free (degrees of freedom), and which shapes
// can still move. Shapes in `fixed` (origin, axes, part edges) and text outlines don't count.
function jacobian(E, vars, cons) {
  const base = cons.map(c => residual(c, E) || []);
  const off = []; let m = 0;
  for (const r of base) { off.push(m); m += r.length; }
  const byEnt = {};
  cons.forEach((c, i) => { for (const r of [c.a, c.b, c.c]) if (r) (byEnt[r.e] ||= new Set()).add(i); });
  const J = Array.from({ length: m }, () => new Float64Array(vars.length));
  vars.forEach((v, j) => {
    const e = E[v.e], x0 = getP(e, v.path), h = 1e-6 * Math.max(1, Math.abs(x0));
    for (const i of byEnt[v.e] || []) {
      setP(e, v.path, x0 + h); const fp = residual(cons[i], E) || [];
      setP(e, v.path, x0 - h); const fm = residual(cons[i], E) || [];
      setP(e, v.path, x0);
      for (let r = 0; r < base[i].length; r++) J[off[i] + r][j] = ((fp[r] ?? 0) - (fm[r] ?? 0)) / (2 * h);
    }
  });
  return J;
}

function rankNull(J, n) {
  const R = J.map(r => { const mx = r.reduce((a, x) => Math.max(a, Math.abs(x)), 0); return mx > 0 ? r.map(x => x / mx) : r; });
  const piv = [];
  let row = 0;
  for (let col = 0; col < n && row < R.length; col++) {
    let best = row;
    for (let r = row + 1; r < R.length; r++) if (Math.abs(R[r][col]) > Math.abs(R[best][col])) best = r;
    if (Math.abs(R[best][col]) < 1e-7) continue;
    [R[row], R[best]] = [R[best], R[row]];
    const pv = R[row][col];
    for (let k = col; k < n; k++) R[row][k] /= pv;
    for (let r = 0; r < R.length; r++) {
      if (r === row) continue;
      const f = R[r][col]; if (!f) continue;
      for (let k = col; k < n; k++) R[r][k] -= f * R[row][k];
    }
    piv.push(col); row++;
  }
  const isPiv = new Set(piv), nulls = [];
  for (let f = 0; f < n; f++) {
    if (isPiv.has(f)) continue;
    const v = new Float64Array(n); v[f] = 1;
    piv.forEach((p, i) => { v[p] = -R[i][f]; });
    const mx = v.reduce((a, x) => Math.max(a, Math.abs(x)), 0) || 1;
    nulls.push(v.map(x => x / mx));
  }
  return { rank: piv.length, nulls };
}

export function analyze(entities, constraints, fixed = []) {
  const E = Object.fromEntries(entities.map(e => [e.id, structuredClone(e)]));
  for (const f of fixed) if (!E[f.id]) E[f.id] = f;
  const fx = new Set(fixed.map(f => f.id));
  const cons = constraints.filter(c => !c.ref && refsExist(c, E));
  // shapes tied together by constraints form groups; each group is worked out on its own
  const parent = {};
  const find = x => { while (parent[x] !== x) x = parent[x] = parent[parent[x]]; return x; };
  const free = entities.filter(e => !fx.has(e.id) && PARAMS[e.type]);
  for (const e of free) parent[e.id] = e.id;
  const byGroup = {};
  for (const c of cons) {
    const ids = [c.a, c.b, c.c].filter(r => r && parent[r.e] !== undefined).map(r => r.e);
    for (let i = 1; i < ids.length; i++) { const a = find(ids[0]), b = find(ids[i]); if (a !== b) parent[a] = b; }
  }
  for (const c of cons) {
    const r = [c.a, c.b, c.c].find(r => r && parent[r.e] !== undefined);
    if (r) (byGroup[find(r.e)] ||= []).push(c);
  }
  const members = {};
  for (const e of free) (members[find(e.id)] ||= []).push(e);
  let dof = 0, rank = 0;
  const under = new Set();
  for (const [g, ents] of Object.entries(members)) {
    const vars = [];
    for (const e of ents) for (const path of paramsOf(e)) vars.push({ e: e.id, path });
    const gc = byGroup[g] || [];
    if (!gc.length) { dof += vars.length; for (const e of ents) under.add(e.id); continue; }
    const r = rankNull(jacobian(E, vars, gc), vars.length);
    rank += r.rank; dof += vars.length - r.rank;
    for (const v of r.nulls) v.forEach((x, j) => { if (Math.abs(x) > 1e-6) under.add(vars[j].e); });
  }
  return { dof, under, rank };
}

/** Would `c` add nothing new (the other constraints already settle what it measures)? */
export function isRedundant(entities, constraints, c, fixed = []) {
  const a = analyze(entities, constraints, fixed), b = analyze(entities, [...constraints, c], fixed);
  return b.rank === a.rank;
}

/** Constraints that don't hold (after a failed solve, or in a sketch opened that way). */
export function unsatisfied(entities, constraints, fixed = []) {
  const E = Object.fromEntries(entities.map(e => [e.id, e]));
  for (const f of fixed) if (!E[f.id]) E[f.id] = f;
  return constraints.filter(c => !c.ref && refsExist(c, E)).filter(c => (residual(c, E) || []).some(x => Math.abs(x) > 1e-5));
}
