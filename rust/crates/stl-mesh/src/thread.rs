//! ISO metric 60° threads (`printing.py`: `_thread_section`, `_wall`, `_helical_thread`,
//! `threaded_rod`, `thread_tool`).

use std::f64::consts::PI;

use stl_kernel::{MeshArrays, Shape2d, Solid};

use crate::MeshError;

/// Coarse ISO metric diameter -> pitch table (`printing.METRIC_COARSE`).
pub const METRIC_COARSE: [(f64, f64); 14] = [
    (2.0, 0.4),
    (2.5, 0.45),
    (3.0, 0.5),
    (4.0, 0.7),
    (5.0, 0.8),
    (6.0, 1.0),
    (8.0, 1.25),
    (10.0, 1.5),
    (12.0, 1.75),
    (14.0, 2.0),
    (16.0, 2.0),
    (20.0, 2.5),
    (24.0, 3.0),
    (30.0, 3.5),
];

/// 1.x `_thread_section`: planar cross-section of a thread profile and its radial depth.
pub fn thread_section(d: f64, p: f64, n: usize) -> (Vec<[f64; 2]>, f64) {
    let h = 0.8660254 * p;
    let r_maj = d / 2.0;
    let depth = 5.0 * h / 8.0;
    let a = p / 8.0;
    let b = p / 8.0 + 5.0 * p / 16.0;
    let c = b + p / 8.0;
    let d_end = b + p / 8.0 + 5.0 * p / 16.0;
    let f = |u: f64| -> f64 {
        let u = u.rem_euclid(p);
        if u < a || u >= d_end {
            r_maj - depth
        } else if u < b {
            r_maj - depth + depth * (u - a) / (b - a)
        } else if u < c {
            r_maj
        } else {
            r_maj - depth * (u - c) / (d_end - c)
        }
    };
    let step = if n > 0 { 2.0 * PI / n as f64 } else { 0.0 };
    let pts = (0..n)
        .map(|i| {
            let t = i as f64 * step;
            let r = f(t / (2.0 * PI) * p);
            [r * t.cos(), r * t.sin()]
        })
        .collect();
    (pts, depth)
}

/// 1.x `_wall`: triangles closing the flat cut between a column of profile corners and the axis
/// (both lists of `(index, z)`, `z` increasing, same first and last `z`).
fn wall(outer: &[(u32, f64)], axis: &[(u32, f64)], flip: bool) -> Vec<[u32; 3]> {
    let mut tris = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i + 1 < outer.len() || j + 1 < axis.len() {
        let t = if j + 1 == axis.len() || (i + 1 < outer.len() && outer[i + 1].1 <= axis[j + 1].1) {
            let tri = [outer[i].0, outer[i + 1].0, axis[j].0];
            i += 1;
            tri
        } else {
            let tri = [outer[i].0, axis[j + 1].0, axis[j].0];
            j += 1;
            tri
        };
        tris.push(if flip { [t[2], t[1], t[0]] } else { t });
    }
    tris
}

