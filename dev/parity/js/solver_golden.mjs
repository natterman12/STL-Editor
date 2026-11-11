/**
 * Generate rust/tests/parity/solver_golden.json from 1.x constraints.js.
 *
 * Reads every *.json scene under rust/tests/parity/solver/, runs solve /
 * analyze / unsatisfied / inferDimension, rounds coordinates to 1e-9.
 *
 * Existing record keys (always): solved, analyze, unsatisfied, infer, ok, error;
 * plus redundant when the scene lists redundant candidates.
 *
 * Optional scene fields → extra golden keys (omit when the scene does not list them):
 *   make: [{tool, picks}] → make: [constraint | {result: message|null}]
 *   measure: true → measure: [{id?, type, value, ref?}, ...] for each dim constraint
 *   glyphSpots: true → glyphSpots: [{id?, type, spots: [[u,v], ...]}, ...]
 *   lineMeet: [{a, b}] (entity ids) → lineMeet: [{a, b, at: [x,y]|null}, ...]
 *   ref: true → ref: [constraint, ...] scene constraints with truthy ref
 *
 * Usage:
 *   node dev/parity/js/solver_golden.mjs
 *   node dev/parity/js/solver_golden.mjs --check
 */
import fs from 'fs';
import path from 'path';
import { fileURLToPath, pathToFileURL } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '..', '..', '..');
const CASES = path.join(ROOT, 'rust', 'tests', 'parity', 'solver');
const GOLDEN = path.join(ROOT, 'rust', 'tests', 'parity', 'solver_golden.json');

const constraintsUrl = pathToFileURL(path.join(ROOT, 'web', 'js', 'constraints.js')).href;
const {
  solve, analyze, unsatisfied, inferDimension, isRedundant,
  makeConstraint, measure, glyphSpots, lineMeet, CTYPES,
} = await import(constraintsUrl);

function round(v, nd = 9) {
  if (typeof v === 'number') {
    if (!Number.isFinite(v)) return v;
    const p = 10 ** nd;
    return Math.round(v * p) / p;
  }
  if (Array.isArray(v)) return v.map(x => round(x, nd));
  if (v && typeof v === 'object') {
    const out = {};
    for (const k of Object.keys(v).sort()) out[k] = round(v[k], nd);
    return out;
  }
  return v;
}

function dumps(obj) {
  return JSON.stringify(obj, null, 1) + '\n';
}

function normNewlines(s) {
  return s.replace(/\r\n/g, '\n').replace(/\r/g, '\n');
}

function wrapOutcome(r) {
  return round(typeof r === 'string' || r == null ? { result: r } : r);
}

function runScene(scene) {
  const entities = structuredClone(scene.entities || []);
  const constraints = structuredClone(scene.constraints || []);
  const locks = structuredClone(scene.locks || []);
  const fixed = structuredClone(scene.fixed || []);
  const fixedIds = new Set((scene.fixed_ids || []).map(String));

  const solved = solve(entities, constraints, locks, fixed);
  const solvedEnts = solved.entities || entities;
  const solvedMap = {};
  for (const e of solvedEnts) {
    if (e && e.id != null) solvedMap[e.id] = round(e);
  }

  const an = analyze(solvedEnts, constraints, fixed);
  const analyzeOut = {
    dof: an.dof,
    rank: an.rank,
    under: [...(an.under || [])].map(String).sort(),
  };

  const uns = unsatisfied(solvedEnts, constraints, fixed);
  const unsatisfiedOut = round(uns.map(c => {
    const o = { ...c };
    return o;
  }));

  const inferOut = [];
  for (const inf of scene.infer || []) {
    const picks = structuredClone(inf.picks || []);
    const cursor = inf.cursor ?? null;
    const fx = inf.fixed ? new Set(inf.fixed.map(String)) : fixedIds;
    const r = inferDimension(picks, solvedEnts, cursor, fx);
    inferOut.push(wrapOutcome(r));
  }

  const redundantOut = [];
  for (const c of scene.redundant || []) {
    redundantOut.push({
      constraint: round(c),
      redundant: !!isRedundant(solvedEnts, constraints, c, fixed),
    });
  }

  const byId = Object.fromEntries(solvedEnts.map(e => [e.id, e]));

  const rec = {
    solved: solvedMap,
    analyze: analyzeOut,
    unsatisfied: unsatisfiedOut,
    infer: inferOut,
    ok: !!solved.ok,
    error: round(solved.error ?? 0),
  };
  if (redundantOut.length) rec.redundant = redundantOut;

  if (Array.isArray(scene.make)) {
    rec.make = scene.make.map(m => {
      const r = makeConstraint(m.tool, structuredClone(m.picks || []), solvedEnts);
      return wrapOutcome(r);
    });
  }

  if (scene.measure) {
    rec.measure = constraints
      .filter(c => CTYPES[c.type]?.dim)
      .map(c => {
        const o = { type: c.type, value: measure(c, solvedEnts) };
        if (c.id != null) o.id = c.id;
        if (c.ref) o.ref = true;
        return round(o);
      });
  }

  if (scene.glyphSpots) {
    rec.glyphSpots = constraints.map(c => {
      const o = { type: c.type, spots: glyphSpots(c, solvedEnts) };
      if (c.id != null) o.id = c.id;
      if (c.ref) o.ref = true;
      return round(o);
    });
  }

  if (Array.isArray(scene.lineMeet)) {
    rec.lineMeet = scene.lineMeet.map(pair => {
      const A = byId[pair.a], B = byId[pair.b];
      const at = (A && B) ? lineMeet(A, B) : null;
      return round({ a: pair.a, b: pair.b, at });
    });
  }

  if (scene.ref) {
    rec.ref = round(constraints.filter(c => c.ref));
  }

  return rec;
}

function build() {
  const records = {};
  const files = fs.readdirSync(CASES).filter(f => f.endsWith('.json')).sort();
  for (const f of files) {
    const scene = JSON.parse(fs.readFileSync(path.join(CASES, f), 'utf8'));
    const name = scene.name || path.basename(f, '.json');
    records[name] = runScene(scene);
  }
  const ordered = {};
  for (const k of Object.keys(records).sort()) ordered[k] = records[k];
  return ordered;
}

const check = process.argv.includes('--check');
if (!fs.existsSync(CASES)) {
  console.error(`solver_golden: missing ${path.relative(ROOT, CASES)}`);
  process.exit(1);
}
const golden = build();
const text = dumps(golden);
if (check) {
  const existing = fs.existsSync(GOLDEN) ? fs.readFileSync(GOLDEN, 'utf8') : '';
  if (normNewlines(existing) !== normNewlines(text)) {
    console.error(`solver_golden: MISMATCH ${path.relative(ROOT, GOLDEN)}`);
    process.exit(1);
  }
  console.log(`solver_golden: ok (${Object.keys(golden).length} scenes)`);
  process.exit(0);
}
fs.mkdirSync(path.dirname(GOLDEN), { recursive: true });
fs.writeFileSync(GOLDEN, text, 'utf8');
console.log(`wrote ${path.relative(ROOT, GOLDEN)} (${Object.keys(golden).length} scenes)`);
