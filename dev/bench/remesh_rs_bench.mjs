// Remesh benchmark: the JavaScript remesh (web/engine/meshedit.js) against its Rust port
// (rust/remesh, built with `wasm-pack build --target web` in that folder).
// Run: node dev/bench/remesh_rs_bench.mjs [edge] [runs]
//
// Both get the same mesh, already refined to the edge length by the Manifold kernel, so the
// numbers compare remesh with remesh. The JS remesh() is given a stand-in solid whose
// refineToLength() hands back that refined mesh. Its time therefore still includes reading
// the mesh out of the kernel and building a Manifold from the result; the Rust side is timed
// both alone (arrays in, arrays out) and with those same two steps added.
import fs from 'node:fs';
import path from 'node:path';
import { performance } from 'node:perf_hooks';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(here, '..', '..');
const web = path.join(root, 'web');
const load = rel => import(pathToFileURL(path.join(web, rel)).href);
const EDGE = Number(process.argv[2]) || 1.0, RUNS = Number(process.argv[3]) || 3, ITER = 5;

const W = await load('engine/wasm.js');
await W.init();
const { manifoldFromArrays } = await load('engine/geom.js');
const { remesh: jsRemesh } = await load('engine/meshedit.js');

const pkg = path.join(root, 'rust', 'remesh', 'pkg');
const wasmPath = path.join(pkg, 'remesh_rs_bg.wasm');
if (!fs.existsSync(wasmPath)) {
  console.error('rust/remesh/pkg is missing. Build it first: cd rust/remesh; wasm-pack build --target web');
  process.exit(2);
}
const rs = await import(pathToFileURL(path.join(pkg, 'remesh_rs.js')).href);
await rs.default({ module_or_path: fs.readFileSync(wasmPath) });

/** binary STL -> welded { v: Float32Array, f: Uint32Array } (vertices with the same bits are one vertex) */
function parseStl(buf) {
  const dv = new DataView(buf.buffer, buf.byteOffset, buf.byteLength), nt = dv.getUint32(80, true);
  if (buf.byteLength < 84 + nt * 50) throw new Error('not a binary STL');
  const bits = new Uint32Array(3);
  let size = 1; while (size < nt * 3) size *= 2;   // open-addressing table, load under 0.5
  const table = new Int32Array(size).fill(-1), keys = new Uint32Array(nt * 9);   // room for no sharing at all
  const f = new Uint32Array(nt * 3);
  let nv = 0;
  for (let t = 0; t < nt; t++) for (let e = 0; e < 3; e++) {
    const off = 84 + t * 50 + 12 + e * 12;
    for (let k = 0; k < 3; k++) bits[k] = dv.getUint32(off + k * 4, true);
    let h = (Math.imul(bits[0], 0x9E3779B1) ^ Math.imul(bits[1], 0x85EBCA6B) ^ Math.imul(bits[2], 0xC2B2AE35)) >>> 0;
    h = (h ^ (h >>> 15)) & (size - 1);
    for (;;) {
      const i = table[h];
      if (i < 0) { table[h] = nv; keys.set(bits, nv * 3); f[t * 3 + e] = nv++; break; }
      if (keys[i * 3] === bits[0] && keys[i * 3 + 1] === bits[1] && keys[i * 3 + 2] === bits[2]) { f[t * 3 + e] = i; break; }
      h = (h + 1) & (size - 1);
    }
  }
  return { v: new Float32Array(keys.buffer, 0, nv * 3).slice(), f };
}

/** the same as meshArrays() in meshedit.js, but keeping the kernel's Float32 positions */
function arraysOf(solid) {
  const m = solid.getMesh(), np = m.numProp, nv = m.vertProperties.length / np;
  const v = new Float32Array(nv * 3);
  for (let i = 0; i < nv; i++) { v[i * 3] = m.vertProperties[i * np]; v[i * 3 + 1] = m.vertProperties[i * np + 1]; v[i * 3 + 2] = m.vertProperties[i * np + 2]; }
  return { v, f: Uint32Array.from(m.triVerts) };
}

function stats(v, f) {
  const nv = v.length / 3, count = new Map();
  for (let t = 0; t < f.length; t += 3) for (let e = 0; e < 3; e++) {
    const a = f[t + e], b = f[t + (e + 1) % 3], k = a < b ? a * nv + b : b * nv + a;
    count.set(k, (count.get(k) || 0) + 1);
  }
  let min = Infinity, max = 0, sum = 0, watertight = f.length > 0;
  for (const [k, n] of count) {
    if (n !== 2) watertight = false;
    const a = Math.floor(k / nv) * 3, b = (k % nv) * 3, l = Math.hypot(v[a] - v[b], v[a + 1] - v[b + 1], v[a + 2] - v[b + 2]);
    min = Math.min(min, l); max = Math.max(max, l); sum += l;
  }
  const r = x => Math.round(x * 1e4) / 1e4;
  return { triangles: f.length / 3, vertices: nv, edgeMin: r(min), edgeMean: r(sum / count.size), edgeMax: r(max), watertight };
}
const median = a => { const s = [...a].sort((x, y) => x - y), n = s.length; return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2; };
const ms = x => Math.round(x * 10) / 10;

