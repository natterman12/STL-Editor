//! The drag-the-plane control on screen (spec 2.2): the translucent plane, its outline, the
//! arrow and, while dragging, the distance box. Drawn by egui over the 3D view (no new render
//! when it changes); the maths is in `plane_drag.rs`.

use egui::{Color32, CornerRadius, CursorIcon, FontId, Pos2, Rect, Shape, Stroke, StrokeKind};

use super::camera::{self, V3};
use super::plane_drag::{self, DragPlane};
use super::Viewport;

/// The Move gizmo's handle colour under the pointer and while dragging (`gizmo_draw::colour`).
const ACTIVE: Color32 = Color32::from_rgb(0xff, 0xff, 0x00);
/// The arrow at rest.
const ARROW: Color32 = Color32::from_rgb(0x2f, 0x6f, 0xd1);
/// The plane's fill (`#2f6fd1` at 25 %, premultiplied) and outline.
const PLANE_FILL: Color32 = Color32::from_rgba_premultiplied(0x0c, 0x1c, 0x34, 0x40);
const PLANE_LINE: Color32 = Color32::from_rgb(0x2f, 0x6f, 0xd1);
/// The arrow's head (points).
const HEAD_LEN: f32 = 13.0;
const HEAD_HALF: f32 = 6.0;

/// A drag of the arrow: where it started, from what plane and distance, and where it is now.
/// Every frame works from the start (the app feeds the new distance back mid-drag).
#[derive(Clone, Copy, Debug)]
pub(super) struct Grab {
    from: [f64; 2],
    /// Projection or vertical, decided when the drag started and kept to its end.
    mode: plane_drag::DragMode,
    d0: f64,
    plane: DragPlane,
    last: f64,
}

/// Two unit vectors in the plane of `n`.
fn perp(n: V3) -> (V3, V3) {
    let t = if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let x = camera::normalize(camera::cross(n, t));
    (x, camera::cross(n, x))
}

impl Viewport {
    /// The arrow under the pointer, and a drag of it (before the camera's own input).
    pub(super) fn drag_input(&mut self, ui: &egui::Ui, response: &egui::Response) {
        self.drag_hot = false;
        self.drag_busy = false;
        let Some(p) = self.drag_plane else {
            self.drag_hover = false;
            if let Some(g) = self.drag.take() {
                self.drag_out = Some((g.last, true));
                self.drag_busy = true;
                ui.ctx().request_repaint();
            }
            return;
        };
        let rect = self.rect;
        let local = |pos: Pos2| [(pos.x - rect.min.x) as f64, (pos.y - rect.min.y) as f64];
        let (hover, latest, pressed, down, origin) =
            ui.input(|i| (i.pointer.hover_pos(), i.pointer.latest_pos(), i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.press_origin()));
        if let Some(mut g) = self.drag {
            if let Some(pos) = latest.or(hover) {
                let d = plane_drag::drag_distance_in(&self.camera, &g.plane, g.from, local(pos), g.d0, g.mode);
                if d != g.last {
                    g.last = d;
                    self.drag_out = Some((d, false));
                }
            }
            self.drag_hot = true;
            self.drag_busy = true;
            if down {
                self.drag = Some(g);
            } else {
                self.drag = None;
                self.drag_out = Some((g.last, true));
            }
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
            if self.drag_out.is_some() {
                // the app collects the distance on the next frame
                ui.ctx().request_repaint();
            }
            return;
        }
        let over = response.contains_pointer() && hover.is_some_and(|h| plane_drag::hits_handle(&self.camera, &p, local(h)));
        if over != self.drag_hover {
            self.drag_hover = over;
            ui.ctx().request_repaint();
        }
        if !over {
            return;
        }
        self.drag_hot = true;
        ui.ctx().set_cursor_icon(CursorIcon::Grab);
        if pressed {
            let from = local(origin.or(hover).unwrap_or(rect.center()));
            self.drag = Some(Grab { from, mode: plane_drag::drag_mode(&self.camera, &p), d0: p.distance, plane: p, last: p.distance });
            self.drag_busy = true;
            self.camera.cancel_flight();
            ui.ctx().request_repaint();
        }
    }

