#!/usr/bin/env node
/**
 * M4b 1.x screenshots via Playwright + tests/ui/serve.js.
 * Layout: dev/out/m4b-shots/1x/<theme>/<state>.png
 * 2.0 shots: `cargo test -p stl-app --test m4b_shots -- --ignored` → …/2x/…
 *
 * States (dark and light): every TOOLS panel open on defaults, Move gizmo move/rotate,
 * Sculpt with brush, Support Fins Rotate/Add/Remove (coverage on), Draw Walls line + curve.
 *
 * Usage (from repo root):
 *   node dev/m4b_shots/capture_m4b_shots.mjs
 */
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { access, mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { chromium } from '../../tests/ui/node_modules/playwright/index.mjs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '../..');
const OUT = process.env.M4B_SHOTS_OUT
  ? path.resolve(process.env.M4B_SHOTS_OUT)
  : path.join(ROOT, 'dev/out/m4b-shots');
const SIZE = { width: 1280, height: 800 };
const PROJECT = path.join(ROOT, 'rust/tests/projects/basic.stlx');
const FIX_TIPPED = path.join(ROOT, 'tests/ui/fixtures/tipped.stl');
const FIX_ELL = path.join(ROOT, 'tests/ui/fixtures/ell.stl');
const WEB_URL = 'http://127.0.0.1:8790/index.html?web';

const toolsUrl = pathToFileURL(path.join(ROOT, 'web', 'js', 'tools.js')).href;
const { TOOLS } = await import(toolsUrl);

const TOOL_STATES = Object.keys(TOOLS).map((k) => `tool_${k}`);
const EXTRA_STATES = [
  'gizmo_move',
  'gizmo_rotate',
  'sculpt_brush',
  'sf_rotate',
  'sf_add',
  'sf_remove',
  'walls_line',
  'walls_curve',
];
const STATES = [...TOOL_STATES, ...EXTRA_STATES];
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

/** Waits until 1.x has finished drawing: no busy overlay, bodies uploaded, viewer idle. */
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
  await page.waitForTimeout(250);
}

async function setTheme1x(page, theme) {
  await page.evaluate((th) => {
    document.documentElement.dataset.theme = th;
    if (window.app && window.app.setTheme) window.app.setTheme(th);
  }, theme);
  await page.waitForTimeout(200);
  const got = await page.evaluate(() => document.documentElement.dataset.theme);
  if (got !== theme) throw new Error(`theme ${theme} not applied (got ${got})`);
}

async function openProject1x(page) {
  const fs = await import('node:fs/promises');
  const data = await fs.readFile(PROJECT);
  const b64 = data.toString('base64');
  await page.evaluate((b64) => {
    const bin = atob(b64);
    const bytes = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    const file = new File([bytes], 'basic.stlx', { type: 'application/zip' });
    window.showOpenFilePicker = async () => [
      {
        name: file.name,
        getFile: async () => file,
        queryPermission: async () => 'granted',
        requestPermission: async () => 'granted',
      },
    ];
  }, b64);
  await page.evaluate(async () => {
    await window.app.cmd('open', { force: true });
  });
  await page.waitForFunction(() => window.app.state.bodies.length > 0, null, { timeout: 120000 });
  await waitIdle(page);
  await page.evaluate(() => {
    if (window.app.viewer?.setView) window.app.viewer.setView('iso');
    const id = window.app.state.bodies[0]?.id;
    if (id) window.app.selection = [id];
  });
  await page.waitForTimeout(200);
}

async function importStl(page, filePath, name) {
  const fs = await import('node:fs/promises');
  const b64 = (await fs.readFile(filePath)).toString('base64');
  await page.evaluate(
    async ([b64, name]) => {
      const bin = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
      const { upload } = await import('/js/api.js');
      const { post } = await import('/js/api.js');
      await post('new', { force: true }, { quiet: true });
      await window.app.refresh();
      const r = await upload(new File([bin], name), { place: 'keep' });
      await window.app.apply(r);
      window.app.viewer.setView('iso');
      window.app.viewer.fit();
      const id = window.app.state.bodies[0]?.id;
      if (id) window.app.selection = [id];
    },
    [b64, name],
  );
  await page.waitForFunction(() => window.app.state.bodies.length > 0, null, { timeout: 120000 });
  await waitIdle(page);
}

async function closeTool(page) {
  await page.evaluate(() => {
    if (window.app.tool) {
      try {
        window.app.cancelTool?.() || window.app.closeTool?.();
      } catch (_) {
        /* fall through */
      }
    }
  });
  await page.keyboard.press('Escape').catch(() => {});
  await page.waitForFunction(() => !window.app.tool, null, { timeout: 15000 }).catch(() => {});
  await page.waitForTimeout(100);
}

async function openToolPanel(page, key) {
  await closeTool(page);
  await page.evaluate(async (k) => {
    if (k === 'supportfins' || k === 'supportwall') {
      await window.app.openSupportTool(k);
    } else {
      window.app.openTool(k);
    }
  }, key);
  await page.waitForSelector('#tool-panel:not(.hidden)', { timeout: 60000 });
  // Support Fins may show "Checking…" / "Building…" — wait until the form is idle enough
  await page
    .waitForFunction(
      () => {
        const msg = document.querySelector('#tool-msg')?.textContent || '';
        if (/Building|Checking/i.test(msg)) return false;
        return !!window.app.tool;
      },
      null,
      { timeout: 90000 },
    )
    .catch(() => {});
  await waitIdle(page);
}

