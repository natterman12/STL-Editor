//! The transform handles 1.x shows on a part (`web/js/gizmo.js`: three r170's
//! `TransformControls` in world space, size 1.1): arrows and plane squares to move, half rings
//! facing the camera to turn, boxes to stretch. Their shapes, colours and sizes are three's;
//! the invisible "picker" shapes decide which handle the pointer is on, as three's do.
//!
//! The handles keep their size on screen: one handle unit is
//! `distance to the camera * min(1.9 * tan(fov / 2), 7) * 1.1 / 4`.

use super::camera::{self, Camera, V3};
use super::overlays::Depth;
use super::renderer::FlatBatch;
use super::scene::{rgba, FlatVertex};

/// `tc.setSize(1.1)`.
pub const GIZMO_SIZE: f64 = 1.1;

const RED: u32 = 0xff0000;
const GREEN: u32 = 0x00ff00;
const BLUE: u32 = 0x0000ff;
const YELLOW: u32 = 0xffff00;
const GRAY: u32 = 0x787878;
const WHITE: u32 = 0xffffff;
/// `lineGeometry2`'s radius: the handles' axis lines and rings are 0.015 units thick.
const LINE_W: f64 = 0.015;

/// A handle, by three's name for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Handle {
    X,
    Y,
    Z,
    XY,
    YZ,
    XZ,
    XYZ,
    /// Rotate: the outer ring, turning about the line of sight.
    E,
    /// Rotate: the inner grey ring (and the ball inside it), turning freely.
    XYZE,
}

impl Handle {
    pub fn name(self) -> &'static str {
        match self {
            Handle::X => "X",
            Handle::Y => "Y",
            Handle::Z => "Z",
            Handle::XY => "XY",
            Handle::YZ => "YZ",
            Handle::XZ => "XZ",
            Handle::XYZ => "XYZ",
            Handle::E => "E",
            Handle::XYZE => "XYZE",
        }
    }

    /// `axis.indexOf(c) !== -1`.
    pub fn has(self, c: char) -> bool {
        self.name().contains(c)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GizmoMode {
    Translate,
    Rotate,
    Scale,
}

/// What the viewport draws for the gizmo this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct GizmoShape {
    pub mode: GizmoMode,
    /// The handles' centre (three's `worldPosition`).
    pub position: V3,
    /// The handle under the pointer, or the one being dragged (three's `axis`): drawn yellow.
    pub axis: Option<Handle>,
    /// False while Support Fins is still putting the last turn on the part: no highlight.
    pub enabled: bool,
    pub dragging: bool,
    /// Where a drag started (`worldPositionStart`), for the helper lines.
    pub start: V3,
    /// A free turn's axis (`rotationAxis`), for its helper line.
    pub rotation_axis: V3,
}

impl GizmoShape {
    pub fn new(mode: GizmoMode, position: V3) -> Self {
        GizmoShape { mode, position, axis: None, enabled: true, dragging: false, start: position, rotation_axis: [0.0, 0.0, 1.0] }
    }
}

