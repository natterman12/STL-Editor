//! Exact placement of Hole and Thread points (spec 2.4): a point measured along a sketch line or
//! from part edges and sketch lines, re-placed from those references when the project runs.
//!
//! Stored beside the points (Hole `point_refs`, one list per point; Thread `position_refs`, one
//! list). Each reference is an object:
//!
//! - `{"kind": "edge", "a": [x,y,z], "b": [x,y,z], "normal": [x,y,z], "dist": d, "body": id?}`:
//!   a straight part edge as it was when picked (its end points and the normal of the face the
//!   point sits on); `dist` is the point's distance from it, in the face's plane.
//! - `{"kind": "sketch_line", "sketch": id, "entity": id, "along": d?, "dist": d?}`: a sketch
//!   line (`entity` is a line's id, or `"<id>#<k>"` for side `k` of a rectangle, polygon, slot
//!   or polyline). `along` puts the point that far from the line's start; `dist` alone measures
//!   the point's distance from the line.
//!
//! The UI computes the same point with [`place`] when a distance is typed, so the stored point
//! already matches; running the project again re-finds the references and re-places it.

use std::collections::BTreeMap;

use serde_json::{json, Value};
use stl_kernel::{cross, dot, MeshArrays, Vec3};
use stl_sketch::contours::entity_path;

use crate::{FeatureError, State};

/// One reference a placed point was measured from.
#[derive(Clone, Debug, PartialEq)]
pub enum PlaceRef {
    /// A sketch line: `along` is the distance from its start; `dist` the distance from the line.
    SketchLine { sketch: String, entity: String, along: Option<f64>, dist: Option<f64> },
    /// A straight part edge (its end points when picked) on the face with this normal.
    Edge { a: Vec3, b: Vec3, face_normal: Vec3, dist: f64 },
}

// ------------------------------------------------------------------ small vector helpers

fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn mul(a: Vec3, s: f64) -> Vec3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn len(a: Vec3) -> f64 {
    dot(a, a).sqrt()
}

fn unit(a: Vec3) -> Option<Vec3> {
    let l = len(a);
    (l > 1e-12 && l.is_finite()).then(|| mul(a, 1.0 / l))
}

/// cos 1°: the angle tolerance of spec 2.4.
const COS_1DEG: f64 = 0.999_847_695_156_391_2;
/// Two candidate edges this close in distance count as a tie (spec 2.4).
const TIE: f64 = 0.01;
/// An edge is a feature edge when its two faces turn by more than this (degrees).
const SHARP_DEG: f64 = 20.0;

// ------------------------------------------------------------------ parsing

fn vec3(v: Option<&Value>) -> Option<Vec3> {
    let a = v?.as_array()?;
    if a.len() != 3 {
        return None;
    }
    Some([a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?])
}

fn num(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64).filter(|x| x.is_finite())
}

/// One reference; `None` for an entry this version doesn't understand (it is skipped).
pub fn parse_ref(v: &Value) -> Option<PlaceRef> {
    let o = v.as_object()?;
    match o.get("kind").and_then(Value::as_str)? {
        "edge" => Some(PlaceRef::Edge {
            a: vec3(o.get("a"))?,
            b: vec3(o.get("b"))?,
            face_normal: vec3(o.get("normal"))?,
            dist: num(o.get("dist"))?,
        }),
        "sketch_line" => Some(PlaceRef::SketchLine {
            sketch: o.get("sketch")?.as_str()?.to_owned(),
            entity: o.get("entity")?.as_str()?.to_owned(),
            along: num(o.get("along")),
            dist: num(o.get("dist")),
        }),
        _ => None,
    }
}

/// One point's list of references (Thread's `position_refs`).
pub fn parse_ref_list(v: Option<&Value>) -> Vec<PlaceRef> {
    v.and_then(Value::as_array).map(|a| a.iter().filter_map(parse_ref).collect()).unwrap_or_default()
}

/// The param `point_refs`: one list per point; absent means none (Task 5).
pub fn parse_refs(v: Option<&Value>) -> Vec<Vec<PlaceRef>> {
    v.and_then(Value::as_array).map(|a| a.iter().map(|l| parse_ref_list(Some(l))).collect()).unwrap_or_default()
}

