/**
 * Golden bare-spot centroids for Support Fins coverage (1.x `sfcoverage.js`).
 *
 *   node dev/parity/js/sfcoverage_golden.mjs
 *   node dev/parity/js/sfcoverage_golden.mjs --check
 */
import fs from 'fs';
import path from 'path';
import { fileURLToPath, pathToFileURL } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '..', '..', '..');
const WEB = path.join(ROOT, 'web');
const FIX = path.join(ROOT, 'tests', 'ui', 'fixtures');
const OUT_DIR = path.join(ROOT, 'rust', 'tests', 'parity', 'sfcoverage');

const STL_CASES = [
  { stl: 'mushroom.stl', out: 'mushroom.json' },
  // Bracket sits with no downward face. Turn it about Y so the flange is an overhang.
  { stl: 'bracket.stl', out: 'bracket.json', orient: 'ry90' },
  // Plate is a box on the bed. Lift the +Y edge (z += 0.5*y) so the underside is bare.
  { stl: 'plate.stl', out: 'plate.json', orient: 'lift_y' },
  // A fin under the mushroom cap, on the real mesh (not the synthetic shelf).
  { stl: 'mushroom.stl', out: 'mushroom_fin.json', finBox: [[55, 38, 0], [70, 42, 19.5]] },
];

const load = rel => import(pathToFileURL(path.join(WEB, rel)).href);

function readBinaryStl(file) {
  const buf = fs.readFileSync(file);
  const n = buf.readUInt32LE(80);
  const pos = [];
  const idx = [];
  let off = 84;
  for (let f = 0; f < n; f++) {
    const i0 = pos.length / 3;
    for (let v = 0; v < 3; v++) {
      pos.push(buf.readFloatLE(off + 12 + v * 12), buf.readFloatLE(off + 16 + v * 12), buf.readFloatLE(off + 20 + v * 12));
    }
    idx.push(i0, i0 + 1, i0 + 2);
    off += 50;
  }
  return { pos: new Float32Array(pos), idx: new Uint32Array(idx) };
}

function shelfMesh() {
  const pos = new Float32Array([0, 0, 10, 30, 0, 10, 30, 20, 10, 0, 20, 10, 0, 0, 0, 0, 20, 0]);
  const idx = new Uint32Array([0, 2, 1, 0, 3, 2, 0, 4, 5, 0, 5, 3]);
  return { pos, idx, over: [0, 1] };
}

function finSoup(lo, hi) {
  const P = [];
  for (const x of [lo[0], hi[0]]) for (const y of [lo[1], hi[1]]) for (const z of [lo[2], hi[2]]) P.push([x, y, z]);
  const out = [];
  for (let i = 0; i + 2 < 8; i++) out.push(...P[i], ...P[i + 1], ...P[i + 2]);
  return new Float32Array(out);
}

/** 1.x `viewer.js` `overhangFaces`. */
function overhangFaces(pos, idx, thresholdDeg = 45) {
  const nf = idx.length / 3;
  const cut = -(Math.cos(thresholdDeg * Math.PI / 180) + 1e-4);
  let minZ = Infinity;
  for (let i = 2; i < pos.length; i += 3) if (pos[i] < minZ) minZ = pos[i];
  const over = new Uint8Array(nf);
  const area = new Float64Array(nf);
  for (let f = 0; f < nf; f++) {
    const a = idx[f * 3] * 3, b = idx[f * 3 + 1] * 3, c = idx[f * 3 + 2] * 3;
    const top = Math.max(pos[a + 2], pos[b + 2], pos[c + 2]) - minZ;
    if (top < 0.35) continue;
    const ux = pos[b] - pos[a], uy = pos[b + 1] - pos[a + 1], uz = pos[b + 2] - pos[a + 2];
    const vx = pos[c] - pos[a], vy = pos[c + 1] - pos[a + 1], vz = pos[c + 2] - pos[a + 2];
    const nx = uy * vz - uz * vy, ny = uz * vx - ux * vz, nz = ux * vy - uy * vx, len = Math.hypot(nx, ny, nz);
    if (!(len > 0)) continue;
    if (nz / len < cut) { over[f] = 1; area[f] = len / 2; }
  }
  const parent = new Int32Array(nf).map((_, i) => i);
  const find = x => { while (parent[x] !== x) x = parent[x] = parent[parent[x]]; return x; };
  const firstAt = new Int32Array(pos.length / 3).fill(-1);
  for (let f = 0; f < nf; f++) {
    if (!over[f]) continue;
    for (let j = 0; j < 3; j++) {
      const vi = idx[f * 3 + j], g = firstAt[vi];
      if (g < 0) firstAt[vi] = f; else { const ra = find(f), rb = find(g); if (ra !== rb) parent[ra] = rb; }
    }
  }
  const sum = new Map();
  for (let f = 0; f < nf; f++) if (over[f]) { const r = find(f); sum.set(r, (sum.get(r) || 0) + area[f]); }
  const out = [];
  for (let f = 0; f < nf; f++) if (over[f] && sum.get(find(f)) >= 12) out.push(f);
  return out;
}

