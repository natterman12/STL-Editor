// Mesh file reading and writing: STL (binary / ASCII), OBJ, 3MF; .stlx projects use zip.
import * as fflate from '../vendor/fflate.module.js';
import { GeometryError, weld, vertsBBox } from './geom.js';

export const MESH_EXTS = ['.stl', '.obj', '.3mf', '.ply', '.off', '.glb', '.gltf'];

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

// ---------------------------------------------------------------- PLY, OFF, glTF
const fan = (idx, out) => { for (let k = 1; k + 1 < idx.length; k++) out.push(idx[0], idx[k], idx[k + 1]); };

/** PLY, ASCII or binary (either byte order). Polygons are split into triangle fans. */
export function readPLY(buf) {
  const u8 = new Uint8Array(buf);
  const headEnd = (() => {
    const t = new TextDecoder('latin1').decode(u8.subarray(0, Math.min(u8.length, 65536)));
    const m = /end_header[^\n]*\n/.exec(t);
    if (!t.startsWith('ply') || !m) throw new GeometryError('Could not read this PLY file.');
    return { text: t.slice(0, m.index), end: m.index + m[0].length };
  })();
  const lines = headEnd.text.split(/\r?\n/).map(l => l.trim()).filter(Boolean);
  let format = 'ascii';
  const elements = [];
  for (const l of lines) {
    const w = l.split(/\s+/);
    if (w[0] === 'format') format = w[1];
    else if (w[0] === 'element') elements.push({ name: w[1], count: +w[2], props: [] });
    else if (w[0] === 'property' && elements.length) {
      const el = elements[elements.length - 1];
      if (w[1] === 'list') el.props.push({ list: true, ct: w[2], it: w[3], name: w[4] });
      else el.props.push({ type: w[1], name: w[2] });
    }
  }
  const SIZE = { char: 1, int8: 1, uchar: 1, uint8: 1, short: 2, int16: 2, ushort: 2, uint16: 2, int: 4, int32: 4, uint: 4, uint32: 4, float: 4, float32: 4, double: 8, float64: 8 };
  const v = [], f = [];
  if (format === 'ascii') {
    const tok = new TextDecoder('latin1').decode(u8.subarray(headEnd.end)).split(/\s+/).filter(Boolean);
    let p = 0;
    for (const el of elements) for (let i = 0; i < el.count; i++) {
      const rec = {};
      for (const pr of el.props) {
        if (pr.list) { const n = +tok[p++]; rec[pr.name] = tok.slice(p, p + n).map(Number); p += n; }
        else rec[pr.name] = +tok[p++];
      }
      if (el.name === 'vertex') v.push(rec.x, rec.y, rec.z);
      else if (el.name === 'face') fan(rec.vertex_indices || rec.vertex_index || [], f);
    }
  } else {
    const le = format === 'binary_little_endian';
    if (!le && format !== 'binary_big_endian') throw new GeometryError('Could not read this PLY file.');
    const dv = new DataView(u8.buffer, u8.byteOffset, u8.byteLength);
    let o = headEnd.end;
    const read = t => {
      let x;
      switch (t) {
        case 'char': case 'int8': x = dv.getInt8(o); break;
        case 'uchar': case 'uint8': x = dv.getUint8(o); break;
        case 'short': case 'int16': x = dv.getInt16(o, le); break;
        case 'ushort': case 'uint16': x = dv.getUint16(o, le); break;
        case 'int': case 'int32': x = dv.getInt32(o, le); break;
        case 'uint': case 'uint32': x = dv.getUint32(o, le); break;
        case 'float': case 'float32': x = dv.getFloat32(o, le); break;
        case 'double': case 'float64': x = dv.getFloat64(o, le); break;
        default: throw new GeometryError(`PLY property type "${t}" isn't supported.`);
      }
      o += SIZE[t];
      return x;
    };
    try {
      for (const el of elements) {
        const ix = el.props.findIndex(p => p.name === 'x'), iy = el.props.findIndex(p => p.name === 'y'), iz = el.props.findIndex(p => p.name === 'z');
        const il = el.props.findIndex(p => p.list && (p.name === 'vertex_indices' || p.name === 'vertex_index'));
        for (let i = 0; i < el.count; i++) {
          const vals = [];
          let idx = null;
          for (let k = 0; k < el.props.length; k++) {
            const pr = el.props[k];
            if (pr.list) { const n = read(pr.ct), a = new Array(n); for (let j = 0; j < n; j++) a[j] = read(pr.it); if (k === il) idx = a; }
            else vals[k] = read(pr.type);
          }
          if (el.name === 'vertex') v.push(vals[ix], vals[iy], vals[iz]);
          else if (el.name === 'face' && idx) fan(idx, f);
        }
      }
    } catch (e) {
      if (e instanceof GeometryError) throw e;
      throw new GeometryError('This PLY file is cut short.');
    }
  }
  if (!f.length) throw new GeometryError('No triangles found in this PLY file (point clouds can’t be edited).');
  return { v: new Float32Array(v), f: new Uint32Array(f) };
}

