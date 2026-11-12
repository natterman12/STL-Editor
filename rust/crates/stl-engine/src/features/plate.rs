//! 1.x `f_plate` (Plate Layout): pack the chosen bodies onto the print bed. Rows of footprints
//! a gap apart, the block centred on the origin, every body resting on the bed. Footprints are
//! placed tallest first (ties: wider first, then the order you chose them), each row fills left
//! to right, and a new row starts above the tallest part in the row before.

use std::cmp::Ordering;

use serde_json::{Map, Value};

use super::common::pose::par_map;
use super::common::printers::bed_from_params;
use super::common::{affine, float, rotation, strings, transform_body, truthy};
use crate::state::Body;
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

/// Python's `f"{x:g}"`.
fn fmt_g(x: f64) -> String {
    if x == 0.0 {
        return if x.is_sign_negative() { "-0".into() } else { "0".into() };
    }
    let e = format!("{:.5e}", x);
    let (mant, exp) = e.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    if exp < -4 || exp >= 6 {
        let mant = if mant.contains('.') { mant.trim_end_matches('0').trim_end_matches('.') } else { mant };
        return format!("{}e{}{:02}", mant, if exp < 0 { '-' } else { '+' }, exp.abs());
    }
    let s = format!("{:.*}", (5 - exp).max(0) as usize, x);
    if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s }
}

/// 1.x `_mm`: `f"{round(x, 1):g}"`. Python rounds the exact binary value (ties to even),
/// which is what Rust's fixed-precision formatting does too.
fn mm(x: f64) -> String {
    let r: f64 = format!("{:.1}", x).parse().unwrap_or(x);
    fmt_g(r)
}

struct Item {
    i: usize,
    bid: String,
    name: String,
    lo: [f64; 3],
    hi: [f64; 3],
    rot: bool,
    dx: f64,
    dy: f64,
    x: f64,
    y: f64,
}

/// The vertices of a body, solid or raw.
fn body_points(b: &Body) -> Vec<[f64; 3]> {
    let solid_mesh;
    let mesh = match (&b.solid, &b.mesh) {
        (Some(s), _) => {
            solid_mesh = s.to_mesh();
            &solid_mesh
        }
        (None, Some(m)) => m.as_ref(),
        (None, None) => return Vec::new(),
    };
    mesh.positions.chunks_exact(3).map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]).collect()
}

/// The box round a body's vertices, `None` for a body with no geometry.
fn body_box(b: &Body) -> Option<([f64; 3], [f64; 3])> {
    let v = body_points(b);
    if v.is_empty() {
        return None;
    }
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for q in &v {
        for k in 0..3 {
            lo[k] = lo[k].min(q[k]);
            hi[k] = hi[k].max(q[k]);
        }
    }
    Some((lo, hi))
}

fn f_plate(
    state: &mut State,
    _fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let bids = strings(p.get("bodies"));
    if bids.is_empty() {
        return Err(FeatureError("Choose the bodies to lay out on the bed.".into()));
    }
    let gap = float(p, "gap", Some(5.0))?;
    if gap < 0.0 {
        return Err(FeatureError("The gap can't be negative.".into()));
    }
    let [w, h, hz] = bed_from_params(p)?;
    let rotate = truthy(p.get("rotate"));
    let mut items: Vec<Item> = Vec::new();
    let bodies = bids.iter().map(|bid| state.body(bid)).collect::<Result<Vec<_>, _>>()?;
    let boxes = par_map(bodies.len(), |i| body_box(bodies[i]));
    for (i, bid) in bids.iter().enumerate() {
        let b = bodies[i];
        let Some((lo, hi)) = boxes[i] else {
            return Err(FeatureError(format!("\"{}\" has no geometry.", b.name)));
        };
        let size = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
        if size[2] > hz + 1e-9 {
            return Err(FeatureError(format!(
                "\"{}\" is {} mm tall, taller than the bed ({} mm). Pick a taller printer, or cut it with Fit to Bed.",
                b.name,
                mm(size[2]),
                mm(hz)
            )));
        }
        let mut options = vec![(false, size[0], size[1])];
        if rotate {
            options.push((true, size[1], size[0]));
        }
        let fits: Vec<_> = options.into_iter().filter(|o| o.1 <= w + 1e-9 && o.2 <= h + 1e-9).collect();
        if fits.is_empty() {
            let turn = if rotate { "" } else { "turn on Rotate, " };
            return Err(FeatureError(format!(
                "\"{}\" is {} x {} mm, too big for the bed ({} x {} mm). Try: {}pick a bigger printer, or cut it with Fit to Bed.",
                b.name,
                mm(size[0]),
                mm(size[1]),
                mm(w),
                mm(h),
                turn
            )));
        }
        // the shortest footprint; ties keep the original turn
        let mut best = fits[0];
        for o in &fits[1..] {
            if o.2 < best.2 {
                best = *o;
            }
        }
        items.push(Item { i, bid: bid.clone(), name: b.name.clone(), lo, hi, rot: best.0, dx: best.1, dy: best.2, x: 0.0, y: 0.0 });
    }
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| {
        let (ia, ib) = (&items[a], &items[b]);
        (-ia.dy)
            .partial_cmp(&(-ib.dy))
            .unwrap_or(Ordering::Equal)
            .then((-ia.dx).partial_cmp(&(-ib.dx)).unwrap_or(Ordering::Equal))
            .then(ia.i.cmp(&ib.i))
    });
    let (mut x, mut y, mut shelf) = (0.0f64, 0.0f64, 0.0f64);
    for (n, &k) in order.iter().enumerate() {
        let it = &mut items[k];
        if n > 0 && x + it.dx > w + 1e-9 {
            // start a new row above the tallest part so far
            y += shelf + gap;
            x = 0.0;
            shelf = 0.0;
        }
        if y + it.dy > h + 1e-9 {
            return Err(FeatureError(format!(
                "Not everything fits on the bed: \"{}\" has no room left. Try a smaller gap, turn on Rotate, pick a bigger printer, or choose fewer bodies.",
                it.name
            )));
        }
        it.x = x;
        it.y = y;
        x += it.dx + gap;
        shelf = shelf.max(it.dy);
    }
    let width = items.iter().map(|it| it.x + it.dx).fold(f64::NEG_INFINITY, f64::max);
    let height = items.iter().map(|it| it.y + it.dy).fold(f64::NEG_INFINITY, f64::max);
    let (ox, oy) = (-width / 2.0, -height / 2.0);
    let moved = {
        let jobs: Vec<(&Item, &Body)> =
            order.iter().map(|&k| Ok((&items[k], state.body(&items[k].bid)?))).collect::<Result<_, FeatureError>>()?;
        par_map(jobs.len(), |n| {
            let (it, b) = jobs[n];
            let r = if it.rot { rotation(0.0, 0.0, 90.0) } else { [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] };
            let (mx, my) = if it.rot { (-it.hi[1], it.lo[0]) } else { (it.lo[0], it.lo[1]) };
            let t = [ox + it.x - mx, oy + it.y - my, -it.lo[2]];
            transform_body(b, affine(r, t))
        })
    };
    for (&k, nb) in order.iter().zip(moved) {
        state.bodies.insert(items[k].bid.clone(), nb);
        res.modified.push(items[k].bid.clone());
    }
    let mut info = Map::new();
    info.insert("placed".into(), Value::from(items.len()));
    info.insert("rotated".into(), Value::from(items.iter().filter(|it| it.rot).count()));
    res.extra.insert("info".into(), Value::Object(info));
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("plate", f_plate)
}
