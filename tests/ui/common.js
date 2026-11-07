// Shared settings for the UI tests (Playwright). See README.md.
const path = require('path');
const fs = require('fs');

const ROOT = path.resolve(__dirname, '..', '..');
const FIX = path.join(__dirname, 'fixtures');          // test parts
const OUT = path.join(__dirname, 'out');               // screenshots and saved files (git-ignored)
fs.mkdirSync(OUT, { recursive: true });

// The web edition (web/ served as static files) and the desktop engine (main.py --server).
// run.js starts both; override with WEB_URL / DESKTOP_URL to test elsewhere, e.g. the live site.
const WEB = process.env.WEB_URL || 'http://127.0.0.1:8790/index.html?web';
const DESKTOP = (process.env.DESKTOP_URL || 'http://127.0.0.1:8765/').replace(/\/?$/, '/');

// The tool panel shows a choice as buttons when its labels fit on one line, otherwise as a
// list; these pick an option either way. A folding section is opened before using what's in it.
async function choose(page, key, val) {
  await page.waitForSelector(`[data-seg="${key}"], [data-sel="${key}"]`);
  if (await page.$(`[data-seg="${key}"][data-val="${val}"]`)) return page.click(`[data-seg="${key}"][data-val="${val}"]`);
  return page.selectOption(`[data-sel="${key}"]`, val);
}
async function drawShape(page, k) {
  await page.waitForSelector('[data-wshape], [data-wshapesel]');
  if (await page.$(`[data-wshape="${k}"]`)) return page.click(`[data-wshape="${k}"]`);
  return page.selectOption('[data-wshapesel]', k);
}
const drawShapes = page => page.$$eval('[data-wshape], [data-wshapesel] option', els => els.map(e => e.textContent));
async function openSection(page, key) {
  const h = await page.waitForSelector(`[data-fold="${key}"]`);
  if (await h.getAttribute('aria-expanded') !== 'true') { await h.click(); await page.waitForTimeout(100); }
}

module.exports = { ROOT, FIX, OUT, WEB, DESKTOP, choose, drawShape, drawShapes, openSection };
