// Runs the UI tests (Playwright) against the web edition and the desktop engine.
//
//   node run.js              every test
//   node run.js web          only the web edition's tests (or: desktop)
//   node run.js skdim sfcov  just these (names from the list below)
//
// It starts what's needed and stops it again: the web folder on :8790, the docs folder on
// :8791 (the published copy, for the offline/update test), both with serve.js, and the
// desktop engine on :8765 (python main.py --server; set PYTHON to pick the Python that has
// the app's packages). A server that's already running is used as it is.
// A test fails when it exits non-zero, times out, or prints page errors ("ERRORS",
// "pageerror"). Logs and screenshots go to out/.
const { spawn } = require('child_process');
const path = require('path');
const fs = require('fs');
const http = require('http');
const { ROOT, OUT, DESKTOP } = require('./common');
const { serve } = require('./serve');

const PY = process.env.PYTHON || (process.platform === 'win32' ? 'python' : 'python3');

// [name, file, group, what it covers, extra env]
const TESTS = [
  ['ribbon', 'ribbon.js', 'web', 'Toolbar tabs: each tool once, Inspect pinned, narrow window folds into »'],
  ['skdim', 'skdim.js', 'web', 'Smart Dimension, part edges, reference dims, Use Edge, parameters, edges found again'],
  ['skdim2', 'skdim2.js', 'web', 'Typed sizes become dimensions, double-click to edit, silhouette edges, dark theme'],
  ['sfcov', 'sfcov.js', 'web', 'Support Fins coverage: bare spots, Full coverage, Bare beyond'],
  ['sf9', 'sf9.js', 'web', 'Support Fins modes: Rotate, Add fins, Remove fins; Move applies on release'],
  ['sf11', 'sf11.js', 'web', 'Suggest orientations, strength, support plastic, gussets, anchors'],
  ['web7', 'web7.js', 'web', 'Support Fins panel from the engine schema, preview, edit, Draw Wall'],
  ['web8', 'web8.js', 'web', 'Instant red overhangs, fins-only preview, Draw Walls lines / multi-line / curve'],
  ['params', 'params.js', 'web', 'Parameters dialog, expressions in fields, undo, removing a used one'],
  ['gizmo', 'gizmo.js', 'web', 'Move / Rotate / Scale handles, land on bed'],
  ['v110', 'v110.js', 'web', 'Supports tab, Export / Send to Slicer menus, supports edited not stacked'],
  ['kofi', 'kofi.js', 'web', 'Ko-fi links: start screen, Help menu, About'],
  ['web2', 'web2.js', 'web', 'Web edition tools: threads, holes, sketch, export, round trip'],
  ['web4', 'web4.js', 'web', 'Web edition files: save in place, recent files, autosave and recovery'],
  ['web6', 'web6.js', 'docs', 'Published web app: installable, works offline, update bar (edits docs/app/sw.js, then restores it)'],
  ['touch', 'touch.js', 'web', 'Tablet layout and touch gestures'],
  ['tap', 'tap.js', 'web', 'Phone layout, sketching by tapping'],
  ['skdim-desktop', 'skdim.js', 'desktop', 'Smart Dimension on the desktop engine', { BASE: DESKTOP }],
  ['sfcov-desktop', 'sfcov.js', 'desktop', 'Support Fins coverage on the desktop engine', { BASE: DESKTOP }],
  ['sf9-desktop', 'sf9.js', 'desktop', 'Support Fins modes on the desktop engine', { BASE: DESKTOP }],
  ['web8-desktop', 'web8.js', 'desktop', 'Draw Walls on the desktop engine', { BASE: DESKTOP }],
  ['desktop-v11', 'desktop_v11.js', 'desktop', 'Threads, lay flat, sketch relations and dimensions, inch mode, dark theme'],
  ['desktop-new', 'desktop_new.js', 'desktop', 'Hole presets, push/pull, trim, offset, mirror, regions, split, fit to bed, sculpt'],
  ['desktop-tools', 'desktop_tools.js', 'desktop', 'Planes, scale, info, section and other tools'],
  ['desktop-sketch2', 'desktop_sketch2.js', 'desktop', 'Sketch on XZ, revolve, edit a sketch in the timeline, undo, save'],
  ['desktop-mirror', 'desktop_mirror.js', 'desktop', 'Mirror and pattern'],
  ['desktop-place', 'desktop_place.js', 'desktop', 'Placing primitives on faces'],
  ['desktop-smoke', 'desktop_smoke.js', 'desktop', 'Start screen, import, basic tools'],
  ['desktop-flow', 'desktop_flow.js', 'desktop', 'A whole edit: import, sketch, cut, hole, export'],
];

