// The engine's API (mirrors the desktop app's stleditor/server.py routes).
import { Document } from './document.js';
import { GeometryError, faceRegion, planeFrame, regionBoundary, dot, sub } from './geom.js';
import * as S from './sketch2d.js';
import * as PR from './printing.js';
import * as MT from './meshtools.js';
import * as SFX from './supportfins.js';

export const VERSION = '1.13.1';
const PALETTE = ['#9aa7b4', '#c9a66b', '#7fa8c9', '#a3c48a', '#c98a8a', '#a993c9', '#c9b98a', '#8ac9c0'];

export const app = { doc: new Document() };

function fname(fid, def) { const f = app.doc.features.find(x => x.id === fid); return f?.name || def; }

export function stateJson() {
  const d = app.doc, st = d.state;
  const res = Object.fromEntries(d._results.map(r => [r.id, r]));
  const features = d.features.map((f, i) => {
    const r = res[f.id] || {};
    return { ...f, error: r.error ?? null, warning: r.warning ?? null, ms: r.ms ?? null, active: d.rollback === null || i < d.rollback };
  });
  const bodies = [...st.bodies].map(([bid, b], i) => {
    const [lo, hi] = b.bbox();
    return { id: bid, name: d.bodyName(bid), serial: b.serial, solid: b.isSolid, triangles: b.mesh().f.length / 3,
      color: d.ui.colors[bid] || PALETTE[i % PALETTE.length], hidden: d.ui.hidden.includes(bid), bbox: Number.isFinite(lo[0]) ? [lo, hi] : null };
  });
  const planes = [...st.planes].map(([k, p]) => ({ id: k, name: fname(k, p.name), frame: p.frame, hidden: d.ui.hidden.includes(k) }));
  const sketches = [...st.sketches].map(([k, s]) => ({ id: k, name: fname(k, s.name), frame: s.frame, entities: s.entities, refs: s.refs || [], hidden: d.ui.hidden.includes(k) }));
  const pv = d.paramValues();
  const params = d.params.map(q => ({ ...q, value: pv.values[q.name] ?? null, error: pv.errors[q.name] ?? null }));
  return { features, rollback: d.rollback, bodies, planes, sketches, params,
    doc: { path: d.path, name: d.path || d.title || 'Untitled', dirty: d.dirty, can_undo: !!d._undo.length, can_redo: !!d._redo.length, version: VERSION, autosaved: null, web: true },
    recent: [] };
}

/** {meshes: [{id, nv, nf, serial, solid, positions, index}], ...header} plus transferables. */
export function packMeshes(list, header = {}) {
  const meshes = [], transfer = [];
  for (const [bid, b] of list) {
    const { v, f } = b.mesh();
    const positions = Float32Array.from(v), index = Uint32Array.from(f);
    meshes.push({ id: bid, nv: positions.length / 3, nf: index.length / 3, serial: b.serial, solid: b.isSolid, positions, index });
    transfer.push(positions.buffer, index.buffer);
  }
  return { result: { ...header, meshes }, transfer };
}

const ok = extra => ({ ok: true, state: stateJson(), ...extra });

