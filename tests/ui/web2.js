const { FIX, OUT, WEB, DESKTOP } = require('./common');
const { chromium } = require('playwright');
const fs = require('fs');
const BASE = WEB;
const SP = FIX;
(async () => {
  const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
  const ctx = await browser.newContext({ viewport: { width: 1500, height: 900 }, acceptDownloads: true });
  await ctx.addInitScript(() => { delete window.showSaveFilePicker; delete window.showOpenFilePicker; });
  const page = await ctx.newPage();
  const errs = []; page.on('pageerror', e => errs.push('pageerror ' + e.message + '\n' + e.stack)); page.on('console', m => { if (m.type() === 'error') errs.push('console ' + m.text()); });
  await page.goto(BASE); await page.waitForTimeout(1500);
  const log = m => console.log(m);
  const api = (route, data) => page.evaluate(async ([r, d]) => { const { post } = await import('/js/api.js'); const x = await post(r, d, { quiet: true }); return JSON.parse(JSON.stringify(x, (k, v) => ArrayBuffer.isView(v) ? `[${v.length}]` : v)); }, [route, data]);
  const scr = async p => page.evaluate(p => { const r = document.getElementById('view3d').getBoundingClientRect(); const [x, y] = window.app.viewer.worldToScreen(p); return [x + r.left, y + r.top]; }, p);
  const vol = async id => (await api('info', { id })).info.volume;
  // import through the real file chooser
  const fc = page.waitForEvent('filechooser');
  page.evaluate(() => window.app.cmd('import'));
  await (await fc).setFiles(`${SP}/blk.stl`);
  await page.waitForTimeout(1200);
  let st = await api('state'); const B = st.bodies[0].id; log('imported ' + st.bodies.map(b => b.name + ' ' + b.solid));
  await page.evaluate(() => window.app.viewer.setView('iso')); await page.waitForTimeout(300);
  const v0 = await vol(B);
  // hole tool: click top face + preset
  await page.click('[data-tool="hole"]'); await page.waitForTimeout(300);
  let p = await scr([10, 5, 30]); await page.mouse.click(p[0], p[1]); await page.waitForTimeout(600);
  await page.selectOption('[data-sel="preset"]', 'ins_M3'); await page.waitForTimeout(800);
  await page.click('#tool-ok'); await page.waitForTimeout(800);
  log('insert removed ' + (v0 - await vol(B)).toFixed(2) + ' expected ' + (Math.PI * 4 * 6.7).toFixed(2));
  // sketch on top face via double click, draw rectangle, finish & extrude cut
  p = await scr([-15, -5, 30]); await page.mouse.dblclick(p[0], p[1]); await page.waitForTimeout(1200);
  log('sketch active ' + await page.evaluate(() => window.app.sketcher.active));
  const sk = async (u, v) => page.evaluate(([u, v]) => { const s = window.app.sketcher; const r = document.getElementById('view3d').getBoundingClientRect(); const [x, y] = window.app.viewer.worldToScreen(s.to3(u, v).toArray()); return [x + r.left, y + r.top]; }, [u, v]);
  const clickUV = async (u, v) => { const q = await sk(u, v); await page.mouse.move(q[0], q[1], { steps: 2 }); await page.mouse.click(q[0], q[1]); await page.waitForTimeout(150); };
  await page.keyboard.press('r'); await clickUV(-25, -15); await clickUV(-5, 5);
  // line across it to test regions + trim
  await page.keyboard.press('l'); await clickUV(-15, -18); await clickUV(-15, 8); await page.keyboard.press('Escape');
  await page.keyboard.press('t'); await clickUV(-15, 7); await page.waitForTimeout(400);
  log('after trim ' + JSON.stringify(await page.evaluate(() => window.app.sketcher.entities.map(e => e.type))));
  // text
  await page.click('[data-tool="text"]'); await page.waitForTimeout(200);
  let q = await sk(2, -10); await page.mouse.click(q[0], q[1]); await page.waitForSelector('#modal:not(.hidden)');
  log('fonts: ' + await page.evaluate(() => [...document.querySelectorAll('#tx-font option')].map(o => o.textContent).join(' | ')));
  await page.fill('#tx-text', 'Hi'); await page.fill('#tx-size', '8');
  await page.click('#modal-actions .primary'); await page.waitForTimeout(1500);
  log('text parts ' + await page.evaluate(() => window.app.sketcher.entities.filter(e => e.group).length));
  // project
  await page.click('[data-tool="sk_project"]'); await page.waitForSelector('#modal:not(.hidden)');
  await page.fill('#pj-offset', '1'); await page.check('#pj-cons'); await page.click('#modal-actions .primary'); await page.waitForTimeout(800);
  log('projected ' + await page.evaluate(() => window.app.sketcher.entities.filter(e => e.projected).length));
  await page.screenshot({ path: require('path').join(OUT, 'w4.png') });
  await page.click('[data-tool="finish_extrude"]'); await page.waitForTimeout(1500);
  await page.click('[data-pick="regions"]'); await page.waitForTimeout(300);
  await clickUV(-20, -5); await page.waitForTimeout(900);
  log('regions ' + JSON.stringify(await page.evaluate(() => window.app.tool.values.regions)));
  await page.selectOption('[data-sel="operation"]', 'cut').catch(() => {});
  await page.fill('[data-num="distance"]', '4'); await page.press('[data-num="distance"]', 'Enter'); await page.waitForTimeout(1200);
  const v1 = await vol(B);
  await page.click('#tool-ok'); await page.waitForTimeout(1200);
  st = await api('state');
  log('extrude: ' + (st.features.at(-1).error || 'ok') + ' removed ' + (v1 - await vol(B)).toFixed(1) + ' expected ' + (10 * 20 * 4));
  // thread on PRINT tab via click
  await page.click('[data-tab="solid"]'); await page.click('[data-tool="thread"]'); await page.waitForTimeout(300);
  p = await scr([20, 10, 30]); await page.mouse.click(p[0], p[1]); await page.waitForTimeout(1500);
  await page.click('#tool-ok'); await page.waitForTimeout(1200);
  st = await api('state'); log('thread: ' + (st.features.at(-1).error || 'ok'));
  // resize hole on the insert hole
  await page.click('[data-tool="resizehole"]'); await page.waitForTimeout(300);
  let picked = false;
  for (const pt of [[10 - 2, 5, 27], [10, 5 - 2, 27], [10 + 2, 5, 27], [10, 5 + 2, 27]]) {
    p = await scr(pt); await page.mouse.click(p[0], p[1]); await page.waitForTimeout(500);
    if (await page.evaluate(() => window.app.tool?.values.face?.label)) { picked = true; break; }
  }
  log('resize picked ' + picked + ' ' + await page.evaluate(() => window.app.tool?.values.face?.label));
  if (picked) { await page.fill('[data-num="diameter"]', '3.4'); await page.press('[data-num="diameter"]', 'Enter'); await page.waitForTimeout(800); await page.click('#tool-ok'); await page.waitForTimeout(900); }
  // undo/redo
  const vA = await vol(B); await page.keyboard.press('Control+z'); await page.waitForTimeout(800); const vB = await vol(B); await page.keyboard.press('Control+y'); await page.waitForTimeout(800);
  log('undo changed volume ' + (vA !== vB) + ', redo back ' + (Math.abs(await vol(B) - vA) < 1e-6));
  await page.evaluate(() => { window.app.viewer.setView('iso'); window.app.viewer.fit(); }); await page.waitForTimeout(600);
  await page.screenshot({ path: require('path').join(OUT, 'w5.png') });
  // save then reopen through the file chooser
  const [dl] = await Promise.all([page.waitForEvent('download'), page.evaluate(() => window.app.cmd('save'))]);
  const proj = require('path').join(OUT, 'web-project.stlx'); await dl.saveAs(proj);
  st = await api('state'); const nf = st.features.length;
  await page.evaluate(() => window.app.cmd('new')); await page.waitForTimeout(500);
  const fc2 = page.waitForEvent('filechooser');
  page.evaluate(() => window.app.cmd('open'));
  await (await fc2).setFiles(proj); await page.waitForTimeout(2500);
  st = await api('state');
  log('reopened features ' + st.features.length + ' of ' + nf + ' errors ' + st.features.filter(f => f.error).map(f => f.name + ':' + f.error).join(';'));
  console.log(errs.join('\n') || 'no errors');
  await browser.close();
})();
