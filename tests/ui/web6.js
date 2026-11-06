// web edition as an app: installable, works offline, offers updates
const { FIX, OUT, WEB, DESKTOP } = require('./common');
const { chromium } = require('playwright');
const fs = require('fs');
const ROOT = process.env.ROOT || 'http://127.0.0.1:8791/app/';
const SWFILE = process.env.SWFILE || require('path').join(require('./common').ROOT, 'docs', 'app', 'sw.js');
const SP = FIX;
const log = m => console.log(m);
(async () => {
  const browser = await chromium.launch({ channel: 'chromium', args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
  const ctx = await browser.newContext({ viewport: { width: 1400, height: 850 }, acceptDownloads: true });
  await ctx.addInitScript(() => { try { localStorage.setItem('stle.webWelcome', '1'); } catch {} });
  const page = await ctx.newPage();
  const errs = []; page.on('pageerror', e => errs.push('pageerror ' + e.message)); page.on('console', m => { if (m.type() === 'error') errs.push('console ' + m.text()); });
  await page.goto(ROOT + '?web&sw'); await page.waitForTimeout(1500);
  await page.evaluate(() => navigator.serviceWorker.ready);
  await page.waitForTimeout(1500);
  const cached = await page.evaluate(async () => { const keys = await caches.keys(); const c = await caches.open(keys[0]); return [keys, (await c.keys()).length]; });
  log('cache ' + JSON.stringify(cached));
  const cdp = await ctx.newCDPSession(page);
  const inst = await cdp.send('Page.getInstallabilityErrors');
  log('installability errors: ' + JSON.stringify(inst.installabilityErrors.map(e => e.errorId)));
  const man = await cdp.send('Page.getAppManifest');
  log('manifest errors: ' + JSON.stringify(man.errors) + ' file_handlers=' + /file_handlers/.test(man.data || ''));
  // offline: reload and work without the network
  await ctx.setOffline(true);
  await page.reload(); await page.waitForTimeout(2000);
  log('offline reload ok: ' + await page.evaluate(() => !!window.app && document.title));
  const fc = page.waitForEvent('filechooser'); page.evaluate(() => window.app.cmd('import')); await (await fc).setFiles(`${SP}/blk.stl`); await page.waitForTimeout(1500);
  const api = (route, data) => page.evaluate(async ([r, d]) => { const { post } = await import('./js/api.js'); const x = await post(r, d, { quiet: true }); return JSON.parse(JSON.stringify(x, (k, v) => ArrayBuffer.isView(v) ? `[${v.length}]` : v)); }, [route, data]);
  let st = await api('state');
  await api('feature/add', { type: 'hollow', params: { body: st.bodies[0].id, thickness: 2, resolution: 'draft' } });
  const tx = await api('sketch_op', { op: 'text', text: 'OK', font: 'DejaVu Serif', size: 8, bold: true, entities: [] });
  st = await api('state');
  log(`offline: ${st.features.map(f => f.name + (f.error ? '!' : '')).join(' | ')}, text parts ${tx.entities.length}`);
  await ctx.setOffline(false);
  // a new version is published: the page offers to reload into it
  const orig = fs.readFileSync(SWFILE, 'utf8');
  // a copy for run.js to put back if this test is killed before it restores the file
  const BAK = require('path').join(OUT, 'sw.js.bak');
  fs.writeFileSync(BAK, orig);
  fs.writeFileSync(SWFILE, orig.replace(/const BUILD = '[^']*';/, "const BUILD = 'test-update';"));
  try {
    await page.evaluate(async () => (await navigator.serviceWorker.getRegistration()).update());
    await page.waitForSelector('#web-bar [data-reload]', { timeout: 20000 });
    log('update offered: ' + (await page.textContent('#web-bar')).trim().slice(0, 45));
    page.once('dialog', d => d.accept());
    const nav = page.waitForNavigation({ timeout: 20000 });
    await page.click('#web-bar [data-reload]');
    await page.waitForTimeout(300);
    if (await page.$('#modal:not(.hidden)')) await page.click('#modal-actions .primary');
    await nav; await page.waitForTimeout(1500);
    log('after reload cache: ' + JSON.stringify(await page.evaluate(() => caches.keys())));
    const rec = await page.evaluate(() => !document.querySelector('#modal').classList.contains('hidden') && document.querySelector('#modal-body').textContent.slice(0, 40));
    log('recovery offered after update reload: ' + rec);
  } finally { fs.writeFileSync(SWFILE, orig); fs.rmSync(BAK, { force: true }); }
  console.log(errs.filter(e => !/ERR_INTERNET_DISCONNECTED|Failed to load resource/.test(e)).join('\n') || 'no errors');
  await browser.close();
})();
