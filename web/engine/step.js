// Faceted STEP writer (ISO 10303-21, AP214 "automotive design" schema).
// Every mesh triangle becomes one planar ADVANCED_FACE. Faces share their edges and vertices,
// so each body is a MANIFOLD_SOLID_BREP with a CLOSED_SHELL. Curves are written as flat facets
// (the same triangles the mesh has), so CAD tools open the file as a solid made of flat faces.
import { GeometryError } from './geom.js';

// a STEP REAL literal: always has a decimal point ("1.", "1.E-7")
function real(x) {
  if (!Number.isFinite(x) || x === 0) return '0.';
  let s = String(x);
  if (s.includes('e')) {
    let [m, e] = s.split('e');
    if (!m.includes('.')) m += '.';
    return m + 'E' + (e.startsWith('-') ? '-' : '') + e.replace(/^[+-]/, '');
  }
  return s.includes('.') ? s : s + '.';
}

// a STEP string: ASCII only, apostrophes doubled
const str = s => "'" + String(s).replace(/[^\x20-\x7e]/g, '?').replace(/'/g, "''") + "'";

const vec = (p, q) => [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
const cross = (p, q) => [p[1] * q[2] - p[2] * q[1], p[2] * q[0] - p[0] * q[2], p[0] * q[1] - p[1] * q[0]];
const unit = p => { const l = Math.hypot(p[0], p[1], p[2]); return [p[0] / l, p[1] / l, p[2] / l]; };

/**
 * meshes: [{ v: Float array (x,y,z per vertex), f: index array (3 per triangle), name }]
 * Returns the file as UTF-8 bytes. Throws GeometryError when a body isn't a closed solid
 * (a hole, a doubled face or a face with no area), since CAD would not read it as a solid.
 */
export function stepBytes(meshes, opts = {}) {
  const title = opts.title || 'model';
  const date = (opts.date || new Date().toISOString()).slice(0, 19);
  const L = [];
  let n = 0;
  const add = body => { L.push(`#${++n}=${body};`); return `#${n}`; };

  // product and application context
  const actx = add(`APPLICATION_CONTEXT('core data for automotive mechanical design processes')`);
  add(`APPLICATION_PROTOCOL_DEFINITION('international standard','automotive_design',2001,${actx})`);
  const pctx = add(`PRODUCT_CONTEXT('',${actx},'mechanical')`);
  const prod = add(`PRODUCT(${str(title)},${str(title)},'',(${pctx}))`);
  const pdf = add(`PRODUCT_DEFINITION_FORMATION('','',${prod})`);
  const pdc = add(`PRODUCT_DEFINITION_CONTEXT('part definition',${actx},'design')`);
  const pd = add(`PRODUCT_DEFINITION('design','',${pdf},${pdc})`);
  const pds = add(`PRODUCT_DEFINITION_SHAPE('','',${pd})`);

  // geometric context: millimetres, plane angles in radians
  const origin = add(`CARTESIAN_POINT('',(0.,0.,0.))`);
  const zdir = add(`DIRECTION('',(0.,0.,1.))`);
  const xdir = add(`DIRECTION('',(1.,0.,0.))`);
  const place0 = add(`AXIS2_PLACEMENT_3D('',${origin},${zdir},${xdir})`);
  const lu = add(`( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.) )`);
  const au = add(`( NAMED_UNIT(*) PLANE_ANGLE_UNIT() SI_UNIT($,.RADIAN.) )`);
  const su = add(`( NAMED_UNIT(*) SI_UNIT($,.STERADIAN.) SOLID_ANGLE_UNIT() )`);
  const unc = add(`UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-06),${lu},'distance_accuracy_value','')`);
  const ctx = add(`( GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((${unc})) GLOBAL_UNIT_ASSIGNED_CONTEXT((${lu},${au},${su})) REPRESENTATION_CONTEXT('Context3D','3D') )`);

  const breps = [];
  for (const m of meshes) {
    const v = m.v, f = m.f, nv = v.length / 3, nt = f.length / 3;
    const label = m.name || title;
    const P = i => [v[3 * i], v[3 * i + 1], v[3 * i + 2]];

    // check first: every edge used by exactly two faces, once in each direction; no zero-area faces
    const uses = new Map();
    for (let t = 0; t < nt; t++) {
      const tri = [f[3 * t], f[3 * t + 1], f[3 * t + 2]];
      const c = cross(vec(P(tri[0]), P(tri[1])), vec(P(tri[0]), P(tri[2])));
      if (!(Math.hypot(c[0], c[1], c[2]) > 0) || new Set(tri).size < 3) {
        throw new GeometryError(`${label} has faces with no area. Run Repair on it first, then export.`);
      }
      for (let k = 0; k < 3; k++) {
        const a = tri[k], b = tri[(k + 1) % 3], lo = Math.min(a, b), hi = Math.max(a, b);
        const key = `${lo},${hi}`;
        const u = uses.get(key) || { count: 0, forward: 0 };
        u.count++;
        if (a < b) u.forward++;
        uses.set(key, u);
      }
    }
    for (const u of uses.values()) {
      if (u.count !== 2 || u.forward !== 1) {
        throw new GeometryError(`${label} isn't a closed solid (it has holes, or faces that overlap or face the wrong way). Run Repair on it first, then export.`);
      }
    }

    // vertices and edges are written once and shared by the faces next to them
    const cps = new Map(), vps = new Map(), ecs = new Map();
    const cp = i => {
      if (!cps.has(i)) {
        const p = P(i);
        cps.set(i, add(`CARTESIAN_POINT('',(${real(p[0])},${real(p[1])},${real(p[2])}))`));
      }
      return cps.get(i);
    };
    const vp = i => {
      if (!vps.has(i)) vps.set(i, add(`VERTEX_POINT('',${cp(i)})`));
      return vps.get(i);
    };
    // the edge's own direction runs from the lower index to the higher one
    const edge = (a, b) => {
      const lo = Math.min(a, b), hi = Math.max(a, b), key = `${lo},${hi}`;
      if (!ecs.has(key)) {
        const d = vec(P(lo), P(hi)), len = Math.hypot(d[0], d[1], d[2]), u = d.map(x => x / len);
        const dir = add(`DIRECTION('',(${u.map(real).join(',')}))`);
        const vc = add(`VECTOR('',${dir},${real(len)})`);
        const line = add(`LINE('',${cp(lo)},${vc})`);
        ecs.set(key, add(`EDGE_CURVE('',${vp(lo)},${vp(hi)},${line},.T.)`));
      }
      return ecs.get(key);
    };

    const faces = [];
    for (let t = 0; t < nt; t++) {
      const a = f[3 * t], b = f[3 * t + 1], c = f[3 * t + 2];
      const ab = vec(P(a), P(b)), ac = vec(P(a), P(c));
      const nrm = unit(cross(ab, ac));
      const xd = unit(ab);   // in the plane and across the normal: a valid reference direction
      const dn = add(`DIRECTION('',(${nrm.map(real).join(',')}))`);
      const dx = add(`DIRECTION('',(${xd.map(real).join(',')}))`);
      const ax = add(`AXIS2_PLACEMENT_3D('',${cp(a)},${dn},${dx})`);
      const pl = add(`PLANE('',${ax})`);
      const oes = [[a, b], [b, c], [c, a]].map(([p, q]) => add(`ORIENTED_EDGE('',*,*,${edge(p, q)},${p < q ? '.T.' : '.F.'})`));
      const loop = add(`EDGE_LOOP('',(${oes.join(',')}))`);
      const bound = add(`FACE_OUTER_BOUND('',${loop},.T.)`);
      faces.push(add(`ADVANCED_FACE('',(${bound}),${pl},.T.)`));
    }
    const shell = add(`CLOSED_SHELL('',(${faces.join(',')}))`);
    breps.push(add(`MANIFOLD_SOLID_BREP(${str(label)},${shell})`));
  }

  const rep = add(`ADVANCED_BREP_SHAPE_REPRESENTATION(${str(title)},(${[place0, ...breps].join(',')}),${ctx})`);
  add(`SHAPE_DEFINITION_REPRESENTATION(${pds},${rep})`);

  const text = [
    'ISO-10303-21;',
    'HEADER;',
    `FILE_DESCRIPTION(('faceted STEP: each triangle is a flat face'),'2;1');`,
    `FILE_NAME(${str(title + '.step')},${str(date)},(''),(''),'STL Editor','STL Editor','');`,
    `FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));`,
    'ENDSEC;',
    'DATA;',
    ...L,
    'ENDSEC;',
    'END-ISO-10303-21;',
    '',
  ].join('\n');
  return new TextEncoder().encode(text);
}
