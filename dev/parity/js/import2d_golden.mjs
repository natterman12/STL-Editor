/**
 * Generate rust/tests/parity/import2d_golden.json from 1.x import2d.js.
 *
 * Drives headless Chrome (puppeteer-core + CHROME_PATH) on a tiny page that
 * imports web/js/import2d.js, then runs parseSVG / parseDXF on every file under
 * rust/tests/parity/import2d/.
 *
 * Usage:
 *   node dev/parity/js/import2d_golden.mjs
 *   node dev/parity/js/import2d_golden.mjs --check
 */
import { createRequire } from 'module';
import { execSync } from 'child_process';
import fs from 'fs';
import http from 'http';
import path from 'path';
import { fileURLToPath } from 'url';

const require = createRequire(import.meta.url);
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '..', '..', '..');
const CASES = path.join(ROOT, 'rust', 'tests', 'parity', 'import2d');
const GOLDEN = path.join(ROOT, 'rust', 'tests', 'parity', 'import2d_golden.json');
const WEB = path.join(ROOT, 'web');

function loadPuppeteer() {
  try {
    return require('puppeteer-core');
  } catch {
    const root = execSync('npm root -g', { encoding: 'utf8' }).trim();
    return require(path.join(root, 'puppeteer-core'));
  }
}

function round(v, nd = 9) {
  if (typeof v === 'number') {
    if (!Number.isFinite(v)) return v;
    const p = 10 ** nd;
    return Math.round(v * p) / p;
  }
  if (Array.isArray(v)) return v.map(x => round(x, nd));
  if (v && typeof v === 'object') {
    const out = {};
    for (const k of Object.keys(v).sort()) out[k] = round(v[k], nd);
    return out;
  }
  return v;
}

function dumps(obj) {
  return JSON.stringify(obj, null, 1) + '\n';
}

function normNewlines(s) {
  return s.replace(/\r\n/g, '\n').replace(/\r/g, '\n');
}

function contentType(p) {
  if (p.endsWith('.js')) return 'text/javascript; charset=utf-8';
  if (p.endsWith('.css')) return 'text/css; charset=utf-8';
  if (p.endsWith('.html')) return 'text/html; charset=utf-8';
  if (p.endsWith('.svg')) return 'image/svg+xml; charset=utf-8';
  return 'application/octet-stream';
}

function startServer() {
  const server = http.createServer((req, res) => {
    const url = decodeURIComponent((req.url || '/').split('?')[0]);
    if (url === '/' || url === '/index.html') {
      const html = `<!doctype html><html><body>
<script type="module">
import { parseSVG, parseDXF } from '/js/import2d.js';
window.__import2d = { parseSVG, parseDXF };
window.__ready = true;
</script></body></html>`;
      res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
      res.end(html);
      return;
    }
    const rel = url.replace(/^\//, '');
    const file = path.join(WEB, rel);
    if (!file.startsWith(WEB) || !fs.existsSync(file) || !fs.statSync(file).isFile()) {
      res.writeHead(404);
      res.end('not found');
      return;
    }
    res.writeHead(200, { 'Content-Type': contentType(file) });
    res.end(fs.readFileSync(file));
  });
  return new Promise(resolve => {
    server.listen(0, '127.0.0.1', () => resolve(server));
  });
}

async function runAll() {
  const puppeteer = loadPuppeteer();
  const chrome = process.env.CHROME_PATH;
  if (!chrome) {
    console.error('import2d_golden: CHROME_PATH is not set');
    process.exit(1);
  }
  const server = await startServer();
  const { port } = server.address();
  const browser = await puppeteer.launch({
    executablePath: chrome,
    headless: 'new',
    args: ['--no-sandbox', '--disable-gpu'],
  });
  try {
    const page = await browser.newPage();
    await page.goto(`http://127.0.0.1:${port}/`, { waitUntil: 'networkidle0' });
    await page.waitForFunction('window.__ready === true');

    const records = {};
    const files = fs.readdirSync(CASES).filter(f => /\.(svg|dxf)$/i.test(f)).sort();
    for (const f of files) {
      const text = fs.readFileSync(path.join(CASES, f), 'utf8');
      const kind = f.toLowerCase().endsWith('.dxf') ? 'dxf' : 'svg';
      const result = await page.evaluate(async ({ text, kind }) => {
        try {
          const ents = kind === 'dxf'
            ? window.__import2d.parseDXF(text)
            : window.__import2d.parseSVG(text);
          return { entities: ents };
        } catch (e) {
          return { error: String(e && e.message ? e.message : e) };
        }
      }, { text, kind });
      records[f] = round(result);
    }
    const ordered = {};
    for (const k of Object.keys(records).sort()) ordered[k] = records[k];
    return ordered;
  } finally {
    await browser.close();
    server.close();
  }
}

const check = process.argv.includes('--check');
if (!fs.existsSync(CASES)) {
  console.error(`import2d_golden: missing ${path.relative(ROOT, CASES)}`);
  process.exit(1);
}
const golden = await runAll();
const text = dumps(golden);
if (check) {
  const existing = fs.existsSync(GOLDEN) ? fs.readFileSync(GOLDEN, 'utf8') : '';
  if (normNewlines(existing) !== normNewlines(text)) {
    console.error(`import2d_golden: MISMATCH ${path.relative(ROOT, GOLDEN)}`);
    process.exit(1);
  }
  console.log(`import2d_golden: ok (${Object.keys(golden).length} files)`);
  process.exit(0);
}
fs.mkdirSync(path.dirname(GOLDEN), { recursive: true });
fs.writeFileSync(GOLDEN, text, 'utf8');
console.log(`wrote ${path.relative(ROOT, GOLDEN)} (${Object.keys(golden).length} files)`);
