const { FIX, OUT, WEB, DESKTOP } = require('./common');
const { chromium } = require('playwright');
const fs = require('fs');
const SP = FIX;
(async () => {
  const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
  const page = await browser.newPage({ viewport: { width: 1500, height: 900 } });
  const errs = [];
  page.on('console', m => { if (m.type() === 'error') errs.push(`${m.type()}: ${m.text()}`); });
  page.on('pageerror', e => errs.push('pageerror: ' + e.message + '\n' + e.stack));
  await page.goto(DESKTOP);
  await page.waitForFunction(() => window.app?.state); await page.evaluate(async () => { const { post } = await import('/js/api.js'); await post('new', { force: true }, { quiet: true }); await window.app.refresh(); });   // start from an empty project
  await page.waitForTimeout(800);
  // upload via drop-equivalent API
  const buf = fs.readFileSync(require('path').join(FIX, 'bracket.stl')).toString('base64');
  await page.evaluate(async (b64) => {
    const bin = Uint8Array.from(atob(b64), c => c.charCodeAt(0));
    const f = new File([bin], 'bracket.stl');
    const { upload } = await import('/js/api.js');
    const r = await upload(f, { place: 'keep' }); await window.app.apply(r); window.app.viewer.setView('iso');
  }, buf);
  await page.waitForTimeout(800);
  await page.screenshot({ path: `${OUT}/ui_1_import.png` });
  // double-click top face: find screen pos of top-face centre
  const pos = await page.evaluate(() => { const r = document.getElementById('view3d').getBoundingClientRect(); const [x, y] = window.app.viewer.worldToScreen([20, 15, 20]); return [x + r.left, y + r.top]; });
  await page.mouse.dblclick(pos[0], pos[1]);
  await page.waitForTimeout(900);
  await page.screenshot({ path: `${OUT}/ui_2_sketch.png` });
  // draw a centre rectangle with typed sizes, and a circle
  await page.keyboard.press('r');
  const sc = async (u, v) => page.evaluate(([u, v]) => { const sk = window.app.sketcher; const p = sk.to3(u, v).toArray(); const r = document.getElementById('view3d').getBoundingClientRect(); const [x, y] = window.app.viewer.worldToScreen(p); return [x + r.left, y + r.top]; }, [u, v]);
  let a = await sc(8, 6), b = await sc(32, 24);
  await page.mouse.move(a[0], a[1]); await page.mouse.click(a[0], a[1]);
  await page.mouse.move(b[0], b[1], { steps: 5 }); await page.waitForTimeout(200);
  await page.screenshot({ path: `${OUT}/ui_3_drawing.png` });
  await page.mouse.click(b[0], b[1]);
  await page.keyboard.press('c');
  a = await sc(20, 15); b = await sc(24, 15);
  await page.mouse.move(a[0], a[1]); await page.mouse.click(a[0], a[1]);
  await page.mouse.move(b[0], b[1], { steps: 3 });
  await page.keyboard.type('6'); await page.keyboard.press('Enter');
  await page.waitForTimeout(300);
  await page.screenshot({ path: `${OUT}/ui_4_sketch_done.png` });
  console.log('entities', JSON.stringify(await page.evaluate(() => window.app.sketcher.entities)));
  // finish & extrude
  await page.click('[data-tool="finish_extrude"]');
  await page.waitForTimeout(1200);
  await page.click('[data-seg="operation"][data-val="cut"]');
  await page.fill('[data-num="distance"]', '5'); await page.press('[data-num="distance"]', 'Enter');
  await page.waitForTimeout(1500);
  await page.screenshot({ path: `${OUT}/ui_5_extrude_preview.png` });
  await page.click('#tool-ok');
  await page.waitForTimeout(1500);
  await page.evaluate(() => window.app.viewer.setView('iso'));
  await page.waitForTimeout(600);
  await page.screenshot({ path: `${OUT}/ui_6_result.png` });
  console.log('features', JSON.stringify((await (await fetch(DESKTOP + 'api/state')).json()).features.map(f => [f.type, f.error, f.warning])));
  console.log(errs.join('\n') || 'no console errors');
  await browser.close();
})();
