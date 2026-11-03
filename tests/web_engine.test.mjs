// Tests for the web edition's JavaScript engine (web/engine). Run: node --test tests
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const web = path.join(here, '..', 'web');
const load = rel => import(pathToFileURL(path.join(web, rel)).href);   // file:// URLs work on Windows too
const W = await load('engine/wasm.js');
await W.init();
const { Document } = await load('engine/document.js');
const G = await load('engine/geom.js');
const IO = await load('engine/io.js');
const MT = await load('engine/meshtools.js');
const S = await load('engine/sketch2d.js');
MT.setFontLoader(async (name, bold) => { const b = fs.readFileSync(path.join(web, 'fonts', bold ? 'DejaVuSans-Bold.ttf' : 'DejaVuSans.ttf')); return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength); });

const approx = (a, b, rel = 1e-3, msg) => assert.ok(Math.abs(a - b) <= rel * Math.max(Math.abs(b), 1e-9), `${msg || ''} ${a} vs ${b}`);
const vol = (doc, id) => doc.state.body(id).info().volume;
function stlOf(man) { const m = man.getMesh(); return IO.stlBytes([{ v: m.vertProperties, f: m.triVerts }]); }
function block() {
  const doc = new Document();
  const [f] = doc.importFile('block.stl', stlOf(W.Manifold.cube([40, 30, 20])), true, 'keep');
  return [doc, f.id];
}

test('import centres on bed', () => {
  const doc = new Document();
  const [f] = doc.importFile('c.stl', stlOf(W.Manifold.cube([10, 10, 10]).translate([50, 60, 70])));
  const [lo, hi] = doc.state.body(f.id).bbox();
  approx(lo[2] + 1, 1); approx((lo[0] + hi[0]) / 2 + 1, 1); approx((lo[1] + hi[1]) / 2 + 1, 1);
});

test('sketch cut and join', () => {
  const [doc, B] = block();
  const v0 = vol(doc, B);
  const [sk] = doc.addFeature('sketch', { plane: { origin: [0, 0, 20], normal: [0, 0, 1] }, entities: [
    { id: 'r', type: 'rect', x: 5, y: 5, w: 30, h: 20 }, { id: 'c', type: 'circle', cx: 20, cy: 15, r: 5 }] });
  doc.addFeature('extrude', { sketch: sk.id, distance: 8, operation: 'cut' });
  approx(vol(doc, B), v0 - (30 * 20 - Math.PI * 25) * 8, 2e-3, 'cut');
  doc.addFeature('extrude', { sketch: sk.id, distance: 5, operation: 'join', entities: ['c'] });
  approx(doc.state.body(B).bbox()[1][2], 25);
});

test('regions from lines split a rectangle', () => {
  const ents = [{ id: 'r', type: 'rect', x: 0, y: 0, w: 20, h: 10 }, { id: 'l', type: 'line', p1: [8, -5], p2: [8, 15] },
    { id: 'c', type: 'circle', cx: 15, cy: 5, r: 2 }];
  const fs2 = S.faces(ents);
  const areas = fs2.map(f => Math.round(f.area)).sort((a, b) => a - b);
  assert.deepEqual(areas, [13, 80, 107], `areas ${areas}`);
  const cs = S.profile(ents, null, [[4, 5]]);
  approx(cs.area(), 80, 1e-6);
  const cs2 = S.profile(ents, null, [[11, 5]]);
  approx(cs2.area(), 120 - Math.PI * 4, 1e-2);
});

test('trim a line between crossings', () => {
  const ents = [{ id: 'a', type: 'line', p1: [0, 0], p2: [10, 0] }, { id: 'b', type: 'line', p1: [3, -5], p2: [3, 5] }, { id: 'c', type: 'line', p1: [7, -5], p2: [7, 5] }];
  const out = S.trim(ents, 'a', [5, 0]);
  const lines = out.filter(e => e.id.startsWith('a')).map(e => [e.p1[0], e.p2[0]]);
  assert.deepEqual(lines, [[0, 3], [7, 10]]);
});

