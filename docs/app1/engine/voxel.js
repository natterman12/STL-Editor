// Voxel tools for the web edition: Hollow and Repair › Rebuild (ports of the voxel code in
// stleditor/geometry.py). Grids are flat typed arrays, x fastest: index = i + nx*(j + ny*k),
// and voxel (i,j,k) has its centre at origin + (i,j,k)*pitch.
import { GeometryError } from './geom.js';
import { Manifold } from './wasm.js';

// browsers have far less memory than the desktop engine (which allows 300 M voxels)
export const MAX_VOXELS = 40e6;

function gridFor(lo, hi, pitch, pad = 2) {
  const n = [0, 1, 2].map(k => Math.ceil((hi[k] - lo[k]) / pitch) + 1 + 2 * pad);
  const origin = [0, 1, 2].map(k => lo[k] - pad * pitch);
  return { n, origin };
}

function checkGrid(n) {
  if (n[0] * n[1] * n[2] > MAX_VOXELS)
    throw new GeometryError('Resolution too fine for this model size in the browser. Choose a coarser Quality, or use the desktop app.');
}

export function autoPitch(v, target = 180) {
  const [lo, hi] = bbox(v);
  return Math.max(Math.hypot(hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]) / target, 0.05);
}

function bbox(v) {
  const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
  for (let i = 0; i < v.length; i += 3) for (let k = 0; k < 3; k++) { const x = v[i + k]; if (x < lo[k]) lo[k] = x; if (x > hi[k]) hi[k] = x; }
  return [lo, hi];
}

// ---------------------------------------------------------------- occupancy
/** Even-odd scanline fill of polygons onto slice k of `out` (rows along x, as the desktop does). */
function rasterEvenOdd(polys, ox, oy, pitch, nx, ny, out, base) {
  const rows = new Array(nx);
  for (const poly of polys) {
    const m = poly.length;
    for (let e = 0; e < m; e++) {
      const a = poly[e], b = poly[(e + 1) % m];
      const gx0 = (a[0] - ox) / pitch, gy0 = (a[1] - oy) / pitch, gx1 = (b[0] - ox) / pitch, gy1 = (b[1] - oy) / pitch;
      const lo = Math.max(Math.ceil(Math.min(gx0, gx1)), 0), hi = Math.min(Math.ceil(Math.max(gx0, gx1)) - 1, nx - 1);
      const dx = gx1 - gx0 || 1;
      for (let r = lo; r <= hi; r++) {
        const t = (r - gx0) / dx;
        (rows[r] || (rows[r] = [])).push(gy0 + t * (gy1 - gy0));
      }
    }
  }
  for (let r = 0; r < nx; r++) {
    const ys = rows[r];
    if (!ys || ys.length % 2) continue;
    ys.sort((p, q) => p - q);
    for (let s = 0; s < ys.length; s += 2) {
      const ja = Math.min(Math.max(Math.ceil(ys[s]), 0), ny), jb = Math.min(Math.max(Math.ceil(ys[s + 1]), 0), ny);
      for (let j = ja; j < jb; j++) out[base + r + nx * j] = 1;
    }
  }
}

/** Exact inside/outside grid for a watertight solid, from Manifold slices. */
export function solidOccupancy(solid, pitch) {
  const bb = solid.boundingBox();
  const { n, origin } = gridFor(bb.min, bb.max, pitch);
  checkGrid(n);
  const [nx, ny, nz] = n;
  const occ = new Uint8Array(nx * ny * nz);
  for (let k = 0; k < nz; k++) {
    const z = origin[2] + k * pitch;
    if (z <= bb.min[2] || z >= bb.max[2]) continue;
    const cs = solid.slice(z);
    const polys = cs.toPolygons();
    cs.delete();
    rasterEvenOdd(polys, origin[0], origin[1], pitch, nx, ny, occ, k * nx * ny);
  }
  return { occ, origin, n };
}

