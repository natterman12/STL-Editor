// v1.3 tools (port of stleditor/meshtools.py): project outlines, fit/resize round holes and pins,
// offset bodies, text outlines (opentype.js).
import { Manifold, crossSection } from './wasm.js';
import { GeometryError, unit, cross, dot, add, mul, sub, norm, worldToFrame, frameMatrix, triNormals, triAdjacency } from './geom.js';
import opentype from '../vendor/opentype.module.js';

// ---------------------------------------------------------------- project outline
export function outline(solids, frame, mode = 'section', tolerance = 0.02, offset = 0) {
  let cs = null;
  const M = worldToFrame(...frame);
  const locs = solids.map(s => s.transform(M));
  if (mode === 'section') {
    // a sketch drawn on a face lies exactly on the surface: look just inside it then
    for (const h of [0, -0.01, 0.01]) {
      cs = null;
      for (const loc of locs) { const c = loc.slice(h); cs = cs ? cs.add(c) : c; }
      if (cs && !cs.isEmpty()) break;
    }
  } else for (const loc of locs) { const c = loc.project(); cs = cs ? cs.add(c) : c; }
  if (!cs || cs.isEmpty()) {
    throw new GeometryError(mode === 'section' ? "The sketch plane doesn't cut through the body here. Try Silhouette, or sketch on a plane that crosses the part." : 'Nothing to project.');
  }
  if (Math.abs(offset) > 1e-9) cs = cs.offset(+offset, 'Round', 2, 64);
  if (tolerance > 0) cs = cs.simplify(+tolerance);
  const polys = cs.toPolygons().map(p => p.map(q => [q[0], q[1]])).filter(p => p.length >= 3);
  if (!polys.length) throw new GeometryError('The outline came out empty.');
  return polys;
}

export function fitCircle(p) {
  // Kasa least squares: x² + y² = a x + b y + c
  let Sxx = 0, Sxy = 0, Syy = 0, Sx = 0, Sy = 0, Sxz = 0, Syz = 0, Sz = 0;
  const n = p.length;
  for (const [x, y] of p) { const z = x * x + y * y; Sxx += x * x; Sxy += x * y; Syy += y * y; Sx += x; Sy += y; Sxz += x * z; Syz += y * z; Sz += z; }
  const A = [[Sxx, Sxy, Sx], [Sxy, Syy, Sy], [Sx, Sy, n]], B = [Sxz, Syz, Sz];
  const sol = solve3(A, B);
  if (!sol) return [0, 0, -1];
  const cx = sol[0] / 2, cy = sol[1] / 2, r2 = sol[2] + cx * cx + cy * cy;
  return [cx, cy, r2 > 0 ? Math.sqrt(r2) : -1];
}

function solve3(A, b) {
  const M = A.map((r, i) => [...r, b[i]]);
  for (let c = 0; c < 3; c++) {
    let piv = c;
    for (let r = c + 1; r < 3; r++) if (Math.abs(M[r][c]) > Math.abs(M[piv][c])) piv = r;
    if (Math.abs(M[piv][c]) < 1e-14) return null;
    [M[c], M[piv]] = [M[piv], M[c]];
    for (let r = 0; r < 3; r++) if (r !== c) { const k = M[r][c] / M[c][c]; for (let j = c; j < 4; j++) M[r][j] -= k * M[c][j]; }
  }
  return [M[0][3] / M[0][0], M[1][3] / M[1][1], M[2][3] / M[2][2]];
}

function asCircle(p) {
  if (p.length < 12) return null;
  const [cx, cy, r] = fitCircle(p);
  if (r <= 0) return null;
  const dev = Math.max(...p.map(q => Math.abs(Math.hypot(q[0] - cx, q[1] - cy) - r)));
  return dev < Math.max(0.01, r * 0.01) ? [+cx.toFixed(4), +cy.toFixed(4), +r.toFixed(4)] : null;
}

