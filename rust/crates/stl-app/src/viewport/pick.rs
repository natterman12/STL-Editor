//! Picking, as 1.x `viewer.js`: `pick`, `rayToPlane` and `pickPreview`.
//!
//! Body hits go through the [`Bvh`] of the revision on screen, built in the background by the
//! body's prepare job (inline on wasm). Until that tree is there a pick tests every triangle
//! ([`ray_front_direct`], same rules) instead of waiting. Preview meshes carry their own tree.

use egui::Pos2;
use stl_mesh::bvh::{ray_front_direct, ray_triangle, tri_points};

use super::camera::{self, V3};
use super::{PlaneFrame, Viewport};

/// What a click landed on. `face` is the triangle index, as three.js `faceIndex`.
#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    Body { id: String, face: u32, point: [f64; 3], normal: [f64; 3] },
    Plane { id: String, point: [f64; 3] },
}

impl Viewport {
    /// `pick(ev, { bodies, planes })`. Hidden bodies are skipped. Planes are tested only when
    /// they are shown. A body just behind a transparent plane wins, within `extent * 0.002`.
    pub fn pick(&self, pos: Pos2, bodies: bool, planes: bool) -> Option<Hit> {
        let (origin, dir) = self.ray(pos);
        if camera::dot(dir, dir) == 0.0 {
            return None;
        }
        let section = self.active_section();
        let body = if bodies { self.nearest_body(origin, dir, section) } else { None };
        let plane = if planes { self.nearest_plane(origin, dir, section) } else { None };
        match (body, plane) {
            (None, None) => None,
            (Some(b), None) => Some(b.into_hit()),
            (None, Some((id, _, point))) => Some(Hit::Plane { id, point }),
            (Some(b), Some((id, pt, point))) => {
                // viewer.js: prefer the body when it is the first hit, or when a plane is only
                // a hair in front (planes are transparent).
                if b.t - pt < self.extent() * 0.002 {
                    Some(b.into_hit())
                } else {
                    Some(Hit::Plane { id, point })
                }
            }
        }
    }

    /// `rayToPlane`: the ray through `pos` meets the infinite plane of `frame` (its origin and
    /// normal). `None` when the plane is parallel and the ray does not lie in it, or the
    /// intersection is behind the camera.
    pub fn ray_to_plane(&self, pos: Pos2, frame: &PlaneFrame) -> Option<[f64; 3]> {
        let (origin, dir) = self.ray(pos);
        if camera::length(frame.normal) == 0.0 {
            return None;
        }
        let n = camera::normalize(frame.normal);
        let constant = -camera::dot(frame.origin, n);
        let denom = camera::dot(n, dir);
        if denom == 0.0 {
            // three's Ray.intersectPlane returns the origin when the ray lies in the plane
            if camera::dot(origin, n) + constant == 0.0 {
                return Some(origin);
            }
            return None;
        }
        let t = -(camera::dot(origin, n) + constant) / denom;
        if t < 0.0 {
            return None;
        }
        Some(camera::add(origin, camera::scale(dir, t)))
    }

    /// `pickPreview`: the preview mesh under the pointer, nearest front face.
    pub fn pick_preview(&self, pos: Pos2) -> Option<String> {
        let (origin, dir) = self.ray(pos);
        if camera::dot(dir, dir) == 0.0 {
            return None;
        }
        let mut best: Option<(String, f64)> = None;
        for p in &self.preview {
            if let Some((_, t)) = p.bvh.ray_front(&p.mesh, origin, dir) {
                if best.as_ref().is_none_or(|(_, bt)| t < *bt) {
                    best = Some((p.id.clone(), t));
                }
            }
        }
        best.map(|(id, _)| id)
    }

    fn nearest_body(&self, origin: V3, dir: V3, section: Option<[f64; 4]>) -> Option<BodyHit> {
        let mut best: Option<BodyHit> = None;
        for e in &self.bodies {
            // `hidden` is mesh.visible in viewer.js. A body covered by a preview stays pickable.
            if e.view.hidden {
                continue;
            }
            let Some(r) = e.current() else { continue };
            let keep = |p: V3| keeps(section, p);
            let hit = match r.bvh.get() {
                Some(bvh) => bvh.ray_front_where(&r.mesh, origin, dir, keep),
                None => ray_front_direct(&r.mesh, origin, dir, keep),
            };
            let Some((face, t)) = hit else { continue };
            if best.as_ref().is_some_and(|h| t >= h.t) {
                continue;
            }
            let Some(pts) = tri_points(&r.mesh, face) else { continue };
            best = Some(BodyHit { id: e.view.id.clone(), face, t, point: along(origin, dir, t), normal: unit_normal(pts) });
        }
        best
    }

    fn nearest_plane(&self, origin: V3, dir: V3, section: Option<[f64; 4]>) -> Option<(String, f64, V3)> {
        let shown = self.shown_planes();
        // shown_planes lists base planes first. viewer.js tests construction planes first, then
        // the base planes, and a stable distance sort keeps the earlier one when t ties.
        let base_n = if self.base_planes_visible() { shown.len().min(3) } else { 0 };
        let mut best: Option<(String, f64, V3)> = None;
        for i in (base_n..shown.len()).chain(0..base_n) {
            let (id, frame, size) = &shown[i];
            let Some(t) = ray_quad(origin, dir, *frame, *size) else { continue };
            let point = along(origin, dir, t);
            if !keeps(section, point) {
                continue;
            }
            if best.as_ref().is_none_or(|(_, bt, _)| t < *bt) {
                best = Some((id.clone(), t, point));
            }
        }
        best
    }
}

struct BodyHit {
    id: String,
    face: u32,
    t: f64,
    point: V3,
    normal: V3,
}

impl BodyHit {
    fn into_hit(self) -> Hit {
        Hit::Body { id: self.id, face: self.face, point: self.point, normal: self.normal }
    }
}

/// `dot(n, p) + d >= -1e-6`, viewer.js `section.distanceToPoint(hit) >= -1e-6`.
fn keeps(section: Option<[f64; 4]>, p: V3) -> bool {
    let Some(s) = section else { return true };
    p[0] * s[0] + p[1] * s[1] + p[2] * s[2] + s[3] >= -1e-6
}

fn along(origin: V3, dir: V3, t: f64) -> V3 {
    camera::add(origin, camera::scale(dir, t))
}

/// The two triangles of three's `PlaneGeometry`, carried onto the frame by `makeBasis(x, y, cross)`.
fn ray_quad(origin: V3, dir: V3, frame: [V3; 4], size: f64) -> Option<f64> {
    let [o, x, y, _] = frame;
    let h = size * 0.5;
    let at = |u: f64, v: f64| camera::add(o, camera::add(camera::scale(x, u), camera::scale(y, v)));
    let v0 = at(-h, h);
    let v1 = at(h, h);
    let v2 = at(-h, -h);
    let v3 = at(h, -h);
    let mut best: Option<f64> = None;
    for (a, b, c) in [(v0, v2, v1), (v2, v3, v1)] {
        if let Some(t) = ray_triangle(origin, dir, a, b, c, false) {
            if best.is_none_or(|bt| t < bt) {
                best = Some(t);
            }
        }
    }
    best
}

fn unit_normal(p: [[f64; 3]; 3]) -> V3 {
    let e1 = camera::sub(p[1], p[0]);
    let e2 = camera::sub(p[2], p[0]);
    camera::normalize(camera::cross(e1, e2))
}
