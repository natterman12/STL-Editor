// Sketch mode: draw 2D entities on a plane, with snapping and typed dimensions.
import * as THREE from 'three';
import { entityOutline, keyPoints, distToEntity, describe, ENTITY_LABEL, arcFrom3, dimensions, applyDimension } from './sketchgeom.js';
import { solve, makeConstraint, measure, CTYPES, CTOOLS, glyphSpots, pointKeys, point as cpoint, refsExist } from './constraints.js';
import { U, parseExpr } from './units.js';

const COL = 0xd9480f, COL_SEL = 0x1f6fd1, COL_PREV = 0x2f9e44, COL_REF = 0x7c8794;
let uid = 0;
const newId = () => `e${Date.now().toString(36)}${(uid++).toString(36)}`;
const fmt = v => (Math.round(v * 1000) / 1000).toString();

export class Sketcher {
  constructor(app) {
    this.app = app;
    this.viewer = app.viewer;
    this.active = false;
    this.hud = document.getElementById('sketch-hud');
    this.inA = document.getElementById('dyn-a');
    this.inB = document.getElementById('dyn-b');
    this.group = new THREE.Group();
    this.viewer.scene.add(this.group);
    this.snapStep = 1;
    this.snapGrid = true;
    this.dims = document.createElement('div');
    this.dims.id = 'dims';
    this.viewer.canvas.parentElement.appendChild(this.dims);
    this.viewer.controls.addEventListener('change', () => { if (this.active) this.placeDims(); });
    new ResizeObserver(() => { if (this.active) this.placeDims(); }).observe(this.viewer.canvas);
    this._bind();
  }

  start({ frame, entities = [], refs = [], featureId = null, planeRef, name, bodyId, constraints = [] }) {
    this.active = true;
    this.constraints = JSON.parse(JSON.stringify(constraints || []));
    this.selC = null;
    this.cpicks = [];
    this.frame = frame.map(a => new THREE.Vector3(...a));
    this.frameArr = frame;
    this.entities = JSON.parse(JSON.stringify(entities));
    this.refs = refs || [];
    this.featureId = featureId;
    this.planeRef = planeRef;
    this.bodyId = bodyId;
    this.name = name;
    this.sel = new Set();
    this.tool = 'select';
    this.draw = null;
    this.history = [];
    this.viewer.setMouseMode('sketch');
    this.viewer.lookAtPlane(frame);
    this._buildPlaneGrid();
    this.redraw();
    this.app.onSketchChanged?.();
  }

  stop() {
    this.active = false;
    this.draw = null;
    this.dims.innerHTML = '';
    this.group.clear();
    this.hideHud();
    this.viewer.setMouseMode('model');
    this.viewer.requestRender();
  }

  setTool(t) {
    this.cancelDraw();
    this.tool = t;
    this.cpicks = [];
    this.dims.classList.toggle('passive', this.pickingTool());
    this.app.onSketchChanged?.();
    this.app.hint(this.toolHint());
  }

  // tools whose clicks pick geometry: dimension labels must let those clicks through
  pickingTool() { return this.tool.startsWith('c_') || this.tool === 'trim' || this.tool === 'sk_mirror' || !!this.draw; }

  toolHint() {
    return {
      select: 'Click a shape to select it · Delete removes it · Right-drag to orbit, middle-drag to pan',
      line: 'Click points to draw connected lines · click the first point to close · Enter or Esc to stop · type a length',
      arc3: 'Click the start, then the end, then a point the arc passes through · or type a radius',
      trim: 'Click the piece of a line, arc or shape to cut away (up to where it crosses something)',
      sk_mirror: 'Select shapes first, then click the straight line to mirror them across',
      rect: 'Click two corners · or type width, Tab, height',
      rect_center: 'Click the centre, then a corner · or type width, Tab, height',
      circle: 'Click the centre, then the edge · or type a diameter',
      polygon: 'Click the centre, then a corner · type radius, Tab, number of sides',
      slot: 'Click both arc centres, then the width',
      text: 'Click where the text should start',
      ...Object.fromEntries(Object.entries(CTOOLS).map(([k, v]) => [k, v.hint])),
    }[this.tool] || '';
  }

  // ------------------------------------------------------------------- coordinates
  toUV(p) { const [o, x, y] = this.frame; const d = p.clone().sub(o); return [d.dot(x), d.dot(y)]; }
  to3(u, v) { const [o, x, y] = this.frame; return o.clone().addScaledVector(x, u).addScaledVector(y, v); }

  eventUV(ev) {
    const p = this.viewer.rayToPlane(ev, this.frameArr);
    if (!p) return null;
    let uv = this.toUV(p);
    const px = this.viewer.pixelSize(p.toArray());
    // snap to key points (entities, references, origin, current drawing)
    let best = null, bd = 10 * px, bestRef = null;
    const cands = [{ p: [0, 0] }];
    for (const e of this.entities) {
      for (const k of pointKeys(e)) cands.push({ p: cpoint(e, k), ref: { e: e.id, k } });
      for (const q of keyPoints(e)) cands.push({ p: q });
    }
    for (const s of this.refs) cands.push({ p: s[0] }, { p: s[1] });
    if (this.draw?.pts) this.draw.pts.forEach((q, i) => cands.push({ p: q, draw: i }));
    for (const c of cands) { const d = Math.hypot(c.p[0] - uv[0], c.p[1] - uv[1]); if (d < bd - 1e-12) { bd = d; best = c.p; bestRef = c.ref || null; } }
    this.snapped = !!best;
    this.snapRef = bestRef;
    if (best) uv = [best[0], best[1]];
    else if (this.snapGrid && this.snapStep > 0) {
      const s = this.snapStep;
      uv = [Math.round(uv[0] / s) * s, Math.round(uv[1] / s) * s];
      if (this.draw?.pts?.length && ['line'].includes(this.tool)) {
        // hold near-horizontal / vertical lines straight
        const last = this.draw.pts[this.draw.pts.length - 1];
        const raw = this.toUV(p);
        if (Math.abs(raw[1] - last[1]) < 6 * px) uv[1] = last[1];
        if (Math.abs(raw[0] - last[0]) < 6 * px) uv[0] = last[0];
      }
    }
    return uv;
  }