/** Calls fn(x, y, z, tri) for points covering every triangle no further than `spacing` apart. */
function surfacePoints(v, f, spacing, fn) {
  const nt = f.length / 3;
  for (let t = 0; t < nt; t++) {
    const a = f[t * 3] * 3, b = f[t * 3 + 1] * 3, c = f[t * 3 + 2] * 3;
    const ax = v[a], ay = v[a + 1], az = v[a + 2], bx = v[b], by = v[b + 1], bz = v[b + 2], cx = v[c], cy = v[c + 1], cz = v[c + 2];
    const e = Math.max(Math.hypot(bx - ax, by - ay, bz - az), Math.hypot(cx - bx, cy - by, cz - bz), Math.hypot(ax - cx, ay - cy, az - cz));
    const k = Math.min(Math.max(Math.ceil(e / spacing), 1), 400);
    fn(ax, ay, az, t); fn(bx, by, bz, t); fn(cx, cy, cz, t);
    if (k === 1) { fn((ax + bx + cx) / 3, (ay + by + cy) / 3, (az + bz + cz) / 3, t); continue; }
    for (let i = 0; i <= k; i++) for (let j = 0; i + j <= k; j++) {
      const u = i / k, w = j / k, s = 1 - u - w;
      fn(ax * s + bx * u + cx * w, ay * s + by * u + cy * w, az * s + bz * u + cz * w, t);
    }
  }
}

/** L1 (city-block) distance, in voxels, from every voxel to the nearest voxel where `seed` is 1
 *  (or, with outsideIsSeed, also to the grid border). Exact: L1 is separable axis by axis. */
function l1Distance(seed, n, outsideIsSeed = false) {
  const [nx, ny, nz] = n, N = nx * ny * nz, BIG = 1 << 29;
  const d = new Int32Array(N);
  for (let i = 0; i < N; i++) d[i] = seed[i] ? 0 : BIG;
  const pass = (count, len, start, stride) => {
    for (let l = 0; l < count; l++) {
      const s0 = start(l);
      let prev = outsideIsSeed ? 0 : BIG;
      for (let i = 0; i < len; i++) { const p = s0 + i * stride; const c = Math.min(d[p], prev + 1); d[p] = c; prev = c; }
      prev = outsideIsSeed ? 0 : BIG;
      for (let i = len - 1; i >= 0; i--) { const p = s0 + i * stride; const c = Math.min(d[p], prev + 1); d[p] = c; prev = c; }
    }
  };
  pass(ny * nz, nx, l => l * nx, 1);
  pass(nx * nz, ny, l => (l % nx) + Math.floor(l / nx) * nx * ny, nx);
  pass(nx * ny, nz, l => l, nx * ny);
  return d;
}

/** Fill the cavities of a mask: voxels not reachable from the grid border through empty
 *  voxels (6-connected), like scipy's binary_fill_holes. */
function fillHoles(mask, n) {
  const [nx, ny, nz] = n, N = nx * ny * nz;
  const out = new Uint8Array(N).fill(1);
  const q = new Int32Array(N);
  let head = 0, tail = 0;
  const push = p => { if (out[p] && !mask[p]) { out[p] = 0; q[tail++] = p; } };
  for (let k = 0; k < nz; k++) for (let j = 0; j < ny; j++) for (let i = 0; i < nx; i++) {
    if (i === 0 || j === 0 || k === 0 || i === nx - 1 || j === ny - 1 || k === nz - 1) push(i + nx * (j + ny * k));
  }
  const sy = nx, sz = nx * ny;
  while (head < tail) {
    const p = q[head++];
    const i = p % nx, j = Math.floor(p / nx) % ny, k = Math.floor(p / sz);
    if (i > 0) push(p - 1); if (i < nx - 1) push(p + 1);
    if (j > 0) push(p - sy); if (j < ny - 1) push(p + sy);
    if (k > 0) push(p - sz); if (k < nz - 1) push(p + sz);
  }
  return out;
}

/** Occupancy for any (possibly broken) mesh: rasterise the surface, then fill the inside.
 *  Holes larger than a voxel are closed by growing the surface before filling. */