/// 1.x `_helical_thread`: right-hand ISO thread as a closed mesh: the four profile corners
/// (root end, crest start, crest end, root start) of every turn are helices sampled at `n`
/// angles, stitched into one band, closed at both ends by a helicoid and a flat cut. Reaches
/// past `z = 0` and `z = l` by about a pitch; the caller clips it.
fn helical_thread(d: f64, p: f64, l: f64, n: usize) -> Result<Solid, MeshError> {
    let err = || MeshError::Message("Could not build this thread. Try a different size or pitch.".into());
    let turns = (l / p - 1e-9).ceil();
    if !turns.is_finite() || turns < 0.0 || turns > 1_000_000.0 {
        return Err(err());
    }
    let h = 0.8660254 * p;
    let r_maj = d / 2.0;
    let rm = r_maj - 5.0 * h / 8.0;
    let u = [14.0 * p / 16.0, 9.0 * p / 16.0, 7.0 * p / 16.0, 2.0 * p / 16.0];
    let rr = [rm, r_maj, r_maj, rm];
    let m = turns as usize + 3;
    let j_len = 4 * m;
    let total_verts = j_len
        .checked_mul(n)
        .and_then(|v| v.checked_add(2 * (n + 1)))
        .ok_or_else(err)?;
    if total_verts > u32::MAX as usize {
        return Err(err());
    }
    let zb = -p;
    let phi: Vec<(f64, f64)> = (0..n)
        .map(|k| {
            let ang = 2.0 * PI * k as f64 / n as f64;
            (ang.cos(), ang.sin())
        })
        .collect();
    let k_step: Vec<f64> = (0..n).map(|k| k as f64 * p / n as f64).collect();

    let mut v = Vec::with_capacity(j_len * n);
    for jj in 0..j_len {
        let r = rr[jj % 4];
        let z0 = zb + (jj / 4) as f64 * p - u[jj % 4];
        for kk in 0..n {
            let (cp, sp) = phi[kk];
            v.push([r * cp, r * sp, z0 + k_step[kk]]);
        }
    }

    let idx = |jj: usize, kk: usize| -> u32 { (jj * n + kk) as u32 };

    // band: [A, B, C], then [A, C, Dd], matching np.vstack([column_stack([A,B,C]), column_stack([A,C,Dd])])
    let band_quads = (j_len - 1) * (n - 1) + (j_len - 5);
    let mut triangles: Vec<u32> = Vec::with_capacity((2 * band_quads + 4 * n + 2 * (n + 4)) * 3);

    for jj in 0..(j_len - 1) {
        for kk in 0..(n - 1) {
            triangles.extend_from_slice(&[idx(jj, kk), idx(jj, kk + 1), idx(jj + 1, kk + 1)]);
        }
    }
    for js in 0..(j_len - 5) {
        triangles.extend_from_slice(&[idx(js, n - 1), idx(js + 4, 0), idx(js + 5, 0)]);
    }
    for jj in 0..(j_len - 1) {
        for kk in 0..(n - 1) {
            triangles.extend_from_slice(&[idx(jj, kk), idx(jj + 1, kk + 1), idx(jj + 1, kk)]);
        }
    }
    for js in 0..(j_len - 5) {
        triangles.extend_from_slice(&[idx(js, n - 1), idx(js + 5, 0), idx(js + 1, n - 1)]);
    }

    // bottom: helicoid under corner row 0, flat cut at phi = 0
    let base = (j_len * n) as u32;
    let ab: Vec<u32> = (0..=n).map(|i| base + i as u32).collect();
    let zab: Vec<f64> = (0..=n).map(|i| zb - u[0] + i as f64 * p / n as f64).collect();
    let mut o_bot: Vec<u32> = (0..n).map(|kk| idx(0, kk)).collect();
    o_bot.push(idx(4, 0));
    for i in 0..n {
        triangles.extend_from_slice(&[ab[i], o_bot[i + 1], o_bot[i]]);
    }
    for i in 0..n {
        triangles.extend_from_slice(&[ab[i], ab[i + 1], o_bot[i + 1]]);
    }
    let col_bot: Vec<(u32, f64)> = (0..5)
        .map(|q| {
            let id = idx(q, 0);
            (id, v[id as usize][2])
        })
        .collect();
    let axis_bot: Vec<(u32, f64)> = ab.iter().copied().zip(zab.iter().copied()).collect();
    for t in wall(&col_bot, &axis_bot, false) {
        triangles.extend_from_slice(&t);
    }

    // top: helicoid over corner row J - 1, flat cut at phi of column N - 1
    let at_base = base + (n + 1) as u32;
    let at: Vec<u32> = (0..=n).map(|i| at_base + i as u32).collect();
    let zat: Vec<f64> = (0..=n).map(|i| zb + (m - 1) as f64 * p - u[3] + (i as f64 - 1.0) * p / n as f64).collect();
    let mut o_top: Vec<u32> = Vec::with_capacity(n + 1);
    o_top.push(idx(j_len - 5, n - 1));
    o_top.extend((0..n).map(|kk| idx(j_len - 1, kk)));
    for i in 0..n {
        triangles.extend_from_slice(&[at[i], o_top[i], o_top[i + 1]]);
    }
    for i in 0..n {
        triangles.extend_from_slice(&[at[i], o_top[i + 1], at[i + 1]]);
    }
    let col_top: Vec<(u32, f64)> = ((j_len - 5)..j_len)
        .map(|q| {
            let id = idx(q, n - 1);
            (id, v[id as usize][2])
        })
        .collect();
    let axis_top: Vec<(u32, f64)> = at.iter().copied().zip(zat.iter().copied()).collect();
    for t in wall(&col_top, &axis_top, true) {
        triangles.extend_from_slice(&t);
    }

    let mut positions: Vec<f32> = Vec::with_capacity(total_verts * 3);
    for pt in &v {
        positions.extend_from_slice(&[pt[0] as f32, pt[1] as f32, pt[2] as f32]);
    }
    for &z in &zab {
        positions.extend_from_slice(&[0.0, 0.0, z as f32]);
    }
    for &z in &zat {
        positions.extend_from_slice(&[0.0, 0.0, z as f32]);
    }

    Solid::from_mesh(&MeshArrays { positions, triangles }).map_err(|_| err())
}

