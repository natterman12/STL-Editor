// Feature evaluators (port of stleditor/features.py). Each feature reads and updates a State.
import { Manifold, CrossSection, crossSection } from './wasm.js';
import * as G from './geom.js';
import { Body, GeometryError, num, unit, add, sub, mul, dot, cross, norm, mat4, frameMatrix, eye3, det3, matVec3, transformVerts } from './geom.js';
import * as S from './sketch2d.js';
import * as PR from './printing.js';
import * as MT from './meshtools.js';
import * as VX from './voxel.js';
import * as ME from './meshedit.js';
import { decimate } from './decimate.js';
import * as SFX from './supportfins.js';

const OPS = ['new', 'join', 'cut', 'intersect'];

export class State {
  constructor() { this.bodies = new Map(); this.planes = new Map(); this.sketches = new Map(); this.tools = new Map(); }
  clone() {
    const s = new State();
    s.bodies = new Map(this.bodies); s.planes = new Map(this.planes); s.sketches = new Map(this.sketches); s.tools = new Map(this.tools);
    return s;
  }
  body(bid) {
    const b = this.bodies.get(bid);
    if (!b) throw new GeometryError(`Body ${JSON.stringify(bid)} no longer exists.`);
    return b;
  }
  frame(ref) {
    if (ref === null || ref === undefined) ref = 'XY';
    if (typeof ref === 'string') {
      if (G.BASE_PLANES[ref]) { const [o, n, x] = G.BASE_PLANES[ref]; return G.planeFrame(o, n, x); }
      if (this.planes.has(ref)) return this.planes.get(ref).frame;
      throw new GeometryError(`Plane ${JSON.stringify(ref)} no longer exists.`);
    }
    if (typeof ref === 'object') {
      if (ref.plane) return this.frame(ref.plane);
      if (ref.origin && ref.normal) {
        let fr = G.planeFrame(ref.origin, ref.normal, ref.xdir);
        if (ref.flip) fr = [fr[0], fr[1], mul(fr[2], -1), mul(fr[3], -1)];
        return fr;
      }
    }
    throw new GeometryError('Invalid plane reference.');
  }
}

const bboxOf = m => { const b = m.boundingBox(); return [b.min, b.max]; };
const overlaps = (a, b, tol = 1e-6) => [0, 1, 2].every(k => a[0][k] <= b[1][k] + tol && b[0][k] <= a[1][k] + tol);

export function applyTool(state, fid, tool, operation, targets, name, result, newName = null) {
  if (tool.isEmpty()) throw new GeometryError('The feature produced no geometry.');
  const op = operation || 'new';
  if (!OPS.includes(op)) throw new GeometryError(`Unknown operation ${op}.`);
  state.tools.set(fid, { tool, op, targets: [...(targets || [])], name });
  if (op === 'new') { state.bodies.set(fid, new Body(newName || name, { solid: tool })); result.created.push(fid); return; }
  const tb = bboxOf(tool);
  let tids;
  if (targets && targets.length) {
    tids = targets.filter(t => state.bodies.has(t));
    if (!tids.length) throw new GeometryError('The target body no longer exists.');
  } else tids = [...state.bodies].filter(([, b]) => b.isSolid && overlaps(bboxOf(b.solid), tb)).map(([k]) => k);
  if (!tids.length) {
    if (op === 'join') {
      state.bodies.set(fid, new Body(newName || name, { solid: tool }));
      result.created.push(fid);
      result.warning = 'Nothing to join to: made a new body.';
      return;
    }
    throw new GeometryError(`Nothing to ${op}: the tool doesn't touch any body.`);
  }
  if (op === 'join') {
    const first = tids[0];
    const solids = [state.body(first).requireSolid(), tool, ...tids.slice(1).map(t => state.body(t).requireSolid())];
    state.bodies.set(first, state.body(first).copy({ solid: Manifold.union(solids) }));
    for (const t of tids.slice(1)) state.bodies.delete(t);
    result.modified.push(first);
    return;
  }
  let changed = 0;
  for (const t of tids) {
    const b = state.body(t), s = b.requireSolid();
    if (!overlaps(bboxOf(s), tb)) { if (op === 'intersect') state.bodies.delete(t); continue; }
    const res = op === 'cut' ? s.subtract(tool) : s.intersect(tool);
    if (res.isEmpty()) { state.bodies.delete(t); (result.removed ||= []).push(t); }
    else { state.bodies.set(t, b.copy({ solid: res })); result.modified.push(t); }
    changed++;
  }
  if (!changed) throw new GeometryError(`Nothing to ${op}: the tool doesn't touch the target body.`);
}

function place(man, frame, offset = [0, 0, 0]) {
  const [o, x, y, n] = frame;
  const o2 = add(add(add(o, mul(x, +offset[0])), mul(y, +offset[1])), mul(n, +offset[2]));
  return man.transform(frameMatrix(o2, x, y, n));
}

export function rotation(rx, ry, rz) {
  [rx, ry, rz] = [rx, ry, rz].map(a => (+a || 0) * Math.PI / 180);
  const Rx = [[1, 0, 0], [0, Math.cos(rx), -Math.sin(rx)], [0, Math.sin(rx), Math.cos(rx)]];
  const Ry = [[Math.cos(ry), 0, Math.sin(ry)], [0, 1, 0], [-Math.sin(ry), 0, Math.cos(ry)]];
  const Rz = [[Math.cos(rz), -Math.sin(rz), 0], [Math.sin(rz), Math.cos(rz), 0], [0, 0, 1]];
  return G.matMul3(Rz, G.matMul3(Ry, Rx));
}

function transformBody(b, R, t) {
  if (b.isSolid) return b.copy({ solid: b.solid.transform(mat4(R, t)) });
  const { v, f } = b.mesh();
  let f2 = f;
  if (det3(R) < 0) { f2 = new Uint32Array(f.length); for (let i = 0; i < f.length; i += 3) { f2[i] = f[i]; f2[i + 1] = f[i + 2]; f2[i + 2] = f[i + 1]; } }
  return new Body(b.name, { verts: transformVerts(v, R, t), faces: f2 });
}