export function meshOccupancy(v, f, pitch) {
  const [lo, hi] = bbox(v);
  const { n, origin } = gridFor(lo, hi, pitch, 12);
  checkGrid(n);
  const [nx, ny, nz] = n, N = nx * ny * nz;
  const shell = new Uint8Array(N);
  let shellCount = 0;
  surfacePoints(v, f, pitch * 0.5, (x, y, z) => {
    const i = Math.min(Math.max(Math.round((x - origin[0]) / pitch), 0), nx - 1);
    const j = Math.min(Math.max(Math.round((y - origin[1]) / pitch), 0), ny - 1);
    const k = Math.min(Math.max(Math.round((z - origin[2]) / pitch), 0), nz - 1);
    const p = i + nx * (j + ny * k);
    if (!shell[p]) { shell[p] = 1; shellCount++; }
  });
  const dist = l1Distance(shell, n);
  for (const it of [0, 1, 2, 3, 5, 8, 11]) {
    const grown = new Uint8Array(N);
    for (let p = 0; p < N; p++) grown[p] = dist[p] <= it ? 1 : 0;
    let filled = fillHoles(grown, n);
    let inside = 0;
    for (let p = 0; p < N; p++) if (filled[p] && !grown[p]) inside++;
    if (inside > 0.2 * shellCount) {
      if (it) {
        // erode back by the same amount (outside the grid counts as empty), keep the surface
        const nd = l1Distance(filled.map(x => 1 - x), n, true);
        for (let p = 0; p < N; p++) filled[p] = nd[p] > it || shell[p] ? 1 : 0;
      }
      return { occ: filled, origin, n };
    }
  }
  return { occ: fillHoles(shell, n), origin, n };
}

// ---------------------------------------------------------------- distance fields
/** 1D squared distance transform (Felzenszwalb-Huttenlocher) with nearest-site tracking. */
function dt1d(fl, fe, n, d, fo, v, z) {
  let k = -1;
  for (let q = 0; q < n; q++) {
    if (fl[q] === Infinity) continue;
    if (k < 0) { k = 0; v[0] = q; z[0] = -Infinity; z[1] = Infinity; continue; }
    let s;
    for (;;) {
      const p = v[k];
      s = ((fl[q] + q * q) - (fl[p] + p * p)) / (2 * (q - p));
      if (s <= z[k]) k--; else break;   // z[0] is -Infinity, so k never drops below 0
    }
    k++; v[k] = q; z[k] = s; z[k + 1] = Infinity;
  }
  if (k < 0) { for (let q = 0; q < n; q++) { d[q] = Infinity; fo[q] = -1; } return; }
  let j = 0;
  for (let q = 0; q < n; q++) {
    while (z[j + 1] < q) j++;
    const p = v[j];
    d[q] = (q - p) * (q - p) + fl[p];
    fo[q] = fe[p];
  }
}

/** Squared Euclidean distance (in voxels) from every voxel to the nearest seed voxel, and the
 *  index of that seed (the feature transform). */
export function edtFeature(seed, n) {
  const [nx, ny, nz] = n, N = nx * ny * nz;
  const D = new Float32Array(N), F = new Int32Array(N);
  for (let p = 0; p < N; p++) { if (seed[p]) { D[p] = 0; F[p] = p; } else { D[p] = Infinity; F[p] = -1; } }
  const L = Math.max(nx, ny, nz);
  const fl = new Float64Array(L), fe = new Int32Array(L), d = new Float64Array(L), fo = new Int32Array(L);
  const v = new Int32Array(L), z = new Float64Array(L + 1);
  const axis = (count, len, start, stride) => {
    for (let l = 0; l < count; l++) {
      const s0 = start(l);
      let any = false;
      for (let i = 0; i < len; i++) { const p = s0 + i * stride; fl[i] = D[p]; fe[i] = F[p]; if (fl[i] !== Infinity) any = true; }
      if (!any) continue;
      dt1d(fl, fe, len, d, fo, v, z);
      for (let i = 0; i < len; i++) { const p = s0 + i * stride; D[p] = d[i]; F[p] = fo[i]; }
    }
  };
  axis(ny * nz, nx, l => l * nx, 1);
  axis(nx * nz, ny, l => (l % nx) + Math.floor(l / nx) * nx * ny, nx);
  axis(nx * ny, nz, l => l, nx * ny);
  return { D, F };
}

