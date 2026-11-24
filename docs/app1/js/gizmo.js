// On-part handles for Move / Rotate and Scale: arrows to drag along an axis, rings to turn
// about one, boxes to stretch. Dragging moves the part on screen at once; letting go fills
// in the tool's numbers and the engine's preview follows.
import * as THREE from 'three';
import { TransformControls } from 'three/addons/TransformControls.js';

const DEG = Math.PI / 180;

/** Our rotation (degrees about X, then Y, then Z: R = Rz·Ry·Rx) as a three.js quaternion. */
export function quatOf(rot) {
  return new THREE.Quaternion().setFromEuler(new THREE.Euler((+rot[0] || 0) * DEG, (+rot[1] || 0) * DEG, (+rot[2] || 0) * DEG, 'ZYX'));
}
export function rotOf(q) {
  const e = new THREE.Euler().setFromQuaternion(q, 'ZYX');
  const r = v => { let d = Math.round(v / DEG * 1000) / 1000; if (Math.abs(d) < 1e-9) d = 0; return d; };
  return [r(e.x), r(e.y), r(e.z)];
}

export class Gizmo {
  constructor(app) {
    this.app = app;
    const v = app.viewer;
    this.obj = new THREE.Object3D();
    v.scene.add(this.obj);
    const tc = this.tc = new TransformControls(v.camera, v.canvas);
    tc.setSpace('world');
    tc.setSize(1.1);
    tc.setRotationSnap(5 * DEG);
    this.helper = tc.getHelper();
    v.scene.add(this.helper);
    tc.addEventListener('change', () => v.requestRender());
    tc.addEventListener('dragging-changed', e => { v.controls.enabled = !e.value; });
    tc.addEventListener('mouseDown', () => { this.grabbed = true; this.start(); });
    tc.addEventListener('objectChange', () => this.drag());
    tc.addEventListener('mouseUp', () => this.drop());
    this.helper.visible = false; tc.enabled = false;
    // Shift: turn freely (5° steps otherwise)
    window.addEventListener('keydown', e => { if (e.key === 'Shift') tc.setRotationSnap(null); });
    window.addEventListener('keyup', e => { if (e.key === 'Shift') tc.setRotationSnap(5 * DEG); });
    this.moved = new Set();
  }

  /** A click that landed on a handle isn't a click on the part. */
  consumeClick() { const g = this.grabbed; this.grabbed = false; return g; }

  // Move / Rotate, Scale, or Support Fins' Rotate mode ('pose': turn the part to print it)
  get tool() {
    const t = this.app.tool;
    if (!t?.def.gizmo) return null;
    if (t.def.gizmo === 'pose' && (t.values._mode || 'rotate') !== 'rotate') return null;
    return t;
  }

  mode(t = this.tool) {
    if (!t) return null;
    if (t.def.gizmo === 'scale') return 'scale';
    if (t.def.gizmo === 'pose') return 'rotate';
    return t.values._gizmo === 'rotate' ? 'rotate' : 'translate';
  }

  bodies(t = this.tool || this.app.tool) {
    const ids = (t?.def.gizmo === 'pose' ? t.values.body : t?.values.bodies) || [];
    return ids.map(id => [id, this.app.viewer.bodies.get(id), this.app.state.bodies.find(b => b.id === id)]).filter(x => x[1] && x[2]?.bbox);
  }

  /** The point a body turns or scales about, as the engine takes it (features.js pivot). */
  pivot(b, kind) {
    const [lo, hi] = b.bbox;
    if (kind === 'origin') return new THREE.Vector3();
    if (kind === 'base') return new THREE.Vector3((lo[0] + hi[0]) / 2, (lo[1] + hi[1]) / 2, lo[2]);
    return new THREE.Vector3((lo[0] + hi[0]) / 2, (lo[1] + hi[1]) / 2, (lo[2] + hi[2]) / 2);
  }

