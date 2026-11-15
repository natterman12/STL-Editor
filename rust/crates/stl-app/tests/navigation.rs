//! Milestone 7 task 3 (spec 2.2): the Fusion mouse layout, "orbit around" and zoom toward the
//! cursor. The pure functions are tested on a `Camera`; the buttons and the wheel are also driven
//! through egui events into a real `Viewport`.
//!
//! Native only: the harness needs the native test stack.
#![cfg(not(target_arch = "wasm32"))]

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Event, Modifiers, MouseWheelUnit, PointerButton, Pos2};
use egui_kittest::Harness;
use stl_app::viewport::camera::{Camera, V3};
use stl_app::viewport::navigation::{nav_action, orbit_pivot, wheel_action, zoom_anchor, NavAction, OrbitAround, WheelAction};
use stl_app::viewport::{MouseMode, Viewport};
use stl_kernel::Aabb;

const W: f64 = 800.0;
const H: f64 = 600.0;

/// A 40 x 30 x 20 box standing at (50, 50, 0).
fn part() -> Aabb {
    Aabb { min: [30.0, 35.0, 0.0], max: [70.0, 65.0, 20.0] }
}

fn centre(b: &Aabb) -> V3 {
    [(b.min[0] + b.max[0]) / 2.0, (b.min[1] + b.max[1]) / 2.0, (b.min[2] + b.max[2]) / 2.0]
}

/// A camera looking at `b` from the iso direction, fitted, with no flight left.
fn iso_on(b: Aabb) -> Camera {
    let mut cam = Camera::new();
    cam.set_size(W, H);
    assert!(cam.set_view("iso", Some(b)));
    cam.finish_flight();
    cam
}

fn px_moved(cam: &Camera, p: V3, before: [f64; 2]) -> f64 {
    let now = cam.world_to_view(p);
    ((now[0] - before[0]).powi(2) + (now[1] - before[1]).powi(2)).sqrt()
}

/// The point is in front of the camera and inside the view.
fn on_screen(cam: &Camera, p: V3) -> bool {
    let (_, depth) = cam.project(p);
    let s = cam.world_to_view(p);
    depth > 0.0 && (0.0..=W).contains(&s[0]) && (0.0..=H).contains(&s[1])
}

fn dist(a: V3, b: V3) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// One drag the way `handle_input` does it: the pivot is chosen once, then kept.
fn drag_orbit(cam: &mut Camera, around: OrbitAround, scene: Aabb, under: Option<V3>, steps: &[(f64, f64)]) {
    let mut kept = None;
    for &(dx, dy) in steps {
        let pivot = orbit_pivot(cam, around, Some(scene), under, kept);
        kept = Some(pivot);
        cam.orbit_about(pivot, dx, dy);
    }
}

#[test]
fn fusion_bindings() {
    use PointerButton::*;
    // Model mode: the left button never moves the view
    for shift in [false, true] {
        assert_eq!(nav_action(MouseMode::Model, Primary, shift), None);
    }
    assert_eq!(nav_action(MouseMode::Model, Middle, false), Some(NavAction::Pan));
    assert_eq!(nav_action(MouseMode::Model, Middle, true), Some(NavAction::Orbit));
    assert_eq!(nav_action(MouseMode::Model, Secondary, false), Some(NavAction::Orbit));
    assert_eq!(nav_action(MouseMode::Model, Secondary, true), Some(NavAction::Orbit));
    assert_eq!(nav_action(MouseMode::Model, Extra1, false), None);
    // Sketch and Sculpt: middle pans, right and Shift + middle orbit, left belongs to the tool
    for m in [MouseMode::Sketch, MouseMode::Sculpt] {
        assert_eq!(nav_action(m, Primary, false), None);
        assert_eq!(nav_action(m, Primary, true), None);
        assert_eq!(nav_action(m, Middle, false), Some(NavAction::Pan));
        assert_eq!(nav_action(m, Middle, true), Some(NavAction::Orbit));
        assert_eq!(nav_action(m, Secondary, false), Some(NavAction::Orbit));
    }
    // a mouse wheel zooms toward the cursor: up zooms in, down zooms out
    let none = Modifiers::NONE;
    let up = wheel_action(MouseWheelUnit::Line, egui::vec2(0.0, 1.0), none);
    let down = wheel_action(MouseWheelUnit::Line, egui::vec2(0.0, -1.0), none);
    assert!(matches!(up, WheelAction::Zoom(s) if s < 1.0), "{up:?}");
    assert!(matches!(down, WheelAction::Zoom(s) if s > 1.0), "{down:?}");
    // a browser's wheel notch is 100 points
    let notch = wheel_action(MouseWheelUnit::Point, egui::vec2(0.0, -100.0), none);
    assert!(matches!(notch, WheelAction::Zoom(s) if (s - 1.0 / 0.95).abs() < 1e-12), "{notch:?}");
}

