//! Orthonormal sketch/workplane frame shared by the engine and mesh refs.

pub type Vec3 = [f64; 3];

/// An orthonormal right-handed frame: origin, x, y and normal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub o: Vec3,
    pub x: Vec3,
    pub y: Vec3,
    pub n: Vec3,
}

pub fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