  // ------------------------------------------------------------------- events
  _bind() {
    const c = this.viewer.canvas;
    c.addEventListener('pointermove', ev => { if (this.active) this.onMove(ev); });
    c.addEventListener('pointerdown', ev => {
      if (!this.active || ev.button !== 0) return;
      this._down = [ev.clientX, ev.clientY];
      if (this.tool === 'select') {
        const p = this.viewer.rayToPlane(ev, this.frameArr);
        if (p && this.dragStart(this.toUV(p))) c.setPointerCapture(ev.pointerId);
      }
    });
    c.addEventListener('pointerup', ev => {
      if (!this.active || ev.button !== 0 || !this._down) return;
      const moved = Math.hypot(ev.clientX - this._down[0], ev.clientY - this._down[1]) > 5;
      this._down = null;
      const dragged = this._drag ? this.dragEnd() : false;
      if (!moved && !dragged) this.onClick(ev);
    });
    c.addEventListener('dblclick', ev => { if (this.active && this.tool === 'line') this.finishLine(false); });
    for (const inp of [this.inA, this.inB]) {
      inp.addEventListener('keydown', ev => {
        if (ev.key === 'Enter') { ev.preventDefault(); this.commitTyped(); }
        else if (ev.key === 'Tab') { ev.preventDefault(); (inp === this.inA ? this.inB : this.inA).focus(); (inp === this.inA ? this.inB : this.inA).select(); }
        else if (ev.key === 'Escape') { ev.preventDefault(); inp.blur(); this.cancelDraw(); }
        ev.stopPropagation();
      });
      inp.addEventListener('input', () => { this.typed = true; this.updateDraw(); });
    }
  }

  onKey(ev) {
    if (!this.active) return false;
    const k = ev.key;
    if (this.draw && /^[0-9.\-]$/.test(k) && document.activeElement !== this.inA && document.activeElement !== this.inB) {
      this.inA.value = ''; this.inA.focus(); return false; // let the key land in the field
    }
    if (k === 'Escape') { if (this.draw) { if (this.tool === 'line') this.finishLine(false); else this.cancelDraw(); } else if (this.tool !== 'select') this.setTool('select'); else this.setSel([]); return true; }
    if (k === 'Enter' && this.tool === 'line' && this.draw) { this.finishLine(false); return true; }
    if ((k === 'Delete' || k === 'Backspace') && this.sel.size) { this.deleteSelected(); return true; }
    const map = { l: 'line', a: 'arc3', r: 'rect', c: 'circle', p: 'polygon', s: 'slot', t: 'trim' };
    if (!ev.ctrlKey && map[k.toLowerCase()]) { this.setTool(map[k.toLowerCase()]); return true; }
    if (ev.ctrlKey && k.toLowerCase() === 'z') { this.undoLocal(); return true; }
    return false;
  }

  onMove(ev) {
    const uv = this.eventUV(ev);
    if (!uv) return;
    this.cursor = uv;
    this._lastEv = ev;
    if (this.draw) { if (!this.typed) this.updateDraw(); this.placeHud(ev); }
    else if (this.tool === 'select') {
      const id = this.entityAt(uv, ev);
      if (id !== this.hoverId) { this.hoverId = id; this.redraw(); }
    }
    this.drawCursor(uv);
    this.app.status(`Sketch  X ${U.fmt(uv[0])}  Y ${U.fmt(uv[1])} ${U.label}${this.snapped ? '  · snapped' : ''}`);
    if (this._drag) this.dragMove(uv);
  }

  entityAt(uv, ev) {
    const px = this.viewer.pixelSize(this.to3(...uv).toArray());
    let best = null, bd = 8 * px;
    for (const e of this.entities) { const d = distToEntity(e, uv); if (d < bd) { bd = d; best = e.id; } }
    return best;
  }