// ---------------------------------------------------------------- features
function f_import(state, fid, p, res, ctx) {
  const parts = ctx.loadSource(p.source);
  const repair = p.repair !== false;
  const placeMode = p.place || 'keep';
  let shift = [0, 0, 0];
  if (placeMode === 'center') {
    const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
    for (const pt of parts) { const [l, h] = G.vertsBBox(pt.v); for (let k = 0; k < 3; k++) { lo[k] = Math.min(lo[k], l[k]); hi[k] = Math.max(hi[k], h[k]); } }
    shift = [-(lo[0] + hi[0]) / 2, -(lo[1] + hi[1]) / 2, -lo[2]];
    const others = [...state.bodies.values()].map(b => b.bbox()).filter(bb => Number.isFinite(bb[1][0]));
    if (others.length) shift[0] = Math.max(...others.map(bb => bb[1][0])) + 10 - lo[0];
    if (norm(shift) > 1e-9) res.moved = shift.map(x => +x.toFixed(4));
  }
  let broken = 0;
  parts.forEach((pt, i) => {
    const v = norm(shift) > 0 ? transformVerts(pt.v, eye3(), shift) : pt.v;
    const f = pt.f;
    const bid = parts.length === 1 ? fid : `${fid}.${i + 1}`;
    let name = p.name || pt.name;
    if (parts.length > 1 && p.name) name = `${p.name} ${i + 1}`;
    let man = G.manifoldFromArrays(v, f);
    if (!man && repair) {
      try {
        man = G.localRepair(v, f);
        if (man) res.warning = 'The mesh had small defects (open or doubled edges); they were fixed automatically.';
      } catch { man = null; }
    }
    if (man) {
      const m = G.mergeOverlappingShells(man);
      if (m.merged) res.warning = `${m.merged} overlapping shells were merged into one solid.` + (res.warning ? ' ' + res.warning : '');
      state.bodies.set(bid, new Body(name, { solid: m.man }));
    } else { state.bodies.set(bid, new Body(name, { verts: v, faces: f })); broken++; }
    res.created.push(bid);
  });
  if (broken) res.warning = `${broken} mesh(es) are not watertight. They display and export, but editing needs Repair. Badly broken meshes need Repair › Rebuild.`;
}

function f_primitive(state, fid, p, res) {
  const shape = p.shape || 'box', seg = Math.floor(+(p.segments || 96)), anchor = p.anchor || 'base';
  let man;
  if (shape === 'box') {
    const L = num(p.length, 10), W = num(p.width, 10), H = num(p.height, 10);
    man = Manifold.cube([L, W, H], false);
    const shift = { corner: [0, 0, 0], base: [-L / 2, -W / 2, 0], center: [-L / 2, -W / 2, -H / 2] };
    man = man.translate(shift[anchor] || shift.base);
  } else if (shape === 'cylinder') {
    const r = num(p.radius, 5), h = num(p.height, 10);
    man = Manifold.cylinder(h, r, r, seg);
    if (anchor === 'center') man = man.translate([0, 0, -h / 2]);
  } else if (shape === 'cone') {
    const r1 = num(p.radius, 5), r2 = num(p.radius2, 0), h = num(p.height, 10);
    man = Manifold.cylinder(h, r1, Math.max(r2, 0), seg);
    if (anchor === 'center') man = man.translate([0, 0, -h / 2]);
  } else if (shape === 'sphere') {
    const r = num(p.radius, 5);
    man = Manifold.sphere(r, seg);
    if (anchor === 'base') man = man.translate([0, 0, r]);
  } else if (shape === 'torus') {
    const R = num(p.radius, 10), r = num(p.radius2, 2);
    if (r >= R) throw new GeometryError('Tube radius must be smaller than the ring radius.');
    man = Manifold.revolve(CrossSection.circle(r, Math.max(24, Math.floor(seg / 2))).translate([R, 0]), seg);
    if (anchor === 'base') man = man.translate([0, 0, r]);
  } else throw new GeometryError(`Unknown shape ${shape}.`);
  man = man.transform(mat4(rotation(...(p.rotate || [0, 0, 0]))));
  man = place(man, state.frame(p.plane), p.offset || [0, 0, 0]);
  applyTool(state, fid, man, p.operation || 'new', p.targets, p.body_name || shape[0].toUpperCase() + shape.slice(1), res);
}

function f_plane(state, fid, p, res) {
  const mode = p.mode || 'offset';
  let fr;
  if (mode === 'offset') {
    const [o, x, y, n] = state.frame(p.base);
    fr = [add(o, mul(n, num(p.distance, 0))), x, y, n];
  } else if (mode === 'angle') {
    const [o, x, y, n] = state.frame(p.base), a = num(p.angle, 45) * Math.PI / 180;
    if ((p.axis || 'x') === 'x') { const n2 = sub(mul(n, Math.cos(a)), mul(y, Math.sin(a))); fr = [o, x, cross(n2, x), n2]; }
    else { const n2 = add(mul(n, Math.cos(a)), mul(x, Math.sin(a))); fr = [o, cross(y, n2), y, n2]; }
    if (p.distance) fr = [add(fr[0], mul(fr[3], +p.distance)), fr[1], fr[2], fr[3]];
  } else if (mode === 'three_points') {
    const pts = p.points || [];
    if (pts.length !== 3) throw new GeometryError('Pick three points.');
    const n = cross(sub(pts[1], pts[0]), sub(pts[2], pts[0]));
    if (norm(n) < 1e-9) throw new GeometryError('The three points are in a line.');
    fr = G.planeFrame(pts[0], n, sub(pts[1], pts[0]));
  } else if (mode === 'midplane') {
    const a = state.frame(p.base), b = state.frame(p.base2);
    const n = unit(add(a[3], dot(a[3], b[3]) >= 0 ? b[3] : mul(b[3], -1)));
    fr = G.planeFrame(mul(add(a[0], b[0]), 0.5), n, a[1]);
  } else if (mode === 'face') {
    fr = state.frame(p.base);
    if (p.distance) fr = [add(fr[0], mul(fr[3], +p.distance)), fr[1], fr[2], fr[3]];
  } else throw new GeometryError(`Unknown plane mode ${mode}.`);
  if (p.flip) fr = [fr[0], fr[1], mul(fr[2], -1), mul(fr[3], -1)];
  state.planes.set(fid, { name: p.name || 'Plane', frame: fr, size: +(p.size || 0) });
  res.created_plane = fid;
}

function f_sketch(state, fid, p, res) {
  state.sketches.set(fid, { name: p.name || 'Sketch', frame: state.frame(p.plane), entities: p.entities || [], refs: p.refs || [] });
  res.created_sketch = fid;
}

