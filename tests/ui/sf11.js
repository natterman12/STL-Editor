// v1.11: Suggest orientations, strength, support plastic, gussets and anchors
const { FIX, OUT, WEB, DESKTOP, drawShape, drawShapes, openSection } = require('./common');
const { chromium } = require('playwright');
const BASE = process.env.BASE || WEB;
const JS = process.env.JS || '/js/api.js';
const SP = FIX;
const log = m => console.log(m);
(async () => {
  const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
  const ctx = await browser.newContext({ viewport: { width: 1500, height: 950 } });
  await ctx.addInitScript(() => { try { localStorage.setItem('stle.webWelcome', '1'); localStorage.setItem('stle_draw_shape', 'line'); } catch {} });
  const page = await ctx.newPage();
  const errs = []; page.on('pageerror', e => errs.push('pageerror ' + e.message)); page.on('console', m => { if (m.type() === 'error' && !/400 \(Bad Request\)/.test(m.text())) errs.push('console ' + m.text()); });
  await page.goto(BASE); await page.waitForTimeout(1500);
  const api = (route, data) => page.evaluate(async ([r, d, js]) => { const { post } = await import(js); const x = await post(r, d, { quiet: true }); return JSON.parse(JSON.stringify(x, (k, v) => ArrayBuffer.isView(v) ? `[${v.length}]` : v)); }, [route, data, JS]);
  const getState = () => page.evaluate(async js => { const { get } = await import(js); return get('state'); }, JS);
  const scr = async p => page.evaluate(p => { const r = document.getElementById('view3d').getBoundingClientRect(); const [x, y] = window.app.viewer.worldToScreen(p); return [x + r.left, y + r.top]; }, p);
  const msg = async () => (await page.textContent('#tool-msg')).trim();
  const text = async sel => (await page.textContent(sel).catch(() => '')).replace(/\s+/g, ' ').trim();
  const waitInfo = () => page.waitForFunction(() => /Support Fins \d/.test(document.querySelector('#tool-msg').textContent), null, { timeout: 30000 });
  await api('new', { force: true }); await page.evaluate(() => window.app.refresh());
  const imp = async (file, name) => page.evaluate(async ([b64, name, js]) => {
    const bin = Uint8Array.from(atob(b64), c => c.charCodeAt(0)); const { upload } = await import(js);
    const r = await upload(new File([bin], name), { place: 'keep' }); await window.app.apply(r); window.app.viewer.fit();
  }, [require('fs').readFileSync(file).toString('base64'), name, JS]);
  await imp(`${SP}/ell.stl`, 'ell.stl'); await page.waitForTimeout(600);
  let st = await getState(); const L = st.bodies[0];
  await page.click('[data-tab="supports"]'); await page.click('[data-tool="supportfins"]'); await page.waitForSelector('#tool-panel:not(.hidden)');
  await waitInfo(); await page.waitForTimeout(600);
  log('strength now: ' + await text('.sf-pose .sf-note'));
  log('support plastic: ' + await text('[data-key="_plastic"]'));
  await openSection(page, '_hdr_output');
  await page.selectOption('[data-sfmat]', 'petg'); await page.waitForTimeout(200);
  log('as PETG: ' + await text('[data-key="_plastic"] .val b'));
  // suggest
  await page.click('[data-sfact="suggest"]');
  await page.waitForSelector('.sf-cand', { timeout: 60000 });
  log('suggestions: ' + (await page.$$eval('.sf-cand span', els => els.map(e => e.textContent))).join(' || ') + ' · ' + await text('.sf-pose .sf-note'));
  await page.click('.sf-cand [data-sfact="use"][data-i="0"]'); await page.waitForTimeout(2500); await waitInfo().catch(() => {});
  st = await getState();
  log(`after Use: ${st.features.map(f => f.name).join(' | ')} · size ${st.bodies[0].bbox[1].map((x, i) => (x - st.bodies[0].bbox[0][i]).toFixed(0)).join('×')} · lowest ${st.bodies[0].bbox[0][2].toFixed(3)} · ${await msg()}`);
  log('strength after: ' + await text('.sf-pose .sf-note:last-of-type'));
  // a load
  await page.selectOption('[data-sel="_load"]', 'up'); await page.waitForTimeout(1200);
  log('load pulled up: ' + await text('.sf-verdict') + (await page.$('[data-sfact="strongest"]') ? ' [Turn it to the strongest pose]' : ''));
  if (await page.$('[data-sfact="strongest"]')) {
    await page.click('[data-sfact="strongest"]'); await page.waitForTimeout(2500);
    log('after strongest: ' + await text('.sf-verdict'));
  }
  await page.screenshot({ path: require('path').join(OUT, 'sf11_suggest.png') });
  await page.keyboard.press('Escape'); await page.waitForTimeout(400);
  // undo the turns: back to the L upright
  st = await getState();
  for (const f of st.features.filter(x => x.type === 'move')) await api('feature/delete', { id: f.id });
  await page.evaluate(() => window.app.refresh()); await page.waitForTimeout(400);
  st = await getState(); const L2 = st.bodies[0];
  // gusset and anchor in Add fins
  await page.click('[data-tool="supportfins"]'); await page.waitForSelector('#tool-panel:not(.hidden)'); await waitInfo();
  await page.click('[data-seg="_mode"][data-val="add"]'); await page.waitForTimeout(200);
  log('shapes: ' + (await drawShapes(page)).join(', '));
  const x0 = L2.bbox[0][0], y0 = L2.bbox[0][1];
  await page.evaluate(([x0, y0]) => { const v = window.app.viewer; v.camera.position.set(x0 + 90, y0 - 70, -25); v.controls.target.set(x0 + 12, y0 + 10, 18); v.controls.update(); v.requestRender(); }, [x0, y0]);
  await page.waitForTimeout(400);
  const click = async (p, wait = 200) => { const q = await scr(p); await page.mouse.move(q[0], q[1]); await page.waitForTimeout(60); await page.mouse.click(q[0], q[1]); await page.waitForTimeout(wait); };
  await drawShape(page, 'gusset');
  await click([x0 + 10, y0 + 10, 8]); await click([x0 + 18, y0 + 10, 27]);
  await page.waitForTimeout(800); await waitInfo().catch(() => {});
  log('after gusset: ' + await msg() + ' · walls ' + await page.evaluate(() => JSON.stringify(window.app.tool.values.walls.map(w => w.shape + ':' + (w.normal || []).map(x => x.toFixed(2)).join(',')))));
  await drawShape(page, 'anchor');
  await click([x0 + 10, y0 + 16, 15]);
  await page.waitForTimeout(800); await waitInfo().catch(() => {});
  log('after anchor: ' + await msg() + ' · plastic ' + await text('[data-key="_plastic"] .val b'));
  await page.screenshot({ path: require('path').join(OUT, 'sf11_gusset.png') });
  await page.click('#tool-ok'); await page.waitForFunction(() => !window.app.tool, null, { timeout: 30000 });
  st = await getState(); const f = st.features.at(-1);
  log(`committed ${f.name}: ${f.params.walls?.map(w => w.shape).join(', ')} · ${f.error || f.warning || 'ok'}`);
  log(errs.length ? 'ERRORS\n' + errs.join('\n') : 'no page errors');
  await browser.close();
})();