async function applyState1x(page, state) {
  if (state.startsWith('tool_')) {
    const key = state.slice('tool_'.length);
    await openProject1x(page);
    await openToolPanel(page, key);
    return;
  }
  if (state === 'gizmo_move') {
    await openProject1x(page);
    await openToolPanel(page, 'move');
    // Differ from tool_move: front view + non-zero Move XYZ so the panel and camera differ.
    await page.evaluate(() => {
      if (!window.app.tool) return;
      window.app.tool.values._gizmo = 'move';
      window.app.tool.values.translate = [12, 0, 0];
      window.app.renderForm?.();
      window.app.gizmo?.setMode?.('move');
      window.app.schedulePreview?.(0);
      window.app.viewer?.setView?.('front');
    });
    await page.waitForTimeout(600);
    await waitIdle(page);
    return;
  }
  if (state === 'gizmo_rotate') {
    await openProject1x(page);
    await page.evaluate(() => {
      window.app.openTool('move', { values: { _gizmo: 'rotate', rotate: [0, 25, 0] } });
    });
    await page.waitForSelector('#tool-panel:not(.hidden)', { timeout: 30000 });
    await page.evaluate(() => {
      if (!window.app.tool) return;
      window.app.tool.values._gizmo = 'rotate';
      window.app.tool.values.rotate = [0, 25, 0];
      window.app.renderForm?.();
      window.app.gizmo?.setMode?.('rotate');
      window.app.schedulePreview?.(0);
      window.app.viewer?.setView?.('front');
    });
    await page.waitForTimeout(600);
    await waitIdle(page);
    return;
  }
  if (state === 'sculpt_brush') {
    await openProject1x(page);
    await openToolPanel(page, 'sculpt');
    await page.evaluate(() => {
      const b = window.app.state.bodies[0];
      if (!b || !window.app.tool) return;
      window.app.tool.values.body = b.id;
      // place the brush ring over the part centre (viewer.setBrush wants {point,normal,radius})
      const point = [
        (b.bbox[0][0] + b.bbox[1][0]) / 2,
        (b.bbox[0][1] + b.bbox[1][1]) / 2,
        b.bbox[1][2],
      ];
      const radius = window.app.tool.values.radius || 5;
      window.app.viewer?.setBrush?.({ point, normal: [0, 0, 1], radius });
    });
    await waitIdle(page);
    return;
  }
  if (state === 'sf_rotate' || state === 'sf_add' || state === 'sf_remove') {
    await importStl(page, FIX_TIPPED, 'tipped.stl');
    const mode = state.slice('sf_'.length);
    await openToolPanel(page, 'supportfins');
    await page.evaluate((mode) => {
      if (!window.app.tool) return;
      window.app.tool.values._mode = mode;
      window.app.renderForm?.();
      window.app.onFormChange?.({ target: { dataset: { key: '_mode' } } });
      // force a coverage refresh the way changing mode does
      window.app.schedulePreview?.(0);
    }, mode);
    await page
      .waitForFunction(
        () => !/Checking|Building/i.test(document.querySelector('#tool-msg')?.textContent || ''),
        null,
        { timeout: 90000 },
      )
      .catch(() => {});
    await waitIdle(page);
    return;
  }
  if (state === 'walls_line' || state === 'walls_curve') {
    await importStl(page, FIX_ELL, 'ell.stl');
    await openToolPanel(page, 'supportwall');
    const shape = state === 'walls_line' ? 'line' : 'curve';
    await page.evaluate((shape) => {
      if (!window.app.tool) return;
      const b = window.app.state.bodies[0];
      const bb = b?.bbox || [
        [0, 0, 0],
        [30, 20, 10],
      ];
      const z = bb[0][2];
      const walls =
        shape === 'line'
          ? [
              {
                shape: 'line',
                points: [
                  [bb[0][0] + 2, bb[0][1] + 2, z],
                  [bb[1][0] - 2, bb[0][1] + 2, z],
                ],
              },
            ]
          : [
              {
                shape: 'curve',
                points: [
                  [bb[0][0] + 4, bb[0][1] + 2, z],
                  [bb[0][0] + 4, bb[1][1] - 2, z],
                  [(bb[0][0] + bb[1][0]) / 2, (bb[0][1] + bb[1][1]) / 2, z],
                ],
              },
            ];
      window.app.tool.values.walls = walls;
      window.app.renderForm?.();
      window.app.schedulePreview?.(0);
    }, shape);
    await page
      .waitForFunction(
        () => !/Building/i.test(document.querySelector('#tool-msg')?.textContent || ''),
        null,
        { timeout: 90000 },
      )
      .catch(() => {});
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
    await ctx.addInitScript(() => {
      delete window.showSaveFilePicker;
      try {
        localStorage.setItem('stle.webWelcome', '1');
      } catch (_) {
        /* ignore */
      }
    });
    for (const theme of THEMES) {
      await mkdir(path.join(OUT, '1x', theme), { recursive: true });
      for (const state of STATES) {
        const dest = destPath(theme, state);
        if (process.env.M4B_SHOTS_RESUME && (await exists(dest))) {
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

/** states_differ: within one theme and edition, no two states may give identical images. */
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
  return `# M4b tools / Support Fins screenshots

Window size: ${SIZE.width}×${SIZE.height}. 1.x from \`node dev/m4b_shots/capture_m4b_shots.mjs\`; 2.0 from \`cargo test -p stl-app --test m4b_shots -- --ignored\`.

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