// Part edges a sketch can measure from: the outline of the faces lying on the sketch plane;
// otherwise where the plane cuts the parts; otherwise their shadow on it. Round loops come back
// as circles, everything else as straight segments (sketch coordinates).
export function refGeometry(solids, frame, maxSegs = 3000) {
  const M = worldToFrame(...frame);
  const tris = [];
  let total = 0;
  for (const s of solids) {
    const m = s.transform(M).getMesh(), P = m.vertProperties, T = m.triVerts, np = m.numProp;
    total += T.length / 3;
    for (let t = 0; t < T.length; t += 3) {
      const q = [0, 1, 2].map(k => T[t + k] * np);
      if (q.some(i => Math.abs(P[i + 2]) > 1e-3)) continue;
      const uv = q.map(i => [P[i], P[i + 1]]);
      const ar = (uv[1][0] - uv[0][0]) * (uv[2][1] - uv[0][1]) - (uv[1][1] - uv[0][1]) * (uv[2][0] - uv[0][0]);
      if (Math.abs(ar) < 1e-12) continue;
      tris.push(ar < 0 ? uv.reverse() : uv);
    }
  }
  let kind = 'face', polys = [];
  if (tris.length) polys = crossSection(tris, 'NonZero').toPolygons().map(p => p.map(q => [q[0], q[1]]));
  if (!polys.length) {
    kind = 'section';
    try { polys = outline(solids, frame, 'section', 0.02); } catch { polys = []; }
  }
  if (!polys.length && total <= 300000) {   // a shadow of a huge scan would take too long
    kind = 'silhouette';
    try { polys = outline(solids, frame, 'silhouette', 0.02); } catch { polys = []; }
  }
  const ents = tol => {
    const ps = tol ? crossSection(polys, 'EvenOdd').simplify(tol).toPolygons().map(p => p.map(q => [q[0], q[1]])) : polys;
    const out = [];
    const r5 = v => +(+v).toFixed(5);
    for (const p of ps) {
      if (p.length < 3) continue;
      const c = asCircle(p);
      if (c) { out.push({ type: 'circle', cx: c[0], cy: c[1], r: c[2] }); continue; }
      for (let i = 0; i < p.length; i++) {
        const a = p[i], b = p[(i + 1) % p.length];
        if (Math.hypot(b[0] - a[0], b[1] - a[1]) > 1e-6) out.push({ type: 'line', p1: [r5(a[0]), r5(a[1])], p2: [r5(b[0]), r5(b[1])] });
      }
    }
    return out;
  };
  let out = ents(0.005);
  for (const tol of [0.05, 0.2, 1]) { if (out.length <= maxSegs) break; out = ents(tol); }
  return { kind: polys.length ? kind : null, refs: out.slice(0, maxSegs) };
}

export function outlineEntities(polys, newId, construction = false) {
  return polys.map(p => {
    const c = asCircle(p);
    const e = c ? { id: newId(), type: 'circle', cx: c[0], cy: c[1], r: c[2] }
      : { id: newId(), type: 'polyline', closed: true, pts: p.map(([a, b]) => [+a.toFixed(4), +b.toFixed(4)]) };
    if (construction) e.construction = true;
    e.projected = true;
    return e;
  });
}

// ---------------------------------------------------------------- round holes and pins
export function nearestTriangle(v, f, point) {
  let best = 0, bd = Infinity;
  for (let t = 0; t < f.length / 3; t++) {
    let cx = 0, cy = 0, cz = 0;
    for (let k = 0; k < 3; k++) { const p = f[t * 3 + k] * 3; cx += v[p]; cy += v[p + 1]; cz += v[p + 2]; }
    const d = (cx / 3 - point[0]) ** 2 + (cy / 3 - point[1]) ** 2 + (cz / 3 - point[2]) ** 2;
    if (d < bd) { bd = d; best = t; }
  }
  return best;
}

