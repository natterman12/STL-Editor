import { GeometryError, num, mat4 } from '../geom.js';

function thinResidual(cs, w) {
  const r = w / 2 - 0.005 * w;
  const inner = cs.offset(-r, 'Miter', 2, 0);
  if (inner.isEmpty()) return cs;
  return cs.subtract(inner.offset(r, 'Miter', 2, 0));
}

function thinParts(cs, w, minArea) {
  const res = thinResidual(cs, w);
  return res.isEmpty() ? [] : res.decompose().filter(c => c.area() >= minArea);
}

export function thinScan(solid, t) {
  const minArea = 0.05 * t * t;
  const bb = solid.boundingBox(), lo = bb.min, hi = bb.max;
  const found = [], widths = [];
  let link = 3 * t;
  for (let a = 0; a < 3; a++) {
    const u = (a + 1) % 3, v = (a + 2) % 3;
    const R = [[0, 0, 0], [0, 0, 0], [0, 0, 0]];
    R[0][u] = R[1][v] = R[2][a] = 1;
    const m = solid.transform(mat4(R));
    const ext = hi[a] - lo[a];
    const step = Math.max(t / 2, ext / 60);
    const n = Math.max(1, Math.ceil(ext / step - 1e-9));
    link = Math.max(link, 1.5 * ext / n);
    const flagged = [];
    for (let i = 0; i < n; i++) {
      const h = lo[a] + (i + 0.5) * ext / n;
      const cs = m.slice(h);
      if (cs.isEmpty()) continue;
      const parts = thinParts(cs, t, minArea);
      if (!parts.length) continue;
      flagged.push(cs);
      for (const c of parts) {
        const b = c.bounds(), p = [0, 0, 0];
        p[a] = h; p[u] = (b.min[0] + b.max[0]) / 2; p[v] = (b.min[1] + b.max[1]) / 2;
        found.push(p);
      }
    }
    if (flagged.length) {
      let aLo = 0, aHi = t;
      for (let k = 0; k < 8; k++) {
        const mid = (aLo + aHi) / 2;
        if (flagged.some(cs => thinParts(cs, mid, minArea).length)) aHi = mid; else aLo = mid;
      }
      widths.push(aHi * 0.99);
    }
  }
  if (!found.length) return { min: null, count: 0, spots: [] };
  // group pieces that sit close together into one spot
  const cell = new Map();
  found.forEach((p, i) => {
    const key = p.map(c => Math.floor(c / link)).join(',');
    if (!cell.has(key)) cell.set(key, []);
    cell.get(key).push(i);
  });
  const parent = found.map((_, i) => i);
  const root = i => { while (parent[i] !== i) { parent[i] = parent[parent[i]]; i = parent[i]; } return i; };
  for (const [key, ids] of cell) {
    const k = key.split(',').map(Number);
    for (let dx = -1; dx <= 1; dx++) for (let dy = -1; dy <= 1; dy++) for (let dz = -1; dz <= 1; dz++) {
      const other = cell.get(`${k[0] + dx},${k[1] + dy},${k[2] + dz}`);
      if (!other) continue;
      for (const i of ids) for (const j of other) if (i < j && Math.hypot(found[i][0] - found[j][0], found[i][1] - found[j][1], found[i][2] - found[j][2]) <= link) parent[root(j)] = root(i);
    }
  }
  const groups = new Map();
  found.forEach((p, i) => { const r = root(i); if (!groups.has(r)) groups.set(r, []); groups.get(r).push(p); });
  const r2 = x => Math.round(x * 100) / 100;
  const spots = [...groups.values()].map(pts => [pts.length, [0, 1, 2].map(k => r2((Math.min(...pts.map(q => q[k])) + Math.max(...pts.map(q => q[k]))) / 2))]);
  spots.sort((a, b) => (b[0] - a[0]) || (a[1][0] - b[1][0]) || (a[1][1] - b[1][1]) || (a[1][2] - b[1][2]));
  return { min: Math.round(Math.max(...widths) * 1000) / 1000, count: spots.length, spots: spots.slice(0, 10).map(s => s[1]) };
}

function f_thincheck(state, fid, p, res) {
  const bid = p.body;
  if (!bid) throw new GeometryError('Choose a body to check.');
  const t = num(p.min, 0.8);
  if (t <= 0) throw new GeometryError('The minimum thickness must be more than zero.');
  const r = thinScan(state.body(bid).requireSolid(), t);
  res.thin = r;
  if (!r.count) { res.info = `Nothing thinner than ${t} mm found.`; return; }
  const where = r.spots.slice(0, 3).map(s => `X ${s[0]}, Y ${s[1]}, Z ${s[2]}`).join('; ');
  res.warning = `${r.count} thin spot${r.count !== 1 ? 's' : ''} under ${t} mm. Thinnest: about ${r.min} mm. Near ${where} (mm).`;
}

import { register } from './registry.js';
register('thincheck', f_thincheck, 'Thin-wall Check');
