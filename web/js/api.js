// Client for the engine. The desktop app talks to its local Python engine over HTTP; the web
// edition (any page not served from this computer) runs the JavaScript engine in a worker.
import * as store from './webstore.js';

export class ApiError extends Error {}

export const WEB = (() => {
  try {
    const q = new URLSearchParams(location.search);
    if (q.has('web')) return true;
    if (q.has('desktop')) return false;
  } catch { /* ignore */ }
  return location.protocol === 'file:' || !['127.0.0.1', 'localhost'].includes(location.hostname);
})();

let busyCount = 0, busyTimer = null;
function busy(on, msg) {
  const el = document.getElementById('busy');
  if (on) {
    busyCount++;
    if (busyCount === 1) {
      busyTimer = setTimeout(() => {
        document.getElementById('busy-msg').textContent = msg || 'Working…';
        el.classList.remove('hidden');
      }, 350);
    }
  } else {
    busyCount = Math.max(0, busyCount - 1);
    if (busyCount === 0) { clearTimeout(busyTimer); el.classList.add('hidden'); }
  }
}

export async function post(route, data = {}, { quiet = false, msg } = {}) {
  if (!quiet) busy(true, msg);
  try {
    if (WEB) return await webCall(route, data);
    const r = await fetch(`/api/${route}`, {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(data),
    });
    const ct = r.headers.get('Content-Type') || '';
    if (ct.includes('octet-stream')) return decodePack(await r.arrayBuffer());
    const j = await r.json();
    if (!r.ok || j.error) throw new ApiError(j.error || `Request failed (${r.status})`);
    return j;
  } finally {
    if (!quiet) busy(false);
  }
}

export async function get(route) {
  if (WEB) {
    const [name, qs] = route.split('?');
    const q = Object.fromEntries(new URLSearchParams(qs || ''));
    return webCall(name, q);
  }
  const r = await fetch(`/api/${route}`);
  const ct = r.headers.get('Content-Type') || '';
  if (ct.includes('octet-stream')) return decodePack(await r.arrayBuffer());
  const j = await r.json();
  if (!r.ok || j.error) throw new ApiError(j.error || `Request failed (${r.status})`);
  return j;
}

export async function upload(file, { place } = {}) {
  busy(true, `Importing ${file.name}…`);
  try {
    if (WEB) return decorate(await engine('upload', { name: file.name, data: await file.arrayBuffer(), place: place || 'center' }));
    const r = await fetch(`/api/upload?name=${encodeURIComponent(file.name)}${place ? `&place=${place}` : ''}`, { method: 'POST', body: file });
    const j = await r.json();
    if (!r.ok || j.error) throw new ApiError(j.error || 'Upload failed');
    return j;
  } finally { busy(false); }
}

// Binary pack: [u32 header len][header json][per mesh: f32 verts (nv*3), u32 tris (nf*3)]
export function decodePack(buf) {
  const dv = new DataView(buf);
  const hl = dv.getUint32(0, true);
  const header = JSON.parse(new TextDecoder().decode(new Uint8Array(buf, 4, hl)));
  const base = 4 + hl;
  header.meshes = (header.meshes || []).map(m => {
    const vo = base + m.offset;
    const positions = new Float32Array(buf, vo, m.nv * 3);
    const index = new Uint32Array(buf, vo + m.nv * 12, m.nf * 3);
    return { ...m, positions, index };
  });
  return header;
}

// ------------------------------------------------------------------ web edition
let worker = null, seq = 0;
const pending = new Map();

function engine(route, data = {}) {
  if (!worker) {
    startWebServices();
    worker = new Worker(new URL('../engine/worker.js', import.meta.url), { type: 'module' });
    worker.onmessage = ev => {
      const { id, ok, result, error } = ev.data;
      const p = pending.get(id); if (!p) return;
      pending.delete(id);
      ok ? p.resolve(result) : p.reject(new ApiError(error));
    };
    worker.onerror = ev => { for (const p of pending.values()) p.reject(new ApiError(`The geometry engine stopped: ${ev.message || 'unknown error'}. Reload the page.`)); pending.clear(); };
  }
  return new Promise((resolve, reject) => {
    const id = ++seq;
    pending.set(id, { resolve, reject });
    const transfer = data && data.data instanceof ArrayBuffer ? [data.data] : (data?.fontData instanceof ArrayBuffer ? [data.fontData] : []);
    worker.postMessage({ id, route, data }, transfer);
  });
}

