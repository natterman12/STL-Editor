const { FIX, OUT, WEB, DESKTOP } = require('./common');
const { chromium } = require('playwright');
(async () => {
  const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
  const errs = [];
  for (const w of [1500, 900]) {
    const ctx = await browser.newContext({ viewport: { width: w, height: 800 } });
    await ctx.addInitScript(() => { try { localStorage.setItem('stle.webWelcome', '1'); } catch {} });
    const page = await ctx.newPage();
    page.on('pageerror', e => errs.push(e.message));
    await page.goto(WEB); await page.waitForTimeout(1500);
    for (const t of ['solid', 'print', 'supports', 'mesh']) {
      await page.click(`[data-tab="${t}"]`); await page.waitForTimeout(200);
      const info = await page.evaluate(() => [...document.querySelectorAll('#ribbon-body .rgroup')].map(g => (g.classList.contains('folded') ? '~' : '') + (g.classList.contains('pinned') ? '📌' : '') + g.querySelector('.rgroup-label').textContent).join(' | '));
      console.log(w, t, document = await page.textContent(`[data-tab="${t}"]`), '::', info);
      if (w === 1500 || t === 'solid') await page.screenshot({ path: require('path').join(OUT, `ribbon_${w}_${t}.png`), clip: { x: 0, y: 0, width: w, height: 140 } });
    }
    // all tools once
    const dup = await page.evaluate(() => { const seen = {}; for (const [t] of window.app.ribbonDef().tabs) for (const [, its] of window.app.ribbonDef(t).groups) for (const [k] of window.app.constructor.flatItems(its)) (seen[k] ||= []).push(t); return Object.entries(seen).filter(([, v]) => v.length > 1); });
    console.log('duplicates', JSON.stringify(dup));
    await ctx.close();
  }
  console.log(errs.length ? errs : 'no errors');
  await browser.close();
})();
