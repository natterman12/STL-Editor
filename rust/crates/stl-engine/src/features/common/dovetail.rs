//! 2.0 sliding dovetail connectors for Split and Fit to Bed (`connector: "dovetail"`; 1.x has
//! none). The half above the cut gets a tongue that hangs down from the cut face; the half
//! below gets a matching groove. Both run in a straight line across their piece of the cut
//! face and out through its edge, so the halves slide together from the side and can't be
//! pulled apart. The tongue widens towards its tip by [`FLARE_DEG`] each side, which is what
//! locks it, and prints without supports when the half is laid on its cut face.
//!
//! Geometry is worked in the cut plane's frame (`connectors::connector_points` uses the same
//! one): `u, v` on the plane and depth below it along `-n`. The groove is the tongue grown by
//! the clearance on its sides and tip, so the gap is the same on every face of the joint.

use stl_kernel::{Shape2d, Solid};

use super::frames::{Frame, Vec3};

/// How far each side of the tongue leans out from square, in degrees. Wood dovetails use 8 to
/// 14; 15 holds better in plastic and is still a gentle overhang to print.
pub const FLARE_DEG: f64 = 15.0;

/// A dovetail's size: `width` across its root on the cut face, `depth` below the cut face,
/// and the gap `clearance` left on each side of it.
#[derive(Clone, Copy, Debug)]
pub struct Dovetail {
    pub width: f64,
    pub depth: f64,
    pub clearance: f64,
}

impl Dovetail {
    /// Half the tongue's width at its tip.
    pub fn tip_half(&self) -> f64 {
        self.width / 2.0 + self.depth * FLARE_DEG.to_radians().tan()
    }
}