/// Press, drag along `path` and release, with the pointer in the viewport.
fn drag(h: &mut Harness<'_>, button: PointerButton, shift: bool, path: &[Pos2]) {
    let mods = if shift { Modifiers::SHIFT } else { Modifiers::NONE };
    h.event(Event::ModifiersChanged(mods));
    h.event(Event::PointerMoved(path[0]));
    h.step();
    h.event(Event::PointerButton { pos: path[0], button, pressed: true, modifiers: mods });
    h.step();
    for p in &path[1..] {
        h.event(Event::PointerMoved(*p));
        h.step();
    }
    h.event(Event::PointerButton { pos: *path.last().unwrap(), button, pressed: false, modifiers: mods });
    h.step();
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(2);
}

fn line(from: Pos2, to: Pos2) -> Vec<Pos2> {
    (0..=10).map(|i| from + (to - from) * (i as f32 / 10.0)).collect()
}

/// A viewport filling an 800 x 600 window in Model mode, having looked at `part()`.
fn viewport_harness() -> (Harness<'static>, Rc<RefCell<Viewport>>) {
    let mut vp = Viewport::new();
    vp.set_view_size(W as f32, H as f32);
    vp.fit(part());
    vp.camera_mut().finish_flight();
    let vp = Rc::new(RefCell::new(vp));
    let shown = vp.clone();
    let mut h = Harness::builder().with_size(egui::vec2(W as f32, H as f32)).build_ui(move |ui| {
        shown.borrow_mut().show(ui);
    });
    h.run_steps(3);
    (h, vp)
}

fn wheel(h: &mut Harness<'_>, unit: MouseWheelUnit, delta: egui::Vec2, modifiers: Modifiers) {
    h.event(Event::PointerMoved(Pos2::new(400.0, 300.0)));
    h.step();
    h.event(Event::MouseWheel { unit, delta, phase: egui::TouchPhase::Move, source: egui::MouseWheelSource::Unknown, modifiers });
    h.run_steps(2);
}

