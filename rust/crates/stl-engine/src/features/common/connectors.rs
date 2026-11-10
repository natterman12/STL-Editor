//! 1.x `printing.split_with_connectors`: cut a solid by a plane and, optionally, add
//! alignment connectors (pins, dowels, filament holes, magnet pockets) across the cut.

use serde_json::{Map, Value};
use stl_kernel::Solid;

use super::float;
use super::frames::{dot, plane_frame, unit, Vec3};
use crate::FeatureError;

/// `(pin diameter, total pin length, hole clearance per side)` for a connector mode.
pub fn connector_defaults(mode: &str) -> Option<(f64, f64, f64)> {
    match mode {
        "pins" => Some((5.0, 10.0, 0.15)),
        "dowels" => Some((5.0, 12.0, 0.15)),
        "filament" => Some((1.75, 10.0, 0.15)),
        "magnets" => Some((6.0, 3.0, 0.15)),
        _ => None,
    }
}

/// What a split gave: the two halves, the separate dowel bodies and the connector counts.
pub struct SplitResult {
    pub above: Solid,
    pub below: Solid,
    pub extras: Vec<Solid>,
    pub connectors: usize,
    pub skipped: bool,
}

fn add_scaled(a: Vec3, b: Vec3, s: f64) -> Vec3 {
    [a[0] + b[0] * s, a[1] + b[1] * s, a[2] + b[2] * s]
}

/// geometry.make_cylinder: a cylinder or cone from `p0` along `dir` for `length`. As in 1.x,
/// the cylinder comes with its move into place still to apply (`Solid::transform_pending`).
pub fn make_cylinder(p0: Vec3, dir: Vec3, length: f64, r0: f64, r1: f64, segments: u32) -> Result<Solid, FeatureError> {
    let c = Solid::cylinder(length, r0, r1, segments, false);
    let f = plane_frame(p0, dir, None)?;
    Ok(c.transform_pending([
        [f.x[0], f.y[0], f.n[0], f.o[0]],
        [f.x[1], f.y[1], f.n[1], f.o[1]],
        [f.x[2], f.y[2], f.n[2], f.o[2]],
    ]))
}

/// Split a solid by the plane through `o` with normal `n`. `mode` is "none" or a connector
/// kind; `pin_d`, `pin_len` and `clearance` fall back to the mode's defaults (0 counts as
/// unset for the first two, as in 1.x).
pub fn split_with_connectors(
    solid: &Solid,
    o: Vec3,
    n: Vec3,
    mode: &str,
    pin_d: Option<&Value>,
    pin_len: Option<&Value>,
    clearance: Option<&Value>,
) -> Result<SplitResult, FeatureError> {
    let n = unit(n)?;
    let (above, below) = solid.split_by_plane(n, dot(n, o));
    let mut out = SplitResult { above, below, extras: Vec::new(), connectors: 0, skipped: false };
    if matches!(mode, "" | "none") || out.above.is_empty() || out.below.is_empty() {
        return Ok(out);
    }
    let (d0, l0, c0) = connector_defaults(mode).ok_or_else(|| FeatureError(format!("Unknown connector {mode}.")))?;
    // 1.x: float(pin_d or d0), float(pin_len or L0), float(clearance if clearance is not None else c0)
    let falsy = |v: &Value| match v {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::Number(n) => n.as_f64() == Some(0.0),
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
    };
    let num = |v: &Value, k: &str| -> Result<f64, FeatureError> {
        let mut m = Map::new();
        m.insert(k.to_string(), v.clone());
        float(&m, k, None)
    };
    let d = match pin_d.filter(|v| !falsy(v)) {
        Some(v) => num(v, "pin_d")?,
        None => d0,
    };
    let len = match pin_len.filter(|v| !falsy(v)) {
        Some(v) => num(v, "pin_len")?,
        None => l0,
    };
    let c = match clearance.filter(|v| !v.is_null()) {
        Some(v) => num(v, "clearance")?,
        None => c0,
    };
    let r = d / 2.0;
    // filament pins are meant for thin shells, so they may sit closer to the surface
    let wall = if mode == "filament" { 0.45 } else { (0.6 * r).max(1.2) };
    let pts = connector_points(solid, o, n, r + c, wall)?;
    if pts.is_empty() {
        out.skipped = true;
        return Ok(out);
    }
    let hole_r = r + c;
    // 1.x re-reads the normal from the section's frame (a second unit(), which can differ in
    // the last bit); the pins and holes use that one
    let n = plane_frame(o, n, None)?.n;
    let neg_n = [-n[0], -n[1], -n[2]];
    // 1.x chains `above + pin + tip` and `below - hole` without working any of them out, and
    // the kernel then does each chain as one operation: one union of the half and all its
    // pins, and one cut by the union of the holes, both with the last piece first.
    let mut add_above: Vec<Solid> = vec![out.above.clone()];
    let mut cut_above: Vec<Solid> = Vec::new();
    let mut cut_below: Vec<Solid> = Vec::new();
    for p in pts {
        match mode {
            "pins" => {
                let pin = make_cylinder(p, neg_n, len / 2.0 - 0.2, r, r, 48)?;
                let tip = make_cylinder(add_scaled(p, n, -(len / 2.0 - 0.2)), neg_n, 0.5, r, (r - 0.5).max(0.2), 48)?;
                add_above.push(pin);
                add_above.push(tip);
                cut_below.push(make_cylinder(add_scaled(p, n, 0.01), neg_n, len / 2.0 + 0.4, hole_r, hole_r, 48)?);
            }
            "magnets" => {
                let depth = len + c + 0.1;
                cut_above.push(make_cylinder(add_scaled(p, n, -0.01), n, depth, hole_r, hole_r, 48)?);
                cut_below.push(make_cylinder(add_scaled(p, n, 0.01), neg_n, depth, hole_r, hole_r, 48)?);
            }
            _ => {
                let half = len / 2.0 + 0.4;
                cut_above.push(make_cylinder(add_scaled(p, n, -0.01), n, half, hole_r, hole_r, 48)?);
                cut_below.push(make_cylinder(add_scaled(p, n, 0.01), neg_n, half, hole_r, hole_r, 48)?);
                if mode == "dowels" {
                    out.extras.push(make_cylinder(add_scaled(p, n, -(len / 2.0 - 0.2)), n, len - 0.4, r - 0.05, r - 0.05, 48)?);
                }
            }
        }
        out.connectors += 1;
    }
    if add_above.len() > 1 {
        add_above.reverse();
        out.above = Solid::union_all(&add_above);
    }
    if !cut_above.is_empty() {
        cut_above.reverse();
        out.above = out.above.difference(&Solid::union_all(&cut_above));
    }
    if !cut_below.is_empty() {
        cut_below.reverse();
        out.below = out.below.difference(&Solid::union_all(&cut_below));
    }
    Ok(out)
}

