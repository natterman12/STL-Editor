// STL Editor: a part seated exactly as Support Fins' draw_entry.js seats it, for the extras
// (same seat and clearances as computeFins, so they line up with the fins it builds).
import { buildTopology, analyze, IDENTITY3 } from '../../../web/overhangs.js';
import { applyTunables } from '../../../web/fins.js';
import { ENGINE_DEFAULTS } from './fins_entry.js';
import { seatSoup, clearances, flatten, snap } from './seat.js';
import { b64ToBytes, bytesToB64 } from './bridge.js';

export function seated(positions, options = {}) {
  const opts = { ...ENGINE_DEFAULTS, ...options };
  const { tunables } = clearances(opts);
  const { pos, shift } = seatSoup(positions);
  const topo = buildTopology({ getAttribute: (k) => (k === 'position' ? { array: pos } : null) });
  const result = analyze(topo, opts.threshold, IDENTITY3);
  const off = result.offset;
  const tris = new Float64Array(pos.length);
  for (let i = 0; i < pos.length; i += 3) {
    tris[i] = pos[i] + off.x; tris[i + 1] = pos[i + 1] + off.y; tris[i + 2] = pos[i + 2] + off.z;
  }
  const seat = (p) => [snap(p[0] - shift.x) + off.x, snap(p[1] - shift.y) + off.y, snap(p[2] - shift.z) + off.z];
  applyTunables(tunables);
  return { opts, tunables, topo, result, tris, seat, rot: IDENTITY3, offset: off,
           // the frame the triangles come back in: input = seated - out, as computeFins'
           out: { x: off.x - shift.x, y: off.y - shift.y, z: off.z - shift.z } };
}

export function soupFromB64(s) {
  const bytes = b64ToBytes(s);
  return bytes.byteOffset % 8 ? new Float64Array(bytes.slice().buffer)
    : new Float64Array(bytes.buffer, bytes.byteOffset, bytes.byteLength / 8);
}
export const trianglesB64 = (t) => bytesToB64(new Uint8Array(t.buffer, t.byteOffset, t.byteLength));
export { flatten };

/** A hand-placed support's result as the hosts take it (ok + Float32 triangles + offset). */
export function packed(s, r, stats) {
  if (!r || !r.ok) return { ok: false, reason: r?.reason || 'Support Fins could not place it there' };
  return { ok: true, triangles: flatten(r.tris), offset: s.out, stats };
}
export const packedJson = (r) => JSON.stringify(r.ok ? { ...r, triangles: trianglesB64(r.triangles) } : r);
