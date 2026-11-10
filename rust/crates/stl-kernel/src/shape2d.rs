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
    /// A circle of `segments` sides centred on the origin. Matches Manifold's
    /// `CrossSection::Circle` (and 1.x `CrossSection.circle`).
    pub fn circle(r: f64, segments: u32) -> Shape2d {
        Shape2d::from_cross_section(&CrossSection::circle(r, self::segments(segments)))
    }

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

    /// Grow (positive) or shrink (negative) the shape by `delta` with round corners. Matches
    /// 1.x `CrossSection.offset(delta, JoinType.Round)`: miter limit 2, and the corner segment
    /// count the kernel picks for `delta`.
    pub fn offset_round(&self, delta: f64) -> Shape2d {
        // manifold-rust 0.19's offset hands back the Clipper2 result unsnapped (C++ divides integer
        // coordinates by 1e8, so its vertices sit on that grid); snap here so the grid searches
        // that follow (connector placement) see the same coordinates as 1.x.
        Shape2d::from_cross_section(&self.to_cross_section().offset(delta)).on_clipper_grid()
    }

    fn on_clipper_grid(mut self) -> Shape2d {
        for p in self.contours.iter_mut().flatten() {
            p[0] = (p[0] * 1e8).round() / 1e8;
            p[1] = (p[1] * 1e8).round() / 1e8;
        }
        self
    }

    /// The separate pieces of the shape, each an outline with its holes, in the kernel's order
    /// (1.x `CrossSection.decompose`).
    pub fn decompose(&self) -> Vec<Shape2d> {
        self.to_cross_section().decompose().iter().map(|c| Shape2d::from_cross_section(c).on_clipper_grid()).collect()
    }

    /// Net area: outer contours add, holes subtract.
    pub fn area(&self) -> f64 {
        self.to_cross_section().area()
    }

    /// `(min, max)` corners, or `None` for an empty shape.
    pub fn bounds(&self) -> Option<([f64; 2], [f64; 2])> {
        let c = self.to_cross_section();
        if c.is_empty() {
            return None;
        }
        let b = c.bounds();
        Some(([b.min.x, b.min.y], [b.max.x, b.max.y]))
    }

    pub fn is_empty(&self) -> bool {
        self.to_cross_section().is_empty()
    }

    fn to_cross_section(&self) -> CrossSection {
        CrossSection::new(self.to_polygons())
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