/// A reference as stored (the inverse of [`parse_ref`]); `body` is kept on part edges.
pub fn ref_json(r: &PlaceRef, body: Option<&str>) -> Value {
    match r {
        PlaceRef::Edge { a, b, face_normal, dist } => {
            let mut v = json!({"kind": "edge", "a": a, "b": b, "normal": face_normal, "dist": dist});
            if let Some(id) = body.filter(|s| !s.is_empty()) {
                v["body"] = json!(id);
            }
            v
        }
        PlaceRef::SketchLine { sketch, entity, along, dist } => {
            let mut v = json!({"kind": "sketch_line", "sketch": sketch, "entity": entity});
            if let Some(x) = along {
                v["along"] = json!(x);
            }
            if let Some(x) = dist {
                v["dist"] = json!(x);
            }
            v
        }
    }
}

/// The body a stored list of references was picked on (the first part edge's `"body"`).
pub fn ref_body(list: Option<&Value>) -> Option<String> {
    list?.as_array()?.iter().find_map(|r| r.get("body").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_owned))
}

// ------------------------------------------------------------------ part edges

/// The straight feature edges of `mesh` that lie in a face whose normal is within 1° of
/// `face_normal` (same side), with touching collinear pieces joined into one edge. Sorted, so the
/// result does not depend on the mesh's vertex order beyond the positions.
pub fn straight_edges(mesh: &MeshArrays, face_normal: Vec3) -> Vec<(Vec3, Vec3)> {
    let Some(fnrm) = unit(face_normal) else { return Vec::new() };
    // weld vertices by position (a solid's mesh may repeat a vertex per face)
    let q = |x: f32| (x as f64 * 1e5).round() as i64;
    let mut ids: BTreeMap<(i64, i64, i64), u32> = BTreeMap::new();
    let mut pos: Vec<Vec3> = Vec::new();
    let mut remap: Vec<u32> = Vec::with_capacity(mesh.positions.len() / 3);
    for p in mesh.positions.chunks_exact(3) {
        let k = (q(p[0]), q(p[1]), q(p[2]));
        let id = *ids.entry(k).or_insert_with(|| {
            pos.push([p[0] as f64, p[1] as f64, p[2] as f64]);
            (pos.len() - 1) as u32
        });
        remap.push(id);
    }
    // each edge's triangles
    let mut normals: Vec<Option<Vec3>> = Vec::new();
    let mut edges: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    for (t, tri) in mesh.triangles.chunks_exact(3).enumerate() {
        let v: Vec<u32> = tri.iter().map(|&i| remap.get(i as usize).copied().unwrap_or(0)).collect();
        let (a, b, c) = (pos[v[0] as usize], pos[v[1] as usize], pos[v[2] as usize]);
        normals.push(unit(cross(sub(b, a), sub(c, a))));
        for (i, j) in [(v[0], v[1]), (v[1], v[2]), (v[2], v[0])] {
            if i != j {
                edges.entry((i.min(j), i.max(j))).or_default().push(t);
            }
        }
    }
    let cos_sharp = SHARP_DEG.to_radians().cos();
    let mut segs: Vec<(u32, u32)> = Vec::new();
    for (&(i, j), tris) in &edges {
        let ns: Vec<Vec3> = tris.iter().filter_map(|&t| normals[t]).collect();
        let in_face = ns.iter().any(|n| dot(*n, fnrm) >= COS_1DEG);
        let sharp = ns.len() != 2 || dot(ns[0], ns[1]) < cos_sharp;
        if in_face && sharp {
            segs.push((i, j));
        }
    }
    // join touching collinear pieces (union-find over shared end points)
    let dir = |s: (u32, u32)| unit(sub(pos[s.1 as usize], pos[s.0 as usize]));
    let mut parent: Vec<usize> = (0..segs.len()).collect();
    fn root(p: &mut [usize], mut i: usize) -> usize {
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    let mut by_vertex: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (k, s) in segs.iter().enumerate() {
        by_vertex.entry(s.0).or_default().push(k);
        by_vertex.entry(s.1).or_default().push(k);
    }
    for list in by_vertex.values() {
        for x in 0..list.len() {
            for y in x + 1..list.len() {
                let (Some(dx), Some(dy)) = (dir(segs[list[x]]), dir(segs[list[y]])) else { continue };
                if dot(dx, dy).abs() >= COS_1DEG {
                    let (rx, ry) = (root(&mut parent, list[x]), root(&mut parent, list[y]));
                    if rx != ry {
                        parent[rx.max(ry)] = rx.min(ry);
                    }
                }
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for k in 0..segs.len() {
        let r = root(&mut parent, k);
        groups.entry(r).or_default().push(k);
    }
    let mut out: Vec<(Vec3, Vec3)> = Vec::new();
    for members in groups.values() {
        let Some(d) = dir(segs[members[0]]) else { continue };
        let base = pos[segs[members[0]].0 as usize];
        let (mut lo, mut hi) = ((f64::INFINITY, base), (f64::NEG_INFINITY, base));
        for &m in members {
            for v in [segs[m].0, segs[m].1] {
                let p = pos[v as usize];
                let t = dot(sub(p, base), d);
                if t < lo.0 {
                    lo = (t, p);
                }
                if t > hi.0 {
                    hi = (t, p);
                }
            }
        }
        if hi.0 - lo.0 > 1e-9 {
            out.push(low_first(lo.1, hi.1));
        }
    }
    out.sort_by(|x, y| cmp_pt(x.0, y.0).then(cmp_pt(x.1, y.1)));
    out
}

fn cmp_pt(a: Vec3, b: Vec3) -> std::cmp::Ordering {
    a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])).then(a[2].total_cmp(&b[2]))
}

/// The pair with the lower end point first.
fn low_first(a: Vec3, b: Vec3) -> (Vec3, Vec3) {
    if cmp_pt(a, b).is_le() {
        (a, b)
    } else {
        (b, a)
    }
}

/// Distance from `p` to the infinite line through `a` with unit direction `d`.
fn line_dist(p: Vec3, a: Vec3, d: Vec3) -> f64 {
    let w = sub(p, a);
    len(sub(w, mul(d, dot(w, d))))
}

/// The edge among `candidates` that matches a stored one (spec 2.4): parallel to it (1°), at
/// least a quarter of its length, nearest the stored line. Pieces of the same line that tie go
/// to the longest, then the lowest end point; two different lines within 0.01 mm of the same
/// distance are a tie, so `None`. The result runs the same way as `a -> b`.
pub fn pick_edge(candidates: &[(Vec3, Vec3)], a: Vec3, b: Vec3) -> Option<(Vec3, Vec3)> {
    let d0 = unit(sub(b, a))?;
    let len0 = len(sub(b, a));
    let mid = mul(add(a, b), 0.5);
    let mut list: Vec<(f64, Vec3, Vec3, Vec3)> = Vec::new(); // (distance, a, b, unit dir)
    for &(p, q) in candidates {
        let Some(d) = unit(sub(q, p)) else { continue };
        if dot(d, d0).abs() < COS_1DEG || len(sub(q, p)) < len0 * 0.25 {
            continue;
        }
        list.push((line_dist(mid, p, d), p, q, d));
    }
    list.sort_by(|x, y| x.0.total_cmp(&y.0).then(cmp_pt(x.1, y.1)));
    let best = *list.first()?;
    let mut same_line: Vec<(f64, Vec3, Vec3, Vec3)> = Vec::new();
    for c in list.iter().take_while(|c| c.0 - best.0 <= TIE) {
        let collinear = line_dist(c.1, best.1, best.3) <= TIE && line_dist(c.2, best.1, best.3) <= TIE;
        if !collinear {
            return None; // two different lines equally near: ambiguous
        }
        same_line.push(*c);
    }
    same_line.sort_by(|x, y| len(sub(y.2, y.1)).total_cmp(&len(sub(x.2, x.1))).then(cmp_pt(x.1, y.1)));
    let (_, p, q, d) = same_line[0];
    Some(if dot(d, d0) >= 0.0 { (p, q) } else { (q, p) })
}

/// The edge on `mesh` that matches a stored one (spec 2.4's rule); `None` when it is missing or
/// two edges tie (Task 5).
pub fn find_edge(mesh: &MeshArrays, a: Vec3, b: Vec3, face_normal: Vec3) -> Option<(Vec3, Vec3)> {
    pick_edge(&straight_edges(mesh, face_normal), a, b)
}

// ------------------------------------------------------------------ placing

/// `w` with its first letter lower-cased (char-safe: a message may start with any character).
pub fn lower_first(w: &str) -> String {
    let mut c = w.chars();
    match c.next() {
        Some(f) => f.to_lowercase().chain(c).collect(),
        None => String::new(),
    }
}

/// A line a point keeps a distance from, as resolved now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DistLine {
    /// The line as it is now (in or parallel to the face).
    pub a: Vec3,
    pub b: Vec3,
    /// The distance the point keeps from it, in the face's plane.
    pub dist: f64,
    /// Which side the point is on: +1 along `normal × (b - a)`, -1 the other way.
    pub side: f64,
}