export const ROUTES = {
  state: () => stateJson(),
  mesh: d => packMeshes([[d.id, app.doc.state.body(d.id)]]),
  'feature/add': d => { const [f, res] = app.doc.addFeature(d.type, d.params || {}, d.name); return ok({ feature: f, result: res }); },
  'feature/edit': d => { app.doc.editFeature(d.id, d.params, d.name, d.suppressed, 'rollback_after' in d ? d.rollback_after : 'keep'); return ok(); },
  'feature/delete': d => { app.doc.deleteFeature(d.id); return ok(); },
  'feature/move': d => { app.doc.moveFeature(d.id, +d.index); return ok(); },
  rollback: d => { app.doc.setRollback(d.index ?? null); return ok(); },
  'params/set': d => { app.doc.setParams(d.params || []); return ok(); },
  preview: d => {
    const [state, res] = app.doc.preview(d.type, d.params || {}, d.edit_id);
    const ids = [...new Set([...(res.created || []), ...(res.modified || [])])].filter(i => state.bodies.has(i));
    return packMeshes(ids.map(i => [i, state.bodies.get(i)]), { result: res, removed: res.removed || [] });
  },
  face: d => {
    const b = app.doc.state.body(d.body);
    const { v, f } = b.mesh();
    const { region, normal, centroid } = faceRegion(v, f, +d.tri, +(d.angle_tol || 1));
    let n = normal, origin = centroid;
    const k = [0, 1, 2].reduce((a, i) => Math.abs(normal[i]) > Math.abs(normal[a]) ? i : a, 0);
    if (Math.abs(normal[k]) > 0.99999) { n = [0, 0, 0]; n[k] = Math.sign(normal[k]); origin = [0, 0, 0]; origin[k] = centroid[k]; }
    const [o, x, y, nn] = planeFrame(origin, n);
    let bd = regionBoundary(f, region);
    if (bd.length > 20000) bd = bd.slice(0, 20000);
    const seg3 = bd.map(([a, c]) => [[v[a * 3], v[a * 3 + 1], v[a * 3 + 2]], [v[c * 3], v[c * 3 + 1], v[c * 3 + 2]]]);
    const seg2 = seg3.map(s => s.map(p => { const q = sub(p, o); return [+dot(q, x).toFixed(5), +dot(q, y).toFixed(5)]; }));
    const contours = d.outline && region.length <= 400000 ? PR.faceOutline(v, f, region, [o, x, y, nn]) : [];
    return { contours, body: d.body, origin: o, normal: nn, xdir: x, ydir: y, centroid, hit: d.point || centroid,
      tris: region.slice(0, 200000), boundary3d: seg3, boundary2d: seg2, planar: region.length > 1 };
  },
  cylinder: d => {
    const b = app.doc.state.body(d.body), { v, f } = b.mesh();
    const tri = d.tri !== undefined && d.tri !== null ? +d.tri : MT.nearestTriangle(v, f, d.point);
    const c = MT.fitCylinder(v, f, tri);
    return { ok: true, kind: c.kind, diameter: +(2 * c.radius).toFixed(3), length: +(c.z1 - c.z0).toFixed(3), axis: c.axis, tris: c.tris.slice(0, 200000) };
  },
  sketch_op: async d => {
    const ents = d.entities || [];
    const used = new Set(ents.map(e => e.id));
    let cnt = 1;
    const newId = () => { for (;;) { const i = `e${d.seed || 'x'}${cnt++}`; if (!used.has(i)) { used.add(i); return i; } } };
    const op = d.op;
    let out;
    if (op === 'trim') out = S.trim(ents, d.id, d.point);
    else if (op === 'offset') out = S.offset(ents, d.ids || [], +(d.distance ?? 1), newId);
    else if (op === 'mirror') out = S.mirror(ents, d.ids || [], d.axis, newId);
    else if (op === 'explode') { out = []; for (const e of ents) out.push(...((d.ids || []).includes(e.id) ? S.explode(e) : [e])); }
    else if (op === 'project') {
      const st = app.doc.state;
      const ids = d.bodies && d.bodies.length ? d.bodies : [...st.bodies].filter(([k, b]) => !app.doc.ui.hidden.includes(k) && b.isSolid).map(([k]) => k);
      const solids = ids.filter(i => st.bodies.has(i)).map(i => st.body(i).requireSolid());
      if (!solids.length) throw new GeometryError('There are no solid bodies to project.');
      const add = MT.outlineEntities(MT.outline(solids, d.frame, d.mode || 'section', +(d.tolerance ?? 0.02), +(d.offset || 0)), newId, !!d.construction);
      return { ok: true, entities: ents.concat(add), added: add.map(e => e.id) };
    } else if (op === 'refs') {
      // part edges for a sketch to dimension from (see MT.refGeometry)
      const st = app.doc.state;
      const ids = [...st.bodies].filter(([k, b]) => !app.doc.ui.hidden.includes(k) && b.isSolid).map(([k]) => k);
      const solids = ids.map(i => st.body(i).requireSolid());
      return { ok: true, ...(solids.length ? MT.refGeometry(solids, d.frame) : { kind: null, refs: [] }) };
    } else if (op === 'text') {
      const cont = await MT.textContours(d.text || '', d.font, +(d.size || 10), !!d.bold, !!d.italic, +(d.spacing || 0), d.align || 'left');
      if (!cont.length) throw new GeometryError('Type some text first.');
      const [lo, hi] = MT.textExtent(cont);
      const anchor = d.anchor || 'baseline', align = d.align || 'left';
      const ox = anchor === 'center' ? (lo[0] + hi[0]) / 2 : ({ left: 0, center: (lo[0] + hi[0]) / 2, right: hi[0] }[align] || 0);
      const oy = anchor === 'center' ? (lo[1] + hi[1]) / 2 : 0;
      const a = +(d.angle || 0) * Math.PI / 180, ca = Math.cos(a), sa = Math.sin(a), x0 = +(d.x || 0), y0 = +(d.y || 0);
      const group = d.group || `t${d.seed || 'x'}`;
      const meta = Object.fromEntries(['text', 'font', 'size', 'bold', 'italic', 'spacing', 'align', 'angle', 'x', 'y', 'anchor'].map(k => [k, d[k] ?? null]));
      const keep = ents.filter(e => e.group !== group);
      const add = cont.map(c => ({ id: newId(), type: 'polyline', closed: true, group, text: meta,
        pts: c.map(([u, w]) => { u -= ox; w -= oy; return [+(x0 + ca * u - sa * w).toFixed(4), +(y0 + sa * u + ca * w).toFixed(4)]; }) }));
      return { ok: true, entities: keep.concat(add), added: add.map(e => e.id), group };
    } else if (op === 'faces') {
      return { ok: true, faces: S.faces(ents).map(f => [[...f.outer, f.outer[0]], ...f.holes.map(h => [...h, h[0]])]) };
    } else throw new GeometryError(`Unknown sketch operation ${op}.`);
    return { ok: true, entities: out };
  },
  'supportfins/info': () => ({ ok: true, ...SFX.info() }),
  'supportfins/suggest': d => ({ ok: true, ...SFX.suggest(app.doc.state.body(d.body).mesh(), d.options || {}, d.top || 3) }),
  'supportfins/strength': d => ({ ok: true, ...SFX.strength(app.doc.state.body(d.body).mesh(), d.options || {}, d.load || null) }),
  printers: () => ({ ok: true, printers: Object.fromEntries(Object.entries(PR.PRINTERS).map(([k, v]) => [k, { name: v[0], bed: v[1] }])) }),
  body: d => { app.doc.setBodyProps(d.id, d.name, d.color, d.hidden); return ok(); },
  info: d => ({ ok: true, info: { ...app.doc.state.body(d.id).info(), name: app.doc.bodyName(d.id) } }),
  undo: () => { app.doc.undo(); return ok(); },
  redo: () => { app.doc.redo(); return ok(); },
  new: d => {
    if (app.doc.dirty && !d.force) return { ok: false, confirm: 'Discard unsaved changes?' };
    app.doc = new Document();
    return ok();
  },
  // file routes: the page reads and writes the files; the engine works on their bytes
  open_bytes: d => {
    if (d.recovered) {
      app.doc = Document.fromBytes(d.data, d.name);
      app.doc.path = d.path || null; app.doc.title = d.title || null; app.doc.dirty = true;
    } else if (/\.stlx$/i.test(d.name)) app.doc = Document.fromBytes(d.data, d.name);
    else {
      app.doc = new Document();
      app.doc.importFile(d.name, new Uint8Array(d.data));
      app.doc.title = d.name.replace(/\.[^.]+$/, '');
      app.doc.dirty = false;
    }
    return ok();
  },
  upload: d => { app.doc.importFile(d.name, new Uint8Array(d.data), true, d.place || 'center'); return ok(); },
  // the page writes the bytes to a file; only then does `saved` mark the project as saved
  save_bytes: () => {
    const bytes = app.doc.saveBytes(false);
    const base = (app.doc.path || app.doc.title || 'Untitled').replace(/\.stlx$/i, '');
    return { result: { ok: true, name: `${base}.stlx`, bytes }, transfer: [bytes.buffer] };
  },
  saved: d => { app.doc.path = d.name; app.doc.dirty = false; return ok(); },
  export_bytes: d => {
    const { bytes, count } = app.doc.exportBytes(d.format || 'stl', d.bodies);
    const ids = d.bodies;
    const base = ids && ids.length === 1 ? app.doc.bodyName(ids[0]) : (app.doc.path || app.doc.title || 'model').replace(/\.stlx$/i, '');
    return { result: { ok: true, name: `${base}.${d.format || 'stl'}`, count, bytes }, transfer: [bytes.buffer] };
  },
  status: () => ({ ok: true, dirty: app.doc.dirty, desktop: false, web: true }),
  // autosave copy: the project as .stlx bytes without marking it saved; `key` changes when
  // the timeline does, so unchanged work isn't written again
  snapshot: () => {
    const d = app.doc;
    if (!d.features.length) return { ok: true, empty: true, dirty: d.dirty };
    const s = d._snapshot();
    let h = 2166136261;
    for (let i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 16777619); }
    const bytes = d.saveBytes(false);
    return { result: { ok: true, bytes, key: `${s.length}-${(h >>> 0).toString(36)}`, title: d.path || d.title || 'Untitled', path: d.path, steps: d.features.length, dirty: d.dirty }, transfer: [bytes.buffer] };
  },
};
