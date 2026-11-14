//! What the viewport draws, on the CPU: body meshes prepared for the GPU (1.x's creased
//! normals and feature edges), and the scene's grid, origin axes and planes. All constants are
//! `web/js/viewer.js`'s.

use std::collections::HashMap;

use stl_kernel::{Aabb, MeshArrays};

use super::camera::{self, V3};

// ---------------------------------------------------------------------- viewer.js constants

/// `SEL`, the selection's emissive colour, and its strength in `_styleBody`.
pub const SEL: u32 = 0x1f6fd1;
pub const SEL_STRENGTH: f32 = 0.32;
/// `HOV`, the hover's emissive colour, and its strength.
pub const HOV: u32 = 0x4b8fe0;
pub const HOV_STRENGTH: f32 = 0.16;
pub const FACE_HL: u32 = 0x2f8cff;
pub const PLANE_COL: u32 = 0xe8763a;
pub const SKETCH_COL: u32 = 0xd9480f;
/// Active sketch stroke, `colDef()` in `sketcher.js` (`--def-full`).
pub const SKETCH_DEF_DARK: u32 = 0xe9ecef;
pub const SKETCH_DEF_LIGHT: u32 = 0x1d2433;
/// `COL_UNDER`, `COL_BAD`, `COL_SEL`, `COL_HOVER`, `COL_PREV`, `COL_EDGE` in `sketcher.js`.
pub const SKETCH_UNDER: u32 = 0x1c6fd6;
pub const SKETCH_BAD: u32 = 0xe03131;
pub const SKETCH_SEL: u32 = 0x0ca678;
pub const SKETCH_HOVER: u32 = 0x38c99a;
pub const SKETCH_PREVIEW: u32 = 0xe8590c;
pub const SKETCH_EDGE: u32 = 0xf76707;
pub const BASE_XY: u32 = 0x3b82f6;
pub const BASE_XZ: u32 = 0x22a35a;
pub const BASE_YZ: u32 = 0xe25555;
/// The colour of a body that isn't solid (`#d9a0a0`).
pub const NOT_SOLID: u32 = 0xd9a0a0;
/// Server PALETTE, for a body that comes without a colour (`stleditor/server.py`).
pub const PALETTE: [u32; 8] = [0x9aa7b4, 0xc9a66b, 0x7fa8c9, 0xa3c48a, 0xc98a8a, 0xa993c9, 0xc9b98a, 0x8ac9c0];

/// `toCreasedNormals(geo, 30°)`.
pub const CREASE_DEG: f64 = 30.0;
/// `new EdgesGeometry(geo, 32)`.
pub const EDGE_DEG: f64 = 32.0;
/// Bodies of more triangles are flat shaded, without creased normals.
pub const FLAT_ABOVE: usize = 1_500_000;
/// Bodies of fewer triangles get feature edges.
pub const EDGES_BELOW: usize = 400_000;
/// Edge colour: `setTheme` sets dark ? 0x0d1013 : 0x2b333c, opacity 0.55 (0.15 dimmed).
pub const EDGE_DARK: u32 = 0x0d1013;
pub const EDGE_LIGHT: u32 = 0x2b333c;
pub const EDGE_OPACITY: f32 = 0.55;
pub const EDGE_OPACITY_DIM: f32 = 0.15;
/// `setBodiesDim`'s opacity.
pub const DIM_OPACITY: f32 = 0.18;
/// MeshStandardMaterial of bodies.
pub const METALNESS: f32 = 0.05;
pub const ROUGHNESS: f32 = 0.62;
/// Lights: hemisphere (sky, ground, intensity) and the two camera lights (offset, intensity).
pub const HEMI_SKY: u32 = 0xffffff;
pub const HEMI_GROUND: u32 = 0x8899aa;
pub const HEMI_INTENSITY: f32 = 1.6;
pub const KEY_OFFSET: V3 = [0.5, 0.3, 1.0];
pub const KEY_INTENSITY: f32 = 1.8;
pub const FILL_OFFSET: V3 = [-1.0, -0.4, 0.2];
pub const FILL_INTENSITY: f32 = 0.6;