function eigSym3(A) {
  // Jacobi rotations; returns {values ascending, vectors (columns as arrays)}
  const a = A.map(r => [...r]);
  const V = [[1, 0, 0], [0, 1, 0], [0, 0, 1]];
  for (let sweep = 0; sweep < 50; sweep++) {
    let off = 0;
    for (let i = 0; i < 3; i++) for (let j = i + 1; j < 3; j++) off += a[i][j] ** 2;
    if (off < 1e-20) break;
    for (let p = 0; p < 3; p++) for (let q = p + 1; q < 3; q++) {
      if (Math.abs(a[p][q]) < 1e-30) continue;
      const th = (a[q][q] - a[p][p]) / (2 * a[p][q]);
      const t = Math.sign(th || 1) / (Math.abs(th) + Math.sqrt(th * th + 1));
      const c = 1 / Math.sqrt(t * t + 1), s = t * c;
      for (let k = 0; k < 3; k++) { const akp = a[k][p], akq = a[k][q]; a[k][p] = c * akp - s * akq; a[k][q] = s * akp + c * akq; }
      for (let k = 0; k < 3; k++) { const apk = a[p][k], aqk = a[q][k]; a[p][k] = c * apk - s * aqk; a[q][k] = s * apk + c * aqk; }
      for (let k = 0; k < 3; k++) { const vkp = V[k][p], vkq = V[k][q]; V[k][p] = c * vkp - s * vkq; V[k][q] = s * vkp + c * vkq; }
    }
  }
  const idx = [0, 1, 2].sort((i, j) => a[i][i] - a[j][j]);
  return { values: idx.map(i => a[i][i]), vectors: idx.map(i => [V[0][i], V[1][i], V[2][i]]) };
}

const frameAround = axis => {
  const ref = Math.abs(axis[0]) < 0.9 ? [1, 0, 0] : [0, 1, 0];
  const ex = unit(cross(axis, ref));
  return [ex, cross(axis, ex)];
};

export function fitCylinder(v, f, tri, maxAngle = 35) {
  const { n, area } = triNormals(v, f);
  const adj = triAdjacency(f);
  const cosT = Math.cos(maxAngle * Math.PI / 180);
  const seen = new Uint8Array(f.length / 3);
  const region = [];
  const stack = [tri];
  seen[tri] = 1;
  while (stack.length) {
    const t = stack.pop();
    region.push(t);
    for (const u of adj[t]) {
      if (seen[u]) continue;
      if (n[t * 3] * n[u * 3] + n[t * 3 + 1] * n[u * 3 + 1] + n[t * 3 + 2] * n[u * 3 + 2] >= cosT) { seen[u] = 1; stack.push(u); }
    }
  }
  if (region.length < 6) throw new GeometryError('Click the curved wall of a round hole or pin.');
  const M = [[0, 0, 0], [0, 0, 0], [0, 0, 0]];
  for (const t of region) { const w = area[t]; for (let i = 0; i < 3; i++) for (let j = 0; j < 3; j++) M[i][j] += w * n[t * 3 + i] * n[t * 3 + j]; }
  const { values, vectors } = eigSym3(M);
  const axis = unit(vectors[0]);
  if (values[0] > 0.08 * values[1] || values[1] < 0.25 * values[2]) throw new GeometryError("That surface isn't round like a hole or pin. Click the curved wall itself.");
  const [ex, ey] = frameAround(axis);
  const vids = new Set();
  for (const t of region) for (let k = 0; k < 3; k++) vids.add(f[t * 3 + k]);
  const pts = [...vids].map(i => [v[i * 3], v[i * 3 + 1], v[i * 3 + 2]]);
  const p2 = pts.map(p => [dot(p, ex), dot(p, ey)]);
  const [cx, cy, r] = fitCircle(p2);
  if (r <= 0) throw new GeometryError("Couldn't measure that surface.");
  const dev = p2.map(q => Math.abs(Math.hypot(q[0] - cx, q[1] - cy) - r)).sort((a, b) => a - b);
  if (dev[Math.floor(dev.length * 0.9)] > Math.max(0.05, 0.04 * r)) throw new GeometryError("That surface isn't a clean cylinder, so it can't be resized.");
  const bins = new Set(p2.map(q => Math.floor((Math.atan2(q[1] - cy, q[0] - cx) + Math.PI) / (2 * Math.PI) * 36)));
  const cover = bins.size / 36;
  if (cover < 0.5) throw new GeometryError('Only part of a round surface was found. Click the wall of a full hole or pin.');
  const zs = pts.map(p => dot(p, axis));
  const center = add(mul(ex, cx), mul(ey, cy));
  let s = 0;
  for (const t of region) {
    const c = [0, 1, 2].reduce((acc, k) => { const p = f[t * 3 + k] * 3; return add(acc, [v[p] / 3, v[p + 1] / 3, v[p + 2] / 3]); }, [0, 0, 0]);
    const radial = sub(sub(c, mul(axis, dot(c, axis))), center);
    s += dot(radial, [n[t * 3], n[t * 3 + 1], n[t * 3 + 2]]);
  }
  return { center, axis, radius: r, z0: Math.min(...zs), z1: Math.max(...zs), kind: s / region.length < 0 ? 'hole' : 'pin', tris: region, cover };
}

