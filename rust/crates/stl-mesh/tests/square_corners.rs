//! Shell and Hollow make square corners on flat parts (spec 2.7).

use std::collections::HashMap;

use stl_kernel::Solid;
use stl_mesh::voxel::hollow;

fn cube20() -> Solid {
    Solid::cube([20.0, 20.0, 20.0], false)
}

/// 20 x 20 outer L, 40 long on both legs, 10 thick (z).
fn l_part() -> Solid {
    let a = Solid::cube([40.0, 20.0, 10.0], false);
    let b = Solid::cube([20.0, 40.0, 10.0], false);
    a.union(&b)
}

fn sphere() -> Solid {
    Solid::sphere(15.0, 96)
}

/// Shell like the engine's Shell: the cavity plus a prism that opens the top; returns the part.
fn open_top_shell(s: &Solid, t: f64, pitch: f64) -> Solid {
    let cavity = hollow(s, t, Some(pitch), true).unwrap();
    // the engine opens the face where the cavity lies under it: the cavity's footprint, upward
    let (cb, bb) = (cavity.bounding_box(), s.bounding_box());
    let top = Solid::cube([cb.max[0] - cb.min[0], cb.max[1] - cb.min[1], 2.0 * t], false).translate([
        cb.min[0],
        cb.min[1],
        bb.max[2] - t,
    ]);
    s.difference(&Solid::union_all(&[cavity, top]))
}

fn points(s: &Solid) -> Vec<[f64; 3]> {
    s.to_mesh()
        .positions
        .chunks_exact(3)
        .map(|p| [p[0] as f64, p[1] as f64, p[2] as f64])
        .collect()
}

fn has_point(pts: &[[f64; 3]], want: [f64; 3], tol: f64) -> bool {
    pts.iter().any(|p| (0..3).all(|a| (p[a] - want[a]).abs() <= tol))
}

/// Every directed edge appears once and its reverse once: a closed, consistently wound surface.
fn assert_watertight(s: &Solid) {
    let m = s.to_mesh();
    let mut edges: HashMap<(u32, u32), u32> = HashMap::new();
    for t in m.triangles.chunks_exact(3) {
        for k in 0..3 {
            *edges.entry((t[k], t[(k + 1) % 3])).or_insert(0) += 1;
        }
    }
    for (&(a, b), &n) in &edges {
        assert_eq!(n, 1, "edge {a}->{b} used {n} times");
        assert_eq!(edges.get(&(b, a)), Some(&1), "edge {a}->{b} has no single reverse");
    }
    assert!(s.volume() > 0.0);
}

/// Volume of the part's own result with the voxel cavity unsnapped (measured before spec 2.7).
const SPHERE_HOLLOW_BEFORE: f64 = 4978.781000;

#[test]
fn cube_shell_open_top() {
    let shell = open_top_shell(&cube20(), 2.0, 0.8);
    // the removed space is 16 x 16 x 18
    let cavity = 20.0 * 20.0 * 20.0 - shell.volume();
    assert!(
        (cavity - 16.0 * 16.0 * 18.0).abs() <= 0.001 * 4608.0,
        "cavity volume {cavity}"
    );
    let pts = points(&shell);
    for x in [2.0, 18.0] {
        for y in [2.0, 18.0] {
            for z in [2.0, 20.0] {
                assert!(has_point(&pts, [x, y, z], 0.01), "no corner at ({x}, {y}, {z})");
            }
        }
    }
    // the closed cavity too: exactly the 16 mm cube
    let c = hollow(&cube20(), 2.0, Some(0.8), true).unwrap();
    assert!((c.volume() - 4096.0).abs() < 4.096, "closed cavity {}", c.volume());
    let cp = points(&c);
    for x in [2.0, 18.0] {
        for y in [2.0, 18.0] {
            for z in [2.0, 18.0] {
                assert!(has_point(&cp, [x, y, z], 0.01), "no cavity corner at ({x}, {y}, {z})");
            }
        }
    }
}

#[test]
fn l_part_inner_corner_square() {
    let c = hollow(&l_part(), 2.0, Some(0.7), true).unwrap();
    // flat walls moved in by 2 mm meet in exact edges: the L's own reflex corner at (18, 18)
    // included (a CAD shell, not the rounded erosion)
    let want = (36.0 * 16.0 + 16.0 * 20.0) * 6.0;
    assert!(
        (c.volume() - want).abs() <= 0.001 * want,
        "volume {} vs {want}",
        c.volume()
    );
    let pts = points(&c);
    for z in [2.0, 8.0] {
        for xy in [
            [2.0, 2.0],
            [38.0, 2.0],
            [38.0, 18.0],
            [18.0, 18.0],
            [18.0, 38.0],
            [2.0, 38.0],
        ] {
            assert!(
                has_point(&pts, [xy[0], xy[1], z], 0.01),
                "no corner at ({}, {}, {z})",
                xy[0],
                xy[1]
            );
        }
    }
    // nothing sits in the rounded bit the voxel surface left at the reflex corner
    let on = |v: f64, set: &[f64]| set.iter().any(|s| (v - s).abs() < 0.01);
    for p in &pts {
        assert!(
            on(p[0], &[2.0, 18.0, 38.0]) || on(p[1], &[2.0, 18.0, 38.0]) || on(p[2], &[2.0, 8.0]),
            "{p:?} is on no wall plane"
        );
        assert!(
            !(p[0] > 18.01 && p[0] < 19.99 && p[1] > 18.01 && p[1] < 19.99),
            "{p:?} is in the corner"
        );
    }
}

#[test]
fn sphere_unchanged() {
    let h = hollow(&sphere(), 2.0, Some(0.8), false).unwrap();
    assert!(
        (h.volume() - SPHERE_HOLLOW_BEFORE).abs() <= 0.001 * SPHERE_HOLLOW_BEFORE,
        "sphere shell volume {}",
        h.volume()
    );
    // nothing is flat, so nothing is touched: the same bytes as before
    assert!(
        (h.volume() - SPHERE_HOLLOW_BEFORE).abs() < 1e-5,
        "sphere shell volume {}",
        h.volume()
    );
}

#[test]
fn still_watertight() {
    assert_watertight(&open_top_shell(&cube20(), 2.0, 0.8));
    assert_watertight(&hollow(&cube20(), 2.0, Some(0.8), false).unwrap());
    assert_watertight(&hollow(&l_part(), 2.0, Some(0.7), false).unwrap());
    assert_watertight(&hollow(&l_part(), 2.0, Some(0.7), true).unwrap());
    assert_watertight(&hollow(&sphere(), 2.0, Some(0.8), false).unwrap());
}

#[test]
fn deterministic() {
    for s in [cube20(), l_part(), sphere()] {
        let a = hollow(&s, 2.0, Some(0.8), false).unwrap().to_mesh();
        let b = hollow(&s, 2.0, Some(0.8), false).unwrap().to_mesh();
        assert_eq!(a.triangles, b.triangles);
        let bits = |m: &stl_kernel::MeshArrays| m.positions.iter().map(|x| x.to_bits()).collect::<Vec<u32>>();
        assert_eq!(bits(&a), bits(&b));
    }
}