test('holes and presets', () => {
  const [doc, B] = block();
  const v0 = vol(doc, B);
  doc.addFeature('hole', { points: [[10, 15, 20]], normal: [0, 0, 1], diameter: 5, through: true });
  approx(v0 - vol(doc, B), Math.PI * 2.5 ** 2 * 20, 0.01);
  for (const kind of ['counterbore', 'countersink', 'hex']) doc.addFeature('hole', { points: [[25, 8, 20]], normal: [0, 0, 1], diameter: 3.4, kind });
  assert.ok(doc.state.body(B).isSolid);
});

test('thread bolt and nut mesh', () => {
  const doc = new Document();
  doc.addFeature('thread', { kind: 'external', diameter: 8, pitch: 1.25, length: 12, tolerance: 0.2, plane: 'XY', offset: [0, 0, 0] });
  const bolt = doc.state.bodies.get(doc.features.at(-1).id).solid;
  const [nf] = doc.addFeature('primitive', { shape: 'cylinder', radius: 7.5, height: 6, segments: 6, plane: 'XY', offset: [0, 0, 4] });
  doc.addFeature('thread', { kind: 'internal', diameter: 8, pitch: 1.25, length: 6, tolerance: 0.2, plane: 'XY', offset: [0, 0, 10], operation: 'cut', targets: [nf.id] });
  const nut = doc.state.body(nf.id).solid;
  let best = Infinity;
  for (let a = 0; a < 360; a += 15) best = Math.min(best, bolt.intersect(nut.rotate([0, 0, a])).volume());
  assert.ok(best < 0.05, `interference ${best}`);
});

test('lay flat, split with pins, fit to bed', () => {
  const [doc, B] = block();
  doc.addFeature('move', { bodies: [B], rotate: [30, 20, 0], translate: [0, 0, 0] });
  doc.addFeature('layflat', { bodies: [B], auto: true });
  const [lo, hi] = doc.state.body(B).bbox();
  approx(lo[2] + 1, 1, 1e-6); approx(hi[2] - lo[2], 20, 0.003);
  const d2 = new Document();
  const [f] = d2.addFeature('primitive', { shape: 'box', length: 400, width: 60, height: 40 });
  d2.addFeature('bedsplit', { body: f.id, printer: 'bambu_a1mini', connector: 'pins' });
  assert.ok(d2.state.bodies.size >= 3);
  for (const b of d2.state.bodies.values()) { const [l, h] = b.bbox(); assert.ok(Math.max(...G.sub(h, l)) <= 180 + 1e-6); }
});

test('mirror and pattern features', () => {
  const [doc, B] = block();
  const [h] = doc.addFeature('hole', { points: [[5, 15, 20]], normal: [0, 0, 1], diameter: 4, through: true });
  const v1 = vol(doc, B);
  doc.addFeature('mirror', { kind: 'features', features: [h.id], plane: { origin: [20, 0, 0], normal: [1, 0, 0] } });
  approx(v1 - vol(doc, B), Math.PI * 4 * 20, 0.02);
  const v2 = vol(doc, B);
  doc.addFeature('pattern', { source: 'features', features: [h.id], kind: 'linear', direction: [0, 1, 0], count: 3, spacing: 5 });
  approx(v2 - vol(doc, B), 2 * Math.PI * 4 * 20, 0.03);
});

test('quick repair of an open mesh', () => {
  const m = W.Manifold.sphere(10, 32).getMesh();
  const f = m.triVerts.slice(15);
  const doc = new Document();
  const [ft] = doc.importFile('b.stl', IO.stlBytes([{ v: m.vertProperties, f }]), false, 'keep');
  assert.ok(!doc.state.body(ft.id).isSolid);
  doc.addFeature('repair', { body: ft.id, mode: 'auto' });
  assert.ok(doc.state.body(ft.id).isSolid);
  approx(vol(doc, ft.id), 4 / 3 * Math.PI * 1000, 0.05);
});