function inside(solid, p, eps = 0.05) {
  const probe = Manifold.cube([eps, eps, eps], true).translate(p);
  return solid.intersect(probe).volume() > 0.5 * eps ** 3;
}

function tube(center, axis0, z0, z1, rIn, rOut, segsN = 128) {
  const h = z1 - z0;
  if (h <= 1e-6 || rOut <= 1e-6) return null;
  let t = Manifold.cylinder(h, rOut, rOut, segsN);
  if (rIn > 1e-6) t = t.subtract(Manifold.cylinder(h + 2, rIn, rIn, segsN).translate([0, 0, -1]));
  const axis = unit(axis0);
  const [ex, ey] = frameAround(axis);
  return t.transform(frameMatrix(add(center, mul(axis, z0)), ex, ey, axis));
}

export function resizeRound(solid, cyl, diameter, fill = false) {
  const c = cyl.center, ax = cyl.axis, r0 = cyl.radius, z0 = cyl.z0, z1 = cyl.z1;
  const delta = Math.max(0.02, r0 * 0.01);
  const addT = t => t ? solid.add(t) : solid, subT = t => t ? solid.subtract(t) : solid;
  if (fill) {
    if (cyl.kind !== 'hole') throw new GeometryError('Fill works on holes. To remove a pin, cut it off with Split or Extrude › Cut.');
    return addT(tube(c, ax, z0, z1, 0, r0 + delta));
  }
  if (!diameter || diameter <= 0) throw new GeometryError('Enter the new diameter.');
  const r1 = +diameter / 2;
  if (Math.abs(r1 - r0) < 1e-4) return solid;
  const at = z => add(c, mul(ax, z));
  if (cyl.kind === 'hole') {
    if (r1 < r0) return addT(tube(c, ax, z0, z1, r1, r0 + delta));
    let lo = z0, hi = z1;
    if (!inside(solid, at(z0 - 0.3))) lo -= 0.5;
    if (!inside(solid, at(z1 + 0.3))) hi += 0.5;
    return subT(tube(c, ax, lo, hi, Math.max(r0 - delta, 0), r1));
  }
  if (r1 < r0) {
    const hi = z1 + (inside(solid, at(z1 + 0.3)) ? 0 : 0.5), lo = z0 - (inside(solid, at(z0 - 0.3)) ? 0 : 0.5);
    return subT(tube(c, ax, lo, hi, r1, r0 + delta + 0.5));
  }
  return addT(tube(c, ax, z0, z1, Math.max(r0 - delta, 0), r1));
}

// ---------------------------------------------------------------- offset body
export function offsetSolid(solid, distance, maxTris = 12000) {
  const d = +distance;
  if (Math.abs(d) < 1e-6) return solid;
  let work = solid, slack = 0;
  if (work.numTri() > maxTris) {
    for (const frac of [0.1, 0.2, 0.3, 0.4]) {
      slack = Math.abs(d) * frac;
      work = solid.simplify(slack);
      if (work.numTri() <= maxTris) break;
    }
  }
  const segsN = 8;
  const r = Math.abs(d) / Math.cos(Math.PI / segsN) + slack;
  const ball = Manifold.sphere(r, segsN);
  if (d > 0) return work.minkowskiSum(ball).add(solid);
  const out = work.minkowskiDifference(ball);
  if (out.isEmpty()) throw new GeometryError(`Shrinking by ${Math.abs(d)} mm leaves nothing of this body.`);
  return out.intersect(solid);
}

