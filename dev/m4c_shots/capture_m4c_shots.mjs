#!/usr/bin/env node
/**
 * M4c 1.x sketch screenshots via Playwright + tests/ui/serve.js.
 * Layout: dev/out/m4c-shots/1x/<theme>/<state>.png
 * 2.0 shots: `cargo test -p stl-app --test m4c_shots -- --ignored` → …/2x/…
 *
 * States (dark and light): plane_prompt, sketch_empty, draw_*, sketch_under/full/over,
 * modify_dialog, text_dialog, useedge_refs, missing_ref, inch_dims, finish_extrude.
 *
 * Usage (from repo root):
 *   node dev/m4c_shots/capture_m4c_shots.mjs
 */
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { access, mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from '../../tests/ui/node_modules/playwright/index.mjs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '../..');
const OUT = process.env.M4C_SHOTS_OUT
  ? path.resolve(process.env.M4C_SHOTS_OUT)
  : path.join(ROOT, 'dev/out/m4c-shots');
const SIZE = { width: 1280, height: 800 };
const WEB_URL = 'http://127.0.0.1:8790/index.html?web';

const STATES = [
  'plane_prompt',
  'sketch_empty',
  'draw_line',
  'draw_arc',
  'draw_rect',
  'draw_circle',
  'draw_polygon',
  'draw_slot',
  'sketch_under',
  'sketch_full',
  'sketch_over',
  'modify_dialog',
  'text_dialog',
  'useedge_refs',
  'missing_ref',
  'inch_dims',
  'finish_extrude',
];
const THEMES = ['dark', 'light'];

function log(msg) {
  console.log(msg);
}

async function exists(p) {
  try {
    await access(p);
    return true;
  } catch {
    return false;
  }
}

function startServe() {
  const child = spawn(process.execPath, ['tests/ui/serve.js'], {
    cwd: ROOT,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  return new Promise((resolve, reject) => {
    let settled = false;
    const done = (err, val) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      clearInterval(probe);
      if (err) reject(err);
      else resolve(val);
    };
    const timer = setTimeout(() => done(new Error('serve.js did not become ready')), 30000);
    const onData = (buf) => {
      process.stdout.write(buf);
      if (/Serving|8790/i.test(buf.toString())) done(null, child);
    };
    child.stdout.on('data', onData);
    child.stderr.on('data', onData);
    child.on('error', (e) => done(e));
    const probe = setInterval(async () => {
      try {
        const res = await fetch(WEB_URL);
        if (res.ok) done(null, child);
      } catch {
        /* wait */
      }
    }, 400);
  });
}

async function waitIdle(page) {
  await page.waitForFunction(
    () => {
      const a = window.app;
      if (!a || !a.viewer || !a.state) return false;
      const busy = document.getElementById('busy');
      if (busy && !busy.classList.contains('hidden')) return false;
      if (!document.getElementById('modal')?.classList.contains('hidden') && window.__allowModal !== true)
        return false;
      return a.state.bodies.every((b) => a.viewer.bodies.has(b.id)) && !a.viewer._raf;
    },
    null,
    { timeout: 120000 },
  );
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  await page.waitForTimeout(200);
}

async function setTheme1x(page, theme) {
  await page.evaluate((th) => {
    document.documentElement.dataset.theme = th;
    if (window.app && window.app.setTheme) window.app.setTheme(th);
  }, theme);
  await page.waitForTimeout(150);
}

async function newBox(page) {
  await page.evaluate(async () => {
    const { post } = await import('/js/api.js');
    await post('new', { force: true }, { quiet: true });
    await post(
      'feature/add',
      {
        type: 'primitive',
        params: {
          shape: 'box',
          length: 40,
          width: 20,
          height: 10,
          anchor: 'corner',
          plane: 'XY',
          operation: 'new',
          offset: [0, 0, 0],
          rotate: [0, 0, 0],
        },
      },
      { quiet: true },
    );
    await window.app.refresh();
    window.app.viewer.setView('iso');
    window.app.viewer.fit();
    const id = window.app.state.bodies[0]?.id;
    if (id) window.app.selection = [id];
  });
  await page.waitForFunction(() => window.app.state.bodies.length > 0, null, { timeout: 60000 });
  await waitIdle(page);
}

async function enterTopSketch(page) {
  await page.evaluate(async () => {
    const id = window.app.state.bodies[0].id;
    await window.app.enterSketch({
      plane: { origin: [0, 0, 10], normal: [0, 0, 1], xdir: [1, 0, 0], label: 'Top', body: id },
    });
  });
  await page.waitForFunction(() => window.app.sketcher?.active, null, { timeout: 30000 });
  await waitIdle(page);
}

async function screenUV(page, u, v) {
  return page.evaluate(
    ([u, v]) => {
      const sk = window.app.sketcher;
      const r = document.getElementById('view3d').getBoundingClientRect();
      const [x, y] = window.app.viewer.worldToScreen(sk.to3(u, v).toArray());
      return [x + r.left, y + r.top];
    },
    [u, v],
  );
}

async function clickUV(page, u, v) {
  const [x, y] = await screenUV(page, u, v);
  await page.mouse.move(x, y, { steps: 2 });
  await page.mouse.click(x, y);
  await page.waitForTimeout(80);
}

async function moveUV(page, u, v) {
  const [x, y] = await screenUV(page, u, v);
  await page.mouse.move(x, y, { steps: 4 });
  await page.waitForTimeout(60);
}

async function leaveSketch(page) {
  await page.evaluate(() => {
    if (window.app.sketcher?.active) window.app.cancelSketch?.() || window.app.sketcher.stop?.();
  });
  await page.keyboard.press('Escape').catch(() => {});
  await page.waitForTimeout(100);
}

/** Seed a rect (+ optional driving dims) for under / full / over / modify / inch. */
async function seedRectSketch(page, { dims = true } = {}) {
  await enterTopSketch(page);
  await page.evaluate(() => window.app.sketcher.setTool('rect'));
  await clickUV(page, 5, 5);
  await clickUV(page, 25, 15);
  if (!dims) return;
  // Inject dims directly: Smart Dimension clicks are flaky under inch mode / timing.
  await page.evaluate(() => {
    const sk = window.app.sketcher;
    const e = sk.entities[0];
    if (!e) return;
    sk.constraints.push(
      { id: 'dw', type: 'hdistance', a: { e: e.id, k: 'c0' }, b: { e: e.id, k: 'c1' }, value: 30, off: -4 },
      { id: 'dh', type: 'vdistance', a: { e: e.id, k: 'c1' }, b: { e: e.id, k: 'c2' }, value: 12, off: 4 },
    );
    sk.solveAll();
    sk.redraw();
    window.app.onDefinition?.();
    window.app.renderSketchPanel?.();
  });
  await page.waitForTimeout(100);
}

async function applyState1x(page, state) {
  await leaveSketch(page);
  await newBox(page);

  if (state === 'plane_prompt') {
    await page.evaluate(() => window.app.beginSketch());
    await page.waitForSelector('#tool-panel:not(.hidden)', { timeout: 15000 });
    await page.waitForFunction(() => document.getElementById('tool-title')?.textContent === 'Create Sketch');
    await waitIdle(page);
    return;
  }

  if (state === 'sketch_empty') {
    await enterTopSketch(page);
    await page.evaluate(() => window.app.viewer.setView('top'));
    await waitIdle(page);
    return;
  }

  if (state.startsWith('draw_')) {
    const tool = state.slice('draw_'.length);
    const key = tool === 'arc' ? 'arc3' : tool;
    await enterTopSketch(page);
    await page.evaluate((k) => window.app.sketcher.setTool(k), key);
    // First click starts the rubber band; move to a distinct second point per tool.
    const starts = {
      line: [0, 0],
      arc: [-8, 0],
      rect: [2, 2],
      circle: [10, 8],
      polygon: [-12, 6],
      slot: [-5, -8],
    };
    const ends = {
      line: [18, 6],
      arc: [8, 0],
      rect: [22, 14],
      circle: [16, 8],
      polygon: [-4, 6],
      slot: [12, -8],
    };
    await clickUV(page, ...(starts[tool] || [0, 0]));
    if (tool === 'arc') {
      await clickUV(page, 0, 8);
    }
    await moveUV(page, ...(ends[tool] || [15, 10]));
    await waitIdle(page);
    return;
  }

  if (state === 'sketch_under') {
    await seedRectSketch(page, { dims: false });
    await page.evaluate(() => window.app.viewer.setView('top'));
    await waitIdle(page);
    return;
  }

  if (state === 'sketch_full') {
    await seedRectSketch(page);
    // Pin the corner to the origin so dof drops.
    await page.evaluate(() => {
      const sk = window.app.sketcher;
      const e = sk.entities[0];
      sk.constraints.push({
        id: 'cf',
        type: 'coincident',
        a: { e: e.id, k: 'c0' },
        b: { e: '_o', k: 'p' },
      });
      sk.solveAll();
      sk.redraw();
      window.app.onDefinition?.();
      window.app.renderSketchPanel?.();
    });
    await page.evaluate(() => window.app.viewer.setView('top'));
    await waitIdle(page);
    return;
  }

  if (state === 'sketch_over') {
    await seedRectSketch(page);
    // Conflicting second width on the same corners (forces over-defined / red relations).
    await page.evaluate(() => {
      const sk = window.app.sketcher;
      const e = sk.entities[0];
      if (!e) return;
      sk.constraints.push({
        id: 'cover',
        type: 'hdistance',
        a: { e: e.id, k: 'c0' },
        b: { e: e.id, k: 'c1' },
        value: 18,
      });
      sk.solveAll();
      sk.redraw();
      window.app.onDefinition?.();
      window.app.renderSketchPanel?.();
    });
    await page.evaluate(() => window.app.viewer.setView('top'));
    await waitIdle(page);
    return;
  }

  if (state === 'modify_dialog') {
    await seedRectSketch(page);
    await page.evaluate(() => {
      const sk = window.app.sketcher;
      const c = sk.constraints.find((x) => x.type === 'hdistance' || x.type === 'length' || x.type === 'vdistance');
      if (c) sk.openModify(c.id);
    });
    await page.waitForSelector('.dim-modify input', { timeout: 10000 });
    await page.evaluate(() => {
      window.__allowModal = true;
    });
    await page.evaluate(() => window.app.viewer.setView('top'));
    await waitIdle(page);
    return;
  }

  if (state === 'text_dialog') {
    await enterTopSketch(page);
    await page.evaluate(() => {
      window.__allowModal = true;
      window.app.sketcher.setTool('text');
    });
    await clickUV(page, 5, 5);
    await page.waitForSelector('#modal:not(.hidden) #tx-text', { timeout: 15000 });
    await page.fill('#tx-text', 'Hi');
    await waitIdle(page);
    return;
  }

  if (state === 'useedge_refs') {
    await enterTopSketch(page);
    await page.evaluate(() => window.app.viewer.setView('top'));
    // Orange part-edge refs are already loaded; Use Edge adds one as a sketch entity.
    await page.evaluate(() => window.app.sketcher.setTool('sk_useedge'));
    await clickUV(page, 40, 10);
    await waitIdle(page);
    return;
  }

  if (state === 'missing_ref') {
    await seedRectSketch(page);
    // Dimension the rect's left side to a part edge, finish, grow the box, reopen.
    await page.evaluate(() => {
      const sk = window.app.sketcher;
      const e = sk.entities[0];
      const edge = sk.refEnts.find((r) => r.ref === 'edge' && r.type === 'line');
      if (e && edge) {
        sk.constraints.push({
          id: 'dref',
          type: 'hdistance',
          a: { e: e.id, k: 'c0' },
          b: { e: edge.id, k: 'p1' },
          value: 6,
          off: 8,
        });
        sk.solveAll();
        sk.redraw();
      }
    });
    await page.evaluate(async () => {
      await window.app.finishSketch(false);
    });
    await page.waitForFunction(() => !window.app.sketcher?.active, null, { timeout: 30000 });
    await page.evaluate(async () => {
      const { post } = await import('/js/api.js');
      const st = window.app.state;
      const boxF = st.features[0];
      await window.app.apply(
        await post('feature/edit', {
          id: boxF.id,
          params: { ...boxF.params, length: 55, width: 28 },
        }),
      );
      const sk = st.features.find((f) => f.type === 'sketch');
      if (sk) await window.app.editFeature(sk.id);
    });
    await page.waitForFunction(() => window.app.sketcher?.active, null, { timeout: 30000 });
    await page.evaluate(() => window.app.viewer.setView('top'));
    await waitIdle(page);
    return;
  }

  if (state === 'inch_dims') {
    await page.evaluate(() => window.app.setUnits('in'));
    await seedRectSketch(page);
    await page.evaluate(() => window.app.viewer.setView('top'));
    await waitIdle(page);
    return;
  }

  if (state === 'finish_extrude') {
    await seedRectSketch(page, { dims: false });
    await page.evaluate(() => window.app.onRibbon('finish_extrude'));
    await page.waitForSelector('#tool-panel:not(.hidden)', { timeout: 30000 });
    await page.waitForFunction(() => window.app.tool?.key === 'extrude' || window.app.tool?.def?.type === 'extrude', null, {
      timeout: 30000,
    });
    await page.evaluate(() => {
      if (window.app.tool) {
        window.app.tool.values.distance = 5;
        window.app.tool.values.operation = 'join';
        window.app.renderForm?.();
        window.app.schedulePreview?.(0);
      }
      window.app.viewer.setView('iso');
    });
    await page.waitForTimeout(800);
    await waitIdle(page);
    return;
  }

  throw new Error(`unknown state ${state}`);
}

function destPath(theme, state) {
  return path.join(OUT, '1x', theme, `${state}.png`);
}

async function capture1x() {
  log('starting 1.x serve.js …');
  const server = await startServe();
  const browser = await chromium.launch({
    headless: true,
    args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'],
  });
  try {
    const ctx = await browser.newContext({ viewport: SIZE });
    // Force mm on every new page before the app reads stle.settings. Shared browser
    // context otherwise keeps setUnits('in') from inch_dims into later states/themes.
    await ctx.addInitScript(() => {
      delete window.showSaveFilePicker;
      try {
        localStorage.setItem('stle.webWelcome', '1');
        const s = JSON.parse(localStorage.getItem('stle.settings') || '{}');
        s.units = 'mm';
        localStorage.setItem('stle.settings', JSON.stringify(s));
      } catch (_) {
        /* ignore */
      }
    });
    for (const theme of THEMES) {
      await mkdir(path.join(OUT, '1x', theme), { recursive: true });
      for (const state of STATES) {
        const dest = destPath(theme, state);
        if (process.env.M4C_SHOTS_RESUME && (await exists(dest))) {
          log(`skip existing ${dest}`);
          continue;
        }
        const page = await ctx.newPage();
        try {
          await page.goto(WEB_URL, { waitUntil: 'domcontentloaded', timeout: 120000 });
          await page.waitForFunction(() => !!window.app, null, { timeout: 120000 });
          await setTheme1x(page, theme);
          await applyState1x(page, state);
          await page.screenshot({
            path: dest,
            clip: { x: 0, y: 0, width: SIZE.width, height: SIZE.height },
          });
          log(`wrote ${dest}`);
        } catch (e) {
          console.error(`FAILED ${theme}/${state}: ${e.message || e}`);
          throw e;
        } finally {
          await page.close();
        }
      }
    }
  } finally {
    await browser.close();
    server.kill();
  }
}

async function statesDiffer(edition) {
  const bad = [];
  let checked = 0;
  for (const theme of THEMES) {
    const seen = new Map();
    for (const state of STATES) {
      const p = path.join(OUT, edition, theme, `${state}.png`);
      if (!(await exists(p))) continue;
      checked++;
      const h = createHash('sha256').update(await readFile(p)).digest('hex');
      if (seen.has(h)) bad.push(`${edition}/${theme}: ${seen.get(h)} and ${state} are identical`);
      else seen.set(h, state);
    }
  }
  if (bad.length) {
    for (const b of bad) console.error(`states_differ FAILED: ${b}`);
    return false;
  }
  log(`states_differ ok (${edition}, ${checked} images)`);
  return true;
}

function indexMd() {
  const rows = STATES.flatMap((state) =>
    THEMES.map((theme) => {
      const a = `1x/${theme}/${state}.png`;
      const b = `2x/${theme}/${state}.png`;
      return `| ${state} / ${theme} | [${a}](${a}) | [${b}](${b}) |`;
    }),
  );
  return `# M4c sketch screenshots

Window size: ${SIZE.width}×${SIZE.height}. 1.x from \`node dev/m4c_shots/capture_m4c_shots.mjs\`; 2.0 from \`cargo test -p stl-app --test m4c_shots -- --ignored\`.

| State / theme | 1.x | 2.0 |
| --- | --- | --- |
${rows.join('\n')}
`;
}

async function main() {
  await mkdir(OUT, { recursive: true });
  await capture1x();
  const md = indexMd();
  await writeFile(path.join(OUT, 'index.md'), md);
  log(`index: ${path.join(OUT, 'index.md')}`);
  const ok = await statesDiffer('1x');
  if (!ok) process.exit(1);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
