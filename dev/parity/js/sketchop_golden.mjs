/**
 * Generate rust/tests/parity/sketchop/<case>.json from 1.x routes.js sketch_op.
 *
 * Loads the fixture project (basic.stlx) as `app.doc`, runs a fixed case list through
 * ROUTES.sketch_op with seed "g", and writes `{ op, data, result }` per case.
 *
 * Usage:
 *   node dev/parity/js/sketchop_golden.mjs
 *   node dev/parity/js/sketchop_golden.mjs --check
 */
import fs from 'fs';
import path from 'path';
import { fileURLToPath, pathToFileURL } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '..', '..', '..');
const OUT_DIR = path.join(ROOT, 'rust', 'tests', 'parity', 'sketchop');
const WEB = path.join(ROOT, 'web');
const PROJECT = path.join(ROOT, 'rust', 'tests', 'projects', 'basic.stlx');
const TIPPED = path.join(ROOT, 'tests', 'ui', 'fixtures', 'tipped.stl');

const load = (rel) => import(pathToFileURL(path.join(WEB, rel)).href);

function dumps(obj) {
  return JSON.stringify(obj, null, 2) + '\n';
}

function normNewlines(s) {
  return s.replace(/\r\n/g, '\n').replace(/\r/g, '\n');
}

/** Stable JSON clone (strips undefined; keeps insertion order). */
function cloneJson(v) {
  return JSON.parse(JSON.stringify(v));
}

const AXIS = 1e4;
const AXIS_H = { id: '_h', type: 'line', p1: [-AXIS, 0], p2: [AXIS, 0], ref: 'axis' };
const AXIS_V = { id: '_v', type: 'line', p1: [0, -AXIS], p2: [0, AXIS], ref: 'axis' };

function topFrame(body, G) {
  const [lo, hi] = body.bbox();
  const origin = [(lo[0] + hi[0]) / 2, (lo[1] + hi[1]) / 2, hi[2]];
  return G.planeFrame(origin, [0, 0, 1]);
}

function midFrame(body, G) {
  const [lo, hi] = body.bbox();
  const origin = [(lo[0] + hi[0]) / 2, (lo[1] + hi[1]) / 2, (lo[2] + hi[2]) / 2];
  return G.planeFrame(origin, [0, 0, 1]);
}

async function setup() {
  const W = await load('engine/wasm.js');
  await W.init();
  const MT = await load('engine/meshtools.js');
  MT.setFontLoader(async (name, bold, italic) => {
    const fam = {
      'DejaVu Sans': { '': 'DejaVuSans.ttf', b: 'DejaVuSans-Bold.ttf', i: 'DejaVuSans-Oblique.ttf', bi: 'DejaVuSans-BoldOblique.ttf' },
      'DejaVu Serif': { '': 'DejaVuSerif.ttf', b: 'DejaVuSerif-Bold.ttf' },
      'DejaVu Sans Mono': { '': 'DejaVuSansMono.ttf', b: 'DejaVuSansMono-Bold.ttf' },
    };
    const styles = fam[name] || fam['DejaVu Sans'];
    const style = (bold ? 'b' : '') + (italic ? 'i' : '');
    const file = styles[style] || styles[bold ? 'b' : ''] || styles[''];
    const buf = fs.readFileSync(path.join(WEB, 'fonts', file));
    return buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength);
  });
  const R = await load('engine/routes.js');
  const G = await load('engine/geom.js');
  const { Document } = await load('engine/document.js');
  return { R, G, Document, W };
}

function loadFixture(R) {
  const bytes = fs.readFileSync(PROJECT);
  R.ROUTES.open_bytes({ name: 'basic.stlx', data: bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) });
}

function loadBox(R) {
  R.ROUTES.new({ force: true });
  R.ROUTES['feature/add']({
    type: 'primitive',
    params: {
      shape: 'box',
      length: 40,
      width: 20,
      height: 10,
      anchor: 'corner',
      plane: 'XY',
      operation: 'new',
      offset: [0, 0, 0],
      rotate: [0, 0, 0],
    },
  });
}

function loadTipped(R) {
  const bytes = fs.readFileSync(TIPPED);
  R.ROUTES.open_bytes({
    name: 'tipped.stl',
    data: bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength),
  });
}