function sketchOf(state, sid) {
  const s = state.sketches.get(sid);
  if (!s) throw new GeometryError('The sketch for this feature no longer exists.');
  return s;
}

function materialVolume(state, tool, targets) {
  const tb = bboxOf(tool);
  let vol = 0;
  for (const [k, b] of state.bodies) {
    if (targets && targets.length && !targets.includes(k)) continue;
    if (b.isSolid && overlaps(bboxOf(b.solid), tb)) vol += b.solid.intersect(tool).volume();
  }
  return vol;
}

function f_extrude(state, fid, p, res) {
  const sk = sketchOf(state, p.sketch);
  let cs = S.profile(sk.entities, p.entities, p.regions);
  const op = p.operation || 'new';
  let d1 = p.through_all ? G.bigExtent([...state.bodies.values()]) : num(p.distance, 10);
  let direction = p.direction || 'one';
  let d2 = direction === 'two' ? num(p.distance2, 0) : 0;
  if (direction === 'two' && p.through_all) d2 = d1;
  if (Math.abs(d1) < 1e-9 && Math.abs(d2) < 1e-9) throw new GeometryError("Distance can't be zero.");
  if (d1 < 0) { direction = { one: 'reverse', reverse: 'one' }[direction] || direction; d1 = -d1; }
  const start = num(p.start_offset, 0);
  let z0, h;
  if (direction === 'one') [z0, h] = [0, d1];
  else if (direction === 'reverse') [z0, h] = [-d1, d1];
  else if (direction === 'symmetric') [z0, h] = [-d1 / 2, d1];
  else if (direction === 'two') [z0, h] = [-d2, d1 + d2];
  else throw new GeometryError(`Unknown direction ${direction}.`);
  const taper = +(p.taper || 0);
  let man;
  if (taper) {
    const b = cs.bounds(), c = [(b.min[0] + b.max[0]) / 2, (b.min[1] + b.max[1]) / 2];
    const half = Math.max(b.max[0] - b.min[0], b.max[1] - b.min[1]) / 2;
    const k = half > 0 ? Math.max(0, (half - h * Math.tan(taper * Math.PI / 180)) / half) : 1;
    cs = cs.translate([-c[0], -c[1]]);
    man = Manifold.extrude(cs, h, 0, 0, [k, k]).translate([c[0], c[1], z0 + start]);
  } else man = Manifold.extrude(cs, h).translate([0, 0, z0 + start]);
  const M = frameMatrix(...sk.frame);
  let tool = man.transform(M);
  if ((op === 'cut' || op === 'intersect') && (direction === 'one' || direction === 'reverse') && p.auto_flip !== false) {
    const eps = Math.max(tool.volume() * 1e-5, 1e-9);
    if (materialVolume(state, tool, p.targets) <= eps) {
      const flipped = man.mirror([0, 0, 1]).transform(M);
      if (materialVolume(state, flipped, p.targets) > eps) { tool = flipped; res.flipped = true; }
    }
  }
  applyTool(state, fid, tool, op, p.targets, p.body_name || 'Extrude', res);
}

function f_revolve(state, fid, p, res) {
  const sk = sketchOf(state, p.sketch);
  const cs = S.profile(sk.entities, p.entities, p.regions);
  const axis = p.axis || 'y';
  let p0, d;
  if (axis === 'x') [p0, d] = [[0, 0], [1, 0]];
  else if (axis === 'y') [p0, d] = [[0, 0], [0, 1]];
  else [p0, d] = S.findAxis(sk.entities, axis);
  const L = Math.hypot(d[0], d[1]), e = [d[0] / L, d[1] / L], f = [e[1], -e[0]];
  // 2D map: sketch (u,v) -> local (a,b): a along f (radial), b along e (axis); Mat3 column-major
  const tx = -(f[0] * p0[0] + f[1] * p0[1]), ty = -(e[0] * p0[0] + e[1] * p0[1]);
  const local = cs.transform([f[0], e[0], 0, f[1], e[1], 0, tx, ty, 1]);
  const ang = num(p.angle, 360), seg = Math.floor(+(p.segments || 128));
  const b = local.bounds();
  const big = Math.max(...b.min.map(Math.abs), ...b.max.map(Math.abs)) * 4 + 10;
  const right = CrossSection.square([big, big * 2]).translate([0, -big]);
  const rp = local.intersect(right), lp = local.subtract(right).mirror([1, 0]);
  const parts = [rp, lp].filter(x => !x.isEmpty() && x.area() > 1e-12).map(x => Manifold.revolve(x, seg, ang));
  if (!parts.length) throw new GeometryError('The profile is empty.');
  let man = parts.length > 1 ? Manifold.union(parts) : parts[0];
  const [o, x, y] = sk.frame;
  const F = add(mul(x, f[0]), mul(y, f[1])), E = add(mul(x, e[0]), mul(y, e[1])), Gd = cross(E, F);
  const origin = add(add(o, mul(x, p0[0])), mul(y, p0[1]));
  man = man.transform(frameMatrix(origin, F, Gd, E));
  applyTool(state, fid, man, p.operation || 'new', p.targets, p.body_name || 'Revolve', res);
}

function f_hole(state, fid, p, res) {
  const pts = p.points && p.points.length ? p.points : (p.point ? [p.point] : []);
  if (!pts.length) throw new GeometryError('Click on a face to place the hole.');
  const n = unit(p.normal || [0, 0, 1]);
  const d = num(p.diameter, 5), r = d / 2;
  const targets = p.targets;
  const tbodies = targets && targets.length ? targets.filter(t => state.bodies.has(t)).map(t => state.bodies.get(t)) : [...state.bodies.values()].filter(b => b.isSolid);
  const through = p.through !== false;
  const depth = through || !p.depth ? G.bigExtent(tbodies) : num(p.depth, 10);
  const kind = p.kind || 'simple', eps = 0.01, neg = mul(n, -1);
  const tools = [];
  for (const pt0 of pts) {
    const pt = pt0.map(Number);
    tools.push(G.makeCylinder(add(pt, mul(n, eps)), neg, depth + eps, r));
    if (!through && p.depth && p.tip !== false) tools.push(G.makeCylinder(add(pt, mul(n, -depth)), neg, r / Math.tan(59 * Math.PI / 180), r, 0.001));
    if (kind === 'counterbore') {
      const cbd = num(p.cb_diameter, d * 1.8), cbh = num(p.cb_depth, d * 0.6);
      tools.push(G.makeCylinder(add(pt, n), neg, cbh + 1, cbd / 2));
    } else if (kind === 'hex') {
      const af = num(p.hex_af, d * 1.8), hd = num(p.hex_depth, 3), rr = af / Math.sqrt(3);
      tools.push(G.makeCylinder(add(pt, n), neg, hd + 1, rr, rr, 6));
    } else if (kind === 'countersink') {
      const csd = num(p.cs_diameter, d * 2), ang = num(p.cs_angle, 90);
      const h = (csd - d) / 2 / Math.tan(ang / 2 * Math.PI / 180), extra = 1;
      tools.push(G.makeCylinder(add(pt, mul(n, extra)), neg, h + extra, csd / 2 + extra * Math.tan(ang / 2 * Math.PI / 180), r));
    }
  }
  applyTool(state, fid, tools.length > 1 ? Manifold.union(tools) : tools[0], 'cut', targets, 'Hole', res);
}

