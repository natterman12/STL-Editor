const { FIX, OUT, WEB, DESKTOP } = require('./common');
const { chromium } = require('playwright');
const log = m => console.log(m);
(async () => {
  const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
  const ctx = await browser.newContext({ viewport: { width: 1500, height: 950 } });
  await ctx.addInitScript(() => { try { localStorage.setItem('stle.webWelcome', '1'); } catch {} });
  const page = await ctx.newPage();
  const errs = []; page.on('pageerror', e => errs.push('pageerror ' + e.message + ' ' + e.stack)); page.on('console', m => { if (m.type() === 'error' && !/400 \(Bad Request\)/.test(m.text())) errs.push('console ' + m.text()); });
  await page.goto(WEB); await page.waitForTimeout(1500);
  const api = (route, data) => page.evaluate(async ([r, d]) => { const { post } = await import('/js/api.js'); return JSON.parse(JSON.stringify(await post(r, d, { quiet: true }), (k, v) => ArrayBuffer.isView(v) ? 0 : v)); }, [route, data]);
  await api('new', { force: true });
  await api('feature/add', { type: 'primitive', params: { shape: 'cylinder', radius: 8, height: 10, anchor: 'base', plane: 'XY', operation: 'new', offset: [20, 10, 4], rotate: [0, 0, 0] } });
  await page.evaluate(() => window.app.refresh());
  await page.evaluate(() => window.app.beginSketch('XY')); await page.waitForTimeout(900);
  log('XY refs: ' + await page.evaluate(() => `${window.app.sketcher.refKind} ` + window.app.sketcher.refEnts.filter(e => e.ref === 'edge').map(e => e.type + (e.r ? ' r' + e.r : '')).join(',')));
  const scr = uv => page.evaluate(uv => { const sk = window.app.sketcher; const r = document.getElementById('view3d').getBoundingClientRect(); const [x, y] = window.app.viewer.worldToScreen(sk.to3(...uv).toArray()); return [x + r.left, y + r.top]; }, uv);
  const click = async uv => { const q = await scr(uv); await page.mouse.move(q[0] - 2, q[1]); await page.mouse.move(q[0], q[1]); await page.waitForTimeout(50); await page.mouse.click(q[0], q[1]); await page.waitForTimeout(120); };
  await page.keyboard.press('r');
  await click([10, 2]);
  log('scr00 ' + JSON.stringify(await scr([0, 0])) + ' ' + await page.evaluate(() => JSON.stringify(document.getElementById('view3d').getBoundingClientRect())));
  log('draw: ' + await page.evaluate(() => JSON.stringify({ tool: window.app.sketcher.tool, draw: window.app.sketcher.draw, act: document.activeElement?.id })));
  await page.keyboard.type('30');
  log('typed: ' + await page.evaluate(() => JSON.stringify({ a: document.getElementById('dyn-a').value, act: document.activeElement?.id }))); await page.keyboard.press('Tab'); await page.keyboard.type('15'); await page.keyboard.press('Enter'); await page.waitForTimeout(300);
  log('typed rect: ' + await page.evaluate(() => { const s = window.app.sketcher; return JSON.stringify(s.entities[0]) + ' ' + s.constraints.map(c => c.type + (c.value != null ? '=' + c.value : '') + (c.b?.e === '_o' ? '(origin)' : '')).join(',') + ' · ' + s.defText(); }));
  await page.keyboard.press('c'); await click([20, 10]); await page.keyboard.type('6'); await page.keyboard.press('Enter'); await page.waitForTimeout(300);
  log('typed circle: ' + await page.evaluate(() => { const s = window.app.sketcher; return JSON.stringify(s.entities[1]) + ' ' + s.constraints.slice(-3).map(c => c.type + '=' + c.value).join(',') + ' · ' + s.defText(); }));
  // line with a typed length
  await page.keyboard.press('l'); await click([12, 15]); await page.keyboard.type('12'); await page.keyboard.press('Enter'); await page.keyboard.press('Escape'); await page.waitForTimeout(200);
  log('typed line: ' + await page.evaluate(() => { const s = window.app.sketcher; return JSON.stringify(s.entities[2]) + ' ' + s.constraints.filter(c => c.a.e === s.entities[2].id).map(c => c.type + (c.value != null ? '=' + c.value : '')).join(','); }));
  // double-click a dimension and change it
  await page.keyboard.press('Escape');
  const d = await page.$$('#dims .dim'); log('labels: ' + (await Promise.all(d.map(x => x.textContent()))).join(' | '));
  await d[0].dblclick(); await page.waitForSelector('.dim-modify input'); await page.fill('.dim-modify input', '35'); await page.click('.dim-modify .ok'); await page.waitForTimeout(300);
  log('after dblclick 35: w ' + await page.evaluate(() => window.app.sketcher.entities[0].w));
  // Esc in the modify box leaves the value
  await (await page.$$('#dims .dim'))[0].dblclick(); await page.waitForSelector('.dim-modify input'); await page.fill('.dim-modify input', '99'); await page.keyboard.press('Escape'); await page.waitForTimeout(200);
  log('after Esc: w ' + await page.evaluate(() => window.app.sketcher.entities[0].w) + ' box ' + !!(await page.$('.dim-modify')));
  // the circle centre from the cylinder's centre (a part circle)
  await page.keyboard.press('d'); await click([20 + 3, 10]); await page.waitForTimeout(100);
  log('pick circle: ' + await page.evaluate(() => JSON.stringify(window.app.sketcher.dimPend)));
  await page.keyboard.press('Escape');
  await page.screenshot({ path: require('path').join(OUT, 'skdim2.png') });
  await page.evaluate(() => { document.documentElement.dataset.theme = 'dark'; window.app.sketcher._defKey = null; window.app.sketcher.setSel([]); window.app.sketcher.selC = null; window.app.sketcher.redraw(); });
  await page.waitForTimeout(300);
  await page.screenshot({ path: require('path').join(OUT, 'skdim2_dark.png'), clip: { x: 250, y: 150, width: 950, height: 736 } });
  log(errs.length ? 'ERRORS\n' + errs.join('\n') : 'no page errors');
  await browser.close();
})();
