//! The viewer's overlay calls (`highlightFace`, `addMarker`, `addArrow`, `addLine`,
//! `addPolyline`, `setBand`, `addPlaneGhost`, `setBrush`, `drawStrokes`, `addRegions`,
//! `addDistTris`, `sketchLines`), as unlit geometry. Sizes, colours, opacities and draw order
//! are `web/js/viewer.js`'s; `order` is three's `renderOrder`.

use stl_kernel::MeshArrays;

use super::camera::{self, V3};
use super::scene::{self, rgba, FlatVertex, Frame};

/// How an overlay meets the depth buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Depth {
    /// Hidden behind the part, no depth write (planes).
    Test,
    /// As `Test`, pulled towards the camera (`polygonOffset -2`: faces painted on the part).
    TestOffset,
    /// Drawn over everything (`depthTest: false`).
    Off,
}

/// One overlay object: triangles and/or lines with their colours.
#[derive(Clone, Debug)]
pub struct OverlayItem {
    pub tag: String,
    pub order: i32,
    pub depth: Depth,
    pub tris: Vec<FlatVertex>,
    pub lines: Vec<FlatVertex>,
}

impl OverlayItem {
    fn new(tag: &str, order: i32, depth: Depth) -> Self {
        OverlayItem { tag: tag.to_owned(), order, depth, tris: Vec::new(), lines: Vec::new() }
    }
}

/// A sketch entity's outline in plane coordinates (`entityOutline`'s `pts` and `closed`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SketchOutline {
    pub pts: Vec<[f64; 2]>,
    pub closed: bool,
    pub construction: bool,
}

/// `highlightFace`: some triangles of a body, opacity 0.45, both sides.
pub fn highlight_face(mesh: &MeshArrays, tris: &[u32], tag: &str, color: u32) -> OverlayItem {
    let mut it = OverlayItem::new(tag, 3, Depth::TestOffset);
    let c = rgba(color, 0.45);
    let n = mesh.triangles.len() / 3;
    for &t in tris.iter().filter(|&&t| (t as usize) < n) {
        for j in 0..3 {
            let v = mesh.triangles[t as usize * 3 + j] as usize * 3;
            let p = &mesh.positions[v..v + 3];
            it.tris.push(FlatVertex { pos: [p[0], p[1], p[2]], c0: c, c1: c, t: 0.0 });
        }
    }
    it
}

/// `addDistTris`: triangles (flat positions) painted by a distance per corner: `near` up to
/// `limit`, `far` beyond, with a clean edge between.
pub fn dist_tris(arr: &[f32], dist: &[f64], limit: f64, tag: &str, near: (u32, f32), far: (u32, f32)) -> OverlayItem {
    let mut it = OverlayItem::new(tag, 3, Depth::TestOffset);
    let (c0, c1) = (rgba(near.0, near.1), rgba(far.0, far.1));
    for (i, p) in arr.chunks_exact(3).enumerate() {
        let d = dist.get(i).copied().unwrap_or(0.0);
        let t = (d / (2.0 * limit)).min(1.0) as f32;
        it.tris.push(FlatVertex { pos: [p[0], p[1], p[2]], c0, c1, t });
    }
    let whole = it.tris.len() / 3 * 3;
    it.tris.truncate(whole);
    it
}

/// three's `SphereGeometry(radius, 16, 12)` at `p`.
fn sphere(out: &mut Vec<FlatVertex>, p: V3, r: f64, c: [u8; 4]) {
    let (ws, hs) = (16usize, 12usize);
    let at = |ix: usize, iy: usize| {
        let u = ix as f64 / ws as f64;
        let v = iy as f64 / hs as f64;
        let (phi, theta) = (u * std::f64::consts::TAU, v * std::f64::consts::PI);
        camera::add(p, [-r * phi.cos() * theta.sin(), r * theta.cos(), r * phi.sin() * theta.sin()])
    };
    for iy in 0..hs {
        for ix in 0..ws {
            let (a, b, cc, d) = (at(ix + 1, iy), at(ix, iy), at(ix, iy + 1), at(ix + 1, iy + 1));
            if iy != 0 {
                out.extend([a, b, d].map(|q| FlatVertex::new(q, c)));
            }
            if iy != hs - 1 {
                out.extend([b, cc, d].map(|q| FlatVertex::new(q, c)));
            }
        }
    }
}

