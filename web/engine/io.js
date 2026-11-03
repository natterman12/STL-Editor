// Mesh file reading and writing: STL (binary / ASCII), OBJ, 3MF; .stlx projects use zip.
import * as fflate from '../vendor/fflate.module.js';
import { GeometryError, weld } from './geom.js';

export const MESH_EXTS = ['.stl', '.obj', '.3mf'];

export const extOf = name => { const m = /\.[^./\\]+$/.exec(name || ''); return m ? m[0].toLowerCase() : ''; };
export const baseOf = name => (name || 'model').replace(/^.*[\\/]/, '').replace(/\.[^.]+$/, '');

export function readSTL(buf) {
  const u8 = new Uint8Array(buf);
  if (u8.length >= 84) {
    const dv = new DataView(u8.buffer, u8.byteOffset, u8.byteLength);
    const n = dv.getUint32(80, true);
    if (84 + 50 * n === u8.length) {
      const v = new Float32Array(n * 9);
      for (let i = 0; i < n; i++) {
        const o = 84 + i * 50 + 12;
        for (let k = 0; k < 9; k++) v[i * 9 + k] = dv.getFloat32(o + k * 4, true);
      }
      const f = new Uint32Array(n * 3);
      for (let i = 0; i < f.length; i++) f[i] = i;
      return { v, f };
    }
  }
  const txt = new TextDecoder('latin1').decode(u8);
  const pts = [];
  const re = /vertex\s+(\S+)\s+(\S+)\s+(\S+)/g;
  let m;
  while ((m = re.exec(txt))) pts.push(+m[1], +m[2], +m[3]);
  if (!pts.length || pts.length % 9) throw new GeometryError('Could not read this STL file.');
  const f = new Uint32Array(pts.length / 3);
  for (let i = 0; i < f.length; i++) f[i] = i;
  return { v: new Float32Array(pts), f };
}

export function readOBJ(buf) {
  const txt = new TextDecoder().decode(new Uint8Array(buf));
  const v = [], f = [];
  const parts = [];
  let name = null;
  const flush = () => { if (f.length) parts.push({ name, f: f.splice(0) }); };
  for (const line of txt.split(/\r?\n/)) {
    const s = line.trim();
    if (s.startsWith('v ')) { const p = s.split(/\s+/); v.push(+p[1], +p[2], +p[3]); }
    else if (s.startsWith('f ')) {
      const idx = s.split(/\s+/).slice(1).map(t => { const i = parseInt(t, 10); return i < 0 ? v.length / 3 + i : i - 1; });
      for (let k = 1; k + 1 < idx.length; k++) f.push(idx[0], idx[k], idx[k + 1]);
    } else if (s.startsWith('o ') || s.startsWith('g ')) { flush(); name = s.slice(2).trim() || null; }
  }
  flush();
  if (!parts.length) throw new GeometryError('No triangles found in this OBJ file.');
  const V = new Float32Array(v);
  return parts.map(p => ({ name: p.name, v: V, f: new Uint32Array(p.f) }));
}

function attr(tag, name) { const m = new RegExp(`\\b${name}\\s*=\\s*"([^"]*)"`).exec(tag); return m ? m[1] : null; }
function parseTransform(s) {
  if (!s) return null;
  const a = s.trim().split(/\s+/).map(Number);
  return a.length === 12 ? a : null;  // 3MF: m00 m01 m02 m10 m11 m12 m20 m21 m22 m30 m31 m32 (row vectors)
}
function applyT(v, T) {
  if (!T) return v;
  const o = new Float32Array(v.length);
  for (let i = 0; i < v.length; i += 3) {
    const x = v[i], y = v[i + 1], z = v[i + 2];
    o[i] = x * T[0] + y * T[3] + z * T[6] + T[9];
    o[i + 1] = x * T[1] + y * T[4] + z * T[7] + T[10];
    o[i + 2] = x * T[2] + y * T[5] + z * T[8] + T[11];
  }
  return o;
}
function mulT(A, B) {  // apply A then B
  if (!A) return B; if (!B) return A;
  const r = new Array(12);
  for (let i = 0; i < 3; i++) for (let j = 0; j < 3; j++) r[i * 3 + j] = A[i * 3] * B[j] + A[i * 3 + 1] * B[3 + j] + A[i * 3 + 2] * B[6 + j];
  for (let j = 0; j < 3; j++) r[9 + j] = A[9] * B[j] + A[10] * B[3 + j] + A[11] * B[6 + j] + B[9 + j];
  return r;
}

