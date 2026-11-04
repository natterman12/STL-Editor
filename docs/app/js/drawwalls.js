// Draw Walls: Support Fins' Draw mode. Click on the part to place breakaway walls under its
// overhangs, as many as you like, each one a straight line (two clicks), a line of several
// sections (click each corner, double-click or Enter to finish) or a curve (its two ends,
// then a point it runs through). The walls are built by Support Fins' own engine.
const SHAPES = [['line', 'Line'], ['path', 'Multi-line'], ['curve', 'Curve']];
const HINTS = {
  line: 'Click two points under an overhang for each wall. Keep clicking to add more walls.',
  path: 'Click each corner of the wall; double-click (or press Enter) on the last point to finish it.',
  curve: 'Click the two ends of the wall, then a point on the curve between them.',
};
const OK_COL = 0x2f9e5a, BAD_COL = 0xd64545, BAND_COL = 0x59d98e, PENDING_COL = 0x1f9e55;
const SHAPE_KEY = 'stle_draw_shape';

/** The circle through a, b and m in plan, sampled from a to b (null when the three are in line). */
export function arcPath(a, b, m, step = 1.5) {
  const d = 2 * (a[0] * (b[1] - m[1]) + b[0] * (m[1] - a[1]) + m[0] * (a[1] - b[1]));
  if (Math.abs(d) < 1e-9) return null;
  const sa = a[0] ** 2 + a[1] ** 2, sb = b[0] ** 2 + b[1] ** 2, sm = m[0] ** 2 + m[1] ** 2;
  const cx = (sa * (b[1] - m[1]) + sb * (m[1] - a[1]) + sm * (a[1] - b[1])) / d;
  const cy = (sa * (m[0] - b[0]) + sb * (a[0] - m[0]) + sm * (b[0] - a[0])) / d;
  const r = Math.hypot(a[0] - cx, a[1] - cy);
  if (!(r < 1e4)) return null;
  const TAU = 2 * Math.PI, ang = p => Math.atan2(p[1] - cy, p[0] - cx), a0 = ang(a);
  const toB = ((ang(b) - a0) % TAU + TAU) % TAU, toM = ((ang(m) - a0) % TAU + TAU) % TAU;
  const sweep = toM < toB ? toB : toB - TAU;
  const fm = sweep > 0 ? toM / sweep : (TAU - toM) / -sweep;
  const n = Math.min(128, Math.max(8, Math.ceil(Math.abs(sweep) * r / step)));
  const out = [];
  for (let k = 0; k <= n; k++) {
    const t = k / n, th = a0 + sweep * t;
    const z = t <= fm ? a[2] + (m[2] - a[2]) * (fm > 0 ? t / fm : 1) : m[2] + (b[2] - m[2]) * ((t - fm) / (1 - fm));
    out.push([cx + r * Math.cos(th), cy + r * Math.sin(th), z]);
  }
  return { points: out, radius: r };
}

/** The line a wall follows, for drawing it on screen. */
export function wallLine(w) {
  const p = w.points || [];
  if (w.shape === 'curve' && p.length === 3) return arcPath(p[0], p[1], p[2])?.points || [p[0], p[1]];
  return p;
}

export class WallDrawer {
  constructor(app) {
    this.app = app;
    this.pending = [];
    this.hoverPt = null;
    try { this.shape = SHAPES.some(s => s[0] === localStorage.getItem(SHAPE_KEY)) ? localStorage.getItem(SHAPE_KEY) : 'line'; } catch { this.shape = 'line'; }
  }

  // Draw Walls, or Support Fins' "Add fins" (walls drawn by hand next to the auto fins)
  get tool() { const t = this.app.tool; return t && (t.def.wallTool || (t.def.type === 'supportfins' && t.values._mode === 'add')) ? t : null; }
  get active() { return !!this.tool; }
  get walls() { return this.tool?.values.walls || []; }

  reset() { this.pending = []; this.hoverPt = null; this.app.viewer.setBand(null); }

  hint() { return HINTS[this.shape]; }

  fieldHTML(f) {
    const n = this.walls.length, st = this.tool?._wallStatus || [];
    const bad = st.filter(x => x && !x.ok).length;
    const seg = `<div class="seg">${SHAPES.map(([k, l]) => `<button data-wshape="${k}" class="${this.shape === k ? 'on' : ''}">${l}</button>`).join('')}</div>`;
    const count = n ? `${n} wall${n > 1 ? 's' : ''}${bad ? ` <span class="bad">(${bad} couldn’t be placed)</span>` : ''}` : '<span class="ph">None yet: click on the part</span>';
    const pend = this.pending.length ? ` · drawing (${this.pending.length} point${this.pending.length > 1 ? 's' : ''})` : '';
    const finish = this.shape === 'path' && this.pending.length >= 2 ? '<button data-wact="finish">Finish wall</button>' : '';
    return `<div class="field" data-key="_shape"><label title="${HINTS[this.shape]}">Draw <span class="help">?</span></label><div class="ctl">${seg}</div></div>
      <div class="field" data-key="${f.key}"><label>${f.label}</label><div class="ctl walls-ctl"><span class="walls-n">${count}${pend}</span></div></div>
      <div class="field"><label></label><div class="ctl">${finish}<button data-wact="undo" ${n || this.pending.length ? '' : 'disabled'} title="Ctrl+Z">Undo</button><button data-wact="clear" ${n ? '' : 'disabled'}>Clear all</button></div></div>`;
  }