function pickFiles(accept, multiple = false) {
  return new Promise(resolve => {
    const inp = document.createElement('input');
    inp.type = 'file'; inp.accept = accept; inp.multiple = multiple; inp.style.display = 'none';
    let settled = false;
    const done = files => { if (!settled) { settled = true; inp.remove(); resolve(files); } };
    inp.onchange = () => done([...inp.files]);
    inp.addEventListener('cancel', () => done([]));
    document.body.appendChild(inp);
    inp.click();
    // some browsers don't fire "cancel": treat focus coming back with no file as cancel
    window.addEventListener('focus', () => setTimeout(() => { if (!inp.files?.length) done([]); }, 800), { once: true });
  });
}

const MESH_ACCEPT = '.stl,.obj,.3mf,.ply,.off,.glb,.gltf';
const PICKER_TYPES = [
  { description: 'STL Editor project or mesh', accept: { 'application/octet-stream': ['.stlx', '.stl', '.obj', '.3mf', '.ply', '.off', '.glb', '.gltf'] } },
];

/** Write bytes to a file the person picks (Chrome, Edge), or download them (other browsers).
 *  Returns {name, handle} or null if cancelled. */
async function saveFile(bytes, name, mime = 'application/octet-stream') {
  if (window.showSaveFilePicker) {
    let h;
    try {
      const ext = name.slice(name.lastIndexOf('.'));
      h = await window.showSaveFilePicker({ suggestedName: name, types: [{ description: ext.toUpperCase().slice(1) + ' file', accept: { [mime]: [ext] } }] });
    } catch (e) {
      if (e?.name === 'AbortError') return null;
      h = null;   // picker not allowed here: fall back to a download
    }
    if (h) { await writeHandle(h, bytes); return { name: h.name, handle: h }; }
  }
  const url = URL.createObjectURL(new Blob([bytes], { type: mime }));
  const a = document.createElement('a'); a.href = url; a.download = name;
  document.body.appendChild(a); a.click(); a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 30000);
  return { name, handle: null };
}

async function permitted(handle, mode) {
  try {
    if ((await handle.queryPermission?.({ mode })) === 'granted') return true;
    return (await handle.requestPermission?.({ mode })) === 'granted';
  } catch { return false; }
}

async function writeHandle(handle, bytes) {
  if (!(await permitted(handle, 'readwrite'))) throw new ApiError('The browser didn’t allow saving to that file. Use File › Save As.');
  const w = await handle.createWritable(); await w.write(bytes); await w.close();
}

// ---------------------------------------------------------------- recent files, autosave
let current = { handle: null, name: null };   // the project file Save writes back to
let recent = [];                              // [{id, name, when}] newest first
let lastAutosave = null, lastKey = null, servicesStarted = false, recentReady = Promise.resolve();
const RECENT_MAX = 10, RECENT_COPY_MAX = 25e6;
const recentPaths = () => recent.map(r => `recent/${r.id}/${r.name}`);

function decorate(r) {
  const st = r && r.state ? r.state : (r && r.doc && r.features ? r : null);
  if (st) { st.recent = recentPaths(); if (st.doc) st.doc.autosaved = lastAutosave; }
  return r;
}

async function loadRecent() {
  try { recent = (await store.all('recent')).sort((a, b) => b.when - a.when); }
  catch { recent = []; }
}

