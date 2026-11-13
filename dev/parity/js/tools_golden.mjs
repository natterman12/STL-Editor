/**
 * Tool registry golden files for STL Editor 2.0 (M4b Task 0 / Task 1).
 *
 * Imports `web/js/tools.js` in Node (no DOM) and writes one JSON file per tool under
 * `rust/tests/parity/tools/<tool>.json`. Task 1's Rust port replays every case through
 * `tools::defs` and compares JSON exactly.
 *
 * ---------------------------------------------------------------------------
 * Format (per tool file)
 * ---------------------------------------------------------------------------
 * Top-level object keys, in this order:
 *   tool, label, type, icon, flags, hint, extra, fields, ctxNote, defaults,
 *   toParams_defaults, shown_defaults, cases, onChange, roundTrips
 *
 *   tool / label / type / icon  — from TOOLS[key]
 *   flags — { placeOnFace, noLivePreview, gizmo, planeTool, sculpt, paint, wallTool }
 *   fields — metadata only (no show functions). Each field:
 *     { key, label, kind, unit?, multi?, anyFace?, placeholder?, help?,
 *       options?: [[id, label] | [id, label, group], ...], groups?: true, hasShow }
 *   defaults — `def.defaults(ctx)` form values (keeps `_…` UI keys)
 *   toParams_defaults — engine-facing params for those defaults (see toParams below)
 *   shown_defaults — field keys with show(v) true (or no show), in fields order
 *   cases — scripted edits from defaults:
 *     { name, values, toParams, shown }
 *     name patterns: `select:<key>=<id>`, `bool:<key>=true|false`
 *     One case per select option and per bool true/false (even if that equals defaults).
 *   onChange — trigger cases (hole presets, thread sizes, connectors, resize sizes, …):
 *     { name, trigger, values_before, patch, values_after, toParams, shown }
 *     `patch` is what `def.onChange(trigger, values_before)` returned (or null).
 *     `values_after` is values_before with Object.assign(patch) when patch is non-null.
 *   roundTrips — fromParams → toParams:
 *     { name, params, fromParams, toParams }
 *     Always includes `defaults` (toParams_defaults → fromParams → toParams), then one trip per
 *     option of every select field (the Support Fins tools: the first differing option only).
 *   ctxCases — (the 41 TOOLS only) defaults / toParams_defaults / shown_defaults under other project
 *     states: CTX_VARIANTS nosel, two, nonsolid, nobbox, empty (the Rust test rebuilds the same).
 *   extra — (the 41 TOOLS only) hand-picked drift traps, { name, op, input, result } with op
 *     toParams | fromParams | onChange; see extraCases().
 *
 * toParams cleanup (matches app.js `params()` after the optional def.toParams call):
 *   - call `def.toParams(clone(values))` when present, else clone(values)
 *   - delete keys that start with `_` and keys whose value is `undefined`
 *   Number string parsing from the form is NOT applied here (registry tests pass numbers).
 *
 * Key / array order (exact JSON compare — do not sort):
 *   - Object key order is insertion order from tools.js defaults / toParams / fromParams.
 *     Spreading defaults then overriding a key keeps that key's position.
 *   - `shown*` arrays follow `fields` declaration order.
 *   - `cases` / `onChange` / `roundTrips` arrays follow generation order below.
 *   - Select option order is tools.js `options` / HOLE_PRESETS / THREAD_SIZES / … order.
 *   - Indent: two spaces; file ends with a single trailing newline. CRLF is normalised on check.
 *
 * Fixture ctx (fixed; not read from disk — mirrors a small plate + sketch-on-body project):
 *   bodies body_a / body_b with bboxes, sketch sk1, features import+sketch+extrude(cut),
 *   selection [body_a], lastSketch sk1, sketchOnBody true.
 *
 * supportfins / supportwall:
 *   Built like app.js `supportTools` from `web/vendor/support-fins/engine.esm.js`
 *   OPTIONS_SCHEMA (loads in Node). Written as the same JSON shape. If the vendor
 *   engine cannot load, those two files are omitted and a parked note is printed.
 *
 * Usage:
 *   node dev/parity/js/tools_golden.mjs
 *   node dev/parity/js/tools_golden.mjs --check
 */
import fs from 'fs';
import path from 'path';
import { fileURLToPath, pathToFileURL } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '..', '..', '..');
const OUT_DIR = path.join(ROOT, 'rust', 'tests', 'parity', 'tools');
const toolsUrl = pathToFileURL(path.join(ROOT, 'web', 'js', 'tools.js')).href;

const {
  TOOLS,
  HOLE_PRESETS,
  THREAD_SIZES,
  RESIZE_SIZES,
  selBody,
} = await import(toolsUrl);

/** Fixed ctx — deterministic stand-in for a fixture project with a body selected. */
export const FIXTURE_CTX = {
  state: {
    bodies: [
      {
        id: 'body_a',
        name: 'Body A',
        hidden: false,
        solid: true,
        bbox: [
          [0, 0, 0],
          [40, 20, 10],
        ],
      },
      {
        id: 'body_b',
        name: 'Body B',
        hidden: false,
        solid: true,
        bbox: [
          [50, 0, 0],
          [70, 20, 10],
        ],
      },
    ],
    planes: [
      { id: 'XY', name: 'Top' },
      { id: 'XZ', name: 'Front' },
      { id: 'YZ', name: 'Right' },
    ],
    sketches: [{ id: 'sk1', name: 'Sketch 1' }],
    features: [
      { id: 'f_import', type: 'import', active: true, error: null, suppressed: false, params: {} },
      {
        id: 'f_sketch',
        type: 'sketch',
        active: true,
        error: null,
        suppressed: false,
        params: { plane: { body: 'body_a', origin: [0, 0, 10], normal: [0, 0, 1] } },
      },
      {
        id: 'f_extrude',
        type: 'extrude',
        active: true,
        error: null,
        suppressed: false,
        params: { sketch: 'sk1', distance: 5, operation: 'cut' },
      },
    ],
  },
  selection: ['body_a'],
  lastSketch: 'sk1',
  sketchOnBody: true,
};

