/**
 * Generate rust/tests/parity/remesh_golden.json from web/engine/meshedit.js remesh.
 *
 * Reads remesh feature cases with "reference": "web". Error cases (desktop raises)
 * are skipped here; make_golden.py keeps the desktop error record. Successful cases
 * write MeshStats plus remesh/<case>.stl from manifold-3d WASM in Node.
 *
 * Usage:
 *   node dev/parity/js/remesh_golden.mjs
 *   node dev/parity/js/remesh_golden.mjs --check
 */
import fs from 'fs';
import path from 'path';
import { fileURLToPath, pathToFileURL } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '..', '..', '..');
const CASES = path.join(ROOT, 'rust', 'tests', 'parity', 'features', 'remesh.json');
const GOLDEN = path.join(ROOT, 'rust', 'tests', 'parity', 'remesh_golden.json');
const OUT_DIR = path.join(ROOT, 'rust', 'tests', 'parity', 'remesh');
const WEB = path.join(ROOT, 'web');

const load = rel => import(pathToFileURL(path.join(WEB, rel)).href);

function dumps(obj) {
  return JSON.stringify(obj, null, 1) + '\n';
}

function normNewlines(s) {
  return s.replace(/\r\n/g, '\n').replace(/\r/g, '\n');
}

/** MeshStats matching dev/parity/stats.py / stl_parity::mesh_stats. */
function meshStats(vertProps, triVerts, numProp = 3) {
  const nv = vertProps.length / numProp;
  const verts = new Float32Array(nv * 3);
  for (let i = 0; i < nv; i++) {
    verts[i * 3] = vertProps[i * numProp];
    verts[i * 3 + 1] = vertProps[i * numProp + 1];
    verts[i * 3 + 2] = vertProps[i * numProp + 2];
  }
  const nTri = triVerts.length / 3;
  if (nTri === 0) {
    return {
      triangles: 0,
      vertices: nv,
      volume: 0,
      area: 0,
      bbox_min: [0, 0, 0],
      bbox_max: [0, 0, 0],
      watertight: false,
    };
  }
  let volume = 0;
  let area = 0;
  let bmin = [Infinity, Infinity, Infinity];
  let bmax = [-Infinity, -Infinity, -Infinity];
  const edgeCount = new Map();
  const edgeKey = (a, b) => (a < b ? a * 0x100000000 + b : b * 0x100000000 + a);
  for (let t = 0; t < nTri; t++) {
    const i0 = triVerts[t * 3], i1 = triVerts[t * 3 + 1], i2 = triVerts[t * 3 + 2];
    const ax = verts[i0 * 3], ay = verts[i0 * 3 + 1], az = verts[i0 * 3 + 2];
    const bx = verts[i1 * 3], by = verts[i1 * 3 + 1], bz = verts[i1 * 3 + 2];
    const cx = verts[i2 * 3], cy = verts[i2 * 3 + 1], cz = verts[i2 * 3 + 2];
    const cxy = by * cz - bz * cy, cyz = bz * cx - bx * cz, czx = bx * cy - by * cx;
    volume += (ax * cxy + ay * cyz + az * czx) / 6;
    const e1x = bx - ax, e1y = by - ay, e1z = bz - az;
    const e2x = cx - ax, e2y = cy - ay, e2z = cz - az;
    const nx = e1y * e2z - e1z * e2y, ny = e1z * e2x - e1x * e2z, nz = e1x * e2y - e1y * e2x;
    area += 0.5 * Math.hypot(nx, ny, nz);
    for (const [x, y, z] of [[ax, ay, az], [bx, by, bz], [cx, cy, cz]]) {
      if (x < bmin[0]) bmin[0] = x; if (y < bmin[1]) bmin[1] = y; if (z < bmin[2]) bmin[2] = z;
      if (x > bmax[0]) bmax[0] = x; if (y > bmax[1]) bmax[1] = y; if (z > bmax[2]) bmax[2] = z;
    }
    for (const [a, b] of [[i0, i1], [i1, i2], [i2, i0]]) {
      const k = edgeKey(a, b);
      edgeCount.set(k, (edgeCount.get(k) || 0) + 1);
    }
  }
  let watertight = edgeCount.size > 0;
  for (const c of edgeCount.values()) if (c !== 2) { watertight = false; break; }
  return {
    triangles: nTri,
    vertices: nv,
    volume,
    area,
    bbox_min: bmin,
    bbox_max: bmax,
    watertight,
  };
}

function stlBytes(vertProps, triVerts, numProp = 3) {
  const nTri = triVerts.length / 3;
  const buf = Buffer.alloc(84 + nTri * 50);
  buf.write('remesh golden', 0, 'ascii');
  buf.writeUInt32LE(nTri, 80);
  let o = 84;
  for (let t = 0; t < nTri; t++) {
    const i0 = triVerts[t * 3], i1 = triVerts[t * 3 + 1], i2 = triVerts[t * 3 + 2];
    const ax = vertProps[i0 * numProp], ay = vertProps[i0 * numProp + 1], az = vertProps[i0 * numProp + 2];
    const bx = vertProps[i1 * numProp], by = vertProps[i1 * numProp + 1], bz = vertProps[i1 * numProp + 2];
    const cx = vertProps[i2 * numProp], cy = vertProps[i2 * numProp + 1], cz = vertProps[i2 * numProp + 2];
    const e1x = bx - ax, e1y = by - ay, e1z = bz - az;
    const e2x = cx - ax, e2y = cy - ay, e2z = cz - az;
    let nx = e1y * e2z - e1z * e2y, ny = e1z * e2x - e1x * e2z, nz = e1x * e2y - e1y * e2x;
    const ln = Math.hypot(nx, ny, nz) || 1;
    nx /= ln; ny /= ln; nz /= ln;
    buf.writeFloatLE(nx, o); buf.writeFloatLE(ny, o + 4); buf.writeFloatLE(nz, o + 8);
    buf.writeFloatLE(ax, o + 12); buf.writeFloatLE(ay, o + 16); buf.writeFloatLE(az, o + 20);
    buf.writeFloatLE(bx, o + 24); buf.writeFloatLE(by, o + 28); buf.writeFloatLE(bz, o + 32);
    buf.writeFloatLE(cx, o + 36); buf.writeFloatLE(cy, o + 40); buf.writeFloatLE(cz, o + 44);
    buf.writeUInt16LE(0, o + 48);
    o += 50;
  }
  return buf;
}

