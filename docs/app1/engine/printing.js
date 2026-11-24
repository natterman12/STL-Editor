// Print-oriented geometry (port of stleditor/printing.py): split with connectors, fit to bed,
// push/pull, lay flat and threads.
import { Manifold, CrossSection, crossSection } from './wasm.js';
import { GeometryError, manifoldFromArrays, planeFrame, makeCylinder, unit, dot, cross, add, sub, mul, worldToFrame, frameMatrix, mat4, triNormals } from './geom.js';
import { pointInRing } from './sketch2d.js';

export const PRINTERS = {
  bambu_x1: ['Bambu X1C / X1E / P1S / P1P / A1', [256, 256, 256]],
  bambu_a1mini: ['Bambu A1 mini', [180, 180, 180]],
  bambu_h2d: ['Bambu H2D', [325, 320, 325]],
  prusa_mk4: ['Prusa MK4 / MK3S', [250, 210, 220]],
  prusa_xl: ['Prusa XL', [360, 360, 360]],
  voron_350: ['Voron 2.4 (350)', [350, 350, 340]],
  voron_300: ['Voron 2.4 (300)', [300, 300, 280]],
  ender3: ['Ender 3 / V2', [220, 220, 250]],
};

const CONNECTORS = { pins: [5.0, 10.0, 0.15], dowels: [5.0, 12.0, 0.15], filament: [1.75, 10.0, 0.15], magnets: [6.0, 3.0, 0.15] };

export function sectionShape(solid, o, n) {
  const fr = planeFrame(o, n);
  return { cs: solid.transform(worldToFrame(...fr)).slice(0), frame: fr };
}

function distToRings(pt, rings) {
  let best = Infinity;
  for (const r of rings) for (let i = 0; i < r.length; i++) {
    const a = r[i], b = r[(i + 1) % r.length], dx = b[0] - a[0], dy = b[1] - a[1], L = dx * dx + dy * dy;
    let t = L ? ((pt[0] - a[0]) * dx + (pt[1] - a[1]) * dy) / L : 0;
    t = Math.max(0, Math.min(1, t));
    const d = Math.hypot(pt[0] - a[0] - t * dx, pt[1] - a[1] - t * dy);
    if (d < best) best = d;
  }
  return best;
}

/** Spread-out pin centres inside a cross-section, keeping `wall` of material round each. */
export function connectorPoints(cs, radius, wall, count = null, maxPerPart = 3) {
  const inner = cs.offset(-(radius + wall), 'Round');
  if (inner.isEmpty() || inner.area() <= 1e-9) return [];
  const pts = [];
  const comps = inner.decompose().sort((a, b) => b.area() - a.area());
  for (const comp of comps) {
    const rings = comp.toPolygons().map(p => p.map(q => [q[0], q[1]]));
    const inside = p => rings.reduce((s, r) => s ^ pointInRing(p, r), false);
    const b = comp.bounds();
    const ext = Math.max(b.max[0] - b.min[0], b.max[1] - b.min[1]);
    const step = Math.max(ext / 60, radius * 0.25);
    let cand = [];
    for (let x = b.min[0]; x <= b.max[0] + step; x += step) for (let y = b.min[1]; y <= b.max[1] + step; y += step) if (inside([x, y])) cand.push([x, y]);
    if (!cand.length) { const r = rings[0]; cand = [[r.reduce((s, p) => s + p[0], 0) / r.length, r.reduce((s, p) => s + p[1], 0) / r.length]]; }
    const nHere = count || Math.min(maxPerPart, Math.max(1, Math.round(ext / (radius * 10))));
    const de = cand.map(p => distToRings(p, rings));
    const chosen = [cand[de.indexOf(Math.max(...de))]];
    for (let k = 0; k < nHere - 1; k++) {
      let bi = -1, bd = -1;
      cand.forEach((p, i) => { const d = Math.min(...chosen.map(c => Math.hypot(p[0] - c[0], p[1] - c[1]))); if (d > bd) { bd = d; bi = i; } });
      if (bd < radius * 4) break;
      chosen.push(cand[bi]);
    }
    pts.push(...chosen);
    if (pts.length >= 8) break;
  }
  return pts;
}