const up = url => new Promise(res => { const r = http.get(url, x => { x.resume(); res(x.statusCode < 500); }); r.on('error', () => res(false)); r.setTimeout(2000, () => { r.destroy(); res(false); }); });
const sleep = ms => new Promise(r => setTimeout(r, ms));
const started = [], servers = [];

async function ensureStatic(url, dir, port) {
  if (await up(url)) return;
  servers.push(await serve(dir, port));
}

async function ensure(url, cmd, args, opts) {
  if (await up(url)) return;
  const p = spawn(cmd, args, { stdio: 'ignore', ...opts });
  started.push(p);
  let exited = null;
  p.on('exit', code => { exited = code; });
  for (let i = 0; i < 120; i++) { if (await up(url)) return; if (exited !== null) break; await sleep(500); }
  throw new Error(`couldn't start ${cmd} ${args.join(' ')} (${url})` + (exited !== null ? `: it exited (code ${exited}); does this Python have the app's packages? Set PYTHON to one that does.` : ''));
}

function runOne([name, file, , , env]) {
  return new Promise(res => {
    const t0 = Date.now();
    const p = spawn(process.execPath, [path.join(__dirname, file)], { cwd: __dirname, env: { ...process.env, ...(env || {}) } });
    let out = '';
    p.stdout.on('data', d => { out += d; });
    p.stderr.on('data', d => { out += d; });
    const timer = setTimeout(() => { out += '\nTIMEOUT (240 s)'; p.kill(); }, 240000);
    p.on('close', code => {
      clearTimeout(timer);
      // web6 edits docs/app/sw.js for a moment; if it was stopped half way, put the file back
      const bak = path.join(OUT, 'sw.js.bak');
      if (fs.existsSync(bak)) { fs.copyFileSync(bak, path.join(ROOT, 'docs', 'app', 'sw.js')); fs.rmSync(bak); out += '\n(put docs/app/sw.js back)'; }
      fs.writeFileSync(path.join(OUT, `${name}.log`), out);
      const bad = code !== 0 || /^ERRORS$|^pageerror|TIMEOUT \(240 s\)|TimeoutError|^FAIL\b/m.test(out);
      res({ name, ok: !bad, secs: ((Date.now() - t0) / 1000).toFixed(0), out });
    });
  });
}

(async () => {
  const want = process.argv.slice(2);
  const list = TESTS.filter(t => !want.length || want.includes(t[0]) || want.includes(t[2]) || (want.includes('web') && t[2] === 'docs'));
  if (!list.length) { console.log('No such test. Names: ' + TESTS.map(t => t[0]).join(', ')); process.exit(2); }
  const groups = new Set(list.map(t => t[2]));
  try {
    if (groups.has('web')) await ensureStatic('http://127.0.0.1:8790/index.html', path.join(ROOT, 'web'), 8790);
    if (groups.has('docs')) await ensureStatic('http://127.0.0.1:8791/app/index.html', path.join(ROOT, 'docs'), 8791);
    // a throwaway settings folder, so the test files stay out of the user's File › Recent
    if (groups.has('desktop')) await ensure(DESKTOP + 'api/state', PY, [path.join(ROOT, 'main.py'), '--server', '--no-autosave'], { cwd: ROOT, env: { ...process.env, STLEDITOR_PORT: new URL(DESKTOP).port || '8765', STLEDITOR_CONFIG_DIR: fs.mkdtempSync(path.join(require('os').tmpdir(), 'stleditor-ui-config-')) } });
  } catch (e) { console.error(e.message); started.forEach(p => p.kill()); servers.forEach(x => x.close()); process.exit(2); }
  const results = [];
  for (const t of list) {
    process.stdout.write(`${t[0].padEnd(18)} `);
    const r = await runOne(t);
    results.push(r);
    console.log(`${r.ok ? 'ok  ' : 'FAIL'} ${r.secs.padStart(4)} s  ${t[3]}`);
    if (!r.ok) console.log('   ' + r.out.trim().split('\n').slice(-6).join('\n   '));
  }
  started.forEach(p => p.kill());
  servers.forEach(x => x.close());
  const bad = results.filter(r => !r.ok);
  console.log(`\n${results.length - bad.length} of ${results.length} passed${bad.length ? `; failed: ${bad.map(r => r.name).join(', ')}` : ''}. Logs and screenshots: ${OUT}`);
  process.exit(bad.length ? 1 : 0);
})();
