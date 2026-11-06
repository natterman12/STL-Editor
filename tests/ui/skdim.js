const { FIX, OUT, WEB, DESKTOP } = require('./common');
const { chromium } = require('playwright');
const BASE = process.env.BASE || WEB;
const JS = '/js/api.js';
const log = m => console.log(m);
(async () => {
  const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
  const ctx = await browser.newContext({ viewport: { width: 1500, height: 950 } });
  await ctx.addInitScript(() => { try { localStorage.setItem('stle.webWelcome', '1'); } catch {} });
  const page = await ctx.newPage();
  const errs = []; page.on('pageerror', e => errs.push('pageerror ' + e.message + ' ' + e.stack)); page.on('console', m => { if (m.type() === 'error' && !/400 \(Bad Request\)/.test(m.text())) errs.push('console ' + m.text()); });
  await page.goto(BASE); await page.waitForTimeout(1500);
  const api = (route, data) => page.evaluate(async ([r, d, js]) => { const { post } = await import(js); const x = await post(r, d, { quiet: true }); return JSON.parse(JSON.stringify(x, (k, v) => ArrayBuffer.isView(v) ? `[${v.length}]` : v)); }, [route, data, JS]);
  await api('new', { force: true });
  await api('feature/add', { type: 'primitive', params: { shape: 'box', length: 40, width: 20, height: 10, anchor: 'corner', plane: 'XY', operation: 'new', offset: [0, 0, 0], rotate: [0, 0, 0] } });
  await page.evaluate(() => window.app.refresh()); await page.waitForTimeout(500);
  const st0 = await page.evaluate(() => window.app.state.bodies.map(b => [b.id, b.bbox]));
  log('body ' + JSON.stringify(st0));
  const bid = st0[0][0];
  await page.evaluate(id => window.app.enterSketch({ plane: { origin: [0, 0, 10], normal: [0, 0, 1], xdir: [1, 0, 0], label: 'Top', body: id } }), bid);
  await page.waitForTimeout(800);
  log('refs: ' + await page.evaluate(() => `${window.app.sketcher.refKind} ` + window.app.sketcher.refEnts.map(e => e.id + ':' + e.type + (e.ref === 'edge' ? '' : '/' + e.ref)).join(' ')));
  const scr = uv => page.evaluate(uv => { const sk = window.app.sketcher; const r = document.getElementById('view3d').getBoundingClientRect(); const [x, y] = window.app.viewer.worldToScreen(sk.to3(...uv).toArray()); return [x + r.left, y + r.top]; }, uv);
  const click = async (uv, wait = 120) => { const q = await scr(uv); await page.mouse.move(q[0] - 3, q[1] - 2); await page.mouse.move(q[0], q[1]); await page.waitForTimeout(60); await page.mouse.click(q[0], q[1]); await page.waitForTimeout(wait); };
  const move = async uv => { const q = await scr(uv); await page.mouse.move(q[0], q[1], { steps: 3 }); await page.waitForTimeout(80); };
  const tool = t => page.evaluate(t => window.app.onRibbon(t), t);
  const def = () => page.evaluate(() => window.app.sketcher.defText());
  const typeModify = async v => { await page.waitForSelector('.dim-modify input'); await page.fill('.dim-modify input', v); await page.press('.dim-modify input', 'Enter'); await page.waitForTimeout(200); };
  await tool('rect');
  await click([5, 5]); await click([25, 15]);
  log('rect drawn: ' + await def() + ' · ' + JSON.stringify(await page.evaluate(() => window.app.sketcher.entities)));
  await tool('c_dimension');
  await click([15, 5]); await move([15, 1.5]);
  log('pending: ' + await page.evaluate(() => JSON.stringify(window.app.sketcher.dimPend.cand)));
  await page.screenshot({ path: require('path').join(OUT, 'skdim_pending.png') });
  await click([15, 1.5]);
  await typeModify('30');
  log('width 30: ' + JSON.stringify(await page.evaluate(() => window.app.sketcher.entities[0])) + ' · ' + await def());
  // height: right side
  await click([35, 10]); await move([39, 10]); await click([39, 10]); await typeModify('12');
  // left side of rect to the part's left edge
  await click([5, 9]); await click([0, 9]); await move([2.5, 19]); await click([2.5, 19]); await typeModify('6');
  // bottom side to the part's bottom edge
  await click([20, 5]); await click([20, 0]); await move([33, 2.5]); await click([33, 2.5]); await typeModify('4');
  log('after 4 dims: ' + JSON.stringify(await page.evaluate(() => window.app.sketcher.entities[0])) + ' · ' + await def());
  log('dims: ' + await page.evaluate(() => [...document.querySelectorAll('#dims .dim')].map(e => e.className.replace('dim ', '') + '=' + e.textContent).join(' | ')));
  // a circle: diameter, and its centre from the part's right edge
  await tool('circle'); await click([33, 12]); await click([35, 12]);
  await tool('c_dimension'); await click([35, 12]); await move([38, 17]); await click([38, 17]); await typeModify('5');
  log('circle: ' + JSON.stringify(await page.evaluate(() => window.app.sketcher.entities[1])) + ' · ' + await def());
  // redundant: the rect width again -> asks about a reference dimension
  await click([20, 16]); await move([20, 18.5]); await click([20, 18.5]);
  await page.waitForTimeout(300);
  log('asks: ' + (await page.textContent('#modal-body').catch(() => 'no modal')).slice(0, 70));
  await page.click('#modal-actions button.primary'); await page.waitForTimeout(300);
  log('reference: ' + await page.evaluate(() => [...document.querySelectorAll('#dims .dim.ref')].map(e => e.textContent).join(',')));
  await page.evaluate(() => window.app.viewer.fit && 0);
  await page.screenshot({ path: require('path').join(OUT, 'skdim_rect.png') });
  // drag a dimension label
  const lab = await page.$('#dims .dim.driving');
  const bb = await lab.boundingBox();
  await page.mouse.move(bb.x + bb.width / 2, bb.y + bb.height / 2); await page.mouse.down(); await page.mouse.move(bb.x + bb.width / 2 + 30, bb.y + bb.height / 2 + 40, { steps: 5 }); await page.mouse.up(); await page.waitForTimeout(200);
  log('dragged: ' + await page.evaluate(() => JSON.stringify(window.app.sketcher.constraints.filter(c => c.off != null).map(c => [c.type, +c.off.toFixed(2), +(c.slide || 0).toFixed(2)]))));
  // angle between two lines
  await tool('line'); await click([-9, -9]); await click([-1, -9]); await page.keyboard.press('Escape');
  await tool('line'); await click([-9, -9]); await click([-3, -3]); await page.keyboard.press('Escape');
  log('lines: ' + await page.evaluate(() => JSON.stringify(window.app.sketcher.entities.slice(2))));
  await tool('c_dimension'); await click([-3, -9]);
  log('p1: ' + await page.evaluate(() => JSON.stringify(window.app.sketcher.dimPend)));
  await click([-5, -5]);
  log('p2: ' + await page.evaluate(() => JSON.stringify(window.app.sketcher.dimPend)));
  await move([-4, -7.5]); await click([-4, -7.5]);
  log('p3: ' + await page.evaluate(() => JSON.stringify(window.app.sketcher.dimPend)) + (await page.$('#modal-body') ? await page.textContent('#modal-body') : ''));
  await typeModify('40');
  log('angle: ' + await page.evaluate(() => { const sk = window.app.sketcher; const c = sk.constraints.at(-1); return JSON.stringify([c.type, c.value, c.ra, c.rb, c.sign]); }));
  // parameter in a dimension
  await api('params/set', { params: [{ name: 'w', expr: '28' }] }); await page.evaluate(() => window.app.refresh());
  const wdim = await page.evaluate(() => window.app.sketcher.constraints.find(c => c.type === 'hdistance' && c.a.k === 'c0' && c.b.k === 'c1' && !c.ref).id);
  await page.evaluate(id => window.app.sketcher.openModify(id), wdim); await typeModify('w');
  log('expr dim: ' + await page.evaluate(id => { const sk = window.app.sketcher; const c = sk.constraints.find(x => x.id === id); return `${c.expr} ${c.value} rect w ${sk.entities[0].w}`; }, wdim));
  await page.screenshot({ path: require('path').join(OUT, 'skdim_all.png') });
  // use edge
  await tool('sk_useedge'); await click([40, 10]);
  log('use edge: ' + await page.evaluate(() => JSON.stringify(window.app.sketcher.entities.at(-1))) + ' · ' + await def());
  // finish, reopen
  await page.evaluate(() => window.app.finishSketch(false)); await page.waitForTimeout(800);
  let st = await page.evaluate(async js => { const { get } = await import(js); return get('state'); }, JS);
  const sf = st.features.at(-1);
  log(`saved: ${sf.type} refs ${sf.params.refs.length} constraints ${sf.params.constraints.length} error ${sf.error || 'none'}`);
  // change the parameter: the sketch follows
  await page.evaluate(async () => { const { post } = await import('/js/api.js'); await window.app.apply(await post('params/set', { params: [{ name: 'w', expr: '20' }] })); await window.app.resolveSketchParams(); });
  st = await page.evaluate(async js => { const { get } = await import(js); return get('state'); }, JS);
  log('after w=20: rect w ' + st.features.at(-1).params.entities[0].w);
  // grow the box: the edges move, the reopened sketch finds them again
  const boxF = st.features[0];
  await api('feature/edit', { id: boxF.id, params: { ...boxF.params, length: 50, width: 26 } });
  await page.evaluate(() => window.app.refresh());
  await page.evaluate(id => window.app.editFeature(id), st.features.at(-1).id); await page.waitForTimeout(1200);
  log('reopened: ' + await page.evaluate(() => { const sk = window.app.sketcher; return `missing ${sk.refEnts.filter(e => e.missing).length}, ${sk.defText()}, rect ${JSON.stringify(sk.entities[0])}`; }));
  await page.screenshot({ path: require('path').join(OUT, 'skdim_reopen.png') });
  log(errs.length ? 'ERRORS\n' + errs.join('\n') : 'no page errors');
  await browser.close();
})();