async function addRecent(name, handle, bytes) {
  try {
    // the same file again (same handle, or same name without one) replaces its entry
    for (const e of await store.all('recent')) {
      let same = false;
      if (handle && e.hasHandle) { const h = (await store.get('handles', 'recent:' + e.id))?.handle; same = !!h && await handle.isSameEntry(h).catch(() => false); }
      else if (!handle && !e.hasHandle) same = e.name === name;
      if (same) await dropRecent(e.id);
    }
    const copy = !handle && bytes && bytes.byteLength <= RECENT_COPY_MAX;
    if (!handle && !copy) return;   // nothing to reopen it from
    const id = store.newId();
    if (handle) await store.put('handles', { id: 'recent:' + id, handle });
    else await store.put('blobs', { id: 'recent:' + id, bytes: new Uint8Array(bytes).slice() });
    await store.put('recent', { id, name, when: Date.now(), hasHandle: !!handle });
    const rest = (await store.all('recent')).sort((a, b) => b.when - a.when);
    for (const e of rest.slice(RECENT_MAX)) await dropRecent(e.id);
    await loadRecent();
  } catch { /* storage off: no recent list */ }
}

async function dropRecent(id) {
  await store.del('recent', id); await store.del('handles', 'recent:' + id); await store.del('blobs', 'recent:' + id);
}

async function clearAutosave() {
  lastKey = null;
  try {
    await store.del('autosave', store.SESSION);
    await store.del('blobs', 'autosave:' + store.SESSION); await store.del('handles', 'autosave:' + store.SESSION);
  } catch { /* storage off */ }
}

async function autosaveNow(force = false) {
  try {
    const r = await engine('snapshot');
    if (r.empty || !r.dirty) { if (lastKey !== null) await clearAutosave(); return; }
    if (r.key === lastKey && !force) return;
    const id = store.SESSION;
    await store.put('blobs', { id: 'autosave:' + id, bytes: r.bytes });
    if (current.handle) await store.put('handles', { id: 'autosave:' + id, handle: current.handle });
    else await store.del('handles', 'autosave:' + id);
    await store.put('autosave', { id, title: r.title, path: r.path, steps: r.steps, saved: Date.now() / 1000 });
    lastKey = r.key;
    lastAutosave = Date.now() / 1000;
  } catch { /* storage full or off: try again next time */ }
}

/** Write the autosave copy now (before the page reloads). */
export const flushAutosave = () => (servicesStarted ? autosaveNow() : Promise.resolve());

function startWebServices() {
  if (servicesStarted) return;
  servicesStarted = true;
  store.holdSessionLock();
  recentReady = loadRecent();
  const every = Math.max(15, +(loadSettings().autosave_interval || 60)) * 1000;
  setInterval(() => autosaveNow(), every);
  // the tab is being hidden or closed: save what we have
  document.addEventListener('visibilitychange', () => { if (document.visibilityState === 'hidden') autosaveNow(); });
}

async function recoverable() {
  let items = [];
  try { items = await store.all('autosave'); } catch { return []; }
  const live = await store.liveSessions().catch(() => null);
  return items.filter(it => it.id !== store.SESSION && (live ? !live.has(it.id) : Date.now() / 1000 - it.saved > 180));
}

async function discardAutosave(id) {
  await store.del('autosave', id); await store.del('blobs', 'autosave:' + id); await store.del('handles', 'autosave:' + id);
}

async function openFile(file, handle = null) {
  const data = await file.arrayBuffer();
  const isProject = /\.stlx$/i.test(file.name);
  const r = await engine('open_bytes', { name: file.name, data: data.slice(0) });
  current = isProject ? { handle, name: file.name } : { handle: null, name: null };
  await clearAutosave();
  await addRecent(file.name, handle, data);
  return decorate(r);
}

async function pickToOpen() {
  if (window.showOpenFilePicker) {
    try {
      const [h] = await window.showOpenFilePicker({ types: PICKER_TYPES, excludeAcceptAllOption: false, multiple: false });
      return { file: await h.getFile(), handle: h };
    } catch (e) { if (e?.name === 'AbortError') return null; }
  }
  const [file] = await pickFiles('.stlx,' + MESH_ACCEPT);
  return file ? { file, handle: null } : null;
}

