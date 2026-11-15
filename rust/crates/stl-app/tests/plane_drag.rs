//! The drag-the-plane control's maths (spec 2.2): where a fresh pick starts, how a drag follows
//! the pointer, the face-on fallback, and which half a cut keeps.

use stl_app::viewport::camera::{self, Camera, V3};
use stl_app::viewport::plane_drag::{self, DragPlane};
use stl_kernel::Aabb;

/// The 20 mm cube standing on the bed at the origin.
fn cube() -> Aabb {
    Aabb { min: [-10.0, -10.0, 0.0], max: [10.0, 10.0, 20.0] }
}

fn cam(position: V3, target: V3, up: V3) -> Camera {
    let mut c = Camera::new();
    c.set_size(800.0, 600.0);
    c.position = position;
    c.target = target;
    c.up = up;
    c
}

#[test]
fn start_distance_boundary_is_halfway() {
    // the top face (z = 20, normal +Z) only touches the cube: start halfway down
    let d = plane_drag::start_distance([0.0, 0.0, 20.0], [0.0, 0.0, 1.0], &cube());
    assert!((d + 10.0).abs() < 1e-9, "top face starts at {d}");
    // the bottom face, normal -Z: halfway up
    let d = plane_drag::start_distance([0.0, 0.0, 0.0], [0.0, 0.0, -1.0], &cube());
    assert!((d + 10.0).abs() < 1e-9, "bottom face starts at {d}");
    // a side face at an angle to its own normal's length (not unit) still lands in the middle
    let d = plane_drag::start_distance([10.0, 0.0, 5.0], [2.0, 0.0, 0.0], &cube());
    assert!((d + 10.0).abs() < 1e-9, "side face starts at {d}");
    // strictly inside keeps 0
    assert_eq!(plane_drag::start_distance([0.0, 0.0, 5.0], [0.0, 0.0, 1.0], &cube()), 0.0);
    // a plane outside the box also moves to its middle
    let d = plane_drag::start_distance([0.0, 0.0, 50.0], [0.0, 0.0, 1.0], &cube());
    assert!((d + 40.0).abs() < 1e-9, "outside plane starts at {d}");
}

#[test]
fn drag_follows_pointer_projection() {
    // a side view (from the front) of a plane whose normal is +X: the arrow points right
    let c = cam([0.0, -200.0, 10.0], [0.0, 0.0, 10.0], [0.0, 0.0, 1.0]);
    let p = DragPlane { origin: [0.0, 0.0, 10.0], normal: [1.0, 0.0, 0.0], distance: 0.0, span: cube() };
    assert!(!plane_drag::face_on(&c, &p));
    let mid = c.world_to_view(plane_drag::center(&p));
    let tip = plane_drag::handle_at(&c, &p);
    assert!(tip[0] > mid[0] + 50.0 && (tip[1] - mid[1]).abs() < 1e-6, "arrow points right: {mid:?} -> {tip:?}");
    assert!(plane_drag::hits_handle(&c, &p, [(mid[0] + tip[0]) / 2.0, mid[1] + 3.0]));
    assert!(plane_drag::hits_handle(&c, &p, tip));
    assert!(!plane_drag::hits_handle(&c, &p, [mid[0], mid[1] + 60.0]));

    let px = c.pixel_size(plane_drag::center(&p));
    let len = ((tip[0] - mid[0]).powi(2) + (tip[1] - mid[1]).powi(2)).sqrt();
    let dir = [(tip[0] - mid[0]) / len, (tip[1] - mid[1]) / len];
    let from = [mid[0] + dir[0] * 30.0, mid[1] + dir[1] * 30.0];
    let to = [from[0] + dir[0] * 100.0, from[1] + dir[1] * 100.0];
    let d = plane_drag::drag_distance(&c, &p, from, to, 0.0);
    let want = 100.0 * px;
    assert!((d - want).abs() < want * 0.01, "100 px moved the plane {d} mm, want {want}");
    // back the other way, and from a non-zero start
    let back = plane_drag::drag_distance(&c, &p, to, from, 3.0);
    assert!((back - (3.0 - want)).abs() < want * 0.01, "{back}");

    // an angled view: the plane still tracks the pointer's projection (the point on the normal
    // line under the pointer is where the plane ends up)
    let c2 = cam([150.0, -180.0, 120.0], [0.0, 0.0, 10.0], [0.0, 0.0, 1.0]);
    let goal = camera::add(plane_drag::center(&p), [7.5, 0.0, 0.0]);
    let start = c2.world_to_view(plane_drag::center(&p));
    let d = plane_drag::drag_distance(&c2, &p, start, c2.world_to_view(goal), 0.0);
    assert!((d - 7.5).abs() < 1e-6, "angled view: {d}");
}