/** Case builders return { name, op, data } (data includes seed and op args; no top-level op). */
function caseList(ctx) {
  const { R, G } = ctx;
  const SEED = 'g';

  const trim_line = {
    name: 'trim_line',
    op: 'trim',
    data: {
      seed: SEED,
      entities: [
        { id: 'a', type: 'line', p1: [0, 0], p2: [10, 0] },
        { id: 'b', type: 'line', p1: [3, -5], p2: [3, 5] },
        { id: 'c', type: 'line', p1: [7, -5], p2: [7, 5] },
      ],
      id: 'a',
      point: [5, 0],
    },
  };

  const trim_arc = {
    name: 'trim_arc',
    op: 'trim',
    data: {
      seed: SEED,
      entities: [
        { id: 'arc', type: 'arc', cx: 0, cy: 0, r: 10, a0: 0, a1: 180 },
        { id: 'b', type: 'line', p1: [5, -15], p2: [5, 15] },
        { id: 'c', type: 'line', p1: [-5, -15], p2: [-5, 15] },
      ],
      id: 'arc',
      point: [0, 10],
    },
  };

  const offset_closed = {
    name: 'offset_closed',
    op: 'offset',
    data: {
      seed: SEED,
      entities: [{ id: 'r', type: 'rect', x: 0, y: 0, w: 20, h: 10 }],
      ids: ['r'],
      distance: 2,
    },
  };

  const offset_open = {
    name: 'offset_open',
    op: 'offset',
    data: {
      seed: SEED,
      entities: [
        { id: 'l1', type: 'line', p1: [0, 0], p2: [10, 0] },
        { id: 'l2', type: 'line', p1: [10, 0], p2: [10, 8] },
        { id: 'l3', type: 'line', p1: [10, 8], p2: [4, 8] },
      ],
      ids: ['l1', 'l2', 'l3'],
      distance: 1.5,
    },
  };

  const offset_negative = {
    name: 'offset_negative',
    op: 'offset',
    data: {
      seed: SEED,
      entities: [{ id: 'r', type: 'rect', x: 0, y: 0, w: 20, h: 10 }],
      ids: ['r'],
      distance: -2,
    },
  };

  const mirror_line = {
    name: 'mirror_line',
    op: 'mirror',
    data: {
      seed: SEED,
      entities: [
        { id: 'c', type: 'circle', cx: 8, cy: 4, r: 3 },
        { id: 'ax', type: 'line', p1: [0, -10], p2: [0, 10] },
      ],
      ids: ['c'],
      axis: 'ax',
    },
  };

  const mirror_axis = {
    name: 'mirror_axis',
    op: 'mirror',
    data: {
      seed: SEED,
      entities: [
        { id: 'c', type: 'circle', cx: 8, cy: 4, r: 3 },
        { ...AXIS_V },
      ],
      ids: ['c'],
      axis: '_v',
    },
  };

  const explode_rect = {
    name: 'explode_rect',
    op: 'explode',
    data: {
      seed: SEED,
      entities: [{ id: 'r', type: 'rect', x: 1, y: 2, w: 12, h: 8 }],
      ids: ['r'],
    },
  };

  const explode_polygon = {
    name: 'explode_polygon',
    op: 'explode',
    data: {
      seed: SEED,
      entities: [{ id: 'p', type: 'polygon', cx: 0, cy: 0, r: 10, n: 6, angle: 0 }],
      ids: ['p'],
    },
  };

  const explode_slot = {
    name: 'explode_slot',
    op: 'explode',
    data: {
      seed: SEED,
      entities: [{ id: 's', type: 'slot', x1: 0, y1: 0, x2: 20, y2: 0, r: 3 }],
      ids: ['s'],
    },
  };

  const faces_nested = {
    name: 'faces_nested',
    op: 'faces',
    data: {
      seed: SEED,
      entities: [
        { id: 'outer', type: 'rect', x: 0, y: 0, w: 20, h: 16 },
        { id: 'inner', type: 'rect', x: 5, y: 4, w: 8, h: 6 },
      ],
    },
  };

  const textBase = {
    seed: SEED,
    entities: [],
    text: 'Hi',
    font: 'DejaVu Sans',
    size: 10,
    bold: false,
    italic: false,
    spacing: 0,
    x: 0,
    y: 0,
  };

  const text_align_left = {
    name: 'text_align_left',
    op: 'text',
    data: { ...textBase, align: 'left', anchor: 'baseline', angle: 0 },
  };
  const text_align_center = {
    name: 'text_align_center',
    op: 'text',
    data: { ...textBase, align: 'center', anchor: 'baseline', angle: 0 },
  };
  const text_align_right = {
    name: 'text_align_right',
    op: 'text',
    data: { ...textBase, align: 'right', anchor: 'baseline', angle: 0 },
  };
  const text_anchor_baseline = {
    name: 'text_anchor_baseline',
    op: 'text',
    data: { ...textBase, align: 'left', anchor: 'baseline', angle: 0 },
  };
  const text_anchor_center = {
    name: 'text_anchor_center',
    op: 'text',
    data: { ...textBase, align: 'left', anchor: 'center', angle: 0 },
  };
  const text_angle_30 = {
    name: 'text_angle_30',
    op: 'text',
    data: { ...textBase, align: 'left', anchor: 'baseline', angle: 30 },
  };

  // Solid-dependent cases: mutate app.doc then capture frame.
  loadBox(R);
  const boxBody = [...R.app.doc.state.bodies.values()][0];
  const boxFrame = topFrame(boxBody, G);

  const project_section = {
    name: 'project_section',
    op: 'project',
    data: {
      seed: SEED,
      entities: [],
      frame: boxFrame,
      mode: 'section',
      construction: false,
      tolerance: 0.02,
      offset: 0,
    },
  };
  // 1.x meshtools.outline offsets with CrossSection.offset(offset, 'Round', 2, 64).
  const project_section_offset = {
    name: 'project_section_offset',
    op: 'project',
    data: {
      seed: SEED,
      entities: [],
      frame: boxFrame,
      mode: 'section',
      construction: false,
      tolerance: 0.02,
      offset: 1.5,
    },
  };
  const project_section_construction = {
    name: 'project_section_construction',
    op: 'project',
    data: {
      seed: SEED,
      entities: [],
      frame: boxFrame,
      mode: 'section',
      construction: true,
      tolerance: 0.02,
      offset: 0,
    },
  };
  const project_silhouette = {
    name: 'project_silhouette',
    op: 'project',
    data: {
      seed: SEED,
      entities: [],
      frame: boxFrame,
      mode: 'silhouette',
      construction: false,
      tolerance: 0.02,
      offset: 0,
    },
  };
  const project_silhouette_construction = {
    name: 'project_silhouette_construction',
    op: 'project',
    data: {
      seed: SEED,
      entities: [],
      frame: boxFrame,
      mode: 'silhouette',
      construction: true,
      tolerance: 0.02,
      offset: 0,
    },
  };

  const refs_box = {
    name: 'refs_box',
    op: 'refs',
    data: { seed: SEED, entities: [], frame: boxFrame },
  };

  loadTipped(R);
  const tipBody = [...R.app.doc.state.bodies.values()][0];
  const tipFrame = midFrame(tipBody, G);
  const refs_tipped = {
    name: 'refs_tipped',
    op: 'refs',
    data: { seed: SEED, entities: [], frame: tipFrame },
  };

  // Restore fixture project as the default doc for entity-only cases that do not need solids.
  loadFixture(R);

  return [
    trim_line,
    trim_arc,
    offset_closed,
    offset_open,
    offset_negative,
    mirror_line,
    mirror_axis,
    explode_rect,
    explode_polygon,
    explode_slot,
    project_section,
    project_section_offset,
    project_section_construction,
    project_silhouette,
    project_silhouette_construction,
    refs_box,
    refs_tipped,
    text_align_left,
    text_align_center,
    text_align_right,
    text_anchor_baseline,
    text_anchor_center,
    text_angle_30,
    faces_nested,
  ];
}