async function openRecent(path) {
  const id = path.split('/')[1];
  const e = await store.get('recent', id).catch(() => null);
  if (!e) throw new ApiError('That file is no longer in the recent list.');
  if (e.hasHandle) {
    const h = (await store.get('handles', 'recent:' + id))?.handle;
    if (!h || !(await permitted(h, 'read'))) throw new ApiError('The browser didn’t allow opening that file.');
    let file;
    try { file = await h.getFile(); } catch {
      await dropRecent(id); await loadRecent();
      throw new ApiError(`${e.name} has been moved or deleted.`);
    }
    return openFile(file, h);
  }
  const b = (await store.get('blobs', 'recent:' + id))?.bytes;
  if (!b) { await dropRecent(id); await loadRecent(); throw new ApiError('That file is no longer in the recent list.'); }
  return openFile(new File([b], e.name), null);
}

/** Open files handed over by the operating system (installed app, "Open with"). */
export async function openLaunchFiles(handles) {
  const out = [];
  for (const h of handles) {
    const file = await h.getFile();
    if (/\.stlx$/i.test(file.name) || !out.length && handles.length === 1) out.push(await openFile(file, h));
    else out.push(decorate(await engine('upload', { name: file.name, data: await file.arrayBuffer(), place: 'center' })));
  }
  return out.at(-1);
}

const SETTINGS_KEY = 'stle.settings';
const loadSettings = () => { try { return JSON.parse(localStorage.getItem(SETTINGS_KEY) || '{}'); } catch { return {}; } };
const saveSettings = s => { try { localStorage.setItem(SETTINGS_KEY, JSON.stringify(s)); } catch { /* storage off */ } };
const DESKTOP = 'This needs the desktop app. Download it from the project page; your project opens there with the full timeline.';

let localFonts = null;   // FontData objects from the Local Font Access API
export async function requestLocalFonts() {
  if (!window.queryLocalFonts) throw new ApiError('This browser can’t share the fonts installed on your computer. Chrome and Edge can.');
  const list = await window.queryLocalFonts();
  localFonts = list.filter(f => !/\b(Emoji|Symbol)\b/i.test(f.family));
  return [...new Set(localFonts.map(f => f.family))].sort((a, b) => a.localeCompare(b));
}
async function localFontData(family, bold, italic) {
  if (!localFonts) return null;
  const fam = localFonts.filter(f => f.family === family);
  if (!fam.length) return null;
  const want = (bold ? 'Bold' : '') + (italic ? ' Italic' : '');
  const pick = fam.find(f => f.style.replace('Oblique', 'Italic').trim() === (want.trim() || 'Regular'))
    || fam.find(f => /regular|normal|book/i.test(f.style)) || fam[0];
  return (await pick.blob()).arrayBuffer();
}

/** Fonts added from font files (every browser): pick .ttf/.otf/.woff files, keep them in this
 *  browser. Returns the family names added. */
export async function addFontFiles() {
  const files = await pickFiles('.ttf,.otf,.woff', true);
  const added = [];
  for (const f of files) {
    const data = await f.arrayBuffer();
    const info = await engine('font_info', { data: data.slice(0) });
    const rec = (await store.get('fonts', info.family).catch(() => null)) || { id: info.family, family: info.family, styles: {} };
    rec.styles[info.style] = new Uint8Array(data);
    await store.put('fonts', rec);
    if (!added.includes(info.family)) added.push(info.family);
  }
  return added;
}
async function fileFontData(family, bold, italic) {
  const rec = await store.get('fonts', family).catch(() => null);
  if (!rec) return null;
  const want = (bold ? 'b' : '') + (italic ? 'i' : '');
  const b = rec.styles[want] || rec.styles[bold ? 'b' : ''] || rec.styles[''] || Object.values(rec.styles)[0];
  return b ? b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength) : null;
}

