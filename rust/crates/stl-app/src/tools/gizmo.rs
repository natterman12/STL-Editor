//! The on-part handles of Move / Rotate, Scale and Support Fins' Rotate mode (1.x
//! `web/js/gizmo.js`, on three r170's `TransformControls` in world space with 5° rotation
//! steps). Dragging moves the part on screen at once (a per-body transform in the viewport,
//! no new mesh); letting go hands back the tool's new numbers, and the preview follows.
//!
//! How the app drives it, each frame while a gizmo tool is open:
//!
//! 1. `let r = viewport.show(ui);` then
//!    `gizmo.update(&mut viewport, ui, &r.response, &resolved_values)` with the tool's values,
//!    expressions already resolved (1.x `app.resolved`).
//! 2. On [`GizmoEvent::Started`]: drop the pending preview (bump the preview seq, stop the
//!    280 ms timer), as 1.x's `start()` and `clearPreviewTimer` do. The gizmo has already
//!    cleared the preview on screen.
//! 3. On [`GizmoEvent::Moved`]: show the numbers in the panel (`liveNumbers`).
//! 4. On [`GizmoEvent::Dropped`]: [`GizmoDrop::Values`] goes into the tool's values (then
//!    `sync` again and, with `auto_accept`, 1.x's `autoAccept`: add or edit the step at once);
//!    [`GizmoDrop::Pose`] goes to Support Fins' `poseDrop`.
//! 5. When a preview lands: show it only if [`Gizmo::accept_preview`] says so (false while a
//!    handle is held: a late preview never snaps the part back).
//! 6. A click in the viewport that [`Gizmo::consume_click`] claims is not a click on the part.
//!
//! Rotations are degrees about X, then Y, then Z (`R = Rz·Ry·Rx`, three's Euler order 'ZYX'):
//! [`quat_of`] and [`rot_of`] are ports of 1.x's `quatOf` and `rotOf`.

use std::sync::Arc;

use egui::Pos2;
use serde_json::{json, Map, Value};
use stl_kernel::{Aabb, MeshArrays};

use crate::viewport::camera::{self, M4, V3};
use crate::viewport::gizmo_draw::{self, GizmoMode, GizmoShape, Handle};
use crate::viewport::Viewport;

pub type Values = Map<String, Value>;
/// A quaternion, three's order: x, y, z, w.
pub type Quat = [f64; 4];

const DEG: f64 = std::f64::consts::PI / 180.0;
/// `tc.setRotationSnap(5 * DEG)`.
pub const ROTATION_SNAP: f64 = 5.0 * DEG;

// ---------------------------------------------------------------------------- JS numbers

/// `Math.round`: halves go up (towards +infinity).
pub fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// JS `Number(v)`.
pub fn js_number(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        Value::Bool(b) => *b as i32 as f64,
        Value::Null => 0.0,
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                0.0
            } else {
                t.parse::<f64>().unwrap_or(f64::NAN)
            }
        }
        _ => f64::NAN,
    }
}

/// `+x || d`: NaN and 0 give `d`.
fn or(x: f64, d: f64) -> f64 {
    if x.is_nan() || x == 0.0 { d } else { x }
}

/// A number as JS writes it into JSON: whole numbers without a fraction, `-0` as `0`.
pub fn jnum(n: f64) -> Value {
    if n.is_finite() && n == n.trunc() && n.abs() < 9.0e15 {
        json!(n as i64)
    } else {
        serde_json::Number::from_f64(n).map(Value::Number).unwrap_or(Value::Null)
    }
}

fn jvec(v: [f64; 3]) -> Value {
    Value::Array(v.iter().map(|&x| jnum(x)).collect())
}

