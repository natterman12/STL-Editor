// Browser timings for the web engine on test_mesh.stl (real Chrome via puppeteer-core).
// Run:  $env:CHROME_PATH='...chrome.exe'; node dev/bench/browser_bench.mjs
// Prints JSON medians (seconds) for import, cut_cube, cut_sphere, hollow, simplify, remesh, sculpt.
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createRequire } from 'node:module';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..', '..');
const webRoot = path.join(root, 'web');
const meshPath = path.join(here, 'test_mesh.stl');
const chromePath = process.env.CHROME_PATH;
const RUNS = 3;

if (!chromePath || !fs.existsSync(chromePath)) {
  console.error(JSON.stringify({ error: 'CHROME_PATH missing or not found', CHROME_PATH: chromePath || null }));
  process.exit(1);
}
if (!fs.existsSync(meshPath)) {
  console.error(JSON.stringify({ error: 'test_mesh.stl missing', path: meshPath }));
  process.exit(1);
}

const require = createRequire(path.join(here, 'package.json'));
let puppeteer;
try {
  puppeteer = require('puppeteer-core');
} catch (e) {
  console.error(JSON.stringify({ error: 'puppeteer-core missing under dev/bench', detail: String(e.message || e) }));
  process.exit(1);
}

const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'application/javascript; charset=utf-8',
  '.mjs': 'application/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json',
  '.webmanifest': 'application/manifest+json',
  '.wasm': 'application/wasm',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.webp': 'image/webp',
  '.svg': 'image/svg+xml',
  '.ico': 'image/x-icon',
  '.ttf': 'font/ttf',
  '.otf': 'font/otf',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2',
  '.txt': 'text/plain; charset=utf-8',
  '.md': 'text/plain; charset=utf-8',
  '.stl': 'application/octet-stream',
};

function contentType(file) {
  return TYPES[path.extname(file).toLowerCase()] || 'application/octet-stream';
}

function startServer() {
  const server = http.createServer((req, res) => {
    let rel;
    try { rel = decodeURIComponent(new URL(req.url, 'http://x').pathname); }
    catch { res.writeHead(400); res.end(); return; }
    let file;
    if (rel === '/bench/test_mesh.stl') file = meshPath;
    else {
      file = path.join(webRoot, rel);
      if (!file.startsWith(webRoot)) { res.writeHead(403); res.end(); return; }
    }
    fs.stat(file, (err, st) => {
      if (!err && st.isDirectory()) {
        if (!rel.endsWith('/')) { res.writeHead(301, { Location: rel + '/' }); res.end(); return; }
        file = path.join(file, 'index.html');
      }
      fs.readFile(file, (e, data) => {
        if (e) { res.writeHead(404, { 'Content-Type': 'text/plain' }); res.end('not found'); return; }
        res.writeHead(200, { 'Content-Type': contentType(file), 'Cache-Control': 'no-cache' });
        res.end(data);
      });
    });
  });
  return new Promise((ok, fail) => {
    server.once('error', fail);
    server.listen(0, '127.0.0.1', () => ok(server));
  });
}

function median(xs) {
  const a = [...xs].sort((x, y) => x - y);
  const m = Math.floor(a.length / 2);
  return a.length % 2 ? a[m] : (a[m - 1] + a[m]) / 2;
}

