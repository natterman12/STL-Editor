//! 2.0 connector options for Split and Fit to Bed, on top of 1.x's pins, dowels, filament pins
//! and magnet pockets (`connectors.rs`): where the connectors go (`connector_place`: worked
//! out from the cut face as 1.x does, optionally with `connector_count` per piece; one in the
//! centre of each piece; or the `connector_points` the user clicked) and the sliding dovetail
//! (`dovetail.rs`). With none of these keys a step cuts exactly as 1.x does.

use serde_json::{Map, Value};
use stl_kernel::{Shape2d, Solid};

use super::connectors::{connector_points_uv, edge_distance, inside, section};
use super::dovetail::{Dovetail, across, dovetail_pair, long_axis};
use super::float;
use super::frames::{Frame, dot};
use crate::FeatureError;

/// Where the connectors go on the cut face.
#[derive(Clone, Debug, PartialEq)]
pub enum Placement {
    /// Spread out over each piece of the cut face; `Some(k)` asks for `k` on each piece in
    /// place of the number 1.x works out from its size.
    Auto(Option<usize>),
    /// One in the middle of each piece (or the nearest spot with room for it).
    Centre,
    /// At these world points, moved onto the cut plane along its normal.
    Manual(Vec<[f64; 3]>),
}

/// The 2.0 connector settings read from a step's params.
#[derive(Clone, Debug, PartialEq)]
pub struct ConnectorOpts {
    pub place: Placement,
    pub dovetail_width: f64,
    pub dovetail_depth: f64,
    /// The dovetail runs across the cut face's short side (the halves slide the short way).
    pub slide_short: bool,
}

impl Default for ConnectorOpts {
    fn default() -> Self {
        ConnectorOpts {
            place: Placement::Auto(None),
            dovetail_width: 10.0,
            dovetail_depth: 5.0,
            slide_short: false,
        }
    }
}

fn point3(v: &Value) -> Option<[f64; 3]> {
    let a = v.as_array()?;
    if a.len() != 3 {
        return None;
    }
    Some([a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?])
}

impl ConnectorOpts {
    pub fn from_params(p: &Map<String, Value>) -> Result<ConnectorOpts, FeatureError> {
        let d = ConnectorOpts::default();
        let place = match p.get("connector_place").and_then(Value::as_str).unwrap_or("auto") {
            "centre" => Placement::Centre,
            "manual" => {
                let list = p.get("connector_points").and_then(Value::as_array);
                let mut pts = Vec::new();
                for v in list.into_iter().flatten() {
                    pts.push(point3(v).ok_or_else(|| FeatureError("A connector spot is not a point.".into()))?);
                }
                Placement::Manual(pts)
            }
            _ => {
                let k = float(p, "connector_count", Some(0.0))?;
                if !(0.0..=50.0).contains(&k) {
                    return Err(FeatureError("How many must be between 0 (Auto) and 50.".into()));
                }
                Placement::Auto((k >= 1.0).then(|| k.round() as usize))
            }
        };
        let width = float(p, "dovetail_width", Some(d.dovetail_width))?;
        let depth = float(p, "dovetail_depth", Some(d.dovetail_depth))?;
        if !(width > 0.0 && depth > 0.0) {
            return Err(FeatureError(
                "The dovetail's width and depth must be more than 0.".into(),
            ));
        }
        Ok(ConnectorOpts {
            place,
            dovetail_width: width,
            dovetail_depth: depth,
            slide_short: p.get("dovetail_slide").and_then(Value::as_str) == Some("short"),
        })
    }
}

/// The spots for `place` on the cut face of `solid` with the plane of `f`, in the plane's
/// `u, v`, for connectors of `radius` that keep `wall` of material round them. The flag is
/// true when Auto with a count found room for fewer than asked.
pub fn place_points(
    solid: &Solid,
    f: &Frame,
    radius: f64,
    wall: f64,
    place: &Placement,
) -> Result<(Vec<[f64; 2]>, bool), FeatureError> {
    match place {
        Placement::Auto(k) => connector_points_uv(solid, f, radius, wall, *k),
        Placement::Centre => Ok((centre_points(solid, f, radius, wall), false)),
        Placement::Manual(pts) => {
            let inner = section(solid, f).offset_round(-(radius + wall));
            let mut out = Vec::with_capacity(pts.len());
            for (i, p) in pts.iter().enumerate() {
                let r = [p[0] - f.o[0], p[1] - f.o[1], p[2] - f.o[2]];
                let uv = [dot(r, f.x), dot(r, f.y)];
                if !inside(&inner.contours, uv[0], uv[1]) || edge_distance(&inner.contours, uv[0], uv[1]) <= 0.0 {
                    return Err(FeatureError(format!(
                        "Spot {} is off the cut face or too close to its edge. Move it further in, or make the connectors smaller.",
                        i + 1
                    )));
                }
                out.push(uv);
            }
            Ok((out, false))
        }
    }
}