function f_combine(state, fid, p, res) {
  const tgt = p.target;
  const tools = (p.tools || []).filter(t => t !== tgt);
  if (!tgt || !tools.length) throw new GeometryError('Choose a target body and at least one tool body.');
  const b = state.body(tgt), s = b.requireSolid();
  let ts = tools.map(t => state.body(t).requireSolid());
  const op = p.operation || 'join';
  let out;
  if (op === 'join') out = Manifold.union([s, ...ts]);
  else if (op === 'cut') {
    const c = +(p.clearance || 0);
    if (c > 0) ts = ts.map(t => MT.offsetSolid(t, c));
    out = s.subtract(ts.length > 1 ? Manifold.union(ts) : ts[0]);
  } else if (op === 'intersect') { out = s; for (const t of ts) out = out.intersect(t); }
  else throw new GeometryError(`Unknown operation ${op}.`);
  if (out.isEmpty()) throw new GeometryError('The result is empty.');
  state.bodies.set(tgt, b.copy({ solid: out }));
  res.modified.push(tgt);
  if (!p.keep_tools) for (const t of tools) state.bodies.delete(t);
}

function f_split(state, fid, p, res) {
  const bid = p.body, b = state.body(bid), s = b.requireSolid();
  const [o, , , n] = state.frame(p.plane);
  const keep = p.keep || 'both';
  const conn = keep === 'both' ? (p.connector || 'none') : 'none';
  let { above, below, extras, info } = PR.splitWithConnectors(s, o, n, conn, p.pin_d, p.pin_len, p.clearance);
  if (conn !== 'none' && info.skipped) res.warning = 'The cut face is too thin for connectors here; split without them.';
  else if (conn !== 'none') { res.warning = null; res.connectors = info.connectors; }
  extras.forEach((ex, i) => { state.bodies.set(`${fid}.pin${i + 1}`, new Body(`Pin ${i + 1}`, { solid: ex })); res.created.push(`${fid}.pin${i + 1}`); });
  if (keep === 'above') below = null;
  else if (keep === 'below') above = null;
  const empty = m => !m || m.isEmpty();
  if (empty(above) && empty(below)) throw new GeometryError('Nothing left after the split.');
  if ((empty(above) || empty(below)) && keep === 'both') throw new GeometryError("The plane doesn't cut through this body.");
  const [first, second] = !empty(above) ? [above, below] : [below, null];
  state.bodies.set(bid, b.copy({ name: empty(second) ? b.name : `${b.name} (1)`, solid: first }));
  res.modified.push(bid);
  if (!empty(second)) { state.bodies.set(`${fid}.2`, new Body(`${b.name} (2)`, { solid: second })); res.created.push(`${fid}.2`); }
}

function mirrorRT(frame) {
  const [o, , , n0] = frame, n = unit(n0);
  const R = [0, 1, 2].map(i => [0, 1, 2].map(j => (i === j ? 1 : 0) - 2 * n[i] * n[j]));
  return [R, mul(n, 2 * dot(o, n))];
}

function repeatFeatures(state, fid, featureIds, transforms, res, verb) {
  if (!featureIds.length) throw new GeometryError(`Choose the feature(s) to ${verb}.`);
  let done = 0, k = 0;
  const failed = [];
  for (const src of featureIds) {
    const rec = state.tools.get(src);
    if (!rec) throw new GeometryError(`A chosen feature can't be ${verb}ed (it isn't an extrude, revolve, hole or shape that comes earlier in the timeline, or it failed).`);
    for (const [R, t] of transforms) {
      k++;
      const sub2 = { created: [], modified: [] };
      try {
        applyTool(state, `${fid}.${k}`, rec.tool.transform(mat4(R, t)), rec.op, rec.targets.filter(x => state.bodies.has(x)), rec.name, sub2);
        res.created.push(...sub2.created);
        for (const m of sub2.modified) if (!res.modified.includes(m)) res.modified.push(m);
        (res.removed ||= []).push(...(sub2.removed || []));
        done++;
      } catch (e) { if (!(e instanceof GeometryError)) throw e; failed.push(e.message); }
      state.tools.delete(`${fid}.${k}`);
    }
  }
  if (!done) throw new GeometryError(failed[0] || 'Nothing changed.');
  if (failed.length) res.warning = `${failed.length} cop${failed.length === 1 ? 'y' : 'ies'} didn't touch the part and were skipped.`;
}

function f_mirror(state, fid, p, res) {
  const [R, t] = mirrorRT(state.frame(p.plane));
  if (p.kind === 'features') return repeatFeatures(state, fid, p.features || [], [[R, t]], res, 'mirror');
  const bids = p.bodies || [];
  if (!bids.length) throw new GeometryError('Choose a body to mirror.');
  const mode = p.mode || 'join';
  bids.forEach((bid, i) => {
    const b = state.body(bid), mb = transformBody(b, R, t);
    if (mode === 'replace') { state.bodies.set(bid, mb); res.modified.push(bid); }
    else if (mode === 'join') { state.bodies.set(bid, b.copy({ solid: b.requireSolid().add(mb.requireSolid()) })); res.modified.push(bid); }
    else { const id = bids.length === 1 ? fid : `${fid}.${i + 1}`; mb.name = `${b.name} (mirror)`; state.bodies.set(id, mb); res.created.push(id); }
  });
}

