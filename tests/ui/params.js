// v1.11: parameters
const { FIX, OUT, WEB, DESKTOP } = require('./common');
const { chromium } = require('playwright');
const BASE = process.env.BASE || WEB;
const JS = process.env.JS || '/js/api.js';
const log = m => console.log(m);
(async () => {
  const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
  const ctx = await browser.newContext({ viewport: { width: 1500, height: 950 } });
  await ctx.addInitScript(() => { try { localStorage.setItem('stle.webWelcome', '1'); } catch {} });
  const page = await ctx.newPage();
  const errs = []; page.on('pageerror', e => errs.push('pageerror ' + e.message)); page.on('console', m => { if (m.type() === 'error' && !/400 \(Bad Request\)/.test(m.text())) errs.push('console ' + m.text()); });
  await page.goto(BASE); await page.waitForTimeout(1500);
  const api = (route, data) => page.evaluate(async ([r, d, js]) => { const { post } = await import(js); const x = await post(r, d, { quiet: true }); return JSON.parse(JSON.stringify(x, (k, v) => ArrayBuffer.isView(v) ? `[${v.length}]` : v)); }, [route, data, JS]);
  const getState = () => page.evaluate(async js => { const { get } = await import(js); return get('state'); }, JS);
  const vol = async id => (await api('info', { id })).info.volume;
  await api('new', { force: true }); await page.evaluate(() => window.app.refresh());
  // Edit › Parameters…
  await page.click('.menu-btn[data-menu="edit"]'); await page.click('#menu .mi:has-text("Parameters")'); await page.waitForSelector('table.params');
  const rows = async () => page.$$('#modal-body tr[data-row]');
  let r = await rows();
  await r[0].$('[data-pn]').then(e => e.fill('width')); await r[0].$('[data-pe]').then(e => e.fill('40'));
  await page.click('[data-padd]'); r = await rows();
  await r[1].$('[data-pn]').then(e => e.fill('height')); await r[1].$('[data-pe]').then(e => e.fill('width / 4 + 5'));
  await page.click('[data-padd]'); r = await rows();
  await r[2].$('[data-pn]').then(e => e.fill('loop')); await r[2].$('[data-pe]').then(e => e.fill('loop*2'));
  await page.waitForTimeout(200);
  log('live values: ' + (await page.$$eval('#modal-body td.pv', els => els.map(e => e.textContent.trim()))).join(' | '));
  await r[2].$('[data-pdel]').then(e => e.click());
  await page.screenshot({ path: require('path').join(OUT, 'params_dialog.png') });
  await page.click('#modal-actions button.primary'); await page.waitForTimeout(500);
  let st = await getState();
  log('state params: ' + JSON.stringify(st.params.map(q => `${q.name}=${q.expr}→${q.value}`)));
  log('tree: ' + (await page.textContent('#tree')).replace(/\s+/g, ' '));
  // a box whose sizes are parameters
  await page.click('[data-tab="solid"]');
  await page.click('[data-drop="primitive"]'); await page.click('#menu .mi:has-text("Box")'); await page.waitForSelector('#tool-panel:not(.hidden)'); await page.waitForTimeout(300);
  const nums = await page.$$eval('#tool-form [data-num], #tool-form [data-vec]', els => els.map(e => (e.dataset.num || e.dataset.vec + e.dataset.i)));
  log('box fields: ' + nums.join(', '));
  const first = await page.$('#tool-form [data-num]');
  const k0 = await first.getAttribute('data-num');
  await first.fill('width'); await first.press('Tab'); await page.waitForTimeout(300);
  const f2 = (await page.$$('#tool-form [data-num]'))[2]; const k2 = await f2.getAttribute('data-num');
  await f2.fill('=height*2'); await f2.press('Tab'); await page.waitForTimeout(600);
  log(`fields now: ${k0}=${await page.inputValue(`[data-num="${k0}"]`)} (${await page.getAttribute(`[data-num="${k0}"]`, 'title')}), ${k2}=${await page.inputValue(`[data-num="${k2}"]`)} (${await page.getAttribute(`[data-num="${k2}"]`, 'title')})`);
  await page.click('#tool-ok'); await page.waitForFunction(() => !window.app.tool, null, { timeout: 15000 }); await page.waitForTimeout(400);
  st = await getState(); const B = st.bodies[0];
  const size = b => b.bbox[1].map((x, i) => +(x - b.bbox[0][i]).toFixed(2)).join('×');
  log(`box: ${size(B)} · params ${JSON.stringify(st.features.at(-1).params[k0])}, ${JSON.stringify(st.features.at(-1).params[k2])}`);
  // change width: the box follows
  await page.click('#tree [data-params]'); await page.waitForSelector('table.params');
  r = await rows(); await r[0].$('[data-pe]').then(e => e.fill('60')); await page.waitForTimeout(150);
  log('live after edit: ' + (await page.$$eval('#modal-body td.pv', els => els.map(e => e.textContent.trim()))).join(' | '));
  await page.click('#modal-actions button.primary'); await page.waitForTimeout(800);
  st = await getState(); log(`after width=60: ${size(st.bodies[0])} · undo available ${st.doc.can_undo}`);
  await page.keyboard.press('Control+z'); await page.waitForTimeout(800);
  st = await getState(); log(`after undo: ${size(st.bodies[0])} · width ${st.params.find(q => q.name === 'width').value}`);
  // removing a used parameter asks first and the step then shows why
  await page.click('#tree [data-params]'); await page.waitForSelector('table.params');
  r = await rows(); await r[1].$('[data-pdel]').then(e => e.click());
  await page.click('#modal-actions button.primary'); await page.waitForTimeout(300);
  log('asks before removing a used one: ' + (await page.textContent('#modal-body')).slice(0, 90));
  await page.click('#modal-actions button.primary'); await page.waitForTimeout(800);
  st = await getState(); log('step error: ' + st.features.at(-1).error);
  // the project keeps them
  const bytes = await page.evaluate(async js => { const { post } = await import(js); const r = await post('save_bytes', {}, { quiet: true }); return r.bytes?.length || r.size || JSON.stringify(Object.keys(r)); }, JS).catch(e => 'n/a ' + e.message);
  log('save: ' + bytes);
  log(errs.length ? 'ERRORS\n' + errs.join('\n') : 'no page errors');
  await browser.close();
})();