async function webCall(route, data) {
  switch (route) {
    case 'state': await recentReady; return decorate(await engine('state'));
    case 'new': {
      const r = await engine('new', data);
      if (r.ok) { current = { handle: null, name: null }; await clearAutosave(); }
      return decorate(r);
    }
    case 'open': {
      const st = await engine('status');
      if (st.dirty && !data.force) return { ok: false, confirm: 'Discard unsaved changes?' };
      if (data.path && data.path.startsWith('recent/')) return openRecent(data.path);
      const pick = await pickToOpen();
      if (!pick) return { ok: false, cancelled: true };
      return openFile(pick.file, pick.handle);
    }
    case 'import': {
      const files = await pickFiles(MESH_ACCEPT, true);
      if (!files.length) return { ok: false, cancelled: true };
      let r;
      for (const f of files) r = await engine('upload', { name: f.name, data: await f.arrayBuffer(), place: data.place || 'center' });
      return decorate(r);
    }
    case 'save': {
      const r = await engine('save_bytes');
      let out;
      if (!data.save_as && current.handle) { await writeHandle(current.handle, r.bytes); out = { name: current.handle.name, handle: current.handle }; }
      else {
        out = await saveFile(r.bytes, current.name || r.name, 'application/zip');
        if (!out) return { ok: false, cancelled: true };
      }
      current = { handle: out.handle, name: out.name };
      const s2 = await engine('saved', { name: out.name });
      await clearAutosave();
      await addRecent(out.name, out.handle, r.bytes.buffer);
      return decorate({ ok: true, path: out.name, state: s2.state });
    }
    case 'export': {
      const r = await engine('export_bytes', data);
      const mime = { stl: 'model/stl', '3mf': 'model/3mf', obj: 'model/obj' }[data.format] || 'application/octet-stream';
      const out = await saveFile(r.bytes, r.name, mime);
      if (!out) return { ok: false, cancelled: true };
      return { ok: true, path: out.name, count: r.count };
    }
    case 'recovery/list':
      return { ok: true, items: (await recoverable()).map(it => ({ id: it.id, title: it.title, steps: it.steps, saved: it.saved })) };
    case 'recovery/open': {
      const it = await store.get('autosave', data.id);
      const b = (await store.get('blobs', 'autosave:' + data.id))?.bytes;
      if (!it || !b) throw new ApiError('That autosave copy is gone.');
      const h = (await store.get('handles', 'autosave:' + data.id).catch(() => null))?.handle || null;
      const r = await engine('open_bytes', { name: it.title, data: b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength), recovered: true, path: it.path, title: it.title });
      current = { handle: h, name: it.path || null };
      await discardAutosave(data.id);
      await autosaveNow(true);
      return decorate(r);
    }
    case 'recovery/discard': await discardAutosave(data.id).catch(() => {}); return { ok: true };
    case 'apps': return { ok: true, apps: {}, solidcraft_cli: false, web: true };
    case 'send_to': case 'to_step': throw new ApiError(DESKTOP);
    case 'quit': return { ok: true };
    case 'settings/get': return { ok: true, settings: loadSettings() };
    case 'settings/set': { const s = loadSettings(); s[data.key] = data.value; saveSettings(s); return { ok: true }; }
    case 'update/status': return { ok: true, repo: '', current: 'web', available: false };
    case 'open_url': if (/^https:\/\//.test(data.url)) window.open(data.url, '_blank', 'noopener'); return { ok: true };
    case 'fonts': {
      const r = await engine('fonts');
      const local = localFonts ? [...new Set(localFonts.map(f => f.family))].sort((a, b) => a.localeCompare(b)) : [];
      const added = (await store.all('fonts').catch(() => [])).map(x => x.family).sort((a, b) => a.localeCompare(b));
      const extra = [...added, ...local].filter((f, i, a) => !r.fonts.includes(f) && a.indexOf(f) === i);
      return { ...r, fonts: [...r.fonts, ...extra], canAskLocal: !!window.queryLocalFonts && !localFonts, canAddFile: true };
    }
    case 'sketch_op':
      if (data.op === 'text') {
        const fd = (await fileFontData(data.font, data.bold, data.italic)) || (localFonts && await localFontData(data.font, data.bold, data.italic));
        if (fd) return engine(route, { ...data, fontData: fd });
      }
      return engine(route, data);
    default: return decorate(await engine(route, data));
  }
}