/** OFF (and its COFF / NOFF variants): vertices, then polygons. */
export function readOFF(buf) {
  const lines = new TextDecoder('latin1').decode(new Uint8Array(buf)).split(/\r?\n/)
    .map(l => l.replace(/#.*/, '').trim()).filter(Boolean);
  if (!lines.length || !/^[A-Z]*OFF/.test(lines[0])) throw new GeometryError('Could not read this OFF file.');
  let head = lines[0].replace(/^[A-Z]*OFF/, '').trim(), li = 1;
  if (!head) head = lines[li++] || '';
  const [nv, nf] = head.split(/\s+/).map(Number);
  if (!(nv >= 0 && nf >= 0) || li + nv + nf > lines.length) throw new GeometryError('Could not read this OFF file.');
  const v = [], f = [];
  for (let i = 0; i < nv; i++) { const w = lines[li++].split(/\s+/); v.push(+w[0], +w[1], +w[2]); }
  for (let i = 0; i < nf; i++) { const w = lines[li++].split(/\s+/).map(Number); fan(w.slice(1, 1 + w[0]), f); }
  if (!f.length) throw new GeometryError('No triangles found in this OFF file.');
  return { v: new Float32Array(v), f: new Uint32Array(f) };
}

/** glTF 2.0, as .glb or as .gltf with embedded data. One part per mesh primitive placed in
 *  the scene, with the node transforms applied (like the desktop app). */
export function readGLTF(buf) {
  const u8 = new Uint8Array(buf), dv = new DataView(u8.buffer, u8.byteOffset, u8.byteLength);
  let json, bin = null;
  if (u8.length >= 12 && dv.getUint32(0, true) === 0x46546C67) {   // "glTF"
    let o = 12;
    while (o + 8 <= u8.length) {
      const len = dv.getUint32(o, true), type = dv.getUint32(o + 4, true);
      const chunk = u8.subarray(o + 8, o + 8 + len);
      if (type === 0x4E4F534A) json = JSON.parse(new TextDecoder().decode(chunk));
      else if (type === 0x004E4942) bin = chunk;
      o += 8 + len;
    }
  } else {
    try { json = JSON.parse(new TextDecoder().decode(u8)); } catch { json = null; }
  }
  if (!json || !json.asset) throw new GeometryError('Could not read this glTF file.');
  const ext = [...(json.extensionsRequired || [])];
  if (ext.some(e => /draco|meshopt/i.test(e))) throw new GeometryError('This glTF file uses mesh compression (Draco or meshopt), which STL Editor can’t read. Export it again without compression.');
  const buffers = (json.buffers || []).map((b, i) => {
    if (b.uri === undefined) { if (!bin) throw new GeometryError('Could not read this glTF file.'); return bin; }
    const m = /^data:[^,]*;base64,(.*)$/.exec(b.uri);
    if (!m) throw new GeometryError('This .gltf file keeps its data in a separate file. Open the .glb version, or export it as .glb (a single file).');
    const s = atob(m[1]), a = new Uint8Array(s.length);
    for (let k = 0; k < s.length; k++) a[k] = s.charCodeAt(k);
    return a;
  });
  const NC = { SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4, MAT4: 16 };
  const accessor = i => {
    const a = json.accessors[i], bv = json.bufferViews[a.bufferView], b = buffers[bv.buffer];
    const nc = NC[a.type], ct = a.componentType;
    const size = { 5120: 1, 5121: 1, 5122: 2, 5123: 2, 5125: 4, 5126: 4 }[ct];
    const stride = bv.byteStride || nc * size, base = (bv.byteOffset || 0) + (a.byteOffset || 0);
    const d = new DataView(b.buffer, b.byteOffset, b.byteLength), out = new Float64Array(a.count * nc);
    const get = { 5120: o => d.getInt8(o), 5121: o => d.getUint8(o), 5122: o => d.getInt16(o, true), 5123: o => d.getUint16(o, true), 5125: o => d.getUint32(o, true), 5126: o => d.getFloat32(o, true) }[ct];
    for (let i2 = 0; i2 < a.count; i2++) for (let c = 0; c < nc; c++) out[i2 * nc + c] = get(base + i2 * stride + c * size);
    return out;
  };
  const mul = (A, B) => { const C = new Array(16).fill(0); for (let r = 0; r < 4; r++) for (let c = 0; c < 4; c++) for (let k = 0; k < 4; k++) C[c * 4 + r] += A[k * 4 + r] * B[c * 4 + k]; return C; };
  const local = n => {
    if (n.matrix) return n.matrix;
    const [tx, ty, tz] = n.translation || [0, 0, 0], [x, y, z, w] = n.rotation || [0, 0, 0, 1], [sx, sy, sz] = n.scale || [1, 1, 1];
    return [(1 - 2 * (y * y + z * z)) * sx, 2 * (x * y + z * w) * sx, 2 * (x * z - y * w) * sx, 0,
      2 * (x * y - z * w) * sy, (1 - 2 * (x * x + z * z)) * sy, 2 * (y * z + x * w) * sy, 0,
      2 * (x * z + y * w) * sz, 2 * (y * z - x * w) * sz, (1 - 2 * (x * x + y * y)) * sz, 0, tx, ty, tz, 1];
  };
  const parts = [];
  const visit = (ni, M, depth) => {
    const n = json.nodes[ni];
    if (!n || depth > 64) return;
    const W = mul(M, local(n));
    if (n.mesh !== undefined) {
      for (const pr of json.meshes[n.mesh].primitives || []) {
        const mode = pr.mode ?? 4;
        if (![4, 5, 6].includes(mode) || pr.attributes?.POSITION === undefined) continue;
        const P = accessor(pr.attributes.POSITION), nvx = P.length / 3;
        let idx = pr.indices !== undefined ? accessor(pr.indices) : Float64Array.from({ length: nvx }, (_, i) => i);
        const f = [];
        if (mode === 4) for (let i = 0; i + 2 < idx.length; i += 3) f.push(idx[i], idx[i + 1], idx[i + 2]);
        else if (mode === 5) for (let i = 0; i + 2 < idx.length; i++) f.push(...(i % 2 ? [idx[i + 1], idx[i], idx[i + 2]] : [idx[i], idx[i + 1], idx[i + 2]]));
        else for (let i = 1; i + 1 < idx.length; i++) f.push(idx[0], idx[i], idx[i + 1]);
        if (!f.length) continue;
        const v = new Float32Array(nvx * 3);
        for (let i = 0; i < nvx; i++) {
          const x = P[i * 3], y = P[i * 3 + 1], z = P[i * 3 + 2];
          v[i * 3] = W[0] * x + W[4] * y + W[8] * z + W[12];
          v[i * 3 + 1] = W[1] * x + W[5] * y + W[9] * z + W[13];
          v[i * 3 + 2] = W[2] * x + W[6] * y + W[10] * z + W[14];
        }
        // a mirroring transform turns the triangles inside out; flip them back
        const det = W[0] * (W[5] * W[10] - W[9] * W[6]) - W[4] * (W[1] * W[10] - W[9] * W[2]) + W[8] * (W[1] * W[6] - W[5] * W[2]);
        const F = Uint32Array.from(f);
        if (det < 0) for (let i = 0; i < F.length; i += 3) { const t = F[i + 1]; F[i + 1] = F[i + 2]; F[i + 2] = t; }
        parts.push({ name: null, v, f: F });
      }
    }
    for (const c of n.children || []) visit(c, W, depth + 1);
  };
  const I = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];
  const scene = json.scenes?.[json.scene ?? 0];
  if (scene) for (const ni of scene.nodes || []) visit(ni, I, 0);
  else (json.nodes || []).forEach((_, ni) => visit(ni, I, 0));
  if (!parts.length) throw new GeometryError('No triangles found in this glTF file.');
  // scene order differs between glTF readers: sort by position so both editions agree
  const r3 = x => Math.round(x * 1000) / 1000;
  const key = p => { const [lo] = vertsBBox(weld(p.v, p.f).v); return [r3(lo[0]), r3(lo[1]), r3(lo[2]), weld(p.v, p.f).f.length]; };
  const keyed = parts.map(p => ({ p, k: key(p) }));
  keyed.sort((a, b) => { for (let i = 0; i < 4; i++) if (a.k[i] !== b.k[i]) return a.k[i] - b.k[i]; return 0; });
  return keyed.map(x => x.p);
}

/** Read a mesh file into [{name, v, f}] parts with welded vertices. */
export function loadMeshFile(buf, filename) {
  const ext = extOf(filename), base = baseOf(filename);
  let parts;
  if (ext === '.stl') parts = [{ name: base, ...readSTL(buf) }];
  else if (ext === '.obj') parts = readOBJ(buf);
  else if (ext === '.3mf') parts = read3MF(buf);
  else if (ext === '.ply') parts = [{ name: base, ...readPLY(buf) }];
  else if (ext === '.off') parts = [{ name: base, ...readOFF(buf) }];
  else if (ext === '.glb' || ext === '.gltf') parts = readGLTF(buf);
  else throw new GeometryError('Unsupported file type. STL Editor opens STL, OBJ, 3MF, PLY, OFF, GLB and glTF.');
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
