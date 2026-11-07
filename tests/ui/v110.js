// v1.10: SUPPORTS tab, no CONSTRUCT, Output buttons, Send to Slicer submenu, supports edited not stacked
const { FIX, OUT, WEB, DESKTOP, openSection } = require('./common');
const { chromium } = require('playwright');
const BASE = process.env.BASE || WEB;
const JS = process.env.JS || '/js/api.js';
const SP = FIX;
const log = m => console.log(m);
(async () => {
  const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
  const ctx = await browser.newContext({ viewport: { width: 1500, height: 950 }, acceptDownloads: true });
  await ctx.addInitScript(() => { try { localStorage.setItem('stle.webWelcome', '1'); localStorage.setItem('stle.tab', 'construct'); } catch {} delete window.showSaveFilePicker; delete window.showOpenFilePicker; });
  const page = await ctx.newPage();
  const errs = []; page.on('pageerror', e => errs.push('pageerror ' + e.message)); page.on('console', m => { if (m.type() === 'error' && !/400 \(Bad Request\)/.test(m.text())) errs.push('console ' + m.text()); });
  await page.goto(BASE); await page.waitForTimeout(1500);
  const api = (route, data) => page.evaluate(async ([r, d, js]) => { const { post } = await import(js); const x = await post(r, d, { quiet: true }); return JSON.parse(JSON.stringify(x, (k, v) => ArrayBuffer.isView(v) ? `[${v.length}]` : v)); }, [route, data, JS]);
  const getState = () => page.evaluate(async js => { const { get } = await import(js); return get('state'); }, JS);
  const scr = async p => page.evaluate(p => { const r = document.getElementById('view3d').getBoundingClientRect(); const [x, y] = window.app.viewer.worldToScreen(p); return [x + r.left, y + r.top]; }, p);
  const vol = async id => (await api('info', { id })).info.volume;
  const msg = async () => (await page.textContent('#tool-msg')).trim();
  const waitInfo = () => page.waitForFunction(() => /Support Fins \d/.test(document.querySelector('#tool-msg').textContent), null, { timeout: 30000 });
  log('tabs: ' + (await page.$$eval('#ribbon-tabs [data-tab]', els => els.map(e => e.textContent))).join(' ') + ' · open tab ' + await page.evaluate(() => window.app.tab));
  await api('new', { force: true });
  await page.evaluate(() => window.app.refresh());
  const imp = async (file, name) => page.evaluate(async ([b64, name, js]) => {
    const bin = Uint8Array.from(atob(b64), c => c.charCodeAt(0)); const { upload } = await import(js);
    const r = await upload(new File([bin], name), { place: 'keep' }); await window.app.apply(r); window.app.viewer.fit();
  }, [require('fs').readFileSync(file).toString('base64'), name, JS]);
  await imp(`${SP}/tipped.stl`, 'tipped.stl'); await page.waitForTimeout(600);
  let st = await getState(); const T = st.bodies[0].id;
  await page.click('[data-tab="supports"]'); await page.waitForTimeout(200);
  log('SUPPORTS tab: ' + (await page.$$eval('#ribbon-body [data-tool]', els => els.map(e => e.dataset.tool))).join(', '));
  // Print › Output › Export opens its menu and stays open
  await page.click('[data-tab="print"]'); await page.waitForTimeout(200);
  await page.click('#ribbon-body [data-tool="export"]'); await page.waitForTimeout(300);
  log('export menu open: ' + await page.isVisible('#menu') + ' · ' + (await page.textContent('#menu')).replace(/\s+/g, ' ').trim());
  const dl = page.waitForEvent('download', { timeout: 15000 }).catch(() => null);
  await page.click('#menu .mi:has-text("STL")'); const d = await dl;
  log('export download: ' + (d ? d.suggestedFilename() : 'none'));
  await page.click('#ribbon-body [data-tool="send"]'); await page.waitForTimeout(300);
  log('send to slicer: ' + (await page.isVisible('#menu') ? 'menu ' + (await page.textContent('#menu')).replace(/\s+/g, ' ').trim() : (await page.isVisible('#modal') ? 'dialog: ' + (await page.textContent('#modal-body')).slice(0, 60) : 'nothing')));
  await page.keyboard.press('Escape'); await page.click('body', { position: { x: 5, y: 900 } }).catch(() => {}); await page.waitForTimeout(200);
  if (await page.isVisible('#modal')) await page.click('#modal-actions button');
  // File › Send to Slicer opens beside the File menu
  await page.click('.menu-btn[data-menu="file"]'); await page.waitForTimeout(200);
  const hasSend = await page.$('#menu .mi.has-sub');
  if (hasSend) { await hasSend.hover(); await page.waitForTimeout(250); log('File › Send to Slicer submenu: ' + await page.isVisible('#menu .submenu') + ' · ' + (await page.textContent('#menu .submenu')).replace(/\s+/g, ' ').trim() + ' · File menu still open ' + await page.isVisible('#menu .mi:has-text("Save As")')); await page.screenshot({ path: require('path').join(OUT, 'v110_send.png') }); }
  else log('File menu has no Send to Slicer (web edition)');
  await page.keyboard.press('Escape'); await page.mouse.click(700, 900); await page.waitForTimeout(200);
  // Support Fins, OK, then Support Fins again: edits the same step
  await page.evaluate(b => { window.app.selection = [b]; }, T);
  await page.click('[data-tab="supports"]'); await page.waitForTimeout(200);
  await page.click('[data-tool="supportfins"]'); await page.waitForSelector('#tool-panel:not(.hidden)'); await waitInfo();
  await openSection(page, '_hdr_tines');
  await page.fill('[data-num="sf:tineDensity"]', '100'); await page.press('[data-num="sf:tineDensity"]', 'Enter'); await page.waitForTimeout(400); await waitInfo();
  await page.click('#tool-ok'); await page.waitForFunction(() => !window.app.tool, null, { timeout: 30000 });
  st = await getState(); const v1 = await vol(T);
  log(`after OK: ${st.features.map(f => f.name).join(' | ')} · volume ${v1.toFixed(0)}`);
  await page.click('[data-tool="supportfins"]'); await page.waitForSelector('#tool-panel:not(.hidden)'); await waitInfo().catch(() => {});
  log(`clicked Support Fins again: "${await page.textContent('#tool-title')}" · grip ${await page.inputValue('[data-num="sf:tineDensity"]')} · ${await msg()}`);
  await page.fill('[data-num="sf:tineDensity"]', '50'); await page.press('[data-num="sf:tineDensity"]', 'Enter'); await page.waitForTimeout(400); await waitInfo();
  await page.click('#tool-ok'); await page.waitForFunction(() => !window.app.tool, null, { timeout: 30000 });
  st = await getState();
  log(`after changing it: ${st.features.map(f => f.name).join(' | ')} · volume ${(await vol(T)).toFixed(0)} (was ${v1.toFixed(0)})`);
  // Draw Walls on the same part opens Support Fins in Add fins
  await page.click('[data-tool="supportwall"]'); await page.waitForSelector('#tool-panel:not(.hidden)'); await page.waitForTimeout(300);
  log(`Draw Walls on it: "${await page.textContent('#tool-title')}" mode ${await page.evaluate(() => window.app.tool.values._mode)}`);
  await page.keyboard.press('Escape'); await page.waitForTimeout(400);
  // turning the part afterwards rebuilds the fins instead of tipping them with it
  await page.evaluate(() => window.app.viewer.setView('front')); await page.waitForTimeout(300);
  await page.click('[data-tab="print"]'); await page.waitForTimeout(200);
  await page.click('[data-tool="rotate"]'); await page.waitForSelector('#tool-panel:not(.hidden)'); await page.waitForTimeout(300);
  const c = await scr(await page.evaluate(() => window.app.gizmo.obj.position.toArray()));
  let g = null;
  for (let dd = 40; dd < 220; dd += 3) { const q = [c[0] + dd * 0.707, c[1] - dd * 0.707]; await page.mouse.move(q[0], q[1]); await page.waitForTimeout(20); if (await page.evaluate(() => window.app.gizmo.tc.axis) === 'Y') { g = q; break; } }
  if (g) { await page.mouse.down(); for (let i = 1; i <= 5; i++) { await page.mouse.move(g[0] + i * 5, g[1] + i * 8); await page.waitForTimeout(25); } await page.mouse.up(); await page.waitForTimeout(1500); }
  await page.click('#tool-ok'); await page.waitForFunction(() => !window.app.tool, null, { timeout: 30000 }); await page.waitForTimeout(500);
  st = await getState();
  log(`after rotating: ${st.features.map(f => `${f.name}${f.error ? '!' + f.error : ''}`).join(' | ')} · lowest ${st.bodies[0].bbox[0][2].toFixed(3)} · toast "${await page.evaluate(() => [...document.querySelectorAll('.toast')].map(t => t.textContent).join(' / '))}"`);
  const sf = st.features.find(f => f.type === 'supportfins');
  log('fins after the turn: ' + (sf.warning || sf.info || 'ok'));
  await page.screenshot({ path: require('path').join(OUT, 'v110_rotated.png') });
  log(errs.length ? 'ERRORS\n' + errs.join('\n') : 'no page errors');
  await browser.close();
})();
