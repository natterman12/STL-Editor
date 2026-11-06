const { FIX, OUT, WEB, DESKTOP } = require('./common');
const { chromium, devices } = require('playwright');
const SP = FIX;
(async () => {
  const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
  for (const [name, dev] of [['ipad', devices['iPad Pro 11']], ['ipad-land', devices['iPad Pro 11 landscape']], ['phone', devices['iPhone 13']]]) {
    const ctx = await browser.newContext({ ...dev, deviceScaleFactor: 1 });
    await ctx.addInitScript(() => { try { localStorage.setItem('stle.webWelcome', '1'); } catch {} });
    const page = await ctx.newPage();
    const errs = []; page.on('pageerror', e => errs.push(e.message));
    await page.goto(WEB); await page.waitForTimeout(2000);
    const fc = page.waitForEvent('filechooser'); page.evaluate(() => window.app.cmd('import')); await (await fc).setFiles(`${SP}/blk.stl`); await page.waitForTimeout(1500);
    await page.screenshot({ path: require('path').join(OUT, `t_${name}.png`) });
    // open a tool, and the browser drawer
    await page.evaluate(() => window.app.openTool('hollow', { values: { body: [window.app.state.bodies[0].id], thickness: 2 } })); await page.waitForTimeout(600);
    if (await page.isVisible('#browser-toggle')) { await page.tap('#browser-toggle'); await page.waitForTimeout(400); }
    await page.screenshot({ path: require('path').join(OUT, `t_${name}_tool.png`) });
    // pinch-free check: a one-finger drag orbits the model in model mode
    if (await page.isVisible('#browser-toggle')) await page.tap('#browser-toggle');
    await page.waitForTimeout(300);
    const cam0 = await page.evaluate(() => window.app.viewer.camera.position.toArray().map(x => +x.toFixed(2)).join(','));
    const box = await page.locator('#view3d').boundingBox();
    const cdp = await ctx.newCDPSession(page);
    const pt = (x, y) => [{ x: box.x + x, y: box.y + y }];
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: pt(box.width / 2, box.height * 0.3) });
    for (let i = 1; i <= 8; i++) await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: pt(box.width / 2 + i * 12, box.height * 0.3) });
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    await page.waitForTimeout(400);
    const cam1 = await page.evaluate(() => window.app.viewer.camera.position.toArray().map(x => +x.toFixed(2)).join(','));
    console.log(name, 'one-finger drag orbits:', cam0 !== cam1);
    const m = await page.evaluate(() => ({ sw: document.documentElement.scrollWidth, w: innerWidth, h: innerHeight, sh: document.documentElement.scrollHeight }));
    console.log(name, JSON.stringify(m), errs.join('; ') || 'no errors');
    await ctx.close();
  }
  await browser.close();
})();
