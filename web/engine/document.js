// The design document (port of stleditor/document.py): imported sources plus an ordered,
// editable feature history, with cached re-evaluation and undo/redo.
import { EVALUATORS, LABELS, State } from './features.js';
import { GeometryError } from './geom.js';
import * as IO from './io.js';

const FORMAT_VERSION = 1;

export class Document {
  constructor() {
    this.sources = {};        // sid -> {name, ext, data: Uint8Array}
    this._parsed = {};
    this.features = [];
    this.rollback = null;
    this.ui = { names: {}, colors: {}, hidden: [] };
    this.path = null;
    this.title = null;
    this.dirty = false;
    this._next = 1;
    this._cache = [];         // [hash, state] per active feature
    this._memo = new Map();   // hash -> [state, result]
    this._results = [];
    this._undo = [];
    this._redo = [];
    this.state = new State();
  }

  newId(prefix = 'f') {
    const used = new Set([...this.features.map(f => f.id), ...Object.keys(this.sources)]);
    for (;;) { const i = `${prefix}${this._next++}`; if (!used.has(i)) return i; }
  }

  addSource(name, data) {
    const sid = this.newId('s');
    this.sources[sid] = { name, ext: IO.extOf(name), data };
    return sid;
  }

  loadSource(sid) {
    if (!this._parsed[sid]) {
      const src = this.sources[sid];
      if (!src) throw new GeometryError('The imported file is missing from the project.');
      this._parsed[sid] = IO.loadMeshFile(src.data, src.name);
    }
    return this._parsed[sid];
  }

  // -- undo
  _snapshot() { return JSON.stringify({ features: this.features, rollback: this.rollback, ui: this.ui }); }
  checkpoint() { this._undo.push(this._snapshot()); if (this._undo.length > 200) this._undo.shift(); this._redo = []; this.dirty = true; }
  _restore(snap) { const d = JSON.parse(snap); this.features = d.features; this.rollback = d.rollback; this.ui = d.ui; this.evaluate(); }
  undo() { if (!this._undo.length) return false; this._redo.push(this._snapshot()); this._restore(this._undo.pop()); this.dirty = true; return true; }
  redo() { if (!this._redo.length) return false; this._undo.push(this._snapshot()); this._restore(this._redo.pop()); this.dirty = true; return true; }

  // -- evaluation
  activeFeatures() { return this.features.slice(0, this.rollback === null ? this.features.length : this.rollback); }
  static fhash(f, prev) { return prev + '|' + stableStringify(f); }

  evaluate() {
    const ctx = { loadSource: sid => this.loadSource(sid) };
    let state = new State();
    const results = [], cache = [];
    let h = '';
    this.activeFeatures().forEach((f, i) => {
      h = Document.fhash(f, h);
      if (i < this._cache.length && this._cache[i][0] === h && i < this._results.length) {
        state = this._cache[i][1];
        results.push(this._results[i]);
        cache.push(this._cache[i]);
        return;
      }
      let res;
      if (this._memo.has(h)) [state, res] = this._memo.get(h);
      else [state, res] = this.runFeature(state, f, ctx);
      results.push(res);
      cache.push([h, state]);
    });
    this._cache = cache;
    this._results = results;
    this.state = state;
    return results;
  }

  _remember(at, f, state, res) {
    if (at > this._cache.length) this.evaluate();
    const prev = at > 0 && at - 1 < this._cache.length ? this._cache[at - 1][0] : '';
    this._memo.set(Document.fhash(f, prev), [state, res]);
    while (this._memo.size > 6) this._memo.delete(this._memo.keys().next().value);
  }

  runFeature(state, f, ctx) {
    ctx = ctx || { loadSource: sid => this.loadSource(sid) };
    const res = { id: f.id, error: null, warning: null, created: [], modified: [], ms: 0 };
    if (f.suppressed) { res.suppressed = true; return [state, res]; }
    const t0 = performance.now();
    const nxt = state.clone();
    try {
      const fn = EVALUATORS[f.type];
      if (!fn) throw new GeometryError(`Unknown feature ${f.type}.`);
      fn(nxt, f.id, f.params || {}, res, ctx);
      state = nxt;
    } catch (e) {
      if (e instanceof GeometryError) res.error = e.message;
      else { console.error(e); res.error = `${e.name || 'Error'}: ${e.message || e}`; }
    }
    res.ms = Math.round((performance.now() - t0) * 10) / 10;
    return [state, res];
  }

