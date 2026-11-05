// STL Editor: an anchor fin (Support Fins' Anchor fins, GH #351) -- a rib stood by hand against
// a side or a back that leans up, at the spot clicked. web/sway.js swayAtFace, unchanged.
import { swayAtFace, faceTakesAnchor } from '../../../web/sway.js';
import { seated, packed, packedJson, soupFromB64 } from './stle_seat.js';

/**
 * @param point  [x, y, z] the spot on the part
 * @param face   the index of the triangle clicked (in `positions`' order)
 * @param sway   the Sway braces settings (optionsFromDialog's `sway`: gripFrom, tineSpacing, reach)
 */
export function anchorFin(positions, point, face, options = {}, sway = {}) {
  const s = seated(positions, options);
  if (!faceTakesAnchor(s.topo, s.rot, face)) {
    return { ok: false, reason: 'an anchor stands against a side or a back that leans up — click one of those' };
  }
  const r = swayAtFace(s.topo, s.result, s.rot, face, s.seat(point),
    { gap: s.tunables.propGap, tines: s.opts.tines, layerHeight: s.opts.layerHeight,
      ...Object.fromEntries(Object.entries(sway || {}).filter(([, v]) => v != null)), anchor: true, full: true },
    { braces: [], walls: [] });
  return packed(s, r, r?.ok ? { height: r.height ?? 0, tines: r.tines ?? 0, length: 0 } : null);
}

export function anchorFinB64(soupB64, pointJson, face, optionsJson, swayJson) {
  return packedJson(anchorFin(soupFromB64(soupB64), JSON.parse(pointJson), Number(face),
                              optionsJson ? JSON.parse(optionsJson) : {}, swayJson ? JSON.parse(swayJson) : {}));
}