// ---------------------------------------------------------------------- colours

pub fn rgb(hex: u32) -> [u8; 3] {
    [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8]
}

/// `#rrggbb` or `#rgb`.
pub fn parse_css_hex(s: &str) -> Option<[u8; 3]> {
    let h = s.trim().strip_prefix('#')?;
    let v = |t: &str| u8::from_str_radix(t, 16).ok();
    match h.len() {
        6 => Some([v(&h[0..2])?, v(&h[2..4])?, v(&h[4..6])?]),
        3 => {
            let d = |i: usize| v(&h[i..i + 1]).map(|x| x * 17);
            Some([d(0)?, d(1)?, d(2)?])
        }
        _ => None,
    }
}

pub fn srgb_to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c < 0.04045 { c * 0.0773993808 } else { (c * 0.9478672986 + 0.0521327014).powf(2.4) }
}

pub fn linear(c: [u8; 3]) -> [f32; 3] {
    c.map(srgb_to_linear)
}

/// sRGB-encoded colour with an alpha, for the flat (unlit) shader, which writes as is like
/// three's MeshBasicMaterial / LineBasicMaterial do.
pub fn rgba(hex: u32, alpha: f32) -> [u8; 4] {
    let [r, g, b] = rgb(hex);
    [r, g, b, (alpha.clamp(0.0, 1.0) * 255.0).round() as u8]
}

// ---------------------------------------------------------------------- vertices

/// A body vertex: position and creased normal (zero when the body is flat shaded).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LitVertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
}

/// An unlit vertex. The fragment is `c0` where the interpolated `t` is under 0.5 and `c1`
/// beyond (`addDistTris`'s two-texel texture); everything else has `c0 == c1`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FlatVertex {
    pub pos: [f32; 3],
    pub c0: [u8; 4],
    pub c1: [u8; 4],
    pub t: f32,
}

impl FlatVertex {
    pub fn new(p: V3, c: [u8; 4]) -> Self {
        FlatVertex { pos: [p[0] as f32, p[1] as f32, p[2] as f32], c0: c, c1: c, t: 0.0 }
    }
}

