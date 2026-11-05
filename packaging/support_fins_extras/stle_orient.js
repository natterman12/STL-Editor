// STL Editor: Support Fins' Suggest and Strength (web/orient.js, unchanged) -- ranked print
// orientations, which way the layers run for the current pose and for a load, and the
// strongest printable pose for that load.
import { suggestOrientations, layerVerdict, loadAlignment, suggestStrengthPose } from '../../../web/orient.js';
import { seated, soupFromB64 } from './stle_seat.js';

const plain = (c) => ({
  rot: Array.from(c.rot), overArea: c.overArea, bedArea: c.bedArea, height: c.height, size: c.size,
  regions: c.regions, walls: c.walls, coverage: c.coverage, volume: c.volume, seating: c.seating,
  holes: c.holes, bore: c.bore, score: c.score, posture: c.size ? layerVerdict(c.size).posture : null,
});

/** Up to `top` poses, best first: { confidence, candidates: [{ rot (column-major 3x3, applied
 *  to the part as it is), overArea, bedArea, height, walls, volume, posture, ... }] }. */
export function suggestPoses(positions, options = {}, top = 3) {
  const s = seated(positions, options);
  const r = suggestOrientations(s.topo, { top, threshold: s.opts.threshold });
  return { confidence: r.confidence, candidates: r.candidates.map(plain) };
}

/** The current pose: { posture: 'strong'|'mixed'|'weak', size, load: { quality, text } | null }. */
export function strengthOf(positions, options = {}, loadWorld = null) {
  const s = seated(positions, options);
  return { posture: layerVerdict(s.result.size).posture, size: s.result.size,
           load: loadWorld ? loadAlignment(loadWorld) : null };
}

/** The strongest printable pose for a load pointing `loadDir` (as the part sits now), or null. */
export function strongestPose(positions, options = {}, loadDir) {
  const s = seated(positions, options);
  const r = suggestStrengthPose(s.topo, loadDir, { threshold: s.opts.threshold });
  return r ? { ...r, rot: Array.from(r.rot) } : null;
}

export const suggestPosesJson = (soupB64, optionsJson, top) =>
  JSON.stringify(suggestPoses(soupFromB64(soupB64), optionsJson ? JSON.parse(optionsJson) : {}, Number(top) || 3));
export const strengthOfJson = (soupB64, optionsJson, loadJson) =>
  JSON.stringify(strengthOf(soupFromB64(soupB64), optionsJson ? JSON.parse(optionsJson) : {}, loadJson ? JSON.parse(loadJson) : null));
export const strongestPoseJson = (soupB64, optionsJson, loadJson) =>
  JSON.stringify(strongestPose(soupFromB64(soupB64), optionsJson ? JSON.parse(optionsJson) : {}, JSON.parse(loadJson)));
