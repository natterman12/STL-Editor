//! The camera and its mouse controls, as 1.x `viewer.js` sets up three.js: a 35° perspective
//! camera with Z up, and OrbitControls with no damping, screen-space panning and zoom to the
//! cursor. The numbers here are the ones `viewer.js` and three r170's `OrbitControls.js` use.

use stl_kernel::Aabb;

use super::MouseMode;

pub type V3 = [f64; 3];
/// Column-major 4x4 matrix (`m[col][row]`), as wgpu and WGSL expect.
pub type M4 = [[f64; 4]; 4];

/// `new THREE.PerspectiveCamera(35, 1, 0.1, 100000)`.
pub const FOV_DEG: f64 = 35.0;
pub const NEAR: f64 = 0.1;
pub const FAR: f64 = 100000.0;
/// `this.camera.position.set(160, -220, 150)`, looking at the origin.
pub const START_POSITION: V3 = [160.0, -220.0, 150.0];
/// `_flyTo`'s 320 ms, eased out with `1 - (1 - k)^3`.
pub const FLY_SECONDS: f64 = 0.32;
/// `Spherical.makeSafe`'s EPS.
const SAFE_EPS: f64 = 0.000001;

/// What a mouse drag does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragAction {
    Rotate,
    Pan,
}

/// `setMouseMode`: model mode rotates with the left button and pans with middle and right;
/// sketch and sculpt leave the left button to the tool, pan with middle and rotate with right.
pub fn drag_action(mode: MouseMode, button: egui::PointerButton) -> Option<DragAction> {
    use egui::PointerButton::*;
    match (mode, button) {
        (MouseMode::Model, Primary) => Some(DragAction::Rotate),
        (MouseMode::Model, Middle | Secondary) => Some(DragAction::Pan),
        (MouseMode::Sketch | MouseMode::Sculpt, Primary) => None,
        (MouseMode::Sketch | MouseMode::Sculpt, Middle) => Some(DragAction::Pan),
        (MouseMode::Sketch | MouseMode::Sculpt, Secondary) => Some(DragAction::Rotate),
        _ => None,
    }
}

pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn scale(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
pub fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
pub fn length(a: V3) -> f64 {
    dot(a, a).sqrt()
}
/// three's `Vector3.normalize`: a zero vector stays zero.
pub fn normalize(a: V3) -> V3 {
    let l = length(a);
    scale(a, 1.0 / if l > 0.0 { l } else { 1.0 })
}
fn lerp(a: V3, b: V3, t: f64) -> V3 {
    add(a, scale(sub(b, a), t))
}

/// The box's centre and half diagonal, or `None` for an empty box (`Box3.isEmpty`).
pub fn box_is_empty(b: &Aabb) -> bool {
    b.max[0] < b.min[0] || b.max[1] < b.min[1] || b.max[2] < b.min[2]
}
pub fn empty_box() -> Aabb {
    Aabb { min: [f64::INFINITY; 3], max: [f64::NEG_INFINITY; 3] }
}
pub fn box_union(a: &Aabb, b: &Aabb) -> Aabb {
    Aabb {
        min: [a.min[0].min(b.min[0]), a.min[1].min(b.min[1]), a.min[2].min(b.min[2])],
        max: [a.max[0].max(b.max[0]), a.max[1].max(b.max[1]), a.max[2].max(b.max[2])],
    }
}
pub fn box_center(b: &Aabb) -> V3 {
    scale(add(b.min, b.max), 0.5)
}
pub fn box_diagonal(b: &Aabb) -> f64 {
    length(sub(b.max, b.min))
}

#[derive(Clone, Copy, Debug)]
struct Fly {
    start: f64,
    from: (V3, V3, V3),
    to: (V3, V3, V3),
}

/// The camera: position, orbit target and up vector, in millimetres.
#[derive(Clone, Debug)]
pub struct Camera {
    pub position: V3,
    pub target: V3,
    pub up: V3,
    /// The view's size in points (CSS pixels in 1.x).
    pub width: f64,
    pub height: f64,
    fly: Option<Fly>,
    /// The clock `fit` and the views start their flight from (seconds, egui's `input.time`).
    now: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Camera { position: START_POSITION, target: [0.0; 3], up: [0.0, 0.0, 1.0], width: 1.0, height: 1.0, fly: None, now: 0.0 }
    }
}

impl Camera {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_size(&mut self, width: f64, height: f64) {
        if width > 0.0 && height > 0.0 {
            self.width = width;
            self.height = height;
        }
    }