/// 1.x `printing.threaded_rod`: threaded rod along +Z from `0` to `l` (major diameter `d`, pitch `p`).
pub fn threaded_rod(d: f64, p: f64, l: f64, hand: &str, chamfer: bool) -> Result<Solid, MeshError> {
    if !(d > 0.0 && p > 0.0 && l > 0.0) {
        return Err(MeshError::Message("Diameter, pitch and length must be positive.".into()));
    }
    if p > d / 2.0 {
        return Err(MeshError::Message("The pitch is too coarse for this diameter.".into()));
    }
    let n = ((d * 10.0).clamp(90.0, 240.0) + 0.5).floor() as u32;
    let depth = 5.0 * 0.8660254 * p / 8.0;
    let mut rod = helical_thread(d, p, l, n as usize)?;
    if hand != "right" {
        rod = rod.mirror([0.0, 1.0, 0.0]);
    }
    let r = d / 2.0;
    let c = if chamfer { (depth * 1.1).min(l / 3.0) } else { 0.0 };
    let prof = if chamfer {
        vec![
            [0.0, 0.0],
            [r - c, 0.0],
            [r + 0.01, c],
            [r + 0.01, l - c],
            [r - c, l],
            [0.0, l],
        ]
    } else {
        vec![[0.0, 0.0], [r + 0.01, 0.0], [r + 0.01, l], [0.0, l]]
    };
    let clip = Shape2d::from_even_odd(&[prof])
        .revolve(n, 360.0)
        .rotate_deg([0.0, 0.0, 180.0 / n as f64]);
    Ok(rod.intersection(&clip))
}

/// 1.x `printing.thread_tool`: solid in local coordinates (axis +Z from the surface at `z = 0`).
/// `external`: the screw itself, standing on `z = 0`.
/// `internal`: what to cut for a tapped hole, going down into the material (`z` from `-l` to just above `0`).
pub fn thread_tool(
    kind: &str,
    d: f64,
    p: f64,
    l: f64,
    tolerance: f64,
    hand: &str,
    chamfer: bool,
) -> Result<Solid, MeshError> {
    let tol = tolerance;
    if kind == "external" {
        return threaded_rod(d - tol, p, l, hand, chamfer);
    }
    let over = 0.6;
    let mut rod = threaded_rod(d + tol, p, l + over, hand, false)?.translate([0.0, 0.0, -l]);
    if chamfer {
        let (_, depth) = thread_section(d + tol, p, 8);
        let r = (d + tol) / 2.0;
        let cs = Solid::cylinder(depth + over, r - depth, r + over, 96, false).translate([0.0, 0.0, -depth]);
        rod = rod.union(&cs);
    }
    Ok(rod)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_dimensions() {
        assert_eq!(
            thread_tool("external", 0.0, 1.25, 12.0, 0.2, "right", true).err(),
            Some(MeshError::Message("Diameter, pitch and length must be positive.".into()))
        );
        assert_eq!(
            thread_tool("external", 8.0, 0.0, 12.0, 0.0, "right", true).err(),
            Some(MeshError::Message("Diameter, pitch and length must be positive.".into()))
        );
        assert_eq!(
            thread_tool("external", 8.0, 1.25, -1.0, 0.0, "right", true).err(),
            Some(MeshError::Message("Diameter, pitch and length must be positive.".into()))
        );
        assert_eq!(
            thread_tool("external", 8.0, 5.0, 12.0, 0.0, "right", true).err(),
            Some(MeshError::Message("The pitch is too coarse for this diameter.".into()))
        );
    }

    #[test]
    fn external_m8_matches_1x() {
        let s = thread_tool("external", 8.0, 1.25, 12.0, 0.2, "right", true).unwrap();
        assert!(!s.is_empty());
        assert!((s.volume() - 464.7201848909987).abs() / 464.7201848909987 < 1e-3, "vol={}", s.volume());
        assert!(
            (s.surface_area() - 489.6903732708501).abs() / 489.6903732708501 < 1e-3,
            "area={}",
            s.surface_area()
        );
        let bb = s.bounding_box();
        assert!(bb.min[2].abs() < 1e-5 && (bb.max[2] - 12.0).abs() < 1e-5, "{bb:?}");
    }

    #[test]
    fn internal_and_left_hand_threads() {
        let lh = thread_tool("external", 8.0, 1.25, 12.0, 0.0, "left", false).unwrap();
        let rh = thread_tool("external", 8.0, 1.25, 12.0, 0.0, "right", false).unwrap();
        assert!((lh.volume() - rh.volume()).abs() < 1e-4);

        let tap = thread_tool("internal", 8.0, 1.25, 6.0, 0.2, "right", true).unwrap();
        let bb = tap.bounding_box();
        assert!((bb.min[2] - (-6.0)).abs() < 1e-4 && (bb.max[2] - 0.6).abs() < 1e-4, "{bb:?}");
    }
}