/// The side of the line `a -> b` (in the plane with this normal) `p` is on; +1 when on it.
pub fn side_of(p: Vec3, a: Vec3, b: Vec3, normal: Vec3) -> f64 {
    let s = dot(sub(p, a), cross(normal, sub(b, a)));
    if s < 0.0 {
        -1.0
    } else {
        1.0
    }
}

/// Where a point goes: `fixed` (a point along a sketch line) wins; else it keeps its distance
/// from one or two lines, staying in the plane through `plane_at` with this normal (one line: it
/// moves straight away from that line only). `Err` when two lines are parallel.
pub fn place(stored: Vec3, normal: Vec3, plane_at: Vec3, fixed: Option<Vec3>, lines: &[DistLine]) -> Result<Vec3, String> {
    let n = unit(normal).ok_or_else(|| "The face has no direction.".to_owned())?;
    let h = dot(plane_at, n);
    let onto_plane = |p: Vec3| add(p, mul(n, h - dot(p, n)));
    if let Some(p) = fixed {
        return Ok(onto_plane(p));
    }
    // each line's unit normal in the face, pointing to the point's side, and its offset
    let mut rows: Vec<(Vec3, f64, Vec3)> = Vec::new();
    for l in lines.iter().take(2) {
        let d = sub(l.b, l.a);
        let d = unit(sub(d, mul(n, dot(d, n)))).ok_or_else(|| "An edge it was measured from runs along the face normal.".to_owned())?;
        let s = mul(cross(n, d), l.side);
        rows.push((s, l.dist + dot(l.a, s), d));
    }
    match rows.as_slice() {
        [] => Ok(onto_plane(stored)),
        [(s, c, d)] => Ok(add(add(mul(n, h), mul(*s, *c)), mul(*d, dot(stored, *d)))),
        [(s1, c1, _), (s2, c2, _), ..] => {
            if len(cross(*s1, *s2)) < (1.0f64).to_radians().sin() {
                return Err("The two edges it was measured from are parallel, so only one distance can hold.".to_owned());
            }
            Ok(solve3([n, *s1, *s2], [h, *c1, *c2]))
        }
    }
}

