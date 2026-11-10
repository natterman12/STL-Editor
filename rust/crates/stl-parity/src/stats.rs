//! Mesh statistics. Twin of `dev/parity/stats.py`: keep the formulas identical.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MeshStats {
    pub triangles: usize,
    pub vertices: usize,
    pub volume: f64,
    pub area: f64,
    pub bbox_min: [f64; 3],
    pub bbox_max: [f64; 3],
    pub watertight: bool,
}

pub fn mesh_stats(positions: &[f32], triangles: &[u32]) -> MeshStats {
    let vertices = positions.len() / 3;
    let n_tri = triangles.len() / 3;
    if n_tri == 0 {
        return MeshStats {
            triangles: 0,
            vertices,
            volume: 0.0,
            area: 0.0,
            bbox_min: [0.0; 3],
            bbox_max: [0.0; 3],
            watertight: false,
        };
    }
    let p = |i: u32| -> [f64; 3] {
        let k = i as usize * 3;
        [positions[k] as f64, positions[k + 1] as f64, positions[k + 2] as f64]
    };
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let mut volume = 0.0f64;
    let mut area = 0.0f64;
    let mut edges: HashMap<(u32, u32), u32> = HashMap::new();
    for t in triangles.chunks_exact(3) {
        let (v0, v1, v2) = (p(t[0]), p(t[1]), p(t[2]));
        let c = cross(v1, v2);
        volume += v0[0] * c[0] + v0[1] * c[1] + v0[2] * c[2];
        let e1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
        let e2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
        let n = cross(e1, e2);
        area += (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *edges.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    let mut bbox_min = [f64::INFINITY; 3];
    let mut bbox_max = [f64::NEG_INFINITY; 3];
    for v in positions.chunks_exact(3) {
        for k in 0..3 {
            bbox_min[k] = bbox_min[k].min(v[k] as f64);
            bbox_max[k] = bbox_max[k].max(v[k] as f64);
        }
    }
    MeshStats {
        triangles: n_tri,
        vertices,
        volume: volume / 6.0,
        area: 0.5 * area,
        bbox_min,
        bbox_max,
        watertight: edges.values().all(|&c| c == 2),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const CUBE_POS: [f32; 24] = [
        0., 0., 0., 10., 0., 0., 10., 10., 0., 0., 10., 0., 0., 0., 10., 10., 0., 10., 10., 10.,
        10., 0., 10., 10.,
    ];
    pub(crate) const CUBE_TRI: [u32; 36] = [
        0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 1, 2, 6, 1, 6, 5, 2, 3, 7, 2, 7, 6,
        3, 0, 4, 3, 4, 7,
    ];

    #[test]
    fn cube_stats() {
        let s = mesh_stats(&CUBE_POS, &CUBE_TRI);
        assert_eq!(s.triangles, 12);
        assert_eq!(s.vertices, 8);
        assert!((s.volume - 1000.0).abs() < 1e-9);
        assert!((s.area - 600.0).abs() < 1e-9);
        assert_eq!(s.bbox_min, [0.0; 3]);
        assert_eq!(s.bbox_max, [10.0; 3]);
        assert!(s.watertight);
    }

    #[test]
    fn missing_triangle_is_not_watertight() {
        let s = mesh_stats(&CUBE_POS, &CUBE_TRI[..33]);
        assert_eq!(s.triangles, 11);
        assert!(!s.watertight);
    }

    #[test]
    fn empty_mesh_is_zeros() {
        let s = mesh_stats(&[], &[]);
        assert_eq!(s.triangles, 0);
        assert_eq!(s.volume, 0.0);
        assert_eq!(s.bbox_max, [0.0; 3]);
        assert!(!s.watertight);
    }
}
