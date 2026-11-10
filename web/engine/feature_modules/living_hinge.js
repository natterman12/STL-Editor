import { Manifold } from '../wasm.js';
import { GeometryError, num, dot, add, mul, frameMatrix } from '../geom.js';
import { applyTool, place } from '../features.js';


function f_hinge(state, fid, p, res) {
  const slot = num(p.slot, 1), web = num(p.thickness, 0.5);
  if (slot <= 0 || web <= 0) throw new GeometryError('The slot width and the hinge thickness must be more than zero.');
  const targets = p.targets;
  const bodies = targets && targets.length ? targets.filter(t => state.bodies.has(t)).map(t => state.bodies.get(t)) : [...state.bodies.values()].filter(b => b.isSolid);
  if (!bodies.length) throw new GeometryError('Choose the block to put the living hinge in.');
  const fr = state.frame(p.plane), [o, xa, ya, n] = fr;
  let lo = Infinity, hi = -Infinity, reach = 0;
  for (const b of bodies) {
    const { v } = b.mesh();
    for (let i = 0; i < v.length; i += 3) {
      const rel = [v[i] - o[0], v[i + 1] - o[1], v[i + 2] - o[2]];
      const d = dot(rel, n);
      lo = Math.min(lo, d); hi = Math.max(hi, d);
      reach = Math.max(reach, Math.abs(dot(rel, xa)), Math.abs(dot(rel, ya)));
    }
  }
  if (web >= hi - lo) throw new GeometryError('The hinge is as thick as the block, so there is nothing to cut.');
  const B = reach + 10;
  const strip = Manifold.cube([slot, 2 * B, hi - lo - web + 1], false).translate([num(p.x, 0), -B, lo + web]);
  applyTool(state, fid, place(strip, fr), 'cut', targets, 'Living hinge', res);
}

import { register } from './registry.js';
register('hinge', f_hinge, 'Living hinge');
