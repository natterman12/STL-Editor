const { FIX, OUT, WEB, DESKTOP } = require('./common');
const { chromium } = require('playwright');
const fs = require('fs');
const SP = FIX;
(async () => {
  const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
  const page = await browser.newPage({ viewport: { width: 1500, height: 900 } });
  const errs = [];
  page.on('console', m => { if (m.type() === 'error') errs.push(`console: ${m.text()}`); });
  page.on('pageerror', e => errs.push('pageerror: ' + e.message + '\n' + e.stack));
  await page.goto(DESKTOP); await page.waitForTimeout(800);
  await page.waitForFunction(() => window.app?.state); await page.evaluate(async () => { const { post } = await import('/js/api.js'); await post('new', { force: true }, { quiet: true }); await window.app.refresh(); });   // start from an empty project
  const state = async () => (await (await fetch(DESKTOP + 'api/state')).json());
  const post = async (r, d) => (await (await fetch(DESKTOP + 'api/' + r, { method: 'POST', body: JSON.stringify(d) })).json());
  const up = async (path, name) => page.evaluate(async ([b64, name]) => {
    const bin = Uint8Array.from(atob(b64), c => c.charCodeAt(0));
    const { upload } = await import('/js/api.js');
    const r = await upload(new File([bin], name)); await window.app.apply(r); window.app.viewer.setView('iso');
  }, [fs.readFileSync(path).toString('base64'), name]);
  const scr = async p => page.evaluate(p => { const r = document.getElementById('view3d').getBoundingClientRect(); const [x, y] = window.app.viewer.worldToScreen(p); return [x + r.left, y + r.top]; }, p);
  const log = m => console.log(m);
  await up(`${SP}/blk.stl`, 'blk.stl');     // 60 x 40 x 30, centred: x -30..30, y -20..20, z 0..30
  await page.waitForTimeout(500);
  let st = await state(); const B = st.bodies[0].id;
  const vol = async id => (await post('info', { id })).info.volume;
  const v0 = await vol(B);

  // --- hole preset: M3 insert, then magnet
  await page.click('[data-tool="hole"]'); await page.waitForTimeout(300);
  let p = await scr([10, 5, 30]); await page.mouse.click(p[0], p[1]); await page.waitForTimeout(400);
  await page.selectOption('[data-sel="preset"]', 'ins_M3'); await page.waitForTimeout(600);
  log('preset values ' + JSON.stringify(await page.evaluate(() => { const v = window.app.tool.values; return { kind: v.kind, d: v.diameter, depth: v.depth, through: v.through, tip: v.tip }; })));
  await page.click('#tool-ok'); await page.waitForTimeout(700);
  log('insert removed ' + (v0 - await vol(B)).toFixed(2) + ' expected ' + (Math.PI * 4 * 6.7).toFixed(2));
  await page.click('[data-tool="hole"]'); await page.waitForTimeout(300);
  p = await scr([-10, 5, 30]); await page.mouse.click(p[0], p[1]); await page.waitForTimeout(400);
  await page.selectOption('[data-sel="preset"]', 'nut_M3'); await page.waitForTimeout(600);
  await page.click('#tool-ok'); await page.waitForTimeout(700);
  st = await state(); log('nut trap: ' + (st.features.at(-1).error || 'ok'));

  // --- push/pull: pull the +x face out by 8
  const v1 = await vol(B);
  await page.click('[data-tool="pushpull"]'); await page.waitForTimeout(300);
  p = await scr([30, 0, 15]); await page.mouse.click(p[0], p[1]); await page.waitForTimeout(500);
  await page.fill('[data-num="distance"]', '8'); await page.press('[data-num="distance"]', 'Enter'); await page.waitForTimeout(900);
  await page.screenshot({ path: `${OUT}/n_pushpull.png` });
  await page.click('#tool-ok'); await page.waitForTimeout(700);
  log('pull added ' + (await vol(B) - v1).toFixed(1) + ' expected ' + (40 * 30 * 8));

  // --- sketch with arc3 + trim + mirror + offset + dimension edit, then extrude a picked region
  await page.evaluate(() => window.app.beginSketch('XY')); await page.waitForTimeout(800);
  const sk = async (u, v) => page.evaluate(([u, v]) => { const s = window.app.sketcher; const r = document.getElementById('view3d').getBoundingClientRect(); const [x, y] = window.app.viewer.worldToScreen(s.to3(u, v).toArray()); return [x + r.left, y + r.top]; }, [u, v]);
  const clickUV = async (u, v) => { const q = await sk(u, v); await page.mouse.move(q[0], q[1], { steps: 2 }); await page.mouse.click(q[0], q[1]); await page.waitForTimeout(120); };
  await page.keyboard.press('l');
  for (const [u, v] of [[0, -15], [20, -15], [20, 5], [0, 5]]) await clickUV(u, v);
  await page.keyboard.press('Escape');
  await page.keyboard.press('a');
  await clickUV(0, 5); await clickUV(0, -15); await clickUV(-10, -5);      // arc closes the D shape on the left
  log('after arc: ' + JSON.stringify(await page.evaluate(() => window.app.sketcher.entities.map(e => e.type))));
  await page.keyboard.press('l'); await clickUV(10, -20); await clickUV(10, 10); await page.keyboard.press('Escape');
  await page.keyboard.press('t'); await clickUV(10, 8); await clickUV(10, -18);
  log('after trim: ' + JSON.stringify(await page.evaluate(() => window.app.sketcher.entities.map(e => e.type + (e.type === 'line' ? `(${e.p1.map(Math.round)}-${e.p2.map(Math.round)})` : '')))));
  await page.keyboard.press('c'); await clickUV(-22, 14); await clickUV(-19, 14);
  await page.waitForTimeout(200);
  await page.keyboard.press('d'); await clickUV(-19, 14); await clickUV(-16, 18);
  const dim = (await page.$$('#dims .dim')).at(-1); log('dim label: ' + (dim ? await dim.textContent() : 'none'));
  if (await page.$('.dim-modify input')) { await page.fill('.dim-modify input', '8'); await page.keyboard.press('Enter'); await page.waitForTimeout(200); }
  log('circle r=' + await page.evaluate(() => window.app.sketcher.entities.at(-1).r));
  await page.evaluate(async () => { const s = window.app.sketcher; const c = s.entities.at(-1); s.setTool('select'); s.setSel([c.id]); });
  const pr = page.waitForSelector('#modal:not(.hidden)'); await page.click('[data-tool="sk_offset"]'); await pr;
  await page.fill('#modal-body input', '2'); await page.keyboard.press('Enter'); await page.waitForTimeout(500);
  log('after offset: ' + JSON.stringify(await page.evaluate(() => window.app.sketcher.entities.filter(e => e.type === 'circle').map(e => e.r))));
  await page.keyboard.press('l'); await clickUV(-10, 0); await clickUV(-10, 30); await page.keyboard.press('Escape');
  await page.evaluate(() => { const s = window.app.sketcher; const circles = s.entities.filter(e => e.type === 'circle').map(e => e.id); s.setTool('sk_mirror'); s.setSel(circles); });
  await clickUV(-10, 25);
  log('after mirror: circles at x = ' + JSON.stringify(await page.evaluate(() => window.app.sketcher.entities.filter(e => e.type === 'circle').map(e => Math.round(e.cx)))));
  await page.screenshot({ path: `${OUT}/n_sketch.png` });
  await page.click('[data-tool="finish_extrude"]'); await page.waitForTimeout(900);
  // pick one region: the inner circle island at (100,10) and the right half of the D
  await page.click('[data-pick="regions"]'); await page.waitForTimeout(200);
  await clickUV(-22, 14); await clickUV(15, -5); await page.waitForTimeout(900);
  log('regions ' + JSON.stringify(await page.evaluate(() => window.app.tool.values.regions)));
  await page.screenshot({ path: `${OUT}/n_regions.png` });
  await page.fill('[data-num="distance"]', '5'); await page.press('[data-num="distance"]', 'Enter'); await page.waitForTimeout(700);
  await page.click('#tool-ok'); await page.waitForTimeout(800);
  st = await state(); const ex = st.bodies.at(-1).id;
  log('extrude regions volume ' + (await vol(ex)).toFixed(1) + ' expected ' + ((Math.PI * 16 + 10 * 20) * 5).toFixed(1));

  // --- split with printed pins
  await page.evaluate(b => window.app.openTool('split', { values: { body: [b], plane: 'YZ', connector: 'pins', pin_d: 5, pin_len: 10 } }), B); await page.waitForTimeout(1200);
  await page.click('#tool-ok'); await page.waitForTimeout(900);
  st = await state(); log('split+pins: ' + (st.features.at(-1).error || 'ok') + ' bodies=' + st.bodies.length);

  // --- fit to bed on a long bar
  await up(`${SP}/long.stl`, 'long.stl'); await page.waitForTimeout(400);
  st = await state(); const L = st.bodies.at(-1).id;
  await page.evaluate(b => window.app.openTool('bedsplit', { values: { body: [b] } }), L); await page.waitForTimeout(1500);
  await page.evaluate(() => window.app.viewer.fit()); await page.waitForTimeout(500);
  await page.screenshot({ path: `${OUT}/n_bed.png` });
  await page.click('#tool-ok'); await page.waitForTimeout(900);
  st = await state(); log('bedsplit: ' + (st.features.at(-1).error || 'ok') + ' ' + (st.features.at(-1).warning || '') + ' bodies=' + st.bodies.map(b => b.name).join(','));

  // --- remesh + sculpt on a sphere
  await up(`${SP}/sph.stl`, 'sph.stl'); await page.waitForTimeout(400);
  st = await state(); const S = st.bodies.at(-1).id;
  await page.evaluate(b => { window.app.selection = [b]; }, S);
  await page.evaluate(b => window.app.openTool('remesh', { values: { body: [b], edge: 1.5 } }), S); await page.waitForTimeout(300);
  await page.click('#tool-ok'); await page.waitForTimeout(1500);
  await page.evaluate(b => { window.app.selection = [b]; window.app.viewer.fit(); }, S); await page.waitForTimeout(500);
  await page.click('[data-tool="sculpt"]').catch(() => {});
  if (!(await page.evaluate(() => window.app.tool?.key === 'sculpt'))) { await page.click('[data-tab="mesh"]'); await page.click('[data-tool="sculpt"]'); }
  await page.waitForTimeout(300);
  await page.click('[data-seg="mode"][data-val="inflate"]');
  await page.fill('[data-num="radius"]', '8'); await page.press('[data-num="radius"]', 'Enter');
  st = await state(); const sb = st.bodies.find(b => b.id === S).bbox; const c = [(sb[0][0] + sb[1][0]) / 2, (sb[0][1] + sb[1][1]) / 2, sb[1][2]];
  const a1 = await scr([c[0] - 8, c[1], c[2] - 2]), a2 = await scr([c[0] + 8, c[1], c[2] - 2]);
  const vs = await vol(S);
  await page.mouse.move(a1[0], a1[1]); await page.mouse.down(); await page.mouse.move(a2[0], a2[1], { steps: 12 }); await page.mouse.up();
  await page.waitForTimeout(1500);
  await page.screenshot({ path: `${OUT}/n_sculpt.png` });
  log('strokes ' + await page.evaluate(() => (window.app.tool.values.strokes || []).length));
  await page.click('#tool-ok'); await page.waitForTimeout(900);
  st = await state(); log('sculpt: ' + (st.features.at(-1).type) + ' ' + (st.features.at(-1).error || 'ok') + ' volume change ' + (await vol(S) - vs).toFixed(1));
  log('timeline: ' + st.features.map(f => `${f.name}${f.error ? '!' + f.error : ''}`).join(' | '));
  console.log(errs.join('\n') || 'no console errors');
  await browser.close();
})();