  /** The transform the tool's current numbers make for one body, as a matrix. */
  matrixFor(o, b, vals) {
    const t = this.tool;
    if (t.def.gizmo === 'scale') {
      const c = this.pivot(b, vals.pivot || 'base');
      const k = vals._k || [1, 1, 1];
      return new THREE.Matrix4().makeTranslation(c.x, c.y, c.z).multiply(new THREE.Matrix4().makeScale(...k)).multiply(new THREE.Matrix4().makeTranslation(-c.x, -c.y, -c.z));
    }
    const c = this.pivot(b, vals.pivot || 'center');
    const tr = (vals.translate || [0, 0, 0]).map(Number);
    const q = quatOf(vals.rotate || [0, 0, 0]);
    const m = new THREE.Matrix4().makeTranslation(c.x + tr[0], c.y + tr[1], c.z + tr[2])
      .multiply(new THREE.Matrix4().makeRotationFromQuaternion(q))
      .multiply(new THREE.Matrix4().makeTranslation(-c.x, -c.y, -c.z));
    if (vals.bed && (vals.rotate || []).some(a => Math.abs(+a) > 1e-9)) {
      // Land on bed, as the engine does it: the lowest point ends up at Move Z
      const pos = o.geo.attributes.position.array, e = m.elements;
      let lo = Infinity;
      for (let i = 0; i < pos.length; i += 3) { const z = e[2] * pos[i] + e[6] * pos[i + 1] + e[10] * pos[i + 2] + e[14]; if (z < lo) lo = z; }
      m.premultiply(new THREE.Matrix4().makeTranslation(0, 0, tr[2] - lo));
    }
    return m;
  }

  /** Put the handles where the tool's numbers put the part (after opening, typing, a drop). */
  sync() {
    const t = this.tool, tc = this.tc;
    const list = this.bodies(t);
    if (!t || !list.length || this.app.sketcher.active) { this.hide(); return; }
    const [, o, b] = list[0];
    const vals = this.values(t);
    const m = this.matrixFor(o, b, vals);
    const kind = t.def.gizmo === 'scale' ? (vals.pivot || 'base') : (vals.pivot || 'center');
    const c = this.pivot(b, kind).applyMatrix4(m);
    this.obj.position.copy(c);
    this.obj.quaternion.copy(t.def.gizmo === 'scale' ? new THREE.Quaternion() : quatOf(vals.rotate || [0, 0, 0]));
    this.obj.scale.set(...(t.def.gizmo === 'scale' ? vals._k : [1, 1, 1]));
    this.obj.updateMatrixWorld();
    this._land = c.z - this.pivot(b, kind).z - (vals.translate?.[2] || 0);
    tc.setMode(this.mode(t));
    tc.showX = tc.showY = tc.showZ = true;
    if (tc.object !== this.obj) tc.attach(this.obj);
    // no new drag while Support Fins is still putting the last turn on the part
    tc.enabled = !t._posing; this.helper.visible = true;
    this.app.viewer.requestRender();
  }

  hide() {
    if (this.tc.object) this.tc.detach();
    this.tc.enabled = false; this.helper.visible = false;
    this.reset();
    this.app.viewer.requestRender();
  }

  /** The tool's numbers, with the scale tool's as three factors (_k). */
  values(t = this.tool) {
    if (t.def.gizmo === 'pose') return { translate: [0, 0, 0], rotate: [0, 0, 0], pivot: 'center', bed: true };
    const v = this.app.resolved({ ...t.values });
    if (t.def.gizmo === 'scale') {
      const b = this.bodies(t)[0]?.[2];
      const size = b ? b.bbox[1].map((x, i) => x - b.bbox[0][i]) : [1, 1, 1];
      let k = [1, 1, 1];
      if (v.mode === 'factor') k = v.uniform ? [1, 1, 1].map(() => +v.factor || 1) : (v.factor3 || [1, 1, 1]).map(x => +x || 1);
      else if (v.mode === 'fit') {
        k = (v.size || size).map((x, i) => size[i] > 1e-9 ? +x / size[i] : 1);
        if (v.uniform) { const i = k.findIndex(x => Math.abs(x - 1) > 1e-9); k = [0, 1, 2].map(() => k[i >= 0 ? i : 0]); }
      } else if (v.mode === 'units') k = [1, 1, 1].map(() => ({ in_to_mm: 25.4, mm_to_in: 1 / 25.4, cm_to_mm: 10, m_to_mm: 1000 }[v.units] || 1));
      v._k = k;
    }
    return v;
  }