function pivot(b, pv) {
  if (pv === 'origin') return [0, 0, 0];
  const [lo, hi] = b.bbox();
  if (pv === 'base') return [(lo[0] + hi[0]) / 2, (lo[1] + hi[1]) / 2, lo[2]];
  return mul(add(lo, hi), 0.5);
}

function f_move(state, fid, p, res) {
  const bids = p.bodies || [];
  if (!bids.length) throw new GeometryError('Choose a body to move.');
  const R = rotation(...(p.rotate || [0, 0, 0])), tr = (p.translate || [0, 0, 0]).map(Number);
  bids.forEach((bid, i) => {
    const b = state.body(bid), c = pivot(b, p.pivot || 'center');
    let nb = transformBody(b, R, add(sub(c, matVec3(R, c)), tr));
    // Land on bed: a turned part rests on the bed (Move Z then lifts it off), so it never
    // hangs through the bed after a rotation
    if (p.bed && (p.rotate || []).some(a => Math.abs(+a) > 1e-9)) {
      const dz = tr[2] - nb.bbox()[0][2];
      if (Math.abs(dz) > 1e-9) nb = transformBody(nb, rotation(0, 0, 0), [0, 0, dz]);
    }
    if (p.copy) { const id = bids.length === 1 ? fid : `${fid}.${i + 1}`; nb.name = `${b.name} (copy)`; state.bodies.set(id, nb); res.created.push(id); }
    else { state.bodies.set(bid, nb); res.modified.push(bid); }
  });
}

function f_scale(state, fid, p, res) {
  const bids = p.bodies || [];
  if (!bids.length) throw new GeometryError('Choose a body to scale.');
  for (const bid of bids) {
    const b = state.body(bid), [lo, hi] = b.bbox(), size = sub(hi, lo);
    const mode = p.mode || 'factor';
    let k;
    if (mode === 'factor') { const f = p.factor ?? 1; k = Array.isArray(f) ? f.map(Number) : [+f, +f, +f]; }
    else if (mode === 'fit') {
      const target = (p.size || size).map(Number);
      k = target.map((t, i) => size[i] > 1e-9 ? t / size[i] : 1);
      if (p.uniform !== false) {
        const diffs = target.map((t, i) => Math.abs(t - size[i]));
        const axis = p.axis !== undefined && p.axis !== null ? +p.axis : diffs.indexOf(Math.max(...diffs));
        k = [k[axis], k[axis], k[axis]];
      }
    } else if (mode === 'units') {
      const s = { in_to_mm: 25.4, mm_to_in: 1 / 25.4, cm_to_mm: 10, m_to_mm: 1000 }[p.units || 'in_to_mm'] || 1;
      k = [s, s, s];
    } else throw new GeometryError(`Unknown scale mode ${mode}.`);
    if (k.some(x => !(x > 0))) throw new GeometryError('Scale factors must be positive.');
    const c = pivot(b, p.pivot || 'base');
    const R = [[k[0], 0, 0], [0, k[1], 0], [0, 0, k[2]]];
    state.bodies.set(bid, transformBody(b, R, sub(c, matVec3(R, c))));
    res.modified.push(bid);
  }
}

function f_repair(state, fid, p, res) {
  const bid = p.body, b = state.body(bid);
  const { v, f } = b.mesh();
  const mode = p.mode || 'auto';
  let man = null;
  if (mode === 'auto' || mode === 'quick') {
    man = G.localRepair(v, f) || G.quickRepair(v, f);
    if (!man && mode === 'quick') throw new GeometryError("Quick repair wasn't enough. Use Rebuild.");
  }
  if (!man) {
    const pitch = p.voxel ? +p.voxel : VX.autoPitch(v, { draft: 120, normal: 220, fine: 360 }[p.resolution] || 220);
    man = VX.voxelRebuild(v, f, pitch);
    res.warning = `Rebuilt as a watertight solid on a ${+pitch.toPrecision(3)} mm grid.`;
  }
  state.bodies.set(bid, new Body(b.name, { solid: man }));
  res.modified.push(bid);
}

// ---------------------------------------------------------------- Support Fins
// Breakaway support fins from Support Fins (github.com/gittrahan/support-fins) for the part as
// it is posed now: tip it to the orientation you want to print, then add fins.
function addSupports(state, fid, res, bid, solids, output, suffix = 'supports', ctx = {}) {
  const b = state.body(bid);
  const fins = solids.length > 1 ? Manifold.union(solids) : solids[0];
  if (ctx.preview && output !== 'separate' && b.isSolid) {
    // the live preview shows the supports next to the untouched part (no union to wait for,
    // and only the supports are sent to the screen); OK joins them for real
    let nid = `${fid}~sf`;
    state.bodies.set(nid, new Body(`${b.name} ${suffix}`, { solid: fins }));
    res.created.push(nid);
    res.previewOnly = true;
    res.colors = { ...(res.colors || {}), [nid]: SUPPORT_COLOR };
    return;
  }
  if (output === 'separate' || !b.isSolid) {
    let nid = fid, n = 1;
    while (state.bodies.has(nid)) nid = `${fid}.${++n}`;
    state.bodies.set(nid, new Body(`${b.name} ${suffix}`, { solid: fins }));
    res.created.push(nid);
    if (ctx.preview) res.colors = { ...(res.colors || {}), [nid]: SUPPORT_COLOR };
    if (!b.isSolid && output !== 'separate') res.warning = `"${b.name}" isn't a watertight solid, so the supports were added as a separate body.`;
  } else {
    state.bodies.set(bid, b.copy({ solid: b.solid.add(fins) }));
    res.modified.push(bid);
  }
}
const SUPPORT_COLOR = '#59d98e';   // Support Fins' own fin green