/// The area centroid of a flat shape (holes subtract), or `None` for no area.
fn centroid(shape: &Shape2d) -> Option<[f64; 2]> {
    let (mut a, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for c in &shape.contours {
        for i in 0..c.len() {
            let (p, q) = (c[i], c[(i + 1) % c.len()]);
            let k = p[0] * q[1] - q[0] * p[1];
            a += k;
            cx += (p[0] + q[0]) * k;
            cy += (p[1] + q[1]) * k;
        }
    }
    (a.abs() > 1e-12).then(|| [cx / (3.0 * a), cy / (3.0 * a)])
}

/// One spot per piece of the cut face: its centroid when there is room for the connector
/// there, else the spot with room nearest to it (a ring's centre is a hole). Pieces too small
/// for a connector get none. Largest piece first, as Auto.
fn centre_points(solid: &Solid, f: &Frame, radius: f64, wall: f64) -> Vec<[f64; 2]> {
    let mut pieces: Vec<(f64, Shape2d)> = section(solid, f)
        .decompose()
        .into_iter()
        .map(|c| (c.area(), c))
        .collect();
    pieces.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut out = Vec::new();
    for (_, piece) in pieces {
        let Some(c) = centroid(&piece) else { continue };
        let inner = piece.offset_round(-(radius + wall));
        if inner.is_empty() || inner.area() <= 1e-9 {
            continue;
        }
        let ok = |p: [f64; 2]| inside(&inner.contours, p[0], p[1]) && edge_distance(&inner.contours, p[0], p[1]) > 0.0;
        if ok(c) {
            out.push(c);
            continue;
        }
        // the nearest spot with room: on a fine grid over the room left, nearest first
        let Some((lo, hi)) = inner.bounds() else { continue };
        let step = ((hi[0] - lo[0]).max(hi[1] - lo[1]) / 80.0).max(1e-3);
        let mut best: Option<([f64; 2], f64)> = None;
        let (nx, ny) = (
            ((hi[0] - lo[0]) / step).ceil() as usize,
            ((hi[1] - lo[1]) / step).ceil() as usize,
        );
        for j in 0..=ny {
            for i in 0..=nx {
                let p = [lo[0] + i as f64 * step, lo[1] + j as f64 * step];
                if !ok(p) {
                    continue;
                }
                let d = (p[0] - c[0]).hypot(p[1] - c[1]);
                if best.is_none_or(|b| d < b.1) {
                    best = Some((p, d));
                }
            }
        }
        if let Some((p, _)) = best {
            out.push(p);
        }
    }
    out
}

/// What the dovetails did to the two halves.
pub struct DovetailCut {
    pub tongues: Vec<Solid>,
    pub grooves: Vec<Solid>,
    pub short: bool,
}

/// Dovetails across the cut face of `solid` with the plane of `f`: their spots from `opts`, one
/// rail through each, running along its piece's long side (or short side). Each piece gets one
/// dovetail on Auto; asking for more lays the extra ones side by side, a tongue's width and a
/// wall apart.
pub fn dovetails(
    solid: &Solid,
    below: &Solid,
    f: &Frame,
    dt: &Dovetail,
    opts: &ConnectorOpts,
) -> Result<DovetailCut, FeatureError> {
    let wall = 1.2_f64.max(0.3 * dt.width);
    let radius = dt.tip_half() + dt.clearance;
    let pieces: Vec<Shape2d> = section(solid, f).decompose();
    let piece_of = |p: [f64; 2]| pieces.iter().position(|s| inside(&s.contours, p[0], p[1]));
    let dir = |piece: &Shape2d| {
        let t = long_axis(piece);
        if opts.slide_short { across(t) } else { t }
    };
    let (spots, mut short) = match &opts.place {
        Placement::Auto(k) => {
            let (base, _) = connector_points_uv(solid, f, radius, wall, Some(1))?;
            let want = k.unwrap_or(1);
            let inner = section(solid, f).offset_round(-(radius + wall));
            let ok =
                |p: [f64; 2]| inside(&inner.contours, p[0], p[1]) && edge_distance(&inner.contours, p[0], p[1]) > 0.0;
            let mut spots = Vec::new();
            let mut short = false;
            let gap = 2.0 * radius + wall;
            // rails already laid, as (piece, offset across the rail): a piece the room left
            // splits in two (a dumbbell) gives two spots on much the same rail; keep one
            let mut laid: Vec<(usize, f64)> = Vec::new();
            for p0 in base {
                let Some(i) = piece_of(p0) else { continue };
                let s = across(dir(&pieces[i]));
                let off = |q: [f64; 2]| q[0] * s[0] + q[1] * s[1];
                if laid.iter().any(|&(pi, o)| pi == i && (o - off(p0)).abs() < gap * 0.99) {
                    continue;
                }
                let mut here = vec![p0];
                // alternate sides: +1, -1, +2, -2, ... while there is room
                let mut j = 1;
                while here.len() < want && j <= 2 * want {
                    let k = j.div_ceil(2) as f64 * if j % 2 == 1 { 1.0 } else { -1.0 };
                    let q = [p0[0] + k * gap * s[0], p0[1] + k * gap * s[1]];
                    if ok(q) && piece_of(q) == Some(i) {
                        here.push(q);
                    }
                    j += 1;
                }
                short |= here.len() < want;
                laid.extend(here.iter().map(|&q| (i, off(q))));
                spots.extend(here);
            }
            (spots, short)
        }
        other => place_points(solid, f, radius, wall, other)?,
    };
    let mut out = DovetailCut {
        tongues: Vec::new(),
        grooves: Vec::new(),
        short: false,
    };
    for p in spots {
        let Some(i) = piece_of(p) else {
            short = true;
            continue;
        };
        let (tongue, groove) = dovetail_pair(f, &pieces[i], below, p, dir(&pieces[i]), dt);
        out.tongues.push(tongue);
        out.grooves.push(groove);
    }
    out.short = short;
    Ok(out)
}
