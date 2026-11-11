//! 1.x `printing.pushpull_tool`: the solid a Push/Pull adds or removes.

use stl_kernel::{Frame, Shape2d, Solid};
use stl_sketch::P2;

use super::profiles::frame_matrix;
use crate::FeatureError;

/// Push or pull a flat face: the face's `contours` (even-odd, in the face's own 2D
/// coordinates) become a prism `|distance|` thick standing on the face plane, on the normal's
/// side for a positive distance and behind it for a negative one. `frame` places the face.
pub fn pushpull_tool(contours: &[Vec<P2>], frame: &Frame, distance: f64) -> Result<Solid, FeatureError> {
    let cs = Shape2d::from_even_odd(contours);
    if cs.is_empty() {
        return Err(FeatureError("That face has no area.".into()));
    }
    if distance.abs() < 1e-9 {
        return Err(FeatureError("Distance can't be zero.".into()));
    }
    let mut man = cs.extrude(distance.abs());
    if distance < 0.0 {
        man = man.translate([0.0, 0.0, -distance.abs()]);
    }
    Ok(man.transform(frame_matrix(frame)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xy() -> Frame {
        Frame { o: [0.0; 3], x: [1.0, 0.0, 0.0], y: [0.0, 1.0, 0.0], n: [0.0, 0.0, 1.0] }
    }

    #[test]
    fn push_and_pull() {
        let sq = vec![vec![[0.0, 0.0], [4.0, 0.0], [4.0, 3.0], [0.0, 3.0]]];
        let up = pushpull_tool(&sq, &xy(), 2.0).unwrap();
        assert!((up.volume() - 24.0).abs() < 1e-9);
        assert_eq!(up.bounding_box().min[2], 0.0);
        let down = pushpull_tool(&sq, &xy(), -2.0).unwrap();
        assert_eq!(down.bounding_box().min[2], -2.0);
        assert_eq!(down.bounding_box().max[2], 0.0);
    }

    #[test]
    fn errors_and_holes() {
        let sq = vec![vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]]];
        assert_eq!(pushpull_tool(&sq, &xy(), 0.0).err().unwrap().0, "Distance can't be zero.");
        assert_eq!(pushpull_tool(&[], &xy(), 1.0).err().unwrap().0, "That face has no area.");
        let ring = vec![
            vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]],
            vec![[1.0, 1.0], [3.0, 1.0], [3.0, 3.0], [1.0, 3.0]],
        ];
        assert!((pushpull_tool(&ring, &xy(), 1.0).unwrap().volume() - 12.0).abs() < 1e-9);
    }
}