function f_supportfins(state, fid, p, res, ctx = {}) {
  const ids = p.bodies || (p.body ? [p.body] : []);
  if (!ids.length) throw new GeometryError('Choose the body to support.');
  const notes = [];
  for (const bid of ids) {
    const b = state.body(bid);
    const mesh = b.mesh();
    const r = SFX.fins(mesh, p.options || {});
    res.paint = { body: bid, over: r.over, small: r.small };
    // fins you clicked away, and walls you drew by hand (Support Fins' "+ Add walls by hand")
    const { gone, missing } = SFX.matchRemoved(r.pieces, p.removed);
    const hand = drawnWalls(p).filter(w => (w.points || []).length >= (w.shape === 'anchor' ? 1 : 2));
    const wr = hand.length ? SFX.walls(mesh, hand, p.options || {}) : null;
    if (wr) res.walls = wr.walls.map(w => ({ ok: w.ok, reason: w.reason || null }));
    const extra = [gone.size ? `${gone.size} removed` : '', wr ? `${wr.walls.filter(w => w.ok).length} drawn by hand` : ''].filter(Boolean).join(', ');
    res.info = `Support Fins ${r.version}: ${r.report}${extra ? ` (${extra})` : ''}`;
    if (missing) notes.push(`${missing} fin(s) you removed aren't there any more in this build, so nothing was removed for them.`);
    if (wr) wr.walls.forEach((w, i) => { if (!w.ok || w.failed) notes.push(`Hand wall ${i + 1}${w.ok ? ' (part of it)' : ''}: ${w.reason}.`); });
    const kept = r.solids.filter((_, i) => !gone.has(i));
    const all = [...kept, ...(wr ? wr.solids : [])];
    // the plastic the supports cost (the site's grams receipt; the panel turns it into grams)
    res.support = { volume: Math.round(all.reduce((t, m) => t + m.volume(), 0) * 10) / 10, pieces: all.length };
    if (ctx.preview) {
      // every fin a body of its own beside the untouched part, so one can be clicked to remove
      // it; removed ones stay as see-through ghosts to click back
      res.pieces = {};
      res.colors = res.colors || {};
      res.ghosts = res.ghosts || [];
      res.previewOnly = true;
      const put = (nid, solid, info) => {
        state.bodies.set(nid, new Body(`${b.name} supports`, { solid }));
        res.created.push(nid); res.pieces[nid] = info; res.colors[nid] = info.removed ? '#d64545' : SUPPORT_COLOR;
        if (info.removed) res.ghosts.push(nid);
      };
      r.solids.forEach((sd, i) => put(`${fid}~p${i}`, sd, { ...r.pieces[i], removed: gone.has(i) }));
      if (wr) wr.byWall.forEach((list, i) => { if (list.length) put(`${fid}~w${i}`, list.length > 1 ? Manifold.union(list) : list[0], { kind: 'hand', wall: i }); });
      if (!all.length && !r.stats.overhangRegions && !hand.length) notes.push(`"${b.name}" has no overhangs in this position. Turn it the way you want to print it (drag the rings), then add fins.`);
      continue;
    }
    if (!all.length) {
      notes.push(r.stats.overhangRegions || hand.length
        ? `"${b.name}": ${r.report}. Nothing could be placed; try Full coverage, draw walls by hand, or tip the part differently.`
        : `"${b.name}" has no overhangs in this position. Rotate it to the way you want to print it, then add fins.`);
      continue;
    }
    if (r.skipped) notes.push(`${r.skipped} support piece(s) weren't closed solids and were left out.`);
    addSupports(state, fid, res, bid, all, p.output, 'supports', ctx);
  }
  if (notes.length) res.warning = notes.join(' ');
}

/** The walls of a Draw Walls feature (older projects keep one wall in `points`). */
export function drawnWalls(p) {
  if (Array.isArray(p.walls)) return p.walls;
  return (p.points || []).length === 2 ? [{ shape: 'line', points: p.points }] : [];
}

function f_supportwall(state, fid, p, res, ctx = {}) {
  const bid = p.body, b = state.body(bid);
  const list = drawnWalls(p).filter(w => (w.points || []).length >= (w.shape === 'anchor' ? 1 : 2));
  if (!list.length) throw new GeometryError('Click on the part to draw a wall under an overhang.');
  const r = SFX.walls(b.mesh(), list, p.options || {});
  res.walls = r.walls.map(w => ({ ok: w.ok, reason: w.reason || null }));
  const good = r.walls.filter(w => w.ok);
  const bad = r.walls.map((w, i) => [w, i]).filter(([w]) => !w.ok || w.failed);
  if (!good.length) {
    throw new GeometryError(list.length === 1 ? `Support Fins couldn’t place that wall: ${r.walls[0].reason}.`
      : `Support Fins couldn’t place any of the ${list.length} walls (${r.walls[0].reason}).`);
  }
  res.support = { volume: Math.round(r.solids.reduce((t, m) => t + m.volume(), 0) * 10) / 10, pieces: r.solids.length };
  const len = good.reduce((s, w) => s + w.length, 0), tines = good.reduce((s, w) => s + w.tines, 0);
  const tall = Math.max(...good.map(w => w.height));
  res.info = good.length === 1
    ? `Support Fins ${r.version}: wall ${len.toFixed(1)} mm long, ${tall.toFixed(1)} mm tall, ${tines} tines${good[0].partAttached ? ', standing on the part' : ''}`
    : `Support Fins ${r.version}: ${good.length} walls, ${len.toFixed(1)} mm in all, ${tines} tines`;
  if (bad.length) res.warning = bad.map(([w, i]) => `Wall ${i + 1}${w.ok ? ' (part of it)' : ''}: ${w.reason}`).join('. ') + '.';
  addSupports(state, fid, res, bid, r.solids, p.output, 'walls', ctx);
}

function hollowPitch(s, t, q) {
  const bb = s.boundingBox(), diag = Math.hypot(bb.max[0] - bb.min[0], bb.max[1] - bb.min[1], bb.max[2] - bb.min[2]);
  const div = { draft: 80, normal: 150, fine: 260 }[q || 'normal'] || 150;
  return Math.max(Math.min(t / 2.5, diag / div), diag / (div * 2.5));
}

function f_hollow(state, fid, p, res) {
  const bid = p.body, b = state.body(bid), s = b.requireSolid();
  const t = num(p.thickness, 2);
  state.bodies.set(bid, b.copy({ solid: VX.hollow(s, t, hollowPitch(s, t, p.resolution)) }));
  res.modified.push(bid);
}