fn vec3_of(v: Option<&Value>, d: [f64; 3]) -> Option<[f64; 3]> {
    match v {
        Some(Value::Array(a)) => Some([0, 1, 2].map(|i| a.get(i).map(js_number).unwrap_or(f64::NAN))),
        Some(Value::Null) | None => Some(d).filter(|_| true),
        _ => None,
    }
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

fn str_or<'a>(v: Option<&'a Value>, d: &'a str) -> &'a str {
    match v {
        Some(Value::String(s)) if !s.is_empty() => s,
        _ => d,
    }
}

// ---------------------------------------------------------------------------- quaternions

/// three's `Quaternion.setFromEuler(new Euler(x, y, z, 'ZYX'))` (radians).
pub fn quat_from_euler_zyx(x: f64, y: f64, z: f64) -> Quat {
    let (c1, c2, c3) = ((x / 2.0).cos(), (y / 2.0).cos(), (z / 2.0).cos());
    let (s1, s2, s3) = ((x / 2.0).sin(), (y / 2.0).sin(), (z / 2.0).sin());
    [
        s1 * c2 * c3 - c1 * s2 * s3,
        c1 * s2 * c3 + s1 * c2 * s3,
        c1 * c2 * s3 - s1 * s2 * c3,
        c1 * c2 * c3 + s1 * s2 * s3,
    ]
}

/// 1.x `quatOf(rot)`: degrees about X, then Y, then Z.
pub fn quat_of(rot: [f64; 3]) -> Quat {
    quat_from_euler_zyx(or(rot[0], 0.0) * DEG, or(rot[1], 0.0) * DEG, or(rot[2], 0.0) * DEG)
}

/// 1.x `quatOf` on a tool value (`rot[i]` through JS `+x || 0`).
pub fn quat_of_value(v: Option<&Value>) -> Quat {
    quat_of(vec3_of(v, [0.0; 3]).unwrap_or([0.0; 3]))
}

/// three's `Matrix4.makeRotationFromQuaternion`, column-major.
pub fn mat_of_quat(q: Quat) -> M4 {
    let [x, y, z, w] = q;
    let (x2, y2, z2) = (x + x, y + y, z + z);
    let (xx, xy, xz) = (x * x2, x * y2, x * z2);
    let (yy, yz, zz) = (y * y2, y * z2, z * z2);
    let (wx, wy, wz) = (w * x2, w * y2, w * z2);
    [
        [1.0 - (yy + zz), xy + wz, xz - wy, 0.0],
        [xy - wz, 1.0 - (xx + zz), yz + wx, 0.0],
        [xz + wy, yz - wx, 1.0 - (xx + yy), 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// three's `Euler.setFromRotationMatrix(m, 'ZYX')`: (x, y, z) radians.
pub fn euler_zyx_of_mat(m: &M4) -> [f64; 3] {
    let (m11, m12, m21, m22) = (m[0][0], m[1][0], m[0][1], m[1][1]);
    let (m31, m32, m33) = (m[0][2], m[1][2], m[2][2]);
    let y = (-m31.clamp(-1.0, 1.0)).asin();
    if m31.abs() < 0.9999999 { [m32.atan2(m33), y, m21.atan2(m11)] } else { [0.0, y, (-m12).atan2(m22)] }
}

/// 1.x `rotOf(q)`: degrees about X, Y, Z, to 0.001.
pub fn rot_of(q: Quat) -> [f64; 3] {
    let e = euler_zyx_of_mat(&mat_of_quat(q));
    e.map(|v| {
        let d = js_round(v / DEG * 1000.0) / 1000.0;
        if d.abs() < 1e-9 { 0.0 } else { d }
    })
}

/// three's `Quaternion.setFromRotationMatrix` (the 3x3 part of a column-major matrix).
pub fn quat_of_mat(m: &M4) -> Quat {
    let (m11, m12, m13) = (m[0][0], m[1][0], m[2][0]);
    let (m21, m22, m23) = (m[0][1], m[1][1], m[2][1]);
    let (m31, m32, m33) = (m[0][2], m[1][2], m[2][2]);
    let trace = m11 + m22 + m33;
    if trace > 0.0 {
        let s = 0.5 / (trace + 1.0).sqrt();
        [(m32 - m23) * s, (m13 - m31) * s, (m21 - m12) * s, 0.25 / s]
    } else if m11 > m22 && m11 > m33 {
        let s = 2.0 * (1.0 + m11 - m22 - m33).sqrt();
        [0.25 * s, (m12 + m21) / s, (m13 + m31) / s, (m32 - m23) / s]
    } else if m22 > m33 {
        let s = 2.0 * (1.0 + m22 - m11 - m33).sqrt();
        [(m12 + m21) / s, 0.25 * s, (m23 + m32) / s, (m13 - m31) / s]
    } else {
        let s = 2.0 * (1.0 + m33 - m11 - m22).sqrt();
        [(m13 + m31) / s, (m23 + m32) / s, 0.25 * s, (m21 - m12) / s]
    }
}

/// `a * b` (three's `multiplyQuaternions`).
pub fn quat_mul(a: Quat, b: Quat) -> Quat {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        ax * bw + aw * bx + ay * bz - az * by,
        ay * bw + aw * by + az * bx - ax * bz,
        az * bw + aw * bz + ax * by - ay * bx,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

pub fn quat_normalize(q: Quat) -> Quat {
    let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if l == 0.0 {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        let k = 1.0 / l;
        q.map(|c| c * k)
    }
}

pub fn quat_axis_angle(axis: V3, angle: f64) -> Quat {
    let s = (angle / 2.0).sin();
    [axis[0] * s, axis[1] * s, axis[2] * s, (angle / 2.0).cos()]
}

/// three's `Vector3.normalize`: a zero vector stays zero.
fn v_normalize(v: V3) -> V3 {
    let l = camera::length(v);
    camera::scale(v, 1.0 / if l == 0.0 { 1.0 } else { l })
}

/// `Vector3.angleTo`.
fn angle_to(a: V3, b: V3) -> f64 {
    let den = (camera::dot(a, a) * camera::dot(b, b)).sqrt();
    if den == 0.0 {
        return std::f64::consts::FRAC_PI_2;
    }
    (camera::dot(a, b) / den).clamp(-1.0, 1.0).acos()
}

// ---------------------------------------------------------------------------- matrices

fn translation(t: V3) -> M4 {
    [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [t[0], t[1], t[2], 1.0]]
}

fn scaling(k: V3) -> M4 {
    [[k[0], 0.0, 0.0, 0.0], [0.0, k[1], 0.0, 0.0], [0.0, 0.0, k[2], 0.0], [0.0, 0.0, 0.0, 1.0]]
}

/// A point through a column-major matrix.
pub fn apply(m: &M4, p: V3) -> V3 {
    let v = camera::transform(m, [p[0], p[1], p[2], 1.0]);
    [v[0], v[1], v[2]]
}

// ---------------------------------------------------------------------------- the gizmo

/// Which tool the handles belong to (`def.gizmo` in tools.js / app.js).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GizmoKind {
    /// Move / Rotate: arrows or rings by `_gizmo`.
    Move,
    /// Scale: boxes.
    Scale,
    /// Support Fins' Rotate mode: rings; a turn is put on the part at once ([`GizmoDrop::Pose`]).
    Pose,
}

/// A body the handles act on, as the viewport shows it.
#[derive(Clone)]
pub struct GizmoBody {
    pub id: String,
    pub bbox: Aabb,
    pub mesh: Arc<MeshArrays>,
}

/// What a frame of input did.
#[derive(Clone, Debug, PartialEq)]
pub enum GizmoEvent {
    None,
    /// A handle was grabbed: drop the pending preview (seq and timer). The preview on screen
    /// is already cleared.
    Started,
    /// The handle moved: the numbers the handles stand for now (for the panel).
    Moved(Values),
    Dropped(GizmoDrop),
}

/// What letting go hands back.
#[derive(Clone, Debug, PartialEq)]
pub enum GizmoDrop {
    /// Merge into the tool's values, re-render the form, `sync`, then (with `auto_accept`)
    /// add or edit the step at once, as 1.x's `autoAccept`.
    Values { patch: Values, auto_accept: bool },
    /// Support Fins: put this turn (column-major) on the part (`poseDrop(m)`).
    Pose(M4),
    Nothing,
}

#[derive(Clone, Copy, Debug)]
struct Obj {
    pos: V3,
    quat: Quat,
    scale: V3,
}

#[derive(Clone, Copy, Debug)]
struct Drag {
    handle: Handle,
    pos_start: V3,
    quat_start: Quat,
    scale_start: V3,
    point_start: V3,
    rotation_axis: V3,
}

pub struct Gizmo {
    kind: GizmoKind,
    obj: Obj,
    shown: bool,
    hover: Option<Handle>,
    drag: Option<Drag>,
    grabbed: bool,
    enabled: bool,
    land: f64,
    bodies: Vec<GizmoBody>,
    /// The tool's values as last synced (resolved).
    values: Values,
    moved: Vec<String>,
    start_q: Quat,
    sync_key: Option<String>,
}

impl Gizmo {
    pub fn new(kind: GizmoKind) -> Self {
        Gizmo {
            kind,
            obj: Obj { pos: [0.0; 3], quat: [0.0, 0.0, 0.0, 1.0], scale: [1.0; 3] },
            shown: false,
            hover: None,
            drag: None,
            grabbed: false,
            enabled: true,
            land: 0.0,
            bodies: Vec::new(),
            values: Values::new(),
            moved: Vec::new(),
            start_q: [0.0, 0.0, 0.0, 1.0],
            sync_key: None,
        }
    }

    pub fn kind(&self) -> GizmoKind {
        self.kind
    }

    /// A handle is held (three's `tc.dragging`).
    pub fn drag_active(&self) -> bool {
        self.drag.is_some()
    }

    /// The handles are on screen.
    pub fn shown(&self) -> bool {
        self.shown
    }

    /// Support Fins: no new drag while the last turn is still being put on the part
    /// (`tc.enabled = !t._posing`).
    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
    }

    /// `consumeClick`: a click that landed on a handle isn't a click on the part.
    pub fn consume_click(&mut self) -> bool {
        std::mem::take(&mut self.grabbed)
    }

    /// The handle under the pointer (or held).
    pub fn axis(&self) -> Option<Handle> {
        self.drag.map(|d| d.handle).or(self.hover)
    }

    /// `mode()`: the handles a tool's values show.
    pub fn mode(&self, values: &Values) -> GizmoMode {
        match self.kind {
            GizmoKind::Scale => GizmoMode::Scale,
            GizmoKind::Pose => GizmoMode::Rotate,
            GizmoKind::Move if values.get("_gizmo").and_then(Value::as_str) == Some("rotate") => GizmoMode::Rotate,
            GizmoKind::Move => GizmoMode::Translate,
        }
    }

    /// The bodies the tool acts on: Support Fins' `body`, the others' `bodies`.
    fn body_ids(&self, values: &Values) -> Vec<String> {
        let v = if self.kind == GizmoKind::Pose { values.get("body") } else { values.get("bodies") };
        match v {
            Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect(),
            Some(Value::String(s)) if !s.is_empty() => vec![s.clone()],
            _ => Vec::new(),
        }
    }

    /// `values()`: the tool's numbers, with Scale's as three factors (`_k`).
    pub fn values_for(&self, values: &Values, first: Option<&GizmoBody>) -> Values {
        if self.kind == GizmoKind::Pose {
            let mut v = Values::new();
            v.insert("translate".into(), json!([0, 0, 0]));
            v.insert("rotate".into(), json!([0, 0, 0]));
            v.insert("pivot".into(), json!("center"));
            v.insert("bed".into(), json!(true));
            return v;
        }
        let mut v = values.clone();
        if self.kind == GizmoKind::Scale {
            let size = first.map(|b| [0, 1, 2].map(|i| b.bbox.max[i] - b.bbox.min[i])).unwrap_or([1.0; 3]);
            let mode = v.get("mode").and_then(Value::as_str).unwrap_or("");
            let uniform = truthy(v.get("uniform"));
            let mut k = [1.0; 3];
            if mode == "factor" {
                k = if uniform {
                    let f = or(v.get("factor").map(js_number).unwrap_or(f64::NAN), 1.0);
                    [f; 3]
                } else {
                    vec3_of(v.get("factor3"), [1.0; 3]).unwrap_or([1.0; 3]).map(|x| or(x, 1.0))
                };
            } else if mode == "fit" {
                let target = match v.get("size") {
                    Some(Value::Array(_)) => vec3_of(v.get("size"), size).unwrap_or(size),
                    _ => size,
                };
                k = [0, 1, 2].map(|i| if size[i] > 1e-9 { target[i] / size[i] } else { 1.0 });
                if uniform {
                    let i = k.iter().position(|x| (x - 1.0).abs() > 1e-9);
                    let pick = k[i.unwrap_or(0)];
                    k = [pick; 3];
                }
            } else if mode == "units" {
                let f = match v.get("units").and_then(Value::as_str) {
                    Some("in_to_mm") => 25.4,
                    Some("mm_to_in") => 1.0 / 25.4,
                    Some("cm_to_mm") => 10.0,
                    Some("m_to_mm") => 1000.0,
                    _ => 1.0,
                };
                k = [f; 3];
            }
            v.insert("_k".into(), Value::Array(k.iter().map(|&x| json!(x)).collect()));
        }
        v
    }

    /// `pivot(b, kind)`: the point a body turns or scales about, as the engine takes it.
    pub fn pivot(b: &Aabb, kind: &str) -> V3 {
        let (lo, hi) = (b.min, b.max);
        match kind {
            "origin" => [0.0; 3],
            "base" => [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, lo[2]],
            _ => [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0],
        }
    }

    fn k_of(vals: &Values) -> V3 {
        vec3_of(vals.get("_k"), [1.0; 3]).unwrap_or([1.0; 3])
    }

    /// `matrixFor(o, b, vals)`: the transform the numbers make for one body (column-major).
    /// Move with Land on bed and a turn rests the lowest point at Move Z, from the body's mesh.
    pub fn matrix_for(&self, b: &GizmoBody, vals: &Values) -> M4 {
        if self.kind == GizmoKind::Scale {
            let c = Self::pivot(&b.bbox, str_or(vals.get("pivot"), "base"));
            let k = Self::k_of(vals);
            return camera::mul(&camera::mul(&translation(c), &scaling(k)), &translation(camera::scale(c, -1.0)));
        }
        let c = Self::pivot(&b.bbox, str_or(vals.get("pivot"), "center"));
        let tr = vec3_of(vals.get("translate"), [0.0; 3]).unwrap_or([0.0; 3]);
        let q = quat_of_value(vals.get("rotate"));
        let mut m = camera::mul(&camera::mul(&translation(camera::add(c, tr)), &mat_of_quat(q)), &translation(camera::scale(c, -1.0)));
        let turned = match vals.get("rotate") {
            Some(Value::Array(a)) => a.iter().any(|x| js_number(x).abs() > 1e-9),
            _ => false,
        };
        if truthy(vals.get("bed")) && turned {
            let pos = &b.mesh.positions;
            let mut lo = f64::INFINITY;
            for p in pos.chunks_exact(3) {
                let z = m[0][2] * p[0] as f64 + m[1][2] * p[1] as f64 + m[2][2] * p[2] as f64 + m[3][2];
                if z < lo {
                    lo = z;
                }
            }
            if lo.is_finite() {
                m = camera::mul(&translation([0.0, 0.0, tr[2] - lo]), &m);
            }
        }
        m
    }

    fn collect_bodies(&self, vp: &Viewport, values: &Values) -> Vec<GizmoBody> {
        self.body_ids(values)
            .into_iter()
            .filter_map(|id| {
                let mesh = vp.body_mesh(&id)?.clone();
                let bbox = vp.body_box(&id)?;
                Some(GizmoBody { id, bbox, mesh })
            })
            .collect()
    }

    /// `sync()`: put the handles where the tool's numbers put the part (after opening, typing,
    /// a drop). Cheap when nothing changed. Does nothing while a handle is held.
    pub fn sync(&mut self, vp: &mut Viewport, values: &Values) {
        if self.drag.is_some() {
            return;
        }
        if self.kind == GizmoKind::Pose && str_or(values.get("_mode"), "rotate") != "rotate" {
            self.hide(vp);
            return;
        }
        let ids = self.body_ids(values);
        let key = format!(
            "{}|{}",
            serde_json::to_string(values).unwrap_or_default(),
            ids.iter().map(|id| format!("{id}:{:?}", vp.shown_revision(id))).collect::<Vec<_>>().join(",")
        );
        if self.shown && self.sync_key.as_deref() == Some(key.as_str()) {
            self.publish(vp, values);
            return;
        }
        self.bodies = self.collect_bodies(vp, values);
        if self.bodies.is_empty() {
            self.hide(vp);
            return;
        }
        self.values = values.clone();
        let b = self.bodies[0].clone();
        let vals = self.values_for(values, Some(&b));
        let m = self.matrix_for(&b, &vals);
        let kind = if self.kind == GizmoKind::Scale { str_or(vals.get("pivot"), "base") } else { str_or(vals.get("pivot"), "center") };
        let p0 = Self::pivot(&b.bbox, kind);
        let c = apply(&m, p0);
        self.obj.pos = c;
        self.obj.quat = if self.kind == GizmoKind::Scale { [0.0, 0.0, 0.0, 1.0] } else { quat_of_value(vals.get("rotate")) };
        self.obj.scale = if self.kind == GizmoKind::Scale { Self::k_of(&vals) } else { [1.0; 3] };
        let tz = vec3_of(vals.get("translate"), [0.0; 3]).unwrap_or([0.0; 3])[2];
        self.land = c[2] - p0[2] - or(tz, 0.0);
        self.shown = true;
        self.sync_key = Some(key);
        self.publish(vp, values);
    }

    fn shape(&self, values: &Values) -> GizmoShape {
        let mut s = GizmoShape::new(self.mode(values), self.obj.pos);
        s.axis = self.axis();
        s.enabled = self.enabled;
        if let Some(d) = &self.drag {
            s.dragging = true;
            s.start = d.pos_start;
            s.rotation_axis = d.rotation_axis;
        }
        s
    }

    fn publish(&self, vp: &mut Viewport, values: &Values) {
        vp.set_gizmo(self.shown.then(|| self.shape(values)));
    }

    /// `hide()`: no handles, every body back where its mesh is.
    pub fn hide(&mut self, vp: &mut Viewport) {
        self.shown = false;
        self.drag = None;
        self.hover = None;
        self.sync_key = None;
        vp.set_gizmo(None);
        vp.set_camera_locked(false);
        self.reset(vp);
    }

    /// `reset()`: put the parts back where the scene has them (the preview shows the moved copy).
    pub fn reset(&mut self, vp: &mut Viewport) {
        for id in self.moved.drain(..) {
            vp.set_body_transform(&id, None);
        }
    }

    /// A preview has landed. False while a handle is held: drop it (1.x checks
    /// `gizmo.tc.dragging`). True: show it; the moved bodies are put back first.
    pub fn accept_preview(&mut self, vp: &mut Viewport) -> bool {
        if self.drag_active() {
            return false;
        }
        self.reset(vp);
        true
    }

    /// The pointer moved with no handle held (`pointerHover`).
    pub fn hover_at(&mut self, vp: &mut Viewport, pos: Option<Pos2>) {
        if self.drag.is_some() || !self.shown {
            return;
        }
        let h = pos.and_then(|p| vp.gizmo_handle_at(p));
        if h != self.hover {
            self.hover = h;
            let values = self.values.clone();
            self.publish(vp, &values);
        }
    }

    /// The drag plane of `pointerMove` (TransformControlsPlane): its normal, through the
    /// handles' centre.
    fn plane_normal(&self, vp: &Viewport, mode: GizmoMode, h: Handle) -> V3 {
        let cam = vp.camera();
        let e = gizmo_draw::eye(cam, self.obj.pos);
        let facing = cam.basis().2;
        let unit = |i: usize| [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]][i];
        let dir = match mode {
            GizmoMode::Rotate => [0.0; 3],
            _ => match h {
                Handle::X | Handle::Y | Handle::Z => {
                    let v = unit(match h {
                        Handle::X => 0,
                        Handle::Y => 1,
                        _ => 2,
                    });
                    let align = camera::cross(e, v);
                    camera::cross(v, align)
                }
                Handle::XY => unit(2),
                Handle::YZ => unit(0),
                Handle::XZ => unit(1),
                _ => [0.0; 3],
            },
        };
        if camera::length(dir) == 0.0 { facing } else { camera::normalize(dir) }
    }

    fn plane_hit(&self, vp: &Viewport, pos: Pos2, mode: GizmoMode, h: Handle) -> Option<V3> {
        let n = self.plane_normal(vp, mode, h);
        let (o, d) = vp.ray(pos);
        let den = camera::dot(n, d);
        if den == 0.0 {
            return None;
        }
        let t = camera::dot(n, camera::sub(self.obj.pos, o)) / den;
        (t >= 0.0).then(|| camera::add(o, camera::scale(d, t)))
    }

    /// `pointerDown` on a handle. True when a drag starts (the camera is held still and the
    /// preview cleared, as 1.x's `start()`).
    pub fn press(&mut self, vp: &mut Viewport, pos: Pos2) -> bool {
        if !self.shown || !self.enabled || self.drag.is_some() {
            return false;
        }
        let Some(h) = vp.gizmo_handle_at(pos) else { return false };
        let mode = self.mode(&self.values);
        let Some(hit) = self.plane_hit(vp, pos, mode, h) else { return false };
        self.drag = Some(Drag {
            handle: h,
            pos_start: self.obj.pos,
            quat_start: self.obj.quat,
            scale_start: self.obj.scale,
            point_start: camera::sub(hit, self.obj.pos),
            rotation_axis: [0.0, 0.0, 1.0],
        });
        self.hover = Some(h);
        self.grabbed = true;
        self.start_q = self.obj.quat;
        vp.set_camera_locked(true);
        vp.clear_preview();
        let values = self.values.clone();
        self.publish(vp, &values);
        true
    }

    /// `pointerMove` while a handle is held, then `drag()`: the part follows on screen. Returns
    /// the numbers the handles stand for now (`liveNumbers`). `free` is Shift (no 5° steps).
    pub fn drag_to(&mut self, vp: &mut Viewport, pos: Pos2, free: bool) -> Option<Values> {
        let mut d = self.drag?;
        let mode = self.mode(&self.values);
        let hit = self.plane_hit(vp, pos, mode, d.handle)?;
        let end = camera::sub(hit, d.pos_start);
        match mode {
            GizmoMode::Translate => {
                let mut off = camera::sub(end, d.point_start);
                for (k, c) in ['X', 'Y', 'Z'].into_iter().enumerate() {
                    if !d.handle.has(c) {
                        off[k] = 0.0;
                    }
                }
                self.obj.pos = camera::add(off, d.pos_start);
            }
            GizmoMode::Scale => {
                let k = if d.handle == Handle::XYZ {
                    let mut s = camera::length(end) / camera::length(d.point_start);
                    if camera::dot(end, d.point_start) < 0.0 {
                        s = -s;
                    }
                    [s; 3]
                } else {
                    let mut k = [0, 1, 2].map(|i| end[i] / d.point_start[i]);
                    for (i, c) in ['X', 'Y', 'Z'].into_iter().enumerate() {
                        if !d.handle.has(c) {
                            k[i] = 1.0;
                        }
                    }
                    k
                };
                self.obj.scale = [0, 1, 2].map(|i| d.scale_start[i] * k[i]);
            }
            GizmoMode::Rotate => {
                let cam_pos = vp.camera().position;
                let e = gizmo_draw::eye(vp.camera(), self.obj.pos);
                let off = camera::sub(end, d.point_start);
                let speed = 20.0 / camera::length(camera::sub(self.obj.pos, cam_pos));
                let mut in_plane = false;
                let mut axis = [0.0, 0.0, 1.0];
                let mut angle = 0.0;
                match d.handle {
                    Handle::XYZE => {
                        axis = v_normalize(camera::cross(off, e));
                        angle = camera::dot(off, camera::cross(axis, e)) * speed;
                    }
                    Handle::X | Handle::Y | Handle::Z => {
                        axis = match d.handle {
                            Handle::X => [1.0, 0.0, 0.0],
                            Handle::Y => [0.0, 1.0, 0.0],
                            _ => [0.0, 0.0, 1.0],
                        };
                        let t = camera::cross(axis, e);
                        if camera::length(t) == 0.0 {
                            in_plane = true;
                        } else {
                            angle = camera::dot(off, v_normalize(t)) * speed;
                        }
                    }
                    _ => {}
                }
                if d.handle == Handle::E || in_plane {
                    axis = e;
                    angle = angle_to(end, d.point_start);
                    let s = camera::cross(v_normalize(end), v_normalize(d.point_start));
                    angle *= if camera::dot(s, e) < 0.0 { 1.0 } else { -1.0 };
                }
                if !free {
                    angle = js_round(angle / ROTATION_SNAP) * ROTATION_SNAP;
                }
                d.rotation_axis = axis;
                self.obj.quat = quat_normalize(quat_mul(quat_axis_angle(axis, angle), d.quat_start));
            }
        }
        self.drag = Some(d);
        let live = self.drag_values();
        self.apply_transforms(vp, &live);
        let values = self.values.clone();
        self.publish(vp, &values);
        Some(live)
    }

    fn apply_transforms(&mut self, vp: &mut Viewport, vals: &Values) {
        for b in self.bodies.clone() {
            let m = self.matrix_for(&b, vals);
            vp.set_body_transform(&b.id, Some(m));
            if !self.moved.contains(&b.id) {
                self.moved.push(b.id.clone());
            }
        }
    }

    /// `dragValues()`: the tool's numbers the handles stand for right now.
    pub fn drag_values(&mut self) -> Values {
        let first = self.bodies.first().cloned();
        let mut v = self.values_for(&self.values, first.as_ref());
        if self.kind == GizmoKind::Scale {
            let mut k = self.obj.scale.map(|x| (js_round(x * 10000.0) / 10000.0).max(1e-4));
            if truthy(v.get("uniform")) {
                let prev = Self::k_of(&v);
                let i = (0..3).fold(0, |a, j| if (k[j] - prev[j]).abs() > (k[a] - prev[a]).abs() { j } else { a });
                k = [k[i]; 3];
                self.obj.scale = k;
            }
            v.insert("_k".into(), jvec(k));
            return v;
        }
        let Some(b) = first else { return v };
        let c = Self::pivot(&b.bbox, str_or(v.get("pivot"), "center"));
        v.insert("rotate".into(), jvec(rot_of(self.obj.quat)));
        if self.mode(&self.values) == GizmoMode::Translate {
            let p = self.obj.pos;
            let t = [p[0] - c[0], p[1] - c[1], p[2] - c[2] - self.land].map(|x| js_round(x * 1000.0) / 1000.0);
            v.insert("translate".into(), jvec(t));
        }
        v
    }

    /// `pointerUp`, then 1.x's `drop()`.
    pub fn release(&mut self, vp: &mut Viewport) -> GizmoDrop {
        if self.drag.is_none() {
            return GizmoDrop::Nothing;
        }
        let v = self.drag_values();
        self.drag = None;
        self.hover = None;
        vp.set_camera_locked(false);
        let out = match self.kind {
            GizmoKind::Pose => {
                let Some(b) = self.bodies.first().cloned() else { return GizmoDrop::Nothing };
                let m = self.matrix_for(&b, &v);
                let rot = vec3_of(v.get("rotate"), [0.0; 3]).unwrap_or([0.0; 3]);
                if rot.iter().all(|a| a.abs() < 1e-9) {
                    self.reset(vp);
                    self.sync_key = None;
                    let values = self.values.clone();
                    self.sync(vp, &values);
                    return GizmoDrop::Nothing;
                }
                if !self.enabled {
                    // the last turn is still on its way: this drag is let go of
                    self.obj.quat = self.start_q;
                    let back = self.drag_values();
                    self.apply_transforms(vp, &back);
                    let values = self.values.clone();
                    self.publish(vp, &values);
                    return GizmoDrop::Nothing;
                }
                GizmoDrop::Pose(m)
            }
            GizmoKind::Scale => {
                let k = Self::k_of(&v);
                let mut patch = Values::new();
                patch.insert("mode".into(), json!("factor"));
                if truthy(self.values.get("uniform")) || ((k[0] - k[1]).abs() < 1e-9 && (k[1] - k[2]).abs() < 1e-9) {
                    patch.insert("uniform".into(), json!(true));
                    patch.insert("factor".into(), jnum(k[0]));
                } else {
                    patch.insert("uniform".into(), json!(false));
                    patch.insert("factor3".into(), jvec(k));
                }
                GizmoDrop::Values { patch, auto_accept: true }
            }
            GizmoKind::Move => {
                let mut patch = Values::new();
                for key in ["translate", "rotate"] {
                    if let Some(x) = v.get(key) {
                        patch.insert(key.into(), x.clone());
                    }
                }
                GizmoDrop::Values { patch, auto_accept: true }
            }
        };
        let values = self.values.clone();
        self.publish(vp, &values);
        out
    }

    /// Everything for one frame, from egui's input. Call right after `viewport.show(ui)` with
    /// its response and the tool's resolved values.
    pub fn update(&mut self, vp: &mut Viewport, ui: &egui::Ui, response: &egui::Response, values: &Values) -> GizmoEvent {
        if self.drag.is_none() {
            self.sync(vp, values);
        }
        let (hover, pressed, released, down, shift) = ui.input(|i| {
            (i.pointer.hover_pos(), i.pointer.primary_pressed(), i.pointer.primary_released(), i.pointer.primary_down(), i.modifiers.shift)
        });
        if self.drag.is_some() {
            if released || !down {
                return GizmoEvent::Dropped(self.release(vp));
            }
            if let Some(p) = hover {
                if let Some(v) = self.drag_to(vp, p, shift) {
                    return GizmoEvent::Moved(v);
                }
            }
            return GizmoEvent::None;
        }
        if pressed && response.contains_pointer() {
            if let Some(p) = hover {
                if self.press(vp, p) {
                    return GizmoEvent::Started;
                }
            }
        }
        self.hover_at(vp, if response.contains_pointer() { hover } else { None });
        GizmoEvent::None
    }
}