export function read3MF(buf) {
  let files;
  try { files = fflate.unzipSync(new Uint8Array(buf)); } catch { throw new GeometryError('Could not read this 3MF file.'); }
  const objects = new Map();
  const builds = [];
  for (const [path, data] of Object.entries(files)) {
    if (!/\.model$/i.test(path)) continue;
    const xml = new TextDecoder().decode(data);
    const objRe = /<object\b([^>]*)>([\s\S]*?)<\/object>/g;
    let m;
    while ((m = objRe.exec(xml))) {
      const id = attr(m[1], 'id'), name = attr(m[1], 'name');
      const body = m[2];
      const meshM = /<mesh\b[\s\S]*?<\/mesh>/.exec(body);
      const comps = [...body.matchAll(/<component\b([^>]*)\/?>/g)].map(c => ({ id: attr(c[1], 'objectid'), T: parseTransform(attr(c[1], 'transform')) }));
      let mesh = null;
      if (meshM) {
        const vs = [], fs = [];
        for (const vm of meshM[0].matchAll(/<vertex\b([^>]*)\/?>/g)) vs.push(+attr(vm[1], 'x'), +attr(vm[1], 'y'), +attr(vm[1], 'z'));
        for (const tm of meshM[0].matchAll(/<triangle\b([^>]*)\/?>/g)) fs.push(+attr(tm[1], 'v1'), +attr(tm[1], 'v2'), +attr(tm[1], 'v3'));
        mesh = { v: new Float32Array(vs), f: new Uint32Array(fs) };
      }
      objects.set(path + '#' + id, { name, mesh, comps, path });
      if (!objects.has('#' + id)) objects.set('#' + id, objects.get(path + '#' + id));
    }
    for (const b of xml.matchAll(/<item\b([^>]*)\/?>/g)) builds.push({ id: attr(b[1], 'objectid'), T: parseTransform(attr(b[1], 'transform')), path });
  }
  const parts = [];
  const emit = (key, T, depth = 0) => {
    const o = objects.get(key) || objects.get('#' + key.split('#').pop());
    if (!o || depth > 8) return [];
    const out = [];
    if (o.mesh && o.mesh.f.length) out.push({ v: applyT(o.mesh.v, T), f: o.mesh.f });
    for (const c of o.comps) out.push(...emit('#' + c.id, mulT(c.T, T), depth + 1));
    return out;
  };
  for (const b of builds) {
    const o = objects.get(b.path + '#' + b.id) || objects.get('#' + b.id);
    const meshes = emit(b.path + '#' + b.id, b.T);
    if (!meshes.length) continue;
    // merge the components of one build item into one part
    let nv = 0, nf = 0;
    for (const m of meshes) { nv += m.v.length; nf += m.f.length; }
    const v = new Float32Array(nv), f = new Uint32Array(nf);
    let ov = 0, of = 0;
    for (const m of meshes) { v.set(m.v, ov); for (let i = 0; i < m.f.length; i++) f[of + i] = m.f[i] + ov / 3; ov += m.v.length; of += m.f.length; }
    parts.push({ name: o?.name || null, v, f });
  }
  if (!parts.length) throw new GeometryError('No triangles found in this 3MF file.');
  return parts;
}

/** Read a mesh file into [{name, v, f}] parts with welded vertices. */
export function loadMeshFile(buf, filename) {
  const ext = extOf(filename), base = baseOf(filename);
  let parts;
  if (ext === '.stl') parts = [{ name: base, ...readSTL(buf) }];
  else if (ext === '.obj') parts = readOBJ(buf);
  else if (ext === '.3mf') parts = read3MF(buf);
  else throw new GeometryError('Unsupported file type. The web edition opens STL, OBJ and 3MF.');
  return parts.map((p, i) => {
    const w = weld(p.v, p.f);
    return { name: parts.length === 1 ? base : (p.name || `${base} ${i + 1}`), v: w.v, f: w.f };
  });
}