// Hollow the body, then open each picked flat face: the hollow space just under the wall (kept
// inside the face) is pushed out through it. Two passes, so faces that meet at an edge also take
// the wall strip along that edge. No faces: a closed hollow, like Hollow.
function f_shell(state, fid, p, res) {
  const bid = p.body;
  if (!bid) throw new GeometryError('Choose a body to shell.');
  const b = state.body(bid), s = b.requireSolid();
  const t = num(p.thickness, 2);
  if (!(t > 0)) throw new GeometryError('Wall thickness must be positive.');
  const pitch = hollowPitch(s, t, p.resolution);
  const cavity = VX.hollow(s, t, pitch, true);
  const depth = t + 2 * pitch;
  const picked = (p.faces || []).filter(Boolean).map((face, k) => {
    if (!face.contours || !face.contours.length) throw new GeometryError(`Face ${k + 1} isn't flat. Shell can only open flat faces.`);
    const fr = G.planeFrame(face.origin, face.normal, face.xdir);
    return { i: k + 1, fr, outline: crossSection(face.contours.map(c => c.map(q => [+q[0], +q[1]])), 'EvenOdd') };
  });
  const openings = space => picked.map(({ i, fr, outline }) => {
    const hole = space.transform(G.worldToFrame(...fr)).slice(-depth).intersect(outline);
    if (hole.isEmpty() || hole.area() < 1e-6) throw new GeometryError(`There's no hollow space behind face ${i} to open. Try a thinner wall.`);
    return Manifold.extrude(hole, depth + pitch).translate([0, 0, -depth]).transform(frameMatrix(...fr));
  });
  let cut = cavity;
  if (picked.length) cut = Manifold.union([cavity, ...openings(Manifold.union([cavity, ...openings(cavity)]))]);
  state.bodies.set(bid, b.copy({ solid: s.subtract(cut) }));
  res.modified.push(bid);
}

function f_sculpt(state, fid, p, res) {
  const bid = p.body, b = state.body(bid);
  const strokes = p.strokes || [];
  if (!strokes.length) throw new GeometryError('Drag over the model to sculpt.');
  state.bodies.set(bid, b.copy({ solid: ME.sculpt(b.requireSolid(), strokes) }));
  res.modified.push(bid);
}

function f_remesh(state, fid, p, res) {
  const bid = p.body, b = state.body(bid);
  state.bodies.set(bid, b.copy({ solid: ME.remesh(b.requireSolid(), num(p.edge, 1)) }));
  res.modified.push(bid);
}

function f_simplify(state, fid, p, res) {
  // same steps as geometry.simplify on the desktop: quadric decimation to the requested share of
  // triangles (works on open meshes too), then make it a solid again if possible
  const bid = p.body, b = state.body(bid);
  if (p.tolerance && b.isSolid && !p.ratio) {
    state.bodies.set(bid, b.copy({ solid: b.solid.simplify(+p.tolerance) }));
    res.modified.push(bid);
    return;
  }
  const ratio = Math.min(Math.max(+(p.ratio || 0.5), 0.001), 1);
  const { v, f } = b.mesh();
  const nt = f.length / 3;
  const d = decimate(v, f, Math.floor(ratio * nt));
  const w = G.weld(d.v, d.f);
  let man = G.manifoldFromArrays(w.v, w.f);
  if (!man) man = G.quickRepair(w.v, w.f);
  if (!man && b.isSolid) {
    // fall back to Manifold's own (topology-safe) simplification
    const [lo, hi] = b.bbox();
    let tol = norm(sub(hi, lo)) * 1e-3;
    man = b.solid;
    for (let i = 0; i < 12; i++) { man = b.solid.simplify(tol); if (man.numTri() <= nt * ratio * 1.15) break; tol *= 1.8; }
  }
  if (man) state.bodies.set(bid, b.copy({ solid: man }));
  else {
    state.bodies.set(bid, new Body(b.name, { verts: w.v, faces: w.f }));
    res.warning = "Simplified mesh isn't watertight; run Repair to edit it.";
  }
  res.modified.push(bid);
}

function patternTransforms(p) {
  const count = Math.floor(+(p.count || 3));
  if (count < 2) throw new GeometryError('Count must be at least 2.');
  const out = [];
  for (let i = 1; i < count; i++) {
    if ((p.kind || 'linear') === 'linear') out.push([eye3(), mul(unit(p.direction || [1, 0, 0]), num(p.spacing, 10) * i)]);
    else {
      const axis = unit(p.axis || [0, 0, 1]), c = (p.center || [0, 0, 0]).map(Number);
      const total = num(p.angle, 360), step = Math.abs(total) >= 360 ? total / count : total / (count - 1);
      const a = step * i * Math.PI / 180;
      const K = [[0, -axis[2], axis[1]], [axis[2], 0, -axis[0]], [-axis[1], axis[0], 0]];
      const K2 = G.matMul3(K, K);
      const R = [0, 1, 2].map(r => [0, 1, 2].map(j => (r === j ? 1 : 0) + Math.sin(a) * K[r][j] + (1 - Math.cos(a)) * K2[r][j]));
      out.push([R, sub(c, matVec3(R, c))]);
    }
  }
  return out;
}

function f_pattern(state, fid, p, res) {
  const T = patternTransforms(p);
  if (p.source === 'features') return repeatFeatures(state, fid, p.features || [], T, res, 'pattern');
  const bids = p.bodies || [];
  if (!bids.length) throw new GeometryError('Choose a body to pattern.');
  for (const bid of bids) {
    const b = state.body(bid), s = b.requireSolid();
    const copies = [s, ...T.map(([R, t]) => s.transform(mat4(R, t)))];
    if (p.join !== false) { state.bodies.set(bid, b.copy({ solid: Manifold.union(copies) })); res.modified.push(bid); }
    else copies.slice(1).forEach((c, i) => { const id = `${fid}.${bid}.${i + 2}`; state.bodies.set(id, new Body(`${b.name} (${i + 2})`, { solid: c })); res.created.push(id); });
  }
}

function f_delete(state, fid, p, res) {
  for (const bid of p.bodies || []) if (state.bodies.has(bid)) { state.bodies.delete(bid); (res.removed ||= []).push(bid); }
}

function f_separate(state, fid, p, res) {
  const bid = p.body, b = state.body(bid);
  const pieces = b.requireSolid().decompose();
  if (pieces.length < 2) throw new GeometryError('This body is already one piece.');
  pieces.sort((a, c) => c.volume() - a.volume());
  state.bodies.set(bid, b.copy({ solid: pieces[0] }));
  res.modified.push(bid);
  pieces.slice(1).forEach((pc, i) => { const id = `${fid}.${i + 2}`; state.bodies.set(id, new Body(`${b.name} (${i + 2})`, { solid: pc })); res.created.push(id); });
}