  // -- editing
  addFeature(type, params, name = null) {
    if (!EVALUATORS[type]) throw new GeometryError(`Unknown feature ${type}.`);
    const f = { id: this.newId(), type, name: name || this.autoName(type, params), params };
    const at = this.rollback === null ? this.features.length : this.rollback;
    const [st, res] = this.runFeature(this.stateBefore(at), f);
    if (res.error) throw new GeometryError(res.error);
    this._remember(at, f, st, res);
    this.checkpoint();
    this.features.splice(at, 0, f);
    if ((type === 'extrude' || type === 'revolve') && params.sketch) this.ui.hidden = [...new Set([...this.ui.hidden, params.sketch])].sort();
    if (this.rollback !== null) this.rollback += 1;
    this.evaluate();
    return [f, res];
  }

  preview(type, params, editId = null) {
    if (editId) {
      const idx = this.index(editId);
      const f = { ...this.features[idx], params };
      const [st, res] = this.runFeature(this.stateBefore(idx), f);
      if (!res.error) this._remember(idx, f, st, res);
      return [st, res];
    }
    const at = this.rollback === null ? this.features.length : this.rollback;
    return this.runFeature(this.stateBefore(at), { id: 'preview', type, name: 'preview', params });
  }

  editFeature(fid, params = null, name = null, suppressed = null, rollbackAfter = 'keep') {
    const idx = this.index(fid);
    const f = JSON.parse(JSON.stringify(this.features[idx]));
    if (params !== null && params !== undefined) f.params = params;
    if (name !== null && name !== undefined) f.name = name;
    if (suppressed !== null && suppressed !== undefined) f.suppressed = !!suppressed;
    if (params !== null && params !== undefined) {
      const [st, res] = this.runFeature(this.stateBefore(idx), f);
      if (res.error) throw new GeometryError(res.error);
      this._remember(idx, f, st, res);
    }
    if (rollbackAfter !== 'keep') this.rollback = rollbackAfter;
    this.checkpoint();
    this.features[idx] = f;
    return this.evaluate();
  }

  deleteFeature(fid) {
    const idx = this.index(fid);
    this.checkpoint();
    this.features.splice(idx, 1);
    if (this.rollback !== null && idx < this.rollback) this.rollback -= 1;
    return this.evaluate();
  }

  moveFeature(fid, newIndex) {
    const idx = this.index(fid);
    this.checkpoint();
    const [f] = this.features.splice(idx, 1);
    this.features.splice(Math.max(0, Math.min(newIndex, this.features.length)), 0, f);
    return this.evaluate();
  }

  setRollback(index) {
    if (index !== null && index !== undefined && index >= this.features.length) index = null;
    this.rollback = index ?? null;
    return this.evaluate();
  }

  index(fid) {
    const i = this.features.findIndex(f => f.id === fid);
    if (i < 0) throw new GeometryError('That feature no longer exists.');
    return i;
  }

  stateBefore(idx) {
    if (idx === 0) return new State();
    this.evaluate();
    if (idx - 1 < this._cache.length) return this._cache[idx - 1][1];
    let state = new State();
    for (const f of this.features.slice(0, idx)) [state] = this.runFeature(state, f);
    return state;
  }

  autoName(type, params = {}) {
    if (type === 'primitive') {
      const shape = params.shape || 'box';
      const n = this.features.filter(f => f.type === type && f.params.shape === shape).length + 1;
      return `${shape[0].toUpperCase()}${shape.slice(1)}${n}`;
    }
    return `${LABELS[type] || type}${this.features.filter(f => f.type === type).length + 1}`;
  }

  bodyName(bid) { return this.ui.names[bid] || this.state.bodies.get(bid)?.name || bid; }

