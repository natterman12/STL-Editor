// Takes the website screenshots of the current UI (web edition, dark theme, 1600x900).
//   node dev/capture_site_screenshots.mjs [name...]   (from the repo root; needs tests/ui's npm install)
// Starts tests/ui/serve.js on :8790 (or uses one already running), writes docs/img/*.png.
import { createRequire } from 'module';
import path from 'path';
import fs from 'fs';
import http from 'http';
import { fileURLToPath } from 'url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '..');
const UI = path.join(ROOT, 'tests', 'ui');
const require = createRequire(path.join(UI, 'package.json'));
const { chromium } = require('playwright');
const { serve } = require(path.join(UI, 'serve.js'));
const IMG = path.join(ROOT, 'docs', 'img');
const FIX = path.join(UI, 'fixtures');
const PORT = 8790, URL = `http://127.0.0.1:${PORT}/index.html?web`;
fs.mkdirSync(IMG, { recursive: true });
const only = process.argv.slice(2);
const log = m => console.log(m);
const up = url => new Promise(res => { const r = http.get(url, x => { x.resume(); res(x.statusCode < 500); }); r.on('error', () => res(false)); r.setTimeout(2000, () => { r.destroy(); res(false); }); });

let server = null;
if (!(await up(`http://127.0.0.1:${PORT}/index.html`))) server = await serve(path.join(ROOT, 'web'), PORT);
const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
const errs = [];

async function fresh() {
  const ctx = await browser.newContext({ viewport: { width: 1600, height: 900 }, deviceScaleFactor: 1 });
  await ctx.addInitScript(() => { try { localStorage.setItem('stle.webWelcome', '1'); } catch {} });
  const page = await ctx.newPage();
  page.on('pageerror', e => errs.push('pageerror ' + e.message));
  await page.goto(URL); await page.waitForFunction(() => window.app, null, { timeout: 30000 }); await page.waitForTimeout(1500);
  await page.evaluate(() => window.app.setTheme?.('dark'));
  await api(page, 'new', { force: true });
  await page.evaluate(() => { window.app.refresh(); document.getElementById('web-bar')?.remove(); });
  return page;
}
const api = (page, route, data) => page.evaluate(async ([r, d]) => { const { post } = await import('/js/api.js'); const x = await post(r, d, { quiet: true }); return JSON.parse(JSON.stringify(x, (k, v) => ArrayBuffer.isView(v) ? 0 : v)); }, [route, data]);
const scr = (page, p) => page.evaluate(p => { const r = document.getElementById('view3d').getBoundingClientRect(); const [x, y] = window.app.viewer.worldToScreen(p); return [x + r.left, y + r.top]; }, p);
async function importStl(page, name) {
  await page.evaluate(async ([b64, name]) => {
    const bin = Uint8Array.from(atob(b64), c => c.charCodeAt(0)); const { upload } = await import('/js/api.js');
    const r = await upload(new File([bin], name), { place: 'keep' }); await window.app.apply(r);
  }, [fs.readFileSync(path.join(FIX, name)).toString('base64'), name]);
}
const settle = async (page, ms = 800) => { await page.evaluate(() => document.getElementById('web-bar')?.remove()); await page.mouse.move(1590, 450); await page.waitForTimeout(ms); };
const shot = async (page, name) => { await settle(page); await page.screenshot({ path: path.join(IMG, name) }); log('wrote ' + name); };

// A mounting bracket: base plate, a round boss, a slot, four counterbored holes.
async function buildPart(page) {
  const add = (type, params) => api(page, 'feature/add', { type, params });
  await add('primitive', { shape: 'box', length: 90, width: 56, height: 8, anchor: 'base', plane: 'XY', operation: 'new', offset: [0, 0, 0], rotate: [0, 0, 0], body_name: 'Bracket' });
  await add('primitive', { shape: 'cylinder', radius: 14, height: 22, anchor: 'base', plane: 'XY', operation: 'join', offset: [0, 0, 8], rotate: [0, 0, 0] });
  await add('primitive', { shape: 'box', length: 10, width: 56, height: 18, anchor: 'base', plane: 'XY', operation: 'join', offset: [-40, 0, 8], rotate: [0, 0, 0] });
  await add('hole', { points: [[0, 0, 30]], normal: [0, 0, 1], diameter: 12, through: true, kind: 'simple' });
  await add('hole', { points: [[33, 20, 8], [33, -20, 8], [-22, 20, 8], [-22, -20, 8]], normal: [0, 0, 1], diameter: 5.5, through: true, kind: 'counterbore', cb_diameter: 10, cb_depth: 3 });
  await page.evaluate(() => window.app.refresh());
  const st = await api(page, 'state');
  log('part: ' + st.features.map(f => f.type + (f.error ? '(' + f.error + ')' : '')).join(', '));
  await page.waitForTimeout(500);
  await page.evaluate(() => { window.app.viewer.setView('iso'); window.app.viewer.fit(); }); await page.waitForTimeout(600);
  return st;
}

