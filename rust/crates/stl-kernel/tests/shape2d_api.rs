use stl_kernel::{Shape2d, Solid};

#[test]
fn simplify_removes_collinear_midpoint() {
    let shape = Shape2d { contours: vec![vec![[0.0, 0.0], [5.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]] };
    let s = shape.simplify(0.02);
    assert_eq!(s.contours.len(), 1);
    assert_eq!(s.contours[0].len(), 4);
    assert!((s.area() - 100.0).abs() < 1e-9);
}

#[test]
fn project_cube_is_its_footprint() {
    let p = Solid::cube([4.0, 3.0, 5.0], false).project();
    assert_eq!(p.contours.len(), 1);
    assert_eq!(p.contours[0].len(), 4);
    assert!((p.area() - 12.0).abs() < 1e-9);
    let (lo, hi) = p.bounds().unwrap();
    assert_eq!((lo, hi), ([0.0, 0.0], [4.0, 3.0]));
}

#[test]
fn from_even_odd_cuts_a_hole_where_contours_overlap() {
    let a = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
    let b = vec![[5.0, 5.0], [15.0, 5.0], [15.0, 15.0], [5.0, 15.0]];
    let s = Shape2d::from_even_odd(&[a, b]);
    // Two 100 mm² squares minus the 25 mm² overlap counted twice: 200 - 2 * 25.
    assert!((s.area() - 150.0).abs() < 1e-9, "area {}", s.area());
}

#[test]
fn from_even_odd_does_not_snap_to_the_grid() {
    let x = 0.123456789012;
    let tri = vec![[x, 0.0], [10.0, 0.0], [0.0, 10.0]];
    let s = Shape2d::from_even_odd(&[tri]);
    // Clipper2 itself keeps the vertex near x (within 1e-8) but the kernel adds no 1e-8 rounding,
    // so it does not land on the grid value 0.12345679.
    let v = s.contours.iter().flatten().find(|p| (p[0] - x).abs() < 1e-8 && p[1] == 0.0).expect("vertex near x");
    assert_ne!(v[0], (x * 1e8).round() / 1e8, "{:?}", s.contours);
}

fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Shape2d {
    Shape2d { contours: vec![vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]] }
}

#[test]
fn intersection_and_difference_with_a_rect_split_a_shape() {
    let s = square(-4.0, 0.0, 6.0, 10.0);
    let right = s.intersection_rect([0.0, -20.0], [20.0, 20.0]);
    let left = s.difference_rect([0.0, -20.0], [20.0, 20.0]);
    assert!((right.area() - 60.0).abs() < 1e-9, "{}", right.area());
    assert!((left.area() - 40.0).abs() < 1e-9, "{}", left.area());
    assert_eq!(right.bounds().unwrap(), ([0.0, 0.0], [6.0, 10.0]));
}

#[test]
fn mirror_x_flips_across_the_y_axis() {
    let m = square(1.0, 0.0, 3.0, 2.0).mirror_x();
    assert!((m.area() - 4.0).abs() < 1e-9);
    assert_eq!(m.bounds().unwrap(), ([-3.0, 0.0], [-1.0, 2.0]));
}

#[test]
fn extrude_scaled_tapers_and_cones() {
    let s = square(-5.0, -5.0, 5.0, 5.0);
    // frustum: h/3 * (A1 + A2 + sqrt(A1 A2)) = 10/3 * (100 + 25 + 50)
    let f = s.extrude_scaled(10.0, 0.5);
    assert!((f.volume() - 10.0 / 3.0 * 175.0).abs() < 1e-6, "{}", f.volume());
    let cone = s.extrude_scaled(9.0, 0.0);
    assert!((cone.volume() - 300.0).abs() < 1e-6, "{}", cone.volume());
    assert!(s.extrude_scaled(0.0, 0.5).is_empty());
    assert!(s.extrude_scaled(-1.0, 0.5).is_empty());
}
