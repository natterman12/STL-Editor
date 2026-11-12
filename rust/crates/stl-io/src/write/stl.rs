//! Binary STL, as 1.x `geometry.stl_bytes`: the normal is the f32 cross product, normalized in
//! f32 (numpy float32 arithmetic), and 0 when its length is 0.

use super::{ExportBody, ExportError};

const HEADER: &[u8] = b"STL Editor binary STL";

pub(super) fn write(bodies: &[ExportBody]) -> Result<Vec<u8>, ExportError> {
    let count: usize = bodies.iter().map(|b| b.mesh.triangles.len() / 3).sum();
    let mut out = Vec::with_capacity(84 + count * 50);
    out.extend_from_slice(HEADER);
    out.resize(80, b' ');
    out.extend_from_slice(&(count as u32).to_le_bytes());
    for b in bodies {
        let p = &b.mesh.positions;
        let vert = |i: u32| -> [f32; 3] {
            let i = i as usize * 3;
            [p[i], p[i + 1], p[i + 2]]
        };
        for t in b.mesh.triangles.chunks_exact(3) {
            let (v0, v1, v2) = (vert(t[0]), vert(t[1]), vert(t[2]));
            let a = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
            let c = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
            let mut n = [
                a[1] * c[2] - a[2] * c[1],
                a[2] * c[0] - a[0] * c[2],
                a[0] * c[1] - a[1] * c[0],
            ];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len > 0.0 {
                n = [n[0] / len, n[1] / len, n[2] / len];
            } else {
                n = [0.0; 3];
            }
            for v in n.iter().chain(&v0).chain(&v1).chain(&v2) {
                out.extend_from_slice(&v.to_le_bytes());
            }
            out.extend_from_slice(&[0, 0]);
        }
    }
    Ok(out)
}
