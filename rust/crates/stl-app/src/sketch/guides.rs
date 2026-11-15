//! Tracking guides and the grid snap that follows the zoom (spec 2.3).

use stl_sketch::entities::P2;

/// A dotted guide from a tracked point to the snapped cursor.
#[derive(Clone, Debug, PartialEq)]
pub struct Guide {
    pub from: P2,
    pub to: P2,
}

/// How close (screen points) the pointer must be to a point's X or Y to line up with it.
pub const TRACK_PX: f64 = 6.0;

/// The closest the grid lines are drawn to each other, in screen points.
pub const GRID_MIN_PX: f64 = 12.0;

/// Line the pointer up with the X (first) and Y (second) of the nearest point within `tol`:
/// `(the aligned coordinate, the point it came from)`. Points on the pointer itself (within
/// `tol` on both axes) are left to the point snap. The nearest point wins; a tie goes to the
/// earlier point.
#[allow(clippy::type_complexity)]
pub fn align(cursor: P2, points: &[P2], tol: f64) -> (Option<(f64, P2)>, Option<(f64, P2)>) {
    let mut best: [Option<(f64, f64, P2)>; 2] = [None, None];
    for p in points {
        let d = [(p[0] - cursor[0]).abs(), (p[1] - cursor[1]).abs()];
        if d[0] <= tol && d[1] <= tol {
            continue;
        }
        for ax in 0..2 {
            if d[ax] <= tol && best[ax].as_ref().is_none_or(|b| d[ax] < b.1 - 1e-12) {
                best[ax] = Some((p[ax], d[ax], *p));
            }
        }
    }
    (best[0].map(|b| (b.0, b.2)), best[1].map(|b| (b.0, b.2)))
}

/// Snap the cursor to horizontal or vertical alignment with any of `points` within `tol`.
/// Returns the snapped point and the guides to draw (one per aligned axis).
pub fn track(cursor: P2, points: &[P2], tol: f64) -> (P2, Vec<Guide>) {
    let (ax, ay) = align(cursor, points, tol);
    let mut at = cursor;
    let mut from = Vec::new();
    if let Some((x, p)) = ax {
        at[0] = x;
        from.push(p);
    }
    if let Some((y, p)) = ay {
        at[1] = y;
        from.push(p);
    }
    (at, from.into_iter().map(|p| Guide { from: p, to: at }).collect())
}

/// 1, 2, 5, 10, 20, 50... in `unit`s (a decade is a factor of ten).
fn decades(min: f64) -> f64 {
    let base = 10f64.powf(min.log10().floor());
    for m in [1.0, 2.0, 5.0, 10.0] {
        let v = base * m;
        if v >= min * (1.0 - 1e-9) {
            return (v * 1e9).round() / 1e9;
        }
    }
    base * 10.0
}

/// The spacing the grid draws at this zoom (1-2-5 steps), in millimetres: the smallest step
/// whose lines are at least [`GRID_MIN_PX`] screen points apart.
pub fn visible_grid_step(px_per_mm: f64) -> f64 {
    if !px_per_mm.is_finite() || px_per_mm <= 0.0 {
        return 1.0;
    }
    decades(GRID_MIN_PX / px_per_mm)
}

/// [`visible_grid_step`] for inch display: the steps are 1/64, 1/32 ... 1/2, 1, 2, 5, 10... in
/// (returned in millimetres), so the grid reads in fractions of an inch.
pub fn visible_grid_step_in(px_per_mm: f64, inch: bool) -> f64 {
    if !inch {
        return visible_grid_step(px_per_mm);
    }
    if !px_per_mm.is_finite() || px_per_mm <= 0.0 {
        return 25.4 / 16.0;
    }
    let min_in = GRID_MIN_PX / px_per_mm / 25.4;
    for f in [1.0 / 64.0, 1.0 / 32.0, 1.0 / 16.0, 1.0 / 8.0, 1.0 / 4.0, 1.0 / 2.0] {
        if f >= min_in * (1.0 - 1e-9) {
            return f * 25.4;
        }
    }
    decades(min_in.max(1.0)) * 25.4
}
