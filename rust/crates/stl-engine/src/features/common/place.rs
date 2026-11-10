use std::sync::Arc;

use serde_json::Value;
use stl_kernel::{MeshArrays, Solid};

use super::frames::{cross, dot, unit, Frame, Vec3};
use crate::state::Body;
use crate::FeatureError;

/// 1.x `_rotation`: degrees about X, then Y, then Z, as `Rz @ Ry @ Rx`.
pub fn rotation(rx: f64, ry: f64, rz: f64) -> [[f64; 3]; 3] {
    let (rx, ry, rz) = (rx.to_radians(), ry.to_radians(), rz.to_radians());
    let rxm = [[1.0, 0.0, 0.0], [0.0, rx.cos(), -rx.sin()], [0.0, rx.sin(), rx.cos()]];
    let rym = [[ry.cos(), 0.0, ry.sin()], [0.0, 1.0, 0.0], [-ry.sin(), 0.0, ry.cos()]];
    let rzm = [[rz.cos(), -rz.sin(), 0.0], [rz.sin(), rz.cos(), 0.0], [0.0, 0.0, 1.0]];
    matmul(&matmul(&rzm, &rym), &rxm)
}

fn matmul(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut r = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            r[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    r
}

/// 1.x `_affine`: rotation `r` and translation `t` as three rows of four.
pub fn affine(r: [[f64; 3]; 3], t: Vec3) -> [[f64; 4]; 3] {
    let mut m = [[0.0; 4]; 3];
    for i in 0..3 {
        m[i] = [r[i][0], r[i][1], r[i][2], t[i]];
    }
    m
}

/// 1.x `_place`: move a solid from local coordinates into a frame, shifted by `offset` along
/// the frame's x, y and normal.
pub fn place(s: &Solid, f: &Frame, offset: Vec3) -> Solid {
    let mut m = [[0.0; 4]; 3];
    for i in 0..3 {
        let o2 = f.o[i] + offset[0] * f.x[i] + offset[1] * f.y[i] + offset[2] * f.n[i];
        m[i] = [f.x[i], f.y[i], f.n[i], o2];
    }
    s.transform(m)
}

/// The vertices of a body, solid or raw, as f64 triples.
fn body_points(b: &Body) -> Vec<Vec3> {
    let solid_mesh;
    let mesh: &MeshArrays = match (&b.solid, &b.mesh) {
        (Some(s), _) => {
            solid_mesh = s.to_mesh();
            &solid_mesh
        }
        (None, Some(m)) => m,
        (None, None) => return Vec::new(),
    };
    mesh.positions.chunks_exact(3).map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]).collect()
}

fn bounds(pts: &[Vec3]) -> (Vec3, Vec3) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in pts {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    (lo, hi)
}

/// 1.x `_pivot`: `"origin"`, `"base"` (middle of the bottom of the box) or anything else for
/// the middle of the box.
pub fn pivot(b: &Body, pivot: &Value) -> Result<Vec3, FeatureError> {
    let p = pivot.as_str();
    if p == Some("origin") {
        return Ok([0.0; 3]);
    }
    let (lo, hi) = bounds(&body_points(b));
    if p == Some("base") {
        return Ok([(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, lo[2]]);
    }
    Ok([(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0])
}

fn det3(m: &[[f64; 4]; 3]) -> f64 {
    let c = |j: usize| [m[0][j], m[1][j], m[2][j]];
    dot(c(0), cross(c(1), c(2)))
}

/// 1.x `_transform_body`: move a solid or a raw mesh (a mirror flips a raw mesh's triangles).
pub fn transform_body(b: &Body, m: [[f64; 4]; 3]) -> Body {
    if let Some(s) = &b.solid {
        return Body { name: b.name.clone(), solid: Some(s.transform(m)), mesh: None };
    }
    let Some(mesh) = &b.mesh else {
        return b.clone();
    };
    let mut positions = Vec::with_capacity(mesh.positions.len());
    for p in mesh.positions.chunks_exact(3) {
        let v = [p[0] as f64, p[1] as f64, p[2] as f64];
        for r in &m {
            positions.push((r[0] * v[0] + r[1] * v[1] + r[2] * v[2] + r[3]) as f32);
        }
    }
    let mut triangles = mesh.triangles.clone();
    if det3(&m) < 0.0 {
        for t in triangles.chunks_exact_mut(3) {
            t.swap(0, 2);
        }
    }
    Body { name: b.name.clone(), solid: None, mesh: Some(Arc::new(MeshArrays { positions, triangles })) }
}

/// 1.x `_mirror_matrix`: reflection across the plane of a frame.
pub fn mirror_matrix(f: &Frame) -> [[f64; 4]; 3] {
    let n = unit(f.n).unwrap_or(f.n);
    let d = 2.0 * dot(f.o, n);
    let mut m = [[0.0; 4]; 3];
    for i in 0..3 {
        for j in 0..3 {
            m[i][j] = if i == j { 1.0 } else { 0.0 } - 2.0 * n[i] * n[j];
        }
        m[i][3] = d * n[i];
    }
    m
}
