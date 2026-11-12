/**
 * Generate rust/tests/parity/export/{case}/out.step from web/engine/step.js.
 *
 * Reads cases.json, loads each case's bodies (same visibility / body_ids rules as
 * 1.x Document.export), runs stepBytes with date '2026-01-01T00:00:00' and title
 * equal to the 1.x base name, and writes out.step. On GeometryError, records
 * step_error in golden.json and removes out.step.
 *
 * Usage:
 *   node dev/parity/js/step_golden.mjs
 *   node dev/parity/js/step_golden.mjs --check
 */
import fs from 'fs';
import path from 'path';
import { fileURLToPath, pathToFileURL } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '..', '..', '..');
const OUT = path.join(ROOT, 'rust', 'tests', 'parity', 'export');
const CASES = path.join(OUT, 'cases.json');
const MODELS = path.join(ROOT, 'rust', 'tests', 'models');
const DATE = '2026-01-01T00:00:00';

function dumps(obj) {
  return JSON.stringify(obj, null, 1) + '\n';
}

function normNewlines(s) {
  return s.replace(/\r\n/g, '\n').replace(/\r/g, '\n');
}

function findModel(file) {
  for (const dir of [MODELS, path.join(MODELS, 'export'), path.join(MODELS, 'readers')]) {
    const p = path.join(dir, file);
    if (fs.existsSync(p) && fs.statSync(p).isFile()) return p;
  }
  throw new Error(`model not found: ${file}`);
}

/** Binary STL → { positions: Float32Array (flat), triangles: Uint32Array }. */
function readBinaryStl(buf) {
  if (buf.length < 84) throw new Error('STL too short');
  const nTri = buf.readUInt32LE(80);
  const positions = [];
  const triangles = [];
  const keyOf = (x, y, z) => `${x},${y},${z}`;
  const index = new Map();
  let o = 84;
  for (let t = 0; t < nTri; t++) {
    o += 12; // skip normal
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
    o += 2; // attribute
  }
  return {
    v: new Float32Array(positions),
    f: new Uint32Array(triangles),
  };
}

function exportBaseName(bodies, caseDef) {
  if (bodies.length === 1) return bodies[0].name;
  if (caseDef.path) {
    let base = path.basename(caseDef.path, path.extname(caseDef.path));
    if (base.toLowerCase().endsWith('.stlx')) base = base.slice(0, -5);
    return base;
  }
  if (caseDef.title != null && caseDef.title !== '') {
    let base = String(caseDef.title);
    if (base.toLowerCase().endsWith('.stlx')) base = base.slice(0, -5);
    return base;
  }
  return 'model';
}

/** Bodies to export: body_ids by name, else not-hidden, else all (1.x Document.export). */
function selectBodies(models, caseDef) {
  const all = models.map(m => ({ ...m }));
  if (caseDef.body_ids != null) {
    const want = new Set(caseDef.body_ids);
    return all.filter(m => want.has(m.name));
  }
  const hidden = new Set(caseDef.hidden || []);
  const visible = all.filter(m => !hidden.has(m.name));
  return visible.length ? visible : all;
}

async function loadStep() {
  const mod = await import(pathToFileURL(path.join(ROOT, 'web', 'engine', 'step.js')).href);
  return mod;
}

function loadCaseModels(caseDef) {
  const out = [];
  for (const m of caseDef.models || []) {
    const buf = fs.readFileSync(findModel(m.file));
    const mesh = readBinaryStl(buf);
    out.push({ name: m.name, v: mesh.v, f: mesh.f });
  }
  return out;
}

function runCase(stepMod, name, caseDef) {
  const models = loadCaseModels(caseDef);
  const bodies = selectBodies(models, caseDef);
  const title = exportBaseName(bodies, caseDef);
  if (!bodies.length) {
    return { step_error: null, bytes: null, title };
  }
  try {
    const bytes = stepMod.stepBytes(bodies, { title, date: DATE });
    return { step_error: null, bytes: Buffer.from(bytes), title };
  } catch (e) {
    const msg = e && e.message ? e.message : String(e);
    return { step_error: msg, bytes: null, title };
  }
}

function updateGolden(caseDir, step_error) {
  // Patch only step_error so Python's 30.0 floats are not rewritten as 30.
  const gp = path.join(caseDir, 'golden.json');
  let text = fs.existsSync(gp) ? fs.readFileSync(gp, 'utf8') : '{\n}\n';
  const value = JSON.stringify(step_error);
  if (/"step_error"\s*:/.test(text)) {
    text = text.replace(/"step_error"\s*:\s*(null|"((?:\\.|[^"\\])*)")/, `"step_error": ${value}`);
  } else {
    text = text.replace(/\n\}\s*$/, `,\n "step_error": ${value}\n}\n`);
    if (!/"step_error"/.test(text)) {
      text = text.replace(/\}\s*$/, `\n "step_error": ${value}\n}\n`);
    }
  }
  if (!text.endsWith('\n')) text += '\n';
  fs.writeFileSync(gp, text, 'utf8');
}

const check = process.argv.includes('--check');
if (!fs.existsSync(CASES)) {
  console.error(`step_golden: missing ${path.relative(ROOT, CASES)}`);
  process.exit(1);
}

const cases = JSON.parse(fs.readFileSync(CASES, 'utf8'));
const stepMod = await loadStep();
const mismatches = [];

for (const name of Object.keys(cases)) {
  const caseDef = cases[name];
  const caseDir = path.join(OUT, name);
  fs.mkdirSync(caseDir, { recursive: true });
  const result = runCase(stepMod, name, caseDef);
  const stepPath = path.join(caseDir, 'out.step');

  if (check) {
    const gp = path.join(caseDir, 'golden.json');
    if (!fs.existsSync(gp)) {
      mismatches.push(`${name}/golden.json (missing)`);
      continue;
    }
    const g = JSON.parse(fs.readFileSync(gp, 'utf8'));
    const wantErr = g.step_error ?? null;
    const gotErr = result.step_error;
    if (wantErr !== gotErr) {
      mismatches.push(`${name}/step_error`);
    }
    if (result.bytes) {
      if (!fs.existsSync(stepPath)) {
        mismatches.push(`${name}/out.step (missing)`);
      } else {
        const existing = fs.readFileSync(stepPath);
        if (normNewlines(existing.toString('utf8')) !== normNewlines(result.bytes.toString('utf8'))) {
          mismatches.push(`${name}/out.step`);
        }
      }
    } else if (fs.existsSync(stepPath)) {
      mismatches.push(`${name}/out.step (unexpected)`);
    }
  } else {
    if (result.bytes) {
      fs.writeFileSync(stepPath, result.bytes);
    } else if (fs.existsSync(stepPath)) {
      fs.unlinkSync(stepPath);
    }
    updateGolden(caseDir, result.step_error);
    console.log(`wrote ${path.relative(ROOT, caseDir)}/ (step ${result.bytes ? 'ok' : 'error'})`);
  }
}

if (check) {
  if (mismatches.length) {
    for (const m of mismatches) console.error(`step_golden: MISMATCH ${m}`);
    process.exit(1);
  }
  console.log(`step_golden: ok (${Object.keys(cases).length} cases)`);
  process.exit(0);
}
console.log(`step_golden: wrote ${Object.keys(cases).length} cases`);
