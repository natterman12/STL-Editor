//! How the mouse moves the view (spec 2.2): which button orbits or pans, the point a turn goes
//! around, and the point a wheel zoom goes toward. `handle_input` in `viewport/mod.rs` calls these;
//! `camera.rs` does the moves.

use stl_kernel::Aabb;

use super::MouseMode;
use super::camera::{self, Camera, V3};

/// Settings › Navigation › Orbit around (setting `"orbit_around"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum OrbitAround {
    #[default]
    ViewCentre,
    Origin,
    Cursor,
}

impl OrbitAround {
    pub const KEY: &'static str = "orbit_around";

    pub fn from_setting(s: Option<&str>) -> Self {
        match s {
            Some("origin") => OrbitAround::Origin,
            Some("cursor") => OrbitAround::Cursor,
            _ => OrbitAround::ViewCentre,
        }
    }

    pub fn as_setting(self) -> &'static str {
        match self {
            OrbitAround::ViewCentre => "view_centre",
            OrbitAround::Origin => "origin",
            OrbitAround::Cursor => "cursor",
        }
    }
}

/// What a mouse drag does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavAction {
    Orbit,
    Pan,
}

/// What one wheel or trackpad scroll event does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WheelAction {
    /// Scale the distance to the zoom anchor by this factor (below 1 zooms in).
    Zoom(f64),
    /// Pan by these points (right and down positive).
    Pan(f64, f64),
    /// Orbit by these points.
    Orbit(f64, f64),
}

/// The mouse buttons (spec 2.2, the Fusion layout): middle drag pans, Shift + middle drag and right
/// drag orbit, and the left button never moves the view (it belongs to selecting, the tools and
/// the sketch). The same in every mode, so Sketch and Sculpt only gain Shift + middle.
pub fn nav_action(mode: MouseMode, button: egui::PointerButton, shift: bool) -> Option<NavAction> {
    use egui::PointerButton::*;
    let _ = mode;
    match button {
        Middle if shift => Some(NavAction::Orbit),
        Middle => Some(NavAction::Pan),
        Secondary => Some(NavAction::Orbit),
        _ => None,
    }
}

/// A trackpad is told from a mouse wheel by what the event looks like, because egui gives no
/// source on Windows, Linux or the web. A wheel notch is a whole number straight up or down: a
/// `Line` or `Page` count (1, 2, 3...), or on the web, where Chromium reports pixels, about 100
/// of them. A trackpad gives `Point` deltas that are small, fractional or sideways as well, and a
/// Windows precision touchpad sends fractions of a line. So: a notch has no sideways part, is a
/// whole number, and is at least 50 when counted in points. One big whole-number flick on a
/// trackpad can read as a notch; that costs one zoom step.
fn is_notch(unit: egui::MouseWheelUnit, delta: egui::Vec2) -> bool {
    if delta.x != 0.0 || delta.y == 0.0 || delta.y.fract() != 0.0 {
        return false;
    }
    match unit {
        egui::MouseWheelUnit::Line | egui::MouseWheelUnit::Page => true,
        egui::MouseWheelUnit::Point => delta.y.abs() >= 50.0,
    }
}

/// One wheel event (spec 2.2): a mouse wheel zooms toward the cursor; a trackpad two-finger
/// scroll pans, with Shift it orbits, with Ctrl it zooms.
pub fn wheel_action(unit: egui::MouseWheelUnit, delta: egui::Vec2, modifiers: egui::Modifiers) -> WheelAction {
    if !is_notch(unit, delta) {
        // fingers on a trackpad: the view follows them, like a drag
        let (dx, dy) = (delta.x as f64, delta.y as f64);
        if modifiers.ctrl {
            return WheelAction::Zoom(zoom_factor(-dy * 5.0));
        }
        return if modifiers.shift {
            WheelAction::Orbit(dx, dy)
        } else {
            WheelAction::Pan(dx, dy)
        };
    }
    // WheelEvent.deltaY points the other way; a notch is 100 in Chromium
    let mut d = -delta.y as f64
        * match unit {
            egui::MouseWheelUnit::Point => 1.0,
            egui::MouseWheelUnit::Line | egui::MouseWheelUnit::Page => 100.0,
        };
    if modifiers.ctrl {
        d *= 10.0;
    }
    WheelAction::Zoom(zoom_factor(d))
}