function f_bedsplit(state, fid, p, res) {
  const bid = p.body, b = state.body(bid), s = b.requireSolid();
  const bed = p.bed || (PR.PRINTERS[p.printer || 'bambu_x1'] || PR.PRINTERS.bambu_x1)[1];
  const { pieces, extras, info } = PR.splitToBed(s, bed, +(p.margin ?? 5), p.connector || 'pins', p.pin_d, p.pin_len, p.clearance, p.optimize !== false);
  if (pieces.length === 1) res.warning = 'This body already fits the bed; nothing was split.';
  res.info = info;
  state.bodies.set(bid, b.copy({ name: pieces.length > 1 ? `${b.name} 1` : b.name, solid: pieces[0] }));
  res.modified.push(bid);
  pieces.slice(1).forEach((pc, i) => { const id = `${fid}.${i + 2}`; state.bodies.set(id, new Body(`${b.name} ${i + 2}`, { solid: pc })); res.created.push(id); });
  extras.forEach((ex, i) => { const id = `${fid}.pin${i + 1}`; state.bodies.set(id, new Body(`Pin ${i + 1}`, { solid: ex })); res.created.push(id); });
  if (info.faces_without_connectors && (p.connector || 'pins') !== 'none')
    res.warning = `${info.faces_without_connectors} cut face(s) were too thin for connectors. Try Filament pins (1.75 mm), or glue those seams.`;
}

function f_pushpull(state, fid, p, res) {
  const bid = p.body, b = state.body(bid);
  b.requireSolid();
  const face = p.face || {};
  if (!face.contours || !face.contours.length) throw new GeometryError('Click a flat face to push or pull.');
  const fr = G.planeFrame(face.origin, face.normal, face.xdir);
  const d = num(p.distance, 5);
  applyTool(state, fid, PR.pushpullTool(face.contours, fr, d), d > 0 ? 'join' : 'cut', [bid], 'Push/Pull', res);
}

function f_layflat(state, fid, p, res) {
  const bids = p.bodies && p.bodies.length ? p.bodies : (p.body ? [p.body] : []);
  if (!bids.length) throw new GeometryError('Choose a body to lay flat.');
  for (const bid of bids) {
    const b = state.body(bid), s = b.requireSolid();
    const normal = p.auto || !p.normal ? PR.bestRestNormal(s) : p.normal;
    state.bodies.set(bid, b.copy({ solid: s.transform(PR.layFlatMatrix(s, normal)) }));
    res.modified.push(bid);
  }
}

function f_thread(state, fid, p, res) {
  const kind = p.kind || 'internal', D = num(p.diameter, 8), P = num(p.pitch, 1.25), L = num(p.length, 10);
  let tool = PR.threadTool(kind, D, P, L, +(p.tolerance ?? 0.2) || 0, p.hand || 'right', p.chamfer !== false);
  const [o, x, y, n] = state.frame(p.plane), off = (p.offset || [0, 0, 0]).map(Number);
  tool = tool.transform(frameMatrix(add(add(add(o, mul(x, off[0])), mul(y, off[1])), mul(n, off[2])), x, y, n));
  const op = p.operation || (kind === 'internal' ? 'cut' : 'new');
  applyTool(state, fid, tool, op, p.targets, p.body_name || (kind === 'external' ? `M${+D} screw` : 'Thread'), res);
}

function f_offset(state, fid, p, res) {
  const bids = p.bodies || [];
  if (!bids.length) throw new GeometryError('Choose a body to offset.');
  let d = num(p.distance, 0.2);
  d = (p.direction || 'grow') === 'shrink' ? -Math.abs(d) : Math.abs(d);
  if (Math.abs(d) < 1e-6) throw new GeometryError("Distance can't be zero.");
  for (const bid of bids) { const b = state.body(bid); state.bodies.set(bid, b.copy({ solid: MT.offsetSolid(b.requireSolid(), d) })); res.modified.push(bid); }
}

function f_resizehole(state, fid, p, res) {
  const bid = p.body;
  if (!bid || !p.point) throw new GeometryError('Click the wall of a round hole or pin.');
  const b = state.body(bid), s = b.requireSolid();
  const { v, f } = b.mesh();
  const cyl = MT.fitCylinder(v, f, MT.nearestTriangle(v, f, p.point));
  res.info = { kind: cyl.kind, diameter: +(2 * cyl.radius).toFixed(3), length: +(cyl.z1 - cyl.z0).toFixed(3) };
  const fill = p.mode === 'fill';
  const out = MT.resizeRound(s, cyl, fill ? null : num(p.diameter, 2 * cyl.radius), fill);
  if (out.isEmpty()) throw new GeometryError('The result is empty.');
  state.bodies.set(bid, b.copy({ solid: out }));
  res.modified.push(bid);
}

export const EVALUATORS = {
  import: f_import, primitive: f_primitive, plane: f_plane, sketch: f_sketch, extrude: f_extrude, revolve: f_revolve,
  hole: f_hole, combine: f_combine, split: f_split, mirror: f_mirror, move: f_move, scale: f_scale, hollow: f_hollow, shell: f_shell,
  repair: f_repair, simplify: f_simplify, pattern: f_pattern, delete: f_delete, separate: f_separate, bedsplit: f_bedsplit,
  pushpull: f_pushpull, sculpt: f_sculpt, remesh: f_remesh, layflat: f_layflat, thread: f_thread, offset: f_offset,
  resizehole: f_resizehole, supportfins: f_supportfins, supportwall: f_supportwall,
};

export const DESKTOP_ONLY_TYPES = [];

export const LABELS = {
  import: 'Import', primitive: 'Primitive', plane: 'Plane', sketch: 'Sketch', extrude: 'Extrude', revolve: 'Revolve', hole: 'Hole',
  combine: 'Combine', split: 'Split', mirror: 'Mirror', move: 'Move', scale: 'Scale', hollow: 'Hollow', shell: 'Shell', repair: 'Repair',
  simplify: 'Simplify', pattern: 'Pattern', delete: 'Remove', separate: 'Separate', bedsplit: 'Fit to Bed', pushpull: 'Push/Pull',
  sculpt: 'Sculpt', remesh: 'Remesh', layflat: 'Lay Flat', thread: 'Thread', offset: 'Offset', resizehole: 'Resize Hole', supportfins: 'Support Fins', supportwall: 'Draw Walls',
};