// ---------------------------------------------------------------- writers
export function stlBytes(meshes) {
  let n = 0;
  for (const m of meshes) n += m.f.length / 3;
  const buf = new ArrayBuffer(84 + 50 * n);
  const dv = new DataView(buf);
  const head = 'STL Editor binary STL';
  for (let i = 0; i < 80; i++) dv.setUint8(i, i < head.length ? head.charCodeAt(i) : 32);
  dv.setUint32(80, n, true);
  let o = 84;
  for (const { v, f } of meshes) {
    for (let t = 0; t < f.length; t += 3) {
      const a = f[t] * 3, b = f[t + 1] * 3, c = f[t + 2] * 3;
      const ux = v[b] - v[a], uy = v[b + 1] - v[a + 1], uz = v[b + 2] - v[a + 2];
      const wx = v[c] - v[a], wy = v[c + 1] - v[a + 1], wz = v[c + 2] - v[a + 2];
      let nx = uy * wz - uz * wy, ny = uz * wx - ux * wz, nz = ux * wy - uy * wx;
      const L = Math.hypot(nx, ny, nz) || 1;
      dv.setFloat32(o, nx / L, true); dv.setFloat32(o + 4, ny / L, true); dv.setFloat32(o + 8, nz / L, true);
      o += 12;
      for (const p of [a, b, c]) { dv.setFloat32(o, v[p], true); dv.setFloat32(o + 4, v[p + 1], true); dv.setFloat32(o + 8, v[p + 2], true); o += 12; }
      o += 2;
    }
  }
  return new Uint8Array(buf);
}

export function objBytes(meshes) {
  const out = ['# STL Editor'];
  let base = 1;
  meshes.forEach((m, i) => {
    out.push(`o ${(m.name || `body${i + 1}`).replace(/\s+/g, '_')}`);
    for (let k = 0; k < m.v.length; k += 3) out.push(`v ${+m.v[k].toFixed(6)} ${+m.v[k + 1].toFixed(6)} ${+m.v[k + 2].toFixed(6)}`);
    for (let k = 0; k < m.f.length; k += 3) out.push(`f ${m.f[k] + base} ${m.f[k + 1] + base} ${m.f[k + 2] + base}`);
    base += m.v.length / 3;
  });
  return fflate.strToU8(out.join('\n') + '\n');
}

const esc = s => String(s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

export function threeMFBytes(meshes) {
  const objs = [], items = [];
  meshes.forEach((m, i) => {
    const id = i + 1;
    const vs = [], ts = [];
    for (let k = 0; k < m.v.length; k += 3) vs.push(`<vertex x="${+m.v[k].toFixed(6)}" y="${+m.v[k + 1].toFixed(6)}" z="${+m.v[k + 2].toFixed(6)}"/>`);
    for (let k = 0; k < m.f.length; k += 3) ts.push(`<triangle v1="${m.f[k]}" v2="${m.f[k + 1]}" v3="${m.f[k + 2]}"/>`);
    objs.push(`<object id="${id}" name="${esc(m.name || `Body ${id}`)}" type="model"><mesh><vertices>${vs.join('')}</vertices><triangles>${ts.join('')}</triangles></mesh></object>`);
    items.push(`<item objectid="${id}"/>`);
  });
  const model = `<?xml version="1.0" encoding="UTF-8"?>\n<model unit="millimeter" xml:lang="en-US" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><metadata name="Application">STL Editor</metadata><resources>${objs.join('')}</resources><build>${items.join('')}</build></model>`;
  return fflate.zipSync({
    '[Content_Types].xml': fflate.strToU8('<?xml version="1.0" encoding="UTF-8"?>\n<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>'),
    '_rels/.rels': fflate.strToU8('<?xml version="1.0" encoding="UTF-8"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Target="/3D/3dmodel.model" Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/></Relationships>'),
    '3D/3dmodel.model': fflate.strToU8(model),
  }, { level: 6 });
}

export function exportBytes(meshes, fmt) {
  if (fmt === 'stl') return stlBytes(meshes);
  if (fmt === 'obj') return objBytes(meshes);
  if (fmt === '3mf') return threeMFBytes(meshes);
  throw new GeometryError(`Unknown format ${fmt}.`);
}

export const zipSync = (files, opts) => fflate.zipSync(files, opts);
export const unzipSync = buf => fflate.unzipSync(buf);
export const strToU8 = s => fflate.strToU8(s);
export const strFromU8 = b => fflate.strFromU8(b);