pub fn as_bytes<T: Copy>(v: &[T]) -> &[u8] {
    // SAFETY: only used on the `repr(C)` vertex structs above, u32 and f32 arrays: plain old
    // data without padding bytes.
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

// ---------------------------------------------------------------------- bodies

/// A body mesh ready for the GPU.
#[derive(Clone, Debug)]
pub struct PreparedMesh {
    /// Three per triangle (creased), or the mesh's own vertices (flat, with `indices`).
    pub vertices: Vec<LitVertex>,
    pub indices: Option<Vec<u32>>,
    pub flat: bool,
    /// Feature edges, two points per segment (`None` above [`EDGES_BELOW`] triangles).
    pub edges: Option<Vec<[f32; 3]>>,
    pub bbox: Aabb,
    pub triangles: usize,
}

pub fn mesh_box(m: &MeshArrays) -> Aabb {
    let mut b = camera::empty_box();
    for p in m.positions.chunks_exact(3) {
        for k in 0..3 {
            b.min[k] = b.min[k].min(p[k] as f64);
            b.max[k] = b.max[k].max(p[k] as f64);
        }
    }
    b
}

/// 1.x `_makeBody`'s display geometry.
pub fn prepare_mesh(m: &MeshArrays) -> PreparedMesh {
    let ntri = m.triangles.len() / 3;
    let flat = ntri > FLAT_ABOVE;
    let (vertices, indices) = if flat {
        let v = m.positions.chunks_exact(3).map(|p| LitVertex { pos: [p[0], p[1], p[2]], normal: [0.0; 3] }).collect();
        (v, Some(m.triangles[..ntri * 3].to_vec()))
    } else {
        (creased_normals(m, CREASE_DEG), None)
    };
    let edges = if ntri < EDGES_BELOW { Some(feature_edges(m, EDGE_DEG)) } else { None };
    PreparedMesh { vertices, indices, flat, edges, bbox: mesh_box(m), triangles: ntri }
}

fn corner(m: &MeshArrays, i: u32) -> V3 {
    let i = i as usize * 3;
    [m.positions[i] as f64, m.positions[i + 1] as f64, m.positions[i + 2] as f64]
}

/// `Triangle.getNormal`: (c - b) x (a - b), normalised (zero for a degenerate triangle).
fn face_normal(a: V3, b: V3, c: V3) -> V3 {
    camera::normalize(camera::cross(camera::sub(c, b), camera::sub(a, b)))
}

/// three's `toCreasedNormals`: non-indexed vertices whose normal is the sum of the face normals
/// meeting at that spot (positions hashed to 0.01 mm) within `crease_deg` of the face's own.
pub fn creased_normals(m: &MeshArrays, crease_deg: f64) -> Vec<LitVertex> {
    let ntri = m.triangles.len() / 3;
    let crease_dot = crease_deg.to_radians().cos();
    let mult = (1.0 + 1e-10) * 1e2;
    // `~~(v * mult)`: truncate to a 32-bit integer
    let hash = |p: V3| -> (i32, i32, i32) { ((p[0] * mult) as i32, (p[1] * mult) as i32, (p[2] * mult) as i32) };
    let mut normals = Vec::with_capacity(ntri);
    let mut group_of = Vec::with_capacity(ntri * 3);
    let mut groups: HashMap<(i32, i32, i32), u32> = HashMap::with_capacity(m.positions.len() / 3);
    for t in 0..ntri {
        let p = [0, 1, 2].map(|j| corner(m, m.triangles[t * 3 + j]));
        normals.push(face_normal(p[0], p[1], p[2]));
        for q in p {
            let n = groups.len() as u32;
            group_of.push(*groups.entry(hash(q)).or_insert(n));
        }
    }
    // corners of each group (CSR), in triangle order like the JS arrays
    let ng = groups.len();
    let mut start = vec![0u32; ng + 1];
    for &g in &group_of {
        start[g as usize + 1] += 1;
    }
    for i in 0..ng {
        start[i + 1] += start[i];
    }
    let mut fill = start.clone();
    let mut faces = vec![0u32; group_of.len()];
    for (c, &g) in group_of.iter().enumerate() {
        faces[fill[g as usize] as usize] = (c / 3) as u32;
        fill[g as usize] += 1;
    }
    let mut out = Vec::with_capacity(ntri * 3);
    for t in 0..ntri {
        let n = normals[t];
        for j in 0..3 {
            let g = group_of[t * 3 + j] as usize;
            let mut sum = [0.0; 3];
            for &f in &faces[start[g] as usize..start[g + 1] as usize] {
                let o = normals[f as usize];
                if camera::dot(n, o) > crease_dot {
                    sum = camera::add(sum, o);
                }
            }
            let s = camera::normalize(sum);
            let p = corner(m, m.triangles[t * 3 + j]);
            out.push(LitVertex { pos: [p[0] as f32, p[1] as f32, p[2] as f32], normal: [s[0] as f32, s[1] as f32, s[2] as f32] });
        }
    }
    out
}

/// three's `EdgesGeometry(geo, threshold_deg)`: edges between faces whose normals differ by at
/// least the threshold, and open edges (positions matched to 0.0001 mm).
pub fn feature_edges(m: &MeshArrays, threshold_deg: f64) -> Vec<[f32; 3]> {
    let ntri = m.triangles.len() / 3;
    let threshold_dot = threshold_deg.to_radians().cos();
    // `Math.round(x * 1e4)`
    let key = |p: V3| -> [i64; 3] { p.map(|v| (v * 1e4 + 0.5).floor() as i64) };
    struct Slot {
        i0: u32,
        i1: u32,
        normal: V3,
        open: bool,
    }
    let mut map: HashMap<([i64; 3], [i64; 3]), Slot> = HashMap::with_capacity(ntri * 2);
    let mut out: Vec<[f32; 3]> = Vec::new();
    let f = |p: V3| [p[0] as f32, p[1] as f32, p[2] as f32];
    for t in 0..ntri {
        let idx = [m.triangles[t * 3], m.triangles[t * 3 + 1], m.triangles[t * 3 + 2]];
        let p = idx.map(|i| corner(m, i));
        let n = face_normal(p[0], p[1], p[2]);
        let h = p.map(key);
        if h[0] == h[1] || h[1] == h[2] || h[2] == h[0] {
            continue;
        }
        for j in 0..3 {
            let k = (j + 1) % 3;
            if let Some(s) = map.get_mut(&(h[k], h[j])).filter(|s| s.open) {
                if camera::dot(n, s.normal) <= threshold_dot {
                    out.push(f(p[j]));
                    out.push(f(p[k]));
                }
                s.open = false;
            } else {
                map.entry((h[j], h[k])).or_insert(Slot { i0: idx[j], i1: idx[k], normal: n, open: true });
            }
        }
    }
    for s in map.values().filter(|s| s.open) {
        out.push(f(corner(m, s.i0)));
        out.push(f(corner(m, s.i1)));
    }
    out
}

// ---------------------------------------------------------------------- origin, grid, planes

/// `_buildOrigin`'s size step: the power of ten at or above the scene's extent.
pub fn origin_size(extent: f64) -> f64 {
    10f64.powf(extent.max(10.0).log10().ceil())
}

/// three's `GridHelper(size, divisions, c1, c2)` turned to lie in the XY plane.
fn grid_helper(out: &mut Vec<FlatVertex>, size: f64, divisions: usize, c_center: [u8; 4], c_line: [u8; 4]) {
    let center = divisions / 2;
    let step = size / divisions as f64;
    let half = size / 2.0;
    for i in 0..=divisions {
        let k = -half + i as f64 * step;
        let c = if i == center { c_center } else { c_line };
        // GridHelper's (x, 0, z) lines, rotated 90° about X: (x, -z, 0)
        out.push(FlatVertex::new([-half, -k, 0.0], c));
        out.push(FlatVertex::new([half, -k, 0.0], c));
        out.push(FlatVertex::new([k, half, 0.0], c));
        out.push(FlatVertex::new([k, -half, 0.0], c));
    }
}

/// The grid's lines: the minor grid then the major one, as `_buildOrigin` adds them.
pub fn grid_lines(s: f64, dark: bool) -> Vec<FlatVertex> {
    let mut v = Vec::new();
    let (minor, mo) = if dark { (0x3a3f49, 0.6) } else { (0xd2d7de, 0.5) };
    grid_helper(&mut v, s * 2.0, 200, rgba(minor, mo), rgba(minor, mo));
    let (c1, c2, o) = if dark { (0x515866, 0x454b56, 0.9) } else { (0x9aa5b1, 0xbcc3cc, 0.6) };
    grid_helper(&mut v, s * 2.0, 20, rgba(c1, o), rgba(c2, o));
    v
}

/// The X, Y and Z axis lines, `s * 0.35` long, drawn on top of everything.
pub fn axis_lines(s: f64) -> Vec<FlatVertex> {
    let l = s * 0.35;
    let mut v = Vec::new();
    for (d, c) in [([1.0, 0.0, 0.0], 0xe25555), ([0.0, 1.0, 0.0], 0x22a35a), ([0.0, 0.0, 1.0], 0x3b82f6)] {
        v.push(FlatVertex::new([0.0; 3], rgba(c, 0.9)));
        v.push(FlatVertex::new(camera::scale(d, l), rgba(c, 0.9)));
    }
    v
}

/// A plane frame: origin, x, y and normal.
pub type Frame = [V3; 4];

pub const BASE_FRAMES: [(&str, Frame, u32); 3] = [
    ("XY", [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], BASE_XY),
    ("XZ", [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]], BASE_XZ),
    ("YZ", [[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]], BASE_YZ),
];