#[test]
fn drag_face_on_falls_back() {
    // a Z cut seen from the top: the normal points straight at the camera
    let c = cam([0.0, 0.0, 200.0], [0.0, 0.0, 10.0], [0.0, 1.0, 0.0]);
    let p = DragPlane { origin: [0.0, 0.0, 10.0], normal: [0.0, 0.0, 1.0], distance: 0.0, span: cube() };
    assert!(plane_drag::face_on(&c, &p));
    let mid = c.world_to_view(plane_drag::center(&p));
    assert!(plane_drag::hits_handle(&c, &p, mid), "face on, the arrow's dot can still be grabbed");
    let px = c.pixel_size(plane_drag::center(&p));
    let mut last = f64::NEG_INFINITY;
    for dy in [-40.0, -20.0, -5.0, 0.0, 5.0, 20.0, 40.0] {
        // up is a smaller y: moving up should move the plane toward the camera (+Z)
        let d = plane_drag::drag_distance(&c, &p, mid, [mid[0] + 3.0, mid[1] - dy], 0.0);
        assert!(d.is_finite(), "dy {dy}: {d}");
        assert!(d > last, "not monotonic at dy {dy}: {d} after {last}");
        assert!((d - dy * px).abs() < 1e-9, "dy {dy}: {d}, want {}", dy * px);
        last = d;
    }
    // up moves toward the camera from below too (the normal pointing away from it)
    let below = DragPlane { normal: [0.0, 0.0, -1.0], ..p };
    let d = plane_drag::drag_distance(&c, &below, mid, [mid[0], mid[1] - 10.0], 0.0);
    assert!(d < 0.0, "normal away from the camera: up gives {d}");

    // tilted 10° off the line of sight: still face on, still smooth and finite
    let a = 10f64.to_radians();
    let t = DragPlane { normal: [0.0, a.sin(), a.cos()], ..p };
    assert!(plane_drag::face_on(&c, &t));
    let mut last = f64::NEG_INFINITY;
    for i in -10..=10 {
        let d = plane_drag::drag_distance(&c, &t, mid, [mid[0], mid[1] - i as f64 * 4.0], 1.0);
        assert!(d.is_finite() && d > last, "tilted step {i}: {d}");
        last = d;
    }
    // just past 15° the projection takes over and stays finite
    let a = 16f64.to_radians();
    let t = DragPlane { normal: [0.0, a.sin(), a.cos()], ..p };
    assert!(!plane_drag::face_on(&c, &t));
    for i in -10..=10 {
        let d = plane_drag::drag_distance(&c, &t, mid, [mid[0] + i as f64 * 7.0, mid[1] - i as f64 * 4.0], 0.0);
        assert!(d.is_finite(), "16° step {i}: {d}");
    }
}

#[test]
fn plane_eq_flip() {
    let p = DragPlane { origin: [0.0, 0.0, 5.0], normal: [0.0, 0.0, 2.0], distance: 2.0, span: cube() };
    let keep = |e: [f64; 4], x: V3| e[0] * x[0] + e[1] * x[1] + e[2] * x[2] + e[3] >= 0.0;
    let behind = plane_drag::plane_eq(&p, false);
    assert_eq!(behind, [-0.0, -0.0, -1.0, 7.0]);
    assert!(keep(behind, [0.0, 0.0, 3.0]) && !keep(behind, [0.0, 0.0, 9.0]));
    let front = plane_drag::plane_eq(&p, true);
    assert_eq!(front, [0.0, 0.0, 1.0, -7.0]);
    assert!(!keep(front, [0.0, 0.0, 3.0]) && keep(front, [0.0, 0.0, 9.0]));
    // flipping swaps every point that is off the plane
    for z in [-3.0, 0.0, 6.9, 7.1, 30.0] {
        assert_ne!(keep(behind, [1.0, 2.0, z]), keep(front, [1.0, 2.0, z]), "z {z}");
    }
}

