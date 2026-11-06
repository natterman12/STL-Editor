const { FIX, OUT, WEB, DESKTOP } = require('./common');
const { chromium } = require('playwright');
const SP = FIX;
(async () => {
  const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
  const page = await browser.newPage({ viewport: { width: 1500, height: 900 } });
  const errs = []; page.on('pageerror', e => errs.push('pageerror: ' + e.message + e.stack)); page.on('console', m => { if (m.type() === 'error') errs.push(m.text()); });
  await page.goto(DESKTOP); await page.waitForTimeout(800);
  await page.waitForFunction(() => window.app?.state); await page.evaluate(async () => { const { post } = await import('/js/api.js'); await post('new', { force: true }, { quiet: true }); await window.app.refresh(); });   // start from an empty project
  const state = async () => (await (await fetch(DESKTOP + 'api/state')).json());
  // Sketch button -> pick XZ base plane in viewport
  await page.click('[data-tool="sketch"]'); await page.waitForTimeout(400);
  await page.evaluate(() => window.app.viewer.setView('front')); await page.waitForTimeout(500);
  await page.screenshot({ path: `${OUT}/s2_pick.png` });
  // click the XZ base plane (visible in front view) near its centre-ish
  const p = await page.evaluate(() => { const r = document.getElementById('view3d').getBoundingClientRect(); const [x, y] = window.app.viewer.worldToScreen([5, 0, 5]); return [x + r.left, y + r.top]; });
  await page.mouse.click(p[0], p[1]); await page.waitForTimeout(900);
  const sk = await page.evaluate(() => ({ active: window.app.sketcher.active, frame: window.app.sketcher.frameArr }));
  console.log('sketch active', JSON.stringify(sk));
  const sc = async (u, v) => page.evaluate(([u, v]) => { const s = window.app.sketcher; const r = document.getElementById('view3d').getBoundingClientRect(); const [x, y] = window.app.viewer.worldToScreen(s.to3(u, v).toArray()); return [x + r.left, y + r.top]; }, [u, v]);
  // closed polyline (L-profile) away from Y axis for revolve
  await page.keyboard.press('l');
  for (const [u, v] of [[10, 0], [20, 0], [20, 5], [14, 5], [14, 20], [10, 20], [10, 0]]) { const q = await sc(u, v); await page.mouse.move(q[0], q[1], { steps: 2 }); await page.mouse.click(q[0], q[1]); await page.waitForTimeout(80); }
  // a construction axis line along sketch Y (vertical) at u = 0
  await page.waitForTimeout(200);
  console.log('entities', JSON.stringify(await page.evaluate(() => window.app.sketcher.entities.map(e => [e.type, e.closed, (e.pts || []).length]))));
  await page.screenshot({ path: `${OUT}/s2_drawn.png` });
  await page.click('[data-tool="finish"]'); await page.waitForTimeout(900);
  let st = await state();
  const sid = st.features.at(-1).id;
  await page.evaluate(sid => window.app.openTool('revolve', { values: { sketch: sid } }), sid); await page.waitForTimeout(1200);
  await page.click('#tool-ok'); await page.waitForTimeout(900);
  st = await state(); console.log('revolve', st.features.at(-1).type, st.features.at(-1).error, st.bodies.length);
  // edit sketch from timeline: double click it, change the first polyline point
  await page.dblclick(`[data-fid="${sid}"]`); await page.waitForTimeout(900);
  console.log('editing sketch active', await page.evaluate(() => window.app.sketcher.active), 'rollback', (await state()).rollback);
  await page.evaluate(() => { const s = window.app.sketcher; const [a, b, c] = s.entities; s.updateEntity(a.id, { p2: [24, 0] }); s.updateEntity(b.id, { p1: [24, 0], p2: [24, 5] }); s.updateEntity(c.id, { p1: [24, 5] }); });
  await page.click('[data-tool="finish"]'); await page.waitForTimeout(1200);
  st = await state(); console.log('after edit', st.features.map(f => `${f.name}:${f.error || 'ok'}`).join(','), 'rollback', st.rollback);
  const r = await page.evaluate(async () => (await fetch('/api/info', { method: 'POST', body: JSON.stringify({ id: window.app.state.bodies[0].id }) })).json());
  console.log('volume', r.info.volume, 'expected', Math.PI * ((24 ** 2 - 10 ** 2) * 5 + (14 ** 2 - 10 ** 2) * 15));
  // undo the sketch edit -> volume back
  await page.keyboard.press('Control+z'); await page.waitForTimeout(900);
  const r2 = await page.evaluate(async () => (await fetch('/api/info', { method: 'POST', body: JSON.stringify({ id: window.app.state.bodies[0].id }) })).json());
  console.log('after undo volume', r2.info.volume, 'expected', Math.PI * ((20 ** 2 - 10 ** 2) * 5 + (14 ** 2 - 10 ** 2) * 15), 'rollback', (await state()).rollback);
  // save project to a path
  const sv = await page.evaluate(async sp => (await fetch('/api/save', { method: 'POST', body: JSON.stringify({ path: sp + '/test_project.stlx' }) })).json(), OUT);
  console.log('saved', sv.ok, sv.path);
  await page.evaluate(() => window.app.viewer.setView('iso')); await page.waitForTimeout(600);
  await page.screenshot({ path: `${OUT}/s2_final.png` });
  console.log(errs.join('\n') || 'no errors'); await browser.close();
})();