#[test]
fn buttons_move_the_view_through_egui_events() {
    let (mut h, vp) = viewport_harness();
    let cam = |vp: &Rc<RefCell<Viewport>>| vp.borrow().camera().clone();
    let (a, b) = (Pos2::new(300.0, 300.0), Pos2::new(380.0, 300.0));
    let start = cam(&vp);

    // left drag: nothing
    drag(&mut h, PointerButton::Primary, false, &line(a, b));
    let c = cam(&vp);
    assert!(dist(c.position, start.position) < 1e-9 && dist(c.target, start.target) < 1e-9, "left drag moves nothing");

    // middle drag pans: camera and target move together
    drag(&mut h, PointerButton::Middle, false, &line(a, b));
    let c1 = cam(&vp);
    let pan = dist(c1.position, c.position);
    assert!(pan > 1e-3, "middle drag pans");
    assert!((dist(c1.target, c.target) - pan).abs() < 1e-9);

    // right drag orbits: the target stays, the camera turns on a sphere about it
    drag(&mut h, PointerButton::Secondary, false, &line(a, b));
    let c2 = cam(&vp);
    assert!(dist(c2.target, c1.target) < 1e-9, "orbit about the view centre");
    assert!(dist(c2.position, c1.position) > 1e-3, "right drag orbits");
    assert!((dist(c2.position, c2.target) - dist(c1.position, c1.target)).abs() < 1e-6);

    // Shift + middle drag orbits too
    drag(&mut h, PointerButton::Middle, true, &line(a, b));
    let c3 = cam(&vp);
    assert!(dist(c3.target, c2.target) < 1e-9);
    assert!(dist(c3.position, c2.position) > 1e-3, "Shift + middle drag orbits");

    // the wheel zooms: the distance to the target shrinks
    wheel(&mut h, MouseWheelUnit::Line, egui::vec2(0.0, 1.0), Modifiers::NONE);
    let c4 = cam(&vp);
    assert!(dist(c4.position, c4.target) < dist(c3.position, c3.target) - 1e-6, "the wheel zooms in");
}

#[test]
fn trackpad_scroll_pans_shift_orbits() {
    let none = Modifiers::NONE;
    let shift = Modifiers::SHIFT;
    // two fingers, small points: pan, in the direction the fingers go
    let pan = wheel_action(MouseWheelUnit::Point, egui::vec2(3.5, -12.25), none);
    assert_eq!(pan, WheelAction::Pan(3.5, -12.25));
    // whole-number points under 50, sideways parts and fractions of a line are a trackpad too
    assert!(matches!(wheel_action(MouseWheelUnit::Point, egui::vec2(0.0, 7.0), none), WheelAction::Pan(..)));
    assert!(matches!(wheel_action(MouseWheelUnit::Line, egui::vec2(0.0, 0.2), none), WheelAction::Pan(..)));
    assert!(matches!(wheel_action(MouseWheelUnit::Line, egui::vec2(0.5, 1.0), none), WheelAction::Pan(..)));
    // with Shift they orbit
    assert_eq!(wheel_action(MouseWheelUnit::Point, egui::vec2(3.5, -12.25), shift), WheelAction::Orbit(3.5, -12.25));
    // Ctrl and two fingers zoom (scrolling up zooms in)
    let ctrl = Modifiers::CTRL;
    assert!(matches!(wheel_action(MouseWheelUnit::Point, egui::vec2(0.0, 7.5), ctrl), WheelAction::Zoom(s) if s < 1.0));
    assert!(matches!(wheel_action(MouseWheelUnit::Point, egui::vec2(0.0, -7.5), ctrl), WheelAction::Zoom(s) if s > 1.0));

    // and through events: two fingers pan, with Shift they orbit about the view centre
    let (mut h, vp) = viewport_harness();
    let p0 = vp.borrow().camera().clone();
    wheel(&mut h, MouseWheelUnit::Point, egui::vec2(30.5, 10.5), Modifiers::NONE);
    let p1 = vp.borrow().camera().clone();
    let moved = dist(p1.position, p0.position);
    assert!(moved > 1e-3, "two-finger scroll pans");
    assert!((dist(p1.target, p0.target) - moved).abs() < 1e-9, "a pan moves the target with the camera");
    assert!(dist(sub(p1.position, p1.target), sub(p0.position, p0.target)) < 1e-9, "and turns nothing");

    wheel(&mut h, MouseWheelUnit::Point, egui::vec2(30.5, 10.5), Modifiers::SHIFT);
    let p2 = vp.borrow().camera().clone();
    assert!(dist(p2.position, p1.position) > 1e-3, "Shift + two-finger scroll orbits");
    assert!(dist(p2.target, p1.target) < 1e-9, "the turn is about the view centre");
    assert!((dist(p2.position, p2.target) - dist(p1.position, p1.target)).abs() < 1e-6);
}

