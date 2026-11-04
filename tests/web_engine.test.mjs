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
