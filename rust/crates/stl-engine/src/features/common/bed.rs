//! 1.x `printing.plan_cuts` and `split_to_bed`: cut a part into pieces that fit the print bed.

use std::collections::BTreeMap;

use serde_json::Value;
use stl_kernel::Solid;

use super::connectors::split_with_connectors;
use super::frames::{plane_frame, Vec3};
use crate::FeatureError;

/// Area of the part's cross-section at `pos` along `axis`.
fn axis_section_area(solid: &Solid, axis: usize, pos: f64) -> Result<f64, FeatureError> {
    let mut n = [0.0; 3];
    n[axis] = 1.0;
    let mut o = [0.0; 3];
    o[axis] = pos;
    let f = plane_frame(o, n, None)?;
    let d = |a: Vec3| -(a[0] * f.o[0] + a[1] * f.o[1] + a[2] * f.o[2]);
    let m = [
        [f.x[0], f.x[1], f.x[2], d(f.x)],
        [f.y[0], f.y[1], f.y[2], d(f.y)],
        [f.n[0], f.n[1], f.n[2], d(f.n)],
    ];
    Ok(solid.transform(m).slice(0.0).area())
}

/// numpy `linspace(a, b, num)`.
fn linspace(a: f64, b: f64, num: usize) -> Vec<f64> {
    let step = (b - a) / (num - 1) as f64;
    let mut v: Vec<f64> = (0..num).map(|i| i as f64 * step + a).collect();
    v[num - 1] = b;
    v
}

/// Cut positions per axis so that every piece fits `usable` (x, y, z), placing each cut where
/// the cross-section is smallest within the allowed range.
pub fn plan_cuts(solid: &Solid, usable: Vec3, optimize: bool) -> Result<BTreeMap<usize, Vec<f64>>, FeatureError> {
    let b = solid.bounding_box();
    let (lo, hi) = (b.min, b.max);
    let mut cuts = BTreeMap::new();
    for ax in 0..3 {
        let s = hi[ax] - lo[ax];
        let u = usable[ax];
        let npieces = (s / u - 1e-9).ceil() as i64;
        if npieces <= 1 {
            continue;
        }
        let mut pos = Vec::new();
        let mut prev = lo[ax];
        for k in 1..npieces {
            let remaining = npieces - k;
            let even = lo[ax] + s * k as f64 / npieces as f64;
            let mut lo_ok = (prev + 1.0).max(hi[ax] - remaining as f64 * u);
            let mut hi_ok = (prev + u).min(hi[ax] - 1.0);
            if lo_ok > hi_ok {
                let v = even.max(lo_ok).min(hi_ok);
                lo_ok = v;
                hi_ok = v;
            }
            let p = if optimize && hi_ok - lo_ok > 1.0 {
                let cand = linspace(lo_ok, hi_ok, 13);
                // prefer small cross-sections, but stay near the even split
                let mut areas = Vec::with_capacity(cand.len());
                for c in &cand {
                    areas.push(axis_section_area(solid, ax, *c)?);
                }
                let amax = areas.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                let mut best = 0;
                let mut best_score = f64::INFINITY;
                for (i, c) in cand.iter().enumerate() {
                    let score = areas[i] / (amax + 1e-9) + 0.15 * (c - even).abs() / (hi_ok - lo_ok + 1e-9);
                    if score < best_score {
                        best_score = score;
                        best = i;
                    }
                }
                cand[best]
            } else {
                even.max(lo_ok).min(hi_ok)
            };
            pos.push(p);
            prev = p;
        }
        cuts.insert(ax, pos);
    }
    Ok(cuts)
}

/// What a split to the bed gave.
pub struct BedSplit {
    pub pieces: Vec<Solid>,
    pub extras: Vec<Solid>,
    pub connectors: usize,
    pub faces_without_connectors: usize,
    /// The planned cut positions per axis (0 = x), and whether the bed's sides were swapped.
    pub cuts: BTreeMap<usize, Vec<f64>>,
    pub swapped: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn split_to_bed(
    solid: &Solid,
    bed: Vec3,
    margin: f64,
    mode: &str,
    pin_d: Option<&Value>,
    pin_len: Option<&Value>,
    clearance: Option<&Value>,
    optimize: bool,
) -> Result<BedSplit, FeatureError> {
    let mut usable = [bed[0] - 2.0 * margin, bed[1] - 2.0 * margin, bed[2] - 2.0 * margin];
    if usable.iter().any(|u| *u <= 5.0) {
        return Err(FeatureError("The margin is too large for this bed.".into()));
    }
    let b = solid.bounding_box();
    let size = [b.max[0] - b.min[0], b.max[1] - b.min[1]];
    // use the bed's longest horizontal side along the part's longest horizontal side
    let swap = (size[0] < size[1]) != (usable[0] < usable[1]);
    if swap {
        usable = [usable[1], usable[0], usable[2]];
    }
    let cuts = plan_cuts(solid, usable, optimize)?;
    let mut pieces = vec![solid.clone()];
    let mut out = BedSplit { pieces: Vec::new(), extras: Vec::new(), connectors: 0, faces_without_connectors: 0, cuts: cuts.clone(), swapped: swap };
    for (ax, positions) in cuts {
        let mut nvec = [0.0; 3];
        nvec[ax] = 1.0;
        for pos in positions {
            let mut new = Vec::new();
            for pc in pieces {
                let pb = pc.bounding_box();
                if !(pb.min[ax] < pos && pos < pb.max[ax]) {
                    new.push(pc);
                    continue;
                }
                let mut o = [0.0; 3];
                o[ax] = pos;
                let r = split_with_connectors(&pc, o, nvec, mode, pin_d, pin_len, clearance)?;
                out.connectors += r.connectors;
                out.faces_without_connectors += usize::from(r.skipped);
                out.extras.extend(r.extras);
                for m in [r.below, r.above] {
                    if !m.is_empty() {
                        new.push(m);
                    }
                }
            }
            pieces = new;
        }
    }
    out.pieces = pieces;
    Ok(out)
}
