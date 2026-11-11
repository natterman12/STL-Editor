//! Port of `mirror` (`stleditor/sketch.py`): mirrored copies of shapes across a sketch line.

use super::axis::{axis_error, find_axis};
use crate::entities::{Entity, P2};
use crate::error::SketchError;

/// Mirrored copies of the chosen shapes across a sketch line, added after the originals.
/// `new_id` is called once for every chosen shape (points included), as in 1.x.
pub fn mirror(
    entities: &[Entity],
    ids: &[String],
    axis_id: &str,
    new_id: &mut dyn FnMut() -> String,
) -> Result<Vec<Entity>, SketchError> {
    let (p0, d) = find_axis(entities, axis_id)?;
    let len = d[0].hypot(d[1]);
    // a line with no length has no direction to mirror across (1.x divides by zero here)
    if !(len > 0.0) || !len.is_finite() {
        return Err(axis_error());
    }
    let d = [d[0] / len, d[1] / len];
    let th = d[1].atan2(d[0]).to_degrees();
    let m = |p: P2| -> P2 {
        let q = [p[0] - p0[0], p[1] - p0[1]];
        let k = 2.0 * (q[0] * d[0] + q[1] * d[1]);
        [p0[0] + k * d[0] - q[0], p0[1] + k * d[1] - q[1]]
    };
    let mut out = vec![];
    for e in entities {
        let Some(id) = e.id() else { continue };
        if !ids.iter().any(|i| i == id) || id == axis_id {
            continue;
        }
        let nid = Some(new_id());
        match e {
            Entity::Line(l) => {
                let mut n = l.clone();
                n.id = nid;
                n.p1 = m(l.p1);
                n.p2 = m(l.p2);
                out.push(Entity::Line(n));
            }
            Entity::Polyline(p) => {
                let mut n = p.clone();
                n.id = nid;
                n.pts = p.pts.iter().map(|&q| m(q)).rev().collect();
                out.push(Entity::Polyline(n));
            }
            Entity::Circle(c) => {
                let mut n = c.clone();
                n.id = nid;
                let [cx, cy] = m([c.cx, c.cy]);
                n.cx = cx;
                n.cy = cy;
                out.push(Entity::Circle(n));
            }
            Entity::Polygon(p) => {
                let mut n = p.clone();
                n.id = nid;
                let [cx, cy] = m([p.cx, p.cy]);
                n.cx = cx;
                n.cy = cy;
                n.angle = Some(2.0 * th - p.angle.unwrap_or(0.0));
                out.push(Entity::Polygon(n));
            }
            Entity::Arc(a) => {
                let mut n = a.clone();
                n.id = nid;
                let [cx, cy] = m([a.cx, a.cy]);
                n.cx = cx;
                n.cy = cy;
                n.a0 = 2.0 * th - a.a1;
                n.a1 = 2.0 * th - a.a0;
                out.push(Entity::Arc(n));
            }
            Entity::Slot(s) => {
                let mut n = s.clone();
                n.id = nid;
                let (a, b) = (m([s.x1, s.y1]), m([s.x2, s.y2]));
                (n.x1, n.y1, n.x2, n.y2) = (a[0], a[1], b[0], b[1]);
                out.push(Entity::Slot(n));
            }
            Entity::Rect(r) => {
                let mut n = r.clone();
                n.id = nid;
                let c = m([r.x + r.w / 2.0, r.y + r.h / 2.0]);
                let (w, h) = (r.w.abs(), r.h.abs());
                n.x = c[0] - w / 2.0;
                n.y = c[1] - h / 2.0;
                n.w = w;
                n.h = h;
                n.angle = Some(2.0 * th - r.angle.unwrap_or(0.0));
                out.push(Entity::Rect(n));
            }
            Entity::Point(_) => {}
        }
    }
    if out.is_empty() {
        return Err(SketchError::new("Select the shapes to mirror (not the mirror line itself)."));
    }
    let mut all = entities.to_vec();
    all.extend(out);
    Ok(all)
}