/// Even-odd containment against all contours (what 1.x gets from the symmetric difference of
/// the contour polygons).
fn inside(contours: &[Vec<[f64; 2]>], x: f64, y: f64) -> bool {
    let mut odd = false;
    for c in contours {
        let mut j = c.len() - 1;
        for i in 0..c.len() {
            let (a, b) = (c[i], c[j]);
            if (a[1] > y) != (b[1] > y) && x < (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0] {
                odd = !odd;
            }
            j = i;
        }
    }
    odd
}

/// Distance from a point to the nearest contour segment (shapely `distance` to the boundary).
fn edge_distance(contours: &[Vec<[f64; 2]>], x: f64, y: f64) -> f64 {
    let mut best = f64::INFINITY;
    for c in contours {
        let mut j = c.len() - 1;
        for i in 0..c.len() {
            let (a, b) = (c[j], c[i]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            // GEOS Distance::pointToSegment
            let len2 = dx * dx + dy * dy;
            let r = ((x - a[0]) * dx + (y - a[1]) * dy) / len2;
            let d = if r <= 0.0 {
                ((x - a[0]) * (x - a[0]) + (y - a[1]) * (y - a[1])).sqrt()
            } else if r >= 1.0 {
                ((x - b[0]) * (x - b[0]) + (y - b[1]) * (y - b[1])).sqrt()
            } else {
                let sg = ((a[1] - y) * dx - (a[0] - x) * dy) / len2;
                sg.abs() * len2.sqrt()
            };
            best = best.min(d);
            j = i;
        }
    }
    best
}

/// shapely `representative_point()` (GEOS InteriorPointArea): scan at the y halfway between the
/// vertex y-values nearest to the middle of the bounds (below and above it, every ring), then
/// take the middle of the widest stretch inside the shape.
fn representative_point(contours: &[Vec<[f64; 2]>], lo: [f64; 2], hi: [f64; 2]) -> [f64; 2] {
    let centre = (lo[1] + hi[1]) / 2.0;
    let (mut lo_y, mut hi_y) = (lo[1], hi[1]);
    for c in contours {
        for v in c {
            if v[1] <= centre {
                if v[1] > lo_y {
                    lo_y = v[1];
                }
            } else if v[1] < hi_y {
                hi_y = v[1];
            }
        }
    }
    let y = (lo_y + hi_y) / 2.0;
    let mut xs = Vec::new();
    for c in contours {
        let mut j = c.len() - 1;
        for i in 0..c.len() {
            let (a, b) = (c[i], c[j]);
            if (a[1] > y) != (b[1] > y) {
                xs.push((b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0]);
            }
            j = i;
        }
    }
    xs.sort_by(f64::total_cmp);
    let mut best = ([(lo[0] + hi[0]) / 2.0, y], -1.0);
    // JTS keeps the first widest stretch (strict >)
    for w in xs.chunks_exact(2) {
        if w[1] - w[0] > best.1 {
            best = ([(w[0] + w[1]) / 2.0, y], w[1] - w[0]);
        }
    }
    best.0
}

/// 1.x `connector_points` on the section of `solid` with the plane: spread-out centres in
/// world coordinates, keeping `wall` of material round each (and at most 4 per piece).
fn connector_points(solid: &Solid, o: Vec3, n: Vec3, radius: f64, wall: f64) -> Result<Vec<Vec3>, FeatureError> {
    let f = plane_frame(o, n, None)?;
    // world -> plane frame: rows x, y, n; translation -R o
    let m = [
        [f.x[0], f.x[1], f.x[2], -dot(f.x, f.o)],
        [f.y[0], f.y[1], f.y[2], -dot(f.y, f.o)],
        [f.n[0], f.n[1], f.n[2], -dot(f.n, f.o)],
    ];
    let inner = solid.transform(m).slice(0.0).offset_round(-(radius + wall));
    if inner.is_empty() || inner.area() <= 1e-9 {
        return Ok(Vec::new());
    }
    // area once per piece; stable sort, largest first (1.x sorted(key=-area))
    let mut comps: Vec<(f64, _)> = inner.decompose().into_iter().map(|c| (c.area(), c)).collect();
    comps.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut pts: Vec<[f64; 2]> = Vec::new();
    for (_, comp) in comps {
        let contours = &comp.contours;
        // bounds from the vertices (the polygon's extent, as 1.x reads it)
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for v in contours.iter().flatten() {
            for k in 0..2 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
        if !lo[0].is_finite() {
            continue;
        }
        let ext = (hi[0] - lo[0]).max(hi[1] - lo[1]);
        let step = (ext / 60.0).max(radius * 0.25);
        // numpy arange(lo, hi + step, step)
        let arange = |a: f64, b: f64| -> Vec<f64> {
            let cnt = ((b + step - a) / step).ceil().max(0.0) as usize;
            // numpy fills with start + i * ((start + step) - start)
            let delta = (a + step) - a;
            (0..cnt).map(|i| a + i as f64 * delta).collect()
        };
        let (gx, gy) = (arange(lo[0], hi[0]), arange(lo[1], hi[1]));
        let mut cand: Vec<[f64; 2]> = Vec::new();
        for &y in &gy {
            for &x in &gx {
                if inside(contours, x, y) && edge_distance(contours, x, y) > 0.0 {
                    cand.push([x, y]);
                }
            }
        }
        if cand.is_empty() {
            cand.push(representative_point(contours, lo, hi));
        }
        let n_here = ((ext / (radius * 10.0)).round_ties_even().clamp(1.0, 4.0)) as usize;
        let argmax = |v: &[f64]| -> usize {
            let mut k = 0;
            for (i, x) in v.iter().enumerate() {
                if *x > v[k] {
                    k = i;
                }
            }
            k
        };
        let dist_edge: Vec<f64> = cand.iter().map(|c| edge_distance(contours, c[0], c[1])).collect();
        let mut chosen = vec![cand[argmax(&dist_edge)]];
        for _ in 1..n_here {
            let dmin: Vec<f64> = cand
                .iter()
                .map(|c| chosen.iter().map(|q| ((c[0] - q[0]) * (c[0] - q[0]) + (c[1] - q[1]) * (c[1] - q[1])).sqrt()).fold(f64::INFINITY, f64::min))
                .collect();
            let k = argmax(&dmin);
            if dmin[k] < radius * 4.0 {
                break;
            }
            chosen.push(cand[k]);
        }
        pts.extend(chosen);
        if pts.len() >= 8 {
            break;
        }
    }

    Ok(pts
        .into_iter()
        .map(|[u, v]| [f.o[0] + u * f.x[0] + v * f.y[0], f.o[1] + u * f.x[1] + v * f.y[1], f.o[2] + u * f.x[2] + v * f.y[2]])
        .collect())
}

#[cfg(test)]
mod tests {
    use super::representative_point;

    #[test]
    fn representative_point_matches_shapely() {
        let c = vec![vec![[0.0, 0.0], [10.0, 0.0], [10.0, 6.0], [0.0, 10.0]]];
        assert_eq!(representative_point(&c, [0.0, 0.0], [10.0, 10.0]), [5.0, 3.0]);
    }
}