    /// Draw the plane, its outline, the arrow and (while dragging) the distance box.
    pub(super) fn paint_drag_plane(&self, ui: &egui::Ui, rect: Rect) {
        let Some(p) = self.drag_plane else { return };
        let cam = &self.camera;
        let painter = ui.painter_at(rect);
        let to_screen = |w: V3| -> Option<Pos2> {
            if cam.project(w).1 <= 0.0 {
                return None;
            }
            let v = cam.world_to_view(w);
            Some(Pos2::new(rect.min.x + v[0] as f32, rect.min.y + v[1] as f32))
        };
        let n = camera::normalize(p.normal);
        let c = plane_drag::center(&p);
        let (u, v) = perp(n);
        // the box's cross-section in the plane's own axes, a little larger
        let (mut u0, mut u1, mut v0, mut v1) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
        for i in 0..8 {
            let s = &p.span;
            let k = [
                if i & 1 == 0 { s.min[0] } else { s.max[0] },
                if i & 2 == 0 { s.min[1] } else { s.max[1] },
                if i & 4 == 0 { s.min[2] } else { s.max[2] },
            ];
            let d = camera::sub(k, c);
            let (a, b) = (camera::dot(d, u), camera::dot(d, v));
            u0 = u0.min(a);
            u1 = u1.max(a);
            v0 = v0.min(b);
            v1 = v1.max(b);
        }
        if u0.is_finite() && v0.is_finite() {
            let pad = 0.08 * (u1 - u0).max(v1 - v0) + 2.0;
            let at = |a: f64, b: f64| camera::add(c, camera::add(camera::scale(u, a), camera::scale(v, b)));
            let corners = [at(u0 - pad, v0 - pad), at(u1 + pad, v0 - pad), at(u1 + pad, v1 + pad), at(u0 - pad, v1 + pad)];
            let pts: Option<Vec<Pos2>> = corners.iter().map(|&w| to_screen(w)).collect();
            if let Some(pts) = pts {
                painter.add(Shape::convex_polygon(pts.clone(), PLANE_FILL, Stroke::NONE));
                painter.add(Shape::closed_line(pts, Stroke::new(1.5, PLANE_LINE)));
            }
        }

        let (Some(a), Some(_)) = (to_screen(c), to_screen(camera::add(c, camera::scale(n, plane_drag::arrow_len(cam, &p))))) else { return };
        let tip = plane_drag::handle_at(cam, &p);
        let b = Pos2::new(rect.min.x + tip[0] as f32, rect.min.y + tip[1] as f32);
        let col = if self.drag.is_some() || self.drag_hover { ACTIVE } else { ARROW };
        let dark_edge = Stroke::new(1.0, Color32::from_black_alpha(140));
        let along = b - a;
        if along.length() < HEAD_LEN + 2.0 {
            // face on: the arrow is a dot
            painter.circle(a, HEAD_HALF + 2.0, col, dark_edge);
        } else {
            let dir = along.normalized();
            let side = egui::vec2(-dir.y, dir.x) * HEAD_HALF;
            let base = b - dir * HEAD_LEN;
            painter.line_segment([a, base], Stroke::new(3.0, col));
            painter.add(Shape::convex_polygon(vec![b, base + side, base - side], col, dark_edge));
            painter.circle(a, 3.5, col, Stroke::NONE);
        }

        if self.drag.is_some() {
            use crate::tools::fields::number;
            let text = format!("{} {}", number::fmt_len(p.distance, None, self.inch), number::unit_name(self.inch));
            let tk = crate::theme::tokens(self.dark);
            let font = FontId::proportional(12.0);
            let galley = painter.layout_no_wrap(text, font, tk.text);
            let pos = b + egui::vec2(12.0, -12.0 - galley.size().y);
            let r = Rect::from_min_size(pos, galley.size()).expand2(egui::vec2(6.0, 3.0));
            painter.rect(r, CornerRadius::same(3), tk.panel, Stroke::new(1.0, tk.line), StrokeKind::Inside);
            painter.galley(pos, galley, tk.text);
        }
    }
}