  setBodyProps(bid, name = null, color = null, hidden = null) {
    this.checkpoint();
    if (name !== null && name !== undefined) this.ui.names[bid] = name;
    if (color !== null && color !== undefined) this.ui.colors[bid] = color;
    if (hidden !== null && hidden !== undefined) {
      const h = new Set(this.ui.hidden);
      hidden ? h.add(bid) : h.delete(bid);
      this.ui.hidden = [...h].sort();
    }
  }

  // -- files
  importFile(name, data, repair = true, place = 'center') {
    if (!IO.MESH_EXTS.includes(IO.extOf(name))) throw new GeometryError('Unsupported file type. STL Editor opens STL, OBJ, 3MF, PLY, OFF, GLB and glTF.');
    const sid = this.addSource(name, data);
    try { return this.addFeature('import', { source: sid, repair, place }, IO.baseOf(name)); }
    catch (e) { delete this.sources[sid]; throw e; }
  }

  saveBytes(markClean = true) {
    const used = new Set(this.features.filter(f => f.type === 'import').map(f => f.params.source));
    const files = {
      'document.json': IO.strToU8(JSON.stringify({
        format: FORMAT_VERSION, app: 'STL Editor', features: this.features, rollback: this.rollback, ui: this.ui,
        sources: Object.fromEntries(Object.entries(this.sources).filter(([k]) => used.has(k)).map(([k, v]) => [k, { name: v.name, ext: v.ext }])),
      }, null, 1)),
    };
    for (const [sid, src] of Object.entries(this.sources)) if (used.has(sid)) files[`sources/${sid}${src.ext}`] = [src.data, { level: 6 }];
    if (markClean) this.dirty = false;
    return IO.zipSync(files);
  }

  static fromBytes(bytes, name) {
    const doc = new Document();
    let z;
    try { z = IO.unzipSync(new Uint8Array(bytes)); } catch { throw new GeometryError("This isn't an STL Editor project."); }
    if (!z['document.json']) throw new GeometryError("This isn't an STL Editor project.");
    const d = JSON.parse(IO.strFromU8(z['document.json']));
    for (const [sid, meta] of Object.entries(d.sources || {})) {
      const data = z[`sources/${sid}${meta.ext}`];
      if (data) doc.sources[sid] = { name: meta.name, ext: meta.ext, data };
    }
    doc.features = d.features || [];
    doc.rollback = d.rollback ?? null;
    doc.ui = d.ui || doc.ui;
    doc.path = name;
    const nums = [...doc.features.map(f => f.id), ...Object.keys(doc.sources)].map(x => parseInt(x.slice(1), 10)).filter(Number.isFinite);
    doc._next = Math.max(0, ...nums) + 1;
    doc.evaluate();
    return doc;
  }

  exportBytes(fmt, bodyIds = null) {
    let ids = bodyIds && bodyIds.length ? bodyIds : [...this.state.bodies.keys()].filter(b => !this.ui.hidden.includes(b));
    if (!ids.length) ids = [...this.state.bodies.keys()];
    if (!ids.length) throw new GeometryError('There is nothing to export.');
    const meshes = ids.map(id => ({ ...this.state.body(id).mesh(), name: this.bodyName(id) }));
    return { bytes: IO.exportBytes(meshes, fmt), count: ids.length };
  }

  /** Every Manifold still needed by cached states (for freeing the rest). */
  liveObjects() {
    const keep = new Set();
    const addState = st => {
      if (!st) return;
      for (const b of st.bodies.values()) if (b.solid) keep.add(b.solid);
      for (const t of st.tools.values()) if (t.tool) keep.add(t.tool);
    };
    addState(this.state);
    for (const [, st] of this._cache) addState(st);
    for (const [st] of this._memo.values()) addState(st);
    return keep;
  }
}

export function stableStringify(v) {
  if (v === null || typeof v !== 'object') return JSON.stringify(v);
  if (Array.isArray(v)) return '[' + v.map(stableStringify).join(',') + ']';
  return '{' + Object.keys(v).sort().map(k => JSON.stringify(k) + ':' + stableStringify(v[k])).join(',') + '}';
}
