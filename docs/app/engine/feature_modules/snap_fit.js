import { Manifold } from '../wasm.js';
import { GeometryError, num, add, mul, frameMatrix } from '../geom.js';
import { applyTool, place } from '../features.js';
import { register } from './registry.js';


function f_snapfit(state, fid, p, res) {
  const L = num(p.length, 12), W = num(p.width, 6), t = num(p.thickness, 1.5), h = num(p.hook_height, 1.5);
  if (Math.min(L, W, t, h) <= 0) throw new GeometryError('Arm length, width, thickness and hook height must all be more than zero.');
  if (h >= L) throw new GeometryError('The hook must be shorter than the arm.');
  const sink = 0.5;   // the foot sinks into the plate so the join is solid
  const arm = Manifold.cube([t, W, L + sink], false).translate([0, 0, -sink]);
  const hook = Manifold.cube([t, W, h], false).translate([t, 0, L - h]);
  const tool = place(Manifold.union([arm, hook]), state.frame(p.plane), [num(p.x, 0), num(p.y, 0), 0]);
  applyTool(state, fid, tool, p.operation || 'join', p.targets, p.body_name || 'Snap-fit', res);
}

register('snapfit', f_snapfit, 'Snap-fit');