/// `_planeMesh`: a `size` square on the frame (fill at `fill_alpha`, border at 0.7), as
/// (triangles, border lines).
pub fn plane_mesh(frame: &Frame, size: f64, color: u32, fill_alpha: f32) -> (Vec<FlatVertex>, Vec<FlatVertex>) {
    let [o, x, y, _] = *frame;
    let h = size / 2.0;
    let at = |u: f64, v: f64| camera::add(o, camera::add(camera::scale(x, u), camera::scale(y, v)));
    let q = [at(-h, -h), at(h, -h), at(h, h), at(-h, h)];
    let fc = rgba(color, fill_alpha);
    let tris = [0, 1, 2, 0, 2, 3].iter().map(|&i| FlatVertex::new(q[i], fc)).collect();
    let lc = rgba(color, 0.7);
    let lines = [0, 1, 1, 2, 2, 3, 3, 0].iter().map(|&i| FlatVertex::new(q[i], lc)).collect();
    (tris, lines)
}

/// `_buildPlaneGrid` (`sketcher.js`): sheet, grid lines (minor then major) and the two axes.
/// `scene_diagonal` is `sceneBox().getSize().length()`; empty is 100, otherwise at least 40.
/// Returns (sheet triangles, grid lines, axis lines).
pub fn sketch_plane_grid(frame: &Frame, scene_diagonal: Option<f64>) -> (Vec<FlatVertex>, Vec<FlatVertex>, Vec<FlatVertex>) {
    let r = match scene_diagonal {
        Some(d) if d.is_finite() && d > 0.0 => d.max(40.0),
        _ => 100.0,
    };
    let step = if r > 400.0 { 10.0 } else if r > 80.0 { 5.0 } else { 1.0 };
    let half = (r / step).ceil() * step;
    let [o, x, y, _] = *frame;
    let at = |u: f64, v: f64| camera::add(o, camera::add(camera::scale(x, u), camera::scale(y, v)));
    let count = (2.0 * half / step).round() as i32;
    let mut lines = Vec::new();
    let mut major = Vec::new();
    let minor_c = rgba(0xc58a68, 0.16);
    let major_c = rgba(0xc58a68, 0.32);
    for i in 0..=count {
        let v = -half + i as f64 * step;
        let dest_major = (v / step).round() as i32 % 5 == 0;
        let (buf, c) = if dest_major { (&mut major, major_c) } else { (&mut lines, minor_c) };
        buf.push(FlatVertex::new(at(-half, v), c));
        buf.push(FlatVertex::new(at(half, v), c));
        buf.push(FlatVertex::new(at(v, -half), c));
        buf.push(FlatVertex::new(at(v, half), c));
    }
    lines.extend(major);
    let mut axes = Vec::new();
    let red = rgba(0xe25555, 1.0);
    let green = rgba(0x22a35a, 1.0);
    axes.push(FlatVertex::new(at(-half, 0.0), red));
    axes.push(FlatVertex::new(at(half, 0.0), red));
    axes.push(FlatVertex::new(at(0.0, -half), green));
    axes.push(FlatVertex::new(at(0.0, half), green));
    let sheet_c = rgba(0xfff1e8, 0.18);
    let q = [at(-half, -half), at(half, -half), at(half, half), at(-half, half)];
    let sheet = [0, 1, 2, 0, 2, 3].into_iter().map(|i| FlatVertex::new(q[i], sheet_c)).collect();
    (sheet, lines, axes)
}

