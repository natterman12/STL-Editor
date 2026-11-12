/**
 * Time web/engine/step.js stepBytes on dev/bench/test_mesh.stl (median of 3).
 * Writes the STEP bytes to a temp file each run (same work as Document::export).
 *
 * Prints one JSON object on stdout: {"export_step": <seconds>}.
 *
 * Usage:
 *   node dev/parity/js/bench_step.mjs
 *   node --max-old-space-size=8192 dev/parity/js/bench_step.mjs
 */
import fs from 'fs';
import os from 'os';
import path from 'path';
import { fileURLToPath, pathToFileURL } from 'url';
import { performance } from 'perf_hooks';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '..', '..', '..');
const MESH = path.join(ROOT, 'dev', 'bench', 'test_mesh.stl');
const RUNS = 3;
const DATE = '2026-01-01T00:00:00';

/** Binary STL → { v: Float32Array, f: Uint32Array } (same weld as step_golden.mjs). */
function readBinaryStl(buf) {
  if (buf.length < 84) throw new Error('STL too short');
  const nTri = buf.readUInt32LE(80);
  const positions = [];
  const triangles = [];
  const keyOf = (x, y, z) => `${x},${y},${z}`;
  const index = new Map();
  let o = 84;
  for (let t = 0; t < nTri; t++) {
    o += 12;
    const tri = [];
    for (let k = 0; k < 3; k++) {
      const x = buf.readFloatLE(o); o += 4;
      const y = buf.readFloatLE(o); o += 4;
      const z = buf.readFloatLE(o); o += 4;
      const key = keyOf(x, y, z);
      let i = index.get(key);
      if (i === undefined) {
        i = positions.length / 3;
        index.set(key, i);
        positions.push(x, y, z);
      }
      tri.push(i);
    }
    triangles.push(tri[0], tri[1], tri[2]);
    o += 2;
  }
  return {
    v: new Float32Array(positions),
    f: new Uint32Array(triangles),
  };
}

function median(values) {
  const v = [...values].sort((a, b) => a - b);
  return v[Math.floor(v.length / 2)];
}

const stepMod = await import(pathToFileURL(path.join(ROOT, 'web', 'engine', 'step.js')).href);
const buf = fs.readFileSync(MESH);
const mesh = readBinaryStl(buf);
const body = { name: 'test_mesh', v: mesh.v, f: mesh.f };
const tmp = path.join(os.tmpdir(), `stl-editor-bench-step-${process.pid}.step`);

const times = [];
for (let i = 0; i < RUNS; i++) {
  const t0 = performance.now();
  const bytes = stepMod.stepBytes([body], { title: 'test_mesh', date: DATE });
  fs.writeFileSync(tmp, Buffer.from(bytes));
  times.push((performance.now() - t0) / 1000);
}
try { fs.unlinkSync(tmp); } catch (_) { /* ignore */ }

console.log(JSON.stringify({ export_step: median(times) }));