const SHOTS = {
  async hero() {
    const page = await fresh(); await buildPart(page);
    await page.click('[data-tab="solid"]');
    await shot(page, 'hero.png'); await page.context().close();
  },
  async 'tool-search'() {
    const page = await fresh(); await buildPart(page);
    await page.keyboard.press('Control+k'); await page.waitForTimeout(200);
    await page.keyboard.type('ho', { delay: 80 }); await page.waitForTimeout(500);
    await page.screenshot({ path: path.join(IMG, 'tool-search.png') }); log('wrote tool-search.png');
    await page.context().close();
  },
  async 'holes-threads'() {
    const page = await fresh(); await buildPart(page);
    await page.click('[data-tab="solid"]'); await page.click('[data-tool="hole"]'); await page.waitForSelector('#tool-panel:not(.hidden)'); await page.waitForTimeout(400);
    const p = await scr(page, [28, 0, 8]); await page.mouse.click(p[0], p[1]); await page.waitForTimeout(800);
    await page.selectOption('[data-sel="preset"]', 'ins_M3').catch(e => log('no preset: ' + e.message)); await page.waitForTimeout(1200);
    await shot(page, 'holes-threads.png'); await page.context().close();
  },
  async sketch() {
    const page = await fresh(); await buildPart(page);
    const p = await scr(page, [12, -18, 8]); await page.mouse.dblclick(p[0], p[1]); await page.waitForTimeout(1500);
    log('sketch active ' + await page.evaluate(() => window.app.sketcher.active));
    const sk = uv => page.evaluate(uv => { const s = window.app.sketcher; const r = document.getElementById('view3d').getBoundingClientRect(); const [x, y] = window.app.viewer.worldToScreen(s.to3(...uv).toArray()); return [x + r.left, y + r.top]; }, uv);
    const click = async uv => { const q = await sk(uv); await page.mouse.move(q[0] - 2, q[1]); await page.mouse.move(q[0], q[1]); await page.waitForTimeout(50); await page.mouse.click(q[0], q[1]); await page.waitForTimeout(150); };
    const frame = await page.evaluate(() => { const s = window.app.sketcher; const o = s.to3(0, 0), u = s.to3(1, 0), v = s.to3(0, 1); return [o.toArray(), u.toArray(), v.toArray()]; });
    log('sketch frame ' + JSON.stringify(frame));
    await page.keyboard.press('r'); await click([18, 6]); await page.keyboard.type('12'); await page.keyboard.press('Tab'); await page.keyboard.type('8'); await page.keyboard.press('Enter'); await page.waitForTimeout(300);
    await page.keyboard.press('c'); await click([24, -14]); await page.keyboard.type('5'); await page.keyboard.press('Enter'); await page.waitForTimeout(300);
    await page.keyboard.press('Escape'); await page.keyboard.press('Escape');
    await page.evaluate(() => { const s = window.app.sketcher; s.setSel?.([]); s.redraw?.(); window.app.viewer.fit?.(); });
    await page.waitForTimeout(600);
    log('dims: ' + (await page.$$eval('#dims .dim', d => d.map(x => x.textContent))).join(' | '));
    await shot(page, 'sketch.png'); await page.context().close();
  },
  async 'fit-to-bed'() {
    const page = await fresh(); await importStl(page, 'long.stl'); await page.waitForTimeout(500);
    const L = (await api(page, 'state')).bodies.at(-1).id;
    await page.click('[data-tab="print"]'); await page.evaluate(b => window.app.openTool('bedsplit', { values: { body: [b] } }), L); await page.waitForSelector('#tool-panel:not(.hidden)');
    await page.waitForTimeout(2500);
    await page.click('#tool-ok'); await page.waitForFunction(() => !window.app.tool, null, { timeout: 60000 }); await page.waitForTimeout(800);
    const st = await api(page, 'state'); log('fit to bed: ' + (st.features.at(-1).error || 'ok') + ' ' + (st.features.at(-1).warning || '') + ' bodies=' + st.bodies.map(b => b.name).join(','));
    await page.evaluate(() => { window.app.viewer.setView('iso'); window.app.viewer.fit(); }); await page.waitForTimeout(800);
    await shot(page, 'fit-to-bed.png'); await page.context().close();
  },
  async 'support-fins'() {
    const page = await fresh(); await importStl(page, 'mushroom.stl'); await page.waitForTimeout(500);
    await page.evaluate(() => { window.app.viewer.setView('iso'); window.app.viewer.fit(); });
    await page.click('[data-tab="supports"]'); await page.click('[data-tool="supportfins"]'); await page.waitForSelector('#tool-panel:not(.hidden)');
    await page.waitForFunction(() => /held|Nothing/.test(document.querySelector('#tool-form [data-key="_cover"] .val')?.textContent || ''), null, { timeout: 60000 }).catch(() => log('fins: no coverage text'));
    await page.waitForTimeout(1500);
    // the Rotate mode's rings cover the fins in a still picture: hide them for the shot
    await page.evaluate(() => { window.app.gizmo.helper.visible = false; window.app.viewer.requestRender(); }); await page.waitForTimeout(300);
    await shot(page, 'support-fins.png'); await page.context().close();
  },
};

try {
  for (const [name, fn] of Object.entries(SHOTS)) {
    if (only.length && !only.includes(name)) continue;
    try { await fn(); } catch (e) { log(`FAILED ${name}: ${e.message}`); process.exitCode = 1; }
  }
} finally {
  await browser.close();
  if (server) server.close();
}
log(errs.length ? 'page errors:\n' + errs.join('\n') : 'no page errors');