/// `addMarker`: a dot, `max(extent * 0.006, 0.3)` across unless `size` is given.
pub fn marker(p: V3, color: u32, tag: &str, size: Option<f64>, extent: f64) -> OverlayItem {
    let s = size.unwrap_or_else(|| (extent * 0.006).max(0.3));
    let mut it = OverlayItem::new(tag, 20, Depth::Off);
    sphere(&mut it.tris, p, s, rgba(color, 1.0));
    it
}

/// Any unit vector square to `d`.
fn perpendicular(d: V3) -> (V3, V3) {
    let a = if d[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let u = camera::normalize(camera::cross(d, a));
    (u, camera::cross(d, u))
}

/// `addArrow`: three's `ArrowHelper(dir, p, len, color, len * 0.25, len * 0.12)`: a line and a
/// five-sided cone.
pub fn arrow(p: V3, dir: V3, len: f64, color: u32, tag: &str) -> OverlayItem {
    let mut it = OverlayItem::new(tag, 20, Depth::Off);
    let d = camera::normalize(dir);
    let (head_len, head_w) = (len * 0.25, len * 0.12);
    let c = rgba(color, 1.0);
    let base = camera::add(p, camera::scale(d, len - head_len));
    it.lines.push(FlatVertex::new(p, c));
    it.lines.push(FlatVertex::new(camera::add(p, camera::scale(d, (len - head_len).max(1e-4))), c));
    let tip = camera::add(p, camera::scale(d, len));
    let (u, v) = perpendicular(d);
    let r = head_w * 0.5;
    let rim: Vec<V3> = (0..5)
        .map(|i| {
            let a = i as f64 / 5.0 * std::f64::consts::TAU;
            camera::add(base, camera::add(camera::scale(u, r * a.sin()), camera::scale(v, r * a.cos())))
        })
        .collect();
    for i in 0..5 {
        let (a, b) = (rim[i], rim[(i + 1) % 5]);
        it.tris.extend([tip, a, b].map(|q| FlatVertex::new(q, c)));
        it.tris.extend([base, b, a].map(|q| FlatVertex::new(q, c)));
    }
    it
}

/// `addLine`.
pub fn line(a: V3, b: V3, color: u32, tag: &str) -> OverlayItem {
    let mut it = OverlayItem::new(tag, 20, Depth::Off);
    let c = rgba(color, 1.0);
    it.lines.push(FlatVertex::new(a, c));
    it.lines.push(FlatVertex::new(b, c));
    it
}

fn strip(out: &mut Vec<FlatVertex>, pts: &[V3], c: [u8; 4]) {
    for w in pts.windows(2) {
        out.push(FlatVertex::new(w[0], c));
        out.push(FlatVertex::new(w[1], c));
    }
}

/// `addPolyline` (nothing for fewer than two points).
pub fn polyline(points: &[V3], color: u32, tag: &str) -> Option<OverlayItem> {
    if points.len() < 2 {
        return None;
    }
    let mut it = OverlayItem::new(tag, 21, Depth::Off);
    strip(&mut it.lines, points, rgba(color, 1.0));
    Some(it)
}

/// `setBand`: the rubber band (up to 130 points) and a dot at the cursor, 5 pixels across.
pub fn band(points: &[V3], cursor: Option<(V3, f64)>, color: u32) -> Vec<OverlayItem> {
    let mut out = Vec::new();
    let pts = &points[..points.len().min(130)];
    if pts.len() >= 2 {
        let mut it = OverlayItem::new("\u{0}band", 22, Depth::Off);
        strip(&mut it.lines, pts, rgba(color, 1.0));
        out.push(it);
    }
    if let Some((p, pixel)) = cursor {
        let mut it = OverlayItem::new("\u{0}cursor", 23, Depth::Off);
        sphere(&mut it.tris, p, pixel * 5.0, rgba(0xcffbe4, 0.75));
        out.push(it);
    }
    out
}

/// `addPlaneGhost`: a plane at opacity 0.22, `max(extent * 0.35, 20)` across.
pub fn plane_ghost(frame: &Frame, tag: &str, color: u32, extent: f64) -> OverlayItem {
    let (tris, lines) = scene::plane_mesh(frame, (extent * 0.35).max(20.0), color, 0.22);
    OverlayItem { tag: tag.to_owned(), order: 2, depth: Depth::Test, tris, lines }
}

/// `setBrush`: a ring of `radius` facing `normal`, drawn over everything.
pub fn brush(point: V3, normal: V3, radius: f64) -> OverlayItem {
    let mut it = OverlayItem::new("\u{0}brush", 40, Depth::Off);
    let n = camera::normalize(normal);
    let (u, v) = perpendicular(n);
    let c = rgba(0xe8763a, 0.9);
    let at = |r: f64, a: f64| camera::add(point, camera::scale(camera::add(camera::scale(u, a.cos()), camera::scale(v, a.sin())), r * radius));
    for i in 0..64 {
        let (a0, a1) = (i as f64 / 64.0 * std::f64::consts::TAU, (i + 1) as f64 / 64.0 * std::f64::consts::TAU);
        let (p0, p1, p2, p3) = (at(0.94, a0), at(1.0, a0), at(1.0, a1), at(0.94, a1));
        it.tris.extend([p0, p1, p2, p0, p2, p3].map(|q| FlatVertex::new(q, c)));
    }
    it
}

/// A stroke of `drawStrokes`.
#[derive(Clone, Debug, Default)]
pub struct Stroke {
    pub mode: String,
    pub points: Vec<V3>,
}

/// `drawStrokes`' colours by sculpt mode.
pub fn stroke_color(mode: &str) -> u32 {
    match mode {
        "smooth" => 0x2f9e44,
        "flatten" => 0x1f6fd1,
        "inflate" => 0xe8763a,
        "deflate" => 0x9c36b5,
        _ => 0xe8763a,
    }
}

pub fn strokes(strokes: &[Stroke]) -> Vec<OverlayItem> {
    strokes
        .iter()
        .filter(|s| s.points.len() >= 2)
        .map(|s| {
            let mut it = OverlayItem::new("strokes", 30, Depth::Off);
            strip(&mut it.lines, &s.points, rgba(stroke_color(&s.mode), 1.0));
            it
        })
        .collect()
}

fn on_frame(frame: &Frame, p: [f64; 2]) -> V3 {
    let [o, x, y, _] = *frame;
    camera::add(o, camera::add(camera::scale(x, p[0]), camera::scale(y, p[1])))
}

/// `addRegions`: filled sketch regions (outer ring, then holes) at opacity 0.4.
pub fn regions(frame: &Frame, faces: &[Vec<Vec<[f64; 2]>>], tag: &str) -> Vec<OverlayItem> {
    let c = rgba(0x2f8cff, 0.4);
    faces
        .iter()
        .filter(|rings| !rings.is_empty())
        .map(|rings| {
            let mut it = OverlayItem::new(tag, 12, Depth::Off);
            let flat: Vec<[f64; 2]> = rings.iter().flatten().copied().collect();
            for t in stl_kernel::triangulate(rings) {
                for i in t {
                    if let Some(p) = flat.get(i as usize) {
                        it.tris.push(FlatVertex::new(on_frame(frame, *p), c));
                    }
                }
            }
            let whole = it.tris.len() / 3 * 3;
            it.tris.truncate(whole);
            it
        })
        .collect()
}

/// `sketchLines`: entity outlines on a sketch plane; construction lines dashed (1.5 on, 1 off)
/// in 0x8a6a55, left out when `construction` is false.
pub fn sketch_lines(frame: &Frame, entities: &[SketchOutline], color: u32, opacity: f32, construction: bool, tag: &str) -> OverlayItem {
    let mut it = OverlayItem::new(tag, 10, Depth::Off);
    for e in entities {
        if e.pts.len() < 2 || (e.construction && !construction) {
            continue;
        }
        let mut pts: Vec<V3> = e.pts.iter().map(|p| on_frame(frame, *p)).collect();
        if e.closed {
            pts.push(pts[0]);
        }
        if e.construction {
            dashed(&mut it.lines, &pts, 1.5, 1.0, rgba(0x8a6a55, opacity));
        } else {
            strip(&mut it.lines, &pts, rgba(color, opacity));
        }
    }
    it
}

/// `LineDashedMaterial` with `computeLineDistances`: dashes measured along the whole line.
fn dashed(out: &mut Vec<FlatVertex>, pts: &[V3], dash: f64, gap: f64, c: [u8; 4]) {
    let period = dash + gap;
    let mut along = 0.0;
    for w in pts.windows(2) {
        let len = camera::length(camera::sub(w[1], w[0]));
        if len <= 0.0 {
            continue;
        }
        let dir = camera::scale(camera::sub(w[1], w[0]), 1.0 / len);
        let mut s = 0.0;
        while s < len {
            let phase = (along + s) % period;
            if phase < dash {
                let e = (s + dash - phase).min(len);
                out.push(FlatVertex::new(camera::add(w[0], camera::scale(dir, s)), c));
                out.push(FlatVertex::new(camera::add(w[0], camera::scale(dir, e)), c));
                s = e;
            } else {
                s += period - phase;
            }
        }
        along += len;
    }
}
