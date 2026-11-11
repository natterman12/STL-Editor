//! Port of `find_axis` (`stleditor/sketch.py`).

use crate::entities::{Entity, P2};
use crate::error::SketchError;

/// The message for a missing or unusable axis.
pub(crate) fn axis_error() -> SketchError {
    SketchError::new("Pick a straight sketch line (or the sketch X/Y axis) as the revolve axis.")
}

/// The start point and direction (not normalised) of a straight sketch line or the first
/// segment of a polyline.
pub fn find_axis(entities: &[Entity], axis_id: &str) -> Result<(P2, P2), SketchError> {
    for e in entities {
        if e.id() != Some(axis_id) {
            continue;
        }
        match e {
            Entity::Polyline(p) if p.pts.len() >= 2 => {
                let (a, b) = (p.pts[0], p.pts[1]);
                return Ok((a, [b[0] - a[0], b[1] - a[1]]));
            }
            Entity::Line(l) => return Ok((l.p1, [l.p2[0] - l.p1[0], l.p2[1] - l.p1[1]])),
            _ => {}
        }
    }
    Err(axis_error())
}