/// Solve `rows[i] · x = rhs[i]` (Cramer's rule; the rows are independent).
fn solve3(r: [Vec3; 3], rhs: [f64; 3]) -> Vec3 {
    let det = dot(r[0], cross(r[1], r[2]));
    let c0 = cross(r[1], r[2]);
    let c1 = cross(r[2], r[0]);
    let c2 = cross(r[0], r[1]);
    mul(add(add(mul(c0, rhs[0]), mul(c1, rhs[1])), mul(c2, rhs[2])), 1.0 / det)
}

/// A sketch line in world coordinates: (start, end), or `None` when the sketch or the entity is
/// gone or isn't a straight line.
pub fn sketch_line(state: &State, sketch: &str, entity: &str) -> Option<(Vec3, Vec3)> {
    let rec = state.sketches.get(sketch)?;
    let (id, side) = match entity.split_once('#') {
        Some((id, k)) => (id, Some(k.parse::<usize>().ok()?)),
        None => (entity, None),
    };
    let e = rec.entities.iter().find(|e| e.id() == Some(id))?;
    let (pts, closed) = entity_path(e);
    let (p, q) = match side {
        None if pts.len() == 2 => (pts[0], pts[1]),
        Some(k) if k + 1 < pts.len() => (pts[k], pts[k + 1]),
        Some(k) if closed && k + 1 == pts.len() && pts.len() > 2 => (pts[k], pts[0]),
        _ => return None,
    };
    let f = &rec.frame;
    let w = |u: [f64; 2]| add(f.o, add(mul(f.x, u[0]), mul(f.y, u[1])));
    let (a, b) = (w(p), w(q));
    (len(sub(b, a)) > 1e-9).then_some((a, b))
}

