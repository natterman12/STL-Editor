/**
 * Reference values for 2.0's gizmo rotation maths, from 1.x's own web/js/gizmo.js (quatOf,
 * rotOf) and three r170 (Quaternion.setFromRotationMatrix), imported unmodified in Node.
 *
 * The values live in a table in rust/crates/stl-app/tests/gizmo_sculpt.rs, between the
 * `// BEGIN gizmo_quat.mjs` and `// END gizmo_quat.mjs` lines.
 *
 * Usage:
 *   node dev/parity/js/gizmo_quat.mjs           print the table
 *   node dev/parity/js/gizmo_quat.mjs --write   write it into the test file
 *   node dev/parity/js/gizmo_quat.mjs --check   fail when the test file's table differs
 */
import fs from 'fs';
import path from 'path';
import * as mod from 'node:module';
import { fileURLToPath, pathToFileURL } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '..', '..', '..');
const VENDOR = pathToFileURL(path.join(ROOT, 'web', 'vendor') + path.sep).href;
const TEST = path.join(ROOT, 'rust', 'crates', 'stl-app', 'tests', 'gizmo_sculpt.rs');

// gizmo.js imports 'three' and 'three/addons/...' through the page's import map: map them
// to web/vendor the same way.
const mapThree = spec => spec === 'three' ? VENDOR + 'three.module.min.js'
  : spec.startsWith('three/addons/') ? VENDOR + spec.slice('three/addons/'.length) : null;
if (mod.registerHooks) {
  mod.registerHooks({ resolve(spec, ctx, next) { const url = mapThree(spec); return url ? { url, shortCircuit: true } : next(spec, ctx); } });
} else {
  const hook = `const VENDOR = ${JSON.stringify(VENDOR)};
export async function resolve(spec, ctx, next) {
  if (spec === 'three') return { url: VENDOR + 'three.module.min.js', shortCircuit: true };
  if (spec.startsWith('three/addons/')) return { url: VENDOR + spec.slice('three/addons/'.length), shortCircuit: true };
  return next(spec, ctx);
}`;
  mod.register('data:text/javascript,' + encodeURIComponent(hook));
}

const THREE = await import(VENDOR + 'three.module.min.js');
const { quatOf, rotOf } = await import(pathToFileURL(path.join(ROOT, 'web', 'js', 'gizmo.js')).href);

// rotations in degrees: plain, past 180, the gimbal edge (Y = ±90) and just off it, ties at
// the third decimal, and strings / blanks as the tool's fields can hold them
const ROTS = [
  [0, 0, 0], [30, 0, 0], [0, 45, 0], [0, 0, 60], [10, 20, 30], [-35, 75, 120],
  [90, 0, 0], [0, 90, 0], [0, -90, 0], [45, 90, 30], [12, -90, 77], [0, 89.9995, 0], [5, 89.999, 5],
  [0, 90.0004, 0], [200, 0, 0], [0, 0, 270], [-190, 10, 359], [181, 181, 181], [720, -45, 15],
  [0.0005, 0.0015, -0.0025], [123.4565, -0.0005, 45.0005], [1e-10, -1e-10, 0], [5, 10, 15],
  ['15', '', '-30'], [null, 5, 'x'],
];
const fmt = x => (Number.isNaN(x) ? 'f64::NAN' : Object.is(x, -0) ? '-0.0' : Number.isInteger(x) ? x.toFixed(1) : String(x));
const lines = [];
for (const r of ROTS) {
  const q = quatOf(r);
  const back = rotOf(q);
  const neg = rotOf(new THREE.Quaternion(-q.x, -q.y, -q.z, -q.w));
  // a matrix from the quaternion, back to a quaternion (three's setFromRotationMatrix)
  const m = new THREE.Matrix4().makeRotationFromQuaternion(q);
  const q2 = new THREE.Quaternion().setFromRotationMatrix(m);
  // the fields as JS `Number(x)` reads them (quatOf's `+x`)
  const rot = r.map(x => fmt(Number(x)));
  lines.push(`    ([${rot.join(', ')}], [${[q.x, q.y, q.z, q.w].map(fmt).join(', ')}], [${back.map(fmt).join(', ')}], [${neg.map(fmt).join(', ')}], [${[q2.x, q2.y, q2.z, q2.w].map(fmt).join(', ')}]),`);
}
const table = lines.join('\n') + '\n';

const BEGIN = '// BEGIN gizmo_quat.mjs\n', END = '    // END gizmo_quat.mjs';
const arg = process.argv[2];
if (!arg) { process.stdout.write(table); process.exit(0); }
const src = fs.readFileSync(TEST, 'utf8').replace(/\r\n/g, '\n');
const a = src.indexOf(BEGIN), b = src.indexOf(END);
if (a < 0 || b < 0) { console.error('gizmo_quat.mjs: markers not found in', TEST); process.exit(1); }
const have = src.slice(a + BEGIN.length, b);
if (arg === '--check') {
  if (have !== table) { console.error('gizmo_quat.mjs: the table in gizmo_sculpt.rs is out of date (run with --write)'); process.exit(1); }
  console.log(`gizmo_quat.mjs: ${ROTS.length} cases match`);
} else if (arg === '--write') {
  fs.writeFileSync(TEST, src.slice(0, a + BEGIN.length) + table + src.slice(b));
  console.log(`gizmo_quat.mjs: wrote ${ROTS.length} cases`);
}