function clone(v) {
  return structuredClone(v);
}

function dumps(obj) {
  return JSON.stringify(obj, null, 2) + '\n';
}

function normNewlines(s) {
  return s.replace(/\r\n/g, '\n').replace(/\r/g, '\n');
}

/** Engine-facing params: def.toParams then app.js params() cleanup of `_` / undefined. */
function toParamsOf(def, values) {
  const v = clone(values);
  const p = def.toParams ? def.toParams(v) : clone(v);
  for (const k of Object.keys(p)) {
    if (k.startsWith('_') || p[k] === undefined) delete p[k];
  }
  return p;
}

function fromParamsOf(def, params) {
  if (def.fromParams) return def.fromParams(clone(params));
  return clone(params);
}

function shownKeys(def, values) {
  const out = [];
  for (const f of def.fields || []) {
    if (!f.key || f.kind === 'hdr') continue;
    if (f.show && !f.show(values)) continue;
    out.push(f.key);
  }
  return out;
}

function fieldMeta(f) {
  const m = { key: f.key, label: f.label, kind: f.kind };
  if (f.unit != null && f.unit !== '') m.unit = f.unit;
  if (f.multi != null) m.multi = !!f.multi;
  if (f.anyFace) m.anyFace = true;
  if (f.placeholder != null) m.placeholder = f.placeholder;
  if (f.help != null) m.help = f.help;
  if (f.options) m.options = f.options.map((o) => (Array.isArray(o) ? [...o] : o));
  if (f.groups) m.groups = true;
  if (typeof f.show === 'function') m.hasShow = true;
  return m;
}

function flagsOf(def) {
  return {
    placeOnFace: !!def.placeOnFace,
    noLivePreview: !!def.noLivePreview,
    gizmo: def.gizmo || false,
    planeTool: !!def.planeTool,
    sculpt: !!def.sculpt,
    paint: !!def.paint,
    wallTool: !!def.wallTool,
  };
}

function optionIds(field) {
  if (!field.options) return [];
  return field.options.map((o) => (Array.isArray(o) ? o[0] : o));
}

function buildCases(def, defaults) {
  const cases = [];
  for (const f of def.fields || []) {
    if (f.kind === 'select' && f.options?.length) {
      for (const id of optionIds(f)) {
        const values = { ...clone(defaults), [f.key]: id };
        cases.push({
          name: `select:${f.key}=${id}`,
          values,
          toParams: toParamsOf(def, values),
          shown: shownKeys(def, values),
        });
      }
    } else if (f.kind === 'bool') {
      for (const bit of [true, false]) {
        const values = { ...clone(defaults), [f.key]: bit };
        cases.push({
          name: `bool:${f.key}=${bit}`,
          values,
          toParams: toParamsOf(def, values),
          shown: shownKeys(def, values),
        });
      }
    }
  }
  return cases;
}

/**
 * onChange cases for the triggers the plan names. Only records when onChange exists.
 * Hole: every preset id (+ one tol change on M3 insert). Thread: every size + both kinds.
 * Connectors: every connector option on tools that use connectorChange.
 * Resize: every RESIZE_SIZES entry with a diameter. Printers: no onChange in tools.js (select cases cover them).
 */
function buildOnChange(key, def, defaults) {
  if (!def.onChange) return [];
  const out = [];
  const push = (name, trigger, values_before) => {
    const before = clone(values_before);
    const patch = def.onChange(trigger, before);
    const values_after = clone(before);
    if (patch) Object.assign(values_after, patch);
    out.push({
      name,
      trigger,
      values_before: before,
      patch: patch == null ? null : patch,
      values_after,
      toParams: toParamsOf(def, values_after),
      shown: shownKeys(def, values_after),
    });
  };

  if (key === 'hole') {
    for (const pr of HOLE_PRESETS) {
      const values = { ...clone(defaults), preset: pr.id };
      push(`onchange:preset=${pr.id}`, 'preset', values);
    }
    push(
      'onchange:tol@ins_M3',
      'tol',
      { ...clone(defaults), preset: 'ins_M3', tol: 0.3 },
    );
  } else if (key === 'thread') {
    for (const t of THREAD_SIZES) {
      push(`onchange:size=${t[0]}`, 'size', { ...clone(defaults), size: t[0] });
    }
    for (const kind of ['internal', 'external']) {
      push(`onchange:kind=${kind}`, 'kind', { ...clone(defaults), kind });
    }
  } else if (key === 'resizehole') {
    for (const row of RESIZE_SIZES) {
      const id = row[0];
      push(`onchange:size=${id}`, 'size', { ...clone(defaults), size: id });
    }
  } else if (def.onChange) {
    // connectorChange and any other onChange: fire for each select option of fields the
    // handler is known to watch (connector), plus every select as a safety net when patch≠null.
    for (const f of def.fields || []) {
      if (f.kind !== 'select' || !f.options?.length) continue;
      if (f.key !== 'connector' && key !== 'split' && key !== 'bedsplit' && key !== 'plate') {
        // generic: only keep cases that actually return a patch
        for (const id of optionIds(f)) {
          const before = { ...clone(defaults), [f.key]: id };
          const patch = def.onChange(f.key, before);
          if (patch == null) continue;
          const values_after = clone(before);
          Object.assign(values_after, patch);
          out.push({
            name: `onchange:${f.key}=${id}`,
            trigger: f.key,
            values_before: before,
            patch,
            values_after,
            toParams: toParamsOf(def, values_after),
            shown: shownKeys(def, values_after),
          });
        }
        continue;
      }
      for (const id of optionIds(f)) {
        if (f.key === 'connector' || f.key === 'printer') {
          push(`onchange:${f.key}=${id}`, f.key, { ...clone(defaults), [f.key]: id });
        }
      }
    }
  }
  return out;
}

