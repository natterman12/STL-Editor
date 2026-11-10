use manifold_rust::cross_section::CrossSection;
use manifold_rust::linalg::Vec2;
use manifold_rust::manifold::Manifold;
use manifold_rust::types::Polygons;

use crate::solid::{Solid, segments};

/// A flat shape: closed contours in the XY plane. Outer contours run counter-clockwise and
/// holes clockwise. The last point joins back to the first; it is not repeated.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shape2d {
    pub contours: Vec<Vec<[f64; 2]>>,
}

impl Shape2d {
    /// A prism from z = 0 up to `height`. Empty when the shape is empty or `height` is not
    /// positive. The contours are used as given: overlapping contours are not merged first.
    pub fn extrude(&self, height: f64) -> Solid {
        Solid::wrap(Manifold::extrude(&self.to_polygons(), height, 0, 0.0, Vec2::new(1.0, 1.0)))
    }

    /// Turn the shape around the Y axis, which becomes the Z axis of the solid (as Manifold
    /// does); the part of the shape at negative x is dropped. `segments` is the count for a
    /// full circle; 0 lets the kernel choose.
    pub fn revolve(&self, segments: u32, degrees: f64) -> Solid {
        Solid::wrap(Manifold::revolve(&self.to_polygons(), self::segments(segments), degrees))
    }

    fn to_polygons(&self) -> Polygons {
        self.contours.iter().map(|c| c.iter().map(|p| Vec2::new(p[0], p[1])).collect()).collect()
    }

    fn from_polygons(polygons: &Polygons) -> Shape2d {
        Shape2d { contours: polygons.iter().map(|c| c.iter().map(|p| [p.x, p.y]).collect()).collect() }
    }

    pub(crate) fn from_cross_section(section: &CrossSection) -> Shape2d {
        Shape2d::from_polygons(&section.to_polygons())
    }
}