/** Split a solid by a plane. Returns {above, below, extras, info}. */
export function splitWithConnectors(solid, o, n0, mode = 'none', pinD = null, pinLen = null, clearance = null) {
  const n = unit(n0);
  let [above, below] = solid.splitByPlane(n, dot(n, o));
  const info = { connectors: 0, skipped: false };
  const extras = [];
  if (!mode || mode === 'none' || above.isEmpty() || below.isEmpty()) return { above, below, extras, info };
  const [d0, L0, c0] = CONNECTORS[mode];
  const d = +(pinD || d0), L = +(pinLen || L0), c = clearance !== null && clearance !== undefined ? +clearance : c0;
  const r = d / 2;
  const wall = mode === 'filament' ? 0.45 : Math.max(1.2, 0.6 * r);
  const { cs, frame: [fo, x, y] } = sectionShape(solid, o, n);
  const pts = connectorPoints(cs, r + c, wall, null, 4);
  if (!pts.length) { info.skipped = true; return { above, below, extras, info }; }
  const holeR = r + c;
  for (const [u, v] of pts) {
    const P = add(add(fo, mul(x, u)), mul(y, v));
    const neg = mul(n, -1);
    if (mode === 'pins') {
      const pin = makeCylinder(P, neg, L / 2 - 0.2, r, r, 48);
      const tip = makeCylinder(add(P, mul(n, -(L / 2 - 0.2))), neg, 0.5, r, Math.max(r - 0.5, 0.2), 48);
      above = Manifold.union([above, pin, tip]);
      below = below.subtract(makeCylinder(add(P, mul(n, 0.01)), neg, L / 2 + 0.4, holeR, holeR, 48));
    } else if (mode === 'magnets') {
      const depth = L + c + 0.1;
      above = above.subtract(makeCylinder(add(P, mul(n, -0.01)), n, depth, holeR, holeR, 48));
      below = below.subtract(makeCylinder(add(P, mul(n, 0.01)), neg, depth, holeR, holeR, 48));
    } else {
      const half = L / 2 + 0.4;
      above = above.subtract(makeCylinder(add(P, mul(n, -0.01)), n, half, holeR, holeR, 48));
      below = below.subtract(makeCylinder(add(P, mul(n, 0.01)), neg, half, holeR, holeR, 48));
      if (mode === 'dowels') extras.push(makeCylinder(add(P, mul(n, -(L / 2 - 0.2))), n, L - 0.4, r - 0.05, r - 0.05, 48));
    }
    info.connectors++;
  }
  return { above, below, extras, info };
}

function axisSectionArea(solid, ax, pos) {
  const n = [0, 0, 0], o = [0, 0, 0];
  n[ax] = 1; o[ax] = pos;
  return sectionShape(solid, o, n).cs.area();
}

export function planCuts(solid, usable, optimize = true) {
  const b = solid.boundingBox(), lo = b.min, hi = b.max;
  const cuts = {};
  for (let ax = 0; ax < 3; ax++) {
    const S = hi[ax] - lo[ax], U = +usable[ax];
    const np = Math.ceil(S / U - 1e-9);
    if (np <= 1) continue;
    const pos = [];
    let prev = lo[ax];
    for (let k = 1; k < np; k++) {
      const remaining = np - k, even = lo[ax] + S * k / np;
      let loOk = Math.max(prev + 1, hi[ax] - remaining * U), hiOk = Math.min(prev + U, hi[ax] - 1);
      if (loOk > hiOk) loOk = hiOk = Math.min(Math.max(even, loOk), hiOk);
      let p;
      if (optimize && hiOk - loOk > 1) {
        const cand = Array.from({ length: 13 }, (_, i) => loOk + (hiOk - loOk) * i / 12);
        const areas = cand.map(c => axisSectionArea(solid, ax, c));
        const amax = Math.max(...areas) + 1e-9;
        const score = cand.map((c, i) => areas[i] / amax + 0.15 * Math.abs(c - even) / (hiOk - loOk + 1e-9));
        p = cand[score.indexOf(Math.min(...score))];
      } else p = Math.min(Math.max(even, loOk), hiOk);
      pos.push(p);
      prev = p;
    }
    cuts[ax] = pos;
  }
  return cuts;
}