    pub fn aspect(&self) -> f64 {
        self.width / self.height
    }

    /// `halfFov`: half the narrower field of view (tall, narrow screens are limited by the width).
    pub fn half_fov(&self) -> f64 {
        let v = (FOV_DEG / 2.0).to_radians();
        v.min((v.tan() * self.aspect().max(0.05)).atan())
    }

    /// The camera's axes in the world, as `Object3D.lookAt` builds them: x right, y up, z back.
    pub fn basis(&self) -> (V3, V3, V3) {
        let mut z = sub(self.position, self.target);
        if dot(z, z) == 0.0 {
            z[2] = 1.0;
        }
        z = normalize(z);
        let mut x = cross(self.up, z);
        if dot(x, x) == 0.0 {
            if (self.up[2].abs() - 1.0).abs() < f64::EPSILON {
                z[0] += 0.0001;
            } else {
                z[2] += 0.0001;
            }
            z = normalize(z);
            x = cross(self.up, z);
        }
        x = normalize(x);
        let y = cross(z, x);
        (x, y, z)
    }

    /// World to camera.
    pub fn view_matrix(&self) -> M4 {
        let (x, y, z) = self.basis();
        let p = self.position;
        [
            [x[0], y[0], z[0], 0.0],
            [x[1], y[1], z[1], 0.0],
            [x[2], y[2], z[2], 0.0],
            [-dot(x, p), -dot(y, p), -dot(z, p), 1.0],
        ]
    }

    /// Camera to clip space with wgpu's 0..1 depth, reversed (near is 1, far is 0) so the depth
    /// buffer keeps its precision across 0.1 mm to 100 m.
    pub fn projection_matrix(&self) -> M4 {
        let f = 1.0 / (FOV_DEG.to_radians() / 2.0).tan();
        let a = NEAR / (FAR - NEAR);
        let b = FAR * NEAR / (FAR - NEAR);
        [[f / self.aspect(), 0.0, 0.0, 0.0], [0.0, f, 0.0, 0.0], [0.0, 0.0, a, -1.0], [0.0, 0.0, b, 0.0]]
    }

    pub fn view_projection(&self) -> M4 {
        mul(&self.projection_matrix(), &self.view_matrix())
    }

    /// Normalised device coordinates (-1..1, y up) of a world point, and its depth in front of
    /// the camera (negative when behind).
    pub fn project(&self, p: V3) -> ([f64; 2], f64) {
        let m = self.view_projection();
        let c = transform(&m, [p[0], p[1], p[2], 1.0]);
        let w = if c[3].abs() > 1e-300 { c[3] } else { 1e-300 };
        ([c[0] / w, c[1] / w], c[3])
    }

    /// `worldToScreen`: a point in the view, in points from its top left corner.
    pub fn world_to_view(&self, p: V3) -> [f64; 2] {
        let (n, _) = self.project(p);
        [(n[0] + 1.0) / 2.0 * self.width, (1.0 - n[1]) / 2.0 * self.height]
    }

    /// `pixelSize`: world units per screen pixel near a point.
    pub fn pixel_size(&self, at: V3) -> f64 {
        let d = length(sub(self.position, at));
        2.0 * d * (FOV_DEG.to_radians() / 2.0).tan() / self.height.max(1.0)
    }

    /// `Raycaster.setFromCamera`: the ray through a point of the view (points from the top
    /// left): origin at the camera, unit direction.
    pub fn ray(&self, view_pos: [f64; 2]) -> (V3, V3) {
        let nx = view_pos[0] / self.width * 2.0 - 1.0;
        let ny = -(view_pos[1] / self.height) * 2.0 + 1.0;
        let p = self.unproject([nx, ny], 0.5);
        (self.position, normalize(sub(p, self.position)))
    }

    /// A point at normalised device coordinates and a GL-style depth (-1 near, 1 far), the way
    /// three's `Vector3.unproject` reads it.
    pub fn unproject(&self, ndc: [f64; 2], gl_z: f64) -> V3 {
        // three's projection maps view z to GL depth; invert that for a view-space point.
        let f = 1.0 / (FOV_DEG.to_radians() / 2.0).tan();
        let (n, fa) = (NEAR, FAR);
        // GL: z_ndc = (A*z + B) / -z with A = -(f+n)/(f-n), B = -2fn/(f-n)
        let a = -(fa + n) / (fa - n);
        let b = -2.0 * fa * n / (fa - n);
        let zv = -b / (gl_z + a); // negative in front of the camera
        let xv = ndc[0] * (-zv) * self.aspect() / f;
        let yv = ndc[1] * (-zv) / f;
        let (x, y, z) = self.basis();
        add(self.position, add(add(scale(x, xv), scale(y, yv)), scale(z, zv)))
    }

