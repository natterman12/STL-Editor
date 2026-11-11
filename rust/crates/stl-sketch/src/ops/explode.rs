//! Port of `explode` (`stleditor/sketch.py`): break a compound shape into lines and arcs.

use crate::contours::entity_path;
use crate::entities::{Arc, Entity, Line, P2};
use serde_json::Map;

pub(crate) fn sub_id(base: &Option<String>, k: usize) -> Option<String> {
    Some(format!("{}_{k}", base.as_deref().unwrap_or("")))
}

pub(crate) fn line(id: Option<String>, p1: P2, p2: P2, construction: bool) -> Entity {
    Entity::Line(Line { id, p1, p2, construction: construction.then_some(true), extra: Map::new() })
}

fn arc(id: Option<String>, (cx, cy, r): (f64, f64, f64), a0: f64, a1: f64, construction: bool) -> Entity {
    Entity::Arc(Arc { id, cx, cy, r, a0, a1, construction: construction.then_some(true), extra: Map::new() })
}

/// Break a compound shape into lines and arcs (keeps the construction flag). Lines, arcs and
/// circles come back as they are.
pub fn explode(e: &Entity) -> Vec<Entity> {
    let c = e.construction();
    match e {
        Entity::Line(_) | Entity::Arc(_) | Entity::Circle(_) => return vec![e.clone()],
        Entity::Rect(r) if r.fillet.unwrap_or(0.0) == 0.0 && r.angle.unwrap_or(0.0) == 0.0 => {
            let (x, y, w, h) = (r.x, r.y, r.w, r.h);
            let p = [[x, y], [x + w, y], [x + w, y + h], [x, y + h]];
            return (0..4).map(|i| line(sub_id(&r.id, i), p[i], p[(i + 1) % 4], c)).collect();
        }
        Entity::Slot(s) => {
            let (x1, y1, x2, y2, r) = (s.x1, s.y1, s.x2, s.y2, s.r);
            let ang = (y2 - y1).atan2(x2 - x1).to_degrees();
            let (nx, ny) = (-ang.to_radians().sin() * r, ang.to_radians().cos() * r);
            return vec![
                line(sub_id(&s.id, 0), [x1 + nx, y1 + ny], [x2 + nx, y2 + ny], c),
                line(sub_id(&s.id, 1), [x2 - nx, y2 - ny], [x1 - nx, y1 - ny], c),
                arc(sub_id(&s.id, 2), (x2, y2, r), ang - 90.0, ang + 90.0, c),
                arc(sub_id(&s.id, 3), (x1, y1, r), ang + 90.0, ang + 270.0, c),
            ];
        }
        _ => {}
    }
    let (pts, closed) = entity_path(e);
    let n = pts.len();
    let count = if closed { n } else { n.saturating_sub(1) };
    let id = e.id().map(str::to_string);
    (0..count).map(|i| line(sub_id(&id, i), pts[i], pts[(i + 1) % n], c)).collect()
}
