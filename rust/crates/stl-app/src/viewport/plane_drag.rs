//! The drag-the-plane control's maths (spec 2.2), shared by Section View and Split. Plain
//! functions with no wgpu, so the app and the tests can call them.

use super::camera::{self, Camera, V3};
use stl_kernel::Aabb;

/// A plane the user can drag along its normal: `origin` and `normal` as picked, `distance` (mm)
/// along the normal from there, and `span` the box it should cover.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragPlane {
    pub origin: V3,
    pub normal: V3,
    pub distance: f64,
    pub span: Aabb,
}

/// The distance a fresh pick starts at: 0 when the plane passes strictly through `span`, else the
/// distance that puts it halfway through `span` along the normal.
pub fn start_distance(origin: V3, normal: V3, span: &Aabb) -> f64 {
    let n = camera::normalize(normal);
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for i in 0..8 {
        let c = [
            if i & 1 == 0 { span.min[0] } else { span.max[0] },
            if i & 2 == 0 { span.min[1] } else { span.max[1] },
            if i & 4 == 0 { span.min[2] } else { span.max[2] },
        ];
        let s = camera::dot(camera::sub(c, origin), n);
        lo = lo.min(s);
        hi = hi.max(s);
    }
    let eps = 1e-6 * (hi - lo).max(1.0);
    if lo < -eps && hi > eps { 0.0 } else { (lo + hi) / 2.0 }
}

/// The arrow's length on screen (points).
pub const ARROW_PX: f64 = 70.0;
/// How near the arrow the pointer must be to grab it (points).
pub const HIT_PX: f64 = 9.0;
/// `cos 15°`: a normal this close to the line of sight is "face on" and the drag falls back to
/// vertical screen movement.
pub const FACE_ON_COS: f64 = 0.965_925_826_289_068_3;

/// The plane's point after its distance: `origin + normal · distance`.
pub fn moved_origin(p: &DragPlane) -> V3 {
    let n = camera::normalize(p.normal);
    camera::add(p.origin, camera::scale(n, p.distance))
}

/// The middle of the plane: the span's centre dropped onto it. The arrow starts here; the
/// centres for every distance lie on one line along the normal.
pub fn center(p: &DragPlane) -> V3 {
    let n = camera::normalize(p.normal);
    let bc = camera::box_center(&p.span);
    camera::add(bc, camera::scale(n, camera::dot(camera::sub(moved_origin(p), bc), n)))
}

/// The arrow's length in millimetres (a fixed size on screen).
pub fn arrow_len(cam: &Camera, p: &DragPlane) -> f64 {
    ARROW_PX * cam.pixel_size(center(p))
}

/// The normal points (almost) straight at or away from the camera.
pub fn face_on(cam: &Camera, p: &DragPlane) -> bool {
    let n = camera::normalize(p.normal);
    let view = camera::normalize(camera::sub(center(p), cam.position));
    camera::dot(n, view).abs() > FACE_ON_COS
}

/// Where the pointer's ray passes closest to the normal line through the span's centre, as a
/// parameter along the normal. `None` when the ray runs along the line.
fn line_param(cam: &Camera, p: &DragPlane, ptr: [f64; 2]) -> Option<f64> {
    let n = camera::normalize(p.normal);
    let l0 = camera::box_center(&p.span);
    let (o, dir) = cam.ray(ptr);
    let w = camera::sub(l0, o);
    let b = camera::dot(n, dir);
    let den = 1.0 - b * b;
    if den < 1e-9 {
        return None;
    }
    let t = (b * camera::dot(dir, w) - camera::dot(n, w)) / den;
    t.is_finite().then_some(t)
}

/// How a drag turns pointer movement into distance. Chosen once, when the drag starts
/// ([`drag_mode`]), and kept for the whole drag, so it never jumps from one rule to the other
/// as the plane moves across the 15° line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragMode {
    /// The pointer's projection onto the normal line.
    Projection,
    /// Face on: vertical movement, up toward the camera, at the pixel size at the plane.
    Vertical,
}