function pointTriDist2(px, py, pz, v, f, t) {
  const a = f[t * 3] * 3, b = f[t * 3 + 1] * 3, c = f[t * 3 + 2] * 3;
  const ax = v[a], ay = v[a + 1], az = v[a + 2];
  const abx = v[b] - ax, aby = v[b + 1] - ay, abz = v[b + 2] - az;
  const acx = v[c] - ax, acy = v[c + 1] - ay, acz = v[c + 2] - az;
  const apx = px - ax, apy = py - ay, apz = pz - az;
  const d1 = abx * apx + aby * apy + abz * apz, d2 = acx * apx + acy * apy + acz * apz;
  let qx, qy, qz;
  if (d1 <= 0 && d2 <= 0) { qx = ax; qy = ay; qz = az; } else {
    const bpx = px - v[b], bpy = py - v[b + 1], bpz = pz - v[b + 2];
    const d3 = abx * bpx + aby * bpy + abz * bpz, d4 = acx * bpx + acy * bpy + acz * bpz;
    const cpx = px - v[c], cpy = py - v[c + 1], cpz = pz - v[c + 2];
    const d5 = abx * cpx + aby * cpy + abz * cpz, d6 = acx * cpx + acy * cpy + acz * cpz;
    const vc = d1 * d4 - d3 * d2, vb = d5 * d2 - d1 * d6, va = d3 * d6 - d5 * d4;
    if (d3 >= 0 && d4 <= d3) { qx = v[b]; qy = v[b + 1]; qz = v[b + 2]; }
    else if (vc <= 0 && d1 >= 0 && d3 <= 0) { const s = d1 / (d1 - d3); qx = ax + s * abx; qy = ay + s * aby; qz = az + s * abz; }
    else if (d6 >= 0 && d5 <= d6) { qx = v[c]; qy = v[c + 1]; qz = v[c + 2]; }
    else if (vb <= 0 && d2 >= 0 && d6 <= 0) { const s = d2 / (d2 - d6); qx = ax + s * acx; qy = ay + s * acy; qz = az + s * acz; }
    else if (va <= 0 && (d4 - d3) >= 0 && (d5 - d6) >= 0) {
      const s = (d4 - d3) / ((d4 - d3) + (d5 - d6));
      qx = v[b] + s * (v[c] - v[b]); qy = v[b + 1] + s * (v[c + 1] - v[b + 1]); qz = v[b + 2] + s * (v[c + 2] - v[b + 2]);
    } else {
      const den = 1 / (va + vb + vc), s = vb * den, w = vc * den;
      qx = ax + abx * s + acx * w; qy = ay + aby * s + acy * w; qz = az + abz * s + acz * w;
    }
  }
  return (px - qx) ** 2 + (py - qy) ** 2 + (pz - qz) ** 2;
}

// ---------------------------------------------------------------- iso-surface
/** Iso-surface {field > level} of a scalar grid as a Manifold, using Manifold's own level-set
 *  mesher on the trilinear interpolation of the grid (always a closed 2-manifold, and with far
 *  better triangles than marching tetrahedra). The grid border must be outside. */
