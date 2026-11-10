//! glTF / GLB reader. Port of trimesh 5.2.0 `exchange/gltf` (`load_glb`, `load_gltf`,
//! `_read_buffers`) + `Scene.dump()`.
//!
//! Only triangle geometry is read (no materials, no extensions). Buffers must be inside the
//! file (GLB chunk or `data:` URI); outside files cannot be read, as in 1.x.

use std::collections::HashSet;

use serde_json::Value;

use super::{Part, ReadError};

type Mat = [[f64; 4]; 4];

const IDENTITY: Mat = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

fn bad() -> ReadError {
    ReadError("This glTF file couldn't be read.".into())
}

fn mul(a: &Mat, b: &Mat) -> Mat {
    let mut o = [[0.0; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            o[i][j] = (0..4).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    o
}

fn b64(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => continue, // padding, whitespace, anything else is dropped
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

fn uri_bytes(uri: &str) -> Result<Vec<u8>, ReadError> {
    match uri.find("base64,") {
        Some(i) => Ok(b64(&uri[i + 7..])),
        None => Err(ReadError(
            "This glTF file points to other files. Save it as a .glb, or embed the data, and try again."
                .into(),
        )),
    }
}

fn u32le(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(o..o.checked_add(4)?)?.try_into().ok()?))
}

fn read_glb(data: &[u8]) -> Result<(Value, Vec<Vec<u8>>), ReadError> {
    if data.len() < 20 || &data[0..4] != b"glTF" {
        return Err(ReadError("This glTF file couldn't be read: incorrect header.".into()));
    }
    if u32le(data, 4) != Some(2) {
        return Err(ReadError("Only glTF 2 files are supported.".into()));
    }
    let length = u32le(data, 8).ok_or_else(bad)? as usize;
    let clen = u32le(data, 12).ok_or_else(bad)? as usize;
    if u32le(data, 16) != Some(0x4E4F_534A) {
        return Err(bad());
    }
    let json = data.get(20..20usize.checked_add(clen).ok_or_else(bad)?).ok_or_else(bad)?;
    let header: Value = serde_json::from_slice(json).map_err(|_| bad())?;
    let mut pos = 20usize.checked_add(clen).ok_or_else(bad)?;
    let start = pos;
    let mut info: std::collections::VecDeque<Value> = header
        .get("buffers")
        .and_then(|b| b.as_array())
        .cloned()
        .unwrap_or_default()
        .into();
    let mut buffers = Vec::new();
    while pos - start < length {
        if let Some(b) = info.pop_front() {
            if let Some(uri) = b.get("uri").and_then(|u| u.as_str()) {
                buffers.push(uri_bytes(uri)?);
                continue;
            }
        }
        let (Some(l), Some(t)) = (u32le(data, pos), pos.checked_add(4).and_then(|p| u32le(data, p))) else {
            break;
        };
        let (l, body) = (l as usize, pos.checked_add(8).ok_or_else(bad)?);
        let end = body.checked_add(l).ok_or_else(bad)?;
        let chunk = data.get(body..end).ok_or_else(bad)?;
        pos = end;
        if t == 0x004E_4942 {
            buffers.push(chunk.to_vec());
        }
    }
    Ok((header, buffers))
}

struct Reader<'a> {
    h: &'a Value,
    buffers: &'a [Vec<u8>],
}