/// `0.95` per 100 of a browser's `deltaY` (positive zooms out).
fn zoom_factor(d: f64) -> f64 {
    let k = 0.95f64.powf((d * 0.01).abs());
    if d < 0.0 {
        k
    } else if d > 0.0 {
        1.0 / k
    } else {
        1.0
    }
}

fn finite(p: V3) -> bool {
    p.iter().all(|v| v.is_finite())
}

/// How far a point is in front of the eye along the view direction (negative: behind it).
fn depth(cam: &Camera, p: V3) -> f64 {
    let (_, _, back) = cam.basis();
    -camera::dot(camera::sub(p, cam.position), back)
}

/// The middle of the view at the part's depth: the camera target. Fit puts it at the part's
/// centre; panning, zooming and every turn keep it on the view axis in front of the eye, so it is
/// never behind the camera. Only a camera sitting on its target falls back to the scene's middle.
fn view_centre(cam: &Camera, scene: Option<Aabb>) -> V3 {
    if finite(cam.target) && depth(cam, cam.target) > 1e-9 {
        return cam.target;
    }
    let (_, _, back) = cam.basis();
    let ahead = camera::add(cam.position, camera::scale(back, -100.0));
    match scene
        .filter(|b| !camera::box_is_empty(b))
        .map(|b| camera::box_center(&b))
    {
        Some(c) if finite(c) && depth(cam, c) > 1e-9 => c,
        _ => ahead,
    }
}

/// The point a turn goes around. `under_cursor`: the model point under the cursor when the drag
/// started (only asked for with `OrbitAround::Cursor`); `kept`: the pivot this drag used last frame.
pub fn orbit_pivot(
    cam: &Camera,
    around: OrbitAround,
    scene: Option<Aabb>,
    under_cursor: Option<V3>,
    kept: Option<V3>,
) -> V3 {
    if let Some(k) = kept.filter(|k| finite(*k)) {
        return k;
    }
    match around {
        OrbitAround::Origin => [0.0; 3],
        OrbitAround::ViewCentre => view_centre(cam, scene),
        // over empty space (or a point behind the eye): about the middle of the part in view,
        // never a far-off point (SolidCraft #2616)
        OrbitAround::Cursor => under_cursor
            .filter(|p| finite(*p) && depth(cam, *p) > 1e-9)
            .unwrap_or_else(|| view_centre(cam, scene)),
    }
}

/// The point a wheel zoom goes toward: the surface under the cursor, else the point on the
/// cursor's ray at the target's depth; never behind the eye or far off (5 % to 4 times the
/// target's distance along the ray).
pub fn zoom_anchor(cam: &Camera, cursor_ray: (V3, V3), surface_hit: Option<V3>) -> V3 {
    let (origin, dir) = cursor_ray;
    let (_, _, back) = cam.basis();
    let forward = camera::scale(back, -1.0);
    let reach = camera::length(camera::sub(cam.target, cam.position)).max(1e-6);
    // the ray's point on the plane through the target, square to the view
    let along_view = camera::dot(dir, forward);
    let at_target_depth = if along_view > 1e-6 { reach / along_view } else { reach };
    let t = surface_hit
        .filter(|p| finite(*p))
        .map(|p| camera::dot(camera::sub(p, origin), dir))
        .filter(|t| *t > 0.0)
        .unwrap_or(at_target_depth);
    camera::add(origin, camera::scale(dir, t.clamp(0.05 * reach, 4.0 * reach)))
}
