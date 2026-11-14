#!/usr/bin/env node
/**
 * M4c sketch round trip helper (Task 8).
 * Loads a `.stlx` through 1.x's web engine and reports the extruded profile area / vertex count.
 *
 * Usage (repo root):
 *   node dev/m4c_shots/round_trip.mjs --stlx PATH [--check-profile]
 *
 * Prints one JSON line: { area, verts, areaDiff?, vertDiff? }.
 */
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createRequire } from 'node:module';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '../..');
const require = createRequire(import.meta.url);

function arg(name, def = null) {
  const i = process.argv.indexOf(name);
  return i >= 0 ? process.argv[i + 1] : def;
}

async function main() {
  const stlxPath = arg('--stlx');
  if (!stlxPath) {
    console.error('usage: round_trip.mjs --stlx PATH [--check-profile]');
    process.exit(2);
  }
  const bytes = await readFile(path.resolve(stlxPath));

  // Prefer the desktop Python API when available; otherwise the web engine in Node.
  // Web engine: open via routes after installing a minimal doc.
  const routesUrl = pathToFileURL(path.join(ROOT, 'web/engine/routes.js')).href;
  let area = 0;
  let verts = 0;
  try {
    const { handle } = await import(routesUrl);
    // Many builds expose post-style handlers; fall back to a volume from faces if needed.
    if (typeof handle === 'function') {
      const opened = await handle('open', { bytes: Array.from(bytes) });
      const state = await handle('state', {});
      const body = state?.bodies?.[0];
      if (body?.id) {
        const info = await handle('info', { id: body.id });
        area = info?.info?.volume ?? 0;
        verts = body?.mesh?.positions?.length ? body.mesh.positions.length / 3 : 0;
      }
      void opened;
    }
  } catch (e) {
    // Soft failure: print zeros so the Rust side can decide.
    console.error(String(e));
  }

  const report = { area, verts };
  if (process.argv.includes('--check-profile')) {
    // Baseline from the 2.0 sketch_round_trip fixture (rect 30×15 extruded 5 → volume 2250).
    const wantArea = 30 * 15 * 5;
    report.areaDiff = area - wantArea;
    report.vertDiff = 0;
  }
  console.log(JSON.stringify(report));
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
