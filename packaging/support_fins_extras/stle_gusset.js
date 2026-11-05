// STL Editor: a gusset (Support Fins' Draw > Gusset) -- a breakaway fin standing off a SIDE of
// the part out to an overhang, instead of a wall up from the plate. web/gusset.js, unchanged.
import { gussetAt } from '../../../web/gusset.js';
import { seated, packed, packedJson, soupFromB64 } from './stle_seat.js';

/**
 * @param positions  the part (posed soup, as computeFins)
 * @param a          [x, y, z] a point on the side the gusset stands off
 * @param b          [x, y, z] the overhang it reaches out to
 * @param normal     [x, y, z] the side's outward normal (the clicked face's)
 */
export function gussetWall(positions, a, b, normal, options = {}) {
  const s = seated(positions, options);
  const r = gussetAt(s.seat(a), s.seat(b), normal, s.tris, {
    topo: s.topo, rot: s.rot, offset: s.offset, tines: s.opts.tines, tineDensity: s.opts.tineDensity,
    layerHeight: s.opts.layerHeight, plateOnly: s.opts.plateOnly,
  });
  return packed(s, r, r?.ok ? { reach: r.reach, height: r.height, tines: (r.tines ?? 0) + (r.sideTines ?? 0), length: r.reach } : null);
}

export function gussetWallB64(soupB64, aJson, bJson, nJson, optionsJson) {
  return packedJson(gussetWall(soupFromB64(soupB64), JSON.parse(aJson), JSON.parse(bJson), JSON.parse(nJson),
                               optionsJson ? JSON.parse(optionsJson) : {}));
}