impl Reader<'_> {
    /// Accessor values as f64 (flat, row after row) and the number of components per item.
    fn accessor(&self, i: usize) -> Result<(Vec<f64>, usize), ReadError> {
        let a = self.h["accessors"].get(i).ok_or_else(bad)?;
        let count = a["count"].as_u64().ok_or_else(bad)? as usize;
        let (size, kind): (usize, u64) = match a["componentType"].as_u64().ok_or_else(bad)? {
            5120 => (1, 0),
            5121 => (1, 1),
            5122 => (2, 2),
            5123 => (2, 3),
            5125 => (4, 4),
            5126 => (4, 5),
            _ => return Err(bad()),
        };
        let per = match a["type"].as_str().ok_or_else(bad)? {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" | "MAT2" => 4,
            "MAT3" => 9,
            "MAT4" => 16,
            _ => return Err(bad()),
        };
        let Some(bvi) = a.get("bufferView").and_then(|b| b.as_u64()) else {
            // No data: all zeros. Refuse absurd counts instead of allocating them.
            let n = count.checked_mul(per).filter(|&n| n <= 1 << 24).ok_or_else(bad)?;
            return Ok((vec![0.0; n], per));
        };
        let bv = self.h["bufferViews"].get(bvi as usize).ok_or_else(bad)?;
        let buf = self.buffers.get(bv["buffer"].as_u64().ok_or_else(bad)? as usize).ok_or_else(bad)?;
        let to_usize = |x: u64| usize::try_from(x).map_err(|_| bad());
        let bstart = to_usize(bv.get("byteOffset").and_then(|x| x.as_u64()).unwrap_or(0))?;
        let blen = to_usize(bv["byteLength"].as_u64().ok_or_else(bad)?)?;
        let bend = bstart.checked_add(blen).ok_or_else(bad)?;
        let view = buf.get(bstart..bend).ok_or_else(bad)?;
        let start = to_usize(a.get("byteOffset").and_then(|x| x.as_u64()).unwrap_or(0))?;
        let row = per * size;
        let stride = match bv.get("byteStride").and_then(|x| x.as_u64()) {
            Some(0) => return Err(bad()),
            Some(s) => to_usize(s)?,
            None => row,
        };
        // Every row starts at its own byte inside the view, so more rows than bytes is corrupt.
        if count > view.len() {
            return Err(bad());
        }
        let mut out = Vec::with_capacity(count * per);
        for r in 0..count {
            for c in 0..per {
                let o = r
                    .checked_mul(stride)
                    .and_then(|x| x.checked_add(start))
                    .and_then(|x| x.checked_add(c * size))
                    .ok_or_else(bad)?;
                let b = view.get(o..o.checked_add(size).ok_or_else(bad)?).ok_or_else(bad)?;
                out.push(match kind {
                    0 => b[0] as i8 as f64,
                    1 => b[0] as f64,
                    2 => i16::from_le_bytes([b[0], b[1]]) as f64,
                    3 => u16::from_le_bytes([b[0], b[1]]) as f64,
                    4 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64,
                    _ => f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64,
                });
            }
        }
        Ok((out, per))
    }
}

/// A geometry in the scene; `None` faces = a line or point primitive (counted, no faces).
struct Geom {
    v: Vec<f64>,
    f: Vec<u32>,
}

