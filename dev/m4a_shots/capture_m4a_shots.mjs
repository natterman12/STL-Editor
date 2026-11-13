#!/usr/bin/env node
/**
 * M4a 1.x screenshots via Playwright + tests/ui/serve.js.
 * 2.0 shots: `cargo test -p stl-app --test m4a_shots -- --ignored`
 *
 * Usage (from repo root):
 *   node dev/m4a_shots/capture_m4a_shots.mjs
 */
import { spawn } from 'node:child_process';
import { access, mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from '../../tests/ui/node_modules/playwright/index.mjs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '../..');
const OUT = path.join(ROOT, 'dev/out/m4a-shots');
const SIZE = { width: 1280, height: 800 };
const PROJECT = path.join(ROOT, 'rust/tests/projects/basic.stlx');
const WEB_URL = 'http://127.0.0.1:8790/index.html?web';

const STATES = ['start', 'project', 'rename', 'timeline', 'parameters', 'section'];
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
  await page.waitForFunction(() => window.app && !window.app.busy, null, { timeout: 120000 });
  await page.waitForTimeout(350);
}

async function setTheme1x(page, theme) {
  await page.evaluate((th) => {
    document.documentElement.dataset.theme = th;
    if (window.app && window.app.setTheme) window.app.setTheme(th);
  }, theme);
  await page.waitForTimeout(200);
}

async function openProject1x(page) {
  // Polyfill showOpenFilePicker with our fixture File (pickToOpen prefers that API).
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
  await waitIdle(page);
  await page.evaluate(() => {
    if (window.app.viewer && window.app.viewer.setView) window.app.viewer.setView('iso');
  });
  await page.waitForTimeout(300);
}

async function applyState1x(page, state) {
  if (state === 'start') {
    await page.evaluate(async () => {
      const { post } = await import('/js/api.js');
      try {
        await post('new', { force: true }, { quiet: true });
      } catch (_) {
        /* empty start page is fine */
      }
    });
    await page.waitForTimeout(400);
    return;
  }
  await openProject1x(page);
  if (state === 'project') return;
  if (state === 'rename') {
    await page.evaluate(() => {
      const row = document.querySelector('#tree .nm, #browser .nm, .tree .body .nm');
      if (row) row.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
    });
    await page.waitForTimeout(400);
  } else if (state === 'timeline') {
    await page.evaluate(async () => {
      const { post } = await import('/js/api.js');
      await post('rollback', { index: 1 }, { quiet: true });
    });
    await waitIdle(page);
  } else if (state === 'parameters') {
    await page.evaluate(() => window.app.cmd('params'));
    await page.waitForTimeout(400);
  } else if (state === 'section') {
    await page.evaluate(() => window.app.cmd('section'));
    await page.waitForTimeout(500);
  }
}

function indexMd() {
  const rows = STATES.flatMap((state) =>
    THEMES.map((theme) => {
      const a = `${state}_${theme}_1x.png`;
      const b = `${state}_${theme}_2x.png`;
      return `| ${state} / ${theme} | [${a}](${a}) | [${b}](${b}) |`;
    }),
  );
  return `# M4a shell/viewport screenshots

Window size: ${SIZE.width}×${SIZE.height}. 1.x from \`node dev/m4a_shots/capture_m4a_shots.mjs\`; 2.0 from \`cargo test -p stl-app --test m4a_shots -- --ignored\`.

| State / theme | 1.x | 2.0 |
| --- | --- | --- |
${rows.join('\n')}
`;
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
    });
    for (const theme of THEMES) {
      for (const state of STATES) {
        const page = await ctx.newPage();
        await page.goto(WEB_URL, { waitUntil: 'domcontentloaded', timeout: 120000 });
        await page.waitForFunction(() => !!window.app, null, { timeout: 120000 });
        await setTheme1x(page, theme);
        await applyState1x(page, state);
        const dest = path.join(OUT, `${state}_${theme}_1x.png`);
        await page.screenshot({
          path: dest,
          clip: { x: 0, y: 0, width: SIZE.width, height: SIZE.height },
        });
        log(`wrote ${dest}`);
        await page.close();
      }
    }
  } finally {
    await browser.close();
    server.kill();
  }
}

async function main() {
  await mkdir(OUT, { recursive: true });
  await capture1x();
  const md = indexMd();
  await writeFile(path.join(OUT, 'index.md'), md);
  await writeFile(path.join(ROOT, 'dev/m4a_shots/index.md'), md);
  const missing2x = [];
  for (const state of STATES) {
    for (const theme of THEMES) {
      const p = path.join(OUT, `${state}_${theme}_2x.png`);
      if (!(await exists(p))) missing2x.push(path.basename(p));
    }
  }
  if (missing2x.length) {
    log(`note: ${missing2x.length} 2.0 shots not present yet (run cargo test -p stl-app --test m4a_shots -- --ignored)`);
  }
  log(`index: ${path.join(OUT, 'index.md')}`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