#[test]
fn view_centre_keeps_framing_centre() {
    let scene = part();
    let mut cam = iso_on(scene);
    let c = centre(&scene);
    let before = cam.world_to_view(c);
    let start = cam.position;
    // right-drag 180 px across, in one frame and then in small steps
    drag_orbit(&mut cam, OrbitAround::ViewCentre, scene, None, &[(180.0, 0.0)]);
    assert!(dist(cam.position, start) > 10.0, "the view really turned");
    assert!(px_moved(&cam, c, before) < 2.0, "{}", px_moved(&cam, c, before));
    let mut cam = iso_on(scene);
    drag_orbit(&mut cam, OrbitAround::ViewCentre, scene, None, &[(18.0, 6.0); 10]);
    assert!(px_moved(&cam, c, before) < 2.0);
    assert!(on_screen(&cam, c));
}

#[test]
fn origin_keeps_origin() {
    let scene = part();
    let mut cam = iso_on(scene);
    let o = [0.0; 3];
    let before = cam.world_to_view(o);
    let start = cam.position;
    drag_orbit(&mut cam, OrbitAround::Origin, scene, None, &[(180.0, 40.0)]);
    assert!(dist(cam.position, start) > 10.0);
    assert!(px_moved(&cam, o, before) < 2.0, "{}", px_moved(&cam, o, before));
    // while the middle of the view moved: the pivot is not the view centre
    assert!(dist(cam.target, centre(&scene)) > 1.0);
}

#[test]
fn cursor_pivot_far_part_stays_in_view() {
    // SolidCraft #2616: a 10 mm box at the origin and a box far away. The view is on the far one
    // and the drag starts over empty space in a corner: it must not turn about the origin or any
    // other far-off point.
    let small = Aabb { min: [0.0; 3], max: [10.0; 3] };
    let far = Aabb { min: [49990.0, 49990.0, 0.0], max: [50030.0, 50020.0, 20.0] };
    let scene = Aabb { min: small.min, max: far.max };
    let mut cam = iso_on(far);
    let c = centre(&far);
    let before = cam.world_to_view(c);
    drag_orbit(&mut cam, OrbitAround::Cursor, scene, None, &[(180.0, 60.0)]);
    assert!(px_moved(&cam, c, before) < 2.0, "the far part moved {} px", px_moved(&cam, c, before));
    assert!(on_screen(&cam, c));

    // over a surface the point under the cursor stays under the cursor
    let mut cam = iso_on(far);
    let top = [50010.0, 50005.0, 20.0];
    let before = cam.world_to_view(top);
    drag_orbit(&mut cam, OrbitAround::Cursor, scene, Some(top), &[(120.0, -50.0), (60.0, 10.0)]);
    assert!(px_moved(&cam, top, before) < 2.0, "{}", px_moved(&cam, top, before));
    assert!(on_screen(&cam, c));

    // a point behind the eye is not a pivot
    let cam = iso_on(far);
    let behind = [cam.position[0] * 2.0 - cam.target[0], cam.position[1] * 2.0 - cam.target[1], cam.position[2] * 2.0 - cam.target[2]];
    let pivot = orbit_pivot(&cam, OrbitAround::Cursor, Some(scene), Some(behind), None);
    assert!(dist(pivot, cam.target) < 1e-9, "the middle of the view instead: {pivot:?}");
}

