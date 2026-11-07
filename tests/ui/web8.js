// Support Fins v1.8 UI: instant overhangs, fins-only preview, Draw Walls (lines, multi-line, curve)
const { FIX, OUT, WEB, DESKTOP, drawShape } = require('./common');
const { chromium } = require('playwright');
const BASE = process.env.BASE || WEB;
const JS = process.env.JS || '/js/api.js';
const SP = FIX;
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
  const scr = async p => page.evaluate(p => { const r = document.getElementById('view3d').getBoundingClientRect(); const [x, y] = window.app.viewer.worldToScreen(p); return [x + r.left, y + r.top]; }, p);
  const vol = async id => (await api('info', { id })).info.volume;
  const tags = () => page.evaluate(() => { const c = {}; for (const o of window.app.viewer.overlay.children) c[o.userData.tag] = (c[o.userData.tag] || 0) + 1; return c; });
  await api('new', { force: true });
  await page.evaluate(() => window.app.refresh());
  const imp = async (file, name) => page.evaluate(async ([b64, name, js]) => {
    const bin = Uint8Array.from(atob(b64), c => c.charCodeAt(0)); const { upload } = await import(js);
    const r = await upload(new File([bin], name), { place: 'keep' }); await window.app.apply(r); window.app.viewer.fit();
  }, [require('fs').readFileSync(file).toString('base64'), name, JS]);
  await imp(`${SP}/tipped.stl`, 'tipped.stl'); await page.waitForTimeout(800);
  let st = await getState(); const T = st.bodies.at(-1).id; const v0 = await vol(T);
  await page.evaluate(() => { window.app.selection = []; });
  await page.click('[data-tab="supports"]'); await page.waitForTimeout(200);
  await page.click('[data-tool="supportfins"]'); await page.waitForSelector('#tool-panel:not(.hidden)');
  // red straight away, before the engine answers
  await page.waitForTimeout(30);
  log('right after opening: body ' + await page.evaluate(() => JSON.stringify(window.app.tool.values.body)) + ' overlay ' + JSON.stringify(await tags()) + ' msg "' + (await page.textContent('#tool-msg')).trim() + '"');
  const t0 = Date.now();
  await page.waitForFunction(() => /Support Fins/.test(document.querySelector('#tool-msg').textContent), null, { timeout: 30000 });
  log(`preview after ${Date.now() - t0} ms: ` + (await page.textContent('#tool-msg')).trim());
  log('preview meshes: ' + await page.evaluate(() => window.app.viewer.previewGroup.children.filter(c => c.isMesh).map(c => c.material.color.getHexString() + ' ' + c.geometry.attributes.position.count / 3).join(', ')));
  await page.screenshot({ path: require('path').join(OUT, 'sf8_preview.png') });
  const t1 = Date.now();
  await page.click('#tool-ok'); await page.waitForFunction(() => !window.app.tool, null, { timeout: 30000 });
  log(`OK took ${Date.now() - t1} ms · volume ${v0.toFixed(0)} -> ${(await vol(T)).toFixed(0)}`);

  // Draw Walls under the L's arm: two lines, a multi-line, a curve
  await imp(`${SP}/ell.stl`, 'ell.stl'); await page.waitForTimeout(800);
  st = await getState(); const Lb = st.bodies.at(-1); const ve = await vol(Lb.id);
  await page.evaluate(b => { window.app.selection = [b]; window.app.viewer.setView('bottom'); }, Lb.id); await page.waitForTimeout(600);
  await page.click('[data-tool="supportwall"]'); await page.waitForSelector('#tool-panel:not(.hidden)'); await page.waitForTimeout(300);
  log('draw panel: ' + (await page.textContent('#tool-form')).replace(/\s+/g, ' ').slice(0, 160) + ' · overlay ' + JSON.stringify(await tags()));
  const x0 = Lb.bbox[0][0], y0 = Lb.bbox[0][1];
  const P = (x, y) => [x0 + x, y0 + y, 27];
  const click = async (p, wait = 150) => { const q = await scr(p); await page.mouse.move(q[0], q[1]); await page.waitForTimeout(60); await page.mouse.click(q[0], q[1]); await page.waitForTimeout(wait); };
  await click(P(13, 3)); await click(P(13, 17));
  await click(P(17, 3)); await click(P(17, 17));
  // the rubber band follows the pointer
  await click(P(21, 3)); { const q = await scr(P(21, 12)); await page.mouse.move(q[0], q[1]); await page.waitForTimeout(150); }
  log('band visible while drawing: ' + await page.evaluate(() => window.app.viewer._band.visible));
  await page.keyboard.press('Escape'); await page.waitForTimeout(100);
  log('after Esc: tool still open ' + await page.evaluate(() => !!window.app.tool) + ', pending ' + await page.evaluate(() => window.app.walls.pending.length));
  await drawShape(page, 'path');
  await click(P(21, 2)); await click(P(26, 6)); await click(P(26, 14)); await click(P(26, 14));   // double-click ends it
  await drawShape(page, 'curve');
  await click(P(28.5, 2)); await click(P(28.5, 18)); await click(P(24, 10));
  await page.waitForFunction(() => /walls/.test(document.querySelector('#tool-msg').textContent), null, { timeout: 30000 }).catch(() => {});
  await page.waitForTimeout(500);
  log('walls: ' + await page.evaluate(() => JSON.stringify(window.app.tool.values.walls.map(w => w.shape + ':' + w.points.length))));
  log('draw preview: ' + (await page.textContent('#tool-msg')).trim() + ' · ' + (await page.textContent('.walls-n')).trim());
  await page.evaluate(() => window.app.viewer.setView('iso')); await page.waitForTimeout(400);
  await page.screenshot({ path: require('path').join(OUT, 'sf8_walls.png') });
  // undo the curve, redo it with Ctrl+Z / clicks
  await page.keyboard.press('Control+z'); await page.waitForTimeout(400);
  log('after Ctrl+Z: ' + await page.evaluate(() => window.app.tool.values.walls.length) + ' walls');
  await page.evaluate(() => window.app.viewer.setView('bottom')); await page.waitForTimeout(400);
  await click(P(28.5, 2)); await click(P(28.5, 18)); await click(P(24, 10));
  await page.waitForTimeout(1500);
  await page.click('#tool-ok'); await page.waitForFunction(() => !window.app.tool, null, { timeout: 30000 });
  st = await getState();
  const f = st.features.at(-1);
  log(`added ${f.name} ${f.error || 'ok'} · ${f.params.walls.length} walls · volume ${ve.toFixed(0)} -> ${(await vol(Lb.id)).toFixed(0)}`);
  // reopen from the timeline and add one more
  await page.evaluate(id => window.app.editFeature(id), f.id); await page.waitForTimeout(1500);
  log('edit: ' + (await page.textContent('.walls-n')).trim());
  await page.click('#tool-cancel').catch(() => page.keyboard.press('Escape'));
  log(errs.length ? 'ERRORS\n' + errs.join('\n') : 'no page errors');
  await browser.close();
})();