export function splitToBed(solid, bed, margin = 5, mode = 'pins', pinD = null, pinLen = null, clearance = null, optimize = true) {
  let usable = bed.map(x => +x - 2 * +margin);
  if (usable.some(u => u <= 5)) throw new GeometryError('The margin is too large for this bed.');
  const b = solid.boundingBox(), size = sub(b.max, b.min);
  const swap = (size[0] < size[1]) !== (usable[0] < usable[1]);
  if (swap) usable = [usable[1], usable[0], usable[2]];
  const cuts = planCuts(solid, usable, optimize);
  let pieces = [solid];
  const extras = [];
  let totalConn = 0, skipped = 0;
  for (const [axs, positions] of Object.entries(cuts)) {
    const ax = +axs, nvec = [0, 0, 0];
    nvec[ax] = 1;
    for (const pos of positions) {
      const next = [];
      for (const pc of pieces) {
        const pb = pc.boundingBox();
        if (!(pb.min[ax] < pos && pos < pb.max[ax])) { next.push(pc); continue; }
        const o = [0, 0, 0]; o[ax] = pos;
        const r = splitWithConnectors(pc, o, nvec, mode, pinD, pinLen, clearance);
        totalConn += r.info.connectors;
        if (r.info.skipped) skipped++;
        extras.push(...r.extras);
        for (const m of [r.below, r.above]) if (!m.isEmpty()) next.push(m);
      }
      pieces = next;
    }
  }
  const named = {};
  for (const [k, v] of Object.entries(cuts)) named['xyz'[k]] = v.map(p => Math.round(p * 100) / 100);
  return { pieces, extras, info: { cuts: named, connectors: totalConn, faces_without_connectors: skipped, swapped: swap } };
}

// ---------------------------------------------------------------- push / pull
export function faceOutline(v, f, region, frame) {
  const [o, x, y] = frame;
  const tris = [];
  for (const t of region) {
    const uv = [0, 1, 2].map(k => { const p = f[t * 3 + k] * 3; const d = [v[p] - o[0], v[p + 1] - o[1], v[p + 2] - o[2]]; return [dot(d, x), dot(d, y)]; });
    const a = [uv[1][0] - uv[0][0], uv[1][1] - uv[0][1]], b = [uv[2][0] - uv[0][0], uv[2][1] - uv[0][1]];
    tris.push(a[0] * b[1] - a[1] * b[0] < 0 ? uv.reverse() : uv);
  }
  if (!tris.length) return [];
  return crossSection(tris, 'NonZero').toPolygons().map(p => p.map(q => [q[0], q[1]]));
}

export function pushpullTool(contours, frame, distance) {
  const cs = crossSection(contours.map(c => c.map(p => [+p[0], +p[1]])), 'EvenOdd');
  if (cs.isEmpty()) throw new GeometryError('That face has no area.');
  const d = +distance;
  if (Math.abs(d) < 1e-9) throw new GeometryError("Distance can't be zero.");
  let man = Manifold.extrude(cs, Math.abs(d));
  if (d < 0) man = man.translate([0, 0, -Math.abs(d)]);
  return man.transform(frameMatrix(...frame));
}

// ---------------------------------------------------------------- lay flat
export function rotationTo(a0, b0) {
  const a = unit(a0), b = unit(b0);
  const v = cross(a, b), c = dot(a, b);
  if (c < -0.999999) {
    const axis = unit(Math.abs(a[0]) < 0.9 ? cross(a, [1, 0, 0]) : cross(a, [0, 1, 0]));
    return [0, 1, 2].map(i => [0, 1, 2].map(j => 2 * axis[i] * axis[j] - (i === j ? 1 : 0)));
  }
  const K = [[0, -v[2], v[1]], [v[2], 0, -v[0]], [-v[1], v[0], 0]];
  const K2 = K.map(r => [0, 1, 2].map(j => r[0] * K[0][j] + r[1] * K[1][j] + r[2] * K[2][j]));
  return [0, 1, 2].map(i => [0, 1, 2].map(j => (i === j ? 1 : 0) + K[i][j] + K2[i][j] / (1 + c)));
}

