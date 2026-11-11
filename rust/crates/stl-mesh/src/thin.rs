//! The thin-wall scan (1.x `feature_modules/thin_wall.py` `thin_scan`): slices the solid along
//! each axis, finds the parts of a slice narrower than the limit and groups them into spots.

use std::collections::HashMap;

use stl_kernel::{Shape2d, Solid};

use crate::refs::round_py;

/// What the scan found: the thinnest width, how many spots, and where (up to 10, biggest first).
#[derive(Clone, Debug, PartialEq)]
pub struct ThinScan {
    pub min: Option<f64>,
    pub count: usize,
    pub spots: Vec<[f64; 3]>,
}

/// The parts of a slice narrower than `w` (a wall, pin or rib thinner than that).
fn thin_residual(cs: &Shape2d, w: f64) -> Shape2d {
    let r = w / 2.0 - 0.005 * w;
    let inner = cs.offset_miter(-r, 2.0);
    if inner.is_empty() {
        return cs.clone();
    }
    let opened = inner.offset_miter(r, 2.0);
    // cs minus opened. A mitred spike of `opened` can stick out past cs, so take only the part of
    // it inside cs: even-odd of cs and (cs AND opened) is then exactly cs minus opened.
    let mut all = cs.to_polygons();
    all.extend(cs.intersection(&opened).to_polygons());
    Shape2d::from_even_odd(&all)
}

fn thin_parts(cs: &Shape2d, w: f64, min_area: f64) -> Vec<Shape2d> {
    let res = thin_residual(cs, w);
    if res.is_empty() {
        return Vec::new();
    }
    res.decompose().into_iter().filter(|c| c.area() >= min_area).collect()
}

/// Does any slice still have a thin part at width `w`?
fn any_thin(flagged: &[Shape2d], w: f64, min_area: f64) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use rayon::prelude::*;
        flagged.par_iter().any(|cs| !thin_parts(cs, w, min_area).is_empty())
    }
    #[cfg(target_arch = "wasm32")]
    {
        flagged.iter().any(|cs| !thin_parts(cs, w, min_area).is_empty())
    }
}