  onClick(ev) {
    const uv = this.eventUV(ev);
    if (!uv) return;
    const t = this.tool;
    if (t === 'trim') {
      const id = this.entityAt(uv, ev);
      if (!id) return;
      this.app.sketchServerOp({ op: 'trim', id, point: uv }).catch(e => this.app.toast(e.message, 'err'));
      return;
    }
    if (t === 'sk_mirror') {
      const id = this.entityAt(uv, ev);
      const ent = this.entities.find(x => x.id === id);
      if (!ent || !['line', 'polyline'].includes(ent.type)) { this.app.toast('Click a straight line to mirror across.', 'warn'); return; }
      if (!this.sel.size) { this.app.toast('First select the shapes to mirror (Select tool), then choose Mirror and click the mirror line.', 'warn'); return; }
      this.app.sketchServerOp({ op: 'mirror', ids: [...this.sel], axis: id }).catch(e => this.app.toast(e.message, 'err'));
      return;
    }
    if (t.startsWith('c_')) return this.constraintClick(uv, ev);
    if (t === 'text') {
      this.setTool('select');
      this.app.sketchText({ x: +uv[0].toFixed(3), y: +uv[1].toFixed(3) });
      return;
    }
    if (t === 'select') {
      const id = this.entityAt(uv, ev);
      this.selC = null;
      const ids = id ? this.groupOf(id) : [];
      if (ev.shiftKey && id) { const s = new Set(this.sel); const on = s.has(id); for (const g of ids) on ? s.delete(g) : s.add(g); this.setSel([...s]); }
      else this.setSel(ids);
      return;
    }
    if (!this.draw) {
      this.draw = { pts: [uv], refs: [this.snapRef] };
      this.typed = false;
      this.inA.value = ''; this.inB.value = '';
      this.showHud(ev);
      this.updateDraw();
      return;
    }
    if (t === 'line') return this.addLinePoint(this.draw.preview?.p2 || uv);
    if (t === 'arc3') {
      if (this.draw.pts.length === 1) {
        const p = this.draw.preview?.p2 || uv;
        if (Math.hypot(p[0] - this.draw.pts[0][0], p[1] - this.draw.pts[0][1]) < 1e-9) return;
        this.draw.pts.push(p); this.draw.refs.push(this.snapRef); this.typed = false; this.inA.value = ''; this.updateDraw(); return;
      }
      if (this.draw.preview?.type !== 'arc') { this.app.toast('The three points are in a line.', 'warn'); return; }
      this.draw.refs.push(null);
      return this.commit();
    }
    if (t === 'slot' && this.draw.pts.length === 1) {
      this.draw.pts.push(this.draw.preview ? [this.draw.preview.x2, this.draw.preview.y2] : uv);
      this.draw.refs.push(this.snapRef);
      this.typed = false; this.inA.value = ''; this.inB.value = '';
      this.updateDraw();
      return;
    }
    this.draw.refs.push(this.snapRef);
    this.commit();
  }

  // ------------------------------------------------------------------- drawing
  updateDraw() {
    const d = this.draw; if (!d) return;
    const c = this.cursor || d.pts[0];
    const bIsLen = this.tool === 'rect' || this.tool === 'rect_center';
    const Araw = parseExpr(this.inA.value), Braw = parseExpr(this.inB.value);
    const A = Araw * U.factor, B = bIsLen ? Braw * U.factor : Braw;
    const hasA = isFinite(A), hasB = isFinite(B);
    const p0 = d.pts[0];
    let e = null;
    switch (this.tool) {
      case 'line': {
        const last = d.pts.at(-1);
        let dx = c[0] - last[0], dy = c[1] - last[1];
        let L = Math.hypot(dx, dy), ang = Math.atan2(dy, dx) * 180 / Math.PI;
        if (hasB) ang = B;
        if (hasA) L = A;
        const end = (hasA || hasB) ? [last[0] + L * Math.cos(ang * Math.PI / 180), last[1] + L * Math.sin(ang * Math.PI / 180)] : c;
        e = { type: 'line', p1: [...last], p2: end };
        this.setHud('Length', U.fmt(L), 'Angle', fmt(ang));
        break;
      }
      case 'arc3': {
        if (d.pts.length === 1) {
          e = { type: 'line', p1: [...p0], p2: c };
          this.setHud('Chord', U.fmt(Math.hypot(c[0] - p0[0], c[1] - p0[1])));
        } else {
          const [a, b] = d.pts;
          let mid = c;
          if (hasA) { // typed radius: put the arc on the cursor's side of the chord
            const mx = (a[0] + b[0]) / 2, my = (a[1] + b[1]) / 2, ch = Math.hypot(b[0] - a[0], b[1] - a[1]) / 2;
            const R = Math.max(Math.abs(A), ch + 1e-6), sag = R - Math.sqrt(R * R - ch * ch);
            const nx = -(b[1] - a[1]) / (2 * ch), ny = (b[0] - a[0]) / (2 * ch);
            const side = Math.sign((c[0] - mx) * nx + (c[1] - my) * ny) || 1;
            mid = [mx + nx * sag * side, my + ny * sag * side];
          }
          e = arcFrom3(a, mid, b) || { type: 'line', p1: a, p2: b };
          this.setHud('Radius', U.fmt(e.r || 0));
        }
        break;
      }
      case 'rect': {
        let w = c[0] - p0[0], h = c[1] - p0[1];
        if (hasA) w = Math.sign(w || 1) * Math.abs(A);
        if (hasB) h = Math.sign(h || 1) * Math.abs(B);
        e = { type: 'rect', x: p0[0], y: p0[1], w, h };
        this.setHud('Width', U.fmt(Math.abs(w)), 'Height', U.fmt(Math.abs(h)));
        break;
      }
      case 'rect_center': {
        let w = Math.abs(c[0] - p0[0]) * 2, h = Math.abs(c[1] - p0[1]) * 2;
        if (hasA) w = Math.abs(A); if (hasB) h = Math.abs(B);
        e = { type: 'rect', x: p0[0] - w / 2, y: p0[1] - h / 2, w, h };
        this.setHud('Width', U.fmt(w), 'Height', U.fmt(h));
        break;
      }
      case 'circle': {
        let r = Math.hypot(c[0] - p0[0], c[1] - p0[1]);
        if (hasA) r = Math.abs(A) / 2;
        e = { type: 'circle', cx: p0[0], cy: p0[1], r };
        this.setHud('Diameter', U.fmt(r * 2));
        break;
      }
      case 'polygon': {
        let r = Math.hypot(c[0] - p0[0], c[1] - p0[1]);
        const ang = Math.atan2(c[1] - p0[1], c[0] - p0[0]) * 180 / Math.PI;
        if (hasA) r = Math.abs(A);
        const n = hasB ? Math.max(3, Math.round(B)) : (this.polySides || 6);
        e = { type: 'polygon', cx: p0[0], cy: p0[1], r, n, angle: hasA ? (d.angle ?? 90) : ang };
        this.setHud('Radius', U.fmt(r), 'Sides', String(n));
        break;
      }
      case 'slot': {
        if (d.pts.length === 1) {
          let dx = c[0] - p0[0], dy = c[1] - p0[1];
          let L = Math.hypot(dx, dy), ang = Math.atan2(dy, dx) * 180 / Math.PI;
          if (hasA) L = A; if (hasB) ang = B;
          const x2 = p0[0] + L * Math.cos(ang * Math.PI / 180), y2 = p0[1] + L * Math.sin(ang * Math.PI / 180);
          e = { type: 'slot', x1: p0[0], y1: p0[1], x2, y2, r: Math.max(L * 0.15, 0.5) };
          this.setHud('Length', U.fmt(L), 'Angle', fmt(ang));
        } else {
          const [p1, p2] = d.pts;
          const seg = [p2[0] - p1[0], p2[1] - p1[1]], Ls = Math.hypot(...seg) || 1;
          let r = Math.abs((c[0] - p1[0]) * seg[1] - (c[1] - p1[1]) * seg[0]) / Ls;
          if (hasA) r = Math.abs(A) / 2;
          e = { type: 'slot', x1: p1[0], y1: p1[1], x2: p2[0], y2: p2[1], r: Math.max(r, 1e-3) };
          this.setHud('Width', U.fmt(r * 2));
        }
        break;
      }
    }
    d.preview = e;
    this.redraw();
  }

