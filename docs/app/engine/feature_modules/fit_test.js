import { Manifold } from '../wasm.js';
import { GeometryError, num } from '../geom.js';
import { applyTool } from '../features.js';
import { register } from './registry.js';

function f_fittest(state, fid, p, res) {
  const D = num(p.diameter, 8), start = num(p.start, 0.05), step = num(p.step, 0.05);
  const count = Math.floor(+(p.count ?? 6)), T = num(p.thickness, 3), gap = num(p.gap, 4);
  if (!(D > 0)) throw new GeometryError('Peg diameter must be more than zero.');
  if (start < 0) throw new GeometryError("Clearance start can't be negative.");
  if (!(step > 0)) throw new GeometryError('Clearance step must be more than zero.');
  if (!(count >= 1)) throw new GeometryError('Count must be at least 1.');
  if (!(T > 0)) throw new GeometryError('Plate thickness must be more than zero.');
  if (!(gap > 0)) throw new GeometryError('Gap must be more than zero.');
  const clearances = Array.from({ length: count }, (_, i) => start + i * step);
  const maxHole = D + clearances[clearances.length - 1];
  const L = count * maxHole + (count + 1) * gap, W = maxHole + D + 3 * gap, seg = 64;
  let man = Manifold.cube([L, W, T], false).translate([-L / 2, -W / 2, 0]);
  for (let i = 0; i < count; i++) {
    const c = clearances[i], x = -L / 2 + gap + maxHole / 2 + i * (maxHole + gap);
    const yHole = -W / 2 + gap + maxHole / 2, yPeg = -W / 2 + gap + maxHole + gap + D / 2, hr = (D + c) / 2;
    man = man.subtract(Manifold.cylinder(T + 2, hr, hr, seg).translate([x, yHole, -1]));
    man = man.add(Manifold.cylinder(T, D / 2, D / 2, seg).translate([x, yPeg, T]));
  }
  applyTool(state, fid, man, 'new', null, p.body_name || 'Fit Test', res);
  res.info = `${count} pair${count !== 1 ? 's' : ''}, peg Ø${D} mm, clearances ${clearances.map(c => +c).join(', ')} mm.`;
}

register('fittest', f_fittest, 'Fit Test');
