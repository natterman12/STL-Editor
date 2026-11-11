//! Helpers shared by Sketch, Extrude, Revolve, Loft, Sweep and Hole: the sketch lookup, loop
//! preparation for loft and sweep, path checks, and a few small geometry functions.
//! Ports of the `features.py` helpers (`_sketch`, `_material_volume`, `_dedupe`, `_ccw`,
//! `_profile_loop`, `_resample`, `_align`, `_mesh_solid`, `_path_points`, `_cross2`,
//! `_path_self_crosses`, `_rot_between`, `_bbox`, `_overlaps`) and the `geometry.py` ones
//! (`big_extent`, `frame_matrix`, `make_cylinder`).

use indexmap::IndexMap;
use serde_json::Value;
use stl_kernel::{cross, dot, Aabb, Frame, MeshArrays, Solid, Vec3};
use stl_sketch::contours::entity_path;
use stl_sketch::profile::profile_with;
use stl_sketch::{Entity, P2};

use super::frames::plane_frame;
use crate::state::Body;
use crate::FeatureError;

/// A sketch a Sketch feature stored: its plane frame, entities and part-edge references.
#[derive(Clone, Debug, PartialEq)]
pub struct SketchRec {
    pub name: String,
    pub frame: Frame,
    pub entities: Vec<Entity>,
    pub refs: Vec<Value>,
}

/// 1.x `_sketch`: the stored sketch for `sid`.
pub fn sketch<'a>(sketches: &'a IndexMap<String, SketchRec>, sid: Option<&str>) -> Result<&'a SketchRec, FeatureError> {
    sid.and_then(|s| sketches.get(s))
        .ok_or_else(|| FeatureError("The sketch for this feature no longer exists.".into()))
}

/// 1.x `_bbox`.
pub fn bbox(s: &Solid) -> Aabb {
    s.bounding_box()
}

/// 1.x `_overlaps`: do two boxes touch or overlap, within `tol`.
pub fn overlaps(a: &Aabb, b: &Aabb, tol: f64) -> bool {
    (0..3).all(|i| a.min[i] <= b.max[i] + tol) && (0..3).all(|i| b.min[i] <= a.max[i] + tol)
}

/// 1.x `_material_volume`: how much solid material the tool overlaps in the target bodies
/// (all bodies when `targets` is empty).
pub fn material_volume(bodies: &IndexMap<String, Body>, tool: &Solid, targets: &[String]) -> f64 {
    let tb = bbox(tool);
    let mut vol = 0.0;
    for (k, b) in bodies {
        if !targets.is_empty() && !targets.contains(k) {
            continue;
        }
        if let Some(s) = &b.solid {
            if overlaps(&bbox(s), &tb, 1e-6) {
                vol += s.intersection(tool).volume();
            }
        }
    }
    vol
}

fn dist(a: P2, b: P2) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// 1.x `_dedupe`: drop consecutive points closer than `tol`; when `closed`, also a last point
/// that repeats the first.
pub fn dedupe(pts: &[P2], closed: bool, tol: f64) -> Vec<P2> {
    let mut out: Vec<P2> = Vec::new();
    for &p in pts {
        if out.last().is_none_or(|&l| dist(p, l) > tol) {
            out.push(p);
        }
    }
    if closed {
        while out.len() > 1 && dist(out[0], out[out.len() - 1]) <= tol {
            out.pop();
        }
    }
    out
}

/// 1.x `_ccw`: the loop turned to run counter-clockwise.
pub fn ccw(pts: &[P2]) -> Vec<P2> {
    let n = pts.len();
    let mut s = 0.0;
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        s += a[0] * b[1] - b[0] * a[1];
    }
    let mut v = pts.to_vec();
    if 0.5 * s < 0.0 {
        v.reverse();
    }
    v
}