async function main() {
  const server = await startServer();
  const { port } = server.address();
  const url = `http://127.0.0.1:${port}/index.html?web`;

  const browser = await puppeteer.launch({
    executablePath: chromePath,
    headless: 'new',
    args: [
      '--no-sandbox',
      '--disable-gpu',
      '--use-angle=swiftshader',
      '--enable-unsafe-swiftshader',
      '--ignore-gpu-blocklist',
    ],
  });

  const page = await browser.newPage();
  page.setDefaultTimeout(600_000);
  await page.evaluateOnNewDocument(() => {
    try { localStorage.setItem('stle.webWelcome', '1'); } catch { /* ignore */ }
  });
  page.on('pageerror', e => console.error('[pageerror]', e.message));

  await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 120_000 });
  await page.waitForFunction(() => !!(window.app && window.app.state !== undefined), { timeout: 120_000 });
  // First engine call warms the WASM worker.
  await page.evaluate(async () => {
    const { post } = await import('./js/api.js');
    await post('new', { force: true });
  });

  const timings = await page.evaluate(async (runs) => {
    const { post, upload } = await import('./js/api.js');
    const out = {
      import: [], cut_cube: [], cut_sphere: [], hollow: [], simplify: [], remesh: [], sculpt: [],
    };
    const notes = [];

    async function dismissUi() {
      const modal = document.getElementById('modal');
      if (modal && !modal.classList.contains('hidden')) {
        const btn = document.querySelector('#modal-actions .primary, #modal-actions button');
        if (btn) btn.click();
      }
    }

    async function freshDoc() {
      await dismissUi();
      await post('new', { force: true });
      if (window.app?.apply) {
        const st = await post('state');
        await window.app.apply({ ok: true, state: st });
      }
    }

    async function importMesh() {
      const buf = await fetch('/bench/test_mesh.stl').then(r => {
        if (!r.ok) throw new Error(`fetch test_mesh.stl failed: ${r.status}`);
        return r.arrayBuffer();
      });
      const file = new File([buf], 'test_mesh.stl');
      const t0 = performance.now();
      const res = await upload(file, { place: 'center' });
      const t1 = performance.now();
      if (window.app?.apply) await window.app.apply(res);
      const st = res.state || await post('state');
      const body = st.bodies?.[0];
      if (!body) throw new Error('import produced no body');
      return { ms: t1 - t0, bodyId: body.id, bbox: body.bbox, triangles: body.triangles };
    }

    async function addFeature(type, params) {
      const t0 = performance.now();
      const res = await post('feature/add', { type, params }, { quiet: true });
      const t1 = performance.now();
      if (res.error) throw new Error(String(res.error));
      if (res.feature?.error) throw new Error(`${type}: ${res.feature.error}`);
      if (window.app?.apply) await window.app.apply(res);
      return t1 - t0;
    }

    // Import (timed)
    for (let i = 0; i < runs; i++) {
      await freshDoc();
      const r = await importMesh();
      out.import.push(r.ms);
      if (i === 0) notes.push(`mesh_tris=${r.triangles}`);
    }

    // Cut cube: setup cube, time combine cut
    for (let i = 0; i < runs; i++) {
      await freshDoc();
      const { bodyId } = await importMesh();
      const box = await post('feature/add', {
        type: 'primitive',
        params: {
          shape: 'box', length: 20, width: 20, height: 20, anchor: 'center',
          plane: 'XY', offset: [0, 0, 20], operation: 'new',
        },
      }, { quiet: true });
      if (window.app?.apply) await window.app.apply(box);
      const toolId = box.feature?.id;
      if (!toolId) throw new Error('cube primitive missing id');
      out.cut_cube.push(await addFeature('combine', {
        target: bodyId, tools: [toolId], operation: 'cut', keep_tools: false, clearance: 0,
      }));
    }

    // Cut sphere: same-size (r=20 matches test_mesh builder), time combine cut
    for (let i = 0; i < runs; i++) {
      await freshDoc();
      const { bodyId } = await importMesh();
      const sph = await post('feature/add', {
        type: 'primitive',
        params: {
          shape: 'sphere', radius: 20, segments: 96, anchor: 'center',
          plane: 'XY', offset: [0, 0, 20], operation: 'new',
        },
      }, { quiet: true });
      if (window.app?.apply) await window.app.apply(sph);
      const toolId = sph.feature?.id;
      if (!toolId) throw new Error('sphere primitive missing id');
      out.cut_sphere.push(await addFeature('combine', {
        target: bodyId, tools: [toolId], operation: 'cut', keep_tools: false, clearance: 0,
      }));
    }

    // Hollow
    for (let i = 0; i < runs; i++) {
      await freshDoc();
      const { bodyId } = await importMesh();
      out.hollow.push(await addFeature('hollow', { body: bodyId, thickness: 2, resolution: 'normal' }));
    }

    // Simplify ratio 0.1
    for (let i = 0; i < runs; i++) {
      await freshDoc();
      const { bodyId } = await importMesh();
      out.simplify.push(await addFeature('simplify', { body: bodyId, ratio: 0.1 }));
    }

    // Remesh edge 1.0
    for (let i = 0; i < runs; i++) {
      await freshDoc();
      const { bodyId } = await importMesh();
      out.remesh.push(await addFeature('remesh', { body: bodyId, edge: 1.0 }));
    }

    // One sculpt stroke: radius 6, strength 0.8, 15 points
    for (let i = 0; i < runs; i++) {
      await freshDoc();
      const { bodyId, bbox } = await importMesh();
      const [[x0, y0], [x1, y1, z1]] = bbox;
      const points = [];
      for (let k = 0; k < 15; k++) {
        const t = k / 14;
        points.push([
          x0 + (x1 - x0) * (0.35 + 0.3 * t),
          (y0 + y1) / 2,
          z1,
        ]);
      }
      out.sculpt.push(await addFeature('sculpt', {
        body: bodyId,
        strokes: [{ mode: 'smooth', radius: 6, strength: 0.8, points }],
      }));
    }

    return { samples_ms: out, notes };
  }, RUNS);

  await browser.close();
  server.close();

  const medians_s = {};
  for (const [op, samples] of Object.entries(timings.samples_ms)) {
    medians_s[op] = +(median(samples) / 1000).toFixed(3);
  }

  const report = {
    mesh: 'dev/bench/test_mesh.stl',
    runs: RUNS,
    chrome: chromePath,
    medians_s,
    samples_s: Object.fromEntries(
      Object.entries(timings.samples_ms).map(([k, v]) => [k, v.map(ms => +(ms / 1000).toFixed(3))]),
    ),
    notes: timings.notes,
  };
  console.log(JSON.stringify(report, null, 2));
}

main().catch(err => {
  console.error(JSON.stringify({ error: String(err && err.stack || err) }));
  process.exit(1);
});
