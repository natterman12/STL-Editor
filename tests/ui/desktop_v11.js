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
  await fetch(DESKTOP + 'api/new', { method: 'POST', body: JSON.stringify({ force: true }) });
  await page.goto(DESKTOP); await page.waitForTimeout(1200);
  await page.waitForFunction(() => window.app?.state); await page.evaluate(async () => { const { post } = await import('/js/api.js'); await post('new', { force: true }, { quiet: true }); await window.app.refresh(); });   // start from an empty project
  const log = m => console.log(m);
  const state = async () => (await (await fetch(DESKTOP + 'api/state')).json());
  const post = async (r, d) => (await (await fetch(DESKTOP + 'api/' + r, { method: 'POST', body: JSON.stringify(d) })).json());
  // dismiss a recovery dialog if one is shown
  if (await page.$('#modal:not(.hidden)')) { await page.click('[data-close]').catch(() => {}); }
  const up = async (path, name) => page.evaluate(async ([b64, name]) => {
    const bin = Uint8Array.from(atob(b64), c => c.charCodeAt(0));
    const { upload } = await import('/js/api.js');
    const r = await upload(new File([bin], name)); await window.app.apply(r); window.app.viewer.setView('iso');
  }, [fs.readFileSync(path).toString('base64'), name]);
  const scr = async p => page.evaluate(p => { const r = document.getElementById('view3d').getBoundingClientRect(); const [x, y] = window.app.viewer.worldToScreen(p); return [x + r.left, y + r.top]; }, p);
  const vol = async id => (await post('info', { id })).info.volume;

  await up(`${SP}/blk.stl`, 'blk.stl'); await page.waitForTimeout(400);
  let st = await state(); const B = st.bodies[0].id;

  // --- thread: M8 tapped hole on the top face
  const v0 = await vol(B);
  await page.click('[data-tab="solid"]'); await page.click('[data-tool="thread"]'); await page.waitForTimeout(300);
  let p = await scr([10, 5, 30]); await page.mouse.click(p[0], p[1]); await page.waitForTimeout(1200);
  log('thread values ' + JSON.stringify(await page.evaluate(() => { const v = window.app.tool.values; return { kind: v.kind, d: v.diameter, p: v.pitch, op: v.operation, plane: !!v.plane?.origin }; })));
  await page.screenshot({ path: `${OUT}/v_thread.png` });
  await page.click('#tool-ok'); await page.waitForTimeout(900);
  st = await state(); log('tapped hole: ' + (st.features.at(-1).error || 'ok') + ' removed ' + (v0 - await vol(B)).toFixed(1));
  // screw: external M6 standing on XY away from the block
  await page.click('[data-tool="thread"]'); await page.waitForTimeout(300);
  await page.click('[data-seg="kind"][data-val="external"]');
  await page.selectOption('[data-sel="size"]', 'M6');
  const off = await page.$$('[data-vec="offset"]'); await off[0].fill('60'); await off[0].press('Enter');
  await page.fill('[data-num="length"]', '25'); await page.press('[data-num="length"]', 'Enter'); await page.waitForTimeout(1200);
  await page.click('#tool-ok'); await page.waitForTimeout(900);
  st = await state(); log('screw: ' + (st.features.at(-1).error || 'ok') + ' bodies=' + st.bodies.map(b => b.name).join(','));

  // --- lay flat: tilt a copy, then auto lay flat
  await page.evaluate(b => window.app.openTool('move', { values: { bodies: [b], rotate: [30, 20, 0], translate: [0, 0, 0] } }), B); await page.waitForTimeout(800);
  await page.click('#tool-ok'); await page.waitForTimeout(700);
  await page.evaluate(b => window.app.openTool('layflat', { values: { how: 'auto', bodies: [b] } }), B); await page.waitForTimeout(1200);
  await page.click('#tool-ok'); await page.waitForTimeout(800);
  st = await state(); const bb = st.bodies.find(b => b.id === B).bbox;
  log('layflat auto: ' + (st.features.at(-1).error || 'ok') + ' minZ=' + bb[0][2].toFixed(3) + ' height=' + (bb[1][2] - bb[0][2]).toFixed(2));
  // face mode: click the side face
  await page.evaluate(() => window.app.viewer.fit());
  await page.click('[data-tab="print"]'); await page.click('[data-tool="layflat"]'); await page.waitForTimeout(300);
  const side = await page.evaluate(b => { const s = window.app.state.bodies.find(x => x.id === b).bbox; return [(s[0][0] + s[1][0]) / 2, s[0][1] + 0.01, (s[0][2] + s[1][2]) / 2]; }, B);
  p = await scr(side); await page.mouse.click(p[0], p[1]); await page.waitForTimeout(1000);
  await page.click('#tool-ok'); await page.waitForTimeout(800);
  st = await state(); const bb2 = st.bodies.find(b => b.id === B).bbox;
  log('layflat face: ' + (st.features.at(-1).error || 'ok') + ' height=' + (bb2[1][2] - bb2[0][2]).toFixed(2));

  // --- constraints: square from lines (auto coincident + H/V), dimension, edit, drag
  await page.evaluate(() => window.app.beginSketch('XY')); await page.waitForTimeout(900);
  const sk = async (u, v) => page.evaluate(([u, v]) => { const s = window.app.sketcher; const r = document.getElementById('view3d').getBoundingClientRect(); const [x, y] = window.app.viewer.worldToScreen(s.to3(u, v).toArray()); return [x + r.left, y + r.top]; }, [u, v]);
  const clickUV = async (u, v) => { const q = await sk(u, v); await page.mouse.move(q[0], q[1], { steps: 2 }); await page.mouse.click(q[0], q[1]); await page.waitForTimeout(120); };
  await page.keyboard.press('l');
  for (const [u, v] of [[-20, -10], [0, -10], [0, 10], [-20, 10], [-20, -10]]) await clickUV(u, v);
  const cons = await page.evaluate(() => window.app.sketcher.constraints.map(c => c.type));
  log('auto constraints: ' + JSON.stringify(cons));
  // dimension the bottom line = 30, the right line = 15
  await page.evaluate(() => window.app.sketcher.setTool('c_dimension'));
  await clickUV(-10, -10); await clickUV(-10, -14);
  await page.waitForSelector('.dim-modify input'); await page.fill('.dim-modify input', '30'); await page.keyboard.press('Enter'); await page.waitForTimeout(300);
  const rm = await page.evaluate(() => { const e = window.app.sketcher.entities[1]; return [(e.p1[0] + e.p2[0]) / 2, (e.p1[1] + e.p2[1]) / 2]; });
  await clickUV(...rm);
  await clickUV(rm[0] + 5, rm[1]);
  await page.waitForSelector('.dim-modify input'); await page.fill('.dim-modify input', '15'); await page.keyboard.press('Enter'); await page.waitForTimeout(300);
  let ents = await page.evaluate(() => window.app.sketcher.entities.map(e => [e.p1.map(v => +v.toFixed(3)), e.p2.map(v => +v.toFixed(3))]));
  log('after dims: ' + JSON.stringify(ents));
  // fix the first corner, then drag the opposite corner: size is locked by dims, so nothing should stretch
  await page.evaluate(() => { const s = window.app.sketcher; s.setTool('select'); });
  // edit the 30 dimension label to 40
  const dl = await page.$$('#dims .dim.driving'); log('driving dims shown: ' + dl.length);
  if (dl.length) { await dl[0].dblclick(); await page.waitForSelector('.dim-modify input'); await page.fill('.dim-modify input', '40'); await page.keyboard.press('Enter'); await page.waitForTimeout(300); }
  ents = await page.evaluate(() => window.app.sketcher.entities.map(e => [e.p1.map(v => +v.toFixed(3)), e.p2.map(v => +v.toFixed(3))]));
  log('after edit 40: ' + JSON.stringify(ents));
  // remove the height dimension and drag the top-left corner up: height follows, square stays rectangular
  await page.evaluate(() => { const s = window.app.sketcher; const c = s.constraints.find(c => c.type === 'length' && Math.abs(c.value - 15) < 1e-6); s.deleteConstraint(c.id); });
  const tl = await page.evaluate(() => { const s = window.app.sketcher; const e = s.entities[2]; return e.p2; });
  const a = await sk(...tl), b2 = await sk(tl[0], tl[1] + 8);
  await page.mouse.move(a[0], a[1]); await page.mouse.down(); await page.mouse.move(b2[0], b2[1], { steps: 8 }); await page.mouse.up(); await page.waitForTimeout(300);
  ents = await page.evaluate(() => window.app.sketcher.entities.map(e => [e.p1.map(v => +v.toFixed(2)), e.p2.map(v => +v.toFixed(2))]));
  log('after drag: ' + JSON.stringify(ents));
  // a circle tangent to the top line, equal circle concentric check
  await page.keyboard.press('c'); await clickUV(-5, 30); await clickUV(-1, 30);
  await page.evaluate(() => window.app.sketcher.setTool('c_tangent'));
  const top = await page.evaluate(() => { const e = window.app.sketcher.entities[2]; return [(e.p1[0] + e.p2[0]) / 2, (e.p1[1] + e.p2[1]) / 2]; });
  await clickUV(...top);
  const circ = await page.evaluate(() => { const c = window.app.sketcher.entities.at(-1); return [c.cx + c.r, c.cy]; });
  await clickUV(...circ); await page.waitForTimeout(300);
  const tang = await page.evaluate(() => { const s = window.app.sketcher; const c = s.entities.at(-1), l = s.entities[2]; return { cy: +c.cy.toFixed(3), r: +c.r.toFixed(3), lineY: +l.p1[1].toFixed(3), n: s.constraints.length }; });
  log('tangent: ' + JSON.stringify(tang));
  await page.screenshot({ path: `${OUT}/v_constraints.png` });
  await page.click('[data-tool="finish"]'); await page.waitForTimeout(900);
  st = await state(); const skf = st.features.filter(f => f.type === 'sketch').at(-1);
  log('saved constraints: ' + (skf.params.constraints || []).length);
  // reopen sketch: constraints come back
  await page.dblclick(`[data-fid="${skf.id}"]`); await page.waitForTimeout(800);
  log('reopened constraints: ' + await page.evaluate(() => window.app.sketcher.constraints.length));
  await page.click('[data-tool="sk_cancel"]'); await page.waitForTimeout(600);

  // --- units + dark theme
  await page.evaluate(() => window.app.setUnits('in')); await page.waitForTimeout(300);
  await page.evaluate(b => window.app.openTool('primitive_box'), B); await page.waitForTimeout(400);
  const boxLen = await page.inputValue('[data-num="length"]'); const unitLbl = await page.textContent('[data-key="length"] .unit');
  log('inch mode box length field: ' + boxLen + ' ' + unitLbl);
  await page.fill('[data-num="length"]', '1 1/2'); await page.press('[data-num="length"]', 'Enter');
  log('stored mm: ' + await page.evaluate(() => window.app.tool.values.length));
  await page.click('#tool-cancel');
  await page.evaluate(() => window.app.setTheme('dark')); await page.waitForTimeout(500);
  await page.evaluate(() => window.app.viewer.setView('iso')); await page.waitForTimeout(600);
  await page.screenshot({ path: `${OUT}/v_dark.png` });
  log('theme attr: ' + await page.evaluate(() => document.documentElement.dataset.theme));
  // update banner rendering
  await page.evaluate(() => window.app.showUpdateBanner({ latest: '1.2.0', current: '1.1.0', url: 'https://github.com/example/stl-editor/releases/latest' }));
  await page.waitForTimeout(200); await page.screenshot({ path: `${OUT}/v_banner.png`, clip: { x: 1000, y: 30, width: 500, height: 160 } });
  await page.click('#banner [data-u="later"]');
  await page.evaluate(() => { window.app.setUnits('mm'); window.app.setTheme('light'); });
  st = await state(); log('timeline: ' + st.features.map(f => `${f.name}${f.error ? '!' + f.error : ''}`).join(' | '));
  console.log(errs.join('\n') || 'no console errors');
  await browser.close();
})();
