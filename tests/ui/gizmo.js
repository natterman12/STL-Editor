// Move / Rotate / Scale handles on the part, and Land on bed
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
  const scr = async p => page.evaluate(p => { const r = document.getElementById('view3d').getBoundingClientRect(); const [x, y] = window.app.viewer.worldToScreen(p); return [x + r.left, y + r.top]; }, p);
  await api('new', { force: true });
  await api('feature/add', { type: 'primitive', params: { shape: 'box', width: 40, depth: 20, height: 10, plane: 'XY', offset: [0, 0, 0], rotate: [0, 0, 0], anchor: 'base', operation: 'new', targets: [], body_name: 'Box' } });
  await page.evaluate(() => window.app.refresh()); await page.waitForTimeout(500);
  let st = await getState(); const B = st.bodies[0];
  log('box bbox ' + JSON.stringify(B.bbox));
  await page.evaluate(b => { window.app.selection = [b]; window.app.viewer.setView('front'); }, B.id); await page.waitForTimeout(500);
  // --- Rotate from the ribbon: rings on the part
  await page.click('[data-tab="print"]'); await page.waitForTimeout(150);
  await page.click('[data-tool="rotate"]'); await page.waitForSelector('#tool-panel:not(.hidden)'); await page.waitForTimeout(400);
  log('gizmo: ' + await page.evaluate(() => { const g = window.app.gizmo; return `${g.tc.mode} visible ${g.helper.visible} at ${g.obj.position.toArray().map(x => x.toFixed(1))}`; }));
  await page.screenshot({ path: require('path').join(OUT, 'gz_rotate.png') });
  // drag the Y ring (seen from the front, the green ring is a circle around the centre)
  const c = await scr([0, 0, 5]);
  // find a point on the Y ring: radius in screen px from the gizmo size
  const ringPx = await page.evaluate(() => { const g = window.app.gizmo, v = window.app.viewer; const p = g.obj.position.clone(); const ps = v.pixelSize(p.toArray()); const f = p.distanceTo(v.camera.position) * Math.min(Math.tan(Math.PI * v.camera.fov / 360) / v.camera.zoom, 7) / 4 * g.tc.size; return f / ps; });
  log('ring radius px ' + ringPx.toFixed(0));
  const sx = c[0], sy = c[1] - ringPx * 0.5 * 2 * 0.5;
  let grabbed = null;
  for (let d = 60; d < 140; d += 3) {
    const q = [c[0] + d * 0.707, c[1] - d * 0.707];
    await page.mouse.move(q[0], q[1]); await page.waitForTimeout(40);
    if (await page.evaluate(() => window.app.gizmo.tc.axis) === 'Y') { grabbed = q; break; }
  }
  log('hovered ring: ' + JSON.stringify(grabbed));
  if (grabbed) {
    await page.mouse.down(); for (let i = 1; i <= 10; i++) { await page.mouse.move(grabbed[0] + i * 6, grabbed[1] + i * 9); await page.waitForTimeout(30); }
    log('while dragging: rotate inputs ' + await page.evaluate(() => [...document.querySelectorAll('[data-vec="rotate"]')].map(e => e.value).join(',')));
    await page.mouse.up(); await page.waitForTimeout(1500);
  }
  log('after drop: rotate ' + await page.evaluate(() => JSON.stringify(window.app.tool.values.rotate)) + ' bed ' + await page.evaluate(() => window.app.tool.values.bed) + ' · msg ' + (await page.textContent('#tool-msg')).trim());
  await page.screenshot({ path: require('path').join(OUT, 'gz_rotated.png') });
  await page.click('#tool-ok'); await page.waitForTimeout(800);
  st = await getState();
  log('after OK bbox ' + JSON.stringify(st.bodies[0].bbox.map(p => p.map(x => +x.toFixed(2)))) + ' (min z should be 0)');
  // --- typed rotation lands too
  await page.click('[data-tool="move"]'); await page.waitForSelector('#tool-panel:not(.hidden)'); await page.waitForTimeout(300);
  log('move gizmo: ' + await page.evaluate(() => window.app.gizmo.tc.mode));
  await page.fill('[data-vec="rotate"][data-i="0"]', '30'); await page.press('[data-vec="rotate"][data-i="0"]', 'Tab'); await page.waitForTimeout(1200);
  // drag the Z arrow up
  await page.evaluate(() => window.app.viewer.setView('front')); await page.waitForTimeout(300);
  const g0 = await page.evaluate(() => window.app.gizmo.obj.position.toArray());
  const gs = await scr(g0);
  let zgrab = null;
  for (let d = 20; d < 160; d += 8) { await page.mouse.move(gs[0], gs[1] - d); await page.waitForTimeout(40); if (await page.evaluate(() => window.app.gizmo.tc.axis) === 'Z') { zgrab = [gs[0], gs[1] - d]; break; } }
  log('hovered Z arrow: ' + JSON.stringify(zgrab));
  if (zgrab) { await page.mouse.down(); for (let i = 1; i <= 8; i++) { await page.mouse.move(zgrab[0], zgrab[1] - i * 8); await page.waitForTimeout(30); } await page.mouse.up(); await page.waitForTimeout(1200); }
  log('after Z drag: translate ' + await page.evaluate(() => JSON.stringify(window.app.tool.values.translate)) + ' rotate ' + await page.evaluate(() => JSON.stringify(window.app.tool.values.rotate)));
  log('tool still has body ' + await page.evaluate(() => JSON.stringify(window.app.tool.values.bodies)));
  await page.click('#tool-ok'); await page.waitForTimeout(800);
  st = await getState();
  log('after OK min z ' + st.bodies[0].bbox[0][2].toFixed(2));
  // --- scale handles
  await page.click('[data-tool="scale"]'); await page.waitForSelector('#tool-panel:not(.hidden)'); await page.waitForTimeout(400);
  log('scale gizmo: ' + await page.evaluate(() => `${window.app.gizmo.tc.mode} visible ${window.app.gizmo.helper.visible}`));
  await page.screenshot({ path: require('path').join(OUT, 'gz_scale.png') });
  const p0 = await page.evaluate(() => window.app.gizmo.obj.position.toArray());
  const ps = await scr(p0);
  let xgrab = null;
  for (let d = 20; d < 200; d += 6) { await page.mouse.move(ps[0] + d, ps[1]); await page.waitForTimeout(30); if (await page.evaluate(() => window.app.gizmo.tc.axis) === 'X') { xgrab = [ps[0] + d, ps[1]]; break; } }
  log('hovered X handle: ' + JSON.stringify(xgrab));
  if (xgrab) { await page.mouse.down(); for (let i = 1; i <= 8; i++) { await page.mouse.move(xgrab[0] + i * 10, xgrab[1]); await page.waitForTimeout(30); } await page.mouse.up(); await page.waitForTimeout(1200); }
  log('after scale drag: ' + await page.evaluate(() => JSON.stringify({ mode: window.app.tool.values.mode, f: window.app.tool.values.factor, u: window.app.tool.values.uniform })));
  await page.keyboard.press('Escape'); await page.waitForTimeout(300);
  log('gizmo hidden after cancel: ' + !(await page.evaluate(() => window.app.gizmo.helper.visible)));
  log(errs.length ? 'ERRORS\n' + errs.join('\n') : 'no page errors');
  await browser.close();
})();