/// The mode a drag starting now uses: [`DragMode::Vertical`] when the normal is face on.
pub fn drag_mode(cam: &Camera, p: &DragPlane) -> DragMode {
    if face_on(cam, p) {
        DragMode::Vertical
    } else {
        DragMode::Projection
    }
}

/// The distance after dragging the arrow from screen point `from` to `to` (view points from
/// the top left), starting at `d0`, in the mode [`drag_mode`] picks for `p` as given. A drag
/// should pass the plane as it was when it started (or use [`drag_distance_in`] with a latched
/// mode). The plane follows the pointer's projection onto the normal line; face on (within
/// about 15°) it follows vertical movement instead, up moving the plane toward the camera.
pub fn drag_distance(cam: &Camera, p: &DragPlane, from: [f64; 2], to: [f64; 2], d0: f64) -> f64 {
    drag_distance_in(cam, p, from, to, d0, drag_mode(cam, p))
}

/// [`drag_distance`] in a given mode (latched when the drag started).
pub fn drag_distance_in(cam: &Camera, p: &DragPlane, from: [f64; 2], to: [f64; 2], d0: f64, mode: DragMode) -> f64 {
    let vertical = || {
        let c = center(p);
        let toward = camera::dot(camera::normalize(p.normal), camera::sub(cam.position, c));
        let s = if toward >= 0.0 { 1.0 } else { -1.0 };
        d0 + (from[1] - to[1]) * cam.pixel_size(c) * s
    };
    let d = match mode {
        DragMode::Vertical => vertical(),
        DragMode::Projection => match (line_param(cam, p, from), line_param(cam, p, to)) {
            (Some(a), Some(b)) => d0 + (b - a),
            // the pointer sits exactly on the normal line's vanishing point
            _ => vertical(),
        },
    };
    if d.is_finite() { d } else { d0 }
}

/// The arrow's tip on screen.
pub fn handle_at(cam: &Camera, p: &DragPlane) -> [f64; 2] {
    let n = camera::normalize(p.normal);
    cam.world_to_view(camera::add(center(p), camera::scale(n, arrow_len(cam, p))))
}

/// The pointer is over the arrow (its shaft or its head; face on, its dot).
pub fn hits_handle(cam: &Camera, p: &DragPlane, ptr: [f64; 2]) -> bool {
    let c = center(p);
    if cam.project(c).1 <= 0.0 {
        return false;
    }
    let a = cam.world_to_view(c);
    let b = handle_at(cam, p);
    let ab = [b[0] - a[0], b[1] - a[1]];
    let ap = [ptr[0] - a[0], ptr[1] - a[1]];
    let len2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if len2 > 1e-12 {
        ((ap[0] * ab[0] + ap[1] * ab[1]) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let q = [a[0] + ab[0] * t - ptr[0], a[1] + ab[1] * t - ptr[1]];
    let near_shaft = (q[0] * q[0] + q[1] * q[1]).sqrt() <= HIT_PX;
    let near_tip = ((b[0] - ptr[0]).powi(2) + (b[1] - ptr[1]).powi(2)).sqrt() <= HIT_PX * 1.5;
    let near_dot = (ap[0] * ap[0] + ap[1] * ap[1]).sqrt() <= HIT_PX * 1.5;
    near_shaft || near_tip || near_dot
}

/// The kept half as `section.rs` takes it (keep where `dot(n, x) + d >= 0`): the side behind the
/// normal, or the side in front of it with `flip`.
pub fn plane_eq(p: &DragPlane, flip: bool) -> [f64; 4] {
    let n = camera::normalize(p.normal);
    let c = camera::dot(n, camera::add(p.origin, camera::scale(n, p.distance)));
    if flip {
        [n[0], n[1], n[2], -c]
    } else {
        [-n[0], -n[1], -n[2], c]
    }
}
