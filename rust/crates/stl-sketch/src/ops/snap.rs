//! Port of `snap_points` (`stleditor/sketch.py`): key points used for snapping.

use crate::contours::arc_points;
use crate::entities::{Entity, P2};

pub fn snap_points(entities: &[Entity]) -> Vec<P2> {
    let mut pts: Vec<P2> = vec![];
    for e in entities {
        match e {
            Entity::Rect(r) => {
                let (x, y, w, h) = (r.x, r.y, r.w, r.h);
                pts.extend([[x, y], [x + w, y], [x + w, y + h], [x, y + h], [x + w / 2.0, y + h / 2.0]]);
            }
            Entity::Circle(c) => pts.push([c.cx, c.cy]),
            Entity::Polygon(p) => pts.push([p.cx, p.cy]),
            Entity::Slot(s) => pts.extend([[s.x1, s.y1], [s.x2, s.y2]]),
            Entity::Polyline(p) => pts.extend(p.pts.iter().copied()),
            Entity::Line(l) => pts.extend([l.p1, l.p2]),
            Entity::Arc(a) => {
                let p = arc_points(a.cx, a.cy, a.r, a.a0, a.a1, Some(2));
                pts.extend([[a.cx, a.cy], p[0], p[p.len() - 1]]);
            }
            Entity::Point(_) => {}
        }
    }
    pts
}
