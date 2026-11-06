const { FIX, OUT, WEB, DESKTOP } = require('./common');
const { chromium, devices } = require('playwright');
const SP = FIX;
(async () => {
  const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
  const ctx = await browser.newContext({ ...devices['iPad Pro 11'], deviceScaleFactor: 1 });
  await ctx.addInitScript(() => { try { localStorage.setItem('stle.webWelcome', '1'); } catch {} });
  const page = await ctx.newPage();
  await page.goto(WEB); await page.waitForTimeout(2000);
  const fc = page.waitForEvent('filechooser'); page.evaluate(() => window.app.cmd('import')); await (await fc).setFiles(`${SP}/blk.stl`); await page.waitForTimeout(1500);
  const scr = async p => page.evaluate(p => { const r = document.getElementById('view3d').getBoundingClientRect(); const [x, y] = window.app.viewer.worldToScreen(p); return [x + r.left, y + r.top]; }, p);
  const st = await page.evaluate(() => window.app.state.bodies[0].bbox);
  const top = [(st[0][0] + st[1][0]) / 2, (st[0][1] + st[1][1]) / 2, st[1][2]];
  const [x, y] = await scr(top);
  await page.touchscreen.tap(x, y); await page.waitForTimeout(500);
  console.log('tap selects:', JSON.stringify(await page.evaluate(() => window.app.selection)));
  await page.touchscreen.tap(x, y); await page.waitForTimeout(120); await page.touchscreen.tap(x, y); await page.waitForTimeout(1500);
  console.log('double tap sketches:', await page.evaluate(() => window.app.sketcher.active));
  if (await page.evaluate(() => window.app.sketcher.active)) {
    const sk = async (u, v) => page.evaluate(([u, v]) => { const s = window.app.sketcher; const r = document.getElementById('view3d').getBoundingClientRect(); const [x, y] = window.app.viewer.worldToScreen(s.to3(u, v).toArray()); return [x + r.left, y + r.top]; }, [u, v]);
    await page.evaluate(() => window.app.sketcher.setTool('rect'));
    await page.waitForTimeout(200);
    for (const [u, v] of [[-10, -5], [10, 5]]) { const q = await sk(u, v); await page.touchscreen.tap(q[0], q[1]); await page.waitForTimeout(250); }
    console.log('rect drawn by taps:', JSON.stringify(await page.evaluate(() => window.app.sketcher.entities.map(e => e.type))));
  }
  await page.screenshot({ path: require('path').join(OUT, 't_tap.png') });
  await browser.close();
})();