#[test]
fn zoom_into_empty_space_then_orbit_keeps_part() {
    let scene = part();
    let mut cam = iso_on(scene);
    let c = centre(&scene);
    let reach0 = dist(cam.position, cam.target);
    // ten wheel notches toward the top left corner of the view, over nothing
    let notch = match wheel_action(MouseWheelUnit::Line, egui::vec2(0.0, 1.0), Modifiers::NONE) {
        WheelAction::Zoom(s) => s,
        a => panic!("{a:?}"),
    };
    for _ in 0..10 {
        let ray = cam.ray([20.0, 20.0]);
        let anchor = zoom_anchor(&cam, ray, None);
        assert!(dist(anchor, cam.position) < 4.0 * dist(cam.position, cam.target) + 1e-9, "not far off");
        cam.zoom_at(notch, anchor);
    }
    assert!(dist(cam.position, cam.target) < reach0 * 0.7, "it zoomed in");
    assert!(cam.project(cam.target).1 > 0.0, "the middle of the view is in front");
    assert!(on_screen(&cam, c), "still in view after the zoom");

    for around in [OrbitAround::ViewCentre, OrbitAround::Cursor] {
        let mut turned = cam.clone();
        let pivot = orbit_pivot(&turned, around, Some(scene), None, None);
        assert!(turned.project(pivot).1 > 0.0, "the turn centre is in front of the eye");
        assert!(dist(pivot, turned.position) < 2.0 * reach0, "and not far off");
        drag_orbit(&mut turned, around, scene, None, &[(120.0, 30.0)]);
        assert!(on_screen(&turned, c), "{around:?}: the part stays on screen after the orbit");
        // the box's corners stay in front of the eye as well
        for x in [scene.min[0], scene.max[0]] {
            for z in [scene.min[2], scene.max[2]] {
                assert!(turned.project([x, scene.min[1], z]).1 > 0.0);
            }
        }
    }

    // zooming over a surface keeps that surface point under the cursor
    let mut cam = iso_on(scene);
    let top = [50.0, 50.0, 20.0];
    let at = cam.world_to_view(top);
    for _ in 0..5 {
        let ray = cam.ray(at);
        let anchor = zoom_anchor(&cam, ray, Some(top));
        cam.zoom_at(notch, anchor);
    }
    assert!(px_moved(&cam, top, at) < 1e-6);
    assert!(dist(cam.position, cam.target) < reach0, "closer");

    // a hit far behind the target is held to four times the target's distance
    let cam = iso_on(scene);
    let ray = cam.ray([400.0, 300.0]);
    let far_hit = [ray.0[0] + ray.1[0] * 1e6, ray.0[1] + ray.1[1] * 1e6, ray.0[2] + ray.1[2] * 1e6];
    let anchor = zoom_anchor(&cam, ray, Some(far_hit));
    let reach = dist(cam.position, cam.target);
    assert!((dist(anchor, cam.position) - 4.0 * reach).abs() < 1e-6);
    // and a hit behind the eye is ignored for the ray point at the target's depth
    let anchor = zoom_anchor(&cam, ray, Some([cam.position[0] - ray.1[0], cam.position[1] - ray.1[1], cam.position[2] - ray.1[2]]));
    assert!(dist(anchor, cam.target) < 1e-6, "{anchor:?}");

    // zooming in never reaches the target
    let mut cam = iso_on(scene);
    for _ in 0..500 {
        cam.zoom_at(0.5, cam.target);
    }
    assert!(dist(cam.position, cam.target) >= 0.5 - 1e-9);
    assert!(cam.position.iter().chain(cam.target.iter()).all(|v| v.is_finite()));
}