export function bestRestNormal(solid) {
  const hull = solid.hull().getMesh();
  const np = hull.numProp, vp = hull.vertProperties;
  const v = new Float32Array(vp.length / np * 3);
  for (let i = 0; i < vp.length / np; i++) for (let k = 0; k < 3; k++) v[i * 3 + k] = vp[i * np + k];
  const { n, area } = triNormals(v, hull.triVerts);
  const groups = new Map();
  for (let t = 0; t < area.length; t++) {
    const key = [n[t * 3], n[t * 3 + 1], n[t * 3 + 2]].map(x => Math.round(x * 1000)).join(',');
    const g = groups.get(key) || { a: 0, n: [0, 0, 0] };
    g.a += area[t]; g.n = add(g.n, mul([n[t * 3], n[t * 3 + 1], n[t * 3 + 2]], area[t]));
    groups.set(key, g);
  }
  let best = null;
  for (const g of groups.values()) if (!best || g.a > best.a) best = g;
  return unit(best.n);
}

export function layFlatMatrix(solid, normal) {
  const R = rotationTo(normal, [0, 0, -1]);
  const b = solid.boundingBox();
  const c = mul(add(b.min, b.max), 0.5);
  const Rc = R.map(r => dot(r, c));
  let t = sub(c, Rc);
  const moved = solid.transform(mat4(R, t));
  const nb = moved.boundingBox();
  const nc = mul(add(nb.min, nb.max), 0.5);
  t = add(t, [c[0] - nc[0], c[1] - nc[1], -nb.min[2]]);
  return mat4(R, t);
}

// ---------------------------------------------------------------- threads
// ISO metric 60° profile, built as a helical sweep so the flanks follow the helix.
function threadSection(D, P, n) {
  const H = 0.8660254 * P, R = D / 2, depth = 5 * H / 8;
  const a = P / 8, b = P / 8 + 5 * P / 16, c = b + P / 8, d = b + P / 8 + 5 * P / 16;
  const f = u0 => {
    const u = ((u0 % P) + P) % P;
    if (u < a || u >= d) return R - depth;
    if (u < b) return R - depth + depth * (u - a) / (b - a);
    if (u < c) return R;
    return R - depth * (u - c) / (d - c);
  };
  const pts = [];
  for (let i = 0; i < n; i++) { const th = 2 * Math.PI * i / n, r = f(th / (2 * Math.PI) * P); pts.push([r * Math.cos(th), r * Math.sin(th)]); }
  return { pts, depth };
}

// Triangles closing the flat cut between a column of profile corners and the axis
// (both lists of [index, z], z increasing, same first and last z).
function threadWall(outer, axis, flip, out) {
  let i = 0, j = 0;
  while (i < outer.length - 1 || j < axis.length - 1) {
    let t;
    if (j === axis.length - 1 || (i < outer.length - 1 && outer[i + 1][1] <= axis[j + 1][1])) { t = [outer[i][0], outer[i + 1][0], axis[j][0]]; i++; }
    else { t = [outer[i][0], axis[j + 1][0], axis[j][0]]; j++; }
    if (flip) t.reverse();
    out.push(...t);
  }
}

