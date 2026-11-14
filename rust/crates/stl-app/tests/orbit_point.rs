//! Looking straight at a face keeps the orbit point on the part (Nathan, 2026-11-14): Sketch and
//! Look At put the camera where 1.x does, but orbiting afterwards turns around the middle of the
//! parts, not around the point on the face.

use stl_app::viewport::camera::{box_center, Camera};
use stl_kernel::Aabb;

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

#[test]
fn face_view_orbits_around_the_part() {
    // a 40 x 20 x 10 part; looking at its +X side face (x = 40)
    let scene = Aabb { min: [0.0, 0.0, 0.0], max: [40.0, 20.0, 10.0] };
    let mut cam = Camera::new();
    cam.set_size(800.0, 600.0);
    cam.look_at_plane([40.0, 10.0, 5.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0], None, Some(scene));
    cam.finish_flight();

    let centre = box_center(&scene);
    assert_eq!(cam.target, centre, "the orbit point is the parts' middle");
    // the view is still square on to the face, centred on it
    let d = [cam.position[0] - cam.target[0], cam.position[1] - cam.target[1], cam.position[2] - cam.target[2]];
    assert!(d[0] > 0.0 && d[1].abs() < 1e-9 && d[2].abs() < 1e-9, "looking straight down -X: {d:?}");

    // an orbit step keeps the distance to the middle
    let r = dist(cam.position, centre);
    cam.rotate(120.0, 40.0);
    assert!((dist(cam.position, centre) - r).abs() < 1e-6, "orbiting turns around the part");
    assert_eq!(cam.target, centre);
}

#[test]
fn close_up_keeps_the_target_in_front() {
    // a tiny radius on a long part: the middle would be behind the camera, so the face point is kept
    let scene = Aabb { min: [0.0, 0.0, 0.0], max: [1000.0, 20.0, 10.0] };
    let mut cam = Camera::new();
    cam.set_size(800.0, 600.0);
    cam.look_at_plane([1000.0, 10.0, 5.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0], Some(5.0), Some(scene));
    cam.finish_flight();
    assert!(cam.position[0] > cam.target[0], "the target is in front of the camera");
}
