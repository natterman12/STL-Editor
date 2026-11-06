const { FIX, OUT, WEB, DESKTOP } = require('./common');
const { chromium } = require('playwright');
const SP = FIX;
(async () => {
  const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
  const page = await browser.newPage({ viewport: { width: 1500, height: 900 } });
  const errs = [];
  page.on('console', m => { if (m.type() === 'error' || m.type() === 'warning') errs.push(`${m.type()}: ${m.text()}`); });
  page.on('pageerror', e => errs.push('pageerror: ' + e.message));
  await page.goto(DESKTOP);
  await page.waitForFunction(() => window.app?.state); await page.evaluate(async () => { const { post } = await import('/js/api.js'); await post('new', { force: true }, { quiet: true }); await window.app.refresh(); });   // start from an empty project
  await page.waitForTimeout(1500);
  await page.screenshot({ path: `${OUT}/ui_empty.png` });
  console.log(errs.join('\n') || 'no console errors');
  await browser.close();
})();
