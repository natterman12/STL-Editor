//! Matching rules (spec 5.3) between a golden `MeshStats` and a result.

use crate::MeshStats;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exactness {
    Exact,
    Approx,
}

/// |d| <= 1e-4 (absolute) or |d| <= 1e-3 * |golden| (0.1% relative).
pub fn close(golden: f64, got: f64) -> bool {
    let d = (got - golden).abs();
    d <= 1e-4 || d <= 1e-3 * golden.abs()
}

pub fn compare_mesh(golden: &MeshStats, got: &MeshStats, ex: Exactness) -> Vec<String> {
    let mut out = Vec::new();
    let tri_ok = match ex {
        Exactness::Exact => golden.triangles == got.triangles,
        // within 5%: 20 * |d| <= golden, in integers
        Exactness::Approx => golden.triangles.abs_diff(got.triangles) * 20 <= golden.triangles,
    };
    if !tri_ok {
        out.push(format!("triangles: golden {} got {}", golden.triangles, got.triangles));
    }
    let mut num = |name: String, g: f64, v: f64| {
        if !close(g, v) {
            out.push(format!("{name}: golden {g} got {v}"));
        }
    };
    num("volume".into(), golden.volume, got.volume);
    num("area".into(), golden.area, got.area);
    for k in 0..3 {
        num(format!("bbox_min[{k}]"), golden.bbox_min[k], got.bbox_min[k]);
    }
    for k in 0..3 {
        num(format!("bbox_max[{k}]"), golden.bbox_max[k], got.bbox_max[k]);
    }
    if golden.watertight != got.watertight {
        out.push(format!("watertight: golden {} got {}", golden.watertight, got.watertight));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> MeshStats {
        MeshStats {
            triangles: 100,
            vertices: 60,
            volume: 1000.0,
            area: 600.0,
            bbox_min: [0.0; 3],
            bbox_max: [10.0; 3],
            watertight: true,
        }
    }

    #[test]
    fn volume_within_tolerance_passes() {
        let mut g = base();
        g.volume = 1000.9;
        assert!(compare_mesh(&base(), &g, Exactness::Exact).is_empty());
    }

    #[test]
    fn volume_out_of_tolerance_fails() {
        let mut g = base();
        g.volume = 1001.1;
        assert_eq!(
            compare_mesh(&base(), &g, Exactness::Exact),
            vec!["volume: golden 1000 got 1001.1".to_string()]
        );
    }

    #[test]
    fn absolute_tolerance_near_zero() {
        assert!(close(0.0, 0.00009));
        assert!(!close(0.0, 0.0002));
    }

    #[test]
    fn approx_triangles() {
        let mut g = base();
        g.triangles = 105;
        assert!(compare_mesh(&base(), &g, Exactness::Approx).is_empty());
        g.triangles = 106;
        assert_eq!(
            compare_mesh(&base(), &g, Exactness::Approx),
            vec!["triangles: golden 100 got 106".to_string()]
        );
    }

    #[test]
    fn exact_triangles() {
        let mut g = base();
        g.triangles = 101;
        assert_eq!(compare_mesh(&base(), &g, Exactness::Exact).len(), 1);
    }

    #[test]
    fn watertight_must_match() {
        let mut g = base();
        g.watertight = false;
        assert_eq!(
            compare_mesh(&base(), &g, Exactness::Exact),
            vec!["watertight: golden true got false".to_string()]
        );
    }

    #[test]
    fn vertices_never_compared_and_bbox_per_component() {
        let mut g = base();
        g.vertices = 1;
        assert!(compare_mesh(&base(), &g, Exactness::Exact).is_empty());
        g.bbox_max[1] = 11.0;
        assert_eq!(
            compare_mesh(&base(), &g, Exactness::Exact),
            vec!["bbox_max[1]: golden 10 got 11".to_string()]
        );
    }
}
