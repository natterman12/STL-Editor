// Loads the Manifold geometry kernel (WebAssembly) and keeps track of every Manifold and
// CrossSection it creates, so objects no document state uses any more can be freed.
import Module from '../vendor/manifold/manifold.js';

export let W = null;            // the wasm module
export let Manifold = null;
export let CrossSection = null;
export let Mesh = null;

const live = new Set();

function track(x) {
  if (x && typeof x === 'object') {
    if (x instanceof Manifold || x instanceof CrossSection) live.add(x);
    else if (Array.isArray(x)) for (const y of x) track(y);
  }
  return x;
}

function wrapAll(target, skip) {
  for (const name of Object.getOwnPropertyNames(target)) {
    if (name.startsWith('_') || skip.has(name)) continue;
    const d = Object.getOwnPropertyDescriptor(target, name);
    if (!d || typeof d.value !== 'function') continue;
    const fn = d.value;
    target[name] = function (...args) { return track(fn.apply(this, args)); };
  }
}

export async function init(opts = {}) {
  if (W) return W;
  W = await Module(opts);
  W.setup();
  Manifold = W.Manifold;
  CrossSection = W.CrossSection;
  Mesh = W.Mesh;
  const skip = new Set(['constructor', 'delete', 'isDeleted', 'getMesh', 'volume', 'surfaceArea', 'area', 'isEmpty',
    'numVert', 'numTri', 'numEdge', 'numProp', 'numPropVert', 'boundingBox', 'bounds', 'toPolygons', 'genus', 'status',
    'numContour', 'originalID', 'tolerance', 'minGap', 'length', 'name', 'prototype']);
  wrapAll(Manifold.prototype, skip);
  wrapAll(Manifold, skip);
  wrapAll(CrossSection.prototype, skip);
  wrapAll(CrossSection, skip);
  W.setMinCircularAngle?.(5);
  return W;
}

// new Manifold / CrossSection go through these so they are tracked too
export function manifoldOf(mesh) { return track(new Manifold(mesh)); }
export function crossSection(polys, rule = 'EvenOdd') { return track(new CrossSection(polys, rule)); }

/** Free every tracked object not reachable from `keep` (a Set of objects still in use). */
export function sweep(keep) {
  let n = 0;
  for (const x of live) {
    if (keep.has(x)) continue;
    live.delete(x);
    try { if (!x.isDeleted()) { x.delete(); n++; } } catch { /* already gone */ }
  }
  return n;
}

export function liveCount() { return live.size; }
