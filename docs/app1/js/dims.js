// How a sketch dimension is drawn: extension lines, a dimension line with arrows, the text
// spot; leaders for radius / diameter; an arc with arrows for angles. Everything is in sketch
// coordinates (mm); `px` is how many mm one screen pixel is, for gaps and default offsets.
// Where a dimension sits is kept on the constraint: `off` / `slide` (linear: distance from the
// measured points, and along them; angle: arc radius), `ang` / `out` (radial: direction and
// how far past the circle the text is).
import { point, lineMeet } from './constraints.js';

const sub = (a, b) => [a[0] - b[0], a[1] - b[1]];
const add = (a, b) => [a[0] + b[0], a[1] + b[1]];
const mul = (a, s) => [a[0] * s, a[1] * s];
const dot = (a, b) => a[0] * b[0] + a[1] * b[1];
const len = a => Math.hypot(a[0], a[1]);
const unit = a => { const l = len(a); return l > 1e-12 ? [a[0] / l, a[1] / l] : [1, 0]; };
const D2R = Math.PI / 180;

export const LINEAR = new Set(['hdistance', 'vdistance', 'distance', 'length']);
export const RADIAL = new Set(['radius', 'diameter']);

/** The two measured points and the measuring direction of a linear dimension. */
export function linearBase(c, E) {
  const A = E[c.a.e];
  if (c.type === 'length') {
    if (A.type === 'slot') { const P = [A.x1, A.y1], Q = [A.x2, A.y2]; return { P, Q, u: unit(sub(Q, P)) }; }
    return { P: A.p1, Q: A.p2, u: unit(sub(A.p2, A.p1)) };
  }
  const P = point(A, c.a.k);
  if (c.type === 'distance' && !c.b.k) {
    // point to line: the foot of the perpendicular
    const L = E[c.b.e], d = unit(sub(L.p2, L.p1));
    const Q = add(L.p1, mul(d, dot(sub(P, L.p1), d)));
    const u = len(sub(Q, P)) > 1e-12 ? unit(sub(Q, P)) : [-d[1], d[0]];
    return { P, Q, u, line: L };
  }
  const Q = point(E[c.b.e], c.b.k);
  const u = c.type === 'hdistance' ? [1, 0] : c.type === 'vdistance' ? [0, 1] : unit(sub(Q, P));
  return { P, Q, u };
}

/** Centre and radius a radius / diameter dimension measures. */
export function roundBase(c, E) {
  const A = E[c.a.e];
  if (A.type === 'slot') return { C: [A.x2, A.y2], r: A.r, ent: A };
  return { C: [A.cx, A.cy], r: A.r, ent: A };
}

/** Default placement for a dimension that wasn't placed by hand (typed while drawing). */
export function defaultPlacement(c, E, px) {
  if (LINEAR.has(c.type)) {
    const { P, Q, u } = linearBase(c, E), n = [-u[1], u[0]], M = mul(add(P, Q), 0.5);
    // away from the middle of the shape it belongs to
    const A = E[c.a.e];
    const mid = A.type === 'rect' ? [A.x + A.w / 2, A.y + A.h / 2] : 'cx' in A ? [A.cx, A.cy] : M;
    const s = Math.sign(dot(sub(M, mid), n)) || 1;
    return { off: s * 26 * px, slide: 0 };
  }
  if (RADIAL.has(c.type)) return { ang: 45, out: 18 * px };
  if (c.type === 'angle') return { off: 46 * px };
  return {};
}

/** Move a dimension so its text sits at `cur` (and, for an angle, measure that sector). */
export function placeAt(c, E, cur) {
  if (LINEAR.has(c.type)) {
    const { P, Q, u } = linearBase(c, E), n = [-u[1], u[0]], M = mul(add(P, Q), 0.5);
    const s = dot(sub(cur, M), u), half = Math.abs(dot(sub(Q, P), u)) / 2;
    c.off = dot(sub(cur, M), n);
    c.slide = Math.abs(s) <= half ? 0 : s;
  } else if (RADIAL.has(c.type)) {
    const { C, r } = roundBase(c, E), d = sub(cur, C);
    c.ang = Math.atan2(d[1], d[0]) / D2R;
    c.out = len(d) - r;
  } else if (c.type === 'angle') {
    const V = lineMeet(E[c.a.e], E[c.b.e]);
    if (V) c.off = Math.max(len(sub(cur, V)), 1e-6);
  }
}

/**
 * Lines, extension lines, arrows and arcs for one dimension.
 * Returns { lines: [[p, q]], thin: [[p, q]], arrows: [{ at, d }], arcs: [{ c, r, a0, a1, thin }], text }
 * (arrow `d`: the way it points; arcs run counter-clockwise from a0 to a1, radians).
 */
