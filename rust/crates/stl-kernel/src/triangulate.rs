use manifold_rust::linalg::Vec2;
use manifold_rust::polygon;
use manifold_rust::types::Polygons;

/// Triangles for some polygon contours, like 1.x `manifold3d.triangulate(polys)`: default
/// epsilon (-1, worked out from the shape's size) with the convex shortcut on. Indices run
/// across the contours in order, so the second contour starts after the last point of the first.
pub fn triangulate(polys: &[Vec<[f64; 2]>]) -> Vec<[u32; 3]> {
    let polygons: Polygons = polys.iter().map(|c| c.iter().map(|p| Vec2::new(p[0], p[1])).collect()).collect();
    polygon::triangulate(&polygons, -1.0, true).iter().map(|t| [t.x as u32, t.y as u32, t.z as u32]).collect()
}