/// `toCreasedNormals`, a slice at a time. [`creased_normals`] is the same work in one go;
/// `tests/flyin.rs` checks the two agree.
pub struct CreaseBuild {
    ntri: usize,
    crease_dot: f64,
    phase: u8,
    next: usize,
    normals: Vec<V3>,
    group_of: Vec<u32>,
    groups: HashMap<(i32, i32, i32), u32>,
    start: Vec<u32>,
    faces: Vec<u32>,
    out: Vec<LitVertex>,
}

impl CreaseBuild {
    pub fn new(m: &MeshArrays, crease_deg: f64) -> Self {
        let ntri = m.triangles.len() / 3;
        CreaseBuild {
            ntri,
            crease_dot: crease_deg.to_radians().cos(),
            phase: 0,
            next: 0,
            normals: Vec::with_capacity(ntri),
            group_of: Vec::with_capacity(ntri * 3),
            groups: HashMap::with_capacity(m.positions.len() / 3),
            start: Vec::new(),
            faces: Vec::new(),
            out: Vec::new(),
        }
    }

    /// Work until `budget` is used (`None`: to the end). The vertices when it has finished.
    pub fn step(&mut self, m: &MeshArrays, budget: Option<std::time::Duration>) -> Option<Vec<LitVertex>> {
        let t0 = web_time::Instant::now();
        let mut checks = 0u32;
        let mult = (1.0 + 1e-10) * 1e2;
        let hash = |p: V3| -> (i32, i32, i32) { ((p[0] * mult) as i32, (p[1] * mult) as i32, (p[2] * mult) as i32) };
        loop {
            if let Some(b) = budget {
                checks += 1;
                if checks % 1024 == 0 && t0.elapsed() >= b {
                    return None;
                }
            }
            match self.phase {
                0 => {
                    if self.next >= self.ntri {
                        self.phase = 1;
                        continue;
                    }
                    let t = self.next;
                    self.next += 1;
                    let p = [0, 1, 2].map(|j| corner(m, m.triangles[t * 3 + j]));
                    self.normals.push(face_normal(p[0], p[1], p[2]));
                    for q in p {
                        let n = self.groups.len() as u32;
                        self.group_of.push(*self.groups.entry(hash(q)).or_insert(n));
                    }
                }
                1 => {
                    let ng = self.groups.len();
                    let mut start = vec![0u32; ng + 1];
                    for &g in &self.group_of {
                        start[g as usize + 1] += 1;
                    }
                    for i in 0..ng {
                        start[i + 1] += start[i];
                    }
                    let mut fill = start.clone();
                    let mut faces = vec![0u32; self.group_of.len()];
                    for (c, &g) in self.group_of.iter().enumerate() {
                        faces[fill[g as usize] as usize] = (c / 3) as u32;
                        fill[g as usize] += 1;
                    }
                    self.start = start;
                    self.faces = faces;
                    self.groups.clear();
                    self.phase = 2;
                    self.next = 0;
                    if budget.is_some_and(|b| t0.elapsed() >= b) {
                        return None;
                    }
                }
                _ => {
                    if self.next >= self.ntri {
                        return Some(std::mem::take(&mut self.out));
                    }
                    let t = self.next;
                    self.next += 1;
                    let n = self.normals[t];
                    for j in 0..3 {
                        let g = self.group_of[t * 3 + j] as usize;
                        let mut sum = [0.0; 3];
                        for &f in &self.faces[self.start[g] as usize..self.start[g + 1] as usize] {
                            let o = self.normals[f as usize];
                            if camera::dot(n, o) > self.crease_dot {
                                sum = camera::add(sum, o);
                            }
                        }
                        let s = camera::normalize(sum);
                        let p = corner(m, m.triangles[t * 3 + j]);
                        self.out.push(LitVertex {
                            pos: [p[0] as f32, p[1] as f32, p[2] as f32],
                            normal: [s[0] as f32, s[1] as f32, s[2] as f32],
                        });
                    }
                }
            }
        }
    }
}
