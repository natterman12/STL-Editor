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

// Ideal ISO metric profile radius at axial phase u (same profile and phase as printing.js).
function isoThreadRadius(u0, D, P) {
  const depth = 5 * 0.8660254 * P / 8, R = D / 2, a = P / 8, b = 7 * P / 16, c = 9 * P / 16, d = 7 * P / 8;
  const u = ((u0 % P) + P) % P;
  if (u < a || u >= d) return R - depth;
  if (u < b) return R - depth + depth * (u - a) / (b - a);
  if (u < c) return R;
  return R - depth * (u - c) / (d - c);
}

test('thread flanks smooth', async () => {
  // The thread surface follows the ISO profile along the helix (no jagged flanks).
  const PR = await load('engine/printing.js');
  const D = 8, P = 1.25, L = 12, n = 200000;
  let s2 = 0;
  for (let i = 0; i <= n; i++) s2 += (i === 0 || i === n ? 0.5 : 1) * isoThreadRadius(P * i / n, D, P) ** 2;
  const idealVol = L * Math.PI / P * s2 * P / n;
  for (const [hand, s] of [['right', 1], ['left', -1]]) {
    const rod = PR.threadTool('external', D, P, L, 0, hand, false);
    const m = rod.getMesh(), np = m.numProp, v = m.vertProperties, t = m.triVerts;
    let dev = 0;
    for (let i = 0; i < t.length; i += 3) {
      const c = [0, 1, 2].map(k => (v[np * t[i] + k] + v[np * t[i + 1] + k] + v[np * t[i + 2] + k]) / 3);
      if (c[2] <= 1.5 || c[2] >= L - 1.5) continue;
      const u = c[2] - s * Math.atan2(c[1], c[0]) / (2 * Math.PI) * P;
      dev = Math.max(dev, Math.abs(Math.hypot(c[0], c[1]) - isoThreadRadius(u, D, P)));
    }
    assert.ok(dev < 0.01, `${hand}-hand thread surface is jagged: ${dev.toFixed(4)} mm off the ISO profile`);
    approx(rod.volume(), idealVol, 0.01, hand);
  }
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

test('plate layout is hand-checkable and refuses what does not fit', () => {
  const near = (a, b) => assert.ok(Math.abs(a - b) < 1e-3, `${a} vs ${b}`);
  const box = (doc, l, w, h) => doc.addFeature('primitive', { shape: 'box', length: l, width: w, height: h })[0].id;
  const doc = new Document();
  const A = box(doc, 40, 30, 10), B = box(doc, 40, 30, 20), C = box(doc, 50, 20, 5);
  doc.addFeature('move', { bodies: [A], translate: [0, 0, 7] });
  doc.addFeature('plate', { bodies: [A, B, C], bed: [100, 100, 50], gap: 5 });
  // same layout as the Python engine's test: A and B share a row, C starts a row 5 mm above
  const expect = { [A]: [[-42.5, -27.5, 0], [-2.5, 2.5, 10]], [B]: [[2.5, -27.5, 0], [42.5, 2.5, 20]], [C]: [[-42.5, 7.5, 0], [7.5, 27.5, 5]] };
  for (const [bid, [lo, hi]] of Object.entries(expect)) {
    const [l, h] = doc.state.body(bid).bbox();
    for (let k = 0; k < 3; k++) { near(l[k], lo[k]); near(h[k], hi[k]); }
  }
  const d2 = new Document();
  const L = box(d2, 80, 20, 5);
  assert.throws(() => d2.addFeature('plate', { bodies: [L], bed: [50, 100, 50] }), /too big.*Rotate/);
  d2.addFeature('plate', { bodies: [L], bed: [50, 100, 50], rotate: true });
  const [l, h] = d2.state.body(L).bbox();
  near(l[0], -10); near(l[1], -40); near(h[0], 10); near(h[1], 40);
  const T = box(d2, 10, 10, 60);
  assert.throws(() => d2.addFeature('plate', { bodies: [T], bed: [100, 100, 50] }), /taller/);
  const C1 = box(d2, 60, 60, 5), C2 = box(d2, 60, 60, 5);
  assert.throws(() => d2.addFeature('plate', { bodies: [C1, C2], bed: [100, 100, 50] }), /Not everything fits/);
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

test('hollow keeps the outside and leaves an even wall', () => {
  const doc = new Document();
  const [f] = doc.importFile('cube.stl', stlOf(W.Manifold.cube([40, 40, 40])), true, 'keep');
  doc.addFeature('hollow', { body: f.id, thickness: 2, resolution: 'normal' });
  const info = doc.state.body(f.id).info();
  approx(info.volume, 40 ** 3 - 36 ** 3, 5e-3, 'hollow volume');   // the desktop engine gives 17371
  approx(info.size[0], 40, 1e-6);
  assert.equal(info.genus, -1, 'outer skin and cavity are two shells');
  assert.throws(() => doc.addFeature('hollow', { body: f.id, thickness: 25 }), /too large/);
});

test('shell opens the picked faces of a hollow', () => {
  // 20 mm cube, 2 mm wall: closed = 20^3 - 16^3; top open = 20^3 - 16*16*18; top+bottom = 20^3 - 16*16*20;
  // top + a side (no bar left along their shared edge) = 20^3 - 18*16*18
  const sq = [[[-10, -10], [10, -10], [10, 10], [-10, 10]]];
  const top = { origin: [10, 10, 20], normal: [0, 0, 1], xdir: [1, 0, 0], contours: sq };
  const bottom = { origin: [10, 10, 0], normal: [0, 0, -1], xdir: [1, 0, 0], contours: sq };
  const side = { origin: [20, 10, 10], normal: [1, 0, 0], contours: sq };   // +x side, shares an edge with the top
  const stl = stlOf(W.Manifold.cube([20, 20, 20]));
  for (const [faces, expected, genus] of [[[], 8000 - 16 ** 3, -1], [[top], 8000 - 16 * 16 * 18, 0], [[top, bottom], 8000 - 16 * 16 * 20, 1], [[top, side], 8000 - 18 * 16 * 18, 0]]) {
    const doc = new Document();
    const [f] = doc.importFile('cube.stl', stl, true, 'keep');
    doc.addFeature('shell', { body: f.id, faces, thickness: 2 });
    const info = doc.state.body(f.id).info();
    approx(info.volume, expected, 3e-3, `shell ${faces.length} faces`);
    assert.equal(info.genus, genus);
  }
  const doc = new Document();
  const [f] = doc.importFile('cube.stl', stl, true, 'keep');
  assert.throws(() => doc.addFeature('shell', { body: f.id, faces: [top], thickness: 12 }), /too large/);
  assert.throws(() => doc.addFeature('shell', { body: f.id, faces: [{ origin: [10, 10, 20], normal: [0, 0, 1], contours: [] }], thickness: 2 }), /flat/);
});

test('lattice fills a hollow with struts', () => {
  // 20 mm cube, 2 mm wall: cavity 16^3, cell 8 -> 12 struts 16 mm long. Skin 3904 + struts (~143
  // after crossings) = ~4038. The cavity is voxel-made, so this is a range check, not exact.
  const stl = stlOf(W.Manifold.cube([20, 20, 20]));
  const make = extra => {
    const doc = new Document();
    const [f] = doc.importFile('cube.stl', stl, true, 'keep');
    doc.addFeature('lattice', { body: f.id, cell: 8, strut: 1, thickness: 2, ...extra });
    return doc.state.body(f.id).info();
  };
  const v = make({}).volume;
  assert.ok(v > 3904 && v < 8000);
  approx(v, 4038, 0.011, 'lattice volume');
  assert.throws(() => make({ cell: 1, strut: 2 }), /thinner/);
  assert.throws(() => make({ thickness: 12 }), /too large/);
  assert.throws(() => make({ cell: 0 }), /above zero/);
});

test('repair rebuild closes a mesh with holes', () => {
  const sphere = W.Manifold.sphere(20, 64), m = sphere.getMesh();
  const f = Array.from(m.triVerts).filter((_, i) => Math.floor(i / 3) % 400 !== 7);   // punch small holes
  const doc = new Document();
  const [imp] = doc.importFile('holey.stl', IO.stlBytes([{ v: m.vertProperties, f: Uint32Array.from(f) }]), true, 'keep');
  const [, res] = doc.addFeature('repair', { body: imp.id, mode: 'rebuild', resolution: 'normal' });
  const b = doc.state.body(imp.id);
  assert.ok(b.isSolid, 'rebuilt body is a solid');
  approx(b.info().volume, 34350, 0.01, 'rebuilt volume (the desktop engine gives 34350; the true sphere is 33321)');
  assert.match(res.warning || '', /Rebuilt as a watertight solid/);
});

test('remesh evens out triangles and keeps sharp edges', () => {
  const doc = new Document();
  const [f] = doc.importFile('cube.stl', stlOf(W.Manifold.cube([20, 20, 20])), true, 'keep');
  doc.addFeature('remesh', { body: f.id, edge: 1 });
  const b = doc.state.body(f.id), { v, f: tris } = b.mesh();
  approx(b.info().volume, 8000, 1e-3, 'volume');
  const ls = [];
  for (let t = 0; t < tris.length; t += 3) for (let e = 0; e < 3; e++) { const a = tris[t + e] * 3, c = tris[t + (e + 1) % 3] * 3; ls.push(Math.hypot(v[a] - v[c], v[a + 1] - v[c + 1], v[a + 2] - v[c + 2])); }
  const mean = ls.reduce((s, x) => s + x, 0) / ls.length;
  assert.ok(mean > 0.85 && mean < 1.15, `mean edge ${mean}`);
  ls.sort((x, y) => x - y);
  assert.ok(ls[Math.floor(ls.length * 0.95)] < 1.4, 'no long edges');
});

test('sculpt brushes', () => {
  const doc = new Document();
  const [f] = doc.importFile('ball.stl', stlOf(W.Manifold.sphere(10, 48)), true, 'keep');
  doc.addFeature('remesh', { body: f.id, edge: 0.6 });
  const v0 = vol(doc, f.id);
  const stroke = mode => [{ mode, radius: 3, strength: 0.8, points: [[-7, 0, 7], [0, 0, 10], [7, 0, 7]] }];
  const [sc] = doc.addFeature('sculpt', { body: f.id, strokes: stroke('inflate') });
  assert.ok(vol(doc, f.id) > v0 + 5, 'inflate adds volume');
  doc.editFeature(sc.id, { body: f.id, strokes: stroke('deflate') });
  assert.ok(vol(doc, f.id) < v0 - 5, 'deflate removes volume');
  doc.editFeature(sc.id, { body: f.id, strokes: stroke('flatten') });
  assert.ok(vol(doc, f.id) < v0, 'flatten shaves the top');
  assert.throws(() => doc.addFeature('sculpt', { body: f.id, strokes: [] }), /Drag over the model/);
});

test('simplify matches the desktop and works on open meshes', () => {
  const sphere = W.Manifold.sphere(20, 96), m = sphere.getMesh();
  const doc = new Document();
  const [a] = doc.importFile('ball.stl', stlOf(sphere), true, 'keep');
  const n0 = doc.state.body(a.id).info().triangles;
  doc.addFeature('simplify', { body: a.id, ratio: 0.25 });
  const b = doc.state.body(a.id);
  assert.ok(b.isSolid);
  assert.equal(b.info().triangles, Math.floor(n0 * 0.25), 'hits the requested share exactly');
  approx(b.info().volume, sphere.volume(), 0.01);
  // an open mesh (one triangle removed): still simplified, and closed again afterwards
  const f = m.triVerts.slice(3);
  const [o] = doc.importFile('open.stl', IO.stlBytes([{ v: m.vertProperties, f }]), false, 'keep');
  doc.addFeature('simplify', { body: o.id, ratio: 0.5 });
  assert.ok(doc.state.body(o.id).info().triangles < f.length / 3 * 0.6);
});

test('repair touching shells, fins and inside-out meshes', () => {
  const ga = W.Manifold.cube([10, 10, 10]).getMesh(), gb = W.Manifold.cube([10, 10, 10]).translate([10, 0, 0]).getMesh();
  const v = new Float32Array([...ga.vertProperties, ...gb.vertProperties]);
  const f = new Uint32Array([...ga.triVerts, ...Array.from(gb.triVerts, i => i + ga.vertProperties.length / 3)]);
  approx(G.localRepair(v, f).volume(), 2000, 1e-6, 'two cubes sharing a wall');
  const s = W.Manifold.sphere(10, 32), sm = s.getMesh(), nv = sm.vertProperties.length / 3;
  const fin = G.localRepair(new Float32Array([...sm.vertProperties, 0, 0, 20]), new Uint32Array([...sm.triVerts, sm.triVerts[0], sm.triVerts[1], nv]));
  approx(fin.volume(), s.volume(), 1e-6, 'stray fin');
  const inv = Uint32Array.from(sm.triVerts); for (let i = 0; i < inv.length; i += 3) { const x = inv[i + 1]; inv[i + 1] = inv[i + 2]; inv[i + 2] = x; }
  approx(G.localRepair(sm.vertProperties, inv).volume(), s.volume(), 1e-6, 'inside out');
});

test('import PLY, OFF and glTF', () => {
  const g = W.Manifold.cube([10, 20, 30]).getMesh(), v = g.vertProperties, f = g.triVerts, nv = v.length / 3, nt = f.length / 3;
  const enc = s => new TextEncoder().encode(s).buffer;
  const vol = parts => G.manifoldFromArrays(parts[0].v, parts[0].f).volume();
  // OFF with quads
  const off = 'OFF\n# cube\n8 6 0\n0 0 0\n1 0 0\n1 1 0\n0 1 0\n0 0 1\n1 0 1\n1 1 1\n0 1 1\n4 0 3 2 1\n4 4 5 6 7\n4 0 1 5 4\n4 1 2 6 5\n4 2 3 7 6\n4 3 0 4 7\n';
  approx(vol(IO.loadMeshFile(enc(off), 'q.off')), 1, 1e-9, 'OFF');
  // PLY ascii and binary
  const head = fmt => `ply\nformat ${fmt} 1.0\nelement vertex ${nv}\nproperty float x\nproperty float y\nproperty float z\nelement face ${nt}\nproperty list uchar int vertex_indices\nend_header\n`;
  let txt = head('ascii');
  for (let i = 0; i < nv; i++) txt += `${v[i * 3]} ${v[i * 3 + 1]} ${v[i * 3 + 2]}\n`;
  for (let t = 0; t < nt; t++) txt += `3 ${f[t * 3]} ${f[t * 3 + 1]} ${f[t * 3 + 2]}\n`;
  approx(vol(IO.loadMeshFile(enc(txt), 'a.ply')), 6000, 1e-9, 'PLY ascii');
  const h = new TextEncoder().encode(head('binary_little_endian'));
  const bin = new DataView(new ArrayBuffer(h.length + nv * 12 + nt * 13));
  new Uint8Array(bin.buffer).set(h);
  let o = h.length;
  for (let i = 0; i < nv * 3; i++, o += 4) bin.setFloat32(o, v[i], true);
  for (let t = 0; t < nt; t++) { bin.setUint8(o++, 3); for (let k = 0; k < 3; k++, o += 4) bin.setInt32(o, f[t * 3 + k], true); }
  approx(vol(IO.loadMeshFile(bin.buffer, 'b.ply')), 6000, 1e-9, 'PLY binary');
  // GLB with two placed instances of the cube, one mirrored
  const pos = new Float32Array(v), idx = new Uint32Array(f);
  const binData = new Uint8Array(pos.byteLength + idx.byteLength); binData.set(new Uint8Array(pos.buffer)); binData.set(new Uint8Array(idx.buffer), pos.byteLength);
  const gltf = { asset: { version: '2.0' }, scene: 0, scenes: [{ nodes: [0, 1] }],
    nodes: [{ mesh: 0, translation: [100, 0, 0] }, { mesh: 0, scale: [-1, 1, 1] }],
    meshes: [{ primitives: [{ attributes: { POSITION: 0 }, indices: 1 }] }],
    buffers: [{ byteLength: binData.length }],
    bufferViews: [{ buffer: 0, byteOffset: 0, byteLength: pos.byteLength }, { buffer: 0, byteOffset: pos.byteLength, byteLength: idx.byteLength }],
    accessors: [{ bufferView: 0, componentType: 5126, count: nv, type: 'VEC3' }, { bufferView: 1, componentType: 5125, count: f.length, type: 'SCALAR' }] };
  let js = new TextEncoder().encode(JSON.stringify(gltf));
  const pad = n => (4 - n % 4) % 4, jl = js.length + pad(js.length), bl = binData.length + pad(binData.length);
  const glb = new Uint8Array(12 + 8 + jl + 8 + bl), d = new DataView(glb.buffer);
  d.setUint32(0, 0x46546C67, true); d.setUint32(4, 2, true); d.setUint32(8, glb.length, true);
  d.setUint32(12, jl, true); d.setUint32(16, 0x4E4F534A, true); glb.fill(32, 20, 20 + jl); glb.set(js, 20);
  d.setUint32(20 + jl, bl, true); d.setUint32(24 + jl, 0x004E4942, true); glb.set(binData, 28 + jl);
  const parts = IO.loadMeshFile(glb.buffer, 's.glb');
  assert.equal(parts.length, 2);
  for (const p of parts) approx(G.manifoldFromArrays(p.v, p.f).volume(), 6000, 1e-9, 'GLB part (mirrored one flipped back)');
  assert.ok(G.vertsBBox(parts[0].v)[0][0] < 0 && G.vertsBBox(parts[1].v)[0][0] >= 100, 'sorted by position');
});

test('Draw Walls: several walls, a multi-section line and a curve; preview beside the part', async () => {
  const SFX = await load('engine/supportfins.js');
  await SFX.load();
  assert.ok(SFX.info().curves, 'the curved-wall extra is built in');
  const doc = new Document();
  const ell = W.Manifold.union([W.Manifold.cube([10, 20, 35]), W.Manifold.cube([30, 20, 8]).translate([0, 0, 27])]);
  const [e] = doc.importFile('ell.stl', stlOf(ell), true, 'keep');
  const v0 = vol(doc, e.id);
  const walls = [
    { shape: 'line', points: [[17, 3, 27], [17, 17, 27]] },
    { shape: 'path', points: [[21, 2, 27], [26, 6, 27], [26, 14, 27]] },
    { shape: 'curve', points: [[28.5, 2, 27], [28.5, 18, 27], [24, 10, 27]] },
    { shape: 'line', points: [[22, 3, 27], [22.5, 3, 27]] },
  ];
  // the live preview: the walls as a body of their own next to the untouched part
  const [pst, pr] = doc.preview('supportwall', { body: e.id, walls, output: 'join', options: {} });
  assert.ok(pr.previewOnly && pr.created.length === 1, 'preview shows the walls only');
  approx(pst.body(e.id).info().volume, v0, 1e-9, 'part untouched in the preview');
  assert.deepEqual(pr.walls.map(w => w.ok), [true, true, true, false]);
  assert.match(pr.warning, /Wall 4: wall too short/);
  const [f, r] = doc.addFeature('supportwall', { body: e.id, walls, output: 'join', options: {} });
  assert.match(r.info, /3 walls/);
  assert.equal(doc.state.bodies.size, 1);
  const v1 = vol(doc, e.id);
  assert.ok(v1 > v0 + 600, `walls joined: ${v0} -> ${v1}`);
  // a curve built as one wall is not the same as its chord
  doc.editFeature(f.id, { body: e.id, walls: [walls[2]], output: 'separate', options: {} });
  const curved = [...doc.state.bodies.values()].at(-1).info().volume;
  doc.editFeature(f.id, { body: e.id, walls: [{ shape: 'line', points: [walls[2].points[0], walls[2].points[1]] }], output: 'separate', options: {} });
  const straight = [...doc.state.bodies.values()].at(-1).info().volume;
  assert.ok(Math.abs(curved - straight) > 1, `curve ${curved} vs line ${straight}`);
  // an older project's single wall still opens
  doc.editFeature(f.id, { body: e.id, points: [[22, 3, 27], [22, 17, 27]], output: 'join', options: {} });
  assert.match(doc._results.at(-1).info, /wall 14\.0 mm long/);
  console.log(`walls: ${v0} -> ${v1}, curve ${curved.toFixed(1)} vs chord ${straight.toFixed(1)}`);
});

test('Support Fins: remove a fin, add one by hand, each fin its own preview piece', async () => {
  const SFX = await load('engine/supportfins.js');
  await SFX.load();
  const doc = new Document();
  const [a] = doc.importFile('tip.stl', stlOf(W.Manifold.cube([30, 20, 10], true).rotate([0, 40, 0])), true, 'keep');
  const [, pr] = doc.preview('supportfins', { body: a.id, output: 'join', options: {} });
  const ids = Object.keys(pr.pieces);
  assert.deepEqual(Object.values(pr.pieces).map(p => p.kind), ['prop', 'prop', 'prop', 'pad']);
  const { kind, c, size } = pr.pieces[ids[1]];
  const [, pr2] = doc.preview('supportfins', { body: a.id, output: 'join', options: {}, removed: [{ kind, c, size }] });
  assert.deepEqual(pr2.ghosts, [ids[1]]);
  assert.match(pr2.info, /1 removed/);
  // a slightly different build (gap 0.25) still finds it
  const [, pr3] = doc.preview('supportfins', { body: a.id, output: 'join', options: { gap: 0.25 }, removed: [{ kind, c, size }] });
  assert.equal(pr3.ghosts.length, 1);
  const [f] = doc.addFeature('supportfins', { body: a.id, output: 'join', options: {}, removed: [{ kind, c, size }] });
  approx(vol(doc, a.id), 6543.19, 1e-3, 'one fin fewer');   // the desktop engine gives the same
  // a removal that matches nothing removes nothing
  doc.editFeature(f.id, { body: a.id, output: 'join', options: {}, removed: [{ kind: 'prop', c: [50, 50, 50], size }] });
  approx(vol(doc, a.id), 6796, 2e-3);
  assert.match(doc._results.at(-1).warning, /aren't there any more/);
  // walls by hand next to the auto fins, on the L
  const ell = W.Manifold.union([W.Manifold.cube([10, 20, 35]), W.Manifold.cube([30, 20, 8]).translate([0, 0, 27])]).translate([0, 100, 0]);
  const [e] = doc.importFile('ell.stl', stlOf(ell), true, 'keep');
  const [, ph] = doc.preview('supportfins', { body: e.id, output: 'join', options: {}, walls: [{ shape: 'line', points: [[24, 104, 27], [24, 116, 27]] }] });
  assert.match(ph.info, /1 drawn by hand/);
  assert.ok(Object.values(ph.pieces).some(p => p.kind === 'hand'));
});

test('Parameters: expressions in any number, rebuilt when a value changes', async () => {
  const E = await load('js/expr.js');
  assert.equal(E.evaluate('=2*(3+4)^2 - sqrt(16) + sin(30) + max(1,5,3) % 3', {}), 96.5);
  assert.equal(E.evaluate('round(2.5) + floor(-1.5) + atan(1)', {}), 3 - 2 + 45);
  const r = E.resolveParams([{ name: 'b', expr: 'a*2' }, { name: 'a', expr: '10' }, { name: 'c', expr: 'c+1' }, { name: '9x', expr: '1' }]);
  assert.deepEqual({ ...r.values }, { a: 10, b: 20 });
  assert.match(r.errors.c, /depends on itself/);
  assert.match(r.errors['9x'], /isn't a name/);
  assert.throws(() => E.evaluate('=w+', { w: 1 }), /ends too soon/);
  const doc = new Document();
  doc.setParams([{ name: 'w', expr: '40' }, { name: 'h', expr: 'w/4 + 5' }]);
  doc.addFeature('primitive', { shape: 'box', length: '=w', width: 20, height: '=h*2', plane: 'XY', offset: [0, 0, '=h'], rotate: [0, 0, 0], anchor: 'base', operation: 'new', targets: [], body_name: 'Box' });
  const size = () => { const [lo, hi] = [...doc.state.bodies.values()][0].bbox(); return hi.map((x, i) => +(x - lo[i]).toFixed(4)).concat(+lo[2].toFixed(4)); };
  assert.deepEqual(size(), [40, 20, 30, 15]);
  doc.setParams([{ name: 'w', expr: '60' }, { name: 'h', expr: 'w/4 + 5' }]);
  assert.deepEqual(size(), [60, 20, 40, 20]);
  doc.undo();
  assert.deepEqual(size(), [40, 20, 30, 15]);
  // a project keeps them
  const back = Document.fromBytes(doc.saveBytes(), 'p.stlx');
  assert.deepEqual(back.params.map(q => q.name), ['w', 'h']);
  doc.setParams([{ name: 'w', expr: '40' }]);
  assert.match(doc._results.at(-1).error, /no parameter "h"/);
  assert.throws(() => doc.setParams([{ name: 'sin', expr: '1' }]), /built-in/);
});

test('Support Fins extras: suggested poses, strength, gussets and anchor fins', async () => {
  const SFX = await load('engine/supportfins.js');
  await SFX.load();
  // a Support Fins release that broke an extra is vendored without it; what is there has to work
  const have = JSON.parse(fs.readFileSync(path.join(web, 'vendor/support-fins/VERSION.json'), 'utf8')).features || [];
  const feats = SFX.info().features;
  assert.deepEqual(Object.keys(feats).filter(k => feats[k]).sort(), [...have].sort());
  const doc = new Document();
  const ell = W.Manifold.union([W.Manifold.cube([10, 20, 35]), W.Manifold.cube([30, 20, 8]).translate([0, 0, 27])]);
  const [e] = doc.importFile('ell.stl', stlOf(ell), true, 'keep');
  const mesh = doc.state.body(e.id).mesh();
  if (feats.orient) {
    const sg = SFX.suggest(mesh, {}, 3);
    assert.equal(sg.candidates.length, 3);
    assert.equal(sg.candidates[0].rot.length, 9);
    assert.equal(Math.round(sg.candidates[0].overArea), 0, 'lying down needs nothing');
    const st = SFX.strength(mesh, {}, [0, 0, 1]);
    assert.equal(st.posture, 'weak');
    assert.equal(st.load.quality, 'poor');
    assert.equal(st.strongest.quality, 'good');
  }
  const walls = [...(feats.gussets ? [{ shape: 'gusset', points: [[10, 10, 8], [18, 10, 27]], normal: [1, 0, 0] }] : []),
    ...(feats.anchors ? [{ shape: 'anchor', points: [[10, 10, 15]], normal: [1, 0, 0] }] : [])];
  const [, pr] = doc.preview('supportfins', { body: e.id, output: 'join', options: {}, walls });
  assert.ok((pr.walls || []).every(w => w.ok));
  if (walls.length === 2) {
    assert.match(pr.info, /2 drawn by hand/);
    approx(pr.support.volume, 2014.4, 2e-3, 'support plastic');   // the desktop engine gives the same
  }
});

test('Move: a rotated part lands on the bed', () => {
  const [doc, B] = block();
  doc.addFeature('move', { bodies: [B], translate: [0, 0, 0], rotate: [30, 0, 0], pivot: 'center', bed: true });
  approx(doc.state.body(B).bbox()[0][2] + 1, 1, 1e-6, 'lowest point on the bed');
  doc.addFeature('move', { bodies: [B], translate: [0, 0, 5], rotate: [0, 20, 0], pivot: 'center', bed: true });
  approx(doc.state.body(B).bbox()[0][2], 5, 1e-6, 'Move Z lifts it off the bed');
  const [doc2, B2] = block();
  doc2.addFeature('move', { bodies: [B2], translate: [0, 0, 0], rotate: [30, 0, 0], pivot: 'center' });
  assert.ok(doc2.state.body(B2).bbox()[0][2] < -1, 'older projects (no bed) keep turning about the centre');
});

test('Support Fins: fins, a drawn wall, separate body', async () => {
  const SFX = await load('engine/supportfins.js');
  await SFX.load();
  const inf = SFX.info();
  assert.equal(inf.schema.version, 1);
  assert.ok(inf.schema.options.some(o => o.key === 'tineDensity'), 'settings come from Support Fins');
  const doc = new Document();
  const [a] = doc.importFile('tip.stl', stlOf(W.Manifold.cube([30, 20, 10], true).rotate([0, 40, 0])), true, 'keep');
  const [f, r] = doc.addFeature('supportfins', { body: a.id, output: 'join', options: {} });
  assert.match(r.info, /3 walls, 15 tines/);
  assert.ok(r.paint.over.length > 0, 'overhang faces reported');
  approx(vol(doc, a.id), 6796, 2e-3, 'part + fins');   // the desktop engine gives the same
  doc.editFeature(f.id, { body: a.id, output: 'separate', options: { tineDensity: 1 } });
  assert.equal(doc.state.bodies.size, 2);
  approx(vol(doc, a.id), 6000, 1e-6, 'part untouched');
  assert.match(doc._results.at(-1).info, /30 tines/);
  assert.throws(() => doc.addFeature('supportfins', { body: a.id, options: { tineDensity: 5 } }), /tineDensity/);
  // nothing to hold up on a block lying flat
  const [flat] = doc.importFile('flat.stl', stlOf(W.Manifold.cube([30, 20, 10]).translate([100, 0, 0])), true, 'keep');
  const [, r2] = doc.addFeature('supportfins', { body: flat.id, options: {} });
  assert.match(r2.warning, /no overhangs/);
  // a hand-drawn wall under the arm of an upside-down L
  const ell = W.Manifold.union([W.Manifold.cube([10, 20, 35]), W.Manifold.cube([30, 20, 8]).translate([0, 0, 27])]).translate([0, 100, 0]);
  const [e] = doc.importFile('ell.stl', stlOf(ell), true, 'keep');
  const ve = vol(doc, e.id);
  const [, rw] = doc.addFeature('supportwall', { body: e.id, points: [[22, 103, 27], [22, 117, 27]], output: 'join', options: {} });
  assert.match(rw.info, /wall 14\.0 mm long/);
  assert.ok(vol(doc, e.id) > ve + 100);
});

test('Sketch: part edges to dimension from (face, cut, shadow)', () => {
  const doc = new Document();
  const [f] = doc.addFeature('primitive', { shape: 'box', length: 40, width: 30, height: 20 });
  doc.addFeature('hole', { points: [[0, 0, 20]], normal: [0, 0, 1], diameter: 8, through: true });
  const solid = doc.state.body(f.id).solid;
  for (const [z, kind] of [[20, 'face'], [10, 'section'], [-5, 'silhouette']]) {
    const r = MT.refGeometry([solid], G.planeFrame([0, 0, z], [0, 0, 1]));
    assert.equal(r.kind, kind);
    assert.deepEqual(r.refs.map(e => e.type).sort(), ['circle', 'line', 'line', 'line', 'line']);
    approx(r.refs.find(e => e.type === 'circle').r, 4, 0.005);
    const xs = r.refs.filter(e => e.type === 'line').flatMap(e => [e.p1[0], e.p2[0]]);
    approx(Math.max(...xs) - Math.min(...xs), 40, 1e-4);
  }
});

test('Sketch: Smart Dimension, how defined a sketch is, reference dimensions', async () => {
  const C = await load('js/constraints.js');
  const D = await load('js/dims.js');
  const fixed = [{ id: '_o', type: 'point', x: 0, y: 0 }, { id: '_h', type: 'line', p1: [-1e5, 0], p2: [1e5, 0] }, { id: 'r0', type: 'line', p1: [-5, -50], p2: [-5, 50] }];
  const F = new Set(fixed.map(e => e.id));
  let ents = [{ id: 'r', type: 'rect', x: 2, y: 3, w: 20, h: 10 }];
  const all = () => [...ents, ...fixed];
  // a rectangle side: horizontal size; with the pointer beside it, vertical
  const w = C.inferDimension([{ e: 'r', side: 0 }], all(), [10, 0], F);
  assert.equal(w.type, 'hdistance');
  assert.equal(C.inferDimension([{ e: 'r', side: 1 }], all(), [30, 8], F).type, 'vdistance');
  // a corner and a part edge: the distance to it; two fixed things: refused
  const d = C.inferDimension([{ e: 'r', k: 'c0' }, { e: 'r0' }], all(), [0, 0], F);
  assert.equal(d.type, 'distance');
  assert.equal(typeof C.inferDimension([{ e: '_o', k: 'p' }, { e: 'r0' }], all(), [0, 0], F), 'string');
  let cons = [{ type: 'coincident', a: { e: 'r', k: 'c0' }, b: { e: '_o', k: 'p' } }];
  assert.equal(C.analyze(ents, cons, fixed).dof, 2);
  cons.push({ ...w, value: 30 }, { ...C.inferDimension([{ e: 'r', side: 1 }], all(), [30, 8], F), value: 12 });
  const r = C.solve(ents, cons, [], fixed);
  assert.ok(r.ok);
  ents = r.entities;
  assert.ok(Math.abs(ents[0].x) < 1e-6 && Math.abs(ents[0].y) < 1e-6);
  approx(ents[0].w, 30, 1e-6); approx(ents[0].h, 12, 1e-6);
  const a = C.analyze(ents, cons, fixed);
  assert.equal(a.dof, 0); assert.equal(a.under.size, 0);
  // the same size again adds nothing: it can only be a reference dimension
  assert.ok(C.isRedundant(ents, cons, { ...w, value: 30 }, fixed));
  assert.ok(C.solve(ents, [...cons, { ...w, value: 31, ref: true }], [], fixed).ok, 'reference dimensions drive nothing');
  // two lines: the angle of the sector the pointer is in
  const L = [{ id: 'a', type: 'line', p1: [0, 0], p2: [10, 0] }, { id: 'b', type: 'line', p1: [0, 0], p2: [10, 10] }];
  const ang = C.inferDimension([{ e: 'a' }, { e: 'b' }], L, [5, 2]);
  approx(C.measure(ang, L), 45, 1e-9);
  approx(C.measure(C.angleTo(ang, Object.fromEntries(L.map(e => [e.id, e])), [-5, 2]), L), 135, 1e-9);
  // the drawing: arrows, extension lines and the value where it was placed
  const E = Object.fromEntries(all().map(e => [e.id, e]));
  const c = { ...w, value: 30 };
  D.placeAt(c, E, [15, -6]);
  const lay = D.layout(c, E, 0.1);
  assert.equal(lay.arrows.length, 2);
  assert.equal(lay.thin.length, 2);
  approx(lay.text[1], -6, 1e-9);
});

test('Support Fins coverage: held near a fin or a wall going down, bare beyond reach', async () => {
  const C = await load('js/sfcoverage.js');
  // a 30 x 20 mm shelf underside at z = 10 (two triangles, facing down), with a wall going
  // down from its x = 0 edge
  const pos = new Float32Array([0, 0, 10, 30, 0, 10, 30, 20, 10, 0, 20, 10, 0, 0, 0, 0, 20, 0]);
  const idx = new Uint32Array([0, 2, 1, 0, 3, 2, 0, 4, 5, 0, 5, 3]);
  const soup = (lo, hi) => {   // a box as triangle soup (only its points matter)
    const P = [];
    for (const x of [lo[0], hi[0]]) for (const y of [lo[1], hi[1]]) for (const z of [lo[2], hi[2]]) P.push([x, y, z]);
    const out = [];
    for (let i = 0; i + 2 < 8; i++) out.push(...P[i], ...P[i + 1], ...P[i + 2]);
    return new Float32Array(out);
  };
  const alone = C.coverage({ pos, idx, over: [0, 1], fins: [], reach: 5 });
  approx(alone.total, 600, 1e-3);
  approx(alone.held / alone.total, 5 / 30, 0.15, 'the wall holds a 5 mm strip');
  assert.ok(alone.spots.length >= 1 && (alone.spots[0].far ?? Infinity) > 5);
  // a fin along y under x = 20, its top 0.2 mm below the shelf
  const fin = soup([19.6, 0, 0], [20.4, 20, 9.8]);
  const withFin = C.coverage({ pos, idx, over: [0, 1], fins: [fin], reach: 5 });
  assert.ok(withFin.held > alone.held * 2.5, `${withFin.held} vs ${alone.held}`);
  // held: x 0..5 (the wall) and 15..25 (the fin); bare: 5..15 and 25..30
  approx(withFin.held / withFin.total, 0.5, 0.1, 'half held');
  assert.equal(withFin.spots.length, 2);
  assert.ok(withFin.spots.every(s => s.far > 8.5 && s.far < 10.5), JSON.stringify(withFin.spots.map(s => s.far)));
  assert.equal(withFin.dist.length * 3, withFin.tris.length);
  // a bare spot gets a wall across it, its ends on the shelf (x 5..30 bare)
  const [e0, e1] = alone.spots[0].ends;
  assert.ok(Math.abs(e1[0] - e0[0]) > 15 && e0[2] === 10 && e1[2] === 10, JSON.stringify(alone.spots[0].ends));
});

test('DXF import: lines, circles, arcs and bulged polylines become sketch shapes', async () => {
  const I = await load('js/import2d.js');
  const dxf = ['0', 'SECTION', '2', 'ENTITIES',
    '0', 'LINE', '10', '0', '20', '0', '11', '10', '21', '0',
    '0', 'CIRCLE', '10', '5', '20', '5', '40', '2',
    '0', 'ARC', '10', '0', '20', '0', '40', '3', '50', '0', '51', '90',
    '0', 'LWPOLYLINE', '70', '1', '10', '0', '20', '0', '42', '1', '10', '4', '20', '0', '10', '4', '20', '4',
    '0', 'ENDSEC', '0', 'EOF'].join('\n');
  const [l, c, a, p] = I.parseDXF(dxf);
  assert.deepEqual([l.type, l.p1, l.p2], ['line', [0, 0], [10, 0]]);
  assert.deepEqual([c.type, c.cx, c.r], ['circle', 5, 2]);
  assert.deepEqual([a.type, a.r, a.a1], ['arc', 3, 90]);
  // the bulge 1 from (0,0) to (4,0) is a half circle of radius 2 through (2,-2)
  assert.equal(p.type, 'polyline');
  assert.ok(p.closed && p.pts.some(q => q[1] < -1.9 && Math.abs(Math.hypot(q[0] - 2, q[1]) - 2) < 1e-9), JSON.stringify(p));
});

test('Opening a file: a failed open keeps the open project; an opened mesh has nothing to undo', async () => {
  const R = await load('engine/routes.js');
  const good = stlOf(W.Manifold.cube([10, 10, 10]));
  R.ROUTES.open_bytes({ name: 'a.stl', data: good.buffer.slice(0) });
  assert.equal(R.app.doc.features.length, 1);
  assert.equal(R.app.doc._undo.length, 0, 'Undo right after opening would empty the project');
  R.ROUTES.undo();
  assert.equal(R.app.doc.features.length, 1);
  R.app.doc.addFeature('primitive', { shape: 'box', length: 5, width: 5, height: 5, plane: 'XY', offset: [30, 0, 0], rotate: [0, 0, 0], anchor: 'base', operation: 'new', targets: [], body_name: 'Box' });
  const before = R.app.doc;
  assert.throws(() => R.ROUTES.open_bytes({ name: 'broken.stl', data: new Uint8Array([1, 2, 3]).buffer }));
  assert.throws(() => R.ROUTES.open_bytes({ name: 'broken.stlx', data: new Uint8Array([1, 2, 3]).buffer }), /isn't an STL Editor project/);
  assert.equal(R.app.doc, before, 'still the same project');
  assert.equal(R.app.doc.features.length, 2);
});

test('Autosave snapshot: nothing is zipped when the project is saved or unchanged; parameters count', async () => {
  const R = await load('engine/routes.js');
  R.ROUTES.open_bytes({ name: 'a.stl', data: stlOf(W.Manifold.cube([10, 10, 10])).buffer.slice(0) });
  assert.deepEqual(R.ROUTES.snapshot({}), { ok: true, dirty: false }, 'opened and not changed: nothing to keep');
  R.app.doc.setParams([{ name: 'w', expr: '4' }]);
  const s1 = R.ROUTES.snapshot({}).result;
  assert.ok(s1.bytes.length > 100 && s1.dirty);
  assert.equal(s1.key.length, 32);
  const same = R.ROUTES.snapshot({ last: s1.key });
  assert.ok(same.same && !same.result && !same.bytes, 'unchanged: no zip work');
  R.app.doc.setParams([{ name: 'w', expr: '5' }]);   // a parameter-only change is a change
  const s2 = R.ROUTES.snapshot({ last: s1.key }).result;
  assert.ok(s2.bytes && s2.key !== s1.key);
});

test('Parameters: functions take the right number of numbers; built-in object names are ordinary names', async () => {
  const E = await load('js/expr.js');
  assert.throws(() => E.evaluate('=round(2.5, 1)', {}), /^Error: round\(\) takes 1 number$|round\(\) takes 1 number/);
  assert.throws(() => E.evaluate('=sqrt(4, 2)', {}), /sqrt\(\) takes 1 number/);
  assert.throws(() => E.evaluate('=sin()', {}), /sin\(\) needs a number/);
  assert.equal(E.evaluate('=max(1, 7, 3) + min(4)', {}), 11);
  assert.throws(() => E.evaluate('=constructor(1)', {}), /no function "constructor"/);
  assert.throws(() => E.evaluate('=toString + 1', {}), /no parameter "toString"/);
  const r = E.resolveParams([{ name: 'constructor', expr: '3' }, { name: 'b', expr: 'constructor * 2' }, { name: 'toString', expr: 'nope' }]);
  assert.equal(r.values.constructor, 3);
  assert.equal(r.values.b, 6);
  assert.equal(r.values.toString, undefined);
  assert.match(r.errors.toString, /no parameter "nope"/);
  assert.equal(E.resolveDeep('=constructor + 1', r.values), 4);
  assert.throws(() => E.resolveDeep('=hasOwnProperty', r.values), /no parameter "hasOwnProperty"/);
});

test('Cache keys stay the same size however long the timeline is', () => {
  const [doc, B] = block();
  for (let i = 0; i < 6; i++) doc.addFeature('hole', { points: [[5 + 5 * i, 15, 20]], normal: [0, 0, 1], diameter: 2, through: true });
  assert.equal(doc._cache.length, 7);
  for (const [h] of doc._cache) assert.equal(h.length, 32);
  assert.equal(new Set(doc._cache.map(c => c[0])).size, 7);
  assert.equal(Document.digest('abc'), Document.digest('abc'));
  assert.notEqual(Document.digest('abc'), Document.digest('abd'));
  approx(vol(doc, B), 40 * 30 * 20 - 6 * Math.PI * 20, 0.02);
});

test('Pattern of features: no tool records of the copies are left behind', () => {
  const [doc] = block();
  const [h] = doc.addFeature('hole', { points: [[5, 15, 20]], normal: [0, 0, 1], diameter: 4, through: true });
  const [p] = doc.addFeature('pattern', { source: 'features', features: [h.id], kind: 'linear', direction: [0, 1, 0], count: 3, spacing: 5 });
  assert.deepEqual([...doc.state.tools.keys()].filter(k => k.startsWith(p.id + '.')), []);
});

test('Big meshes: repair and hole measuring work without spreading huge arrays', () => {
  // a finely divided cube: over 400k vertex coordinates in one shell
  const cube = W.Manifold.cube([10, 10, 10]).refine(150).getMesh();
  assert.ok(cube.vertProperties.length > 400000);
  const r = G.quickRepair(cube.vertProperties, cube.triVerts);
  approx(r.volume(), 1000, 1e-6);
  // a finely divided round pin: well over 100k points on its wall
  const pin = W.Manifold.cylinder(10, 5, 5, 720).refine(12);
  const m = pin.getMesh();
  const pv = m.vertProperties, tv = m.triVerts;
  let wall = -1;
  for (let t = 0; t < tv.length / 3 && wall < 0; t++) {
    const zs = [0, 1, 2].map(k => pv[tv[t * 3 + k] * 3 + 2]);
    if (Math.max(...zs) - Math.min(...zs) > 0.5) wall = t;
  }
  const c = MT.fitCylinder(pv, tv, wall);
  assert.equal(c.kind, 'pin');
  approx(c.radius, 5, 1e-3);
  approx(c.z1 - c.z0, 10, 1e-6);
});

test('Face click: only the connected flat face is picked, in triangle order', () => {
  // two separate blocks of the same height: clicking one top must not pick the other's
  const two = W.Manifold.union([W.Manifold.cube([10, 10, 5]), W.Manifold.cube([10, 10, 5]).translate([20, 0, 0])]).refine(3);
  const m = two.getMesh();
  const v = m.vertProperties, f = m.triVerts;
  let top = -1;
  for (let t = 0; t < f.length / 3 && top < 0; t++) if ([0, 1, 2].every(k => v[f[t * 3 + k] * 3 + 2] > 4.999) && v[f[t * 3] * 3] < 10.001) top = t;
  const { region, normal } = G.faceRegion(v, f, top);
  approx(normal[2], 1);
  const area = region.reduce((s, t) => {
    const p = [0, 1, 2].map(k => [v[f[t * 3 + k] * 3], v[f[t * 3 + k] * 3 + 1]]);
    return s + Math.abs((p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1])) / 2;
  }, 0);
  approx(area, 100, 1e-6, 'one top face');
  assert.deepEqual(region, [...region].sort((a, b) => a - b));
});

const shoelace = cs => Math.abs(cs.reduce((s, c) => s + c.reduce((a, p, i) => { const q = c[(i + 1) % c.length]; return a + (p[0] * q[1] - q[0] * p[1]) / 2; }, 0), 0));
const unitNormals = (v, f) => G.triNormals(v, f).n;

test('Face pick: a cylinder side is not flat; its top and a box top are', async () => {
  const R = await load('engine/routes.js');
  R.ROUTES.new({ force: true });
  R.app.doc.addFeature('primitive', { shape: 'cylinder', radius: 10, height: 20, plane: 'XY' });
  R.app.doc.addFeature('primitive', { shape: 'box', length: 20, width: 20, height: 20, plane: 'XY', offset: [40, 0, 0] });
  const [cyl, box] = [...R.app.doc.state.bodies.keys()];
  const pick = (bid, want) => {
    const { v, f } = R.app.doc.state.body(bid).mesh();
    const n = unitNormals(v, f);
    let tri = -1;
    for (let t = 0; t < f.length / 3 && tri < 0; t++) if (want(n[t * 3], n[t * 3 + 1], n[t * 3 + 2])) tri = t;
    return R.ROUTES.face({ body: bid, tri, outline: true });
  };
  assert.equal(pick(cyl, (x, y, z) => Math.abs(z) < 0.1).planar, false, 'cylinder side');
  const top = pick(cyl, (x, y, z) => z > 0.99);
  assert.equal(top.planar, true);
  approx(shoelace(top.contours), 0.5 * 96 * 100 * Math.sin(2 * Math.PI / 96), 1e-6);
  assert.ok(Math.abs(shoelace(top.contours) / (Math.PI * 100) - 1) < 5e-3);
  const btop = pick(box, (x, y, z) => z > 0.99);
  assert.equal(btop.planar, true);
  approx(shoelace(btop.contours), 400, 1e-6);
  assert.equal(pick(box, x => x > 0.99).planar, true);
});

test('Face pick: flat faces of fine, holed and rounded meshes stay flat; sphere facets are not', () => {
  const topOf = man => {
    const m = man.getMesh(), v = m.vertProperties, f = m.triVerts, n = unitNormals(v, f);
    let tri = -1;
    for (let t = 0; t < f.length / 3 && tri < 0; t++) if (n[t * 3 + 2] > 0.9999) tri = t;
    const { region, normal } = G.faceRegion(v, f, tri);
    return G.facePlanar(v, f, region, normal);
  };
  const M = W.Manifold;
  const corners = [];
  for (const x of [0, 20]) for (const y of [0, 20]) for (const z of [0, 10]) corners.push(M.sphere(3, 48).translate([x, y, z]));
  assert.equal(topOf(M.cube([20, 20, 20]).refine(8)), true, 'finely tessellated');
  assert.equal(topOf(M.cube([20, 20, 20]).subtract(M.cylinder(30, 4, 4, 64).translate([10, 10, -5]))), true, 'with a hole');
  assert.equal(topOf(M.hull(corners)), true, 'blends into rounded edges');
  const m = M.sphere(10, 48).getMesh(), v = m.vertProperties, f = m.triVerts;
  for (const tri of [0, Math.floor(f.length / 6)]) {
    const { region, normal } = G.faceRegion(v, f, tri);
    assert.equal(G.facePlanar(v, f, region, normal), false, `sphere facet ${tri}`);
  }
});

// ---- loft and sweep (same hand-worked volumes as tests/test_engine.py)
const lrect = (id, x, y, w, h) => ({ id, type: 'rect', x, y, w, h });
const lsk = (doc, origin, entities, normal = [0, 0, 1]) => doc.addFeature('sketch', { plane: { origin, normal }, entities })[0].id;

test('loft: frustum, offset top, and a middle profile all give 1040 mm³', () => {
  let doc = new Document();
  let a = lsk(doc, [0, 0, 0], [lrect('a', -5, -5, 10, 10)]);
  let b = lsk(doc, [0, 0, 20], [lrect('b', -2, -2, 4, 4)]);
  let [f] = doc.addFeature('loft', { profiles: [a, b] });
  approx(vol(doc, f.id), 1040, 1e-3, 'frustum');

  doc = new Document();
  a = lsk(doc, [0, 0, 0], [lrect('a', -5, -5, 10, 10)]);
  b = lsk(doc, [0, 0, 20], [lrect('b', 1, 1, 4, 4)]);
  [f] = doc.addFeature('loft', { profiles: [a, b] });
  approx(vol(doc, f.id), 1040, 1e-3, 'offset top');

  doc = new Document();
  a = lsk(doc, [0, 0, 0], [lrect('a', -5, -5, 10, 10)]);
  const m = lsk(doc, [0, 0, 10], [lrect('m', -2, -2, 4, 4)]);
  const c = lsk(doc, [0, 0, 20], [lrect('c', -5, -5, 10, 10)]);
  [f] = doc.addFeature('loft', { profiles: [a, m, c] });
  approx(vol(doc, f.id), 1040, 1e-3, 'middle profile');
});

test('loft and sweep refuse bad input in plain words', () => {
  const doc = new Document();
  const a = lsk(doc, [0, 0, 0], [lrect('a', -5, -5, 10, 10)]);
  assert.throws(() => doc.addFeature('loft', { profiles: [a] }), /at least two/);
  const open = lsk(doc, [0, 0, 20], [{ id: 'l', type: 'line', p1: [0, 0], p2: [5, 0] }]);
  assert.throws(() => doc.addFeature('loft', { profiles: [a, open] }), /closed shape/);
  const crossing = lsk(doc, [0, 0, 0], [{ id: 'pl', type: 'polyline', pts: [[0, 0], [10, 10], [10, 0], [0, 10]], closed: false }]);
  const prof = lsk(doc, [0, 0, 0], [lrect('p', -1, -1, 2, 2)]);
  assert.throws(() => doc.addFeature('sweep', { profile: prof, path: crossing }), /crosses itself/);
});

test('sweep: straight 40 mm³, two collinear legs 80, and a mitred L 80', () => {
  const doc = new Document();
  const prof = lsk(doc, [0, 0, 0], [lrect('p', -1, -1, 2, 2)]);
  let path = lsk(doc, [0, 0, 0], [{ id: 'l', type: 'line', p1: [0, 0], p2: [10, 0] }]);
  let [f] = doc.addFeature('sweep', { profile: prof, path });
  approx(vol(doc, f.id), 40, 1e-3, 'straight');
  path = lsk(doc, [0, 0, 0], [{ id: 'l1', type: 'line', p1: [0, 0], p2: [10, 0] }, { id: 'l2', type: 'line', p1: [10, 0], p2: [20, 0] }]);
  [f] = doc.addFeature('sweep', { profile: prof, path });
  approx(vol(doc, f.id), 80, 1e-3, 'collinear');
  path = lsk(doc, [0, 0, 0], [{ id: 'l1', type: 'line', p1: [0, 0], p2: [10, 0] }, { id: 'l2', type: 'line', p1: [10, 0], p2: [10, 10] }]);
  [f] = doc.addFeature('sweep', { profile: prof, path });
  approx(vol(doc, f.id), 80, 1e-3, 'mitred L');
});

test('sweep: three collinear legs 120 mm³, and a profile drawn off-origin stays on the path start', () => {
  const doc = new Document();
  const prof = lsk(doc, [0, 0, 0], [lrect('p', -1, -1, 2, 2)]);
  let path = lsk(doc, [0, 0, 0], [{ id: 'a', type: 'line', p1: [0, 0], p2: [10, 0] }, { id: 'b', type: 'line', p1: [10, 0], p2: [20, 0] }, { id: 'c', type: 'line', p1: [20, 0], p2: [30, 0] }]);
  let [f] = doc.addFeature('sweep', { profile: prof, path });
  approx(vol(doc, f.id), 120, 1e-3, 'collinear');
  const off = lsk(doc, [0, 0, 0], [lrect('q', 19, -1, 2, 2)]);
  path = lsk(doc, [0, 0, 0], [{ id: 'd', type: 'line', p1: [20, 0], p2: [20, 10] }]);
  [f] = doc.addFeature('sweep', { profile: off, path });
  const [lo, hi] = doc.state.body(f.id).bbox();
  approx((lo[0] + hi[0]) / 2, 20, 1e-3, 'centre x');
  approx(vol(doc, f.id), 40, 1e-3, 'offset volume');
});

test('STEP export: a unit cube is 12 flat faces in a closed solid, and parses back', async () => {
  const ST = await load('engine/step.js');
  const m = W.Manifold.cube([10, 10, 10]).getMesh();
  const bytes = ST.stepBytes([{ v: m.vertProperties, f: m.triVerts, name: 'cube' }], { title: 'cube', date: '2026-11-09T00:00:00' });
  const text = new TextDecoder().decode(bytes);
  // header
  assert.ok(text.startsWith('ISO-10303-21;\nHEADER;\n'), 'starts with the ISO header');
  assert.ok(text.includes("FILE_SCHEMA(('AUTOMOTIVE_DESIGN"), 'names the AP214 schema');
  assert.ok(/\nENDSEC;\nDATA;\n/.test(text) && text.trimEnd().endsWith('END-ISO-10303-21;'), 'DATA section and end line');
  // one ADVANCED_FACE per triangle, one closed shell, one solid
  const count = re => (text.match(re) || []).length;
  assert.equal(count(/=ADVANCED_FACE\(/g), 12, 'faces');
  assert.equal(count(/=CLOSED_SHELL\(/g), 1, 'closed shells');
  assert.equal(count(/=MANIFOLD_SOLID_BREP\(/g), 1, 'solids');
  assert.equal(count(/=PLANE\(/g), 12, 'planes');
  // parse back: the 8 cube corners (in mm) plus the origin
  const pts = [...text.matchAll(/=CARTESIAN_POINT\('',\(([^)]*)\)\);/g)].map(x => x[1].split(',').map(Number));
  assert.equal(pts.length, 9, 'CARTESIAN_POINT count (8 corners + origin)');
  assert.ok(pts.every(p => p.length === 3 && p.every(Number.isFinite)), 'every point has three finite numbers');
  const corners = pts.filter(p => p.every(c => c === 0 || Math.abs(c) === 10 || Math.abs(c - 10) < 1e-6 || Math.abs(c + 10) < 1e-6)).length;
  assert.equal(corners, 9, 'every point sits on the cube corners (origin included)');
  // every edge is shared by exactly two faces (the ORIENTED_EDGE count is 3 per face)
  assert.equal(count(/=ORIENTED_EDGE\(/g), 36, 'oriented edges');
  assert.equal(count(/=EDGE_CURVE\(/g), 18, 'edges: 12 cube edges plus 6 diagonals');
});

test('STEP export: an open or inside-out mesh is refused with a plain message', async () => {
  const ST = await load('engine/step.js');
  const m = W.Manifold.cube([10, 10, 10]).getMesh();
  const open = { v: m.vertProperties, f: m.triVerts.slice(0, -3), name: 'open' };
  assert.throws(() => ST.stepBytes([open]), /isn't a closed solid/);
});

test('STEP export: the route writes the body as .step and the export menu route agrees', async () => {
  const R = await load('engine/routes.js');
  const doc = new Document();
  const [f] = doc.importFile('block.stl', stlOf(W.Manifold.cube([40, 30, 20])), true, 'keep');
  R.app.doc = doc;
  const r = R.ROUTES.to_step({ body: f.id });
  assert.equal(r.result.ok, true);
  assert.ok(r.result.name.endsWith('.step'), 'file name ends in .step');
  const text = new TextDecoder().decode(r.result.bytes);
  assert.equal((text.match(/=ADVANCED_FACE\(/g) || []).length, 12, 'a box is 12 faces');
  const e = R.ROUTES.export_bytes({ format: 'step' });
  assert.equal(e.result.count, 1);
  assert.equal(new TextDecoder().decode(e.result.bytes), text, 'export_bytes(step) matches to_step');
});

test('thin-wall check: a 0.5 mm slab and pin are flagged, a 2 mm wall and a cube are not', () => {
  const check = (man, mn) => {
    const doc = new Document();
    const [f] = doc.importFile('t.stl', stlOf(man), true, 'keep');
    const [, res] = doc.addFeature('thincheck', mn === undefined ? { body: f.id } : { body: f.id, min: mn });
    approx(vol(doc, f.id), man.volume(), 1e-6, 'changes nothing');
    return res;
  };
  const C = W.Manifold.cube;
  const ok = check(C([10, 10, 10]));
  assert.equal(ok.thin.count, 0); assert.equal(ok.thin.min, null); assert.ok(!ok.warning);
  assert.equal(check(C([20, 20, 2])).thin.count, 0);
  const r = check(C([20, 20, 0.5]));
  assert.equal(r.thin.count, 1); assert.ok(r.warning);
  assert.ok(Math.abs(r.thin.min - 0.5) <= 0.02, `min ${r.thin.min}`);
  assert.ok(Math.abs(r.thin.spots[0][2] - 0.25) <= 0.05);
  assert.ok(Math.abs(check(C([20, 20, 2]), 2.5).thin.min - 2) <= 0.02, 'a 2 mm wall is thin under a 2.5 mm minimum');
  const pin = C([10, 10, 10]).add(W.Manifold.cylinder(5, 0.25, 0.25, 32).translate([5, 5, 10]));
  const p = check(pin).thin;
  assert.equal(p.count, 1); assert.ok(Math.abs(p.min - 0.5) <= 0.02);
  const [x, y, z] = p.spots[0];
  assert.ok(Math.abs(x - 5) <= 0.3 && Math.abs(y - 5) <= 0.3 && z > 10 && z < 15);
  assert.throws(() => check(C([10, 10, 10]), 0), /more than zero/);
});

test('fit test plate: volume is plate minus holes plus pegs (hand-checked)', () => {
  const D = 8, start = 0.05, step = 0.05, count = 3, T = 3, gap = 4;
  const clearances = Array.from({ length: count }, (_, i) => start + i * step);
  const maxHole = D + clearances[clearances.length - 1];
  const L = count * maxHole + (count + 1) * gap, W = maxHole + D + 3 * gap;
  let expected = L * W * T;
  for (const c of clearances) expected -= Math.PI * ((D + c) / 2) ** 2 * T;
  expected += count * Math.PI * (D / 2) ** 2 * T;
  const doc = new Document();
  const [f, res] = doc.addFeature('fittest', { diameter: D, start, step, count, thickness: T, gap });
  approx(vol(doc, f.id), expected, 0.01, 'fit test volume');
  assert.ok(res.info.includes('0.05') && res.info.includes('0.15'));
  assert.throws(() => doc.addFeature('fittest', { diameter: 0, start, step, count, thickness: T, gap }), /more than zero/);
});

test('snap-fit and living hinge', () => {
  const [doc, B] = block();
  const top = { origin: [0, 0, 20], normal: [0, 0, 1], xdir: [1, 0, 0] };
  const clip = { plane: top, x: 5, y: 5, length: 12, width: 6, thickness: 1.5, hook_height: 1.5 };
  // new body: arm 1.5 x 6 x 12.5 plus hook 1.5 x 6 x 1.5
  doc.addFeature('snapfit', { ...clip, operation: 'new' });
  approx(vol(doc, [...doc.state.bodies.keys()].at(-1)), 126, 1e-6, 'snap-fit alone');
  const v0 = vol(doc, B);
  doc.addFeature('snapfit', { ...clip, targets: [B] });
  approx(vol(doc, B) - v0, 126 - 4.5, 1e-6, 'snap-fit joined');
  assert.throws(() => doc.addFeature('snapfit', { ...clip, hook_height: 12 }), /shorter than the arm/);
  const v1 = vol(doc, B);
  doc.addFeature('hinge', { x: 10, slot: 1, thickness: 0.5 });
  approx(v1 - vol(doc, B), 1 * 30 * 19.5, 1e-6, 'living hinge');
});