function round4(x) {
  return Math.round(x * 1e4) / 1e4;
}

function bareAt(spots) {
  return spots.map(s => s.at.map(round4)).sort((a, b) => a[0] - b[0] || a[1] - b[1] || a[2] - b[2]);
}

/** Must match `orient_positions` in `supports_coverage.rs`. Writes f32, then seats on z = 0. */
function orientPositions(pos, orient) {
  if (!orient) return pos;
  const n = pos.length / 3;
  const pts = new Array(n);
  for (let i = 0; i < n; i++) {
    let x = pos[i * 3], y = pos[i * 3 + 1], z = pos[i * 3 + 2];
    if (orient === 'ry90') {
      const nx = z, ny = y, nz = -x;
      x = nx; y = ny; z = nz;
    } else if (orient === 'lift_y') {
      z = z + 0.5 * y;
    } else {
      throw new Error('unknown orient ' + orient);
    }
    pts[i] = [x, y, z];
  }
  let minZ = Infinity;
  for (const p of pts) if (p[2] < minZ) minZ = p[2];
  const out = new Float32Array(pos.length);
  for (let i = 0; i < n; i++) {
    out[i * 3] = pts[i][0];
    out[i * 3 + 1] = pts[i][1];
    out[i * 3 + 2] = pts[i][2] - minZ;
  }
  return out;
}

function pack(out, extra = {}) {
  return {
    bare_at: bareAt(out.spots),
    held: out.held,
    total: out.total,
    reach: 5,
    gap: 0.2,
    ...extra,
  };
}

async function build() {
  const { coverage } = await load('js/sfcoverage.js');
  const cases = {};
  for (const spec of STL_CASES) {
    const raw = readBinaryStl(path.join(FIX, spec.stl));
    const pos = orientPositions(raw.pos, spec.orient);
    const over = overhangFaces(pos, raw.idx, 45);
    const fins = spec.finBox ? [finSoup(spec.finBox[0], spec.finBox[1])] : [];
    const out = coverage({ pos, idx: raw.idx, over, fins, gap: 0.2, reach: 5 });
    const extra = { threshold_deg: 45, stl: spec.stl };
    if (spec.orient) extra.orient = spec.orient;
    if (spec.finBox) extra.fin_box = spec.finBox;
    cases[spec.out] = pack(out, extra);
  }
  const shelf = shelfMesh();
  cases['shelf_alone.json'] = pack(
    coverage({ ...shelf, fins: [], reach: 5 }),
    { synthetic: 'shelf' },
  );
  const fin = finSoup([19.6, 0, 0], [20.4, 20, 9.8]);
  cases['shelf_fin.json'] = pack(
    coverage({ ...shelf, fins: [fin], reach: 5 }),
    { synthetic: 'shelf', fins: true },
  );
  return cases;
}

function dumps(obj) {
  return JSON.stringify(obj, null, 2) + '\n';
}

async function main() {
  const check = process.argv.includes('--check');
  const built = await build();
  fs.mkdirSync(OUT_DIR, { recursive: true });
  for (const [name, data] of Object.entries(built)) {
    const outPath = path.join(OUT_DIR, name);
    const text = dumps(data);
    if (!data.bare_at.length || !(data.total > 0)) {
      console.error(`sfcoverage golden is empty: ${name}`);
      process.exit(1);
    }
    if (data.fin_box && !(data.held > 0)) {
      console.error(`sfcoverage fin golden holds nothing: ${name}`);
      process.exit(1);
    }
    if (check) {
      const onDisk = fs.existsSync(outPath) ? fs.readFileSync(outPath, 'utf8').replace(/\r\n/g, '\n') : '';
      if (onDisk !== text) {
        console.error(`sfcoverage golden mismatch: ${outPath}`);
        process.exit(1);
      }
    } else {
      fs.writeFileSync(outPath, text);
      console.log('wrote', outPath);
    }
  }
  if (check) console.log('sfcoverage golden OK');
}

main().catch(e => { console.error(e); process.exit(1); });
