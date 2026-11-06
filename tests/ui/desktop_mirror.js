const { FIX, OUT, WEB, DESKTOP } = require('./common');
const { chromium } = require('playwright'); const fs = require('fs'); const SP = FIX;
(async () => {
  const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader'] });
  const page = await browser.newPage({ viewport: { width: 1500, height: 900 } });
  const errs = []; page.on('pageerror', e => errs.push(e.message + e.stack)); page.on('console', m => { if (m.type() === 'error') errs.push(m.text()); });
  await page.goto(DESKTOP); await page.waitForTimeout(700);
  await page.waitForFunction(() => window.app?.state); await page.evaluate(async () => { const { post } = await import('/js/api.js'); await post('new', { force: true }, { quiet: true }); await window.app.refresh(); });   // start from an empty project
  await page.evaluate(async b64 => { const bin = Uint8Array.from(atob(b64), c => c.charCodeAt(0)); const { upload } = await import('/js/api.js'); await window.app.apply(await upload(new File([bin], 'plate.stl'))); window.app.viewer.setView('iso'); }, fs.readFileSync(`${SP}/plate.stl`).toString('base64'));
  const post = (r, d) => page.evaluate(async ([r, d]) => (await fetch('/api/' + r, { method: 'POST', body: JSON.stringify(d) })).json(), [r, d]);
  let st = (await post('state', {}).catch(() => null));
  await post('feature/add', { type: 'sketch', params: { plane: { origin: [0, 0, 10], normal: [0, 0, 1] }, entities: [{ id: 'r', type: 'rect', x: 10, y: -5, w: 12, h: 10 }, { id: 'c', type: 'circle', cx: 20, cy: 12, r: 3 }] } });
  const st2 = await (await fetch(DESKTOP + 'api/state')).json();
  await post('feature/add', { type: 'extrude', params: { sketch: st2.features.at(-1).id, distance: 5, operation: 'cut' } });
  await page.evaluate(() => window.app.refresh()); await page.waitForTimeout(600);
  await page.click('[data-tool="mirror"]'); await page.waitForTimeout(1500);
  console.log('mirror values', JSON.stringify(await page.evaluate(() => window.app.tool.values)));
  await page.screenshot({ path: `${OUT}/mirror_preview.png` });
  await page.click('#tool-ok'); await page.waitForTimeout(900);
  const s3 = await (await fetch(DESKTOP + 'api/state')).json();
  console.log('timeline', s3.features.map(f => `${f.name}:${f.error || 'ok'}`).join(', '));
  const info = await post('info', { id: s3.bodies[0].id }); console.log('removed', 24000 - info.info.volume, 'expected', 2 * 5 * (120 + Math.PI * 9));
  // pattern the same cut via timeline click
  await page.click('[data-tool="pattern"]'); await page.waitForTimeout(500);
  console.log('pattern defaults', JSON.stringify(await page.evaluate(() => ({ s: window.app.tool.values.source, f: window.app.tool.values.features }))));
  await page.click('[data-tool="pattern"]'); // no-op re-open
  await page.keyboard.press('Escape'); await page.waitForTimeout(300);
  await page.evaluate(() => window.app.viewer.setView('top')); await page.waitForTimeout(500);
  await page.screenshot({ path: `${OUT}/mirror_done.png` });
  console.log(errs.join('\n') || 'no errors'); await browser.close();
})();