  commitTyped() {
    if (!this.draw) return;
    this.updateDraw();
    if (this.tool === 'line') {
      this.addLinePoint(this.draw.preview.p2);
      if (this.draw) this.inA.focus();
      return;
    }
    if (this.tool === 'arc3' && this.draw.pts.length === 1) {
      this.draw.pts.push(this.draw.preview.p2); this.typed = false; this.inA.value = ''; this.inA.focus(); this.updateDraw(); return;
    }
    if (this.tool === 'slot' && this.draw.pts.length === 1) {
      this.draw.pts.push([this.draw.preview.x2, this.draw.preview.y2]);
      this.typed = false; this.inA.value = ''; this.inB.value = '';
      this.inA.focus();
      this.updateDraw();
      return;
    }
    this.commit();
  }

  commit() {
    const e = this.draw?.preview;
    if (!e) return this.cancelDraw();
    const bad = (e.type === 'rect' && (Math.abs(e.w) < 1e-6 || Math.abs(e.h) < 1e-6)) || ((e.type === 'circle' || e.type === 'polygon') && e.r < 1e-6);
    if (bad) { this.app.toast('Too small: give it a size.', 'warn'); return; }
    if (e.type === 'polygon') this.polySides = e.n;
    this.push();
    e.id = newId();
    this.entities.push(e);
    this.autoConstrain(e, this.draw.refs || []);
    this.draw = null; this.typed = false;
    this.hideHud();
    this.setSel([e.id]);
  }

  // Line tool: every click adds a separate line segment (so lines can be trimmed and joined
  // into closed regions); clicking the chain's first point closes it.
  addLinePoint(p) {
    const d = this.draw;
    const last = d.pts.at(-1), first = d.pts[0];
    if (Math.hypot(p[0] - last[0], p[1] - last[1]) < 1e-9) return;
    this.push();
    const e = { id: newId(), type: 'line', p1: [...last], p2: [...p] };
    this.entities.push(e);
    const auto = [];
    if (d.lastLine) auto.push({ type: 'coincident', a: { e: d.lastLine, k: 'p2' }, b: { e: e.id, k: 'p1' } });
    else if (d.refs?.[0]) auto.push({ type: 'coincident', a: { e: e.id, k: 'p1' }, b: d.refs[0] });
    const closing = d.pts.length >= 2 && Math.hypot(p[0] - first[0], p[1] - first[1]) < 1e-9;
    if (closing && d.firstLine) auto.push({ type: 'coincident', a: { e: e.id, k: 'p2' }, b: { e: d.firstLine, k: 'p1' } });
    else if (this.snapRef && this.snapRef.e !== e.id) auto.push({ type: 'coincident', a: { e: e.id, k: 'p2' }, b: this.snapRef });
    if (Math.abs(p[1] - last[1]) < 1e-9) auto.push({ type: 'horizontal', a: { e: e.id } });
    else if (Math.abs(p[0] - last[0]) < 1e-9) auto.push({ type: 'vertical', a: { e: e.id } });
    for (const c of auto) this.constraints.push({ ...c, id: newId(), auto: true });
    d.firstLine = d.firstLine || e.id;
    d.lastLine = e.id;
    d.pts.push([...p]);
    d.made = (d.made || 0) + 1;
    this.typed = false; this.inA.value = ''; this.inB.value = '';
    if (d.pts.length >= 3 && Math.hypot(p[0] - first[0], p[1] - first[1]) < 1e-9) return this.finishLine(true);
    this.updateDraw();
    this.app.onSketchChanged?.();
  }

  finishLine() {
    const d = this.draw; if (!d) return;
    this.draw = null; this.hideHud();
    this.redraw(); this.app.onSketchChanged?.();
  }

