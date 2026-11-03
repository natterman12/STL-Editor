// Client for the engine. The desktop app talks to its local Python engine over HTTP; the web
// edition (any page not served from this computer) runs the JavaScript engine in a worker.
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
    if (WEB) return await engine('upload', { name: file.name, data: await file.arrayBuffer(), place: place || 'center' });
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

async function download(bytes, name, mime = 'application/octet-stream') {
  if (window.showSaveFilePicker) {
    try {
      const ext = name.slice(name.lastIndexOf('.'));
      const h = await window.showSaveFilePicker({ suggestedName: name, types: [{ description: ext.toUpperCase().slice(1) + ' file', accept: { [mime]: [ext] } }] });
      const w = await h.createWritable(); await w.write(bytes); await w.close();
      return h.name;
    } catch (e) {
      if (e?.name === 'AbortError') return null;
    }
  }
  const url = URL.createObjectURL(new Blob([bytes], { type: mime }));
  const a = document.createElement('a'); a.href = url; a.download = name;
  document.body.appendChild(a); a.click(); a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 30000);
  return name;
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

async function webCall(route, data) {
  switch (route) {
    case 'open': {
      const st = await engine('status');
      if (st.dirty && !data.force) return { ok: false, confirm: 'Discard unsaved changes?' };
      const [file] = await pickFiles('.stlx,.stl,.obj,.3mf');
      if (!file) return { ok: false, cancelled: true };
      return engine('open_bytes', { name: file.name, data: await file.arrayBuffer() });
    }
    case 'import': {
      const files = await pickFiles('.stl,.obj,.3mf', true);
      if (!files.length) return { ok: false, cancelled: true };
      let r;
      for (const f of files) r = await engine('upload', { name: f.name, data: await f.arrayBuffer(), place: data.place || 'center' });
      return r;
    }
    case 'save': {
      const r = await engine('save_bytes');
      const name = await download(r.bytes, r.name, 'application/zip');
      if (!name) return { ok: false, cancelled: true };
      return { ok: true, path: name, state: r.state };
    }
    case 'export': {
      const r = await engine('export_bytes', data);
      const mime = { stl: 'model/stl', '3mf': 'model/3mf', obj: 'model/obj' }[data.format] || 'application/octet-stream';
      const name = await download(r.bytes, r.name, mime);
      if (!name) return { ok: false, cancelled: true };
      return { ok: true, path: name, count: r.count };
    }
    case 'apps': return { ok: true, apps: {}, solidcraft_cli: false, web: true };
    case 'send_to': case 'to_step': throw new ApiError(DESKTOP);
    case 'quit': return { ok: true };
    case 'recovery/list': return { ok: true, items: [] };
    case 'recovery/open': case 'recovery/discard': return { ok: true };
    case 'settings/get': return { ok: true, settings: loadSettings() };
    case 'settings/set': { const s = loadSettings(); s[data.key] = data.value; saveSettings(s); return { ok: true }; }
    case 'update/status': return { ok: true, repo: '', current: 'web', available: false };
    case 'open_url': if (/^https:\/\//.test(data.url)) window.open(data.url, '_blank', 'noopener'); return { ok: true };
    case 'fonts': {
      const r = await engine('fonts');
      const local = localFonts ? [...new Set(localFonts.map(f => f.family))].sort((a, b) => a.localeCompare(b)) : [];
      return { ...r, fonts: [...r.fonts, ...local.filter(f => !r.fonts.includes(f))], canAskLocal: !!window.queryLocalFonts && !localFonts };
    }
    case 'sketch_op':
      if (data.op === 'text' && localFonts) {
        const fd = await localFontData(data.font, data.bold, data.italic);
        if (fd) return engine(route, { ...data, fontData: fd });
      }
      return engine(route, data);
    default: return engine(route, data);
  }
}