/// 1.x `_profile_loop`: the one closed outline of a sketch, counter-clockwise, in sketch
/// coordinates.
pub fn profile_loop(sk: &SketchRec, idx: usize) -> Result<Vec<P2>, FeatureError> {
    let cs = profile_with(&sk.entities, None, None).map_err(|_| {
        FeatureError(format!("Profile {idx} isn't a closed shape with an area. Close its outline first."))
    })?;
    if cs.contours.len() != 1 {
        return Err(FeatureError(format!(
            "Profile {idx} has a hole or more than one shape. Loft and sweep profiles must be one closed outline in this version."
        )));
    }
    Ok(ccw(&dedupe(&cs.contours[0], true, 1e-9)))
}

/// 1.x `_resample`: `n` points spaced evenly by length around a closed loop, starting at
/// `pts[0]`.
pub fn resample(pts: &[P2], n: usize) -> Vec<P2> {
    let m = pts.len();
    let closed = |i: usize| pts[i % m];
    let seg: Vec<f64> = (0..m).map(|i| dist(closed(i), closed(i + 1))).collect();
    let mut cum = vec![0.0];
    let mut acc = 0.0;
    for s in &seg {
        acc += s;
        cum.push(acc);
    }
    let total = cum[m];
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        let s = k as f64 * total / n as f64;
        // searchsorted(cum, s, side="right") - 1, capped at the last segment
        let i = (cum.partition_point(|&c| c <= s) as i64 - 1).clamp(0, m as i64 - 1) as usize;
        let f = (s - cum[i]) / seg[i];
        let (a, b) = (closed(i), closed(i + 1));
        out.push([a[0] + f * (b[0] - a[0]), a[1] + f * (b[1] - a[1])]);
    }
    out
}

/// 1.x `_align`: rotate the start of loop `cur` so its points line up best with loop `prev`.
pub fn align(prev: &[Vec3], cur: &[Vec3]) -> Vec<Vec3> {
    let n = cur.len();
    let mut best: Option<f64> = None;
    let mut bs = 0;
    for s in 0..n {
        let mut d = 0.0;
        for i in 0..n {
            let c = cur[(i + s) % n];
            for k in 0..3 {
                d += (prev[i][k] - c[k]).powi(2);
            }
        }
        if best.is_none_or(|b| d < b - 1e-12) {
            best = Some(d);
            bs = s;
        }
    }
    (0..n).map(|i| cur[(i + bs) % n]).collect()
}

/// 1.x `_mesh_solid`: a solid from vertices and triangles, or the "couldn't be closed" error.
pub fn mesh_solid(verts: &[Vec3], tris: &[[u32; 3]], what: &str) -> Result<Solid, FeatureError> {
    let mesh = MeshArrays {
        positions: verts.iter().flat_map(|v| [v[0] as f32, v[1] as f32, v[2] as f32]).collect(),
        triangles: tris.iter().flatten().copied().collect(),
    };
    match Solid::from_mesh(&mesh) {
        Ok(s) if !s.is_empty() && s.signed_volume() > 1e-9 => Ok(s),
        _ => Err(FeatureError(format!(
            "The {what} couldn't be closed into a solid. Check that the profiles or path don't cross or fold back on themselves."
        ))),
    }
}