const UNIT: [V3; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// One handle unit in millimetres at `pos` (three's `factor * size / 4`).
pub fn handle_unit(cam: &Camera, pos: V3) -> f64 {
    let half = (camera::FOV_DEG / 2.0).to_radians();
    let factor = camera::length(camera::sub(pos, cam.position)) * (1.9 * half.tan()).min(7.0);
    factor * GIZMO_SIZE / 4.0
}

/// `eye`: from the handles towards the camera.
pub fn eye(cam: &Camera, pos: V3) -> V3 {
    camera::normalize(camera::sub(cam.position, pos))
}

fn axis_index(h: Handle) -> Option<usize> {
    match h {
        Handle::X => Some(0),
        Handle::Y => Some(1),
        Handle::Z => Some(2),
        _ => None,
    }
}

/// The handles shown in a mode: an arrow or box pointing at the camera (within 0.99) and a
/// plane square seen edge on (under 0.2) are hidden, as three hides them.
pub fn visible_handles(mode: GizmoMode, eye: V3) -> Vec<Handle> {
    match mode {
        GizmoMode::Rotate => vec![Handle::XYZE, Handle::X, Handle::Y, Handle::Z, Handle::E],
        GizmoMode::Translate | GizmoMode::Scale => {
            let mut v = Vec::new();
            for (h, i) in [(Handle::X, 0), (Handle::Y, 1), (Handle::Z, 2)] {
                if camera::dot(UNIT[i], eye).abs() <= 0.99 {
                    v.push(h);
                }
            }
            v.push(Handle::XYZ);
            for (h, n) in [(Handle::XY, 2), (Handle::YZ, 0), (Handle::XZ, 1)] {
                if camera::dot(UNIT[n], eye).abs() >= 0.2 {
                    v.push(h);
                }
            }
            v
        }
    }
}

// ---------------------------------------------------------------------------- picking

/// Inside a picker shape, in handle units around the centre.
fn inside(mode: GizmoMode, h: Handle, q: V3, _eye: V3) -> bool {
    match (mode, h) {
        (GizmoMode::Rotate, Handle::XYZE) => camera::length(q) <= 0.25,
        (GizmoMode::Rotate, Handle::X | Handle::Y | Handle::Z) => {
            let n = UNIT[axis_index(h).unwrap_or(0)];
            in_torus(q, n, 0.5, 0.1)
        }
        (GizmoMode::Rotate, Handle::E) => false, // flat: tested on its plane
        (_, Handle::X | Handle::Y | Handle::Z) => {
            // CylinderGeometry(0.2, 0, 0.6, 4) both ways: a point at the centre, 0.2 across at 0.6
            let a = UNIT[axis_index(h).unwrap_or(0)];
            let along = camera::dot(q, a).abs();
            let perp = camera::length(camera::sub(q, camera::scale(a, camera::dot(q, a))));
            along <= 0.6 && perp <= 0.2 * along / 0.6
        }
        (GizmoMode::Translate, Handle::XYZ) => q[0].abs() + q[1].abs() + q[2].abs() <= 0.2,
        _ => false,
    }
}

fn in_torus(q: V3, n: V3, r: f64, tube: f64) -> bool {
    let h = camera::dot(q, n);
    let perp = camera::length(camera::sub(q, camera::scale(n, h)));
    ((perp - r).powi(2) + h * h).sqrt() <= tube
}

/// The boxes among the pickers: (centre, half sizes), in handle units.
fn picker_box(mode: GizmoMode, h: Handle) -> Option<(V3, V3)> {
    match h {
        Handle::XY => Some(([0.15, 0.15, 0.0], [0.1, 0.1, 0.005])),
        Handle::YZ => Some(([0.0, 0.15, 0.15], [0.005, 0.1, 0.1])),
        Handle::XZ => Some(([0.15, 0.0, 0.15], [0.1, 0.005, 0.1])),
        Handle::XYZ if mode == GizmoMode::Scale => Some(([0.0; 3], [0.1, 0.1, 0.1])),
        _ => None,
    }
}

/// Slab test: where the ray enters the box (`t >= 0`).
fn ray_box(o: V3, d: V3, c: V3, half: V3) -> Option<f64> {
    let (mut t0, mut t1) = (0.0f64, f64::INFINITY);
    for k in 0..3 {
        let (lo, hi) = (c[k] - half[k], c[k] + half[k]);
        if d[k].abs() < 1e-300 {
            if o[k] < lo || o[k] > hi {
                return None;
            }
            continue;
        }
        let (mut a, mut b) = ((lo - o[k]) / d[k], (hi - o[k]) / d[k]);
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        t0 = t0.max(a);
        t1 = t1.min(b);
        if t0 > t1 {
            return None;
        }
    }
    Some(t0)
}

/// The handle under a pick ray (`origin`, unit `dir`), the nearest one, as three's
/// `intersectObjectWithRay(picker)`.
pub fn pick(shape: &GizmoShape, cam: &Camera, origin: V3, dir: V3) -> Option<Handle> {
    let u = handle_unit(cam, shape.position);
    if !(u > 0.0) {
        return None;
    }
    let e = eye(cam, shape.position);
    // the ray in handle units, centred on the handles
    let o = camera::scale(camera::sub(origin, shape.position), 1.0 / u);
    let tc = -camera::dot(o, dir);
    let mut best: Option<(f64, Handle)> = None;
    let mut keep = |t: f64, h: Handle| {
        if t >= 0.0 && best.is_none_or(|(bt, _)| t < bt) {
            best = Some((t, h));
        }
    };
    for h in visible_handles(shape.mode, e) {
        if let Some((c, half)) = picker_box(shape.mode, h) {
            if let Some(t) = ray_box(o, dir, c, half) {
                keep(t, h);
            }
            continue;
        }
        if shape.mode == GizmoMode::Rotate && h == Handle::E {
            // TorusGeometry(0.75, 0.1, 2, 24) facing the camera: a flat band 0.65..0.85
            let den = camera::dot(dir, e);
            if den.abs() > 1e-12 {
                let t = -camera::dot(o, e) / den;
                let r = camera::length(camera::add(o, camera::scale(dir, t)));
                if (0.65..=0.85).contains(&r) {
                    keep(t, h);
                }
            }
            continue;
        }
        // march through the handles' neighbourhood, then refine the entry point
        const STEPS: usize = 160;
        let (a, b) = (tc - 1.2, tc + 1.2);
        let at = |t: f64| camera::add(o, camera::scale(dir, t));
        let mut prev = a;
        for i in 0..=STEPS {
            let t = a + (b - a) * i as f64 / STEPS as f64;
            if inside(shape.mode, h, at(t), e) {
                let (mut lo, mut hi) = (prev, t);
                if i > 0 {
                    for _ in 0..20 {
                        let m = (lo + hi) / 2.0;
                        if inside(shape.mode, h, at(m), e) {
                            hi = m;
                        } else {
                            lo = m;
                        }
                    }
                }
                keep(hi, h);
                break;
            }
            prev = t;
        }
    }
    best.map(|(_, h)| h)
}

// ---------------------------------------------------------------------------- drawing

struct Out {
    tris: Vec<FlatVertex>,
    lines: Vec<FlatVertex>,
    p: V3,
    u: f64,
    cam: V3,
}

impl Out {
    fn w(&self, q: V3) -> V3 {
        camera::add(self.p, camera::scale(q, self.u))
    }
    fn tri(&mut self, a: V3, b: V3, c: V3, col: [u8; 4]) {
        let (a, b, c) = (self.w(a), self.w(b), self.w(c));
        self.tris.extend([a, b, c].map(|q| FlatVertex::new(q, col)));
    }
    fn quad(&mut self, a: V3, b: V3, c: V3, d: V3, col: [u8; 4]) {
        self.tri(a, b, c, col);
        self.tri(a, c, d, col);
    }
    /// A thick line (three draws them as thin cylinders), as a band facing the camera.
    fn band(&mut self, a: V3, b: V3, col: [u8; 4]) {
        let (wa, wb) = (self.w(a), self.w(b));
        let dir = camera::sub(wb, wa);
        let side = camera::cross(dir, camera::sub(self.cam, wa));
        if camera::length(side) == 0.0 {
            return;
        }
        let s = camera::scale(camera::normalize(side), LINE_W * self.u / 2.0);
        let (p0, p1, p2, p3) = (camera::sub(wa, s), camera::add(wa, s), camera::add(wb, s), camera::sub(wb, s));
        self.tris.extend([p0, p1, p2, p0, p2, p3].map(|q| FlatVertex::new(q, col)));
    }
    fn line_world(&mut self, a: V3, b: V3, col: [u8; 4]) {
        self.lines.push(FlatVertex::new(a, col));
        self.lines.push(FlatVertex::new(b, col));
    }
    /// A cone along unit `a`: base radius `r` at `from`, tip at `to` (distances along `a`).
    fn cone(&mut self, a: V3, from: f64, to: f64, r: f64, col: [u8; 4]) {
        let (x, y) = perp(a);
        let base = camera::scale(a, from);
        let tip = camera::scale(a, to);
        for i in 0..12 {
            let (a0, a1) = (i as f64 / 12.0 * std::f64::consts::TAU, (i + 1) as f64 / 12.0 * std::f64::consts::TAU);
            let rim = |t: f64| camera::add(base, camera::add(camera::scale(x, r * t.cos()), camera::scale(y, r * t.sin())));
            let (q0, q1) = (rim(a0), rim(a1));
            self.tri(q0, q1, tip, col);
            self.tri(q1, q0, base, col);
        }
    }
    fn cube(&mut self, c: V3, half: V3, col: [u8; 4]) {
        let v = |sx: f64, sy: f64, sz: f64| [c[0] + sx * half[0], c[1] + sy * half[1], c[2] + sz * half[2]];
        let faces = [
            [v(-1., -1., -1.), v(1., -1., -1.), v(1., 1., -1.), v(-1., 1., -1.)],
            [v(-1., -1., 1.), v(1., -1., 1.), v(1., 1., 1.), v(-1., 1., 1.)],
            [v(-1., -1., -1.), v(1., -1., -1.), v(1., -1., 1.), v(-1., -1., 1.)],
            [v(-1., 1., -1.), v(1., 1., -1.), v(1., 1., 1.), v(-1., 1., 1.)],
            [v(-1., -1., -1.), v(-1., 1., -1.), v(-1., 1., 1.), v(-1., -1., 1.)],
            [v(1., -1., -1.), v(1., 1., -1.), v(1., 1., 1.), v(1., -1., 1.)],
        ];
        for [a, b, cc, d] in faces {
            self.quad(a, b, cc, d, col);
        }
    }
    fn octahedron(&mut self, r: f64, col: [u8; 4]) {
        for sx in [-1.0, 1.0] {
            for sy in [-1.0, 1.0] {
                for sz in [-1.0, 1.0] {
                    self.tri([sx * r, 0.0, 0.0], [0.0, sy * r, 0.0], [0.0, 0.0, sz * r], col);
                }
            }
        }
    }
    /// A ring of radius `r` from `at(0)` to `at(arc)`, `at` giving unit points.
    fn ring(&mut self, r: f64, arc: f64, at: impl Fn(f64) -> V3, col: [u8; 4]) {
        let n = (64.0 * arc).round().max(1.0) as usize;
        for i in 0..n {
            let (a0, a1) = (arc * std::f64::consts::TAU * i as f64 / n as f64, arc * std::f64::consts::TAU * (i + 1) as f64 / n as f64);
            self.band(camera::scale(at(a0), r), camera::scale(at(a1), r), col);
        }
    }
}

fn perp(n: V3) -> (V3, V3) {
    let t = if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let x = camera::normalize(camera::cross(n, t));
    (x, camera::cross(n, x))
}

/// The handles' colour: their own, or yellow at full strength when the pointer is on them
/// (or on a plane square or the centre that includes their axis).
fn colour(shape: &GizmoShape, h: Handle, base: u32, alpha: f32) -> [u8; 4] {
    if let (true, Some(ax)) = (shape.enabled, shape.axis) {
        if ax == h || ax.name().chars().any(|c| h.name().len() == 1 && h.name().starts_with(c)) {
            return rgba(YELLOW, 1.0);
        }
    }
    rgba(base, alpha)
}

/// The handles' triangles and lines (world positions; draw with depth test off), and their
/// batches within the returned vertex list.
pub fn build(shape: &GizmoShape, cam: &Camera) -> (Vec<FlatVertex>, Vec<FlatBatch>) {
    let u = handle_unit(cam, shape.position);
    let e = eye(cam, shape.position);
    let mut o = Out { tris: Vec::new(), lines: Vec::new(), p: shape.position, u, cam: cam.position };
    let axis_col = [RED, GREEN, BLUE];
    let shown = visible_handles(shape.mode, e);
    match shape.mode {
        GizmoMode::Translate | GizmoMode::Scale => {
            for (i, h) in [Handle::X, Handle::Y, Handle::Z].into_iter().enumerate() {
                if !shown.contains(&h) {
                    continue;
                }
                let c = colour(shape, h, axis_col[i], 1.0);
                let a = UNIT[i];
                let na = camera::scale(a, -1.0);
                o.band([0.0; 3], camera::scale(a, 0.5), c);
                if shape.mode == GizmoMode::Translate {
                    // CylinderGeometry(0, 0.04, 0.1, 12) at ±0.5: the tip at 0.6
                    o.cone(a, 0.5, 0.6, 0.04, c);
                    o.cone(na, 0.5, 0.6, 0.04, c);
                } else {
                    // BoxGeometry(0.08) from 0.5 to 0.58
                    let h4 = [0.04; 3];
                    o.cube(camera::scale(a, 0.54), h4, c);
                    o.cube(camera::scale(a, -0.54), h4, c);
                }
            }
            for (h, centre, half, base) in [
                (Handle::XY, [0.15, 0.15, 0.0], [0.075, 0.075, 0.0], BLUE),
                (Handle::YZ, [0.0, 0.15, 0.15], [0.0, 0.075, 0.075], RED),
                (Handle::XZ, [0.15, 0.0, 0.15], [0.075, 0.0, 0.075], GREEN),
            ] {
                if !shown.contains(&h) {
                    continue;
                }
                let c = colour(shape, h, base, 0.5);
                // the two axes the square lies along
                let ax: Vec<usize> = (0..3).filter(|&k| half[k] > 0.0).collect();
                let corner = |s0: f64, s1: f64| {
                    let mut q = centre;
                    q[ax[0]] += s0 * half[ax[0]];
                    q[ax[1]] += s1 * half[ax[1]];
                    q
                };
                o.quad(corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0), c);
            }
            let c = colour(shape, Handle::XYZ, WHITE, 0.25);
            if shape.mode == GizmoMode::Translate {
                o.octahedron(0.1, c);
            } else {
                o.cube([0.0; 3], [0.05; 3], c);
            }
        }
        GizmoMode::Rotate => {
            let (ex, ey, ez) = (e[0], e[1], e[2]);
            let (fx, fy) = perp(e);
            let facing = move |a: f64| camera::add(camera::scale(fx, a.cos()), camera::scale(fy, a.sin()));
            o.ring(0.5, 1.0, facing, colour(shape, Handle::XYZE, GRAY, 1.0));
            let phi = (-ey).atan2(ez);
            o.ring(0.5, 0.5, move |a: f64| [0.0, a.cos() * phi.cos() - a.sin() * phi.sin(), a.cos() * phi.sin() + a.sin() * phi.cos()], colour(shape, Handle::X, RED, 1.0));
            let psi = ex.atan2(ez);
            o.ring(0.5, 0.5, move |a: f64| [a.cos() * psi.cos() + a.sin() * psi.sin(), 0.0, -a.cos() * psi.sin() + a.sin() * psi.cos()], colour(shape, Handle::Y, GREEN, 1.0));
            let chi = ey.atan2(ex);
            o.ring(0.5, 0.5, move |a: f64| [a.sin() * chi.cos() - a.cos() * chi.sin(), a.sin() * chi.sin() + a.cos() * chi.cos(), 0.0], colour(shape, Handle::Z, BLUE, 1.0));
            o.ring(0.75, 1.0, facing, colour(shape, Handle::E, YELLOW, 0.25));
        }
    }
    // helper lines (white at half strength): the axis being hovered or dragged
    let helper = rgba(WHITE, 0.5);
    let far = 1e3 * u;
    if let Some(ax) = shape.axis {
        let at = if shape.dragging { shape.start } else { shape.position };
        match shape.mode {
            GizmoMode::Translate | GizmoMode::Scale => {
                for (i, c) in ['X', 'Y', 'Z'].into_iter().enumerate() {
                    if ax != Handle::E && ax != Handle::XYZE && ax.has(c) {
                        o.line_world(camera::add(at, camera::scale(UNIT[i], -far)), camera::add(at, camera::scale(UNIT[i], far)), helper);
                    }
                }
                if shape.mode == GizmoMode::Translate && shape.dragging {
                    o.line_world(shape.start, shape.position, helper);
                }
            }
            GizmoMode::Rotate => {
                let dir = match ax {
                    Handle::X | Handle::Y | Handle::Z => {
                        let a = UNIT[axis_index(ax).unwrap_or(0)];
                        (camera::dot(a, e).abs() <= 0.9).then_some(a)
                    }
                    Handle::XYZE if shape.dragging => Some(shape.rotation_axis),
                    _ => None,
                };
                if let Some(a) = dir {
                    let p = shape.position;
                    o.line_world(camera::add(p, camera::scale(a, -far)), camera::add(p, camera::scale(a, far)), helper);
                }
            }
        }
    }
    let mut verts = o.tris;
    let mut batches = Vec::new();
    if !verts.is_empty() {
        batches.push(FlatBatch { order: i32::MAX, lines: false, depth: Depth::Off, range: 0..verts.len() as u32 });
    }
    if !o.lines.is_empty() {
        let start = verts.len() as u32;
        verts.extend(o.lines);
        batches.push(FlatBatch { order: i32::MAX, lines: true, depth: Depth::Off, range: start..verts.len() as u32 });
    }
    (verts, batches)
}