#[test]
fn turntable_up_stays_up() {
    let scene = part();
    for around in [OrbitAround::ViewCentre, OrbitAround::Origin] {
        let mut cam = iso_on(scene);
        // a long run of drags in every direction, some far enough to hit the poles
        let drags = [(400.0, 0.0), (0.0, 900.0), (-200.0, -2000.0), (37.0, 5.0), (0.0, 4000.0), (-90.0, 0.0), (0.0, -8000.0), (13.0, 77.0)];
        for (dx, dy) in drags {
            drag_orbit(&mut cam, around, scene, None, &[(dx, dy)]);
            let (right, up, back) = cam.basis();
            assert_eq!(cam.up, [0.0, 0.0, 1.0]);
            assert!(right[2].abs() < 1e-9, "no roll: the right axis is level: {right:?}");
            assert!(up[2] >= -1e-9, "up stays up: {up:?}");
            assert!(back.iter().all(|v| v.is_finite()));
        }
    }

    // from the top view (screen up is +Y) the first turn keeps the heading and puts Z up
    let mut cam = Camera::new();
    cam.set_size(W, H);
    cam.set_view("top", Some(part()));
    cam.finish_flight();
    cam.orbit_about(cam.target, 0.0, 0.0);
    let (_, up, _) = cam.basis();
    assert!(up[1] > 0.999, "the screen's up still points along +Y: {up:?}");
    assert_eq!(cam.up, [0.0, 0.0, 1.0]);
    // tilting down from the top view turns the camera toward the heading of screen up
    let t0 = cam.target;
    cam.orbit_about(t0, 0.0, -150.0); // a quarter of the view height, tilting toward the horizon
    assert!(cam.position[2] < t0[2] + dist(cam.position, t0) * 0.01, "it came down to the horizon");
    assert!(cam.position[1] < t0[1] - 1.0, "from the -Y side, as the top view's screen up is +Y");
}

#[test]
fn orbit_turns_as_before_about_the_target() {
    // the 1.x numbers: a drag of the view's height is a full turn, and the distance stays
    let mut cam = Camera::new();
    cam.set_size(W, H);
    let start = cam.position;
    cam.orbit_about(cam.target, H, 0.0);
    assert!(dist(cam.position, start) < 1e-6);
    cam.orbit_about([0.0; 3], H / 4.0, 0.0);
    assert!((cam.position[2] - start[2]).abs() < 1e-9, "a turn about Z keeps the height");
    assert!((dist(cam.position, [0.0; 3]) - dist(start, [0.0; 3])).abs() < 1e-9);
}

// ---- review fixes (Task 3) ----

#[test]
fn zoom_with_the_camera_on_its_target_stays_finite() {
    let mut cam = iso_on(part());
    cam.position = cam.target;
    for s in [0.5, 1.0, 2.0, 1e-12, 1e12] {
        for anchor in [cam.target, [1.0, 2.0, 3.0]] {
            cam.zoom_at(s, anchor);
            assert!(cam.position.iter().chain(cam.target.iter()).all(|v| v.is_finite()), "s = {s}: {:?} {:?}", cam.position, cam.target);
        }
    }
    // zooming in from nothing does nothing; a broken scale or anchor changes nothing
    let mut cam = iso_on(part());
    cam.position = cam.target;
    let before = (cam.position, cam.target);
    cam.zoom_at(0.5, [1.0, 2.0, 3.0]);
    assert_eq!((cam.position, cam.target), before, "no distance to zoom in on");
    cam.zoom_at(f64::NAN, cam.target);
    cam.zoom_at(0.5, [f64::INFINITY, 0.0, 0.0]);
    assert_eq!((cam.position, cam.target), before);
}