// Right-hand ISO thread as a closed mesh: the four profile corners (root end, crest start,
// crest end, root start) of every turn are helices sampled at N angles, stitched into one
// band, closed at both ends by a helicoid and a flat cut. Reaches past z=0 and z=L by about
// a pitch; the caller clips it. Surface radius: f(phi*P/2pi - z). Same as printing.py.
function helicalThread(D, P, L, N) {
  const H = 0.8660254 * P, R = D / 2, Rm = R - 5 * H / 8;
  const U = [14 * P / 16, 9 * P / 16, 7 * P / 16, 2 * P / 16], RR = [Rm, R, R, Rm];
  const M = Math.ceil(L / P - 1e-9) + 3, J = 4 * M, zb = -P;
  const nv = J * N + 2 * (N + 1), v = new Float64Array(nv * 3);
  const idx = (j, k) => j * N + k;
  for (let j = 0; j < J; j++) for (let k = 0; k < N; k++) {
    const phi = 2 * Math.PI * k / N, r = RR[j % 4], p = 3 * idx(j, k);
    v[p] = r * Math.cos(phi); v[p + 1] = r * Math.sin(phi);
    v[p + 2] = zb + Math.floor(j / 4) * P - U[j % 4] + k * P / N;
  }
  const f = [], q = [];
  for (let j = 0; j < J - 1; j++) for (let k = 0; k < N - 1; k++) q.push([idx(j, k), idx(j, k + 1), idx(j + 1, k + 1), idx(j + 1, k)]);
  for (let j = 0; j < J - 5; j++) q.push([idx(j, N - 1), idx(j + 4, 0), idx(j + 5, 0), idx(j + 1, N - 1)]);
  for (const [a, b, c] of q) f.push(a, b, c);
  for (const [a, , c, d] of q) f.push(a, c, d);
  // the two ends: the faces are listed in the same order as printing.py so the meshes match
  const base = J * N, ab = [], at = [];
  for (let s = 0; s <= N; s++) {
    ab.push([base + s, zb - U[0] + s * P / N]);
    at.push([base + N + 1 + s, zb + (M - 1) * P - U[3] + (s - 1) * P / N]);
  }
  for (const [i, z] of ab.concat(at)) v[3 * i + 2] = z;
  const zOf = i => v[3 * i + 2];
  const ob = []; for (let k = 0; k < N; k++) ob.push(idx(0, k)); ob.push(idx(4, 0));
  for (let s = 0; s < N; s++) f.push(ab[s][0], ob[s + 1], ob[s]);
  for (let s = 0; s < N; s++) f.push(ab[s][0], ab[s + 1][0], ob[s + 1]);
  const cb = []; for (let q = 0; q < 5; q++) cb.push([idx(q, 0), zOf(idx(q, 0))]);
  threadWall(cb, ab, false, f);
  const ot = [idx(J - 5, N - 1)]; for (let k = 0; k < N; k++) ot.push(idx(J - 1, k));
  for (let s = 0; s < N; s++) f.push(at[s][0], ot[s], ot[s + 1]);
  for (let s = 0; s < N; s++) f.push(at[s][0], ot[s + 1], at[s + 1][0]);
  const ct = []; for (let q = J - 5; q < J; q++) ct.push([idx(q, N - 1), zOf(idx(q, N - 1))]);
  threadWall(ct, at, true, f);
  const man = manifoldFromArrays(Float32Array.from(v), Uint32Array.from(f));
  if (!man) throw new GeometryError('Could not build this thread. Try a different size or pitch.');
  return man;
}

export function threadedRod(D, P, L, hand = 'right', chamfer = true) {
  if (D <= 0 || P <= 0 || L <= 0) throw new GeometryError('Diameter, pitch and length must be positive.');
  if (P > D / 2) throw new GeometryError('The pitch is too coarse for this diameter.');
  const n = Math.round(Math.min(240, Math.max(90, D * 10)));
  const depth = 5 * 0.8660254 * P / 8;
  let rod = helicalThread(D, P, L, n);
  if (hand !== 'right') rod = rod.mirror([0, 1, 0]);
  const R = D / 2, c = chamfer ? Math.min(depth * 1.1, L / 3) : 0;
  const prof = chamfer ? [[0, 0], [R - c, 0], [R + 0.01, c], [R + 0.01, L - c], [R - c, L], [0, L]]
    : [[0, 0], [R + 0.01, 0], [R + 0.01, L], [0, L]];
  // turned half a step so its edges fall between the thread's columns (fewer slivers)
  const clip = Manifold.revolve(crossSection([prof], 'Positive'), n).rotate([0, 0, 180 / n]);
  return rod.intersect(clip);
}

export function threadTool(kind, D, P, L, tolerance = 0.2, hand = 'right', chamfer = true) {
  const tol = +tolerance;
  if (kind === 'external') return threadedRod(D - tol, P, L, hand, chamfer);
  const over = 0.6;
  let rod = threadedRod(D + tol, P, L + over, hand, false).translate([0, 0, -L]);
  if (chamfer) {
    const { depth } = threadSection(D + tol, P, 8);
    const R = (D + tol) / 2;
    rod = rod.add(Manifold.cylinder(depth + over, R - depth, R + over, 96).translate([0, 0, -depth]));
  }
  return rod;
}

export { CrossSection };