function buildRoundTrips(def, defaults, paramsDefaults, full) {
  const trips = [];
  const from = fromParamsOf(def, paramsDefaults);
  trips.push({
    name: 'defaults',
    params: paramsDefaults,
    fromParams: from,
    toParams: toParamsOf(def, from),
  });
  // One trip per select option (the first differing one only, for the Support Fins tools).
  for (const f of def.fields || []) {
    if (f.kind !== 'select' || !f.options?.length) continue;
    const ids = full ? optionIds(f) : [optionIds(f).find((x) => x !== defaults[f.key]) ?? optionIds(f)[0]];
    for (const id of ids) {
      const values = { ...clone(defaults), [f.key]: id };
      const params = toParamsOf(def, values);
      const fp = fromParamsOf(def, params);
      trips.push({
        name: `select:${f.key}=${id}`,
        params,
        fromParams: fp,
        toParams: toParamsOf(def, fp),
      });
    }
    if (!full) break;
  }
  return trips;
}

/** Other project states the defaults depend on: nothing selected, no bbox, empty project, ... */
export const CTX_VARIANTS = {
  nosel: () => ({ ...clone(FIXTURE_CTX), selection: [] }),
  two: () => ({ ...clone(FIXTURE_CTX), selection: ['body_a', 'body_b'] }),
  nonsolid: () => {
    const c = clone(FIXTURE_CTX);
    c.selection = [];
    c.state.bodies.push({ id: 'body_n', name: 'Body N', hidden: false, solid: false, bbox: [[0, 0, 0], [5, 5, 5]] });
    return c;
  },
  nobbox: () => {
    const c = clone(FIXTURE_CTX);
    c.selection = ['body_c'];
    c.state.bodies = [{ id: 'body_c', name: 'Body C', hidden: false, solid: true }];
    c.state.features = [];
    return c;
  },
  empty: () => ({
    state: { bodies: [], planes: [], sketches: [], features: [] },
    selection: [],
    lastSketch: undefined,
    sketchOnBody: false,
  }),
};

function buildCtxCases(def) {
  const out = {};
  for (const [name, make] of Object.entries(CTX_VARIANTS)) {
    const ctx = make();
    const defaults = def.defaults(ctx);
    out[name] = {
      defaults,
      toParams_defaults: toParamsOf(def, defaults),
      shown_defaults: shownKeys(def, defaults),
    };
  }
  return out;
}

const PRESET_IDS = HOLE_PRESETS.map((p) => p.id);

/**
 * Hand-picked inputs aimed at the places a port can drift from tools.js: number coercion
 * (`+v.tol || 0`, toFixed), truthiness (`p.body ? [p.body] : []`, `||`, `??`), and keys JS leaves
 * undefined (and so drops). Each entry: { name, op, input, result }.
 *   op toParams   — input = form values, result = def.toParams + the `_` / undefined cleanup
 *   op fromParams — input = engine params, result = def.fromParams
 *   op onChange   — input = { trigger, values }, result = the patch (or null)
 */
