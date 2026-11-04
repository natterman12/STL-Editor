import * as THREE from 'three';
import { OrbitControls } from 'three/addons/OrbitControls.js';
import { toCreasedNormals } from 'three/addons/BufferGeometryUtils.js';
import { computeBoundsTree, disposeBoundsTree, acceleratedRaycast } from 'three/addons/three-mesh-bvh.module.js';
import { entityOutline } from './sketchgeom.js';

THREE.BufferGeometry.prototype.computeBoundsTree = computeBoundsTree;
THREE.BufferGeometry.prototype.disposeBoundsTree = disposeBoundsTree;
THREE.Mesh.prototype.raycast = acceleratedRaycast;

const SEL = new THREE.Color(0x1f6fd1), HOV = new THREE.Color(0x4b8fe0);
const FACE_HL = 0x2f8cff, PLANE_COL = 0xe8763a, SKETCH_COL = 0xd9480f;
const BASE_COLS = { XY: 0x3b82f6, XZ: 0x22a35a, YZ: 0xe25555 };

export class Viewer {
  constructor(canvas) {
    this.canvas = canvas;
    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: true, preserveDrawingBuffer: true });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.localClippingEnabled = true;
    this.scene = new THREE.Scene();
    this.camera = new THREE.PerspectiveCamera(35, 1, 0.1, 100000);
    this.camera.up.set(0, 0, 1);
    this.camera.position.set(160, -220, 150);
    this.controls = new OrbitControls(this.camera, canvas);
    this.controls.enableDamping = false;
    this.controls.screenSpacePanning = true;
    this.controls.zoomToCursor = true;
    this.controls.addEventListener('change', () => this.requestRender());
    this.setMouseMode('model');

    this.scene.add(new THREE.HemisphereLight(0xffffff, 0x8899aa, 1.6));
    const key = new THREE.DirectionalLight(0xffffff, 1.8); key.position.set(0.5, 0.3, 1);
    const fill = new THREE.DirectionalLight(0xffffff, 0.6); fill.position.set(-1, -0.4, 0.2);
    this.camera.add(key, fill);
    this.scene.add(this.camera);

    this.bodiesGroup = new THREE.Group(); this.scene.add(this.bodiesGroup);
    this.previewGroup = new THREE.Group(); this.scene.add(this.previewGroup);
    this.planesGroup = new THREE.Group(); this.scene.add(this.planesGroup);
    this.sketchGroup = new THREE.Group(); this.scene.add(this.sketchGroup);
    this.overlay = new THREE.Group(); this.scene.add(this.overlay);
    this.originGroup = new THREE.Group(); this.scene.add(this.originGroup);
    this.bodies = new Map();       // id -> {mesh, edges, serial, color, back}
    this.selected = new Set();
    this.hovered = null;
    this.extent = 100;
    this.section = null;
    this.showEdges = true;
    this.baseVisible = false;
    this.baseVisibleUser = false;
    this._buildOrigin(100);

    this.raycaster = new THREE.Raycaster();
    this.raycaster.firstHitOnly = true;
    new ResizeObserver(() => this.resize()).observe(canvas.parentElement);
    this.resize();
    this.requestRender();
  }

  setMouseMode(mode) {
    // model: left orbit (click = select), right pan; sketch: left draws, right orbit, middle pan
    // touch: one finger orbits (model) or draws (sketch, sculpt); two fingers zoom and pan, or
    // zoom and orbit while sculpting
    const c = this.controls;
    if (mode === 'sketch' || mode === 'sculpt') c.mouseButtons = { LEFT: null, MIDDLE: THREE.MOUSE.PAN, RIGHT: THREE.MOUSE.ROTATE };
    else c.mouseButtons = { LEFT: THREE.MOUSE.ROTATE, MIDDLE: THREE.MOUSE.PAN, RIGHT: THREE.MOUSE.PAN };
    c.touches = mode === 'model' ? { ONE: THREE.TOUCH.ROTATE, TWO: THREE.TOUCH.DOLLY_PAN }
      : { ONE: null, TWO: mode === 'sculpt' ? THREE.TOUCH.DOLLY_ROTATE : THREE.TOUCH.DOLLY_PAN };
  }

  resize() {
    const el = this.canvas.parentElement, w = el.clientWidth, h = el.clientHeight;
    if (!w || !h) return;
    this.renderer.setSize(w, h, false);
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
    this.requestRender();
  }

  requestRender() {
    if (this._raf) return;
    this._raf = requestAnimationFrame(() => { this._raf = null; this.render(); });
  }

  render() {
    if (this._anim) this._anim();
    this.renderer.render(this.scene, this.camera);
    if (this._anim) this.requestRender();
  }

  // ---------------------------------------------------------------- origin, grid
  _buildOrigin(size) {
    this.originGroup.clear();
    const s = Math.pow(10, Math.ceil(Math.log10(Math.max(size, 10))));
    const dk = this.dark;
    const major = new THREE.GridHelper(s * 2, 20, dk ? 0x58626d : 0x9aa5b1, dk ? 0x3d454e : 0xc3cad2);
    major.rotation.x = Math.PI / 2;
    major.material.transparent = true; major.material.opacity = 0.55; major.material.depthWrite = false;
    const minor = new THREE.GridHelper(s * 2, 200, dk ? 0x2b3138 : 0xd5dae0, dk ? 0x2b3138 : 0xd5dae0);
    minor.rotation.x = Math.PI / 2;
    minor.material.transparent = true; minor.material.opacity = 0.35; minor.material.depthWrite = false;
    this.grid = new THREE.Group(); this.grid.add(minor, major); this.grid.renderOrder = -1;
    this.originGroup.add(this.grid);
    const L = s * 0.35;
    const axes = [[[1, 0, 0], 0xe25555], [[0, 1, 0], 0x22a35a], [[0, 0, 1], 0x3b82f6]];
    for (const [d, c] of axes) {
      const g = new THREE.BufferGeometry().setFromPoints([new THREE.Vector3(), new THREE.Vector3(...d).multiplyScalar(L)]);
      const ln = new THREE.Line(g, new THREE.LineBasicMaterial({ color: c, depthTest: false, transparent: true, opacity: 0.9 }));
      ln.renderOrder = 5;
      this.originGroup.add(ln);
    }
    // base planes (selectable when visible)
    this.basePlanes = new THREE.Group();
    const ps = s * 0.3;
    const frames = { XY: [[0, 0, 0], [1, 0, 0], [0, 1, 0], [0, 0, 1]], XZ: [[0, 0, 0], [1, 0, 0], [0, 0, 1], [0, -1, 0]], YZ: [[0, 0, 0], [0, 1, 0], [0, 0, 1], [1, 0, 0]] };
    for (const [name, fr] of Object.entries(frames)) {
      const m = this._planeMesh(fr, ps, BASE_COLS[name], name);
      m.userData = { kind: 'plane', id: name };
      this.basePlanes.add(m);
    }
    this.basePlanes.visible = this.baseVisible;
    this.originGroup.add(this.basePlanes);
  }

  setTheme(dark) {
    this.dark = !!dark;
    const vis = this.baseVisible, gv = this.grid ? this.grid.visible : true;
    this._buildOrigin(this.extent);
    this.setBasePlanesVisible(vis); this.grid.visible = gv;
    for (const o of this.bodies.values()) if (o.edges) o.edges.material.color.set(this.dark ? 0x0d1013 : 0x2b333c);
    this.requestRender();
  }

  setBasePlanesVisible(v) { v = !!v; this.baseVisible = v; this.basePlanes.visible = v; this.requestRender(); }
  setGridVisible(v) { this.grid.visible = v; this.requestRender(); }

  _planeMesh(frame, size, color, label) {
    const [o, x, y] = frame.map(a => new THREE.Vector3(...a));
    const n = new THREE.Vector3().crossVectors(x, y);
    const g = new THREE.PlaneGeometry(size, size);
    const mat = new THREE.MeshBasicMaterial({ color, transparent: true, opacity: 0.13, side: THREE.DoubleSide, depthWrite: false });
    const mesh = new THREE.Mesh(g, mat);
    const m4 = new THREE.Matrix4().makeBasis(x, y, n).setPosition(o);
    mesh.applyMatrix4(m4);
    const edge = new THREE.LineSegments(new THREE.EdgesGeometry(g), new THREE.LineBasicMaterial({ color, transparent: true, opacity: 0.7 }));
    mesh.add(edge);
    mesh.renderOrder = 2;
    mesh.userData.label = label;
    return mesh;
  }

  // ---------------------------------------------------------------- bodies
  async syncBodies(bodies, fetchMesh) {
    const keep = new Set(bodies.map(b => b.id));
    for (const [id, o] of this.bodies) if (!keep.has(id)) { this._disposeBody(o); this.bodies.delete(id); }
    const jobs = [];
    for (const b of bodies) {
      let o = this.bodies.get(b.id);
      if (!o || o.serial !== b.serial) {
        jobs.push(fetchMesh(b.id).then(m => {
          const old = this.bodies.get(b.id);
          if (old) this._disposeBody(old);
          const obj = this._makeBody(b.id, m.positions, m.index, b);
          obj.serial = b.serial;
          this.bodies.set(b.id, obj);
          this._styleBody(b.id);
        }));
      } else {
        o.info = b;
        o.mesh.visible = !b.hidden;
        if (o.edges) o.edges.visible = !b.hidden && this.showEdges;
        o.mesh.material.color.set(b.color);
        if (!b.solid) o.mesh.material.color.set('#d9a0a0');
      }
    }
    await Promise.all(jobs);
    this._updateExtent();
    this.requestRender();
  }

  _makeBody(id, positions, index, info) {
    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.BufferAttribute(new Float32Array(positions), 3));
    geo.setIndex(new THREE.BufferAttribute(new Uint32Array(index), 1));
    const ntri = index.length / 3;
    let disp;
    const flat = ntri > 1500000;
    if (!flat) disp = toCreasedNormals(geo, 30 * Math.PI / 180);
    else { disp = geo.clone(); }
    const mat = new THREE.MeshStandardMaterial({
      color: info.solid ? info.color : '#d9a0a0', metalness: 0.05, roughness: 0.62, flatShading: flat,
      polygonOffset: true, polygonOffsetFactor: 1, polygonOffsetUnits: 1, side: THREE.FrontSide,
    });
    const mesh = new THREE.Mesh(disp, mat);
    disp.computeBoundsTree({ indirect: true });
    mesh.userData = { kind: 'body', id };
    mesh.visible = !info.hidden;
    this.bodiesGroup.add(mesh);
    let edges = null;
    if (ntri < 400000) {
      edges = new THREE.LineSegments(new THREE.EdgesGeometry(geo, 32), new THREE.LineBasicMaterial({ color: 0x2b333c, transparent: true, opacity: 0.55 }));
      edges.visible = !info.hidden && this.showEdges;
      edges.userData = { kind: 'edges' };
      this.bodiesGroup.add(edges);
    }
    const o = { id, mesh, edges, geo, info, ntri };
    this._applySection(o);
    return o;
  }

  _disposeBody(o) {
    this.bodiesGroup.remove(o.mesh); o.mesh.geometry.disposeBoundsTree?.(); o.mesh.geometry.dispose(); o.mesh.material.dispose();
    if (o.edges) { this.bodiesGroup.remove(o.edges); o.edges.geometry.dispose(); }
    if (o.back) { this.bodiesGroup.remove(o.back); }
    o.geo.dispose();
  }

  bodyGeometry(id) { return this.bodies.get(id)?.geo; }

  setSelection(ids) { this.selected = new Set(ids); for (const id of this.bodies.keys()) this._styleBody(id); this.requestRender(); }
  setHover(id) { if (this.hovered === id) return; const prev = this.hovered; this.hovered = id; if (prev) this._styleBody(prev); if (id) this._styleBody(id); this.requestRender(); }
  _styleBody(id) {
    const o = this.bodies.get(id); if (!o) return;
    const m = o.mesh.material;
    if (this.selected.has(id)) m.emissive.copy(SEL).multiplyScalar(0.32);
    else if (this.hovered === id) m.emissive.copy(HOV).multiplyScalar(0.16);
    else m.emissive.setRGB(0, 0, 0);
  }

  setEdgesVisible(v) { this.showEdges = v; for (const o of this.bodies.values()) if (o.edges) o.edges.visible = v && o.mesh.visible; this.requestRender(); }

  setBodiesDim(ids, dim) {
    for (const [id, o] of this.bodies) {
      const d = dim && ids.includes(id);
      o.mesh.material.transparent = d; o.mesh.material.opacity = d ? 0.18 : 1; o.mesh.material.depthWrite = !d;
      if (o.edges) o.edges.material.opacity = d ? 0.15 : 0.55;
    }
    this.requestRender();
  }

  _updateExtent() {
    const box = new THREE.Box3();
    for (const o of this.bodies.values()) if (o.mesh.visible) { o.geo.computeBoundingBox(); box.union(o.geo.boundingBox); }
    const size = box.isEmpty() ? 100 : box.getSize(new THREE.Vector3()).length();
    const ext = Math.max(size, 20);
    if (Math.abs(Math.log10(ext) - Math.log10(this.extent)) > 0.5 || !this._builtOnce) {
      this.extent = ext; this._buildOrigin(ext); this._builtOnce = true;
    } else this.extent = ext;
  }

  sceneBox() {
    const box = new THREE.Box3();
    for (const o of this.bodies.values()) if (o.mesh.visible) { o.geo.computeBoundingBox(); box.union(o.geo.boundingBox); }
    for (const c of this.previewGroup.children) { c.geometry?.computeBoundingBox(); if (c.geometry?.boundingBox) box.union(c.geometry.boundingBox); }
    return box;
  }

  // ---------------------------------------------------------------- preview
  clearPreview() {
    for (const c of [...this.previewGroup.children]) { this.previewGroup.remove(c); c.geometry?.dispose(); c.material?.dispose(); }
    for (const o of this.bodies.values()) { o.mesh.material.visible = true; if (o.edges) o.edges.material.visible = true; if (o.back) o.back.material.visible = true; }
    this.requestRender();
  }

  showPreview(pack, { hide = [] } = {}) {
    this.clearPreview();
    this._pi = -1;
    const replaced = new Set([...pack.meshes.map(m => m.id), ...(pack.removed || []), ...hide]);
    for (const id of replaced) { const o = this.bodies.get(id); if (o) { o.mesh.material.visible = false; if (o.edges) o.edges.material.visible = false; if (o.back) o.back.material.visible = false; } }
    for (const m of pack.meshes) {
      const geo = new THREE.BufferGeometry();
      geo.setAttribute('position', new THREE.BufferAttribute(new Float32Array(m.positions), 3));
      geo.setIndex(new THREE.BufferAttribute(new Uint32Array(m.index), 1));
      const existing = this.bodies.get(m.id);
      const disp = m.nf < 1500000 ? toCreasedNormals(geo, 30 * Math.PI / 180) : geo;
      const PAL = [0x7fb0e8, 0xe8a87f, 0x8fd18a, 0xd88fd1, 0xe8d27f, 0x7fe0d6, 0xb39ae8, 0xe87f8f];
      const tint = pack.result?.colors?.[m.id];
      const color = tint ? new THREE.Color(tint) : existing ? existing.mesh.material.color.clone() : new THREE.Color(PAL[(this._pi = (this._pi || 0) + 1) % PAL.length]);
      const mat = new THREE.MeshStandardMaterial({ color, metalness: 0.05, roughness: 0.6, emissive: new THREE.Color(0x2f6fd1).multiplyScalar(existing || tint ? 0.08 : 0.2),
        polygonOffset: true, polygonOffsetFactor: 1, polygonOffsetUnits: 1, clippingPlanes: this.section ? [this.section] : [] });
      const mesh = new THREE.Mesh(disp, mat);
      this.previewGroup.add(mesh);
      if (m.nf < 400000) {
        const e = new THREE.LineSegments(new THREE.EdgesGeometry(geo, 32), new THREE.LineBasicMaterial({ color: 0x1f4f8f, transparent: true, opacity: 0.6, clippingPlanes: this.section ? [this.section] : [] }));
        this.previewGroup.add(e);
      }
    }
    this.requestRender();
  }

  // ---------------------------------------------------------------- planes & sketches
  syncPlanes(planes) {
    this.planesGroup.clear();
    const size = Math.max(this.extent * 0.35, 20);
    for (const p of planes) {
      if (p.hidden) continue;
      const m = this._planeMesh(p.frame, size, PLANE_COL, p.name);
      m.userData = { kind: 'plane', id: p.id };
      this.planesGroup.add(m);
    }
    this.requestRender();
  }

  syncSketches(sketches, activeId) {
    this.sketchGroup.clear();
    for (const s of sketches) {
      if (s.hidden || s.id === activeId) continue;
      this.sketchGroup.add(this.sketchLines(s.frame, s.entities, SKETCH_COL, 0.9));
    }
    this.requestRender();
  }

  sketchLines(frame, entities, color, opacity = 1, construction = true) {
    const [o, x, y] = frame.map(a => new THREE.Vector3(...a));
    const g = new THREE.Group();
    const to3 = ([u, v]) => o.clone().addScaledVector(x, u).addScaledVector(y, v);
    for (const e of entities) {
      const { pts, closed } = entityOutline(e);
      if (pts.length < 2) continue;
      const P = pts.map(to3); if (closed) P.push(P[0]);
      const mat = e.construction ? new THREE.LineDashedMaterial({ color: 0x8a6a55, dashSize: 1.5, gapSize: 1, transparent: true, opacity, depthTest: false })
        : new THREE.LineBasicMaterial({ color, transparent: true, opacity, depthTest: false });
      if (e.construction && !construction) continue;
      const ln = new THREE.Line(new THREE.BufferGeometry().setFromPoints(P), mat);
      if (e.construction) ln.computeLineDistances();
      ln.renderOrder = 10;
      ln.userData = { kind: 'sketch-entity', eid: e.id };
      g.add(ln);
    }
    return g;
  }

  // ---------------------------------------------------------------- overlay helpers
  clearOverlay(tag) {
    for (const c of [...this.overlay.children]) if (!tag || c.userData.tag === tag) { this.overlay.remove(c); c.traverse(x => { x.geometry?.dispose(); }); }
    this.requestRender();
  }

  highlightFace(bodyId, tris, tag = 'face', color = FACE_HL) {
    const o = this.bodies.get(bodyId); if (!o) return;
    const pos = o.geo.attributes.position.array, idx = o.geo.index.array;
    const out = new Float32Array(tris.length * 9);
    let k = 0;
    for (const t of tris) for (let j = 0; j < 3; j++) { const vi = idx[t * 3 + j] * 3; out[k++] = pos[vi]; out[k++] = pos[vi + 1]; out[k++] = pos[vi + 2]; }
    const g = new THREE.BufferGeometry(); g.setAttribute('position', new THREE.BufferAttribute(out, 3));
    const m = new THREE.Mesh(g, new THREE.MeshBasicMaterial({ color, transparent: true, opacity: 0.45, side: THREE.DoubleSide, depthWrite: false,
      polygonOffset: true, polygonOffsetFactor: -2, polygonOffsetUnits: -2 }));
    m.userData.tag = tag; m.renderOrder = 3;
    this.overlay.add(m);
    this.requestRender();
  }

  addMarker(p, color = 0xd9480f, tag = 'marker', size) {
    const s = size || Math.max(this.extent * 0.006, 0.3);
    const m = new THREE.Mesh(new THREE.SphereGeometry(s, 16, 12), new THREE.MeshBasicMaterial({ color, depthTest: false }));
    m.position.set(...p); m.renderOrder = 20; m.userData.tag = tag;
    this.overlay.add(m); this.requestRender();
    return m;
  }

  addArrow(p, dir, len, color = 0x1f6fd1, tag = 'arrow') {
    const a = new THREE.ArrowHelper(new THREE.Vector3(...dir).normalize(), new THREE.Vector3(...p), len, color, len * 0.25, len * 0.12);
    a.userData.tag = tag; a.traverse(c => { if (c.material) { c.material.depthTest = false; c.renderOrder = 20; } });
    this.overlay.add(a); this.requestRender();
  }

  addLine(a, b, color = 0x1f6fd1, tag = 'line') {
    const l = new THREE.Line(new THREE.BufferGeometry().setFromPoints([new THREE.Vector3(...a), new THREE.Vector3(...b)]),
      new THREE.LineBasicMaterial({ color, depthTest: false }));
    l.userData.tag = tag; l.renderOrder = 20; this.overlay.add(l); this.requestRender();
  }

  addPolyline(points, color = 0x1f6fd1, tag = 'line') {
    if (!points || points.length < 2) return;
    const l = new THREE.Line(new THREE.BufferGeometry().setFromPoints(points.map(p => new THREE.Vector3(...p))),
      new THREE.LineBasicMaterial({ color, depthTest: false, transparent: true, linewidth: 2 }));
    l.userData.tag = tag; l.renderOrder = 21; this.overlay.add(l); this.requestRender();
  }

  /** The rubber band of a wall being drawn, and a dot where the pointer meets the part. */
  setBand(points, cursor = null, color = 0x59d98e) {
    if (!this._band) {
      const g = new THREE.BufferGeometry();
      g.setAttribute('position', new THREE.BufferAttribute(new Float32Array(130 * 3), 3));
      this._band = new THREE.Line(g, new THREE.LineBasicMaterial({ color, depthTest: false, transparent: true }));
      this._band.frustumCulled = false; this._band.renderOrder = 22;
      this._cursor = new THREE.Mesh(new THREE.SphereGeometry(1, 16, 12), new THREE.MeshBasicMaterial({ color: 0xcffbe4, depthTest: false, transparent: true, opacity: 0.75 }));
      this._cursor.renderOrder = 23;
      this.scene.add(this._band, this._cursor);
    }
    const b = this._band, pts = (points || []).slice(0, 130);
    b.visible = pts.length >= 2;
    if (b.visible) {
      const a = b.geometry.attributes.position;
      pts.forEach((p, i) => a.setXYZ(i, p[0], p[1], p[2]));
      a.needsUpdate = true; b.geometry.setDrawRange(0, pts.length);
      b.material.color.set(color);
    }
    this._cursor.visible = !!cursor;
    if (cursor) { this._cursor.position.set(...cursor); this._cursor.scale.setScalar(this.pixelSize(cursor) * 5); }
    this.requestRender();
  }

  /**
   * Faces that point down more steeply than `thresholdDeg` from the plate, as Support Fins
   * judges them (overhangs.js analyze: not the faces on the bed, regions of 12 mm² or more).
   * Instant, from the mesh on screen, so the red shows before the fins are built.
   */
  overhangFaces(bodyId, thresholdDeg = 45) {
    const o = this.bodies.get(bodyId); if (!o) return [];
    const pos = o.geo.attributes.position.array, idx = o.geo.index.array, nf = idx.length / 3;
    const cut = -(Math.cos(thresholdDeg * Math.PI / 180) + 1e-4);
    let minZ = Infinity;
    for (let i = 2; i < pos.length; i += 3) if (pos[i] < minZ) minZ = pos[i];
    const over = new Uint8Array(nf), area = new Float64Array(nf);
    for (let f = 0; f < nf; f++) {
      const a = idx[f * 3] * 3, b = idx[f * 3 + 1] * 3, c = idx[f * 3 + 2] * 3;
      const top = Math.max(pos[a + 2], pos[b + 2], pos[c + 2]) - minZ;
      if (top < 0.35) continue;
      const ux = pos[b] - pos[a], uy = pos[b + 1] - pos[a + 1], uz = pos[b + 2] - pos[a + 2];
      const vx = pos[c] - pos[a], vy = pos[c + 1] - pos[a + 1], vz = pos[c + 2] - pos[a + 2];
      const nx = uy * vz - uz * vy, ny = uz * vx - ux * vz, nz = ux * vy - uy * vx, len = Math.hypot(nx, ny, nz);
      if (!(len > 0)) continue;
      if (nz / len < cut) { over[f] = 1; area[f] = len / 2; }
    }
    // join faces sharing a vertex into regions; drop the slivers
    const parent = new Int32Array(nf).map((_, i) => i);
    const find = x => { while (parent[x] !== x) x = parent[x] = parent[parent[x]]; return x; };
    const firstAt = new Int32Array(pos.length / 3).fill(-1);
    for (let f = 0; f < nf; f++) {
      if (!over[f]) continue;
      for (let j = 0; j < 3; j++) {
        const vi = idx[f * 3 + j], g = firstAt[vi];
        if (g < 0) firstAt[vi] = f; else { const ra = find(f), rb = find(g); if (ra !== rb) parent[ra] = rb; }
      }
    }
    const sum = new Map();
    for (let f = 0; f < nf; f++) if (over[f]) { const r = find(f); sum.set(r, (sum.get(r) || 0) + area[f]); }
    const out = [];
    for (let f = 0; f < nf; f++) if (over[f] && sum.get(find(f)) >= 12) out.push(f);
    return out;
  }

  addPlaneGhost(frame, tag = 'plane-ghost', color = PLANE_COL) {
    const m = this._planeMesh(frame, Math.max(this.extent * 0.35, 20), color, '');
    m.material.opacity = 0.22; m.userData.tag = tag; this.overlay.add(m); this.requestRender();
  }

  setBrush(b) {
    if (!b) { if (this._brush) { this._brush.visible = false; this.requestRender(); } return; }
    if (!this._brush) {
      const g = new THREE.RingGeometry(0.94, 1, 64);
      this._brush = new THREE.Mesh(g, new THREE.MeshBasicMaterial({ color: 0xe8763a, side: THREE.DoubleSide, depthTest: false, transparent: true, opacity: 0.9 }));
      this._brush.renderOrder = 40; this.scene.add(this._brush);
    }
    const m = this._brush; m.visible = true;
    m.position.set(...b.point); m.scale.setScalar(b.radius);
    m.quaternion.setFromUnitVectors(new THREE.Vector3(0, 0, 1), new THREE.Vector3(...b.normal).normalize());
    this.requestRender();
  }

  drawStrokes(strokes) {
    this.clearOverlay('strokes');
    for (const st of strokes) {
      if ((st.points || []).length < 2) continue;
      const l = new THREE.Line(new THREE.BufferGeometry().setFromPoints(st.points.map(p => new THREE.Vector3(...p))),
        new THREE.LineBasicMaterial({ color: { smooth: 0x2f9e44, flatten: 0x1f6fd1, inflate: 0xe8763a, deflate: 0x9c36b5 }[st.mode] || 0xe8763a, depthTest: false }));
      l.userData.tag = 'strokes'; l.renderOrder = 30; this.overlay.add(l);
    }
    this.requestRender();
  }

  addRegions(frame, faces, tag = 'regions') {
    const [o, x, y] = frame.map(a => new THREE.Vector3(...a));
    const n = new THREE.Vector3().crossVectors(x, y);
    for (const rings of faces) {
      const shape = new THREE.Shape(rings[0].map(p => new THREE.Vector2(p[0], p[1])));
      for (const h of rings.slice(1)) shape.holes.push(new THREE.Path(h.map(p => new THREE.Vector2(p[0], p[1]))));
      const m = new THREE.Mesh(new THREE.ShapeGeometry(shape), new THREE.MeshBasicMaterial({ color: 0x2f8cff, transparent: true, opacity: 0.4, side: THREE.DoubleSide, depthTest: false, depthWrite: false }));
      m.applyMatrix4(new THREE.Matrix4().makeBasis(x, y, n).setPosition(o));
      m.renderOrder = 12; m.userData.tag = tag; this.overlay.add(m);
    }
    this.requestRender();
  }

  // ---------------------------------------------------------------- picking
  ndc(ev) {
    const r = this.canvas.getBoundingClientRect();
    return new THREE.Vector2(((ev.clientX - r.left) / r.width) * 2 - 1, -((ev.clientY - r.top) / r.height) * 2 + 1);
  }

  pick(ev, { bodies = true, planes = true } = {}) {
    this.raycaster.setFromCamera(this.ndc(ev), this.camera);
    const objs = [];
    if (bodies) for (const o of this.bodies.values()) if (o.mesh.visible) objs.push(o.mesh);
    if (planes) {
      for (const c of this.planesGroup.children) objs.push(c);
      if (this.basePlanes.visible) for (const c of this.basePlanes.children) objs.push(c);
    }
    const hits = this.raycaster.intersectObjects(objs, false).filter(h => !this.section || this.section.distanceToPoint(h.point) >= -1e-6);
    if (!hits.length) return null;
    // prefer bodies over transparent planes when both are hit at nearly the same depth
    const bodyHit = hits.find(h => h.object.userData.kind === 'body');
    const h = bodyHit && (hits[0].object.userData.kind !== 'plane' || bodyHit.distance - hits[0].distance < this.extent * 0.002) ? bodyHit : hits[0];
    const ud = h.object.userData;
    if (ud.kind === 'body') {
      const n = h.face.normal.clone().transformDirection(h.object.matrixWorld);
      return { kind: 'body', id: ud.id, tri: h.faceIndex, point: h.point.toArray(), normal: n.toArray() };
    }
    return { kind: 'plane', id: ud.id, point: h.point.toArray() };
  }

  rayToPlane(ev, frame) {
    this.raycaster.setFromCamera(this.ndc(ev), this.camera);
    const [o, , , n] = frame.map(a => new THREE.Vector3(...a));
    const pl = new THREE.Plane().setFromNormalAndCoplanarPoint(n, o);
    const p = new THREE.Vector3();
    return this.raycaster.ray.intersectPlane(pl, p) ? p : null;
  }

  worldToScreen(p) {
    const v = new THREE.Vector3(...p).project(this.camera);
    const r = this.canvas.getBoundingClientRect();
    return [(v.x + 1) / 2 * r.width, (1 - v.y) / 2 * r.height];
  }

  pixelSize(atPoint) {
    // world units per screen pixel near a point
    const d = this.camera.position.distanceTo(new THREE.Vector3(...atPoint));
    const h = this.canvas.clientHeight || 1;
    return 2 * d * Math.tan(THREE.MathUtils.degToRad(this.camera.fov / 2)) / h;
  }

  // ---------------------------------------------------------------- views
  /** half the narrower field of view (tall, narrow screens are limited by the width) */
  halfFov() {
    const v = THREE.MathUtils.degToRad(this.camera.fov / 2);
    return Math.min(v, Math.atan(Math.tan(v) * Math.max(this.camera.aspect || 1, 0.05)));
  }

  fit(box) {
    box = box && !box.isEmpty() ? box : this.sceneBox();
    if (box.isEmpty()) box = new THREE.Box3(new THREE.Vector3(-50, -50, 0), new THREE.Vector3(50, 50, 50));
    const c = box.getCenter(new THREE.Vector3()), r = box.getSize(new THREE.Vector3()).length() / 2 || 50;
    const dir = this.camera.position.clone().sub(this.controls.target).normalize();
    const dist = r / Math.sin(this.halfFov()) * 1.05;
    this._flyTo(c, c.clone().addScaledVector(dir, dist), this.camera.up.clone());
  }

  setView(name) {
    const dirs = { top: [0, 0, 1], bottom: [0, 0, -1], front: [0, -1, 0], back: [0, 1, 0], right: [1, 0, 0], left: [-1, 0, 0], iso: [1, -1, 0.85] };
    if (name === 'fit') return this.fit();
    const d = new THREE.Vector3(...dirs[name]).normalize();
    const box = this.sceneBox();
    const c = box.isEmpty() ? new THREE.Vector3() : box.getCenter(new THREE.Vector3());
    const r = box.isEmpty() ? 60 : box.getSize(new THREE.Vector3()).length() / 2;
    const dist = r / Math.sin(this.halfFov()) * 1.05;
    const up = Math.abs(d.z) > 0.99 ? new THREE.Vector3(0, d.z > 0 ? 1 : -1, 0) : new THREE.Vector3(0, 0, 1);
    this._flyTo(c, c.clone().addScaledVector(d, dist), up);
  }

  lookAtPlane(frame, radius) {
    const [o, , y, n] = frame.map(a => new THREE.Vector3(...a));
    const box = this.sceneBox();
    const r = radius || (box.isEmpty() ? 60 : box.getSize(new THREE.Vector3()).length() / 2);
    let c = o.clone();
    if (!box.isEmpty()) { const bc = box.getCenter(new THREE.Vector3()); c = bc.sub(n.clone().multiplyScalar(n.dot(bc.clone().sub(o)))); }
    const dist = r / Math.sin(this.halfFov()) * 1.0;
    this._flyTo(c, c.clone().addScaledVector(n, dist), y);
  }

  _flyTo(target, pos, up) {
    const t0 = performance.now(), dur = 320;
    const sT = this.controls.target.clone(), sP = this.camera.position.clone(), sU = this.camera.up.clone();
    this._anim = () => {
      const k = Math.min(1, (performance.now() - t0) / dur), e = 1 - Math.pow(1 - k, 3);
      this.controls.target.lerpVectors(sT, target, e);
      this.camera.position.lerpVectors(sP, pos, e);
      this.camera.up.lerpVectors(sU, up, e).normalize();
      this.camera.lookAt(this.controls.target);
      if (k >= 1) { this._anim = null; this.camera.up.copy(up); this.controls.update(); }
    };
    this.requestRender();
  }

  // ---------------------------------------------------------------- section view
  setSection(axis, offset, flip) {
    if (!axis) { this.section = null; }
    else {
      // keep the half away from the camera, so the cut face looks at you
      const n = new THREE.Vector3(...{ x: [1, 0, 0], y: [0, 1, 0], z: [0, 0, 1] }[axis]);
      const camSide = this.camera.position.clone().sub(this.controls.target).dot(n) > 0;
      const keepPositive = camSide ? !!flip : !flip;
      this.section = keepPositive ? new THREE.Plane(n, -offset) : new THREE.Plane(n.negate(), offset);
    }
    for (const o of this.bodies.values()) this._applySection(o);
    this.requestRender();
  }

  _applySection(o) {
    const planes = this.section ? [this.section] : [];
    o.mesh.material.clippingPlanes = planes; o.mesh.material.needsUpdate = true;
    if (o.edges) { o.edges.material.clippingPlanes = planes; o.edges.material.needsUpdate = true; }
    if (this.section && !o.back) {
      o.back = new THREE.Mesh(o.mesh.geometry, new THREE.MeshBasicMaterial({ color: 0xd06060, side: THREE.BackSide, clippingPlanes: planes }));
      o.back.userData = { kind: 'back' };
      o.back.visible = o.mesh.visible;
      this.bodiesGroup.add(o.back);
    } else if (!this.section && o.back) { this.bodiesGroup.remove(o.back); o.back.material.dispose(); o.back = null; }
    else if (o.back) { o.back.material.clippingPlanes = planes; o.back.visible = o.mesh.visible; }
  }

  screenshot() { this.render(); return this.canvas.toDataURL('image/png'); }
}