/// Where a point goes from its references: (point, warning). With no references, or one that can
/// no longer be found, the stored point is kept (with a warning in the second case) (Task 5).
/// `body` is the body its part edges are on; empty means any body.
pub fn resolve_point(state: &State, body: &str, stored: Vec3, normal: Vec3, refs: &[PlaceRef]) -> Result<(Vec3, Option<String>), FeatureError> {
    if refs.is_empty() {
        return Ok((stored, None));
    }
    let missing = |what: &str| -> Result<(Vec3, Option<String>), FeatureError> {
        Ok((stored, Some(format!("{what} this point was measured from can no longer be found, so the point stays where it was."))))
    };
    let Some(n) = unit(normal) else { return Ok((stored, None)) };
    // the meshes part edges are looked for on, made once
    let mut edges: Option<Vec<(Vec3, Vec3)>> = None;
    let mut fixed: Option<Vec3> = None;
    let mut lines: Vec<DistLine> = Vec::new();
    let mut plane_at = stored;
    let mut moved_plane = false;
    for r in refs {
        match r {
            PlaceRef::Edge { a, b, face_normal, dist } => {
                let cands = edges.get_or_insert_with(|| {
                    let meshes: Vec<MeshArrays> = state
                        .bodies
                        .iter()
                        .filter(|(id, _)| body.is_empty() || id.as_str() == body)
                        .filter_map(|(_, b)| match (&b.solid, &b.mesh) {
                            (Some(s), _) => Some(s.to_mesh()),
                            (None, Some(m)) => Some((**m).clone()),
                            _ => None,
                        })
                        .collect();
                    meshes.iter().flat_map(|m| straight_edges(m, *face_normal)).collect()
                });
                let Some((na, nb)) = pick_edge(cands, *a, *b) else { return missing("An edge") };
                if !moved_plane {
                    // the face moved along its normal as much as the edge did
                    plane_at = add(stored, mul(n, dot(sub(na, *a), n)));
                    moved_plane = true;
                }
                lines.push(DistLine { a: na, b: nb, dist: *dist, side: side_of(stored, *a, *b, n) });
            }
            PlaceRef::SketchLine { sketch, entity, along, dist } => {
                let Some((a, b)) = sketch_line(state, sketch, entity) else { return missing("A sketch line") };
                match (along, dist) {
                    (Some(t), off) => {
                        let Some(d) = unit(sub(b, a)) else { return missing("A sketch line") };
                        let mut p = add(a, mul(d, *t));
                        if let Some(o) = off {
                            // sideways in the face's plane, the same rule as the panel
                            // (`place_refs::resolve_current`), so what is shown is what is cut
                            p = add(p, mul(cross(n, d), side_of(stored, a, b, n) * o));
                        }
                        fixed.get_or_insert(p);
                    }
                    (None, Some(o)) => lines.push(DistLine { a, b, dist: *o, side: side_of(stored, a, b, n) }),
                    (None, None) => {}
                }
            }
        }
    }
    match place(stored, n, plane_at, fixed, &lines) {
        Ok(p) => Ok((p, None)),
        Err(e) => Ok((stored, Some(e))),
    }
}