fn node_matrix(n: &Value) -> Result<Mat, ReadError> {
    let mut m = IDENTITY;
    if let Some(a) = n.get("matrix") {
        let v: Vec<f64> = a.as_array().ok_or_else(bad)?.iter().filter_map(|x| x.as_f64()).collect();
        if v.len() != 16 {
            return Err(bad());
        }
        for r in 0..4 {
            for c in 0..4 {
                m[r][c] = v[c * 4 + r];
            }
        }
    }
    let nums = |k: &str, len: usize| -> Result<Option<Vec<f64>>, ReadError> {
        match n.get(k) {
            None => Ok(None),
            Some(a) => {
                let v: Vec<f64> =
                    a.as_array().ok_or_else(bad)?.iter().filter_map(|x| x.as_f64()).collect();
                if v.len() != len {
                    return Err(bad());
                }
                Ok(Some(v))
            }
        }
    };
    if let Some(t) = nums("translation", 3)? {
        let mut tm = IDENTITY;
        for r in 0..3 {
            tm[r][3] = t[r];
        }
        m = mul(&m, &tm);
    }
    if let Some(q) = nums("rotation", 4)? {
        // glTF is xyzw; trimesh reorders to wxyz and normalises
        let q = [q[3], q[0], q[1], q[2]];
        let nn: f64 = q.iter().map(|x| x * x).sum();
        let mut rm = IDENTITY;
        if nn >= f64::EPSILON * 4.0 {
            let s = (2.0 / nn).sqrt();
            let q = [q[0] * s, q[1] * s, q[2] * s, q[3] * s];
            let o = |i: usize, j: usize| q[i] * q[j];
            rm = [
                [1.0 - o(2, 2) - o(3, 3), o(1, 2) - o(3, 0), o(1, 3) + o(2, 0), 0.0],
                [o(1, 2) + o(3, 0), 1.0 - o(1, 1) - o(3, 3), o(2, 3) - o(1, 0), 0.0],
                [o(1, 3) - o(2, 0), o(2, 3) + o(1, 0), 1.0 - o(1, 1) - o(2, 2), 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ];
        }
        m = mul(&m, &rm);
    }
    if let Some(s) = nums("scale", 3)? {
        let mut sm = IDENTITY;
        for r in 0..3 {
            sm[r][r] = s[r];
        }
        m = mul(&m, &sm);
    }
    Ok(m)
}

fn build(header: &Value, buffers: &[Vec<u8>], base: &str) -> Result<Vec<Part>, ReadError> {
    let rd = Reader { h: header, buffers };
    // geometry per glTF mesh index
    let mut geoms: Vec<Geom> = Vec::new();
    let mut mesh_prim: Vec<Vec<usize>> = Vec::new();
    for m in header.get("meshes").and_then(|m| m.as_array()).map(|a| a.as_slice()).unwrap_or(&[]) {
        let mut ids = Vec::new();
        for p in m["primitives"].as_array().ok_or_else(bad)? {
            if let Some(ext) = p.get("extensions").and_then(|e| e.as_object()) {
                if ext.contains_key("KHR_draco_mesh_compression")
                    || ext.contains_key("EXT_meshopt_compression")
                {
                    return Err(ReadError(
                        "This glTF file uses compressed meshes, which can't be read.".into(),
                    ));
                }
            }
            let mode = p.get("mode").and_then(|x| x.as_u64()).unwrap_or(4);
            match mode {
                0 | 1 => {
                    geoms.push(Geom { v: Vec::new(), f: Vec::new() });
                    ids.push(geoms.len() - 1);
                }
                4 | 5 => {
                    let pos = p["attributes"]["POSITION"].as_u64().ok_or_else(bad)? as usize;
                    let (v, per) = rd.accessor(pos)?;
                    if per != 3 {
                        return Err(bad());
                    }
                    let nv = v.len() / 3;
                    let flat: Vec<u32> = match p.get("indices").and_then(|x| x.as_u64()) {
                        Some(i) => rd.accessor(i as usize)?.0.iter().map(|&x| x as u32).collect(),
                        None => (0..nv as u32).collect(),
                    };
                    let f: Vec<u32> = if mode == 5 {
                        let mut f = Vec::new();
                        for i in 0..flat.len().saturating_sub(2) {
                            if i % 2 == 0 {
                                f.extend([flat[i], flat[i + 1], flat[i + 2]]);
                            } else {
                                f.extend([flat[i + 2], flat[i + 1], flat[i]]);
                            }
                        }
                        f
                    } else {
                        if flat.len() % 3 != 0 {
                            return Err(bad());
                        }
                        flat
                    };
                    if f.iter().any(|&i| i as usize >= nv) {
                        return Err(bad());
                    }
                    geoms.push(Geom { v, f });
                    ids.push(geoms.len() - 1);
                }
                _ => {} // line loops / strips and fans are skipped
            }
        }
        mesh_prim.push(ids);
    }

    let nodes = header.get("nodes").and_then(|n| n.as_array()).cloned().unwrap_or_default();
    let scene_index = header.get("scene").and_then(|s| s.as_u64()).unwrap_or(0) as usize;
    let mut queue: Vec<(Option<usize>, usize)> = Vec::new();
    if let Some(scenes) = header.get("scenes").and_then(|s| s.as_array()) {
        let sc = scenes.get(scene_index).ok_or_else(bad)?;
        for r in sc.get("nodes").and_then(|n| n.as_array()).map(|a| a.as_slice()).unwrap_or(&[]) {
            queue.push((None, r.as_u64().ok_or_else(bad)? as usize));
        }
    }
    let mut consumed: HashSet<(Option<usize>, usize)> = HashSet::new();
    let mut world: Vec<Mat> = vec![IDENTITY; nodes.len()];
    let mut have_camera = false;
    // (geometry, world matrix) in dump order
    let mut dump: Vec<(usize, Mat)> = Vec::new();
    while let Some(edge) = queue.pop() {
        if !consumed.insert(edge) {
            continue;
        }
        let (a, b) = edge;
        let child = nodes.get(b).ok_or_else(bad)?;
        if let Some(cs) = child.get("children").and_then(|c| c.as_array()) {
            for c in cs {
                queue.push((Some(b), c.as_u64().ok_or_else(bad)? as usize));
            }
        }
        let local = node_matrix(child)?;
        let parent = a.map(|a| world[a]).unwrap_or(IDENTITY);
        world[b] = mul(&parent, &local);
        // trimesh skips every node with a camera until one full perspective camera is taken
        if !have_camera {
            if let Some(ci) = child.get("camera").and_then(|c| c.as_u64()) {
                let cam = &header["cameras"][ci as usize]["perspective"];
                if cam.get("znear").is_some() && cam.get("aspectRatio").is_some() && cam.get("yfov").is_some() {
                    have_camera = true;
                }
                continue;
            }
        }
        if let Some(mi) = child.get("mesh").and_then(|m| m.as_u64()) {
            let ids = mesh_prim.get(mi as usize).ok_or_else(bad)?;
            for &g in ids {
                dump.push((g, world[b]));
            }
        }
    }

    let n = dump.len();
    let mut parts = Vec::new();
    for (i, (g, tf)) in dump.into_iter().enumerate() {
        let geom = &geoms[g];
        if geom.f.is_empty() {
            continue;
        }
        let mut v = Vec::with_capacity(geom.v.len());
        for p in geom.v.chunks_exact(3) {
            for r in 0..3 {
                v.push(p[0] * tf[r][0] + p[1] * tf[r][1] + p[2] * tf[r][2] + tf[r][3]);
            }
        }
        let name = if n == 1 { base.to_string() } else { format!("{base} {}", i + 1) };
        parts.push(Part { name, positions: v, triangles: geom.f.clone() });
    }
    Ok(parts)
}

/// See the contract in `read/mod.rs`.
pub fn read(data: &[u8], base: &str) -> Result<Vec<Part>, ReadError> {
    if data.starts_with(b"glTF") {
        let (header, buffers) = read_glb(data)?;
        return build(&header, &buffers, base);
    }
    let header: Value = serde_json::from_slice(data).map_err(|_| bad())?;
    let version = header.get("asset").and_then(|a| a.get("version"));
    let major = match version {
        Some(Value::String(s)) => s.split('.').next().and_then(|m| m.parse::<i64>().ok()).ok_or_else(bad)?,
        Some(v) => v.as_f64().map(|f| f as i64).ok_or_else(bad)?,
        None => 2,
    };
    if major < 2 {
        return Err(ReadError("Only glTF 2 files are supported.".into()));
    }
    let mut buffers = Vec::new();
    for b in header.get("buffers").and_then(|b| b.as_array()).map(|a| a.as_slice()).unwrap_or(&[]) {
        let uri = b.get("uri").and_then(|u| u.as_str()).ok_or_else(bad)?;
        buffers.push(uri_bytes(uri)?);
    }
    build(&header, &buffers, base)
}