    // ------------------------------------------------------------------ OrbitControls

    /// `_handleMouseMoveRotate`: 2π per view height, both ways ("yes, height").
    pub fn rotate(&mut self, dx: f64, dy: f64) {
        self.fly = None;
        let tau = std::f64::consts::TAU;
        self.orbit(-tau * dx / self.height, -tau * dy / self.height);
    }

    /// `update()`'s spherical step. OrbitControls works in the space where the camera's up at
    /// construction, (0, 0, 1), is +Y, so the orbit is always around the world Z axis.
    fn orbit(&mut self, d_theta: f64, d_phi: f64) {
        let o = sub(self.position, self.target);
        // Z-up world to Y-up: (x, y, z) -> (x, z, -y)
        let (x, y, z) = (o[0], o[2], -o[1]);
        let r = (x * x + y * y + z * z).sqrt();
        let (mut theta, mut phi) = if r == 0.0 { (0.0, 0.0) } else { (x.atan2(z), (y / r).clamp(-1.0, 1.0).acos()) };
        theta += d_theta;
        phi += d_phi;
        phi = phi.clamp(0.0, std::f64::consts::PI);
        phi = phi.clamp(SAFE_EPS, std::f64::consts::PI - SAFE_EPS);
        let s = phi.sin() * r;
        let (x, y, z) = (s * theta.sin(), phi.cos() * r, s * theta.cos());
        // back to Z up: (a, b, c) -> (a, -c, b)
        self.position = add(self.target, [x, -z, y]);
    }

    /// `_pan` for a perspective camera: deltas in points, right and down positive.
    pub fn pan(&mut self, dx: f64, dy: f64) {
        self.fly = None;
        let td = length(sub(self.position, self.target)) * (FOV_DEG / 2.0).to_radians().tan();
        let (x, y, _) = self.basis();
        let left = scale(x, -(2.0 * dx * td / self.height));
        let up = scale(y, 2.0 * dy * td / self.height);
        let off = add(left, up);
        self.target = add(self.target, off);
        self.position = add(self.position, off);
    }

    /// `_handleMouseWheel` with `zoomToCursor`: `delta_y` as a browser's `WheelEvent.deltaY`
    /// (positive zooms out), the cursor in points from the view's top left.
    pub fn wheel(&mut self, delta_y: f64, cursor: [f64; 2]) {
        if delta_y == 0.0 {
            return;
        }
        let k = 0.95f64.powf((delta_y * 0.01).abs());
        let s = if delta_y < 0.0 { k } else { 1.0 / k };
        self.zoom_to_cursor(s, cursor);
    }

    /// Scale the camera's distance by `s` (below 1 zooms in) along the ray under the cursor.
    pub fn zoom_to_cursor(&mut self, s: f64, cursor: [f64; 2]) {
        self.fly = None;
        let nx = cursor[0] / self.width * 2.0 - 1.0;
        let ny = -(cursor[1] / self.height) * 2.0 + 1.0;
        let dolly_dir = normalize(sub(self.unproject([nx, ny], 1.0), self.position));
        let (_, _, z) = self.basis();
        let prev = length(sub(self.position, self.target));
        let new_r = (prev * s).max(0.0);
        self.position = add(self.position, scale(dolly_dir, prev - new_r));
        // screenSpacePanning: the target sits in front of the moved camera
        self.target = add(self.position, scale(z, -new_r));
    }

    // ------------------------------------------------------------------ views

    /// The clock the flights run on (egui's `input.time`, in seconds).
    pub fn set_time(&mut self, now: f64) {
        self.now = now;
    }

    /// `fit(box)`: an empty box is replaced by the scene's box, and an empty scene by
    /// (-50, -50, 0)..(50, 50, 50).
    pub fn fit(&mut self, b: Aabb, scene: Option<Aabb>) {
        let mut b = b;
        if box_is_empty(&b) {
            b = scene.unwrap_or_else(empty_box);
        }
        if box_is_empty(&b) {
            b = Aabb { min: [-50.0, -50.0, 0.0], max: [50.0, 50.0, 50.0] };
        }
        let c = box_center(&b);
        let mut r = box_diagonal(&b) / 2.0;
        if !(r > 0.0) || !r.is_finite() {
            r = 50.0;
        }
        let dir = normalize(sub(self.position, self.target));
        let dist = r / self.half_fov().sin() * 1.05;
        self.fly_to(c, add(c, scale(dir, dist)), self.up);
    }

