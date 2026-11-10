// Builds the deterministic ~500k-triangle benchmark mesh with the web engine's kernel.
// Run: node dev/bench/make_test_mesh.mjs  -> dev/bench/test_mesh.stl
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const web = path.join(here, '..', '..', 'web');
const load = rel => import(pathToFileURL(path.join(web, rel)).href);
const W = await load('engine/wasm.js');
await W.init();
const IO = await load('engine/io.js');
const { Manifold } = W;

const ball = Manifold.sphere(20, 1130);
const hole = Manifold.cube([14, 60, 14], true).translate([0, 0, 0]);
const man = ball.subtract(hole).subtract(Manifold.cube([60, 60, 6], true).translate([0, 0, 12]));
const m = man.getMesh();
const bytes = IO.stlBytes([{ v: m.vertProperties, f: m.triVerts }]);
const out = path.join(here, 'test_mesh.stl');
fs.writeFileSync(out, bytes);
console.log(`triangles ${m.triVerts.length / 3}, vertices ${m.vertProperties.length / m.numProp}, bytes ${bytes.byteLength} -> ${out}`);