/// A drag whose plane crosses the 15° line while it moves: the mode is picked once, when the
/// drag starts, so the distance follows the pointer without a jump (Review Focus 1).
#[test]
fn drag_mode_latched_across_the_threshold() {
    // looking straight down; the normal leans 16° from vertical, so at the start the arrow is
    // just outside "face on". Moving the plane down along the normal (away from the camera)
    // swings the line of sight to its centre past 15°.
    let c = cam([0.0, 0.0, 120.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    let a = 16f64.to_radians();
    let n = [a.sin(), 0.0, a.cos()];
    let span = Aabb { min: [-10.0, -10.0, -10.0], max: [10.0, 10.0, 10.0] };
    let p0 = DragPlane { origin: [0.0; 3], normal: n, distance: 0.0, span };
    let mode = plane_drag::drag_mode(&c, &p0);
    assert_eq!(mode, plane_drag::DragMode::Projection);
    let at = |d: f64| DragPlane { distance: d, ..p0 };

    // drag against the arrow on screen, a pixel at a time, with the latched mode and the plane
    // as it was at the start (what the viewport does)
    let from = c.world_to_view(plane_drag::center(&p0));
    let tip = plane_drag::handle_at(&c, &p0);
    let len = ((tip[0] - from[0]).powi(2) + (tip[1] - from[1]).powi(2)).sqrt();
    assert!(len > 1.0, "the arrow shows on screen ({len} px)");
    let dir = [(tip[0] - from[0]) / len, (tip[1] - from[1]) / len];
    let mut last = 0.0;
    let mut last_step: Option<f64> = None;
    let mut crossed = None;
    for k in 1..=60 {
        let to = [from[0] - dir[0] * k as f64, from[1] - dir[1] * k as f64];
        let d = plane_drag::drag_distance_in(&c, &p0, from, to, 0.0, mode);
        assert!(d.is_finite(), "step {k}: {d}");
        let step = last - d;
        // a pixel moves the plane by under a millimetre, every step, and each step is close to
        // the one before (perspective grows them slowly): no jump at the 15° line
        assert!(step > 0.0 && step < 1.0, "step {k}: {last} -> {d}");
        if let Some(s0) = last_step {
            assert!(step / s0 > 0.9 && step / s0 < 1.1, "step {k}: {step} after {s0}");
        }
        last_step = Some(step);
        if crossed.is_none() && plane_drag::face_on(&c, &at(d)) {
            crossed = Some(k);
        }
        last = d;
    }
    assert!(crossed.is_some(), "the drag never took the plane past 15° (ended at {last})");

    // a drag that starts face on stays vertical the whole way, even after the plane leaves it
    let start = (0..400).map(|k| -(k as f64) * 0.25).find(|&d| plane_drag::face_on(&c, &at(d))).unwrap() - 2.0;
    let pf = at(start);
    let mode = plane_drag::drag_mode(&c, &pf);
    assert_eq!(mode, plane_drag::DragMode::Vertical);
    let from = c.world_to_view(plane_drag::center(&pf));
    let px = c.pixel_size(plane_drag::center(&pf));
    let mut last = start;
    for k in 1..=200 {
        // up: toward the camera
        let d = plane_drag::drag_distance_in(&c, &pf, from, [from[0], from[1] - k as f64], start, mode);
        assert!((d - (start + k as f64 * px)).abs() < 1e-9, "vertical step {k}: {d}");
        assert!(d > last);
        last = d;
    }
    assert!(!plane_drag::face_on(&c, &at(last)), "the drag left the face-on cone ({last})");
}