test('resize hole, offset, clearance cut', () => {
  const [doc, B] = block();
  doc.addFeature('hole', { points: [[20, 15, 20]], normal: [0, 0, 1], diameter: 5, through: true });
  const v0 = vol(doc, B);
  doc.addFeature('resizehole', { body: B, point: [22.5, 15, 10], diameter: 3.4 });
  approx(vol(doc, B) - v0, Math.PI * (2.5 ** 2 - 1.7 ** 2) * 20, 0.02);
  const d3 = new Document();
  const [c] = d3.addFeature('primitive', { shape: 'box', length: 20, width: 20, height: 20 });
  d3.addFeature('offset', { bodies: [c.id], distance: 0.5 });
  const [lo, hi] = d3.state.body(c.id).bbox();
  assert.ok(hi[0] - lo[0] >= 21 - 1e-6 && hi[0] - lo[0] <= 21.2);
  const d4 = new Document();
  const [a] = d4.addFeature('primitive', { shape: 'box', length: 30, width: 30, height: 10 });
  const [t] = d4.addFeature('primitive', { shape: 'cylinder', radius: 5, height: 20, offset: [0, 0, 2] });
  const peg = d4.state.body(t.id).solid;
  d4.addFeature('combine', { target: a.id, tools: [t.id], operation: 'cut', clearance: 0.3, keep_tools: true });
  assert.ok(d4.state.body(a.id).solid.intersect(peg).volume() < 1e-6);
});

test('project outline and text', async () => {
  const doc = new Document();
  const [f] = doc.addFeature('primitive', { shape: 'box', length: 40, width: 30, height: 20 });
  doc.addFeature('hole', { points: [[0, 0, 20]], normal: [0, 0, 1], diameter: 8, through: true });
  let k = 0;
  const ents = MT.outlineEntities(MT.outline([doc.state.body(f.id).solid], G.planeFrame([0, 0, 10], [0, 0, 1])), () => `p${k++}`);
  assert.deepEqual(ents.map(e => e.type).sort(), ['circle', 'polyline']);
  const cont = await MT.textContours('O', 'DejaVu Sans', 10);
  assert.equal(cont.length, 2);
  const [sk] = doc.addFeature('sketch', { plane: 'XY', entities: cont.map((c, i) => ({ id: `t${i}`, type: 'polyline', closed: true, pts: c })) });
  const [e] = doc.addFeature('extrude', { sketch: sk.id, distance: 2, operation: 'new' });
  assert.equal(doc.state.bodies.get(e.id).solid.genus(), 1);
});

test('save, reopen, export, undo', () => {
  const [doc, B] = block();
  doc.addFeature('hole', { points: [[20, 15, 20]], normal: [0, 0, 1], diameter: 6, through: true });
  const v1 = vol(doc, B);
  doc.undo(); approx(vol(doc, B), 24000); doc.redo(); approx(vol(doc, B), v1);
  const bytes = doc.saveBytes();
  const d2 = Document.fromBytes(bytes, 'p.stlx');
  approx(vol(d2, B), v1);
  for (const fmt of ['stl', '3mf', 'obj']) {
    const { bytes: out } = d2.exportBytes(fmt);
    const parts = IO.loadMeshFile(out, `x.${fmt}`);
    const man = G.manifoldFromArrays(parts[0].v, parts[0].f);
    approx(man.volume(), v1, 1e-3, fmt);
  }
});

test('memory: unused geometry is freed', () => {
  const [doc, B] = block();
  for (let i = 0; i < 5; i++) doc.preview('hole', { points: [[10 + i, 15, 20]], normal: [0, 0, 1], diameter: 4, through: true });
  const before = W.liveCount();
  W.sweep(doc.liveObjects());
  assert.ok(W.liveCount() < before, `${W.liveCount()} < ${before}`);
  approx(vol(doc, B), 24000);
});

test('project outline on a face', () => {
  const doc = new Document();
  const [f] = doc.addFeature('primitive', { shape: 'box', length: 40, width: 30, height: 20 });
  const polys = MT.outline([doc.state.body(f.id).solid], G.planeFrame([0, 0, 20], [0, 0, 1]));
  const xs = polys.flat().map(p => p[0]);
  approx(Math.max(...xs) - Math.min(...xs), 40, 1e-3);
});
