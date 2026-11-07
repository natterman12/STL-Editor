// The Support Fins panel on a plain box: the mode buttons, the Held card (no error, "Nothing to
// hold up"), folding sections that stay open or closed after the tool is closed and opened
// again, "N changed" on a closed section, the credit in the footer, and choices that fit on one
// line (Hole Type). SHOTS=<folder> also saves screenshots there (dark and light).
const { OUT, WEB } = require('./common');
const { chromium } = require('playwright');
const path = require('path');
const BASE = process.env.BASE || WEB;
const JS = process.env.JS || '/js/api.js';
const SHOTS = process.env.SHOTS || '';
const log = m => console.log(m);
let failed = 0;
const check = (ok, what) => { log(`${ok ? 'ok  ' : 'FAIL'} ${what}`); if (!ok) failed++; };
(async () => {
  const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 860 } });
  await ctx.addInitScript(() => { try { localStorage.setItem('stle.webWelcome', '1'); } catch {} });
  const page = await ctx.newPage();
  const errs = []; page.on('pageerror', e => errs.push('pageerror ' + e.message)); page.on('console', m => { if (m.type() === 'error' && !/400 \(Bad Request\)/.test(m.text())) errs.push('console ' + m.text()); });
  await page.goto(BASE); await page.waitForTimeout(1500);
  await page.evaluate(async js => { const { post } = await import(js); await post('new', { force: true }, { quiet: true }); await window.app.refresh(); }, JS);
  await page.evaluate(() => { try { localStorage.removeItem('stle_sections'); } catch {} window.app._sections = null; });
  // Start with a box
  await page.click('#empty-box'); await page.waitForSelector('#tool-panel:not(.hidden)'); await page.waitForTimeout(600);
  await page.click('#tool-ok'); await page.waitForFunction(() => !window.app.tool && window.app.state.bodies.length === 1, null, { timeout: 30000 });
  const openSF = async () => {
    await page.click('[data-tab="supports"]'); await page.click('[data-tool="supportfins"]'); await page.waitForSelector('#tool-panel:not(.hidden)');
    await page.waitForFunction(() => !/Checking/.test(document.querySelector('#tool-form [data-key="_cover"] .val')?.textContent || 'Checking'), null, { timeout: 30000 });
  };
  const closeTool = async () => { await page.click('#tool-cancel'); await page.waitForFunction(() => !window.app.tool, null, { timeout: 30000 }); };
  await openSF();
  // the mode: three buttons, no label beside them
  const modes = await page.$$eval('#tool-form [data-key="_mode"] [data-seg="_mode"]', els => els.map(e => e.textContent));
  check(modes.join('|') === 'Rotate|Add fins|Remove fins', `mode buttons: ${modes.join(' | ')}`);
  check(!(await page.$('#tool-form [data-key="_mode"] > label')), 'mode row has no label');
  // the Held card: no error, and a box has nothing to hold up
  const held = (await page.textContent('#tool-form [data-key="_cover"] .val')).trim();
  check(!/undefined|Cannot|buffer|error/i.test(held), `Held shows no error: "${held}"`);
  check(/Nothing to hold up/.test(held), 'a box has nothing to hold up');
  const plastic = (await page.textContent('#tool-form [data-key="_plastic"]')).replace(/\s+/g, ' ').trim();
  log('plastic card: ' + plastic);
  // the credit sits in the footer, not in the hint
  const credit = (await page.textContent('#tool-credit')).trim(), hint = (await page.textContent('#hint')).trim();
  check(/^Support Fins \S+ · gittrahan \(MIT\)$/.test(credit), `credit in the footer: "${credit}"`);
  check(!/gittrahan/.test(hint), 'the hint has no credit');
  // folding sections: the first two of Support Fins' sections are open, the rest and Output closed
  const hdrs = () => page.$$eval('#tool-form .field-hdr', els => els.map(e => ({ key: e.dataset.fold, open: e.classList.contains('open'), text: e.textContent })));
  let H = await hdrs();
  log('sections: ' + H.map(h => `${h.text}${h.open ? ' (open)' : ''}`).join(' · '));
  check(H.length >= 3 && H[0].open && H[1].open && H.slice(2).every(h => !h.open), 'first two sections open, the rest closed');
  check(H.at(-1).key === '_hdr_output' && /Output/.test(H.at(-1).text), 'Output is the last section');
  const fold = H[2].key, nFields = await page.locator('#tool-form .field').count();
  await page.click(`[data-fold="${fold}"]`); await page.waitForTimeout(150);
  const nOpen = await page.locator('#tool-form .field').count();
  check(nOpen > nFields && await page.getAttribute(`[data-fold="${fold}"]`, 'aria-expanded') === 'true', `${fold} opens (${nFields} -> ${nOpen} rows)`);
  // a change inside, then folded: the header says so
  const inside = await page.evaluate(k => { const h = document.querySelector(`[data-fold="${k}"]`); let e = h.nextElementSibling; while (e && !e.classList.contains('field-hdr')) { const i = e.querySelector('input[data-num]'); if (i) return i.dataset.num; e = e.nextElementSibling; } return null; }, fold);
  if (inside) {
    const v0 = await page.inputValue(`[data-num="${inside}"]`);
    await page.fill(`[data-num="${inside}"]`, String((parseFloat(v0) || 0) + 1)); await page.press(`[data-num="${inside}"]`, 'Tab'); await page.waitForTimeout(200);
  }
  await page.click(`[data-fold="${fold}"]`); await page.waitForTimeout(150);
  const chg = (await page.textContent(`[data-fold="${fold}"]`)).trim();
  check(!inside || /1 changed/.test(chg), `closed section shows the change: "${chg}" (${inside})`);
  // open it again, close the tool and open it again: still open
  await page.click(`[data-fold="${fold}"]`); await page.waitForTimeout(150);
  await closeTool(); await openSF();
  check(await page.getAttribute(`[data-fold="${fold}"]`, 'aria-expanded') === 'true', 'still open after closing and opening the tool');
  await page.click(`[data-fold="${fold}"]`); await page.waitForTimeout(150);
  await closeTool(); await openSF();
  check(await page.getAttribute(`[data-fold="${fold}"]`, 'aria-expanded') === 'false', 'still closed after closing and opening the tool');
  check(/"supportfins:_hdr_\w+":/.test(await page.evaluate(() => localStorage.getItem('stle_sections') || '')), 'kept in localStorage (stle_sections)');
  // Output: Add as, Material and Bare beyond
  await page.click('[data-fold="_hdr_output"]'); await page.waitForTimeout(150);
  check(!!(await page.$('#tool-form [data-key="output"]')) && !!(await page.$('#tool-form [data-sfmat]')) && !!(await page.$('#tool-form [data-sfreach]')), 'Output has Add as, Material and Bare beyond');
  await page.click('[data-fold="_hdr_output"]'); await page.waitForTimeout(150);
  // no choice wraps or overflows in the panel
  const segBad = () => page.$$eval('#tool-form .seg', segs => segs.filter(s => { const b = [...s.querySelectorAll('button')]; return b.some(x => x.offsetTop !== b[0].offsetTop || x.scrollWidth > x.clientWidth + 1); }).map(s => s.closest('[data-key]')?.dataset.key));
  let bad = await segBad();
  check(!bad.length, `Support Fins choices fit on one line${bad.length ? ': ' + bad.join(', ') : ''}`);
  if (SHOTS) {
    await page.evaluate(() => { document.querySelector('#tool-form').scrollTop = 0; }); await page.mouse.move(700, 500);
    for (const th of ['dark', 'light']) {
      await page.evaluate(t => window.app.setTheme(t), th); await page.waitForTimeout(400);
      await page.screenshot({ path: path.join(SHOTS, `sf_box_rotate_${th}.png`) });
    }
    await page.evaluate(() => window.app.setTheme('dark')); await page.waitForTimeout(200);
  }
  await closeTool();
  // Hole: Type fits on one line, or is a list
  await page.evaluate(() => window.app.openTool('hole')); await page.waitForSelector('#tool-panel:not(.hidden)'); await page.waitForTimeout(400);
  const typeSeg = await page.$('#tool-form [data-seg="kind"]'), typeSel = await page.$('#tool-form select[data-sel="kind"]');
  bad = await segBad();
  check((typeSeg || typeSel) && !bad.length, `Hole Type: ${typeSeg ? 'buttons on one line' : 'a list'}${bad.length ? '; overflowing: ' + bad.join(', ') : ''}`);
  check(!(await page.textContent('#tool-credit')).trim(), 'no Support Fins credit on other tools');
  if (SHOTS) for (const th of ['dark', 'light']) {
    await page.evaluate(t => window.app.setTheme(t), th); await page.waitForTimeout(400);
    await page.screenshot({ path: path.join(SHOTS, `sf_hole_${th}.png`) });
  }
  await page.screenshot({ path: path.join(OUT, 'sfpanel_hole.png') });
  log(errs.length ? 'ERRORS\n' + errs.join('\n') : 'no page errors');
  if (failed) process.exitCode = 1;
  await browser.close();
})();
