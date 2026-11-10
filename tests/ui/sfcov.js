const { FIX, OUT, WEB, DESKTOP, choose, openSection } = require('./common');
const { chromium } = require('playwright');
const BASE = process.env.BASE || WEB;
const SP = FIX;
const log = m => console.log(m);
(async () => {
  const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
  const ctx = await browser.newContext({ viewport: { width: 1500, height: 950 } });
  await ctx.addInitScript(() => { try { localStorage.setItem('stle.webWelcome', '1'); localStorage.removeItem('stle_sf_reach'); } catch {} });
  const page = await ctx.newPage();
  const errs = []; page.on('pageerror', e => errs.push('pageerror ' + e.message)); page.on('console', m => { if (m.type() === 'error' && !/400 \(Bad Request\)/.test(m.text())) errs.push('console ' + m.text()); });
  await page.goto(BASE); await page.waitForTimeout(1500);
  await page.evaluate(async () => { const { post } = await import('/js/api.js'); await post('new', { force: true }, { quiet: true }); await window.app.refresh(); });
  await page.evaluate(async b64 => {
    const bin = Uint8Array.from(atob(b64), c => c.charCodeAt(0)); const { upload } = await import('/js/api.js');
    const r = await upload(new File([bin], 'mushroom.stl'), { place: 'keep' }); await window.app.apply(r); window.app.viewer.fit();
  }, require('fs').readFileSync(`${SP}/mushroom.stl`).toString('base64'));
  await page.waitForTimeout(500);
  await page.evaluate(() => window.app.viewer.setView('bottom')); await page.waitForTimeout(300);
  await page.click('[data-tab="supports"]'); await page.click('[data-tool="supportfins"]'); await page.waitForSelector('#tool-panel:not(.hidden)');
  const held = () => page.$eval('#tool-form .sf-status', e => [...e.querySelectorAll('[data-key="_cover"] .val, [data-key="_cover"] .sub, .sf-cover-note')].map(x => x.textContent).join(' · ').replace(/\s+/g, ' ').trim()).catch(() => 'none');
  await page.waitForFunction(() => /held|Nothing/.test(document.querySelector('#tool-form [data-key="_cover"] .val')?.textContent || ''), null, { timeout: 30000 });
  log('auto: ' + await held());
  log('overlays: ' + await page.evaluate(() => JSON.stringify(window.app.viewer.overlay.children.map(o => o.userData.tag + ':' + (o.geometry?.attributes?.position?.count || 0)))));
  await page.screenshot({ path: require('path').join(OUT, 'sfcov_auto.png') });
  // Full coverage
  await choose(page, 'sf:mode', 'full').catch(e => log('no mode select ' + e.message));
  const before = await held();
  await page.waitForFunction(b => { const t = (document.querySelector('#tool-form [data-key="_cover"] .val')?.textContent || '').replace(/\s+/g, ' ').trim(); return /held/.test(t) && t !== b; }, before, { timeout: 30000 }).catch(() => {});
  log('full: ' + await held());
  await page.screenshot({ path: require('path').join(OUT, 'sfcov_full.png') });
  // a stricter allowed gap
  await openSection(page, '_hdr_check');
  await page.fill('[data-sfreach]', '3'); await page.press('[data-sfreach]', 'Tab');
  await page.waitForTimeout(300);
  await page.waitForFunction(() => /held/.test(document.querySelector('#tool-form [data-key="_cover"] .val')?.textContent || ''), null, { timeout: 30000 });
  log('reach 3: ' + await held());
  await page.screenshot({ path: require('path').join(OUT, 'sfcov_full3.png') });
  // Add fins: Fill gaps puts a hand wall across each bare spot
  await page.evaluate(() => window.app.setValue('_mode', 'add')); await page.waitForTimeout(300);
  await page.waitForFunction(() => !!document.querySelector('[data-sfact="fill"]'), null, { timeout: 30000 });
  await page.click('[data-sfact="fill"]'); await page.waitForTimeout(300);
  const walls = await page.evaluate(() => window.app.tool.values.walls.length);
  await page.waitForTimeout(3000); log('fill gaps: ' + walls + ' walls, engine: ' + await page.evaluate(() => JSON.stringify((window.app.tool._wallStatus || []).map(w => w.ok ? 'ok' : w.reason))));
  if (!walls) errs.push('Fill gaps added no walls');
  log(errs.length ? 'ERRORS\n' + errs.join('\n') : 'no page errors');
  await browser.close();
})();