  onFormClick(e) {
    const s = e.target.closest('[data-wshape]');
    if (s) { this.finish(); this.setShape(s.dataset.wshape); return true; }
    const a = e.target.closest('[data-wact]');
    if (!a) return false;
    if (a.dataset.wact === 'finish') this.finish();
    if (a.dataset.wact === 'undo') this.undo();
    if (a.dataset.wact === 'clear') { this.pending = []; this.commitWalls([]); }
    return true;
  }

  setShape(k) {
    this.shape = k; this.pending = [];
    try { localStorage.setItem(SHAPE_KEY, k); } catch { /* storage off */ }
    this.app.hint(this.hint());
    this.app.renderForm();
  }

  commitWalls(list) {
    const t = this.tool; if (!t) return;
    t._wallStatus = (t._wallStatus || []).slice(0, list.length);
    this.app.setValue('walls', list);
    this.app.schedulePreview(30);
  }

  add(w) { this.pending = []; this.commitWalls([...this.walls, w]); }

  finish() {
    if (this.shape !== 'path' || this.pending.length < 2) { if (this.shape === 'path') this.pending = []; this.redraw(); return; }
    const pts = this.pending;
    this.add(pts.length === 2 ? { shape: 'line', points: pts } : { shape: 'path', points: pts });
  }

  undo() {
    if (this.pending.length) { this.pending.pop(); this.app.renderForm(); return; }
    const w = [...this.walls]; w.pop(); this.commitWalls(w);
  }

  /** A click in the viewport while drawing. */
  click(hit, ev) {
    const t = this.tool; if (!t) return false;
    if (hit?.kind !== 'body') { this.app.toolMsg('Click on the part, under an overhang.', 'warn'); return true; }
    const body = t.values.body?.[0];
    if (body && body !== hit.id) {
      if (this.walls.length) { this.app.toolMsg(`These walls are on ${this.app.bodyName(body)}. Finish (OK) them first, then draw on another body.`, 'warn'); return true; }
      t.values.body = [hit.id];
    }
    if (!body) t.values.body = [hit.id];
    const p = hit.point.map(x => +x.toFixed(4));
    const last = this.pending.at(-1);
    if (last && this.app.viewer.worldToScreen && this.near(last, p, ev)) {
      // a second click on the same spot: a double-click, which ends a multi-section wall
      if (this.shape === 'path') this.finish();
      return true;
    }
    this.pending.push(p);
    if (this.shape === 'line' && this.pending.length === 2) this.add({ shape: 'line', points: this.pending });
    else if (this.shape === 'curve' && this.pending.length === 3) this.add({ shape: 'curve', points: this.pending });
    else this.app.renderForm();
    return true;
  }

  near(a, b, ev) {
    const [x1, y1] = this.app.viewer.worldToScreen(a), [x2, y2] = this.app.viewer.worldToScreen(b);
    return Math.hypot(x1 - x2, y1 - y2) < 6;
  }

  /** The pointer moved over the viewport: stretch the rubber band to it. */
  hover(hit) {
    if (!this.active) return;
    this.hoverPt = hit?.kind === 'body' ? hit.point : null;
    this.drawBand();
  }

  drawBand() {
    const v = this.app.viewer, p = this.pending, h = this.hoverPt;
    if (!p.length) { v.setBand(null, h); return; }
    let line = [...p];
    if (h) {
      if (this.shape === 'curve' && p.length === 2) line = arcPath(p[0], p[1], h)?.points || [p[0], p[1]];
      else line = [...p, h];
    }
    v.setBand(line, h, BAND_COL);
  }

  key(e) {
    if (!this.active) return false;
    const k = e.key, ctrl = e.ctrlKey || e.metaKey;
    if (k === 'Enter' && this.pending.length) { this.finish(); return true; }
    if (k === 'Escape' && this.pending.length) { this.pending = []; this.app.renderForm(); return true; }
    if ((ctrl && k.toLowerCase() === 'z') || k === 'Backspace') { this.undo(); return true; }
    return false;
  }

  /** The walls drawn so far and the one in progress, on the part. */
  redraw() {
    const v = this.app.viewer;
    v.clearOverlay('walls');
    if (!this.active) { v.setBand(null); return; }
    const st = this.tool._wallStatus || [];
    this.walls.forEach((w, i) => v.addPolyline(wallLine(w), st[i] && !st[i].ok ? BAD_COL : OK_COL, 'walls'));
    for (const q of this.pending) v.addMarker(q, PENDING_COL, 'walls');
    this.drawBand();
  }
}