function extraCases(key, def, defaults, paramsDefaults) {
  const specs = [];
  const add = (name, op, input) => specs.push({ name, op, input });
  const withDefaults = (o) => ({ ...clone(defaults), ...o });
  const withParams = (o) => ({ ...clone(paramsDefaults), ...o });
  const BODY = {
    combine: ['target', 'target'], split: ['body', 'body'], bedsplit: ['body', 'body'], hollow: ['body', 'body'],
    lattice: ['body', 'body'], remesh: ['body', 'body'], repair: ['body', 'body'], separate: ['body', 'body'],
    simplify: ['body', 'body'], thincheck: ['bodies', 'body'], shell: ['body', 'body'],
  };
  if (BODY[key]) {
    const [vk, pk] = BODY[key];
    add('toParams:no body', 'toParams', withDefaults({ [vk]: [] }));
    add('toParams:picked body', 'toParams', withDefaults({ [vk]: ['body_b'] }));
    for (const [n, val] of [['null', null], ['empty string', ''], ['zero', 0], ['name', 'body_b']]) {
      add(`fromParams:body ${n}`, 'fromParams', withParams({ [pk]: val }));
    }
    const noBody = withParams({});
    delete noBody[pk];
    add('fromParams:body missing', 'fromParams', noBody);
  }
  if (key === 'hole') {
    for (const id of PRESET_IDS) {
      for (const tol of [0.1, 0.15, 0.45, -0.1]) {
        add(`onchange:tol=${tol}@${id}`, 'onChange', { trigger: 'tol', values: { preset: id, tol } });
      }
    }
    for (const id of ['ins_M3', 'clr_M3', 'cb_M3', 'nut_M3', 'tap_M3', 'mag_6_3_mm', 'ins_#4-40', 'cb_#6', 'mag_1_8_1_16_']) {
      for (const tol of ['0.3', '', 'abc', null, true, 0, '=w', ' 0.25 ', 1e-7]) {
        add(`onchange:preset=${id} tol=${JSON.stringify(tol)}`, 'onChange', { trigger: 'preset', values: { preset: id, tol } });
      }
    }
    add('onchange:tol@unknown preset', 'onChange', { trigger: 'tol', values: { preset: 'nope', tol: 0.2 } });
    add('onchange:tol@custom', 'onChange', { trigger: 'tol', values: { preset: 'custom', tol: 0.2 } });
    add('onchange:preset@no tol', 'onChange', { trigger: 'preset', values: { preset: 'clr_M3' } });
    for (const k of ['diameter', 'depth', 'kind', 'through', 'cb_diameter', 'cb_depth', 'hex_af', 'hex_depth', 'tip', 'points', 'targets', 'cs_angle']) {
      add(`onchange:${k}`, 'onChange', { trigger: k, values: { preset: 'ins_M3', tol: 0.2 } });
    }
    add('toParams:point key dropped', 'toParams', withDefaults({ point: [1, 2, 3], _x: 1 }));
  }
  if (key === 'resizehole') {
    const picked = { body: 'body_a', point: [1, 2, 3], diameter: 5, mode: 'resize' };
    for (const [n, extra] of [
      ['pin 2.125', { measured: 2.125, kind: 'pin' }],
      ['hole 2.135', { measured: 2.135, kind: 'hole' }],
      ['hole 1.005', { measured: 1.005 }],
      ['hole 0.125', { measured: 0.125, kind: 'hole' }],
      ['measured 0', { measured: 0, kind: 'pin' }],
      ['measured string', { measured: '2.125', kind: 'pin' }],
      ['measured text', { measured: 'abc', kind: 'hole' }],
      ['measured null', { measured: null, kind: 'pin' }],
      ['measured missing', { kind: 'pin' }],
      ['big 1234.5', { measured: 1234.5 }],
      ['other kind', { measured: 8, kind: 'weird' }],
    ]) {
      add(`fromParams:picked ${n}`, 'fromParams', { ...picked, ...extra });
    }
    add('fromParams:point null', 'fromParams', { body: 'body_a', point: null, diameter: 5, mode: 'fill', measured: 3 });
    add('fromParams:point missing', 'fromParams', { body: 'body_a', diameter: 5, mode: 'fill' });
    add('fromParams:point empty list', 'fromParams', { body: 'body_a', point: [], diameter: 5, mode: 'resize', measured: 3 });
    add('fromParams:point zero', 'fromParams', { body: 'body_a', point: 0, diameter: 5, mode: 'resize', measured: 3 });
    add('toParams:keeps measured and kind', 'toParams', {
      face: { label: 'Pin Ø2.13' }, body: 'body_a', point: [1, 2, 3], mode: 'resize', size: 'custom', diameter: 5, measured: 2.125, kind: 'pin',
    });
    add('toParams:nothing picked', 'toParams', { mode: 'resize', size: 'm3', diameter: 3.4 });
    add('onchange:diameter', 'onChange', { trigger: 'diameter', values: { size: 'm3', diameter: 4 } });
    add('onchange:mode', 'onChange', { trigger: 'mode', values: { mode: 'fill' } });
    add('onchange:size unknown', 'onChange', { trigger: 'size', values: { size: 'nope' } });
  }
  if (key === 'scale') {
    add('fromParams:factor 0', 'fromParams', { bodies: ['body_a'], mode: 'factor', factor: 0 });
    add('fromParams:factor null', 'fromParams', { bodies: ['body_a'], mode: 'factor', factor: null });
    add('fromParams:factor missing', 'fromParams', { bodies: ['body_a'], mode: 'fit', size: [4, 5, 6] });
    add('fromParams:factor list', 'fromParams', { bodies: ['body_a'], mode: 'factor', factor: [2, 3, 4], uniform: false });
    add('fromParams:size null', 'fromParams', { bodies: ['body_a'], mode: 'fit', size: null });
    add('fromParams:size empty list', 'fromParams', { bodies: ['body_a'], mode: 'fit', size: [] });
    add('fromParams:size zero', 'fromParams', { bodies: ['body_a'], mode: 'fit', size: 0 });
    add('fromParams:factor string', 'fromParams', { bodies: ['body_a'], mode: 'factor', factor: '=w' });
    const fit = { bodies: ['body_a'], mode: 'fit', uniform: true, units: 'in_to_mm', pivot: 'base', factor: 1, factor3: [1, 1, 1] };
    add('toParams:fit uniform axis 0', 'toParams', { ...fit, size: [20, 20, 10], _size0: [40, 20, 10] });
    add('toParams:fit uniform axis 2', 'toParams', { ...fit, size: [40, 20, 30], _size0: [40, 20, 10] });
    add('toParams:fit uniform all equal', 'toParams', { ...fit, size: [40, 20, 10], _size0: [40, 20, 10] });
    add('toParams:fit uniform strings', 'toParams', { ...fit, size: ['20', '10', '10'], _size0: [10, 10, 10] });
    add('toParams:fit uniform no size0', 'toParams', { ...fit, size: [20, 20, 10] });
    add('toParams:fit not uniform', 'toParams', { ...fit, uniform: false, size: [20, 20, 10], _size0: [40, 20, 10] });
    add('toParams:fit text size', 'toParams', { ...fit, size: ['a', 20, 10], _size0: [40, 20, 10] });
    add('toParams:factor uniform', 'toParams', { ...fit, mode: 'factor', factor: 2, factor3: [1, 2, 3] });
    add('toParams:factor not uniform', 'toParams', { ...fit, mode: 'factor', uniform: false, factor: 2, factor3: [1, 2, 3] });
    add('toParams:factor expression', 'toParams', { ...fit, mode: 'factor', factor: '=w/2' });
    add('toParams:units', 'toParams', { ...fit, mode: 'units', units: 'cm_to_mm' });
    add('toParams:sparse', 'toParams', { mode: 'factor', factor: 3 });
  }
  if (key === 'simplify') {
    for (const percent of [0, 0.05, 0.1, 50, 99.5, 100, 250, '=w', '50', -5]) {
      add(`toParams:percent ${JSON.stringify(percent)}`, 'toParams', withDefaults({ percent }));
    }
    add('toParams:percent missing', 'toParams', { body: ['body_a'] });
    for (const ratio of [0, 0.005, 0.125, 0.333, 0.5, 1, '0.3', null, 'abc', 2.5]) {
      add(`fromParams:ratio ${JSON.stringify(ratio)}`, 'fromParams', { body: 'body_a', ratio });
    }
    add('fromParams:ratio missing', 'fromParams', { body: 'body_a' });
    add('fromParams:empty', 'fromParams', {});
  }
  if (key === 'pattern') {
    const base = { source: 'bodies', features: [], bodies: ['body_a'], kind: 'linear', count: 3, spacing: 20, angle: 360, center: [0, 0, 0], join: false };
    add('fromParams:axisname empty, direction y', 'fromParams', { ...base, axisname: '', direction: [0, 1, 0] });
    add('fromParams:axisname missing, direction x', 'fromParams', { ...base, direction: [1, 0, 0] });
    add('fromParams:axisname missing, direction -z', 'fromParams', { ...base, direction: [0, 0, -3] });
    add('fromParams:direction empty list', 'fromParams', { ...base, direction: [] });
    add('fromParams:direction null', 'fromParams', { ...base, direction: null });
    add('fromParams:direction short', 'fromParams', { ...base, direction: [0, 4] });
    add('fromParams:direction long', 'fromParams', { ...base, direction: [0, 0, 1, 9] });
    add('fromParams:direction missing', 'fromParams', base);
    add('fromParams:axisname 0', 'fromParams', { ...base, axisname: 0, direction: [0, 1, 0] });
    add('fromParams:source empty', 'fromParams', { ...base, source: '' });
    add('fromParams:features null', 'fromParams', { ...base, features: null });
    add('fromParams:minimal', 'fromParams', {});
    add('toParams:unknown axisname', 'toParams', withDefaults({ axisname: 'w' }));
    add('toParams:missing axisname', 'toParams', { source: 'bodies', kind: 'linear', count: 2 });
    add('toParams:expression count', 'toParams', withDefaults({ count: '=n+1', spacing: '=w/2' }));
  }
  if (key === 'mirror') {
    add('fromParams:minimal', 'fromParams', { plane: 'XY' });
    add('fromParams:kind empty', 'fromParams', { kind: '', plane: 'XY' });
  }
  if (key === 'sculpt') {
    add('fromParams:params win', 'fromParams', { body: 'body_a', mode: 'flatten', radius: 9, strength: 1, strokes: [] });
    add('fromParams:strokes only', 'fromParams', { strokes: [{ p: [1, 2, 3] }] });
    add('fromParams:radius 0', 'fromParams', { radius: 0, strength: 0 });
    add('toParams:no strokes', 'toParams', { body: 'body_a', mode: 'smooth' });
    add('toParams:null body', 'toParams', { body: null, strokes: [] });
  }
  if (key === 'loft') {
    add('fromParams:sparse profiles', 'fromParams', { profiles: ['a', '', 'c'], operation: '', targets: null });
    add('fromParams:no profiles', 'fromParams', {});
    add('toParams:gaps', 'toParams', withDefaults({ profile_1: 'a', profile_2: '', profile_3: 'c' }));
    add('toParams:no operation', 'toParams', { profile_1: 'a' });
  }
  if (key === 'import') {
    add('fromParams:place empty', 'fromParams', { place: '', repair: 0 });
    add('fromParams:place null', 'fromParams', { place: null, repair: false });
    add('fromParams:empty', 'fromParams', {});
  }
  if (key === 'plate') {
    add('fromParams:bodies null', 'fromParams', { bodies: null, printer: 'custom', bed: [100, 100, 100] });
    add('fromParams:bodies missing', 'fromParams', { printer: 'bambu_x1' });
    add('toParams:custom printer keeps bed', 'toParams', withDefaults({ printer: 'custom', bed: [100, 120, 90] }));
  }
  if (key === 'bedsplit') {
    add('toParams:custom printer keeps bed', 'toParams', withDefaults({ printer: 'custom', bed: [100, 120, 90] }));
    add('onchange:printer', 'onChange', { trigger: 'printer', values: withDefaults({ printer: 'custom' }) });
    add('onchange:margin', 'onChange', { trigger: 'margin', values: withDefaults({}) });
  }
  if (key === 'split') {
    add('onchange:keep', 'onChange', { trigger: 'keep', values: withDefaults({ keep: 'above' }) });
  }
  if (key === 'layflat') {
    add('toParams:auto no body', 'toParams', withDefaults({ how: 'auto', bodies: [] }));
    add('toParams:face no normal', 'toParams', { how: 'face', face: { label: 'x' } });
    add('toParams:face normal', 'toParams', { how: 'face', body: 'body_a', face: { normal: [0, 0, 1], label: 'x' } });
    add('toParams:no face', 'toParams', { how: 'face', body: 'body_a', face: null });
    add('toParams:body null', 'toParams', { how: 'face', body: null, face: { normal: [1, 0, 0] } });
    add('fromParams:auto', 'fromParams', { auto: true, body: 'body_a' });
    add('fromParams:auto 1', 'fromParams', { auto: 1, body: 'body_a' });
    add('fromParams:auto no body', 'fromParams', { auto: true });
    add('fromParams:auto false', 'fromParams', { auto: false, body: 'body_a', normal: [0, 0, 1] });
    add('fromParams:normal only', 'fromParams', { normal: [0, 0, 1] });
    add('fromParams:no normal', 'fromParams', { body: 'body_a' });
    add('fromParams:normal null', 'fromParams', { body: 'body_a', normal: null });
  }
  if (key === 'shell') {
    const s = withDefaults({ face1: { normal: [0, 0, 1] }, face2: { normal: [1, 0, 0] }, face3: null, face4: { normal: [0, 1, 0] } });
    for (const openings of ['0', '1', '2', '3', '4', 2, '2.5', '', null, 'abc']) {
      add(`toParams:openings ${JSON.stringify(openings)}`, 'toParams', { ...s, openings });
    }
    add('toParams:body is text', 'toParams', { ...s, body: 'body_a', openings: '1' });
    add('toParams:body is empty list', 'toParams', { ...s, body: [], openings: '1' });
    add('fromParams:5 faces', 'fromParams', { body: 'body_a', faces: [1, 2, 3, 4, 5].map((i) => ({ i })), thickness: 2 });
    add('fromParams:falsy faces', 'fromParams', { body: 'body_a', faces: [0, null, { a: 1 }] });
    add('fromParams:faces null', 'fromParams', { body: 'body_a', faces: null });
    add('fromParams:faces missing', 'fromParams', { body: 'body_a' });
  }
  if (key === 'thread') {
    const plane = { body: 'body_a', origin: [0, 0, 10], normal: [0, 0, 1] };
    for (const [n, p] of [['face', plane], ['name', 'XY'], ['list', [1, 2]], ['null', null], ['empty', '']]) {
      add(`onchange:kind=external plane ${n}`, 'onChange', { trigger: 'kind', values: withDefaults({ kind: 'external', plane: p }) });
      add(`onchange:plane plane ${n}`, 'onChange', { trigger: 'plane', values: withDefaults({ kind: 'external', plane: p }) });
    }
    add('onchange:plane internal', 'onChange', { trigger: 'plane', values: withDefaults({ kind: 'internal', plane }) });
    add('onchange:diameter', 'onChange', { trigger: 'diameter', values: defaults });
    add('onchange:pitch', 'onChange', { trigger: 'pitch', values: defaults });
    add('onchange:size unknown', 'onChange', { trigger: 'size', values: withDefaults({ size: 'nope' }) });
    add('toParams:external screw', 'toParams', withDefaults({ kind: 'external', size: 'M5' }));
    add('toParams:custom size', 'toParams', withDefaults({ kind: 'external', size: 'custom' }));
    add('toParams:unknown size', 'toParams', withDefaults({ size: 'nope' }));
    add('fromParams:keeps size', 'fromParams', { kind: 'internal', size: 'M3', diameter: 3 });
  }
  if (key === 'pushpull') {
    add('toParams:sparse', 'toParams', { distance: 3 });
    add('toParams:picked', 'toParams', { body: 'body_a', face: { normal: [0, 0, 1] }, distance: '=h/2', _x: 1 });
  }
  if (key === 'combine') {
    add('toParams:two tools', 'toParams', withDefaults({ target: ['body_a'], tools: ['body_b'], operation: 'cut' }));
  }
  if (key === 'move') {
    add('fromParams:minimal', 'fromParams', { bodies: ['body_a'], translate: [1, 2, 3] });
  }

  return specs.map((s) => {
    const input = clone(s.input);
    let result;
    if (s.op === 'toParams') result = toParamsOf(def, input);
    else if (s.op === 'fromParams') result = fromParamsOf(def, input);
    else result = def.onChange(input.trigger, clone(input.values)) ?? null;
    return { name: s.name, op: s.op, input: s.input, result };
  });
}