  replaceEntities(ents) {
    this.push();
    this.entities = ents;
    const E = Object.fromEntries(ents.map(e => [e.id, e]));
    this.constraints = this.constraints.filter(c => refsExist(c, E));
    const ids = new Set(ents.map(e => e.id));
    this.sel = new Set([...this.sel].filter(i => ids.has(i)));
    this.redraw(); this.app.onSketchChanged?.();
  }

  cancelDraw() { this.draw = null; this.typed = false; this.hideHud(); this.redraw(); }

  push() { this.history.push(JSON.stringify({ e: this.entities, c: this.constraints })); if (this.history.length > 100) this.history.shift(); }
  undoLocal() {
    if (!this.history.length) return;
    const h = JSON.parse(this.history.pop());
    this.entities = h.e; this.constraints = h.c || [];
    this.sel.clear(); this.selC = null; this.redraw(); this.app.onSketchChanged?.();
  }

  // ------------------------------------------------------------------- constraints
  pickRef(uv, pointsFirst = true) {
    const px = this.viewer.pixelSize(this.to3(...uv).toArray());
    let best = null, bd = 9 * px;
    for (const e of this.entities) for (const k of pointKeys(e)) {
      if (k === 'm') continue;
      const q = cpoint(e, k), d = Math.hypot(q[0] - uv[0], q[1] - uv[1]);
      if (d < bd) { bd = d; best = { e: e.id, k }; }
    }
    if (best && pointsFirst) return best;
    const id = this.entityAt(uv);
    return id ? { e: id } : best;
  }

  async constraintClick(uv, ev) {
    const raw = this.viewer.rayToPlane(ev, this.frameArr);
    const pick = this.pickRef(raw ? this.toUV(raw) : uv);
    if (!pick) return;
    this.cpicks.push(pick);
    const c = makeConstraint(this.tool, this.cpicks, this.entities);
    if (c === null) { this.redraw(); this.app.hint(CTOOLS[this.tool].hint + ` · ${this.cpicks.length} picked`); return; }
    this.cpicks = [];
    if (typeof c === 'string') { this.app.toast(c, 'warn'); this.redraw(); return; }
    if (CTYPES[c.type].dim) {
      const cur = measure(c, this.entities) ?? 0;
      const isAng = CTYPES[c.type].angle;
      const txt = await this.app.prompt(`${CTYPES[c.type].label}${isAng ? ' (°)' : ` (${U.label})`}`, isAng ? String(Math.round(cur * 100) / 100) : U.fmt(cur));
      if (txt == null) { this.redraw(); return; }
      const v = isAng ? parseExpr(txt) : U.parse(txt);
      if (!(v > 0) && !(isAng && v === 0)) { this.app.toast('Enter a positive number.', 'warn'); return; }
      c.value = v;
    }
    this.addConstraint(c);
  }

  addConstraint(c) {
    this.push();
    const cc = { ...c, id: newId() };
    const r = solve(this.entities, [...this.constraints, cc]);
    if (!r.ok) {
      this.history.pop();
      this.app.toast('That would conflict with the existing constraints (over-constrained).', 'warn');
      this.redraw();
      return false;
    }
    this.constraints.push(cc);
    this.entities = r.entities;
    this.redraw(); this.app.onSketchChanged?.();
    return true;
  }

  autoConstrain(e, refs) {
    // constraints for the points the new shape was snapped to while drawing
    const keysFor = {
      rect: this.tool === 'rect_center' ? ['c', null] : ['c0', 'c2'], circle: ['c', null], polygon: ['c', null],
      slot: ['s', 'e', null], arc: ['s', 'e', null],
    }[e.type] || [];
    refs.forEach((ref, i) => {
      let k = keysFor[i];
      if (!ref || !k) return;
      if (e.type === 'arc') { // the arc may run either way: pair each pick with its nearest end
        const target = cpoint(this.entities.find(x => x.id === ref.e), ref.k);
        const ds = ['s', 'e'].map(kk => { const q = cpoint(e, kk); return Math.hypot(q[0] - target[0], q[1] - target[1]); });
        k = ds[0] <= ds[1] ? 's' : 'e';
      }
      this.constraints.push({ id: newId(), type: 'coincident', a: { e: e.id, k }, b: ref, auto: true });
    });
    const r = solve(this.entities, this.constraints);
    if (r.ok) this.entities = r.entities;
  }

  solveAll(locks = []) {
    const r = solve(this.entities, this.constraints, locks);
    this.entities = r.entities;
    return r.ok;
  }

  deleteConstraint(id) {
    this.push();
    this.constraints = this.constraints.filter(c => c.id !== id);
    this.selC = null; this.redraw(); this.app.onSketchChanged?.();
  }

  setDimValue(id, v) {
    const c = this.constraints.find(x => x.id === id); if (!c) return;
    this.push();
    const old = c.value; c.value = v;
    const r = solve(this.entities, this.constraints);
    if (!r.ok) { c.value = old; this.history.pop(); this.app.toast('That value conflicts with other constraints.', 'warn'); return; }
    this.entities = r.entities; this.redraw(); this.app.onSketchChanged?.();
  }