/// 1.x `_path_points`: the sketch's open path (lines, arcs and open polylines joined end to
/// end) in sketch coordinates.
pub fn path_points(sk: &SketchRec) -> Result<Vec<P2>, FeatureError> {
    let mut items: Vec<Vec<P2>> = Vec::new();
    for e in &sk.entities {
        if e.construction() || !matches!(e.kind(), "line" | "arc" | "polyline") {
            continue;
        }
        let (pts, closed) = entity_path(e);
        if !closed && pts.len() >= 2 {
            items.push(pts);
        }
    }
    if items.is_empty() {
        return Err(FeatureError("The sweep path needs one open path of lines and arcs in its sketch.".into()));
    }
    let mut chain = items.remove(0);
    let tol = 1e-6;
    while !items.is_empty() {
        let mut found = None;
        for (idx, it) in items.iter().enumerate() {
            let last = chain[chain.len() - 1];
            let (first_c, last_c) = (chain[0], last);
            let (it0, it1) = (it[0], it[it.len() - 1]);
            if dist(it0, last_c) <= tol {
                chain.extend_from_slice(&it[1..]);
            } else if dist(it1, last_c) <= tol {
                chain.extend(it.iter().rev().skip(1).copied());
            } else if dist(it1, first_c) <= tol {
                let mut c = it.clone();
                c.extend_from_slice(&chain[1..]);
                chain = c;
            } else if dist(it0, first_c) <= tol {
                let mut c: Vec<P2> = it.iter().rev().copied().collect();
                c.extend_from_slice(&chain[1..]);
                chain = c;
            } else {
                continue;
            }
            found = Some(idx);
            break;
        }
        match found {
            Some(idx) => {
                items.remove(idx);
            }
            None => {
                return Err(FeatureError("The sweep path must be one connected chain of lines and arcs.".into()));
            }
        }
    }
    let chain = dedupe(&chain, false, 1e-9);
    if chain.len() < 2 {
        return Err(FeatureError("The sweep path is too short.".into()));
    }
    if dist(chain[0], chain[chain.len() - 1]) <= tol {
        return Err(FeatureError(
            "The sweep path must be open in this version (its two ends can't meet).".into(),
        ));
    }
    Ok(chain)
}

/// 1.x `_cross2`: the z part of the cross product of two 2D vectors.
pub fn cross2(u: P2, v: P2) -> f64 {
    u[0] * v[1] - u[1] * v[0]
}

/// 1.x `_path_self_crosses`: does the polyline cross or touch itself (neighbouring segments
/// don't count)?
pub fn path_self_crosses(p: &[P2]) -> bool {
    if p.len() < 2 {
        return false;
    }
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for q in p {
        for k in 0..2 {
            lo[k] = lo[k].min(q[k]);
            hi[k] = hi[k].max(q[k]);
        }
    }
    let eps = 1e-9 * f64::max(1.0, f64::max(hi[0] - lo[0], hi[1] - lo[1]));
    let sg = |x: f64| if x.abs() <= eps { 0.0 } else { x.signum() };
    let sub = |a: P2, b: P2| [a[0] - b[0], a[1] - b[1]];
    let nseg = p.len() - 1;
    for i in 0..nseg {
        let (a, b) = (p[i], p[i + 1]);
        for j in (i + 2)..nseg {
            let (c, d) = (p[j], p[j + 1]);
            let d1 = sg(cross2(sub(b, a), sub(c, a)));
            let d2 = sg(cross2(sub(b, a), sub(d, a)));
            let d3 = sg(cross2(sub(d, c), sub(a, c)));
            let d4 = sg(cross2(sub(d, c), sub(b, c)));
            let hit = if d1 == 0.0 && d2 == 0.0 {
                (0..2).all(|k| {
                    f64::max(a[k].min(b[k]), c[k].min(d[k])) <= f64::min(a[k].max(b[k]), c[k].max(d[k])) + eps
                })
            } else {
                d1 * d2 <= 0.0 && d3 * d4 <= 0.0
            };
            if hit {
                return true;
            }
        }
    }
    false
}