fn dist(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Thin spots of a solid for a minimum thickness of `t` mm.
pub fn thin_scan(s: &Solid, t: f64) -> ThinScan {
    let min_area = 0.05 * t * t;
    let bb = s.bounding_box();
    let (lo, hi) = (bb.min, bb.max);
    let mut found: Vec<[f64; 3]> = Vec::new();
    let mut widths: Vec<f64> = Vec::new();
    let mut link = 1.5 * t * 2.0;
    for a in 0..3 {
        let (u, v) = ((a + 1) % 3, (a + 2) % 3);
        let mut m = [[0.0; 4]; 3];
        m[0][u] = 1.0;
        m[1][v] = 1.0;
        m[2][a] = 1.0;
        let rot = s.transform(m);
        let ext = hi[a] - lo[a];
        let step = (t / 2.0).max(ext / 60.0);
        let n = ((ext / step - 1e-9).ceil() as i64).max(1);
        link = link.max(1.5 * ext / n as f64);
        // each slice is independent; results come back in slice order
        let one = |i: i64| -> Option<(Shape2d, Vec<[f64; 3]>)> {
            let h = lo[a] + (i as f64 + 0.5) * ext / n as f64;
            let cs = rot.slice(h);
            if cs.is_empty() {
                return None;
            }
            let parts = thin_parts(&cs, t, min_area);
            if parts.is_empty() {
                return None;
            }
            let pts = parts
                .into_iter()
                .map(|c| {
                    let (bmin, bmax) = c.bounds().expect("a part with area has bounds");
                    let mut p = [0.0; 3];
                    p[a] = h;
                    p[u] = (bmin[0] + bmax[0]) / 2.0;
                    p[v] = (bmin[1] + bmax[1]) / 2.0;
                    p
                })
                .collect();
            Some((cs, pts))
        };
        #[cfg(not(target_arch = "wasm32"))]
        let results: Vec<Option<(Shape2d, Vec<[f64; 3]>)>> = {
            use rayon::prelude::*;
            (0..n).into_par_iter().map(one).collect()
        };
        #[cfg(target_arch = "wasm32")]
        let results: Vec<Option<(Shape2d, Vec<[f64; 3]>)>> = (0..n).map(one).collect();
        let mut flagged: Vec<Shape2d> = Vec::new();
        for (cs, pts) in results.into_iter().flatten() {
            flagged.push(cs);
            found.extend(pts);
        }
        if !flagged.is_empty() {
            let (mut a_lo, mut a_hi) = (0.0, t);
            for _ in 0..8 {
                let mid = (a_lo + a_hi) / 2.0;
                if any_thin(&flagged, mid, min_area) {
                    a_hi = mid;
                } else {
                    a_lo = mid;
                }
            }
            widths.push(a_hi * 0.99);
        }
    }
    if found.is_empty() {
        return ThinScan { min: None, count: 0, spots: Vec::new() };
    }
    // group pieces that sit close together into one spot
    let mut cell: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
    for (i, p) in found.iter().enumerate() {
        let key = ((p[0] / link).floor() as i64, (p[1] / link).floor() as i64, (p[2] / link).floor() as i64);
        cell.entry(key).or_default().push(i);
    }
    let mut parent: Vec<usize> = (0..found.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for (key, ids) in &cell {
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let Some(other) = cell.get(&(key.0 + dx, key.1 + dy, key.2 + dz)) else { continue };
                    for &i in ids {
                        for &j in other {
                            if i < j && dist(&found[i], &found[j]) <= link {
                                let (ri, rj) = (root(&mut parent, i), root(&mut parent, j));
                                parent[rj] = ri;
                            }
                        }
                    }
                }
            }
        }
    }
    let mut groups: HashMap<usize, Vec<[f64; 3]>> = HashMap::new();
    for i in 0..found.len() {
        let r = root(&mut parent, i);
        groups.entry(r).or_default().push(found[i]);
    }
    let mut spots: Vec<(usize, [f64; 3])> = groups
        .values()
        .map(|pts| {
            let mut c = [0.0; 3];
            for k in 0..3 {
                let lo = pts.iter().map(|p| p[k]).fold(f64::INFINITY, f64::min);
                let hi = pts.iter().map(|p| p[k]).fold(f64::NEG_INFINITY, f64::max);
                c[k] = round_py((lo + hi) / 2.0, 2);
            }
            (pts.len(), c)
        })
        .collect();
    spots.sort_by(|x, y| {
        y.0.cmp(&x.0).then_with(|| {
            for k in 0..3 {
                let o = x.1[k].total_cmp(&y.1[k]);
                if o.is_ne() {
                    return o;
                }
            }
            std::cmp::Ordering::Equal
        })
    });
    let widest = widths.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    ThinScan {
        min: Some(round_py(widest, 3)),
        count: spots.len(),
        spots: spots.into_iter().take(10).map(|s| s.1).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A five-sided shape whose opening (miter limit 2) sticks out past it by about 2 mm2.
    /// 1.x (`cs - opened`, manifold3d) leaves 1.8e-7 mm2 at w = 4 and no part over 0.8 mm2.
    #[test]
    fn opening_spike_outside_the_slice_is_not_thin() {
        let p = [
            [4.66890063169178, 0.9440467977500165],
            [-1.6536936208474007, 7.576573165607959],
            [-5.391226664296701, -0.2825235356864743],
            [-4.192326548828804, -2.115112850269774],
            [2.783980076323551, -1.712921979594813],
        ];
        let cs = Shape2d::from_even_odd(&[p.to_vec()]);
        let res = thin_residual(&cs, 4.0);
        assert!(res.area() < 1e-4, "residual area {}", res.area());
        assert!(thin_parts(&cs, 4.0, 0.05 * 16.0).is_empty());
    }
}