  // dragging points or whole shapes with the Select tool; constraints follow
  dragStart(uv) {
    const ref = this.pickRef(uv);
    if (!ref) return false;
    this._drag = { ref, start: uv, base: JSON.parse(JSON.stringify(this.entities)), moved: false };
    return true;
  }
  dragMove(uv) {
    const d = this._drag; if (!d) return;
    const dx = uv[0] - d.start[0], dy = uv[1] - d.start[1];
    if (!d.moved && Math.hypot(dx, dy) < 1e-9) return;
    if (!d.moved) { d.moved = true; this.history.push(JSON.stringify({ e: d.base, c: this.constraints })); }
    const baseEnt = d.base.find(e => e.id === d.ref.e);
    let locks;
    if (d.ref.k) locks = [{ e: d.ref.e, k: d.ref.k, at: uv }];
    else locks = pointKeys(baseEnt).filter(k => k !== 'm' && !(baseEnt.type === 'rect' && k === 'c')).map(k => { const q = cpoint(baseEnt, k); return { e: d.ref.e, k, at: [q[0] + dx, q[1] + dy] }; });
    const r = solve(d.base, this.constraints, locks);
    this.entities = r.entities;
    this.redraw();
  }
  dragEnd() { const d = this._drag; this._drag = null; if (d?.moved) this.app.onSketchChanged?.(); return d?.moved; }

  setSel(ids) { this.sel = new Set(ids); this.redraw(); this.app.onSketchChanged?.(); }

  deleteSelected() {
    if (this.selC) return this.deleteConstraint(this.selC);
    if (!this.sel.size) return;
    this.push();
    this.entities = this.entities.filter(e => !this.sel.has(e.id));
    const E = Object.fromEntries(this.entities.map(e => [e.id, e]));
    this.constraints = this.constraints.filter(c => refsExist(c, E));
    this.sel.clear(); this.redraw(); this.app.onSketchChanged?.();
  }

  updateEntity(id, patch) {
    const e = this.entities.find(x => x.id === id); if (!e) return;
    this.push(); Object.assign(e, patch);
    if (this.constraints.length) {
      // hold what was just typed, let constraints move everything else
      const locks = [];
      for (const [k, v] of Object.entries(patch)) {
        if (typeof v === 'number') locks.push({ e: id, path: k, value: v });
        else if (Array.isArray(v) && typeof v[0] === 'number') v.forEach((x, i) => locks.push({ e: id, path: `${k}.${i}`, value: x }));
      }
      if (!this.solveAll(locks)) this.app.toast('Some constraints could not be kept.', 'warn');
    }
    this.redraw(); this.app.onSketchChanged?.(true);
  }

  toggleConstruction() {
    if (!this.sel.size) return;
    this.push();
    for (const e of this.entities) if (this.sel.has(e.id)) e.construction = !e.construction;
    this.redraw(); this.app.onSketchChanged?.();
  }

  // ------------------------------------------------------------------- HUD
  showHud(ev) { this.hud.classList.remove('hidden'); this.placeHud(ev); }
  hideHud() { this.hud.classList.add('hidden'); if ([this.inA, this.inB].includes(document.activeElement)) document.activeElement.blur(); }
  placeHud(ev) {
    const r = this.viewer.canvas.getBoundingClientRect();
    this.hud.style.left = `${Math.min(ev.clientX - r.left + 18, r.width - 150)}px`;
    this.hud.style.top = `${Math.min(ev.clientY - r.top + 14, r.height - 70)}px`;
  }
  setHud(la, va, lb, vb) {
    document.getElementById('dyn-a-label').textContent = la;
    if (document.activeElement !== this.inA && !this.typed) this.inA.placeholder = va;
    const rowB = this.inB.parentElement;
    if (lb) { rowB.style.display = ''; document.getElementById('dyn-b-label').textContent = lb; if (document.activeElement !== this.inB && !this.typed) this.inB.placeholder = vb; }
    else rowB.style.display = 'none';
  }

  // ------------------------------------------------------------------- rendering
  _buildPlaneGrid() {
    this.gridGroup = new THREE.Group();
    const box = this.viewer.sceneBox();
    const R = box.isEmpty() ? 100 : Math.max(box.getSize(new THREE.Vector3()).length(), 40);
    const step = R > 400 ? 10 : R > 80 ? 5 : 1;
    const half = Math.ceil(R / step) * step;
    const pts = [], ptsMajor = [];
    for (let v = -half; v <= half; v += step) {
      const arr = Math.round(v / step) % 5 === 0 ? ptsMajor : pts;
      arr.push(this.to3(-half, v), this.to3(half, v), this.to3(v, -half), this.to3(v, half));
    }
    const mk = (p, op) => new THREE.LineSegments(new THREE.BufferGeometry().setFromPoints(p), new THREE.LineBasicMaterial({ color: 0xc58a68, transparent: true, opacity: op, depthWrite: false }));
    this.gridGroup.add(mk(pts, 0.16), mk(ptsMajor, 0.32));
    const ax = (a, b, c) => { const l = new THREE.Line(new THREE.BufferGeometry().setFromPoints([a, b]), new THREE.LineBasicMaterial({ color: c, depthTest: false })); l.renderOrder = 9; return l; };
    this.gridGroup.add(ax(this.to3(-half, 0), this.to3(half, 0), 0xe25555), ax(this.to3(0, -half), this.to3(0, half), 0x22a35a));
    // translucent sheet so the plane reads as a surface
    const [o, x, y, n] = this.frame;
    const sheet = new THREE.Mesh(new THREE.PlaneGeometry(half * 2, half * 2), new THREE.MeshBasicMaterial({ color: 0xfff1e8, transparent: true, opacity: 0.18, side: THREE.DoubleSide, depthWrite: false }));
    sheet.applyMatrix4(new THREE.Matrix4().makeBasis(x, y, n).setPosition(o));
    this.gridGroup.add(sheet);
  }