#[test]
fn wheel_orbit_never_reuses_a_dragged_pivot() {
    let (mut h, vp) = viewport_harness();
    // a drag about the world origin (the target is elsewhere), then let go
    vp.borrow_mut().orbit_around = OrbitAround::Origin;
    let (a, b) = (Pos2::new(300.0, 300.0), Pos2::new(380.0, 300.0));
    drag(&mut h, PointerButton::Secondary, false, &line(a, b));
    let t0 = vp.borrow().camera().target;
    assert!(dist(t0, [0.0; 3]) > 1.0, "the target is not at the origin, so a turn about the origin moves it");

    // the next turn is a wheel orbit about the view centre: it must not go about the old pivot
    vp.borrow_mut().orbit_around = OrbitAround::ViewCentre;
    wheel(&mut h, MouseWheelUnit::Point, egui::vec2(30.5, 10.5), Modifiers::SHIFT);
    let c = vp.borrow().camera().clone();
    assert!(dist(c.target, t0) < 1e-9, "the wheel orbit turned about a stale pivot: target moved {}", dist(c.target, t0));

    // a held drag keeps its pivot through a frame where the pointer stands still
    vp.borrow_mut().orbit_around = OrbitAround::Origin;
    let o = |vp: &Rc<RefCell<Viewport>>| dist(vp.borrow().camera().position, [0.0; 3]);
    h.event(Event::PointerMoved(a));
    h.step();
    h.event(Event::PointerButton { pos: a, button: PointerButton::Secondary, pressed: true, modifiers: Modifiers::NONE });
    h.step();
    h.event(Event::PointerMoved(a + egui::vec2(20.0, 0.0)));
    h.step();
    let d0 = o(&vp);
    h.run_steps(3); // nothing moves
    vp.borrow_mut().orbit_around = OrbitAround::ViewCentre; // were the pivot dropped, the next step would pick this one
    h.event(Event::PointerMoved(a + egui::vec2(60.0, 0.0)));
    h.step();
    assert!((o(&vp) - d0).abs() < 1e-6, "the held drag kept turning about the origin");
    h.event(Event::PointerButton { pos: a, button: PointerButton::Secondary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
}

/// A 20 mm cube on the bed in a viewport, fitted, with the top face in the middle of the window.
fn cube_harness() -> (Harness<'static>, Rc<RefCell<Viewport>>, V3) {
    use std::sync::Arc;
    use stl_app::engine_link::BodyView;
    use stl_kernel::MeshArrays;
    let p = [
        [-10.0, -10.0, 0.0], [10.0, -10.0, 0.0], [10.0, 10.0, 0.0], [-10.0, 10.0, 0.0],
        [-10.0, -10.0, 20.0], [10.0, -10.0, 20.0], [10.0, 10.0, 20.0], [-10.0, 10.0, 20.0],
    ];
    let t: [u32; 36] = [0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 1, 2, 6, 1, 6, 5, 2, 3, 7, 2, 7, 6, 3, 0, 4, 3, 4, 7];
    let mesh = Arc::new(MeshArrays { positions: p.iter().flatten().map(|&v| v as f32).collect(), triangles: t.to_vec() });
    let body = BodyView { id: "b1".into(), name: "Cube".into(), color: None, hidden: false, solid: true, mesh, revision: 1 };
    let mut vp = Viewport::new();
    vp.set_view_size(W as f32, H as f32);
    vp.sync_bodies(&[body]);
    vp.fit(Aabb { min: [-10.0, -10.0, 0.0], max: [10.0, 10.0, 20.0] });
    vp.camera_mut().finish_flight();
    let vp = Rc::new(RefCell::new(vp));
    let shown = vp.clone();
    let mut h = Harness::builder().with_size(egui::vec2(W as f32, H as f32)).build_ui(move |ui| {
        shown.borrow_mut().show(ui);
    });
    h.run_steps(3);
    // the cube is prepared on a worker thread: wait until it is pickable (a slow CI machine
    // could otherwise pick nothing)
    vp.borrow_mut().flush();
    h.run_steps(1);
    // a point on the top face, off the view axis
    (h, vp, [6.0, 4.0, 20.0])
}

fn screen_of(vp: &Rc<RefCell<Viewport>>, p: V3) -> Pos2 {
    let v = vp.borrow().camera().world_to_view(p);
    vp.borrow().rect().min + egui::vec2(v[0] as f32, v[1] as f32)
}

#[test]
fn pinch_zooms_toward_the_surface_under_the_pinch() {
    // a trackpad pinch (Event::Zoom): the surface point under the cursor stays under the cursor
    let (mut h, vp, top) = cube_harness();
    let at = screen_of(&vp, top);
    let before = vp.borrow().surface_point(at).expect("the top face is under the cursor");
    assert!(dist(before, top) < 0.5, "{before:?}");
    h.event(Event::PointerMoved(at));
    h.step();
    let reach0 = dist(vp.borrow().camera().position, vp.borrow().camera().target);
    h.event(Event::Zoom(1.25));
    h.run_steps(2);
    let after = screen_of(&vp, before);
    assert!((after - at).length() < 1.0, "the surface point slid {} px", (after - at).length());
    assert!(dist(vp.borrow().camera().position, vp.borrow().camera().target) < reach0 - 1e-6, "zoomed in");

    // two fingers on a touch screen: the same, about the pinch centre
    let (mut h, vp, top) = cube_harness();
    let at = screen_of(&vp, top);
    let before = vp.borrow().surface_point(at).unwrap();
    let reach0 = dist(vp.borrow().camera().position, vp.borrow().camera().target);
    h.event(Event::PointerMoved(at)); // egui-winit moves the pointer to the first finger
    h.step();
    let touch = |id: u64, phase: egui::TouchPhase, pos: Pos2| Event::Touch { device_id: egui::TouchDeviceId(0), id: egui::TouchId(id), phase, pos, force: None };
    let half = |w: f32| egui::vec2(w, 0.0);
    h.event(touch(1, egui::TouchPhase::Start, at - half(40.0)));
    h.event(touch(2, egui::TouchPhase::Start, at + half(40.0)));
    h.step();
    for w in [50.0, 60.0, 70.0, 80.0] {
        h.event(touch(1, egui::TouchPhase::Move, at - half(w)));
        h.event(touch(2, egui::TouchPhase::Move, at + half(w)));
        h.step();
    }
    h.event(touch(1, egui::TouchPhase::End, at - half(80.0)));
    h.event(touch(2, egui::TouchPhase::End, at + half(80.0)));
    h.run_steps(2);
    assert!(dist(vp.borrow().camera().position, vp.borrow().camera().target) < reach0 - 1e-6, "the two-finger pinch zoomed in");
    let after = screen_of(&vp, before);
    assert!((after - at).length() < 1.5, "the surface under the pinch centre slid {} px", (after - at).length());
}

#[test]
fn two_finger_translate_pans_and_does_not_orbit() {
    // the first finger also arrives as the left button; with two down it must not orbit as well
    let (mut h, vp, top) = cube_harness();
    let at = screen_of(&vp, top);
    let touch = |id: u64, phase: egui::TouchPhase, pos: Pos2| Event::Touch { device_id: egui::TouchDeviceId(0), id: egui::TouchId(id), phase, pos, force: None };
    let half = egui::vec2(40.0, 0.0);
    let dir = |c: &Camera| sub(c.target, c.position);
    let (dir0, target0) = {
        let v = vp.borrow();
        (dir(v.camera()), v.camera().target)
    };
    h.event(Event::PointerMoved(at - half));
    h.event(Event::PointerButton { pos: at - half, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.event(touch(1, egui::TouchPhase::Start, at - half));
    h.event(touch(2, egui::TouchPhase::Start, at + half));
    h.step();
    for k in 1..=5 {
        let d = egui::vec2(12.0 * k as f32, 6.0 * k as f32);
        h.event(Event::PointerMoved(at - half + d));
        h.event(touch(1, egui::TouchPhase::Move, at - half + d));
        h.event(touch(2, egui::TouchPhase::Move, at + half + d));
        h.step();
    }
    h.event(touch(1, egui::TouchPhase::End, at - half + egui::vec2(60.0, 30.0)));
    h.event(touch(2, egui::TouchPhase::End, at + half + egui::vec2(60.0, 30.0)));
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    let v = vp.borrow();
    assert!(dist(v.camera().target, target0) > 1e-6, "the two-finger drag panned");
    let (d1, n) = (dir(v.camera()), |a: V3| (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt());
    let cos = (d1[0] * dir0[0] + d1[1] * dir0[1] + d1[2] * dir0[2]) / (n(d1) * n(dir0));
    assert!(cos > 1.0 - 1e-9, "the view direction stayed put (cos {cos})");
}
