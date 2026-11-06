// web edition: save in place, recent files, autosave + recovery (Chromium; pickers backed by OPFS)
const { FIX, OUT, WEB, DESKTOP } = require('./common');
const { chromium } = require('playwright');
const fs = require('fs');
const BASE = process.env.BASE || WEB;
const JS = process.env.JS || '/js/api.js';
const SP = FIX;
const log = m => console.log(m);
(async () => {
  // a normal (persistent) profile: Chromium's throwaway profiles crash reading file handles back from IndexedDB
  fs.rmSync(require('path').join(OUT, 'profile-web4'), { recursive: true, force: true });
  const ctx = await chromium.launchPersistentContext(require('path').join(OUT, 'profile-web4'), { channel: 'chromium', viewport: { width: 1400, height: 850 }, acceptDownloads: true, args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
  const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
  await ctx.addInitScript(() => {
    localStorage.setItem('stle.settings', JSON.stringify({ autosave_interval: 15 }));
    localStorage.setItem('stle.webWelcome', '1');
    if (window.__noFS) return;
    window.__pickerCalls = 0;
    const dir = () => navigator.storage.getDirectory();
    window.showSaveFilePicker = async ({ suggestedName }) => { window.__pickerCalls++; return (await dir()).getFileHandle('saved-' + suggestedName, { create: true }); };
    window.showOpenFilePicker = async () => { window.__pickerCalls++; return [await (await dir()).getFileHandle(window.__openName)]; };
  });
  const errs = [];
  const newPage = async () => {
    const page = await ctx.newPage();
    page.on('crash', () => console.log('PAGE CRASHED')); page.on('close', () => console.log('page closed')); page.on('pageerror', e => errs.push('pageerror ' + e.message)); page.on('console', m => { if (m.type() === 'error') errs.push('console ' + m.text()); });
    await page.goto(BASE); await page.waitForTimeout(1500);
    return page;
  };
  for (const p of ctx.pages()) await p.close().catch(() => {});
  let page = await newPage();
  const api = (pg, route, data) => pg.evaluate(async ([r, d, js]) => { const { post } = await import(js); const x = await post(r, d, { quiet: true }); return JSON.parse(JSON.stringify(x, (k, v) => ArrayBuffer.isView(v) ? `[${v.length}]` : v)); }, [route, data, JS]);
  const importBlk = async pg => { const fc = pg.waitForEvent('filechooser'); pg.evaluate(() => window.app.cmd('import')); await (await fc).setFiles(`${SP}/blk.stl`); await pg.waitForTimeout(1200); };
  await importBlk(page);
  let st = await api(page, 'state');
  const B = st.bodies[0].id;
  await api(page, 'feature/add', { type: 'hole', params: { body: B, point: [0, 0, 30], normal: [0, 0, 1], diameter: 5, depth: 5 } }).catch(e => log('hole ' + e.message));
  // first save asks where, the second goes straight back to the same file
  await page.evaluate(() => window.app.cmd('save')); await page.waitForTimeout(800);
  st = await api(page, 'state');
  log(`saved as ${st.doc.name} dirty=${st.doc.dirty} pickers=${await page.evaluate(() => window.__pickerCalls)}`);
  await api(page, 'feature/add', { type: 'primitive', params: { shape: 'box', size: [5, 5, 5] } }).catch(() => {});
  await page.evaluate(() => window.app.cmd('save')); await page.waitForTimeout(800);
  const size = await page.evaluate(async n => (await (await (await navigator.storage.getDirectory()).getFileHandle(n)).getFile()).size, st.doc.name);
  st = await api(page, 'state');
  log(`second save: pickers=${await page.evaluate(() => window.__pickerCalls)} file ${size} bytes, ${st.features.length} steps, recent=${JSON.stringify(st.recent.map(p => p.split('/').pop()))}`);
  // new, then reopen from the recent list in the File menu
  await page.evaluate(() => window.app.cmd('new')); await page.waitForTimeout(500);
  await page.click('[data-menu="file"]'); await page.waitForTimeout(200);
  const recentItem = page.locator('#menu .mi', { hasText: st.doc.name }).first();
  log('recent in menu: ' + await recentItem.count());
  await recentItem.click(); await page.waitForTimeout(1500);
  st = await api(page, 'state');
  log(`reopened ${st.doc.name}: ${st.features.length} steps, dirty=${st.doc.dirty}`);
  // reload the tab: recent list survives
  await page.reload(); await page.waitForTimeout(1500);
  st = await api(page, 'state');
  log('after reload recent=' + st.recent.length);
  // autosave: change something, wait for the 15 s autosave
  await api(page, 'open', { path: st.recent[0], force: true });
  await api(page, 'feature/add', { type: 'primitive', params: { shape: 'cylinder', radius: 3, height: 9 } }).catch(() => {});
  st = await api(page, 'state'); const steps = st.features.length;
  log('waiting for autosave…'); await page.waitForTimeout(17000);
  st = await api(page, 'state'); log('status autosaved=' + !!st.doc.autosaved);
  // a second tab while the first is still open: nothing to recover
  const p2 = await newPage();
  log('recoverable while first tab open: ' + (await api(p2, 'recovery/list')).items.length);
  await page.close();   // like a crash: the tab is gone with unsaved work
  await p2.waitForTimeout(500);
  const items = (await api(p2, 'recovery/list')).items;
  log('recoverable after it closed: ' + JSON.stringify(items.map(i => [i.title, i.steps])));
  const r = await api(p2, 'recovery/open', { id: items[0].id });
  log(`recovered: ${r.state.features.length} steps (expected ${steps}), dirty=${r.state.doc.dirty}, name=${r.state.doc.name}`);
  // the recovered project still saves back to its file without asking
  const before = await p2.evaluate(() => window.__pickerCalls);
  await p2.evaluate(() => window.app.cmd('save')); await p2.waitForTimeout(800);
  log('save after recovery asked: ' + ((await p2.evaluate(() => window.__pickerCalls)) - before) + ' times; left to recover: ' + (await api(p2, 'recovery/list')).items.length);
  // browsers without file pickers: download + a copy for the recent list
  const ctx2 = await browser.newContext({ acceptDownloads: true });
  await ctx2.addInitScript(() => { window.__noFS = true; delete window.showSaveFilePicker; delete window.showOpenFilePicker; localStorage.setItem('stle.webWelcome', '1'); });
  const p3 = await ctx2.newPage(); p3.on('pageerror', e => errs.push('p3 ' + e.message));
  await p3.goto(BASE); await p3.waitForTimeout(1500);
  await importBlk(p3);
  const [dl] = await Promise.all([p3.waitForEvent('download'), p3.evaluate(() => window.app.cmd('save'))]);
  await p3.waitForTimeout(500);
  st = await api(p3, 'state');
  log(`no-picker save: download ${dl.suggestedFilename()}, recent=${st.recent.length}`);
  await api(p3, 'new', { force: true });
  st = await api(p3, 'state');
  const r3 = await api(p3, 'open', { path: st.recent[0] });
  log(`no-picker reopen from recent copy: ${r3.state.features.length} steps`);
  console.log(errs.join('\n') || 'no errors');
  await ctx.close(); await browser.close();
})();
