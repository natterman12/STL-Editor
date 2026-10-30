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
    case 'angle': { const a = dir(A), b = dir(B); return [wrap(Math.atan2(crs(a, b), dot(a, b)) - (c.sign || 1) * c.value * D2R)]; }
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
export function solve(entities, constraints, locks = []) {
  const E = Object.fromEntries(entities.map(e => [e.id, structuredClone(e)]));
  const valid = constraints.filter(c => refsExist(c, E));
  if (!valid.length && !locks.length) return { entities: entities.map(e => E[e.id]), ok: true };
  const touched = new Set();
  for (const c of valid) for (const r of [c.a, c.b, c.c]) if (r) touched.add(r.e);
  for (const l of locks) touched.add(l.e);
  // only entities connected to a constraint or lock move
  const vars = [];
  for (const id of touched) if (E[id]) for (const path of paramsOf(E[id])) vars.push({ e: id, path });
  const ceqs = valid.map(c => () => residual(c, E));
  const leqs = locks.map(l => () => {
    const e = E[l.e]; if (!e) return [];
    if (l.k) { const p = point(e, l.k); return [(p[0] - l.at[0]) * 0.05, (p[1] - l.at[1]) * 0.05]; }
    return [(getP(e, l.path) - l.value) * 0.05];
  });
  if (locks.length) run(E, vars, [...ceqs, ...leqs], 40);
  const err = ceqs.length ? run(E, vars, ceqs, 80) : 0;
  // keep shapes sane
  for (const id of touched) { const e = E[id]; if (e && 'r' in e && e.r < 0) e.r = -e.r; }
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
    case 'c_dimension': {
      if (picks.length === 1) {
        const A = E[p.e];
        if (p.k) return null;                       // wait for a second point
        if (isLine(A)) return { type: 'length', a: { e: p.e } };
        if (A.type === 'circle') return { type: 'diameter', a: { e: p.e } };
        if (A.type === 'arc') return { type: 'radius', a: { e: p.e } };
        if (A.type === 'slot') return { type: 'length', a: { e: p.e } };
        if (A.type === 'polygon') return { type: 'radius', a: { e: p.e } };
        return 'Rectangles: click two of its corners (points) to dimension between them.';
      }
      if (p.k && q.k) {
        const d = sub(point(E[q.e], q.k), point(E[p.e], p.k));
        // nearly level / plumb points get a horizontal / vertical dimension, others aligned
        const ang = Math.abs(Math.atan2(d[1], d[0])) / D2R;
        if (ang < 8 || ang > 172) return { type: 'hdistance', a: p, b: q, sign: Math.sign(d[0]) || 1 };
        if (Math.abs(ang - 90) < 8) return { type: 'vdistance', a: p, b: q, sign: Math.sign(d[1]) || 1 };
        return { type: 'distance', a: p, b: q };
      }
      if (isLine(E[p.e]) && isLine(E[q.e]) && !p.k && !q.k) {
        const a = dir(E[p.e]), b = dir(E[q.e]);
        return { type: 'angle', a: { e: p.e }, b: { e: q.e }, sign: Math.sign(crs(a, b)) || 1 };
      }
      if (p.k && isLine(E[q.e])) return { type: 'distance', a: p, b: { e: q.e } };
      if (q.k && isLine(E[p.e])) return { type: 'distance', a: q, b: { e: p.e } };
      return 'Pick a line, circle or arc; or two points; or two lines (angle).';
    }
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
  c_dimension: { label: 'Dimension', hint: 'Click a line / circle / arc, or two points, or two lines (angle), then type the value' },
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