export function fieldToManifold(field, n, origin, pitch, level = 0, edge = pitch) {
  const [nx, ny, nz] = n, sy = nx, sz = nx * ny;
  const outside = level - 1;
  const sdf = p => {
    let x = (p[0] - origin[0]) / pitch, y = (p[1] - origin[1]) / pitch, z = (p[2] - origin[2]) / pitch;
    if (!(x >= 0 && y >= 0 && z >= 0 && x < nx - 1 && y < ny - 1 && z < nz - 1)) return outside - level;
    const i = x | 0, j = y | 0, k = z | 0;
    x -= i; y -= j; z -= k;
    const b = i + nx * (j + ny * k);
    const c00 = field[b] + (field[b + 1] - field[b]) * x, c10 = field[b + sy] + (field[b + sy + 1] - field[b + sy]) * x;
    const c01 = field[b + sz] + (field[b + sz + 1] - field[b + sz]) * x, c11 = field[b + sz + sy] + (field[b + sz + sy + 1] - field[b + sz + sy]) * x;
    const c0 = c00 + (c10 - c00) * y, c1 = c01 + (c11 - c01) * y;
    return c0 + (c1 - c0) * z - level;
  };
  let any = false;
  for (let p = 0; p < field.length; p++) if (field[p] > level) { any = true; break; }
  if (!any) throw new GeometryError('Nothing was left after the voxel step. Try a finer Quality.');
  const hi = [origin[0] + (nx - 1) * pitch, origin[1] + (ny - 1) * pitch, origin[2] + (nz - 1) * pitch];
  const man = Manifold.levelSet(sdf, { min: origin, max: hi }, edge, 0, pitch * 0.02);
  if (man.status() !== 'NoError' || man.isEmpty()) throw new GeometryError('The voxel step produced an invalid mesh.');
  return man;
}

/** In-place separable Gaussian blur (scipy's default truncate = 4 standard deviations). */
export function gaussian(field, n, sigma) {
  const r = Math.floor(4 * sigma + 0.5), w = [];
  let s = 0;
  for (let i = -r; i <= r; i++) { const x = Math.exp(-i * i / (2 * sigma * sigma)); w.push(x); s += x; }
  for (let i = 0; i < w.length; i++) w[i] /= s;
  const [nx, ny, nz] = n, L = Math.max(nx, ny, nz), line = new Float32Array(L + 2 * r);
  const axis = (count, len, start, stride) => {
    for (let l = 0; l < count; l++) {
      const s0 = start(l);
      for (let i = 0; i < len; i++) line[i + r] = field[s0 + i * stride];
      for (let i = 0; i < r; i++) { line[r - 1 - i] = line[r + Math.min(i, len - 1)]; line[r + len + i] = line[r + len - 1 - Math.min(i, len - 1)]; }  // reflect
      for (let i = 0; i < len; i++) { let a = 0; for (let m = 0; m <= 2 * r; m++) a += w[m] * line[i + m]; field[s0 + i * stride] = a; }
    }
  };
  axis(ny * nz, nx, l => l * nx, 1);
  axis(nx * nz, ny, l => (l % nx) + Math.floor(l / nx) * nx * ny, nx);
  axis(nx * ny, nz, l => l, nx * ny);
}

// ---------------------------------------------------------------- tools
/** Rebuild any mesh (open, self-intersecting, multi-shell) as one watertight solid. */
export function voxelRebuild(v, f, pitch, smooth = 0.6) {
  const { occ, origin, n } = meshOccupancy(v, f, pitch);
  const field = Float32Array.from(occ);
  if (smooth > 0) gaussian(field, n, smooth);
  // triangles a little bigger than the grid: the blurred field has no finer detail, and it
  // gives about as many triangles as the desktop's marching cubes without needing to simplify
  return fieldToManifold(field, n, origin, pitch, 0.5, pitch * 1.4);
}

/** Shell a solid inward with the given wall thickness. The outer surface is kept exactly;
 *  the cavity comes from a voxel distance field refined to exact distances near its wall. */