async function runCase(R, c) {
  // Solid-dependent cases need the matching doc loaded again.
  if (c.name.startsWith('project_') || c.name === 'refs_box') {
    loadBox(R);
  } else if (c.name === 'refs_tipped') {
    loadTipped(R);
  } else {
    loadFixture(R);
  }
  const data = cloneJson(c.data);
  const result = await R.ROUTES.sketch_op({ op: c.op, ...data });
  return { op: c.op, data, result: cloneJson(result) };
}

async function build() {
  const ctx = await setup();
  const cases = caseList(ctx);
  const records = {};
  for (const c of cases) {
    records[c.name] = await runCase(ctx.R, c);
  }
  return records;
}

const check = process.argv.includes('--check');
const records = await build();
const names = Object.keys(records).sort();

if (check) {
  let bad = 0;
  for (const name of names) {
    const dest = path.join(OUT_DIR, `${name}.json`);
    const text = dumps(records[name]);
    const existing = fs.existsSync(dest) ? fs.readFileSync(dest, 'utf8') : '';
    if (normNewlines(existing) !== normNewlines(text)) {
      console.error(`sketchop_golden: MISMATCH ${path.relative(ROOT, dest)}`);
      bad++;
    }
  }
  const listed = new Set(names);
  if (!fs.existsSync(OUT_DIR)) {
    console.error(`sketchop_golden: MISSING ${path.relative(ROOT, OUT_DIR)}`);
    bad++;
  } else {
    for (const file of fs.readdirSync(OUT_DIR)) {
      if (!file.endsWith('.json')) continue;
      const stem = file.slice(0, -'.json'.length);
      if (!listed.has(stem)) {
        console.error(`sketchop_golden: STALE ${path.relative(ROOT, path.join(OUT_DIR, file))}`);
        bad++;
      }
    }
  }
  if (bad) process.exit(1);
  console.log(`sketchop_golden: ok (${names.length} cases)`);
  process.exit(0);
}

fs.mkdirSync(OUT_DIR, { recursive: true });
for (const name of names) {
  const dest = path.join(OUT_DIR, `${name}.json`);
  fs.writeFileSync(dest, dumps(records[name]), 'utf8');
}
console.log(`wrote ${path.relative(ROOT, OUT_DIR)} (${names.length} cases)`);