  redraw() {
    this.group.clear();
    if (!this.active) return;
    this.group.add(this.gridGroup);
    if (this.refs.length) {
      const P = [];
      for (const [a, b] of this.refs) P.push(this.to3(...a), this.to3(...b));
      const ref = new THREE.LineSegments(new THREE.BufferGeometry().setFromPoints(P), new THREE.LineBasicMaterial({ color: COL_REF, depthTest: false, transparent: true, opacity: 0.85 }));
      ref.renderOrder = 9; this.group.add(ref);
    }
    const picked = new Set((this.cpicks || []).map(p => p.e));
    const inSelC = new Set();
    if (this.selC) { const c = this.constraints.find(x => x.id === this.selC); for (const r of [c?.a, c?.b, c?.c]) if (r) inSelC.add(r.e); }
    for (const e of this.entities) {
      const hl = this.sel.has(e.id) || picked.has(e.id) || inSelC.has(e.id);
      this.group.add(this._line(e, hl ? COL_SEL : (this.hoverId === e.id ? 0xf08c4a : COL), hl ? 1 : 0.95));
    }
    if (this.draw?.preview) this.group.add(this._line(this.draw.preview, COL_PREV, 1));
    // closed profile fill (helps see what will extrude)
    this.group.add(this._fill());
    this.viewer.requestRender();
    this.buildDims();
  }

  // ------------------------------------------------------------------- dimensions
  buildDims() {
    const drivenEnts = new Set(this.constraints.filter(c => CTYPES[c.type]?.dim && !c.b).map(c => c.a.e));
    const list = this.entities.length <= 40 ? this.entities : this.entities.filter(e => this.sel.has(e.id));
    this.dimItems = [];
    let h = '';
    for (const e of list) {
      if ((e.construction && !this.sel.has(e.id)) || drivenEnts.has(e.id)) continue;
      for (const d of dimensions(e)) {
        const i = this.dimItems.length;
        const label = d.label.replace(/[\d.]+/, () => U.fmt(d.value));
        this.dimItems.push({ e, d });
        h += `<div class="dim ${this.sel.has(e.id) ? 'sel' : ''}" data-i="${i}" title="Click to change">${label}</div>`;
      }
    }
    // driving dimensions and constraint glyphs
    for (const c of this.constraints) {
      const spots = glyphSpots(c, this.entities);
      if (!spots.length) continue;
      const T = CTYPES[c.type]; if (!T) continue;
      if (T.dim) {
        const at = spots.length > 1 ? [(spots[0][0] + spots[1][0]) / 2, (spots[0][1] + spots[1][1]) / 2] : spots[0];
        const txt = T.angle ? `${Math.round(c.value * 100) / 100}°` : (c.type === 'diameter' ? '⌀' : c.type === 'radius' ? 'R' : '') + U.fmt(c.value);
        const i = this.dimItems.length;
        this.dimItems.push({ c, at });
        h += `<div class="dim driving ${this.selC === c.id ? 'sel' : ''}" data-i="${i}" title="${T.label} (driving) · click to change">${txt}</div>`;
      } else if (c.type !== 'coincident' || this.selC === c.id) {
        spots.forEach((at, si) => {
          const i = this.dimItems.length;
          this.dimItems.push({ c, at, glyph: true, off: si });
          h += `<div class="cg ${this.selC === c.id ? 'sel' : ''}" data-i="${i}" title="${T.label} · click to select, Delete to remove">${T.glyph}</div>`;
        });
      }
    }
    this.dims.innerHTML = h;
    // while drawing or constraining, labels must not swallow clicks meant for the geometry
    this.dims.classList.toggle('passive', this.pickingTool());
    this.placeDims();
    this.dims.onclick = ev => {
      const el = ev.target.closest('[data-i]'); if (!el) return;
      const it = this.dimItems[+el.dataset.i];
      if (it.glyph) { this.selC = this.selC === it.c.id ? null : it.c.id; this.sel.clear(); this.redraw(); this.app.onSketchChanged?.(); return; }
      this.editDim(+el.dataset.i, el);
    };
  }

  placeDims() {
    if (!this.dimItems) return;
    const els = this.dims.children;
    for (let i = 0; i < els.length; i++) {
      const it = this.dimItems[i];
      const d = it.d || { at: it.at };
      const [x, y] = this.viewer.worldToScreen(this.to3(...d.at).toArray());
      const gx = it.glyph ? 12 + it.off * 2 : 0, gy = it.glyph ? -12 : (it.c ? -14 : (it.e?.type === 'line' ? -13 : 0));
      els[i].style.left = `${x + gx}px`;
      els[i].style.top = `${y + gy + (d.side === 'below' ? 12 : 0)}px`;
      els[i].style.transform = d.side === 'right' ? 'translate(8px,-50%)' : 'translate(-50%,-50%)';
    }
  }

