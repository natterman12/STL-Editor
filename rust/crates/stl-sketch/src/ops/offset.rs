//! Port of `offset` (`stleditor/sketch.py`): offset copies of the chosen shapes.

use crate::contours::entity_path;
use crate::entities::{Entity, P2, Polyline};
use crate::error::SketchError;
use crate::polygon::{buffer_mitre, is_simple_ring, linemerge, node_lines, offset_curve_pieces};
use serde_json::Map;

fn polyline(id: String, pts: Vec<P2>, closed: bool, construction: bool) -> Entity {
    Entity::Polyline(Polyline { id: Some(id), pts, closed: Some(closed), construction: construction.then_some(true), extra: Map::new() })
}

/// Offset copies of the chosen shapes (positive = outward / to the left of lines), added after
/// the originals. `new_id` is called as in 1.x: once for every chosen shape, and again for every
/// piece made from loose lines.
pub fn offset(
    entities: &[Entity],
    ids: &[String],
    d: f64,
    new_id: &mut dyn FnMut() -> String,
) -> Result<Vec<Entity>, SketchError> {
    let mut out: Vec<Entity> = vec![];
    let mut loose: Vec<&Entity> = vec![];
    for e in entities {
        let Some(id) = e.id() else { continue };
        if !ids.iter().any(|i| i == id) {
            continue;
        }
        let nid = Some(new_id());
        match e {
            Entity::Rect(r) => {
                let f = r.fillet.unwrap_or(0.0);
                let (x0, x1) = if r.x <= r.x + r.w { (r.x, r.x + r.w) } else { (r.x + r.w, r.x) };
                let (y0, y1) = if r.y <= r.y + r.h { (r.y, r.y + r.h) } else { (r.y + r.h, r.y) };
                if x1 - x0 + 2.0 * d <= 0.0 || y1 - y0 + 2.0 * d <= 0.0 {
                    return Err(SketchError::new("Offset is larger than the shape."));
                }
                let mut n = r.clone();
                n.id = nid;
                n.x = x0 - d;
                n.y = y0 - d;
                n.w = x1 - x0 + 2.0 * d;
                n.h = y1 - y0 + 2.0 * d;
                n.fillet = Some(if f != 0.0 { (f + d).max(0.0) } else { 0.0 });
                out.push(Entity::Rect(n));
            }
            Entity::Circle(c) => {
                if c.r + d <= 0.0 {
                    return Err(SketchError::new("Offset is larger than the circle."));
                }
                let mut n = c.clone();
                n.id = nid;
                n.r = c.r + d;
                out.push(Entity::Circle(n));
            }
            Entity::Arc(a) => {
                let mut n = a.clone();
                n.id = nid;
                n.r = (a.r + d).max(1e-6);
                out.push(Entity::Arc(n));
            }
            Entity::Polygon(p) => {
                let mut n = p.clone();
                n.id = nid;
                n.r = p.r + d / (std::f64::consts::PI / p.sides().max(1) as f64).cos();
                out.push(Entity::Polygon(n));
            }
            Entity::Slot(s) => {
                let mut n = s.clone();
                n.id = nid;
                n.r = s.r + d;
                out.push(Entity::Slot(n));
            }
            Entity::Polyline(p) if p.is_closed() => {
                // one outline per piece, all with the one new id (as 1.x); nothing left is one empty outline
                let id = nid.unwrap();
                let mut rings = buffer_mitre(&p.pts, d);
                if rings.is_empty() {
                    rings.push(vec![]);
                }
                for ring in rings {
                    out.push(polyline(id.clone(), ring, true, p.construction.unwrap_or(false)));
                }
            }
            _ => loose.push(e),
        }
    }
    let paths: Vec<Vec<P2>> = loose
        .iter()
        .filter_map(|e| {
            let (mut pts, closed) = entity_path(e);
            if pts.len() < 2 {
                return None;
            }
            if closed {
                pts.push(pts[0]);
            }
            Some(pts)
        })
        .collect();
    if !paths.is_empty() {
        let noded = node_lines(&paths);
        // 1.x: `linemerge` of a single line string is an error
        if noded.len() == 1 {
            return Err(SketchError::new(format!("Cannot linemerge {}", wkt_line(&noded[0]))));
        }
        for g in linemerge(&noded) {
            if is_simple_ring(&g) {
                let mut rings = buffer_mitre(&g, d);
                if rings.is_empty() {
                    rings.push(vec![]);
                }
                for ring in rings {
                    out.push(polyline(new_id(), ring, true, false));
                }
            } else {
                for pts in offset_curve_pieces(&g, d) {
                    if pts.len() >= 2 {
                        out.push(polyline(new_id(), pts, false, false));
                    }
                }
            }
        }
    }
    if out.is_empty() {
        return Err(SketchError::new("Select the shapes to offset first."));
    }
    let mut all = entities.to_vec();
    all.extend(out);
    Ok(all)
}

/// One number the way shapely 2.2's WKT writer prints it.
fn wkt_num(v: f64) -> String {
    if v == 0.0 {
        return "0".into(); // also -0.0
    }
    let a = v.abs();
    if a < 1e-4 || a >= 1e17 {
        // Scientific form, with a sign only on a positive exponent (1.2549e-5, 1e+17).
        let s = format!("{v:e}");
        return match s.split_once('e') {
            Some((m, e)) if !e.starts_with('-') => format!("{m}e+{e}"),
            _ => s,
        };
    }
    // Plain form: shortest round-trip digits, but at most 16 decimals.
    let s = format!("{v}");
    match s.split_once('.') {
        Some((_, frac)) if frac.len() > 16 => {
            format!("{v:.16}").trim_end_matches('0').trim_end_matches('.').to_string()
        }
        _ => s,
    }
}

/// shapely's text for a line string, as it shows in 1.x error messages.
fn wkt_line(pts: &[P2]) -> String {
    let num = wkt_num;
    let body: Vec<String> = pts.iter().map(|p| format!("{} {}", num(p[0]), num(p[1]))).collect();
    format!("LINESTRING ({})", body.join(", "))
}

#[cfg(test)]
mod wkt_tests {
    use super::wkt_num;

    #[test]
    fn numbers_match_shapely() {
        // Strings from shapely 2.2.0: str(LineString([(v, 0), (1, 2)])).
        let cases: [(f64, &str); 8] = [
            (0.1 + 0.2, "0.3"),
            (1e17, "1e+17"),
            (1.5e20, "1.5e+20"),
            (-3e-5, "-3e-5"),
            (1.2549e-5, "1.2549e-5"),
            (5e-324, "5e-324"),
            (123.456, "123.456"),
            (-0.0, "0"),
        ];
        for (v, want) in cases {
            assert_eq!(wkt_num(v), want, "{v:e}");
        }
        assert_eq!(wkt_num(1e16), "10000000000000000");
        assert_eq!(wkt_num(1.0 / 3.0), "0.3333333333333333");
        assert_eq!(wkt_num(1e-4), "0.0001");
    }
}
