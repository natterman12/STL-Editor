//! The `plane` pick field and `placeOnFace` (1.x `fieldHTML` `'plane'`, `feedPick` for planes,
//! `frameOf`), plus the face finder the face fields share (the `face` route of the engine:
//! `faceRegion`, `regionBoundary`, `facePlanar` in `web/engine/geom.js`).

use std::collections::HashMap;

use egui::Ui;
use serde_json::{json, Value};
use stl_kernel::MeshArrays;

use super::bodies::{advance_field, body_name, handle_box, pt_from, sel_box, Mark, PickOut, ShowOut};
use crate::theme::Tokens;
use crate::tools::{Ctx, Field, FieldKind, ToolDef, Values};
use crate::viewport::pick::Hit;
use crate::viewport::PlaneFrame;

// ------------------------------------------------------------------ the face finder

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn unit(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    if l > 0.0 {
        [a[0] / l, a[1] / l, a[2] / l]
    } else {
        a
    }
}

struct Tris<'a> {
    pos: &'a [f32],
    idx: &'a [u32],
    /// Unit normals and areas.
    n: Vec<V3>,
    area: Vec<f64>,
}

impl<'a> Tris<'a> {
    fn new(m: &'a MeshArrays) -> Self {
        let mut t = Tris { pos: &m.positions, idx: &m.triangles, n: Vec::new(), area: Vec::new() };
        for i in 0..t.count() {
            let [a, b, c] = t.corners(i);
            let c = cross(sub(b, a), sub(c, a));
            let l = dot(c, c).sqrt();
            t.n.push(if l > 0.0 { [c[0] / l, c[1] / l, c[2] / l] } else { [0.0; 3] });
            t.area.push(l / 2.0);
        }
        t
    }
    fn count(&self) -> usize {
        self.idx.len() / 3
    }
    fn vert(&self, i: u32) -> V3 {
        let p = i as usize * 3;
        [self.pos[p] as f64, self.pos[p + 1] as f64, self.pos[p + 2] as f64]
    }
    fn corners(&self, t: usize) -> [V3; 3] {
        [self.vert(self.idx[t * 3]), self.vert(self.idx[t * 3 + 1]), self.vert(self.idx[t * 3 + 2])]
    }
    /// Triangle neighbours across shared edges, among `cand`.
    fn neighbours(&self, cand: &[usize]) -> HashMap<usize, Vec<usize>> {
        let mut by_edge: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
        for &t in cand {
            for k in 0..3 {
                let (a, b) = (self.idx[t * 3 + k], self.idx[t * 3 + (k + 1) % 3]);
                by_edge.entry((a.min(b), a.max(b))).or_default().push(t);
            }
        }
        let mut out: HashMap<usize, Vec<usize>> = HashMap::new();
        for ts in by_edge.values() {
            for &a in ts {
                for &b in ts {
                    if a != b {
                        out.entry(a).or_default().push(b);
                    }
                }
            }
        }
        out
    }
}

