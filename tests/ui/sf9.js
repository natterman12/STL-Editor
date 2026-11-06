// v1.9: Support Fins opens in Rotate, Remove fins, Add fins by hand; Move applies on release
const { FIX, OUT, WEB, DESKTOP } = require('./common');
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
  const vol = async id => (await api('info', { id })).info.volume;
  const msg = async () => (await page.textContent('#tool-msg')).trim();
  const waitInfo = () => page.waitForFunction(() => /Support Fins \d/.test(document.querySelector('#tool-msg').textContent), null, { timeout: 30000 });
  await api('new', { force: true });
  await page.evaluate(() => window.app.refresh());
  const imp = async (file, name) => page.evaluate(async ([b64, name, js]) => {
    const bin = Uint8Array.from(atob(b64), c => c.charCodeAt(0)); const { upload } = await import(js);
    const r = await upload(new File([bin], name), { place: 'keep' }); await window.app.apply(r); window.app.viewer.fit();
  }, [require('fs').readFileSync(file).toString('base64'), name, JS]);
  // ---------- 1. Rotate is there as soon as Support Fins opens
  await imp(`${SP}/ell.stl`, 'ell.stl'); await page.waitForTimeout(600);
  let st = await getState(); const L = st.bodies[0];
  await page.evaluate(() => window.app.viewer.setView('front')); await page.waitForTimeout(300);
  await page.click('[data-tab="supports"]'); await page.click('[data-tool="supportfins"]'); await page.waitForSelector('#tool-panel:not(.hidden)');
  await page.waitForTimeout(200);
  log('opened: mode ' + await page.evaluate(() => window.app.tool.values._mode) + ', handles ' + await page.evaluate(() => `${window.app.gizmo.tc.mode} visible ${window.app.gizmo.helper.visible}`));
  await waitInfo(); log('first build: ' + await msg());
  const ring = async () => {
    const c = await scr(await page.evaluate(() => window.app.gizmo.obj.position.toArray()));
    for (let d = 40; d < 200; d += 3) { const q = [c[0] + d * 0.707, c[1] - d * 0.707]; await page.mouse.move(q[0], q[1]); await page.waitForTimeout(25); if (await page.evaluate(() => window.app.gizmo.tc.axis) === 'Y') return q; }
    return null;
  };
  const turn = async (steps) => { const g = await ring(); if (!g) return false; await page.mouse.down(); for (let i = 1; i <= steps; i++) { await page.mouse.move(g[0] + i * 5, g[1] + i * 8); await page.waitForTimeout(25); } await page.mouse.up(); return true; };
  log('turned: ' + await turn(6)); await page.waitForTimeout(1500); await waitInfo().catch(() => {});
  st = await getState();
  log(`timeline: ${st.features.map(f => f.name).join(' | ')} · lowest point ${st.bodies[0].bbox[0][2].toFixed(3)} · ${await msg()}`);
  log('turned again: ' + await turn(4)); await page.waitForTimeout(1500); await waitInfo().catch(() => {});
  st = await getState();
  log(`timeline: ${st.features.map(f => f.name).join(' | ')} · rotate ${JSON.stringify(st.features[1]?.params.rotate)} · lowest ${st.bodies[0].bbox[0][2].toFixed(3)}`);
  await page.screenshot({ path: require('path').join(OUT, 'sf9_rotated.png') });
  // undo the turns for the rest of the test: put the L back upright
  await page.keyboard.press('Escape'); await page.waitForTimeout(300);
  await api('feature/delete', { id: st.features[1].id }); await page.evaluate(() => window.app.refresh()); await page.waitForTimeout(400);

  // ---------- 2. Add fins by hand, then Remove fins
  await page.evaluate(() => window.app.viewer.setView('bottom')); await page.waitForTimeout(300);
  await page.click('[data-tool="supportfins"]'); await page.waitForSelector('#tool-panel:not(.hidden)'); await waitInfo();
  log('auto on the L: ' + await msg());
  await page.click('[data-seg="_mode"][data-val="add"]'); await page.waitForTimeout(200);
  log('add mode panel: ' + (await page.textContent('#tool-form')).replace(/\s+/g, ' ').slice(0, 120));
  const x0 = L.bbox[0][0], y0 = L.bbox[0][1];
  const click = async (p, wait = 150) => { const q = await scr(p); await page.mouse.move(q[0], q[1]); await page.waitForTimeout(50); await page.mouse.click(q[0], q[1]); await page.waitForTimeout(wait); };
  await click([x0 + 24, y0 + 4, 27]); await click([x0 + 24, y0 + 16, 27]);
  await page.waitForTimeout(400); await waitInfo();
  log('after a hand wall: ' + await msg() + ' · walls ' + await page.evaluate(() => window.app.tool.values.walls.length));
  await page.click('[data-seg="_mode"][data-val="remove"]'); await page.waitForTimeout(300);
  const fins = await page.evaluate(() => Object.entries(window.app._sfPieces || {}).map(([id, p]) => ({ id, kind: p.kind, removed: p.removed })));
  log('pieces: ' + JSON.stringify(fins));
  const centre = id => page.evaluate(id => { const m = window.app.viewer.previewGroup.children.find(c => c.userData.id === id); m.geometry.computeBoundingBox(); return m.geometry.boundingBox.getCenter(new (m.position.constructor)()).toArray(); }, id);
  const clickFin = async id => { const q = await scr(await centre(id)); await page.mouse.move(q[0], q[1]); await page.waitForTimeout(80); const hov = await page.evaluate(() => window.app.viewer._pvHover); await page.mouse.click(q[0], q[1]); await page.waitForTimeout(300); await waitInfo().catch(() => {}); await page.waitForTimeout(300); return hov; };
  const auto = fins.find(f => f.kind === 'prop');
  log('hover before removing: ' + await clickFin(auto.id));
  log('removed one: ' + await msg() + ' · removed ' + await page.evaluate(() => window.app.tool.values.removed.length) + ' · ghosts ' + await page.evaluate(() => window.app.viewer.previewGroup.children.filter(c => c.material?.transparent && c.material.opacity < 0.5).length));
  await page.screenshot({ path: require('path').join(OUT, 'sf9_removed.png') });
  await clickFin(auto.id);   // the ghost: bring it back
  log('brought back: ' + await msg() + ' · removed ' + await page.evaluate(() => window.app.tool.values.removed.length));
  await clickFin(auto.id);
  const hand = Object.entries(await page.evaluate(() => window.app._sfPieces)).find(([, p]) => p.kind === 'hand');
  log('hand wall piece: ' + (hand ? hand[0] : 'none'));
  await page.click('#tool-ok'); await page.waitForFunction(() => !window.app.tool, null, { timeout: 30000 });
  st = await getState(); const f = st.features.at(-1);
  log(`committed ${f.name}: removed ${f.params.removed?.length || 0}, walls ${f.params.walls?.length || 0} · ${f.error || 'ok'}`);
  // reopen: the removed fin stays removed
  await page.evaluate(id => window.app.editFeature(id), f.id); await waitInfo();
  log('reopened: ' + await msg() + ' · mode ' + await page.evaluate(() => window.app.tool.values._mode));
  await page.keyboard.press('Escape'); await page.waitForTimeout(400);

  // ---------- 3. Move applies when the handle is let go
  const nBefore = (await getState()).features.length;
  await page.evaluate(() => window.app.viewer.setView('front')); await page.waitForTimeout(300);
  await page.evaluate(b => { window.app.selection = [b]; }, L.id);
  await page.click('[data-tab="print"]'); await page.waitForTimeout(200);
  await page.click('[data-tool="move"]'); await page.waitForSelector('#tool-panel:not(.hidden)'); await page.waitForTimeout(300);
  const drag = async () => {
    const g0 = await scr(await page.evaluate(() => window.app.gizmo.obj.position.toArray()));
    let grab = null;
    for (let d = 15; d < 200; d += 6) { await page.mouse.move(g0[0] + d, g0[1]); await page.waitForTimeout(25); if (await page.evaluate(() => window.app.gizmo.tc.axis) === 'X') { grab = [g0[0] + d, g0[1]]; break; } }
    if (!grab) return false;
    await page.mouse.down(); for (let i = 1; i <= 6; i++) { await page.mouse.move(grab[0] + i * 10, grab[1]); await page.waitForTimeout(25); } await page.mouse.up(); await page.waitForTimeout(1200);
    return true;
  };
  log('dragged: ' + await drag());
  st = await getState();
  log(`after letting go: ${st.features.length - nBefore} new step(s), buttons "${await page.textContent('#tool-ok')}"/"${await page.textContent('#tool-cancel')}", translate ${JSON.stringify(await page.evaluate(() => window.app.tool.values.translate))}`);
  log('dragged again: ' + await drag());
  st = await getState();
  log(`after the second: ${st.features.length - nBefore} new step(s), params ${JSON.stringify(st.features.find(x => x.id === (st.features.at(-1).id)).params.translate)}`);
  await page.keyboard.press('Escape'); await page.waitForTimeout(500);
  st = await getState();
  log(`closed with Esc: ${st.features.length - nBefore} new step(s) kept, rollback ${st.rollback}, body x ${st.bodies[0].bbox[0][0].toFixed(2)} (was ${L.bbox[0][0].toFixed(2)})`);
  log(errs.length ? 'ERRORS\n' + errs.join('\n') : 'no page errors');
  await browser.close();
})();