// ---- the input: test mesh, refined to the edge length by the kernel
const stl = parseStl(fs.readFileSync(path.join(here, 'test_mesh.stl')));
const solid = manifoldFromArrays(stl.v, stl.f);
if (!solid) throw new Error('test_mesh.stl is not a closed solid');
const fine = solid.refineToLength(EDGE);
const input = arraysOf(fine);
const standIn = { surfaceArea: () => fine.surfaceArea(), refineToLength: () => fine };
console.error(`input: ${stl.f.length / 3} triangles in the STL, ${input.f.length / 3} after refineToLength(${EDGE})`);

// ---- (a) JavaScript
const jsTimes = [];
let jsOut = null, jsFellBack = false;
for (let i = 0; i < RUNS; i++) {
  const t0 = performance.now();
  const out = jsRemesh(standIn, EDGE, ITER);
  jsTimes.push(performance.now() - t0);
  if (out === fine) jsFellBack = true;   // remesh() returns the refined mesh when its result is not a solid
  jsOut = arraysOf(out);
  console.error(`  js run ${i + 1}: ${ms(jsTimes[i])} ms`);
}

// ---- (b) Rust (wasm)
const rsTimes = [], rsFullTimes = [];
let rsOut = null, rsManifoldTris = 0;
for (let i = 0; i < RUNS; i++) {
  const t0 = performance.now();
  const a = arraysOf(fine);                       // what meshArrays() does inside the JS remesh
  const t1 = performance.now();
  const res = rs.remesh(a.v, a.f, EDGE, ITER);
  const v = res.positions, f = res.faces;
  res.free();
  const t2 = performance.now();
  const man = manifoldFromArrays(v, f);           // what the JS remesh does with its result
  const t3 = performance.now();
  rsTimes.push(t2 - t1); rsFullTimes.push(t3 - t0);
  rsOut = { v, f }; rsManifoldTris = man ? man.numTri() : 0;
  console.error(`  rust run ${i + 1}: ${ms(t2 - t1)} ms alone, ${ms(t3 - t0)} ms with the kernel steps`);
}

const js = stats(jsOut.v, jsOut.f), rust = stats(rsOut.v, rsOut.f);
const diffPct = Math.abs(rust.triangles - js.triangles) / js.triangles * 100;
const result = {
  mesh: 'dev/bench/test_mesh.stl', edge: EDGE, iterations: ITER, runs: RUNS,
  inputTriangles: stl.f.length / 3, refinedTriangles: input.f.length / 3,
  js: { medianMs: ms(median(jsTimes)), timesMs: jsTimes.map(ms), ...js, fellBackToRefined: jsFellBack },
  rust: {
    medianMs: ms(median(rsTimes)), timesMs: rsTimes.map(ms),
    medianWithKernelStepsMs: ms(median(rsFullTimes)), timesWithKernelStepsMs: rsFullTimes.map(ms),
    ...rust, manifoldAccepts: rsManifoldTris > 0, manifoldTriangles: rsManifoldTris,
  },
  wasmBytes: fs.statSync(wasmPath).size,
  speedup: Math.round(median(jsTimes) / median(rsTimes) * 100) / 100,
  speedupWithKernelSteps: Math.round(median(jsTimes) / median(rsFullTimes) * 100) / 100,
  triangleDiffPercent: Math.round(diffPct * 100) / 100,
  gate: { trianglesWithin5Percent: diffPct <= 5, rustWatertight: rust.watertight },
};
result.gate.pass = result.gate.trianglesWithin5Percent && result.gate.rustWatertight;
console.log(JSON.stringify(result, null, 2));

const row = (...c) => console.log(c.map((x, i) => String(x).padEnd(i ? 14 : 26)).join(''));
console.log('');
row('', 'JS', 'Rust (wasm)');
row('median time (ms)', result.js.medianMs, result.rust.medianMs);
row('  with kernel steps (ms)', result.js.medianMs, result.rust.medianWithKernelStepsMs);
row('triangles', js.triangles, rust.triangles);
row('edge min (mm)', js.edgeMin, rust.edgeMin);
row('edge mean (mm)', js.edgeMean, rust.edgeMean);
row('edge max (mm)', js.edgeMax, rust.edgeMax);
row('watertight', js.watertight ? 'yes' : 'no', rust.watertight ? 'yes' : 'no');
console.log(`wasm size: ${result.wasmBytes} bytes; speed-up ${result.speedup}x (${result.speedupWithKernelSteps}x with the kernel steps); triangle counts differ by ${result.triangleDiffPercent}%`);
console.log(`quality gate (counts within 5%, Rust watertight): ${result.gate.pass ? 'PASS' : 'FAIL'}`);
process.exit(result.gate.pass ? 0 : 1);
