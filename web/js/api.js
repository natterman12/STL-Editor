// Thin client for the local engine.
export class ApiError extends Error {}

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
