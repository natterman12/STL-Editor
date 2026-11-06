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
  const up = async (path, name) => page.evaluate(async ([b64, name]) => {
    const bin = Uint8Array.from(atob(b64), c => c.charCodeAt(0));
    const { upload } = await import('/js/api.js');
    const r = await upload(new File([bin], name), { place: 'keep' }); await window.app.apply(r); window.app.viewer.fit();
  }, [fs.readFileSync(path).toString('base64'), name]);
  await up(`${SP}/bracket.stl`, 'bracket.stl');
  const scr = async p => page.evaluate(p => { const r = document.getElementById('view3d').getBoundingClientRect(); const [x, y] = window.app.viewer.worldToScreen(p); return [x + r.left, y + r.top]; }, p);
  const click3 = async p => { const [x, y] = await scr(p); await page.mouse.click(x, y); await page.waitForTimeout(400); };
  const log = [];
  const step = async (name, fn) => { try { await fn(); const st = await (await fetch(DESKTOP + 'api/state')).json(); const f = st.features.at(-1); log.push(`${name}: ${f?.type} err=${f?.error} warn=${f?.warning} bodies=${st.bodies.length}`); } catch (e) { log.push(`${name}: EXC ${e.message}`); } };
  await page.evaluate(() => window.app.viewer.setView('iso')); await page.waitForTimeout(500);

  await step('hole', async () => {
    await page.click('[data-tool="hole"]'); await page.waitForTimeout(300);
    await click3([30, 8, 20]); await click3([30, 22, 20]);
    await page.click('[data-seg="kind"][data-val="countersink"]'); await page.waitForTimeout(900);
    await page.screenshot({ path: `${OUT}/t_hole.png` });
    await page.click('#tool-ok'); await page.waitForTimeout(800);
  });
  await step('cylinder-cut-on-face', async () => {
    await page.click('[data-drop="primitive"]'); await page.click('#menu [data-mtool="primitive_cylinder"]'); await page.waitForTimeout(300);
    await click3([5, 15, 30]);
    await page.fill('[data-num="radius"]', '3'); await page.press('[data-num="radius"]', 'Enter');
    await page.click('[data-seg="anchor"][data-val="center"]');
    await page.click('[data-seg="operation"][data-val="cut"]'); await page.waitForTimeout(900);
    await page.click('#tool-ok'); await page.waitForTimeout(800);
  });
  await step('offset plane', async () => {
    await page.click('[data-tab="solid"]'); await page.evaluate(() => window.app.onRibbon('plane_offset')); await page.waitForTimeout(300);
    await page.click('[data-pick="base"]');
    await page.evaluate(() => window.app.feedPick({ kind: 'plane', id: 'YZ' }));
    await page.fill('[data-num="distance"]', '20'); await page.press('[data-num="distance"]', 'Enter');
    await page.waitForTimeout(500); await page.click('#tool-ok'); await page.waitForTimeout(600);
  });
  const st0 = await (await fetch(DESKTOP + 'api/state')).json();
  const body = st0.bodies[0].id, plane = st0.planes[0].id;
  await step('split', async () => {
    await page.evaluate(([b, p]) => window.app.openTool('split', { values: { body: [b], plane: p } }), [body, plane]); await page.waitForTimeout(1000);
    await page.screenshot({ path: `${OUT}/t_split.png` });
    await page.click('#tool-ok'); await page.waitForTimeout(800);
  });
  await step('mirror', async () => {
    const st = await (await fetch(DESKTOP + 'api/state')).json();
    await page.evaluate(([b]) => window.app.openTool('mirror', { values: { kind: 'bodies', bodies: [b], plane: 'XZ', mode: 'copy' } }), [st.bodies[1].id]); await page.waitForTimeout(800);
    await page.click('#tool-ok'); await page.waitForTimeout(800);
  });
  await step('move', async () => {
    const st = await (await fetch(DESKTOP + 'api/state')).json();
    await page.evaluate(([b]) => window.app.openTool('move', { values: { bodies: [b], translate: [0, -10, 0] } }), [st.bodies.at(-1).id]); await page.waitForTimeout(800);
    await page.click('#tool-ok'); await page.waitForTimeout(800);
  });
  await step('sphere new', async () => {
    await page.evaluate(() => window.app.openTool('primitive_sphere', { values: { radius: 12, offset: [80, 0, 0] } })); await page.waitForTimeout(800);
    await page.click('#tool-ok'); await page.waitForTimeout(800);
  });
  await step('hollow', async () => {
    const st = await (await fetch(DESKTOP + 'api/state')).json();
    await page.evaluate(([b]) => window.app.openTool('hollow', { values: { body: [b], thickness: 1.5 } }), [st.bodies.at(-1).id]); await page.waitForTimeout(300);
    await page.click('#tool-ok'); await page.waitForTimeout(4000);
  });
  await step('scale fit', async () => {
    const st = await (await fetch(DESKTOP + 'api/state')).json();
    await page.evaluate(([b]) => { window.app.selection = [b]; }, [st.bodies.at(-1).id]);
    await page.click('[data-tab="print"]'); await page.click('[data-tool="scale"]'); await page.waitForTimeout(300);
    await page.click('[data-seg="mode"][data-val="fit"]'); await page.waitForTimeout(200);
    const inp = await page.$$('[data-vec="size"]'); await inp[0].fill('48'); await inp[0].press('Enter'); await page.waitForTimeout(800);
    console.log('size vals', JSON.stringify(await page.evaluate(() => window.app.tool.values.size)));
    await page.click('#tool-ok'); await page.waitForTimeout(800);
  });
  await step('section', async () => {
    await page.click('[data-tool="section"]'); await page.waitForTimeout(400);
    await page.click('[data-sec-axis="y"]'); await page.waitForTimeout(400);
    await page.evaluate(() => window.app.viewer.setView('front')); await page.waitForTimeout(600);
    await page.screenshot({ path: `${OUT}/t_section.png` });
    await page.click('#tool-ok'); await page.waitForTimeout(300);
  });
  await step('info', async () => {
    const st = await (await fetch(DESKTOP + 'api/state')).json();
    await page.evaluate(([b]) => window.app.startInfo(b), [st.bodies.at(-1).id]); await page.waitForTimeout(600);
    await page.screenshot({ path: `${OUT}/t_info.png` });
    await page.click('#tool-ok');
  });
  await step('edit extrude via timeline', async () => {
    // add a box + sketch + extrude then edit the extrude distance
    const st = await (await fetch(DESKTOP + 'api/state')).json();
    const hole = st.features.find(f => f.type === 'hole');
    await page.dblclick(`[data-fid="${hole.id}"]`); await page.waitForTimeout(800);
    await page.fill('[data-num="diameter"]', '3'); await page.press('[data-num="diameter"]', 'Enter'); await page.waitForTimeout(800);
    await page.click('#tool-ok'); await page.waitForTimeout(900);
  });
  await step('undo', async () => { await page.keyboard.press('Control+z'); await page.waitForTimeout(800); });
  await step('redo', async () => { await page.keyboard.press('Control+y'); await page.waitForTimeout(800); });
  await page.evaluate(() => window.app.viewer.setView('iso')); await page.waitForTimeout(600);
  await page.screenshot({ path: `${OUT}/t_final.png` });
  // menus
  await page.click('[data-menu="file"]'); await page.waitForTimeout(200); await page.screenshot({ path: `${OUT}/t_menu.png` }); await page.keyboard.press('Escape'); await page.mouse.click(700, 500);
  const st = await (await fetch(DESKTOP + 'api/state')).json();
  console.log(log.join('\n'));
  console.log('timeline', st.features.map(f => `${f.name}${f.error ? '!' + f.error : ''}`).join(' | '));
  console.log(errs.join('\n') || 'no console errors');
  await browser.close();
})();
