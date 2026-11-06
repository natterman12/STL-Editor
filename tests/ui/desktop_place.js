const { FIX, OUT, WEB, DESKTOP } = require('./common');
const { chromium } = require('playwright'); const fs = require('fs'); const SP = FIX;
(async () => {
  const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader'] });
  const page = await browser.newPage({ viewport: { width: 1500, height: 900 } });
  const errs = []; page.on('pageerror', e => errs.push(e.message)); page.on('console', m => { if (m.type() === 'error') errs.push(m.text()); });
  await page.goto(DESKTOP); await page.waitForTimeout(700);
  await page.waitForFunction(() => window.app?.state); await page.evaluate(async () => { const { post } = await import('/js/api.js'); await post('new', { force: true }, { quiet: true }); await window.app.refresh(); });   // start from an empty project
  await page.evaluate(async b64 => { const bin = Uint8Array.from(atob(b64), c => c.charCodeAt(0)); const { upload } = await import('/js/api.js'); await window.app.apply(await upload(new File([bin], 'off.stl'))); window.app.viewer.setView('iso'); }, fs.readFileSync(`${SP}/off.stl`).toString('base64'));
  await page.waitForTimeout(800);
  let st = await (await fetch(DESKTOP + 'api/state')).json(); console.log('after open', JSON.stringify(st.bodies[0].bbox));
  await page.screenshot({ path: `${OUT}/place_center.png` });
  await page.dblclick(`[data-fid="${st.features[0].id}"]`); await page.waitForTimeout(700);
  await page.click('[data-seg="place"][data-val="keep"]'); await page.waitForTimeout(700); await page.click('#tool-ok'); await page.waitForTimeout(800);
  st = await (await fetch(DESKTOP + 'api/state')).json(); console.log('after keep', JSON.stringify(st.bodies[0].bbox), 'rollback', st.rollback);
  await page.keyboard.press('Control+z'); await page.waitForTimeout(700);
  st = await (await fetch(DESKTOP + 'api/state')).json(); console.log('after undo', JSON.stringify(st.bodies[0].bbox));
  console.log(errs.join('\n') || 'no errors'); await browser.close();
})();