function recordTool(key, def, ctx = FIXTURE_CTX, full = false) {
  const defaults = def.defaults(ctx);
  const toParams_defaults = toParamsOf(def, defaults);
  const roundTrips = buildRoundTrips(def, defaults, toParams_defaults, full);
  const extras = full ? { ctxCases: buildCtxCases(def), extra: extraCases(key, def, defaults, toParams_defaults) } : {};
  return {
    tool: key,
    label: def.label,
    type: def.type,
    icon: def.icon,
    flags: flagsOf(def),
    hint: def.hint ?? null,
    extra: def.extra ?? null,
    fields: (def.fields || []).filter((f) => f.key).map(fieldMeta),
    ctxNote:
      'FIXTURE_CTX: selection [body_a], lastSketch sk1, sketchOnBody true, two bodies, one extrude cut',
    defaults,
    toParams_defaults,
    shown_defaults: shownKeys(def, defaults),
    cases: buildCases(def, defaults),
    onChange: buildOnChange(key, def, defaults),
    roundTrips,
    ...extras,
  };
}

/** Port of app.js supportTools field/defaults/toParams/fromParams for golden capture. */
async function trySupportTools(ctx) {
  const sfUrl = pathToFileURL(path.join(ROOT, 'web', 'vendor', 'support-fins', 'engine.esm.js')).href;
  let SF;
  try {
    SF = await import(sfUrl);
  } catch (e) {
    return { error: `vendor engine.esm.js failed to import: ${e.message}` };
  }
  const schema = SF.OPTIONS_SCHEMA;
  if (!schema?.options) return { error: 'OPTIONS_SCHEMA missing on vendor engine' };
  const opts = schema.options;
  const fk = (k) => 'sf:' + k;
  const vis = (o, v) => {
    const get = (k) => {
      const x = v[fk(k)];
      return x === undefined || x === null || x === '' ? opts.find((q) => q.key === k)?.default : x;
    };
    const holds = (c) =>
      typeof c === 'string' ? get(c) === true : c.any ? c.any.some(holds) : c.in.includes(get(c.key));
    return (o.showIf || []).every(holds);
  };
  const shown = (o) => (o.percent ? Math.round(o.default * 100 * 1000) / 1000 : o.default);
  const fieldOf = (o) => {
    const base = { key: fk(o.key), label: o.label, help: o.tooltip, show: (v) => vis(o, v) };
    if (o.type === 'bool') return { ...base, kind: 'bool' };
    if (o.type === 'choice') return { ...base, kind: 'select', options: o.choices.map((c) => [c.value, c.label]) };
    return {
      ...base,
      kind: 'number',
      unit: o.percent ? '%' : /^mm\b/.test(o.hint || '') ? 'mm' : /^[°%]/.test(o.hint || '') ? o.hint : '',
    };
  };
  const sections = (keys, nOpen) => {
    const out = [];
    for (const sec of schema.sections) {
      const items = opts.filter((o) => o.section === sec.id && (!keys || keys.includes(o.key)));
      if (!items.length) continue;
      out.push(
        {
          key: '_hdr_' + sec.id,
          kind: 'hdr',
          label: sec.label,
          open: out.filter((f) => f.kind === 'hdr').length < nOpen,
          show: (v) => items.some((o) => vis(o, v)),
        },
        ...items.map(fieldOf),
      );
    }
    return out;
  };
  const defaultsOpts = (keys) =>
    Object.fromEntries(opts.filter((o) => !keys || keys.includes(o.key)).map((o) => [fk(o.key), shown(o)]));
  const toOptions = (v, keys) => {
    const out = {};
    for (const o of opts) {
      if (keys && !keys.includes(o.key)) continue;
      let x = v[fk(o.key)];
      if (x === undefined || x === null || x === '') continue;
      if (o.type === 'number') {
        x = +x;
        if (o.percent) x = Math.round((x * 1e6) / 100) / 1e6;
      }
      if (x === o.default || (o.type === 'number' && Math.abs(x - o.default) < 1e-9)) continue;
      out[o.key] = x;
    }
    return out;
  };
  const fromOptions = (options = {}) =>
    Object.fromEntries(
      Object.entries(options).map(([k, x]) => {
        const o = opts.find((q) => q.key === k);
        return [fk(k), o?.percent && typeof x === 'number' ? Math.round(x * 100 * 1000) / 1000 : x];
      }),
    );
  const output = {
    key: 'output',
    label: 'Add as',
    kind: 'select',
    options: [
      ['join', 'Part of the body'],
      ['separate', 'Separate body'],
    ],
    help: 'Part of the body: the fins are joined to it, so it exports and slices as one object (what Support Fins itself does). Separate body: a body of its own you can hide or remove; export both together.',
  };
  const checkSection = (cover) => [
    { key: '_hdr_check', kind: 'hdr', label: 'Print check', open: true },
    { key: '_status', kind: 'sfstatus', cover },
    ...(cover
      ? [{ key: '_pose', label: 'Orientation', kind: 'sfpose', show: (v) => (v._mode || 'rotate') === 'rotate' }]
      : []),
    {
      key: '_mat',
      label: 'Material',
      kind: 'sfmat',
      help: 'The filament you print the supports in, for the weight under Support plastic.',
    },
    ...(cover
      ? [
          {
            key: '_reach',
            label: 'Bare beyond',
            kind: 'sfreach',
            help: "Overhangs further than this from a fin, or from the part itself, are shown bright red: they'd sag or fail. Bridging over a few mm works; over 5 mm usually droops.",
          },
        ]
      : []),
  ];
  const WALL_KEYS = ['threshold', 'nozzle', 'gap', 'tines', 'tineDensity', 'layerHeight', 'plateOnly'];
  const oneBody = (c) => {
    const sel = selBody(c).slice(0, 1);
    if (sel.length) return sel;
    const shownB = c.state.bodies.filter((b) => !b.hidden);
    return shownB.length === 1 ? [shownB[0].id] : [];
  };
  const MODES = [
    ['rotate', 'Rotate'],
    ['add', 'Add fins'],
    ['remove', 'Remove fins'],
  ];
  const supportfins = {
    label: 'Support Fins',
    icon: 'fins',
    type: 'supportfins',
    paint: true,
    gizmo: 'pose',
    fields: [
      { key: 'body', label: 'Body', kind: 'bodies', multi: false },
      output,
      {
        key: '_mode',
        label: 'On the part',
        kind: 'select',
        options: MODES,
        nolabel: true,
        help: 'Rotate: drag the rings to turn the part the way it will print (it stays on the bed, and the fins follow). Add fins: draw walls by hand where Auto left a gap. Remove fins: click a fin to take it away, click it again to bring it back.',
      },
      { key: 'walls', label: 'Hand walls', kind: 'walls', show: (v) => v._mode === 'add' },
      {
        key: 'removed',
        label: 'Removed',
        kind: 'removed',
        show: (v) => v._mode === 'remove' || (v.removed || []).length > 0,
      },
      ...sections(null, 2),
      ...checkSection(true),
    ],
    defaults: (c) => ({
      body: oneBody(c),
      _mode: 'rotate',
      walls: [],
      removed: [],
      output: 'join',
      ...defaultsOpts(null),
    }),
    toParams: (v) => {
      const p = { body: v.body[0], output: v.output, options: toOptions(v, null) };
      if ((v.walls || []).length) p.walls = v.walls;
      if ((v.removed || []).length) p.removed = v.removed;
      return p;
    },
    fromParams: (p) => ({
      body: p.body ? [p.body] : [],
      _mode: 'rotate',
      walls: p.walls || [],
      removed: p.removed || [],
      output: p.output || 'join',
      ...defaultsOpts(null),
      ...fromOptions(p.options),
    }),
    hint: 'Red faces need support. Turn the part with the rings (it stays on the bed); Support Fins puts breakaway fins and a bed pad under the red.',
  };
  const supportwall = {
    label: 'Draw Walls',
    icon: 'finwall',
    type: 'supportwall',
    wallTool: true,
    paint: true,
    fields: [
      { key: 'body', label: 'Body', kind: 'bodies', multi: false, placeholder: 'The part you draw on' },
      output,
      { key: 'walls', label: 'Walls', kind: 'walls' },
      ...sections(WALL_KEYS, 1),
      ...checkSection(false),
    ],
    defaults: (c) => ({ body: oneBody(c), walls: [], output: 'join', ...defaultsOpts(WALL_KEYS) }),
    toParams: (v) => ({ body: v.body[0], walls: v.walls, output: v.output, options: toOptions(v, WALL_KEYS) }),
    fromParams: (p) => ({
      body: p.body ? [p.body] : [],
      walls: Array.isArray(p.walls)
        ? p.walls
        : (p.points || []).length === 2
          ? [{ shape: 'line', points: p.points }]
          : [],
      output: p.output || 'join',
      ...defaultsOpts(WALL_KEYS),
      ...fromOptions(p.options),
    }),
    // app.js: `hint: this.walls.hint()` = drawwalls.js HINTS[shape]; the default shape is 'line'
    hint: 'Click two points under an overhang for each wall. Keep clicking to add more walls.',
  };
  return {
    schemaVersion: SF.OPTIONS_SCHEMA /* keep schema presence */,
    tools: {
      supportfins: recordTool('supportfins', supportfins, ctx),
      supportwall: recordTool('supportwall', supportwall, ctx),
    },
    note: `Support Fins schema from vendor engine.esm.js (${opts.length} options, ${schema.sections.length} sections)`,
  };
}

