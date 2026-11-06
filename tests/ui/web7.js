// Support Fins in the UI (either edition: BASE/JS pick the page and its API module)
const { FIX, OUT, WEB, DESKTOP } = require('./common');
const { chromium } = require('playwright');
const BASE = process.env.BASE || WEB;
const JS = process.env.JS || '/js/api.js';
const SP = FIX;
const log = m => console.log(m);
(async () => {
  const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
  const ctx = await browser.newContext({ viewport: { width: 1500, height: 950 } });
  await ctx.addInitScript(() => { try { localStorage.setItem('stle.webWelcome', '1'); } catch {} });
  const page = await ctx.newPage();
  const errs = []; page.on('pageerror', e => errs.push('pageerror ' + e.message)); page.on('console', m => { if (m.type() === 'error' && !/400 \(Bad Request\)/.test(m.text())) errs.push('console ' + m.text()); });
  await page.goto(BASE); await page.waitForTimeout(1500);
  const api = (route, data) => page.evaluate(async ([r, d, js]) => { const { post } = await import(js); const x = await post(r, d, { quiet: true }); return JSON.parse(JSON.stringify(x, (k, v) => ArrayBuffer.isView(v) ? `[${v.length}]` : v)); }, [route, data, JS]);
  const getState = () => page.evaluate(async js => { const { get } = await import(js); return get('state'); }, JS);
  const scr = async p => page.evaluate(p => { const r = document.getElementById('view3d').getBoundingClientRect(); const [x, y] = window.app.viewer.worldToScreen(p); return [x + r.left, y + r.top]; }, p);
  const vol = async id => (await api('info', { id })).info.volume;
  await api('new', { force: true });
  await page.evaluate(() => window.app.refresh());
  const up = async f => { await page.evaluate(async ([js, f]) => { }, [JS, f]); };
  // import through the page (works in both editions)
  const imp = async (file, name) => page.evaluate(async ([b64, name, js]) => {
    const bin = Uint8Array.from(atob(b64), c => c.charCodeAt(0)); const { upload } = await import(js);
    const r = await upload(new File([bin], name), { place: 'keep' }); await window.app.apply(r); window.app.viewer.fit();
  }, [require('fs').readFileSync(file).toString('base64'), name, JS]);
  await imp(`${SP}/tipped.stl`, 'tipped.stl'); await page.waitForTimeout(800);
  let st = await getState(); const T = st.bodies.at(-1).id; const v0 = await vol(T);
  await page.evaluate(b => { window.app.selection = [b]; }, T);
  await page.click('[data-tab="supports"]'); await page.waitForTimeout(200);
  await page.click('[data-tool="supportfins"]'); await page.waitForSelector('#tool-panel:not(.hidden)');
  await page.waitForTimeout(2500);
  const fields = await page.evaluate(() => [...document.querySelectorAll('#tool-form .field-hdr')].map(e => e.textContent));
  log('sections: ' + fields.join(' | ') + ' · fields ' + await page.locator('#tool-form .field').count());
  log('preview: ' + (await page.textContent('#tool-msg')).trim() + ' · red overlay ' + await page.evaluate(() => window.app.viewer.overlay.children.filter(c => c.userData.tag === 'sf-over').length));
  await page.screenshot({ path: require('path').join(OUT, 'sf_preview.png') });
  // a setting from Support Fins' list: firmer tine grip
  await page.fill('[data-num="sf:tineDensity"]', '100'); await page.press('[data-num="sf:tineDensity"]', 'Enter'); await page.waitForTimeout(2500);
  log('firm grip preview: ' + (await page.textContent('#tool-msg')).trim());
  await page.click('#tool-ok'); await page.waitForTimeout(2500);
  st = await getState();
  const f = st.features.at(-1);
  log(`added ${f.name} ${f.error || 'ok'} params ${JSON.stringify(f.params.options)} · volume ${v0.toFixed(0)} -> ${(await vol(T)).toFixed(0)}`);
  // edit it again from the timeline
  await page.evaluate(id => window.app.editFeature(id), f.id);
  await page.waitForTimeout(2000);
  log('edit panel open: ' + await page.isVisible('#tool-panel') + ' grip shows ' + await page.inputValue('[data-num="sf:tineDensity"]').catch(() => '?'));
  await page.click('[data-seg="output"][data-val="separate"]').catch(() => page.selectOption('[data-sel="output"]', 'separate'));
  await page.waitForTimeout(1500); await page.click('#tool-ok'); await page.waitForTimeout(2500);
  st = await getState();
  log('after edit bodies: ' + st.bodies.map(b => b.name).join(', '));
  // Draw Wall under the L's arm
  await imp(`${SP}/ell.stl`, 'ell.stl'); await page.waitForTimeout(800);
  st = await getState(); const Lb = st.bodies.at(-1);
  await page.evaluate(() => window.app.viewer.setView('bottom')); await page.waitForTimeout(600);
  await page.click('[data-tool="supportwall"]'); await page.waitForSelector('#tool-panel:not(.hidden)'); await page.waitForTimeout(500);
  await page.evaluate(b => { const t = window.app.tool; t.values.body = [b]; t.activeField = 'points'; window.app.renderForm(); }, Lb.id);
  const x0 = Lb.bbox[0][0], y0 = Lb.bbox[0][1];
  for (const p of [[x0 + 22, y0 + 3, 27], [x0 + 22, y0 + 17, 27]]) { const q = await scr(p); await page.mouse.click(q[0], q[1]); await page.waitForTimeout(500); }
  await page.waitForTimeout(2500);
  log('wall preview: ' + (await page.textContent('#tool-msg')).trim());
  await page.click('#tool-ok'); await page.waitForTimeout(2000);
  st = await getState(); log('timeline: ' + st.features.map(x => x.name + (x.error ? '!' + x.error : '')).join(' | '));
  // About
  await page.click('[data-menu="help"]'); await page.click('#menu .mi:has-text("About Support Fins")'); await page.waitForTimeout(800);
  log('about: ' + (await page.textContent('#modal-body')).replace(/\s+/g, ' ').slice(0, 90));
  console.log(errs.join('\n') || 'no errors');
  await browser.close();
})();