    /// `setView(name)`: top, bottom, front, back, right, left or iso (`fit` is [`Camera::fit`]).
    pub fn set_view(&mut self, name: &str, scene: Option<Aabb>) -> bool {
        let d: V3 = match name {
            "top" => [0.0, 0.0, 1.0],
            "bottom" => [0.0, 0.0, -1.0],
            "front" => [0.0, -1.0, 0.0],
            "back" => [0.0, 1.0, 0.0],
            "right" => [1.0, 0.0, 0.0],
            "left" => [-1.0, 0.0, 0.0],
            "iso" => [1.0, -1.0, 0.85],
            _ => return false,
        };
        let d = normalize(d);
        let (c, r) = match scene.filter(|b| !box_is_empty(b)) {
            Some(b) => (box_center(&b), box_diagonal(&b) / 2.0),
            None => ([0.0; 3], 60.0),
        };
        let dist = r / self.half_fov().sin() * 1.05;
        let up = if d[2].abs() > 0.99 { [0.0, if d[2] > 0.0 { 1.0 } else { -1.0 }, 0.0] } else { [0.0, 0.0, 1.0] };
        self.fly_to(c, add(c, scale(d, dist)), up);
        true
    }

    /// `lookAtPlane(frame, radius)`: look straight at a plane, its y axis up.
    pub fn look_at_plane(&mut self, origin: V3, y: V3, normal: V3, radius: Option<f64>, scene: Option<Aabb>) {
        let scene = scene.filter(|b| !box_is_empty(b));
        let r = radius.unwrap_or_else(|| scene.map(|b| box_diagonal(&b) / 2.0).unwrap_or(60.0));
        let mut c = origin;
        if let Some(b) = scene {
            let bc = box_center(&b);
            c = sub(bc, scale(normal, dot(normal, sub(bc, origin))));
        }
        let dist = r / self.half_fov().sin();
        self.fly_to(c, add(c, scale(normal, dist)), y);
    }

    fn fly_to(&mut self, target: V3, pos: V3, up: V3) {
        self.fly = Some(Fly { start: self.now, from: (self.target, self.position, self.up), to: (target, pos, up) });
    }

    /// Where the running flight ends: (target, position, up).
    pub fn goal(&self) -> (V3, V3, V3) {
        match &self.fly {
            Some(f) => f.to,
            None => (self.target, self.position, self.up),
        }
    }

    pub fn is_flying(&self) -> bool {
        self.fly.is_some()
    }

    /// Advance a flight to `now`; true while it still runs.
    pub fn animate(&mut self, now: f64) -> bool {
        self.now = now;
        let Some(f) = self.fly else { return false };
        let k = ((now - f.start) / FLY_SECONDS).clamp(0.0, 1.0);
        let e = 1.0 - (1.0 - k).powi(3);
        self.target = lerp(f.from.0, f.to.0, e);
        self.position = lerp(f.from.1, f.to.1, e);
        self.up = normalize(lerp(f.from.2, f.to.2, e));
        if k >= 1.0 {
            self.fly = None;
            self.up = f.to.2;
            false
        } else {
            true
        }
    }

    /// Jump to the end of a running flight.
    pub fn finish_flight(&mut self) {
        if let Some(f) = self.fly.take() {
            self.target = f.to.0;
            self.position = f.to.1;
            self.up = f.to.2;
        }
    }
}

pub fn mul(a: &M4, b: &M4) -> M4 {
    let mut r = [[0.0; 4]; 4];
    for (c, col) in r.iter_mut().enumerate() {
        for (row, v) in col.iter_mut().enumerate() {
            *v = (0..4).map(|k| a[k][row] * b[c][k]).sum();
        }
    }
    r
}

pub fn transform(m: &M4, v: [f64; 4]) -> [f64; 4] {
    let mut r = [0.0; 4];
    for (row, out) in r.iter_mut().enumerate() {
        *out = (0..4).map(|k| m[k][row] * v[k]).sum();
    }
    r
}

pub fn to_f32(m: &M4) -> [[f32; 4]; 4] {
    m.map(|c| c.map(|v| v as f32))
}
