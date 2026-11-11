//! Expected triangles come from 1.x: manifold3d 3.5.4 `triangulate([...])`.
use stl_kernel::triangulate;

#[test]
fn square_gives_two_triangles_in_1x_order() {
    let t = triangulate(&[vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]]);
    assert_eq!(t, vec![[0, 1, 3], [1, 2, 3]]);
}

#[test]
fn square_with_hole_indexes_across_contours() {
    let t = triangulate(&[
        vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]],
        vec![[1.0, 1.0], [1.0, 3.0], [3.0, 3.0], [3.0, 1.0]],
    ]);
    assert_eq!(
        t,
        vec![[2, 6, 7], [1, 2, 7], [5, 6, 2], [5, 2, 3], [1, 7, 4], [0, 1, 4], [4, 5, 3], [4, 3, 0]]
    );
}

#[test]
fn l_shape_matches_1x() {
    let t = triangulate(&[vec![[0.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0], [1.0, 2.0], [0.0, 2.0]]]);
    assert_eq!(t, vec![[3, 4, 5], [3, 5, 0], [1, 2, 3], [0, 1, 3]]);
}

#[test]
fn empty_gives_nothing() {
    assert!(triangulate(&[]).is_empty());
}
