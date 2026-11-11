//! Port of `faces`, `_cs_from_polys` and `profile` from `stleditor/sketch.py`.

use crate::contours::{entity_contours, id_wanted, linework};
use crate::entities::{Entity, P2};
use crate::error::SketchError;
use crate::polygon::{Face, node_lines, polygonize};
use stl_kernel::Shape2d;

/// All minimal enclosed regions of the sketch, lines and shapes alike (1.x `faces`).
pub fn faces(entities: &[Entity], only: Option<&[String]>) -> Vec<Face> {
    let lw = linework(entities, only);
    if lw.is_empty() {
        return vec![];
    }
    polygonize(&node_lines(&lw)).into_iter().filter(|f| f.area() > 1e-10).collect()
}

/// The area to extrude or revolve (1.x `profile`).
///
/// Without `regions`: shapes inside other shapes become holes (even-odd), and closed loops made
/// of separate lines and arcs count as shapes too. With `regions` (points in sketch
/// coordinates): exactly the enclosed regions under those points.
pub fn profile_with(
    entities: &[Entity],
    only: Option<&[String]>,
    regions: Option<&[P2]>,
) -> Result<Shape2d, SketchError> {
    let shape = match regions {
        Some(pts) if !pts.is_empty() => {
            let fs = faces(entities, only);
            let picked: Vec<&Face> = fs.iter().filter(|f| pts.iter().any(|&p| f.contains(p))).collect();
            if picked.is_empty() {
                return Err(SketchError::new("None of the picked points is inside a closed region of the sketch."));
            }
            let mut contours = vec![];
            for f in picked {
                contours.push(f.exterior.clone());
                contours.extend(f.holes.iter().cloned());
            }
            if contours.is_empty() {
                return Err(SketchError::new("No closed region there."));
            }
            Shape2d::from_even_odd(&contours)
        }
        _ => {
            let mut contours = vec![];
            for e in entities {
                if id_wanted(e, only) {
                    contours.extend(entity_contours(e));
                }
            }
            let loose: Vec<Entity> = entities
                .iter()
                .filter(|e| match e {
                    Entity::Line(_) | Entity::Arc(_) => true,
                    Entity::Polyline(p) => !p.is_closed(),
                    _ => false,
                })
                .cloned()
                .collect();
            if !loose.is_empty() {
                for f in faces(&loose, only) {
                    contours.push(f.exterior);
                }
            }
            if contours.is_empty() {
                return Err(SketchError::new(
                    "The sketch has no closed profile to use. Draw a rectangle, circle, polygon, slot, or a closed loop of lines and arcs.",
                ));
            }
            Shape2d::from_even_odd(&contours)
        }
    };
    if shape.is_empty() || shape.area() <= 1e-12 {
        return Err(SketchError::new("The sketch profile has no area."));
    }
    Ok(shape)
}

/// `profile` with the pick given as JSON, `{"only": [ids], "regions": [[u, v], ...]}` (either
/// key may be missing or null).
pub fn profile(entities: &[Entity], pick: Option<&serde_json::Value>) -> Result<Shape2d, SketchError> {
    let only: Option<Vec<String>> = pick
        .and_then(|p| p.get("only"))
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect());
    let regions: Option<Vec<P2>> = pick.and_then(|p| p.get("regions")).and_then(|v| v.as_array()).map(|a| {
        a.iter()
            .filter_map(|x| {
                let u = x.as_array()?;
                Some([u.first()?.as_f64()?, u.get(1)?.as_f64()?])
            })
            .collect()
    });
    profile_with(entities, only.as_deref(), regions.as_deref())
}