export function hollow(solid, thickness, pitch = null, cavityOnly = false) {   // cavityOnly: the cavity itself (Shell)
  if (!(thickness > 0)) throw new GeometryError('Wall thickness must be positive.');
  const m = solid.getMesh(), np = m.numProp;
  const nvx = m.vertProperties.length / np;
  const v = new Float32Array(nvx * 3);
  for (let i = 0; i < nvx; i++) { v[i * 3] = m.vertProperties[i * np]; v[i * 3 + 1] = m.vertProperties[i * np + 1]; v[i * 3 + 2] = m.vertProperties[i * np + 2]; }
  const f = m.triVerts;
  const [lo, hi] = bbox(v), size = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]], diag = Math.hypot(...size);
  if (thickness * 2 >= Math.min(...size)) throw new GeometryError('Wall thickness is too large for this body; nothing would be left hollow.');
  if (!pitch) pitch = Math.max(Math.min(thickness / 3, diag / 120), diag / 400);
  const { occ, origin, n } = solidOccupancy(solid, pitch);
  const [nx, ny, nz] = n, N = nx * ny * nz;

  // voxels the surface passes through, and which triangles pass through each
  const surf = new Uint8Array(N), stamp = new Int32Array(N).fill(-1);
  let pv = new Int32Array(1 << 16), pt = new Int32Array(1 << 16), np2 = 0;
  surfacePoints(v, f, pitch / 3, (x, y, z, t) => {
    const i = Math.round((x - origin[0]) / pitch), j = Math.round((y - origin[1]) / pitch), k = Math.round((z - origin[2]) / pitch);
    if (i < 0 || j < 0 || k < 0 || i >= nx || j >= ny || k >= nz) return;
    const p = i + nx * (j + ny * k);
    surf[p] = 1;
    if (stamp[p] === t) return;
    stamp[p] = t;
    if (np2 === pv.length) { const a = new Int32Array(pv.length * 2); a.set(pv); pv = a; const b = new Int32Array(pt.length * 2); b.set(pt); pt = b; }
    pv[np2] = p; pt[np2] = t; np2++;
  });
  // CSR lists of triangles per voxel (reusing `stamp` as the start offsets)
  const cnt = new Map();
  for (let e = 0; e < np2; e++) cnt.set(pv[e], (cnt.get(pv[e]) || 0) + 1);
  const start = new Map();
  let acc = 0;
  for (const [p, c] of cnt) { start.set(p, acc); acc += c; }
  const fill = new Map(start), tris = new Int32Array(np2);
  for (let e = 0; e < np2; e++) { const s = fill.get(pv[e]); tris[s] = pt[e]; fill.set(pv[e], s + 1); }

  const { D, F } = edtFeature(surf, n);
  const field = new Float32Array(N);
  for (let p = 0; p < N; p++) {
    if (!occ[p]) { field[p] = -pitch; continue; }
    const coarse = Math.sqrt(D[p]) * pitch - thickness;
    if (Math.abs(coarse) >= 2.5 * pitch || F[p] < 0) { field[p] = coarse; continue; }
    // exact distance to the triangles around the nearest surface voxel
    const i = p % nx, j = Math.floor(p / nx) % ny, k = Math.floor(p / (nx * ny));
    const px = origin[0] + i * pitch, py = origin[1] + j * pitch, pz = origin[2] + k * pitch;
    const s = F[p], si = s % nx, sj = Math.floor(s / nx) % ny, sk = Math.floor(s / (nx * ny));
    let best = Infinity;
    for (let dk = -1; dk <= 1; dk++) for (let dj = -1; dj <= 1; dj++) for (let di = -1; di <= 1; di++) {
      const a = si + di, b = sj + dj, c = sk + dk;
      if (a < 0 || b < 0 || c < 0 || a >= nx || b >= ny || c >= nz) continue;
      const q = a + nx * (b + ny * c);
      const st = start.get(q);
      if (st === undefined) continue;
      const end = st + cnt.get(q);
      for (let e = st; e < end; e++) { const d2 = pointTriDist2(px, py, pz, v, f, tris[e]); if (d2 < best) best = d2; }
    }
    field[p] = best < Infinity ? Math.sqrt(best) - thickness : coarse;
  }
  let any = false;
  for (let p = 0; p < N; p++) if (field[p] > 0) { any = true; break; }
  if (!any) throw new GeometryError('Wall thickness is too large for this body; nothing would be left hollow.');
  const cavity = fieldToManifold(field, n, origin, pitch, 0).simplify(pitch * 0.1);   // more and the cavity drifts (0.25 grows it by 0.2 %)
  return cavityOnly ? cavity : solid.subtract(cavity);
}