export function layout(c, E, px) {
  const out = { lines: [], thin: [], arrows: [], arcs: [], text: [0, 0] };
  const gap = 4 * px, over = 7 * px, arrow = 10 * px;
  if (LINEAR.has(c.type)) {
    const { P, Q, u, line } = linearBase(c, E), n = [-u[1], u[0]], M = mul(add(P, Q), 0.5);
    const off = c.off ?? 26 * px;
    const level = dot(M, n) + off;
    const A = add(P, mul(n, level - dot(P, n))), B = add(Q, mul(n, level - dot(Q, n)));
    for (const [X, Y] of [[P, A], [Q, B]]) {
      const s = level - dot(X, n), sg = Math.sign(s) || 1;
      if (Math.abs(s) > gap) out.thin.push([add(X, mul(n, sg * gap)), add(Y, mul(n, sg * over))]);
    }
    // to a line: carry the line on to the foot of the perpendicular if it stops short
    if (line) {
      const d = unit(sub(line.p2, line.p1)), t1 = dot(sub(line.p1, Q), d), t2 = dot(sub(line.p2, Q), d);
      if (t1 > gap && t2 > gap) out.thin.push([Q, add(Q, mul(d, Math.min(t1, t2)))]);
      else if (t1 < -gap && t2 < -gap) out.thin.push([Q, add(Q, mul(d, Math.max(t1, t2)))]);
    }
    const w = dot(sub(B, A), u), dirAB = Math.sign(w) || 1, uu = mul(u, dirAB);
    const T = add(mul(add(A, B), 0.5), mul(u, c.slide || 0));
    out.text = T;
    out.lines.push([A, B]);
    if (Math.abs(w) > 2.6 * arrow + 14 * px) {
      out.arrows.push({ at: A, d: mul(uu, -1) }, { at: B, d: uu });
    } else {
      // too short for arrows inside: put them outside, pointing in
      out.arrows.push({ at: A, d: uu }, { at: B, d: mul(uu, -1) });
      out.lines.push([A, sub(A, mul(uu, 2.2 * arrow))], [B, add(B, mul(uu, 2.2 * arrow))]);
    }
    // text off the end: run the dimension line out to it
    const tA = dot(sub(T, A), uu), tB = dot(sub(T, B), uu);
    if (tA < 0) out.lines.push([A, T]);
    if (tB > 0) out.lines.push([B, T]);
    return out;
  }
  if (RADIAL.has(c.type)) {
    const { C, r, ent } = roundBase(c, E);
    const th = (c.ang ?? 45) * D2R, d = [Math.cos(th), Math.sin(th)];
    const o = c.out ?? 18 * px;
    const T = add(C, mul(d, Math.max(r + o, 0)));
    const R = add(C, mul(d, r));
    out.text = T;
    if (c.type === 'diameter') {
      const R2 = sub(C, mul(d, r));
      out.lines.push([R2, o >= 0 ? T : R]);
      out.arrows.push({ at: R, d }, { at: R2, d: mul(d, -1) });
    } else {
      out.lines.push([C, o >= 0 ? T : R]);
      out.arrows.push({ at: R, d });
    }
    // an arc that doesn't reach the leader: carry it round, thin
    if (ent.type === 'arc') {
      const deg = x => ((x % 360) + 360) % 360;
      const sw = deg(ent.a1 - ent.a0) || 360, at = deg(c.ang ?? 45);
      const rel = deg(at - ent.a0);
      if (rel > sw) {
        const toStart = 360 - rel, toEnd = rel - sw;
        const [a0, a1] = toEnd < toStart ? [ent.a1, at] : [at, ent.a0];
        out.arcs.push({ c: C, r, a0: a0 * D2R, a1: (a0 + deg(a1 - a0)) * D2R, thin: true });
      }
    }
    return out;
  }
  if (c.type === 'angle') {
    const LA = E[c.a.e], LB = E[c.b.e];
    const V = lineMeet(LA, LB);
    if (!V) { out.text = mul(add(point(LA, 'm'), point(LB, 'm')), 0.5); return out; }
    const ra = mul(unit(sub(LA.p2, LA.p1)), c.ra || 1), rb = mul(unit(sub(LB.p2, LB.p1)), c.rb || 1);
    const R = c.off ?? 46 * px;
    let a0 = Math.atan2(ra[1], ra[0]), a1 = Math.atan2(rb[1], rb[0]);
    if ((c.sign || 1) < 0) [a0, a1] = [a1, a0];
    while (a1 < a0) a1 += 2 * Math.PI;
    out.arcs.push({ c: V, r: R, a0, a1 });
    out.arrows.push({ at: add(V, mul([Math.cos(a0), Math.sin(a0)], R)), d: [Math.sin(a0), -Math.cos(a0)] });
    out.arrows.push({ at: add(V, mul([Math.cos(a1), Math.sin(a1)], R)), d: [-Math.sin(a1), Math.cos(a1)] });
    const mid = (a0 + a1) / 2;
    out.text = add(V, mul([Math.cos(mid), Math.sin(mid)], R));
    // extension lines from the lines out to the arc
    for (const [L, r] of [[LA, ra], [LB, rb]]) {
      const t1 = dot(sub(L.p1, V), r), t2 = dot(sub(L.p2, V), r), hi = Math.max(t1, t2), lo = Math.min(t1, t2);
      if (R > hi + gap) out.thin.push([add(V, mul(r, hi + gap)), add(V, mul(r, R + over))]);
      else if (R < lo - gap) out.thin.push([add(V, mul(r, R - over)), add(V, mul(r, lo - gap))]);
    }
    return out;
  }
  return out;
}