/// 1.x `_rot_between`: the smallest rotation taking unit vector `a` onto unit vector `b`.
pub fn rot_between(a: Vec3, b: Vec3) -> [[f64; 3]; 3] {
    let c = dot(a, b);
    if c < -1.0 + 1e-12 {
        let mut w = cross(a, [1.0, 0.0, 0.0]);
        if dot(w, w).sqrt() < 1e-6 {
            w = cross(a, [0.0, 1.0, 0.0]);
        }
        let n = dot(w, w).sqrt();
        let w = [w[0] / n, w[1] / n, w[2] / n];
        let mut r = [[0.0; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                r[i][j] = 2.0 * w[i] * w[j] - if i == j { 1.0 } else { 0.0 };
            }
        }
        return r;
    }
    let v = cross(a, b);
    let k = [[0.0, -v[2], v[1]], [v[2], 0.0, -v[0]], [-v[1], v[0], 0.0]];
    let mut r = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            let kk: f64 = (0..3).map(|m| k[i][m] * k[m][j]).sum();
            r[i][j] = (if i == j { 1.0 } else { 0.0 }) + k[i][j] + kk / (1.0 + c);
        }
    }
    r
}

/// geometry.frame_matrix: the 3x4 matrix taking local `(u, v, w)` to `o + u x + v y + w n`.
pub fn frame_matrix(f: &Frame) -> [[f64; 4]; 3] {
    let mut m = [[0.0; 4]; 3];
    for i in 0..3 {
        m[i] = [f.x[i], f.y[i], f.n[i], f.o[i]];
    }
    m
}

/// geometry.make_cylinder: a cylinder or cone from `p0` along `direction` for `length`.
/// `r1` defaults to `r0`.
pub fn make_cylinder(
    p0: Vec3,
    direction: Vec3,
    length: f64,
    r0: f64,
    r1: Option<f64>,
    segments: u32,
) -> Result<Solid, FeatureError> {
    let c = Solid::cylinder(length, r0, r1.unwrap_or(r0), segments, false);
    let f = plane_frame(p0, direction, None)?;
    Ok(c.transform(frame_matrix(&f)))
}

fn body_points(b: &Body) -> Vec<Vec3> {
    let solid_mesh;
    let mesh: &MeshArrays = match (&b.solid, &b.mesh) {
        (Some(s), _) => {
            solid_mesh = s.to_mesh();
            &solid_mesh
        }
        (None, Some(m)) => m,
        (None, None) => return Vec::new(),
    };
    mesh.positions.chunks_exact(3).map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]).collect()
}