// ---------------------------------------------------------------- text
const fontCache = new Map();
let fontLoader = null;   // (name) => ArrayBuffer, set by the worker
export function setFontLoader(fn) { fontLoader = fn; }

async function getFont(name, bold, italic) {
  const key = `${name}|${bold ? 'b' : ''}${italic ? 'i' : ''}`;
  if (fontCache.has(key)) return fontCache.get(key);
  if (!fontLoader) throw new GeometryError('Fonts are not available.');
  const buf = await fontLoader(name, bold, italic);
  if (!buf) throw new GeometryError(`The font "${name}" isn't available.`);
  const font = opentype.parse(buf);
  fontCache.set(key, font);
  return font;
}

function flatten(path, steps = 8) {
  const out = [];
  let cur = null, start = null;
  const close = () => { if (cur && cur.length >= 3) out.push(cur); cur = null; };
  for (const c of path.commands) {
    if (c.type === 'M') { close(); cur = [[c.x, -c.y]]; start = [c.x, -c.y]; }
    else if (c.type === 'L') cur.push([c.x, -c.y]);
    else if (c.type === 'Q') {
      const p0 = cur.at(-1);
      for (let k = 1; k <= steps; k++) { const t = k / steps, m = 1 - t; cur.push([m * m * p0[0] + 2 * m * t * c.x1 + t * t * c.x, m * m * p0[1] + 2 * m * t * -c.y1 + t * t * -c.y]); }
    } else if (c.type === 'C') {
      const p0 = cur.at(-1);
      for (let k = 1; k <= steps; k++) {
        const t = k / steps, m = 1 - t;
        cur.push([m ** 3 * p0[0] + 3 * m * m * t * c.x1 + 3 * m * t * t * c.x2 + t ** 3 * c.x, m ** 3 * p0[1] + 3 * m * m * t * -c.y1 + 3 * m * t * t * -c.y2 + t ** 3 * -c.y]);
      }
    } else if (c.type === 'Z') {
      if (cur && start) { const l = cur.at(-1); if (Math.hypot(l[0] - start[0], l[1] - start[1]) < 1e-9) cur.pop(); }
      close();
    }
  }
  close();
  return out;
}

/** Outline contours of `text` (several lines allowed), baseline of the first line at y = 0. */
export async function textContours(text, family, size = 10, bold = false, italic = false, spacing = 0, align = 'left', lineGap = 1.2) {
  text = String(text || '');
  if (!text.trim()) return [];
  const font = await getFont(family, bold, italic);
  const upm = font.unitsPerEm, scale = size / upm;
  const lines = text.split('\n');
  const perLine = [], widths = [];
  for (const line of lines) {
    let x = 0;
    const conts = [];
    const glyphs = font.stringToGlyphs(line);
    glyphs.forEach((g, i) => {
      const p = g.getPath(x * scale, 0, size);
      conts.push(...flatten(p));
      x += (g.advanceWidth || 0) + (i < glyphs.length - 1 ? (font.getKerningValue(g, glyphs[i + 1]) || 0) : 0) + spacing / scale;
    });
    widths.push(x * scale - (line ? spacing : 0));
    perLine.push(conts);
  }
  const maxw = Math.max(...widths, 0);
  const out = [];
  perLine.forEach((conts, i) => {
    const dx = { left: 0, center: (maxw - widths[i]) / 2, right: maxw - widths[i] }[align] || 0;
    const dy = -i * size * lineGap;
    for (const c of conts) out.push(c.map(([x, y]) => [x + dx, y + dy]));
  });
  return out;
}

export function textExtent(contours) {
  if (!contours.length) return [[0, 0], [0, 0]];
  const lo = [Infinity, Infinity], hi = [-Infinity, -Infinity];
  for (const c of contours) for (const [x, y] of c) { lo[0] = Math.min(lo[0], x); lo[1] = Math.min(lo[1], y); hi[0] = Math.max(hi[0], x); hi[1] = Math.max(hi[1], y); }
  return [lo, hi];
}

export { crossSection, norm };