function generateAll(support) {
  const files = {};
  for (const key of Object.keys(TOOLS)) {
    files[`${key}.json`] = dumps(recordTool(key, TOOLS[key], FIXTURE_CTX, true));
  }
  if (support?.tools) {
    for (const [k, rec] of Object.entries(support.tools)) {
      files[`${k}.json`] = dumps(rec);
    }
  }
  return files;
}

async function main() {
  const check = process.argv.includes('--check');
  fs.mkdirSync(OUT_DIR, { recursive: true });

  const support = await trySupportTools(FIXTURE_CTX);
  if (support.error) {
    console.warn(`PARKED supportfins/supportwall golden: ${support.error}`);
  } else {
    console.log(support.note);
  }

  const files = generateAll(support.error ? null : support);
  let failed = 0;
  for (const [name, body] of Object.entries(files)) {
    const dest = path.join(OUT_DIR, name);
    if (check) {
      if (!fs.existsSync(dest)) {
        console.error(`missing ${dest}`);
        failed++;
        continue;
      }
      const onDisk = normNewlines(fs.readFileSync(dest, 'utf8'));
      if (onDisk !== normNewlines(body)) {
        console.error(`outdated ${dest}`);
        failed++;
      } else {
        console.log(`ok ${name}`);
      }
    } else {
      fs.writeFileSync(dest, body);
      console.log(`wrote ${dest}`);
    }
  }

  // Remove stale tool files not in this run (only when writing, not when parked SF left old files).
  if (!check) {
    const keep = new Set(Object.keys(files));
    for (const ent of fs.readdirSync(OUT_DIR)) {
      if (!ent.endsWith('.json')) continue;
      if (!keep.has(ent)) {
        // keep parked supportfins/supportwall if we could not regenerate them
        if (support.error && (ent === 'supportfins.json' || ent === 'supportwall.json')) continue;
        fs.unlinkSync(path.join(OUT_DIR, ent));
        console.log(`removed stale ${ent}`);
      }
    }
  }

  if (check && failed) {
    console.error(`${failed} golden file(s) out of date — run without --check`);
    process.exit(1);
  }
  console.log(check ? 'tools_golden --check ok' : `tools_golden wrote ${Object.keys(files).length} files`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
