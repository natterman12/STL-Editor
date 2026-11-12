//! Change the diameter of a fitted round hole or pin, or fill a hole (1.x
//! `meshtools.resize_round`). The checks that give messages (fill on a pin, no diameter) are
//! the caller's; here those cases return the solid unchanged.

use stl_kernel::{Frame, Solid};
use stl_mesh::fit::{CylKind, Cylinder};

use super::frames::{cross, dot};
use super::profiles::frame_matrix;

type V3 = [f64; 3];

fn unit(a: V3) -> V3 {
    let n = dot(a, a).sqrt();
    [a[0] / n, a[1] / n, a[2] / n]
}

fn at(c: V3, ax: V3, d: f64) -> V3 {
    [c[0] + ax[0] * d, c[1] + ax[1] * d, c[2] + ax[2] * d]
}

/// 1.x `_inside`: is a point inside the solid (a tiny cube probe overlaps it).
fn inside(s: &Solid, pt: V3) -> bool {
    let eps = 0.05_f64;
    let probe = Solid::cube([eps; 3], true).translate(pt);
    s.intersection(&probe).volume() > 0.5 * eps.powi(3)
}

/// 1.x `_tube`: a pipe (or rod, when `r_in` is tiny) along `axis` from `z0` to `z1`.
fn tube(center: V3, axis: V3, z0: f64, z1: f64, r_in: f64, r_out: f64) -> Solid {
    let h = z1 - z0;
    if h <= 1e-6 || r_out <= 1e-6 {
        return Solid::union_all(&[]);
    }
    let mut t = Solid::cylinder(h, r_out, r_out, 128, false);
    if r_in > 1e-6 {
        t = t.difference(&Solid::cylinder(h + 2.0, r_in, r_in, 128, false).translate([0.0, 0.0, -1.0]));
    }
    let axis = unit(axis);
    let rf = if axis[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let ex = unit(cross(axis, rf));
    let ey = cross(axis, ex);
    let o = at(center, axis, z0);
    t.transform(frame_matrix(&Frame { o, x: ex, y: ey, n: axis }))
}

pub fn resize_round(s: &Solid, cyl: &Cylinder, diameter: Option<f64>, fill: bool) -> Solid {
    let (c, ax) = (cyl.center, cyl.axis);
    let r0 = cyl.radius;
    let (z0, z1) = (cyl.z0, cyl.z1);
    let delta = 0.02_f64.max(r0 * 0.01);
    if fill {
        if cyl.kind != CylKind::Hole {
            return s.clone();
        }
        return s.union(&tube(c, ax, z0, z1, 0.0, r0 + delta));
    }
    let d = match diameter {
        Some(d) if d > 0.0 => d,
        _ => return s.clone(),
    };
    let r1 = d / 2.0;
    if (r1 - r0).abs() < 1e-4 {
        return s.clone();
    }
    if cyl.kind == CylKind::Hole {
        if r1 < r0 {
            return s.union(&tube(c, ax, z0, z1, r1, r0 + delta));
        }
        // open ends get cut slightly past the surface; a blind hole keeps its bottom
        let (mut lo, mut hi) = (z0, z1);
        if !inside(s, at(c, ax, z0 - 0.3)) {
            lo -= 0.5;
        }
        if !inside(s, at(c, ax, z1 + 0.3)) {
            hi += 0.5;
        }
        return s.difference(&tube(c, ax, lo, hi, (r0 - delta).max(0.0), r1));
    }
    // pin
    if r1 < r0 {
        let hi = z1 + if !inside(s, at(c, ax, z1 + 0.3)) { 0.5 } else { 0.0 };
        let lo = z0 - if !inside(s, at(c, ax, z0 - 0.3)) { 0.5 } else { 0.0 };
        return s.difference(&tube(c, ax, lo, hi, r1, r0 + delta + 0.5));
    }
    s.union(&tube(c, ax, z0, z1, (r0 - delta).max(0.0), r1))
}
