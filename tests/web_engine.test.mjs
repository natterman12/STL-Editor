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