/// geometry.big_extent: a length longer than anything in the design (for "through all").
pub fn big_extent<'a>(bodies: impl IntoIterator<Item = &'a Body>) -> f64 {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for b in bodies {
        for p in body_points(b) {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
    }
    if !lo.iter().all(|v| v.is_finite()) {
        return 1000.0;
    }
    let d = (0..3).map(|k| (hi[k] - lo[k]).powi(2)).sum::<f64>().sqrt();
    let m = lo.iter().chain(hi.iter()).fold(0.0f64, |a, v| a.max(v.abs()));
    d * 2.0 + 10.0 + m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn dedupe_drops_repeats() {
        let pts = [[0.0, 0.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 0.0]];
        assert_eq!(dedupe(&pts, false, 1e-9).len(), 4);
        assert_eq!(dedupe(&pts, true, 1e-9), vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]);
    }

    #[test]
    fn ccw_flips_clockwise() {
        let cw = [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]];
        assert_eq!(ccw(&cw), vec![[1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.0]]);
        let ok = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]];
        assert_eq!(ccw(&ok), ok.to_vec());
    }

    #[test]
    fn resample_matches_1x() {
        // 1.x: _resample(array([[0,0],[2,0],[2,1],[0,1]]), 5)
        let r = resample(&[[0.0, 0.0], [2.0, 0.0], [2.0, 1.0], [0.0, 1.0]], 5);
        let want = [[0.0, 0.0], [1.2, 0.0], [2.0, 0.4], [1.4, 1.0], [0.2, 1.0]];
        for (a, b) in r.iter().zip(want.iter()) {
            assert!(close(a[0], b[0]) && close(a[1], b[1]), "{a:?} vs {b:?}");
        }
    }

    #[test]
    fn cross2_is_z_of_cross() {
        assert_eq!(cross2([1.0, 0.0], [0.0, 1.0]), 1.0);
        assert_eq!(cross2([2.0, 3.0], [4.0, 5.0]), -2.0);
    }

    #[test]
    fn self_crossing_paths() {
        // a bow tie crosses; an L does not; a path that comes back to touch an earlier point does
        assert!(path_self_crosses(&[[0.0, 0.0], [2.0, 2.0], [2.0, 0.0], [0.0, 2.0]]));
        assert!(!path_self_crosses(&[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0]]));
        assert!(path_self_crosses(&[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [1.0, 2.0], [1.0, 0.0]]));
        // collinear overlap
        assert!(path_self_crosses(&[[0.0, 0.0], [3.0, 0.0], [3.0, 1.0], [1.0, 1.0], [1.0, 0.0], [2.0, 0.0]]));
        // two segments only: nothing to compare
        assert!(!path_self_crosses(&[[0.0, 0.0], [1.0, 0.0]]));
    }

    #[test]
    fn rot_between_matches_1x() {
        // 1.x: _rot_between(x, y) = [[0,-1,0],[1,0,0],[0,0,1]]
        let r = rot_between([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        let want = [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        for i in 0..3 {
            for j in 0..3 {
                assert!(close(r[i][j], want[i][j]));
            }
        }
        // opposite vectors: a half turn about the axis perpendicular to a
        let r = rot_between([1.0, 0.0, 0.0], [-1.0, 0.0, 0.0]);
        let want = [[-1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]];
        for i in 0..3 {
            for j in 0..3 {
                assert!(close(r[i][j], want[i][j]), "{r:?}");
            }
        }
        // identity
        let r = rot_between([0.0, 0.0, 1.0], [0.0, 0.0, 1.0]);
        for i in 0..3 {
            assert!(close(r[i][i], 1.0));
        }
    }

    #[test]
    fn bbox_and_overlaps() {
        let a = Solid::cube([1.0, 1.0, 1.0], false);
        let b = Solid::cube([1.0, 1.0, 1.0], false).translate([1.0, 0.0, 0.0]);
        let c = Solid::cube([1.0, 1.0, 1.0], false).translate([1.1, 0.0, 0.0]);
        let ba = bbox(&a);
        assert_eq!(ba.max, [1.0, 1.0, 1.0]);
        assert!(overlaps(&ba, &bbox(&b), 1e-6)); // touching counts
        assert!(!overlaps(&ba, &bbox(&c), 1e-6));
        assert!(overlaps(&ba, &bbox(&c), 0.2));
    }

    #[test]
    fn align_picks_the_best_start() {
        let prev = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]];
        let cur = [[1.0, 1.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
        assert_eq!(align(&prev, &cur), prev.to_vec());
    }

    #[test]
    fn frame_matrix_and_cylinder() {
        let f = Frame { o: [1.0, 2.0, 3.0], x: [1.0, 0.0, 0.0], y: [0.0, 1.0, 0.0], n: [0.0, 0.0, 1.0] };
        let m = frame_matrix(&f);
        assert_eq!(m[0], [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(m[2], [0.0, 0.0, 1.0, 3.0]);
        let c = make_cylinder([0.0; 3], [1.0, 0.0, 0.0], 10.0, 2.0, None, 64).unwrap();
        let bb = c.bounding_box();
        assert!((bb.max[0] - 10.0).abs() < 1e-9 && bb.min[0].abs() < 1e-9);
        assert!(make_cylinder([0.0; 3], [0.0; 3], 10.0, 2.0, None, 64).is_err());
    }

    #[test]
    fn big_extent_of_nothing_is_1000() {
        assert_eq!(big_extent(std::iter::empty()), 1000.0);
    }

    #[test]
    fn mesh_solid_rejects_open_meshes() {
        let v = [[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let e = mesh_solid(&v, &[[0, 1, 2]], "loft").err().unwrap();
        assert!(e.0.starts_with("The loft couldn't be closed into a solid."));
    }
}