/// The long axis of a flat shape as a unit `[u, v]`: the direction of its least second moment
/// of area. A shape with no long axis (a square, a circle) gives the plane's own `u`; an axis
/// within a millionth of a radian of `u` or `v` is snapped to it, so a box that is cut square
/// gets a dovetail that is square to it.
pub fn long_axis(shape: &Shape2d) -> [f64; 2] {
    // area moments about the centroid, from the contours (holes run the other way and subtract)
    let (mut a, mut cx, mut cy, mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for c in &shape.contours {
        for i in 0..c.len() {
            let (p, q) = (c[i], c[(i + 1) % c.len()]);
            let k = p[0] * q[1] - q[0] * p[1];
            a += k;
            cx += (p[0] + q[0]) * k;
            cy += (p[1] + q[1]) * k;
            sxx += (p[1] * p[1] + p[1] * q[1] + q[1] * q[1]) * k;
            syy += (p[0] * p[0] + p[0] * q[0] + q[0] * q[0]) * k;
            sxy += (p[0] * q[1] + 2.0 * p[0] * p[1] + 2.0 * q[0] * q[1] + q[0] * p[1]) * k;
        }
    }
    if a.abs() < 1e-12 {
        return [1.0, 0.0];
    }
    let area = a / 2.0;
    let (cx, cy) = (cx / (3.0 * a), cy / (3.0 * a));
    // second moments about the axes through the centroid: Iu = ∫v², Iv = ∫u², Iuv = ∫uv
    let iu = sxx / 12.0 - area * cy * cy;
    let iv = syy / 12.0 - area * cx * cx;
    let iuv = sxy / 24.0 - area * cx * cy;
    let scale = (iu.abs() + iv.abs()).max(1e-300);
    if (iu - iv).abs() < 1e-6 * scale && iuv.abs() < 1e-6 * scale {
        return [1.0, 0.0];
    }
    // the long side is the direction t that maximises ∫((u, v)·t)² over the shape: the main
    // eigenvector of [[iv, iuv], [iuv, iu]]
    let theta = 0.5 * (2.0 * iuv).atan2(iv - iu);
    let snap = |t: f64| -> f64 {
        for s in [0.0, std::f64::consts::FRAC_PI_2, -std::f64::consts::FRAC_PI_2] {
            if (t - s).abs() < 1e-6 {
                return s;
            }
        }
        t
    };
    let theta = snap(theta);
    let d = [theta.cos(), theta.sin()];
    // one sign for every run: the half with u > 0, else v > 0
    if d[0] < -1e-12 || (d[0].abs() <= 1e-12 && d[1] < 0.0) {
        [-d[0], -d[1]]
    } else {
        d
    }
}

/// `t` turned a quarter turn: the other in-plane direction.
pub fn across(t: [f64; 2]) -> [f64; 2] {
    [-t[1], t[0]]
}

/// A world point at `u, v` on the plane of `f` and `depth` below it (along `-n`).
fn at(f: &Frame, u: f64, v: f64, depth: f64) -> Vec3 {
    std::array::from_fn(|i| f.o[i] + u * f.x[i] + v * f.y[i] - depth * f.n[i])
}

/// A straight prism with a dovetail section running along `t` through `p` (plane `u, v`),
/// `reach` each way. `half(d)` is its half width at depth `d`; it spans depths `d0..d1`.
fn rail(f: &Frame, p: [f64; 2], t: [f64; 2], reach: f64, d0: f64, d1: f64, half: impl Fn(f64) -> f64) -> Solid {
    let s = across(t);
    let mut pts = Vec::with_capacity(8);
    for along in [-reach, reach] {
        for (d, side) in [(d0, -1.0), (d0, 1.0), (d1, -1.0), (d1, 1.0)] {
            let w = side * half(d);
            pts.push(at(f, p[0] + along * t[0] + w * s[0], p[1] + along * t[1] + w * s[1], d));
        }
    }
    Solid::hull_points(&pts)
}

/// `piece` (a piece of the cut face, in plane `u, v`) as a prism from `d0` to `d1` below the plane.
fn footprint(f: &Frame, piece: &Shape2d, d0: f64, d1: f64) -> Solid {
    // The prism runs up its z from 0, placed with columns (x, -y, -n): that frame is
    // right-handed (two flips of x, y, n), so the solid keeps its orientation; v is mirrored
    // to match, and each contour reversed so outlines still run counter-clockwise.
    let flipped = Shape2d {
        contours: piece
            .contours
            .iter()
            .map(|c| c.iter().rev().map(|p| [p[0], -p[1]]).collect())
            .collect(),
    };
    let o = at(f, 0.0, 0.0, d0);
    flipped.extrude(d1 - d0).transform([
        [f.x[0], -f.y[0], -f.n[0], o[0]],
        [f.x[1], -f.y[1], -f.n[1], o[1]],
        [f.x[2], -f.y[2], -f.n[2], o[2]],
    ])
}

/// One dovetail at `p` running along `t`, on `piece` of the cut face. Returns the tongue (to
/// join to the half above; it stops at the surface of `below`, so it never pokes out) and the
/// groove (to cut from the half below).
pub fn dovetail_pair(
    f: &Frame,
    piece: &Shape2d,
    below: &Solid,
    p: [f64; 2],
    t: [f64; 2],
    dt: &Dovetail,
) -> (Solid, Solid) {
    let tan = FLARE_DEG.to_radians().tan();
    let (lo, hi) = piece.bounds().unwrap_or(([0.0; 2], [0.0; 2]));
    // far enough to leave the piece on both sides wherever `p` is in it
    let reach = (hi[0] - lo[0]).hypot(hi[1] - lo[1]) + dt.width + dt.depth + 2.0;
    let (w, h, c) = (dt.width, dt.depth, dt.clearance);
    // the tongue starts a little above the cut face so it fuses with the half above
    let tongue = rail(f, p, t, reach, -0.01, h, |d| w / 2.0 + d * tan);
    let groove = rail(f, p, t, reach, -0.01, h + c, |d| w / 2.0 + d * tan + c);
    // keep each to its own piece of the cut face (a cut through two legs gives each its own)
    let own = footprint(f, piece, -0.5, h + c + 0.5);
    let own_grown = footprint(f, &piece.offset_round(c + 0.3), -0.5, h + c + 0.5);
    let tongue = tongue.intersection(&own).intersection(below);
    let groove = groove.intersection(&own_grown);
    (tongue, groove)
}