/// `faceRegion`: the coplanar triangles around `tri`, in triangle order, with their area-weighted
/// normal and centroid.
fn face_region(t: &Tris, tri: usize, angle_tol_deg: f64) -> Option<(Vec<usize>, V3, V3)> {
    if tri >= t.count() {
        return None;
    }
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for i in 0..t.pos.len() / 3 {
        let v = t.vert(i as u32);
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    let dist_tol = (dot(sub(hi, lo), sub(hi, lo)).sqrt() * 1e-5).max(1e-5);
    let n0 = t.n[tri];
    let d0 = dot(n0, t.vert(t.idx[tri * 3]));
    let cos_t = angle_tol_deg.to_radians().cos();
    let ok = |u: usize| dot(t.n[u], n0) >= cos_t && t.corners(u).iter().all(|p| (dot(n0, *p) - d0).abs() < dist_tol);
    let cand: Vec<usize> = (0..t.count()).filter(|&u| u == tri || ok(u)).collect();
    let near = t.neighbours(&cand);
    let mut seen = vec![false; t.count()];
    let mut region = Vec::new();
    let mut stack = vec![tri];
    seen[tri] = true;
    while let Some(u) = stack.pop() {
        region.push(u);
        for &w in near.get(&u).into_iter().flatten() {
            if !seen[w] {
                seen[w] = true;
                stack.push(w);
            }
        }
    }
    region.sort_unstable();
    let (mut nrm, mut cen, mut w) = ([0.0; 3], [0.0; 3], 0.0);
    for &u in &region {
        let a = t.area[u];
        for k in 0..3 {
            nrm[k] += t.n[u][k] * a;
        }
        for c in t.corners(u) {
            for k in 0..3 {
                cen[k] += c[k] * a / 3.0;
            }
        }
        w += a;
    }
    let w = w.max(1e-9);
    Some((region, unit(nrm), [cen[0] / w, cen[1] / w, cen[2] / w]))
}

/// `regionBoundary`: edges used by one triangle of the region only, as directed vertex pairs.
fn region_boundary(t: &Tris, region: &[usize]) -> Vec<(u32, u32)> {
    let mut cnt: HashMap<(u32, u32), u32> = HashMap::new();
    let key = |a: u32, b: u32| (a.min(b), a.max(b));
    for &u in region {
        for k in 0..3 {
            *cnt.entry(key(t.idx[u * 3 + k], t.idx[u * 3 + (k + 1) % 3])).or_default() += 1;
        }
    }
    let mut out = Vec::new();
    for &u in region {
        for k in 0..3 {
            let (a, b) = (t.idx[u * 3 + k], t.idx[u * 3 + (k + 1) % 3]);
            if cnt[&key(a, b)] == 1 {
                out.push((a, b));
            }
        }
    }
    out
}

/// Facets meeting at less than this angle are one smooth surface (`FACE_SHALLOW_DEG`).
const FACE_SHALLOW_DEG: f64 = 20.0;

/// `facePlanar`: a real flat face, not one facet of a curved surface.
fn face_planar(t: &Tris, region: &[usize], normal: V3, tol: f64) -> bool {
    if region.len() <= 1 {
        return false;
    }
    let bd = region_boundary(t, region);
    if bd.is_empty() {
        return true;
    }
    let key = |a: u32, b: u32| (a.min(b), a.max(b));
    let mut in_r = vec![false; t.count()];
    for &u in region {
        in_r[u] = true;
    }
    // per boundary edge: the outside triangle across it with the largest bend (lowest index on a tie)
    let mut across: HashMap<(u32, u32), (f64, i64)> = HashMap::new();
    let mut on_bd = vec![false; t.pos.len() / 3];
    for &(a, b) in &bd {
        across.insert(key(a, b), (f64::INFINITY, -1));
        on_bd[a as usize] = true;
        on_bd[b as usize] = true;
    }
    for u in 0..t.count() {
        if in_r[u] || (0..3).filter(|&k| on_bd[t.idx[u * 3 + k] as usize]).count() < 2 {
            continue;
        }
        let d = dot(t.n[u], normal);
        for k in 0..3 {
            if let Some(s) = across.get_mut(&key(t.idx[u * 3 + k], t.idx[u * 3 + (k + 1) % 3])) {
                if d < s.0 {
                    *s = (d, u as i64);
                }
            }
        }
    }
    let cos_s = FACE_SHALLOW_DEG.to_radians().cos();
    let (mut total, mut shallow) = (0.0, 0.0);
    let mut soft: Vec<(f64, i64)> = Vec::new();
    for &(a, b) in &bd {
        let l = dot(sub(t.vert(a), t.vert(b)), sub(t.vert(a), t.vert(b))).sqrt();
        total += l;
        let s = across[&key(a, b)];
        if s.1 >= 0 && s.0 >= cos_s {
            shallow += l;
            soft.push((l, s.1));
        }
    }
    if shallow <= 0.5 * total {
        return true;
    }
    let max_l = soft.iter().map(|s| s.0).fold(0.0, f64::max);
    let Some(next) = soft.iter().filter(|s| s.0 >= max_l - 1e-9).map(|s| s.1).min() else { return true };
    let Some((other, _, _)) = face_region(t, next as usize, tol) else { return true };
    let a: f64 = region.iter().map(|&u| t.area[u]).sum();
    let b: f64 = other.iter().map(|&u| t.area[u]).sum();
    b < 0.5 * a
}

/// One face of a body, as the engine's `face` route returns it.
#[derive(Debug, Clone)]
pub struct FaceInfo {
    pub origin: V3,
    pub normal: V3,
    pub xdir: V3,
    pub ydir: V3,
    pub centroid: V3,
    /// The coplanar triangles (at most 200 000).
    pub tris: Vec<u32>,
    pub planar: bool,
    /// The outline rings in plane coordinates (only when asked for). Chained from the boundary
    /// edges; not simplified and not wound (1.x's `faceOutline` also drops collinear points).
    pub contours: Vec<Vec<[f64; 2]>>,
}

/// `post('face', { body, tri, outline })`.
pub fn face_info(mesh: &MeshArrays, tri: usize, outline: bool) -> Option<FaceInfo> {
    let t = Tris::new(mesh);
    let (region, normal, centroid) = face_region(&t, tri, 1.0)?;
    let (mut n, mut origin) = (normal, centroid);
    let k = (0..3).fold(0, |a, i| if normal[i].abs() > normal[a].abs() { i } else { a });
    if normal[k].abs() > 0.99999 {
        n = [0.0; 3];
        n[k] = normal[k].signum();
        origin = [0.0; 3];
        origin[k] = centroid[k];
    }
    let fr = stl_engine::features::common::frames::plane_frame(origin, n, None).ok()?;
    let planar = face_planar(&t, &region, normal, 1.0);
    let contours = if outline && region.len() <= 400_000 { outline_rings(&t, &region, fr.o, fr.x, fr.y) } else { Vec::new() };
    Some(FaceInfo { origin: fr.o, normal: fr.n, xdir: fr.x, ydir: fr.y, centroid, tris: region.iter().take(200_000).map(|&u| u as u32).collect(), planar, contours })
}

fn outline_rings(t: &Tris, region: &[usize], o: V3, x: V3, y: V3) -> Vec<Vec<[f64; 2]>> {
    let bd = region_boundary(t, region);
    let mut next: HashMap<u32, Vec<u32>> = HashMap::new();
    for &(a, b) in &bd {
        next.entry(a).or_default().push(b);
    }
    let mut used: std::collections::HashSet<(u32, u32)> = Default::default();
    let mut rings = Vec::new();
    let mut starts: Vec<(u32, u32)> = bd.clone();
    starts.sort_unstable();
    for (a0, b0) in starts {
        if used.contains(&(a0, b0)) {
            continue;
        }
        let mut ring = vec![a0];
        let (mut a, mut b) = (a0, b0);
        loop {
            used.insert((a, b));
            if b == a0 {
                break;
            }
            ring.push(b);
            let Some(n) = next.get(&b).and_then(|v| v.iter().copied().find(|&c| !used.contains(&(b, c)))) else { break };
            a = b;
            b = n;
        }
        if ring.len() >= 3 {
            rings.push(ring.iter().map(|&i| {
                let q = sub(t.vert(i), o);
                [(dot(q, x) * 1e5).round() / 1e5, (dot(q, y) * 1e5).round() / 1e5]
            }).collect());
        }
    }
    rings
}

// ------------------------------------------------------------------ frames

/// `frameOf(ref)`: a base plane name, a plane's id, or an `{origin, normal, xdir}` object.
pub fn frame_of(r: &Value, ctx: &Ctx) -> Option<PlaneFrame> {
    let f = |origin, x, y, normal| Some(PlaneFrame { origin, x, y, normal });
    match r {
        Value::String(s) => match s.as_str() {
            "XY" => f([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
            "XZ" => f([0.0; 3], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]),
            "YZ" => f([0.0; 3], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
            id => ctx.planes.iter().find(|p| p.id == id).map(|p| PlaneFrame { origin: p.origin, x: p.x, y: p.y, normal: p.normal }),
        },
        Value::Object(o) => {
            let origin = pt_from(o.get("origin")?)?;
            let n = pt_from(o.get("normal")?)?;
            let x = pt_from(o.get("xdir")?)?;
            f(origin, x, cross(n, x), n)
        }
        _ => None,
    }
}

/// `planeLabel(v)`.
pub fn plane_label(v: Option<&Value>, ctx: &Ctx) -> String {
    match v {
        Some(Value::String(s)) => match s.as_str() {
            "XY" => "XY plane".into(),
            "XZ" => "XZ plane".into(),
            "YZ" => "YZ plane".into(),
            id => ctx.planes.iter().find(|p| p.id == id).map(|p| p.name.clone()).unwrap_or_else(|| id.to_owned()),
        },
        Some(Value::Object(o)) => o.get("label").and_then(Value::as_str).map(str::to_owned).unwrap_or_else(|| "Face".into()),
        _ => String::new(),
    }
}

// ------------------------------------------------------------------ the field

/// `fieldHTML` `'plane'`.
pub fn show(ui: &mut Ui, tk: &Tokens, f: &Field, values: &mut Values, active: &mut Option<String>, ctx: &Ctx) -> ShowOut {
    let label = plane_label(values.get(f.key), ctx);
    let chips: Vec<String> = if label.is_empty() { vec![] } else { vec![label] };
    let b = sel_box(ui, tk, egui::Id::unique(("pick", f.key)), active.as_deref() == Some(f.key), &chips, "Click a plane or flat face");
    let mut out = ShowOut::default();
    handle_box(&b, f.key, active, &mut out, |_| {
        values.insert(f.key.to_owned(), Value::Null);
        true
    });
    out.clear_face = out.changed;
    out
}

/// The field a viewport click goes to: the active pick field, or the `plane` field of a
/// `placeOnFace` tool (primitives: clicking a face places the shape there).
pub fn target_field<'a>(tool: &'a ToolDef, active: Option<&str>, hit: &Hit) -> Option<&'a Field> {
    if let Some(f) = active.and_then(|k| tool.fields.iter().find(|f| f.key == k)) {
        return Some(f);
    }
    let _ = hit;
    if tool.flags.place_on_face {
        return tool.fields.iter().find(|f| f.key == "plane");
    }
    None
}

/// Planes must be shown and pickable: the plane field is active, or the tool places on a face,
/// or it is the 3-point plane (`wantPlanes`).
pub fn wants_planes(tool: &ToolDef, active: Option<&str>) -> bool {
    tool.fields.iter().any(|f| matches!(f.kind, FieldKind::Plane { .. }) && active == Some(f.key)) || tool.flags.place_on_face || tool.key == "plane_3pt"
}

/// `feedPick` for `plane`. A plane hit stores its id; a face hit stores the face as
/// `{origin, normal, xdir, label, body}` and highlights it. `placeOnFace` tools also put the
/// shape where the face was clicked (`offset`).
pub fn feed_pick(tool: &ToolDef, f: &Field, values: &mut Values, hit: &Hit, ctx: &Ctx) -> PickOut {
    let mut out = PickOut::changed();
    match hit {
        Hit::Plane { id, .. } => {
            values.insert(f.key.to_owned(), Value::from(id.clone()));
        }
        Hit::Body { id, face, point, .. } => {
            let Some(b) = ctx.bodies.iter().find(|b| &b.id == id) else { return PickOut::used() };
            let Some(fi) = face_info(&b.mesh, *face as usize, false) else { return PickOut::used() };
            values.insert(
                f.key.to_owned(),
                json!({ "origin": fi.origin, "normal": fi.normal, "xdir": fi.xdir, "label": format!("Face of {}", body_name(ctx, id)), "body": id }),
            );
            if tool.flags.place_on_face {
                let d = sub(*point, fi.origin);
                let r3 = |x: f64| (x * 1e3).round() / 1e3;
                values.insert("offset".to_owned(), json!([r3(dot(d, fi.xdir)), r3(dot(d, fi.ydir)), 0.0]));
            }
            out.clear_face = true;
            out.highlight = Some((id.clone(), fi.tris));
        }
    }
    out.advance = true;
    let _ = advance_field;
    out
}

/// A plane field shows a ghost of its plane, unless the tool is itself a plane tool.
pub fn marks(f: &Field, values: &Values, ctx: &Ctx, plane_tool: bool) -> Vec<Mark> {
    match values.get(f.key).and_then(|v| frame_of(v, ctx)) {
        Some(fr) if !plane_tool => vec![Mark::PlaneGhost(fr)],
        _ => Vec::new(),
    }
}
