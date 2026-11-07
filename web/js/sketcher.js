// Sketch mode: draw 2D entities on a plane, with snapping and typed dimensions.
import * as THREE from 'three';
import { entityOutline, keyPoints, distToEntity, describe, ENTITY_LABEL, arcFrom3, segDist as segD } from './sketchgeom.js';
import { solve, makeConstraint, measure, CTYPES, CTOOLS, glyphSpots, pointKeys, point as cpoint, refsExist, inferDimension, angleTo, analyze, isRedundant, unsatisfied } from './constraints.js';
import { layout, placeAt, defaultPlacement } from './dims.js';
import { U, parseExpr } from './units.js';
import { exprText, exprOf, resolveDeep } from './expr.js';

// Sketch colours, as in SOLIDWORKS: black when a shape is fully defined (its size and place are
// set by dimensions and relations), blue while it can still move, red when relations conflict.
const COL_UNDER = 0x1c6fd6, COL_BAD = 0xe03131, COL_SEL = 0x0ca678, COL_HOVER = 0x38c99a, COL_PREV = 0xe8590c;
const COL_EDGE = 0xf76707;   // part edges (reference geometry)
const dark = () => document.documentElement.dataset.theme === 'dark';
const colDef = () => (dark() ? 0xe9ecef : 0x1d2433);
const AXIS = 1e5;
// what every sketch can measure from: the origin and the two axes (they never move)
export const baseRefs = () => [
  { id: '_o', type: 'point', x: 0, y: 0, ref: 'origin' },
  { id: '_h', type: 'line', p1: [-AXIS, 0], p2: [AXIS, 0], ref: 'axis' },
  { id: '_v', type: 'line', p1: [0, -AXIS], p2: [0, AXIS], ref: 'axis' },
];
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

  start({ frame, entities = [], refs = [], savedRefs = [], refKind = null, featureId = null, planeRef, name, bodyId, constraints = [] }) {
    this.active = true;
    this.constraints = JSON.parse(JSON.stringify(constraints || []));
    this.selC = null;
    this.cpicks = [];
    this.dimPend = null;
    this.hoverPick = null;
    this._defKey = null;
    this.refKind = refKind;
    this.setRefs(refs, savedRefs);
    this.frame = frame.map(a => new THREE.Vector3(...a));
    this.frameArr = frame;
    this.entities = JSON.parse(JSON.stringify(entities));
    this.featureId = featureId;
    this.planeRef = planeRef;
    this.bodyId = bodyId;
    this.name = name;
    this.sel = new Set();
    this.tool = 'select';
    this.draw = null;
    this.history = [];
    if (this.constraints.length) { const r = solve(this.entities, this.constraints, [], this.refEnts); if (r.ok) this.entities = r.entities; }
    this.viewer.setMouseMode('sketch');
    this.viewer.lookAtPlane(frame);
    this._buildPlaneGrid();
    this.redraw();
    this.app.onSketchChanged?.();
  }

  stop() {
    this.active = false;
    this.draw = null;
    this.dimPend = null;
    this.closeModify?.();
    this.dims.innerHTML = '';
    this._clearGroup(true); this.gridGroup = null;
    this.hideHud();
    this.viewer.setMouseMode('model');
    this.viewer.requestRender();
  }

  setTool(t) {
    this.cancelDraw();
    this.closeModify();
    this.tool = t;
    this.cpicks = [];
    this.dimPend = null;
    this.hoverPick = null;
    this.dims.classList.toggle('passive', this.pickingTool());
    this.app.onSketchChanged?.();
    this.app.hint(this.toolHint());
  }

  // tools whose clicks pick geometry: dimension labels must let those clicks through
  pickingTool() {
    if (this.tool === 'c_dimension') return !!this.dimPend?.picks.length;
    return this.tool.startsWith('c_') || this.tool === 'trim' || this.tool === 'sk_mirror' || this.tool === 'sk_useedge' || !!this.draw;
  }

  toolHint() {
    return {
      select: 'Click a shape to select it · Delete removes it · Right-drag to orbit, middle-drag to pan',
      line: 'Click points to draw connected lines · click the first point to close · Enter or Esc to stop · type a length',
      arc3: 'Click the start, then the end, then a point the arc passes through · or type a radius',
      trim: 'Click the piece of a line, arc or shape to cut away (up to where it crosses something)',
      sk_mirror: 'Select shapes first, then click the straight line to mirror them across',
      sk_useedge: 'Click an orange part edge to turn it into a sketch line or circle (it stays tied to the edge)',
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
    const cands = [];
    for (const e of this.entities) {
      for (const k of pointKeys(e)) cands.push({ p: cpoint(e, k), ref: { e: e.id, k } });
      for (const q of keyPoints(e)) cands.push({ p: q });
    }
    // the origin and the part's corners and circle centres (snapping there ties the point to them)
    for (const e of this.refEnts) if (e.ref !== 'axis') for (const k of pointKeys(e)) cands.push({ p: cpoint(e, k), ref: { e: e.id, k } });
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
      // fingers and pens don't hover, so take the point from the tap itself
      if (!moved && !dragged) { if (ev.pointerType !== 'mouse') this.onMove(ev); this.onClick(ev); }
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
    if (k === 'Escape' && this.dimPend) { this.dimPend = null; this.redraw(); this.app.hint(this.toolHint()); return true; }
    if (k === 'Escape') { if (this.draw) { if (this.tool === 'line') this.finishLine(false); else this.cancelDraw(); } else if (this.tool !== 'select') this.setTool('select'); else this.setSel([]); return true; }
    if (k === 'Enter' && this.tool === 'line' && this.draw) { this.finishLine(false); return true; }
    if ((k === 'Delete' || k === 'Backspace') && this.sel.size) { this.deleteSelected(); return true; }
    const map = { l: 'line', a: 'arc3', r: 'rect', c: 'circle', p: 'polygon', s: 'slot', t: 'trim', d: 'c_dimension' };
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
    } else if (this.tool.startsWith('c_') || this.tool === 'sk_useedge') {
      const raw = this.rawUV(ev) || uv;
      const hp = this.tool === 'sk_useedge' ? this.pickEdge(raw) : this.pickRef(raw, { refs: true, sides: this.tool === 'c_dimension' });
      const changed = JSON.stringify(hp) !== JSON.stringify(this.hoverPick);
      this.hoverPick = hp;
      if (this.dimPend?.picks.length) this.updatePending(raw);
      if (changed) this.redraw();
    }
    this.drawCursor(uv);
    this.showStatus(uv);
    if (this._drag) this.dragMove(uv);
  }

  rawUV(ev) { const p = this.viewer.rayToPlane(ev, this.frameArr); return p ? this.toUV(p) : null; }

  showStatus(uv = this.cursor) {
    const at = uv ? `Sketch  X ${U.fmt(uv[0])}  Y ${U.fmt(uv[1])} ${U.label}${this.snapped ? '  · snapped' : ''}` : 'Sketch';
    const d = this.defText();
    this.app.status(d ? `${at}   ·   ${d}` : at);
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
    if (t === 'sk_useedge') return this.useEdge(this.rawUV(ev) || uv);
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
      this.draw.slotLen = this.readTyped().hasA;
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
      this.draw.slotLen = this.readTyped().hasA;
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
    this.addTypedDims(e);
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
    // a typed length becomes a dimension on the line
    if (this.readTyped().hasA) this.addDims([{ type: 'length', a: { e: e.id } }]);
    d.firstLine = d.firstLine || e.id;
    d.lastLine = e.id;
    d.pts.push([...p]);
    d.made = (d.made || 0) + 1;
    this.typed = false; this.inA.value = ''; this.inB.value = '';
    if (d.pts.length >= 3 && Math.hypot(p[0] - first[0], p[1] - first[1]) < 1e-9) return this.finishLine(true);
    this.updateDraw();
    this.app.onSketchChanged?.();
  }

  /** What was typed in the drawing boxes (only counts when typed, not the live readout). */
  readTyped() {
    const A = parseExpr(this.inA.value), B = parseExpr(this.inB.value);
    return { hasA: !!this.typed && this.inA.value.trim() !== '' && Number.isFinite(A), hasB: !!this.typed && this.inB.value.trim() !== '' && Number.isFinite(B) };
  }

  /** Sizes typed while drawing a shape become its driving dimensions. */
  addTypedDims(e) {
    const T = this.readTyped(), id = e.id, specs = [];
    if (e.type === 'rect') {
      if (T.hasA) specs.push({ type: 'hdistance', a: { e: id, k: 'c0' }, b: { e: id, k: 'c1' }, sign: Math.sign(e.w) || 1 });
      if (T.hasB) specs.push({ type: 'vdistance', a: { e: id, k: 'c1' }, b: { e: id, k: 'c2' }, sign: Math.sign(e.h) || 1 });
    } else if (e.type === 'circle' && T.hasA) specs.push({ type: 'diameter', a: { e: id } });
    else if ((e.type === 'arc' || e.type === 'polygon') && T.hasA) specs.push({ type: 'radius', a: { e: id } });
    else if (e.type === 'slot') {
      if (this.draw?.slotLen) specs.push({ type: 'length', a: { e: id } });
      if (T.hasA) specs.push({ type: 'diameter', a: { e: id } });
    }
    this.addDims(specs);
  }

  /** Add dimensions at the sizes the shapes have now, placed beside them (skips any already set). */
  addDims(specs) {
    const all = this.allEnts(), E = this.allE(), px = this.px();
    for (const sp of specs) {
      const c = { ...sp, id: newId() };
      c.value = measure(c, all);
      if (!(c.value > 1e-9)) continue;
      Object.assign(c, defaultPlacement(c, E, px));
      if (isRedundant(this.entities, this.constraints, c, this.refEnts)) continue;
      this.constraints.push(c);
    }
  }

  finishLine() {
    const d = this.draw; if (!d) return;
    this.draw = null; this.hideHud();
    this.redraw(); this.app.onSketchChanged?.();
  }

  replaceEntities(ents) {
    this.push();
    this.entities = ents;
    const E = this.allE();
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

  // ------------------------------------------------------------------- reference geometry
  /** The fixed things a sketch can measure from: origin, axes, part edges (with saved ids kept). */
  setRefs(fresh = [], saved = []) {
    const edges = (fresh || []).filter(e => e && !Array.isArray(e) && e.type);
    const old = (saved || []).filter(e => e && !Array.isArray(e) && e.type && e.id);
    const { list, missing } = matchRefs(edges, old);
    // part edges before the axes: where an edge lies on an axis, a click picks the edge
    const [o, ...axes] = baseRefs();
    this.refEnts = [o, ...list.map(e => ({ ...e, ref: 'edge' })), ...missing.map(e => ({ ...e, ref: 'edge', missing: true })), ...axes];
    this.fixedIds = new Set(this.refEnts.map(e => e.id));
    // the old snapping segments (kept for older sketches)
    this.refs = this.refEnts.filter(e => e.ref === 'edge' && e.type === 'line').map(e => [e.p1, e.p2]);
  }

  allEnts() { return this.entities.concat(this.refEnts); }
  allE() { return Object.fromEntries(this.allEnts().map(e => [e.id, e])); }
  isRef(id) { return this.fixedIds?.has(id); }

  /** Part edges the sketch's relations and dimensions use (saved with it to find them again). */
  usedRefs() {
    const used = new Set();
    for (const c of this.constraints) for (const r of [c.a, c.b, c.c]) if (r && this.isRef(r.e)) used.add(r.e);
    return this.refEnts.filter(e => e.ref === 'edge' && used.has(e.id)).map(({ ref, missing, ...e }) => e);
  }

  /** mm per screen pixel on the sketch plane. */
  px() { return this.viewer.pixelSize(this.frame[0].toArray()) || 0.1; }

  // ------------------------------------------------------------------- constraints
  /** What's under the pointer: a point ({e, k}), else a shape ({e}, rectangles with the side). */
  pickRef(uv, o = {}) {
    const px = this.viewer.pixelSize(this.to3(...uv).toArray());
    const pool = o.refs ? this.allEnts() : this.entities;
    let best = null, bd = 9 * px;
    if (o.points !== false) for (const e of pool) {
      if (e.ref === 'axis') continue;
      for (const k of pointKeys(e)) {
        if (k === 'm') continue;
        const q = cpoint(e, k), d = Math.hypot(q[0] - uv[0], q[1] - uv[1]);
        if (d < bd) { bd = d; best = { e: e.id, k }; }
      }
    }
    if (best) return best;
    let be = null, bed = 8 * px;
    for (const e of pool) {
      if (e.type === 'point') continue;
      // the sketch's own shapes win over part edges, and those over the axes, when close
      if (e.ref === 'axis' && o.axes === false) continue;
      const d = distToEntity(e, uv) + (e.ref === 'axis' ? 4 * px : e.ref ? 2 * px : 0);
      if (d < bed) { bed = d; be = e; }
    }
    if (!be) return null;
    if (o.sides && be.type === 'rect' && !be.angle) {
      const C = ['c0', 'c1', 'c2', 'c3'].map(k => cpoint(be, k));
      let side = 0, sd = Infinity;
      for (let i = 0; i < 4; i++) { const d = segD(uv, C[i], C[(i + 1) % 4]); if (d < sd) { sd = d; side = i; } }
      return { e: be.id, side };
    }
    return { e: be.id };
  }

  /** A part edge under the pointer (for Use Edge). */
  pickEdge(uv) {
    const px = this.viewer.pixelSize(this.to3(...uv).toArray());
    let be = null, bd = 8 * px;
    for (const e of this.refEnts) { if (e.ref !== 'edge' || e.missing) continue; const d = distToEntity(e, uv); if (d < bd) { bd = d; be = e; } }
    return be ? { e: be.id } : null;
  }

  async constraintClick(uv, ev) {
    const raw = this.rawUV(ev) || uv;
    if (this.tool === 'c_dimension') return this.dimClick(raw);
    const pick = this.pickRef(raw, { refs: true });
    if (!pick) return;
    if (this.tool === 'c_fix' && this.isRef(pick.e)) { this.app.toast('Part edges and the origin are fixed already.', 'warn'); return; }
    this.cpicks.push(pick);
    const all = this.allEnts();
    const c = makeConstraint(this.tool, this.cpicks, all);
    if (c === null) { this.redraw(); this.app.hint(CTOOLS[this.tool].hint + ` · ${this.cpicks.length} picked`); return; }
    this.cpicks = [];
    if (typeof c === 'string') { this.app.toast(c, 'warn'); this.redraw(); return; }
    if ([c.a, c.b, c.c].filter(Boolean).every(r => this.isRef(r.e))) { this.app.toast('Pick at least one shape of the sketch: part edges and the origin don\'t move.', 'warn'); this.redraw(); return; }
    this.addConstraint(c);
  }

  // ---- Smart Dimension: pick, move to place, click, type the value
  dimClick(uv) {
    const pend = this.dimPend || (this.dimPend = { picks: [], cand: null });
    // once a dimension is ready to place, the axes don't catch the placing click
    // (pick an axis first to measure from it)
    const pick = this.pickRef(uv, { refs: true, sides: true, axes: !pend.cand });
    const same = pick && pend.picks.some(p => p.e === pick.e && p.k === pick.k && p.side === pick.side);
    if (pick && !same && pend.picks.length < 2) {
      pend.picks.push(pick);
      const r = inferDimension(pend.picks, this.allEnts(), uv, this.fixedIds);
      if (typeof r === 'string') { this.app.toast(r, 'warn'); this.dimPend = null; this.redraw(); return; }
      pend.cand = r ? this.prepDim(r, uv) : null;
      this.app.hint(pend.cand ? 'Move to place the dimension and click · or click a second item (point, line, circle)' : 'Now click a second item: a point, line or circle');
      this.redraw();
      return;
    }
    if (pend.cand) { const c = pend.cand; this.dimPend = null; this.placeDimension(c); return; }
    if (!pend.picks.length) this.dimPend = null;
  }

  /** The pending dimension follows the pointer: horizontal / vertical / aligned, and where it sits. */
  updatePending(uv) {
    const pend = this.dimPend; if (!pend) return;
    const r = inferDimension(pend.picks, this.allEnts(), uv, this.fixedIds);
    if (r && typeof r !== 'string') pend.cand = this.prepDim(r, uv);
    this.buildDims();
  }

  prepDim(r, uv) {
    const E = this.allE();
    let c = { ...r };
    if (c.type === 'angle') c = angleTo(c, E, uv);
    placeAt(c, E, uv);
    c.value = measure(c, this.allEnts()) ?? 0;
    return c;
  }

  async placeDimension(c0) {
    const c = { ...c0, id: newId() };
    c.value = measure(c, this.allEnts()) ?? 0;
    if (!(c.value > 1e-9)) { this.app.toast('That measures zero: use a Coincident relation instead.', 'warn'); this.redraw(); return; }
    this.app.hint(this.toolHint());
    if (isRedundant(this.entities, this.constraints, c, this.refEnts)) {
      const ok = await this.app.modal(`<h3>Over defined</h3><p>The sketch's other dimensions and relations already set this size, so another dimension can't drive it.</p><p>Add it as a <b>reference</b> dimension? It shows the size in brackets and updates by itself, but doesn't change anything.</p>`,
        [{ label: 'Cancel', value: () => false }, { label: 'Add as reference', primary: true, value: () => true }]);
      if (!ok) { this.redraw(); return; }
      c.ref = true;
      this.push(); this.constraints.push(c); this.selC = c.id;
      this.redraw(); this.app.onSketchChanged?.();
      return;
    }
    this.push();
    this.constraints.push(c);
    this.selC = c.id;
    this.redraw(); this.app.onSketchChanged?.();
    this.openModify(c.id);
  }

  addConstraint(c) {
    this.push();
    const cc = { ...c, id: newId() };
    const r = solve(this.entities, [...this.constraints, cc], [], this.refEnts);
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

  /** Use Edge: copy a part edge into the sketch as a line or circle that stays on it. */
  useEdge(uv) {
    const pick = this.pickEdge(uv);
    if (!pick) { this.app.toast('Click one of the orange part edges.', 'warn'); return; }
    const R = this.refEnts.find(e => e.id === pick.e);
    this.push();
    const id = newId();
    if (R.type === 'line') {
      this.entities.push({ id, type: 'line', p1: [...R.p1], p2: [...R.p2] });
      this.constraints.push({ id: newId(), type: 'coincident', a: { e: id, k: 'p1' }, b: { e: R.id, k: 'p1' }, auto: true },
        { id: newId(), type: 'coincident', a: { e: id, k: 'p2' }, b: { e: R.id, k: 'p2' }, auto: true });
    } else if (R.type === 'circle') {
      this.entities.push({ id, type: 'circle', cx: R.cx, cy: R.cy, r: R.r });
      this.constraints.push({ id: newId(), type: 'concentric', a: { e: id }, b: { e: R.id }, auto: true },
        { id: newId(), type: 'equal', a: { e: id }, b: { e: R.id }, auto: true });
    } else { this.history.pop(); return; }
    this.setSel([id]);
  }

  autoConstrain(e, refs) {
    // constraints for the points the new shape was snapped to while drawing
    const keysFor = {
      rect: this.tool === 'rect_center' ? ['c', null] : ['c0', 'c2'], circle: ['c', null], polygon: ['c', null],
      slot: ['s', 'e', null], arc: ['s', 'e', null],
    }[e.type] || [];
    const E = this.allE();
    refs.forEach((ref, i) => {
      let k = keysFor[i];
      if (!ref || !k || !E[ref.e]) return;
      if (e.type === 'arc') { // the arc may run either way: pair each pick with its nearest end
        const target = cpoint(E[ref.e], ref.k);
        const ds = ['s', 'e'].map(kk => { const q = cpoint(e, kk); return Math.hypot(q[0] - target[0], q[1] - target[1]); });
        k = ds[0] <= ds[1] ? 's' : 'e';
      }
      this.constraints.push({ id: newId(), type: 'coincident', a: { e: e.id, k }, b: ref, auto: true });
    });
    const r = solve(this.entities, this.constraints, [], this.refEnts);
    if (r.ok) this.entities = r.entities;
  }

  solveAll(locks = []) {
    const r = solve(this.entities, this.constraints, locks, this.refEnts);
    this.entities = r.entities;
    return r.ok;
  }

  deleteConstraint(id) {
    this.push();
    this.constraints = this.constraints.filter(c => c.id !== id);
    this.selC = null; this.redraw(); this.app.onSketchChanged?.();
  }

  /** Change a dimension's value (or the parameter expression that sets it). */
  setDimValue(id, v, expr = null) {
    const c = this.constraints.find(x => x.id === id); if (!c) return false;
    this.push();
    const old = { value: c.value, expr: c.expr };
    c.value = v;
    if (expr) c.expr = expr; else delete c.expr;
    const r = solve(this.entities, this.constraints, [], this.refEnts);
    if (!r.ok) {
      c.value = old.value; if (old.expr) c.expr = old.expr; else delete c.expr;
      this.history.pop();
      this.app.toast('The sketch can\'t take that size: it conflicts with its other dimensions and relations.', 'warn');
      this.redraw();
      return false;
    }
    this.entities = r.entities; this.redraw(); this.app.onSketchChanged?.();
    return true;
  }

  /** Reference (driven) dimensions show a size; driving ones set it. */
  setDimRef(id, isRef) {
    const c = this.constraints.find(x => x.id === id); if (!c || !!c.ref === !!isRef) return;
    if (!isRef) {
      const test = { ...c, ref: false, value: measure(c, this.allEnts()) };
      if (isRedundant(this.entities, this.constraints.filter(x => x.id !== id), test, this.refEnts)) {
        this.app.toast('That would over-define the sketch: other dimensions and relations already set this size.', 'warn');
        this.redraw(); this.app.onSketchChanged?.();
        return;
      }
      this.push(); c.value = test.value; delete c.ref; delete c.expr;
    } else { this.push(); c.ref = true; delete c.expr; }
    this.redraw(); this.app.onSketchChanged?.();
  }

  /** A dimension's value as typed: a number in display units (degrees for angles) or a parameter expression. */
  parseDim(text, isAng) {
    const t = String(text ?? '').trim();
    if (exprText(t)) {
      const expr = exprOf(t);
      try { return { v: resolveDeep(expr, this.app.paramValues()), expr }; } catch (e) { return { err: e.message }; }
    }
    const v = isAng ? parseExpr(t) : U.parse(t);
    if (!Number.isFinite(v)) return { err: 'Type a number, or a parameter (like width or =width/2).' };
    return { v };
  }

  // dragging points or whole shapes with the Select tool; constraints follow
  dragStart(uv) {
    const ref = this.pickRef(uv);   // sketch shapes only (part edges don't move)
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
    const r = solve(d.base, this.constraints, locks, this.refEnts);
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
    const E = this.allE();
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

  /** Empty the sketch's group, freeing what was drawn; the plane grid is kept unless `grid`. */
  _clearGroup(grid = false) {
    for (const c of [...this.group.children]) {
      this.group.remove(c);
      if (c === this.gridGroup && !grid) continue;
      c.traverse(x => { x.geometry?.dispose(); x.material?.dispose?.(); });
    }
  }

  redraw() {
    this._clearGroup();
    if (!this.active) return;
    this.group.add(this.gridGroup);
    this.updateDefinition();
    const hp = this.hoverPick, hov = hp && !hp.k ? hp.e : null;
    const picked = new Set([...(this.cpicks || []), ...(this.dimPend?.picks || [])].map(p => p.e));
    const inSelC = new Set();
    if (this.selC) { const c = this.constraints.find(x => x.id === this.selC); for (const r of [c?.a, c?.b, c?.c]) if (r) inSelC.add(r.e); }
    // part edges and the origin
    this.group.add(this._refs(hov, picked, inSelC));
    for (const e of this.entities) {
      const hl = this.sel.has(e.id) || picked.has(e.id) || inSelC.has(e.id);
      const col = hl ? COL_SEL : (this.hoverId === e.id || hov === e.id) ? COL_HOVER : this.bad.has(e.id) ? COL_BAD : this.colorOf(e);
      this.group.add(this._line(e, col, 1));
    }
    if (hp?.k) this.group.add(this._dot(cpoint(this.allE()[hp.e], hp.k), COL_HOVER, 9));
    for (const p of [...(this.cpicks || []), ...(this.dimPend?.picks || [])]) if (p.k) { const E = this.allE(); if (E[p.e]) this.group.add(this._dot(cpoint(E[p.e], p.k), COL_SEL, 9)); }
    if (this.draw?.preview) this.group.add(this._line(this.draw.preview, COL_PREV, 1));
    // closed profile fill (helps see what will extrude)
    this.group.add(this._fill());
    this.viewer.requestRender();
    this.buildDims();
  }

  colorOf(e) {
    if (e.type === 'polyline') return colDef();
    return this.defn?.under.has(e.id) ? COL_UNDER : colDef();
  }

  /** Which shapes are fully defined, and whether any relation can't be met. */
  updateDefinition() {
    if (this._drag?.moved || this._ldrag) return;
    const key = JSON.stringify([this.entities, this.constraints.map(c => [c.type, c.a, c.b, c.c, c.value, c.ra, c.rb, c.sign, c.ref, c.inner]), this.refEnts.length]);
    if (key === this._defKey) return;
    this._defKey = key;
    this.defn = analyze(this.entities, this.constraints, this.refEnts);
    const bad = unsatisfied(this.entities, this.constraints, this.refEnts);
    this.badC = new Set(bad.map(c => c.id));
    this.bad = new Set();
    for (const c of bad) for (const r of [c.a, c.b, c.c]) if (r && !this.isRef(r.e)) this.bad.add(r.e);
    this.app.onDefinition?.();
  }

  /** "Fully defined" / "Under defined: 3 more dimensions or relations needed" / "Over defined …". */
  defText() {
    if (!this.active || !this.entities.some(e => e.type !== 'polyline')) return '';
    if (this.badC?.size) return `Over defined: ${this.badC.size} relation${this.badC.size > 1 ? 's' : ''} can't be met (red)`;
    const n = this.defn?.dof ?? 0;
    return n ? `Under defined: ${n} more dimension${n > 1 ? 's' : ''} needed` : 'Fully defined';
  }
  defState() { const t = this.defText(); return !t ? '' : this.badC?.size ? 'over' : (this.defn?.dof ? 'under' : 'full'); }

  _dot(p, color, size = 7) {
    const d = new THREE.Points(new THREE.BufferGeometry().setFromPoints([this.to3(...p)]), new THREE.PointsMaterial({ color, size, sizeAttenuation: false, depthTest: false }));
    d.renderOrder = 13; return d;
  }

  _refs(hov, picked, inSelC) {
    const g = new THREE.Group();
    for (const e of this.refEnts) {
      if (e.ref === 'axis') continue;
      const hl = picked.has(e.id) || inSelC.has(e.id) ? COL_SEL : hov === e.id ? COL_HOVER : null;
      if (e.type === 'point') { g.add(this._dot([e.x, e.y], hl || (dark() ? 0xe9ecef : 0x495057), 7)); continue; }
      const { pts, closed } = entityOutline(e);
      const P = pts.map(p => this.to3(...p)); if (closed && P.length) P.push(P[0]);
      const mat = e.missing
        ? new THREE.LineDashedMaterial({ color: COL_BAD, dashSize: 1, gapSize: 0.7, depthTest: false })
        : new THREE.LineBasicMaterial({ color: hl || COL_EDGE, depthTest: false, transparent: true, opacity: hl ? 1 : 0.85 });
      const ln = new THREE.Line(new THREE.BufferGeometry().setFromPoints(P), mat);
      if (e.missing) ln.computeLineDistances();
      ln.renderOrder = 10; g.add(ln);
    }
    return g;
  }

  // ------------------------------------------------------------------- dimensions
  // Dimensions are drawn on the sketch: extension lines, a dimension line with arrows, the value
  // on it (radius / diameter with a leader, angles with an arc). Drag one to move it, double-click
  // to change it. Black: driving · grey in brackets: reference · purple: set by a parameter.
  buildDims() {
    const all = this.allEnts(), E = this.allE();
    this.dimItems = [];
    for (const c of this.constraints) {
      const T = CTYPES[c.type];
      if (!T || !refsExist(c, E)) continue;
      if (T.dim) this.dimItems.push({ c });
      else if (c.type !== 'coincident' || this.selC === c.id) glyphSpots(c, all).forEach((at, si) => this.dimItems.push({ c, at, glyph: true, off: si }));
    }
    if (this.dimPend?.cand) this.dimItems.push({ c: this.dimPend.cand, preview: true });
    let h = '<svg class="dimsvg" xmlns="http://www.w3.org/2000/svg"></svg>';
    this.dimItems.forEach((it, i) => {
      const c = it.c;
      if (it.glyph) { h += `<div class="cg ${this.selC === c.id ? 'sel' : ''}" data-i="${i}" title="${CTYPES[c.type].label} · click to select, Delete to remove">${CTYPES[c.type].glyph}</div>`; return; }
      it.cls = this.dimClass(c, it.preview);
      const tip = it.preview ? '' : c.ref ? 'Reference dimension (driven)' : c.expr ? `${c.expr.slice(1)} = ${this.dimText(c, E, true)}` : `${CTYPES[c.type].label} · double-click to change, drag to move`;
      h += `<div class="dim ${it.cls}" data-i="${i}" title="${tip.replace(/"/g, '&quot;')}">${this.dimText(c, E)}</div>`;
    });
    this.dims.innerHTML = h;
    this.dimSvg = this.dims.firstChild;
    if (this._modify) this.dims.appendChild(this._modify.box);
    // while drawing or picking, labels must not swallow clicks meant for the geometry
    this.dims.classList.toggle('passive', this.pickingTool());
    this.placeDims();
    this._bindDims();
  }

  dimClass(c, preview) {
    if (preview) return 'preview';
    return [c.ref ? 'ref' : c.expr ? 'expr' : 'driving', this.selC === c.id ? 'sel' : '', this.badC?.has(c.id) ? 'bad' : ''].join(' ');
  }

  dimText(c, E = this.allE(), bare = false) {
    const T = CTYPES[c.type];
    const v = c.ref || !Number.isFinite(c.value) ? (measure(c, Object.values(E)) ?? 0) : c.value;
    let t = T.angle ? `${Math.round(v * 100) / 100}°` : (c.type === 'diameter' ? '⌀' : c.type === 'radius' ? 'R' : '') + U.fmt(v);
    if (bare) return t;
    return c.ref ? `(${t})` : t;
  }

  placeDims() {
    if (!this.dimItems || !this.dimSvg) return;
    const E = this.allE(), px = this.px();
    const S = p => this.viewer.worldToScreen(this.to3(p[0], p[1]).toArray());
    const f = v => v.toFixed(1);
    let svg = '';
    const els = this.dims.querySelectorAll('[data-i]');
    for (const el of els) {
      const it = this.dimItems[+el.dataset.i]; if (!it) continue;
      if (it.glyph) {
        const [x, y] = S(it.at);
        el.style.left = `${x + 12 + it.off * 2}px`; el.style.top = `${y - 12}px`;
        continue;
      }
      let L;
      try { L = layout(it.c, E, px); } catch { continue; }
      const seg = ([a, b]) => { const [x1, y1] = S(a), [x2, y2] = S(b); return `M${f(x1)} ${f(y1)}L${f(x2)} ${f(y2)}`; };
      const arc = a => {
        const n = Math.max(6, Math.ceil((a.a1 - a.a0) / 0.06)); let d = '';
        for (let i = 0; i <= n; i++) { const t = a.a0 + (a.a1 - a.a0) * i / n, [x, y] = S([a.c[0] + a.r * Math.cos(t), a.c[1] + a.r * Math.sin(t)]); d += `${i ? 'L' : 'M'}${f(x)} ${f(y)}`; }
        return d;
      };
      const head = ({ at, d }) => {
        const [tx, ty] = S(at), [bx, by] = S([at[0] - d[0] * px * 10, at[1] - d[1] * px * 10]);
        let ux = tx - bx, uy = ty - by; const l = Math.hypot(ux, uy) || 1; ux /= l; uy /= l;
        const k = 9, w = 3.2, cx = tx - ux * k, cy = ty - uy * k;
        return `M${f(tx)} ${f(ty)}L${f(cx - uy * w)} ${f(cy + ux * w)}L${f(cx + uy * w)} ${f(cy - ux * w)}Z`;
      };
      svg += `<g class="${it.cls}"><path class="dl" d="${L.lines.map(seg).join('')}${L.arcs.filter(a => !a.thin).map(arc).join('')}"/>`
        + `<path class="el" d="${L.thin.map(seg).join('')}${L.arcs.filter(a => a.thin).map(arc).join('')}"/>`
        + `<path class="ah" d="${L.arrows.map(head).join('')}"/></g>`;
      const [x, y] = S(L.text);
      el.style.left = `${x}px`; el.style.top = `${y}px`;
      if (this._modify && !it.preview && this._modify.id === it.c.id) { this._modify.box.style.left = `${x}px`; this._modify.box.style.top = `${y}px`; }
    }
    this.dimSvg.innerHTML = svg;
  }

  _bindDims() {
    const D = this.dims;
    D.onpointerdown = ev => {
      const el = ev.target.closest('.dim[data-i]'); if (!el || ev.button !== 0) return;
      const it = this.dimItems[+el.dataset.i]; if (!it || it.preview) return;
      ev.preventDefault(); ev.stopPropagation();
      el.setPointerCapture(ev.pointerId);
      this._ldrag = { id: it.c.id, x: ev.clientX, y: ev.clientY, moved: false };
    };
    D.onpointermove = ev => {
      const d = this._ldrag; if (!d) return;
      if (!d.moved && Math.hypot(ev.clientX - d.x, ev.clientY - d.y) < 4) return;
      const c = this.constraints.find(x => x.id === d.id); const uv = this.rawUV(ev);
      if (!c || !uv) return;
      if (!d.moved) { d.moved = true; this.push(); }
      placeAt(c, this.allE(), uv);
      if (c.type === 'angle' && c.ref) Object.assign(c, angleTo(c, this.allE(), uv));
      this.placeDims();
    };
    D.onpointerup = () => {
      const d = this._ldrag; this._ldrag = null; if (!d) return;
      if (d.moved) { this.buildDims(); this.app.onSketchChanged?.(); return; }
      // a second click soon after on the same dimension: change it (the labels are rebuilt
      // after the first click, so the browser's own double-click doesn't arrive)
      const now = performance.now(), last = this._lastDimClick;
      this._lastDimClick = { id: d.id, t: now };
      if (last && last.id === d.id && now - last.t < 450) { this._lastDimClick = null; this.editDimension(d.id); return; }
      this.selC = d.id; this.sel.clear();
      this.redraw(); this.app.onSketchChanged?.();
    };
    D.ondblclick = ev => { if (ev.target.closest('.dim[data-i]')) ev.stopPropagation(); };
    D.onclick = ev => {
      const el = ev.target.closest('.cg[data-i]'); if (!el) return;
      const it = this.dimItems[+el.dataset.i];
      this.selC = this.selC === it.c.id ? null : it.c.id; this.sel.clear(); this.redraw(); this.app.onSketchChanged?.();
    };
  }

  editDimension(id) {
    const c = this.constraints.find(x => x.id === id); if (!c) return;
    this.selC = id;
    if (c.ref) { this.redraw(); this.app.onSketchChanged?.(); this.app.toast('A reference dimension follows the sketch. Untick Reference in the panel to make it set the size.', 'info'); return; }
    this.openModify(id);
  }

  /** The Modify box (like SOLIDWORKS'): type a value or a parameter, ✓ to apply, ✗ to leave it. */
  openModify(id) {
    this.closeModify();
    const c = this.constraints.find(x => x.id === id); if (!c) return;
    const isAng = !!CTYPES[c.type].angle;
    const box = document.createElement('div');
    box.className = 'dim-modify';
    box.innerHTML = `<input type="text" spellcheck="false" value="${c.expr ? c.expr.replace(/"/g, '&quot;') : isAng ? String(Math.round(c.value * 1e4) / 1e4) : U.fmt(c.value, 4)}"><span class="u">${isAng ? '°' : U.label}</span><button class="ok" title="Apply (Enter)">✓</button><button class="x" title="Leave it (Esc)">✗</button>`;
    const inp = box.querySelector('input');
    this._modify = { id, box };
    this.dims.appendChild(box);
    this.buildDims();
    inp.focus(); inp.select();
    const apply = () => {
      const r = this.parseDim(inp.value, isAng);
      if (r.err) { this.app.toast(r.err, 'warn'); inp.focus(); return; }
      if (!(r.v > 0) || (isAng && r.v > 180)) { this.app.toast(isAng ? 'An angle between 0° and 180°.' : 'A dimension needs a size above zero.', 'warn'); inp.focus(); return; }
      this.closeModify();
      if (Math.abs(r.v - c.value) > 1e-9 || (r.expr || null) !== (c.expr || null)) this.setDimValue(id, r.v, r.expr);
      else this.buildDims();
    };
    inp.addEventListener('keydown', ev => {
      ev.stopPropagation();
      if (ev.key === 'Enter') { ev.preventDefault(); apply(); }
      if (ev.key === 'Escape') { ev.preventDefault(); this.closeModify(); this.buildDims(); }
    });
    box.addEventListener('pointerdown', ev => ev.stopPropagation());
    box.querySelector('.ok').onclick = apply;
    box.querySelector('.x').onclick = () => { this.closeModify(); this.buildDims(); };
    // clicking elsewhere keeps what's typed, as SOLIDWORKS does
    this._modifyAway = ev => { if (this._modify && !box.contains(ev.target)) apply(); };
    setTimeout(() => document.addEventListener('pointerdown', this._modifyAway, true), 0);
  }

  closeModify() {
    if (this._modifyAway) { document.removeEventListener('pointerdown', this._modifyAway, true); this._modifyAway = null; }
    if (this._modify) { this._modify.box.remove(); this._modify = null; }
  }

  _line(e, color, opacity) {
    const { pts, closed } = entityOutline(e);
    const P = pts.map(p => this.to3(...p)); if (closed && P.length) P.push(P[0]);
    const mat = e.construction ? new THREE.LineDashedMaterial({ color, dashSize: 1.2, gapSize: 0.8, depthTest: false, transparent: true, opacity: 0.75 }) : new THREE.LineBasicMaterial({ color, depthTest: false, transparent: true, opacity });
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

// Find the part edges a sketch used again after the model changed: the same edge if it's still
// there, otherwise the nearest one like it (a parallel line, a circle). Saved ones not found
// come back as `missing` (drawn red; their relations hold to where the edge was).
function matchRefs(fresh, saved) {
  const list = fresh.map(e => ({ ...e }));
  const pts = [...fresh, ...saved].flatMap(e => e.type === 'line' ? [e.p1, e.p2] : [[e.cx - e.r, e.cy - e.r], [e.cx + e.r, e.cy + e.r]]);
  let size = 100;
  if (pts.length) { const xs = pts.map(p => p[0]), ys = pts.map(p => p[1]); size = Math.max(Math.hypot(Math.max(...xs) - Math.min(...xs), Math.max(...ys) - Math.min(...ys)), 1); }
  const dist = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1]);
  const score = (s, f) => {
    if (s.type !== f.type) return null;
    if (s.type === 'line') {
      const d1 = dist(s.p1, f.p1) + dist(s.p2, f.p2), d2 = dist(s.p1, f.p2) + dist(s.p2, f.p1);
      return d1 <= d2 ? { d: d1, flip: false } : { d: d2, flip: true };
    }
    return { d: dist([s.cx, s.cy], [f.cx, f.cy]) + Math.abs(s.r - f.r), flip: false };
  };
  // a moved edge: how far it moved sideways, plus a little for sliding along / changing length
  const near = (s, f) => {
    if (s.type !== 'line') return score(s, f);
    const u = [s.p2[0] - s.p1[0], s.p2[1] - s.p1[1]], L = Math.hypot(...u) || 1, n = [-u[1] / L, u[0] / L];
    const ms = [(s.p1[0] + s.p2[0]) / 2, (s.p1[1] + s.p2[1]) / 2], mf = [(f.p1[0] + f.p2[0]) / 2, (f.p1[1] + f.p2[1]) / 2];
    const side = Math.abs((mf[0] - ms[0]) * n[0] + (mf[1] - ms[1]) * n[1]);
    const along = Math.abs((mf[0] - ms[0]) * u[0] / L + (mf[1] - ms[1]) * u[1] / L);
    const fl = Math.hypot(f.p2[0] - f.p1[0], f.p2[1] - f.p1[1]);
    const flip = (f.p2[0] - f.p1[0]) * u[0] + (f.p2[1] - f.p1[1]) * u[1] < 0;
    return { d: side + 0.25 * along + 0.1 * Math.abs(fl - L), flip };
  };
  const parallel = (s, f) => {
    if (s.type !== 'line') return true;
    const a = [s.p2[0] - s.p1[0], s.p2[1] - s.p1[1]], b = [f.p2[0] - f.p1[0], f.p2[1] - f.p1[1]];
    return Math.abs(a[0] * b[1] - a[1] * b[0]) / ((Math.hypot(...a) * Math.hypot(...b)) || 1) < 0.02;
  };
  const taken = new Set(), missing = [], left = [];
  const assign = (s, i, flip) => {
    const f = list[i]; f.id = s.id;
    if (flip) [f.p1, f.p2] = [f.p2, f.p1];
    taken.add(i);
  };
  for (const s of saved) {
    let bi = -1, bs = null;
    list.forEach((f, i) => { if (taken.has(i)) return; const r = score(s, f); if (r && (!bs || r.d < bs.d)) { bs = r; bi = i; } });
    if (bs && bs.d < Math.max(1e-3, size * 1e-5)) assign(s, bi, bs.flip); else left.push(s);
  }
  // the rest: best pairs first, so each edge goes to the one most like it
  const pairs = [];
  for (const s of left) list.forEach((f, i) => { if (taken.has(i) || !parallel(s, f)) return; const r = near(s, f); if (r && r.d < size * 0.35) pairs.push({ s, i, ...r }); });
  pairs.sort((a, b) => a.d - b.d);
  const done = new Set();
  for (const p of pairs) if (!done.has(p.s) && !taken.has(p.i)) { assign(p.s, p.i, p.flip); done.add(p.s); }
  for (const s of left) if (!done.has(s)) missing.push(s);
  let n = 0;
  for (const s of saved) { const m = /^r(\d+)$/.exec(s.id); if (m) n = Math.max(n, +m[1] + 1); }
  for (const f of list) if (!f.id) f.id = `r${n++}`;
  return { list, missing };
}