  start() {
    this.app.viewer.clearPreview();
    this.app.clearPreviewTimer?.();
    this._startQ = this.obj.quaternion.clone();
  }

  /** While dragging: the part follows on screen (the engine catches up on release). */
  drag() {
    const t = this.tool; if (!t) return;
    const vals = this.dragValues(t);
    for (const [, o, b] of this.bodies(t)) {
      const m = this.matrixFor(o, b, vals);
      for (const x of [o.mesh, o.edges, o.back]) if (x) { x.matrixAutoUpdate = false; x.matrix.copy(m); x.matrixWorldNeedsUpdate = true; }
      this.moved.add(o);
    }
    this.app.liveNumbers?.(vals);
    this.app.viewer.requestRender();
  }

  /** The tool's numbers the handles stand for right now. */
  dragValues(t) {
    const v = this.values(t), [, , b] = this.bodies(t)[0];
    if (t.def.gizmo === 'scale') {
      let k = this.obj.scale.toArray().map(x => Math.max(1e-4, Math.round(x * 10000) / 10000));
      if (v.uniform) { const prev = v._k; const i = [0, 1, 2].reduce((a, j) => Math.abs(k[j] - prev[j]) > Math.abs(k[a] - prev[a]) ? j : a, 0); k = [k[i], k[i], k[i]]; this.obj.scale.set(...k); }
      return { ...v, _k: k };
    }
    const kind = v.pivot || 'center';
    const c = this.pivot(b, kind);
    const rot = rotOf(this.obj.quaternion);
    const p = this.obj.position;
    const out = { ...v, rotate: rot };
    if (this.mode(t) === 'translate') {
      // Move Z under Land on bed is the height above the bed, so the landing is taken off
      out.translate = [p.x - c.x, p.y - c.y, p.z - c.z - (this._land || 0)].map(x => Math.round(x * 1000) / 1000);
    }
    return out;
  }

  drop() {
    const t = this.tool; if (!t) return;
    const v = this.dragValues(t);
    if (t.def.gizmo === 'pose') {
      // Support Fins: the turn is applied to the part straight away (it lands on the bed)
      const [, o, b] = this.bodies(t)[0];
      const m = this.matrixFor(o, b, v);
      if (v.rotate.every(a => Math.abs(a) < 1e-9)) { this.reset(); this.sync(); return; }
      // the last turn is still on its way: this drag is let go of, the part stays as it was
      if (t._posing) { this.obj.quaternion.copy(this._startQ); this.obj.updateMatrixWorld(); this.drag(); return; }
      this.app.poseDrop(m);
      return;
    }
    if (t.def.gizmo === 'scale') {
      const k = v._k;
      if (t.values.uniform || (Math.abs(k[0] - k[1]) < 1e-9 && Math.abs(k[1] - k[2]) < 1e-9)) Object.assign(t.values, { mode: 'factor', uniform: true, factor: k[0] });
      else Object.assign(t.values, { mode: 'factor', uniform: false, factor3: k });
    } else {
      t.values.translate = v.translate; t.values.rotate = v.rotate;
    }
    this.app.renderForm();
    this.sync();
    // no OK needed: letting go puts the part there
    this.app.autoAccept();
  }

  /** Put the parts back where the scene has them (the preview shows the moved copy). */
  reset() {
    for (const o of this.moved) for (const x of [o.mesh, o.edges, o.back]) if (x) { x.matrix.identity(); x.matrixAutoUpdate = true; x.matrixWorldNeedsUpdate = true; }
    this.moved.clear();
  }
}