  editDim(i, el) {
    const item = this.dimItems[i];
    const inp = document.createElement('input');
    inp.className = 'dim-edit'; inp.style.cssText = el.style.cssText; inp.type = 'text';
    if (item.c) {
      const isAng = CTYPES[item.c.type].angle;
      inp.value = isAng ? String(item.c.value) : U.fmt(item.c.value);
      el.replaceWith(inp); inp.focus(); inp.select();
      const done = ok => {
        if (ok) { const v = isAng ? parseExpr(inp.value) : U.parse(inp.value); if (v > 0 || (isAng && v === 0)) { this.setDimValue(item.c.id, v); return; } }
        this.buildDims();
      };
      inp.addEventListener('keydown', ev => { ev.stopPropagation(); if (ev.key === 'Enter') done(true); if (ev.key === 'Escape') done(false); });
      inp.addEventListener('blur', () => done(true), { once: true });
      return;
    }
    const { e, d } = item;
    inp.value = U.fmt(d.value);
    el.replaceWith(inp); inp.focus(); inp.select();
    const done = ok => {
      if (ok) {
        const v = U.parse(inp.value);
        const patch = applyDimension(e, d.key, v);
        if (patch) { this.updateEntity(e.id, patch); return; }
        this.app.toast('Enter a positive number.', 'warn');
      }
      this.buildDims();
    };
    inp.addEventListener('keydown', ev => { ev.stopPropagation(); if (ev.key === 'Enter') done(true); if (ev.key === 'Escape') done(false); });
    inp.addEventListener('blur', () => done(true), { once: true });
  }

  _line(e, color, opacity) {
    const { pts, closed } = entityOutline(e);
    const P = pts.map(p => this.to3(...p)); if (closed && P.length) P.push(P[0]);
    const mat = e.construction ? new THREE.LineDashedMaterial({ color: 0x8a6a55, dashSize: 1.2, gapSize: 0.8, depthTest: false }) : new THREE.LineBasicMaterial({ color, depthTest: false, transparent: true, opacity });
    const ln = new THREE.Line(new THREE.BufferGeometry().setFromPoints(P), mat);
    if (e.construction) ln.computeLineDistances();
    ln.renderOrder = 11;
    const g = new THREE.Group(); g.add(ln);
    if (e.type === 'polyline' || this.sel.has(e.id)) {
      for (const p of (e.type === 'polyline' ? e.pts : keyPoints(e).slice(0, 1))) {
        const dot = new THREE.Points(new THREE.BufferGeometry().setFromPoints([this.to3(...p)]), new THREE.PointsMaterial({ color, size: 6, sizeAttenuation: false, depthTest: false }));
        dot.renderOrder = 12; g.add(dot);
      }
    }
    return g;
  }

  _fill() {
    const g = new THREE.Group();
    const shapes = [];
    for (const e of this.entities) {
      if (e.construction) continue;
      const { pts, closed } = entityOutline(e);
      if (!closed || pts.length < 3) continue;
      shapes.push(pts);
    }
    if (!shapes.length) return g;
    // even-odd fill: draw each contour into the stencil-free way by nesting depth
    const [o, x, y, n] = this.frame;
    for (const pts of shapes) {
      const depth = shapes.filter(s => s !== pts && inside(pts[0], s)).length;
      if (depth % 2) continue;
      const shape = new THREE.Shape(pts.map(p => new THREE.Vector2(...p)));
      for (const s of shapes) if (s !== pts && inside(s[0], pts) && shapes.filter(q => q !== s && inside(s[0], q)).length === depth + 1) shape.holes.push(new THREE.Path(s.map(p => new THREE.Vector2(...p))));
      const m = new THREE.Mesh(new THREE.ShapeGeometry(shape), new THREE.MeshBasicMaterial({ color: 0xf6c7a8, transparent: true, opacity: 0.38, side: THREE.DoubleSide, depthWrite: false, depthTest: false }));
      m.applyMatrix4(new THREE.Matrix4().makeBasis(x, y, n).setPosition(o));
      m.renderOrder = 8;
      g.add(m);
    }
    return g;
  }

  drawCursor(uv) {
    if (!this._cursorObj) {
      this._cursorObj = new THREE.Points(new THREE.BufferGeometry().setFromPoints([new THREE.Vector3()]), new THREE.PointsMaterial({ color: 0x1f6fd1, size: 8, sizeAttenuation: false, depthTest: false }));
      this._cursorObj.renderOrder = 30;
    }
    const p = this.to3(...uv);
    this._cursorObj.geometry.setFromPoints([p]);
    this._cursorObj.material.color.set(this.snapped ? 0x2f9e44 : 0x1f6fd1);
    if (!this.group.children.includes(this._cursorObj)) this.group.add(this._cursorObj);
    this.viewer.requestRender();
  }

  // ------------------------------------------------------------------- panel
  panelHTML() {
    const rows = this.entities.map(e => `<div class="ent ${this.sel.has(e.id) ? 'sel' : ''}" data-id="${e.id}"><span>${ENTITY_LABEL[e.type] || e.type}${e.construction ? ' (construction)' : ''}</span><span class="t">${describe(e)}</span></div>`).join('');
    return `<div class="ent-list">${rows || '<div class="ent"><span class="t">No shapes yet — pick a tool above.</span></div>'}</div>`;
  }

  // text is stored as one closed outline per letter part; they select together
  groupOf(id) {
    const e = this.entities.find(x => x.id === id);
    return e?.group ? this.entities.filter(x => x.group === e.group).map(x => x.id) : [id];
  }

  selectedText() {
    const sel = this.entities.filter(e => this.sel.has(e.id));
    if (!sel.length || !sel[0].group || !sel[0].text) return null;
    return sel.every(e => e.group === sel[0].group) ? { group: sel[0].group, meta: sel[0].text } : null;
  }

  selectedEntity() { return this.sel.size === 1 ? this.entities.find(e => this.sel.has(e.id)) : null; }
}

function inside(p, poly) {
  let c = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [xi, yi] = poly[i], [xj, yj] = poly[j];
    if (((yi > p[1]) !== (yj > p[1])) && p[0] < (xj - xi) * (p[1] - yi) / (yj - yi) + xi) c = !c;
  }
  return c;
}
