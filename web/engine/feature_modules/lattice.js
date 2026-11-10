import { hollowPitch } from '../features.js';
import { num, GeometryError } from '../geom.js';
import { Manifold } from '../wasm.js';
import * as VX from '../voxel.js';

// Hollow the body (solid wall kept outside) and fill the hollow with a cubic grid of round
// struts. Keep in step with f_lattice in stleditor/features.py.
function f_lattice(state, fid, p, res) {
  const bid = p.body;
  if (!bid) throw new GeometryError('Choose a body to fill with a lattice.');
  const b = state.body(bid), s = b.requireSolid();
  const cell = num(p.cell, 6), d = num(p.strut, 1), t = num(p.thickness, 2);
  if (!(cell > 0) || !(d > 0) || !(t > 0)) throw new GeometryError('Cell size, strut diameter and wall thickness must all be above zero.');
  if (d >= cell) throw new GeometryError("The struts are as wide as the cells, so there'd be no gaps. Use a thinner strut or a bigger cell.");
  const cavity = VX.hollow(s, t, hollowPitch(s, t, p.resolution), true);
  const bb = cavity.boundingBox(), lo = bb.min, size = [0, 1, 2].map(a => bb.max[a] - bb.min[a]);
  const n = size.map(x => Math.max(1, Math.round(x / cell)));
  if (n[1] * n[2] + n[0] * n[2] + n[0] * n[1] > 2500) throw new GeometryError("That's too many struts. Use a bigger cell size.");
  const pos = [0, 1, 2].map(a => Array.from({ length: n[a] }, (_, k) => lo[a] + (k + 0.5) * size[a] / n[a]));
  const r = d / 2, struts = [];
  for (let a = 0; a < 3; a++) {
    const [o1, o2] = [0, 1, 2].filter(k => k !== a);
    let cyl = Manifold.cylinder(size[a] + 2, r, r, 16);
    if (a === 0) cyl = cyl.rotate([0, 90, 0]);
    else if (a === 1) cyl = cyl.rotate([-90, 0, 0]);
    for (const u of pos[o1]) for (const w of pos[o2]) {
      const off = [0, 0, 0];
      off[a] = lo[a] - 1; off[o1] = u; off[o2] = w;
      struts.push(cyl.translate(off));
    }
  }
  const grid = Manifold.union(struts);
  state.bodies.set(bid, b.copy({ solid: s.subtract(cavity.subtract(grid)) }));
  res.modified.push(bid);
}

import { register } from './registry.js';
register('lattice', f_lattice, 'Lattice');