function applyAnchor(man, shape, params, Manifold) {
  const anchor = params.anchor || 'base';
  if (shape === 'box') {
    const L = +params.length || 10, W = +params.width || 10, H = +params.height || 10;
    const shift = {
      corner: [0, 0, 0],
      base: [-L / 2, -W / 2, 0],
      center: [-L / 2, -W / 2, -H / 2],
    };
    return man.translate(shift[anchor] || shift.base);
  }
  if (shape === 'sphere') {
    const r = +params.radius || 5;
    if (anchor === 'base') return man.translate([0, 0, r]);
    return man;
  }
  if (shape === 'cylinder') {
    const h = +params.height || 10;
    if (anchor === 'center') return man.translate([0, 0, -h / 2]);
    return man;
  }
  return man;
}

function buildSolid(features, Manifold) {
  // First feature is the body source (primitive); remesh is last.
  const prim = features.find(f => f.type === 'primitive');
  if (!prim) throw new Error('remesh_golden: case needs a primitive body');
  const p = prim.params || {};
  const shape = p.shape || 'box';
  const seg = (p.segments | 0) || 96;
  let man;
  if (shape === 'box') {
    const L = +p.length || 10, W = +p.width || 10, H = +p.height || 10;
    man = Manifold.cube([L, W, H], false);
  } else if (shape === 'sphere') {
    man = Manifold.sphere(+p.radius || 5, seg);
  } else if (shape === 'cylinder') {
    const r = +p.radius || 5, h = +p.height || 10;
    man = Manifold.cylinder(h, r, r, seg);
  } else {
    throw new Error(`remesh_golden: unsupported shape ${shape}`);
  }
  man = applyAnchor(man, shape, p, Manifold);
  if (Array.isArray(p.offset) && p.offset.length === 3) {
    man = man.translate(p.offset.map(Number));
  }
  return man;
}

async function build() {
  const W = await load('engine/wasm.js');
  await W.init();
  const { remesh } = await load('engine/meshedit.js');
  const Manifold = W.Manifold;

  const cases = JSON.parse(fs.readFileSync(CASES, 'utf8'));
  const records = {};
  const meshes = {};

  for (const caseDef of cases) {
    if (caseDef.reference !== 'web') continue;
    const remeshFeat = (caseDef.features || []).find(f => f.type === 'remesh');
    if (!remeshFeat) continue;
    const edge = +(remeshFeat.params || {}).edge;
    try {
      const solid = buildSolid(caseDef.features, Manifold);
      const out = remesh(solid, edge);
      const mesh = out.getMesh();
      const numProp = mesh.numProp || 3;
      const stats = meshStats(mesh.vertProperties, mesh.triVerts, numProp);
      const meshRel = `remesh/${caseDef.name}.stl`;
      const bytes = stlBytes(mesh.vertProperties, mesh.triVerts, numProp);
      records[caseDef.name] = { stats, mesh: meshRel };
      meshes[caseDef.name] = bytes;
    } catch (e) {
      // Desktop error path: leave out of remesh_golden; make_golden keeps the error.
      const msg = String((e && e.message) || e);
      if (/Edge length must be positive/i.test(msg) || /more than 3 million triangles/i.test(msg)) {
        continue;
      }
      throw e;
    }
  }

  const ordered = {};
  for (const k of Object.keys(records).sort()) ordered[k] = records[k];
  return { records: ordered, meshes };
}

const check = process.argv.includes('--check');
const { records: golden, meshes } = await build();
const text = dumps(golden);
if (check) {
  const existing = fs.existsSync(GOLDEN) ? fs.readFileSync(GOLDEN, 'utf8') : '';
  if (normNewlines(existing) !== normNewlines(text)) {
    console.error(`remesh_golden: MISMATCH ${path.relative(ROOT, GOLDEN)}`);
    process.exit(1);
  }
  for (const [name, rec] of Object.entries(golden)) {
    const meshPath = path.join(ROOT, 'rust', 'tests', 'parity', rec.mesh);
    if (!fs.existsSync(meshPath)) {
      console.error(`remesh_golden: MISSING mesh ${rec.mesh}`);
      process.exit(1);
    }
    const got = fs.readFileSync(meshPath);
    if (Buffer.compare(got, meshes[name]) !== 0) {
      console.error(`remesh_golden: MISMATCH mesh ${rec.mesh}`);
      process.exit(1);
    }
  }
  console.log(`remesh_golden: ok (${Object.keys(golden).length} cases)`);
  process.exit(0);
}
fs.mkdirSync(path.dirname(GOLDEN), { recursive: true });
fs.mkdirSync(OUT_DIR, { recursive: true });
for (const [name, bytes] of Object.entries(meshes)) {
  fs.writeFileSync(path.join(OUT_DIR, `${name}.stl`), bytes);
}
fs.writeFileSync(GOLDEN, text, 'utf8');
console.log(`wrote ${path.relative(ROOT, GOLDEN)} (${Object.keys(golden).length} cases)`);
