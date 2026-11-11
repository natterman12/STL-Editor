//! `geometry.simplify` from 1.x: Fast Quadric simplification, then weld and repair into a
//! solid, with Manifold's own simplification as the fallback for a solid body.

mod fqms;

pub use fqms::fqms_simplify;

use stl_kernel::{MeshArrays, Solid};

use crate::{quick_repair, weld};

/// What `simplify` gives back: a solid, or the welded triangles when they don't close up and
/// the body was not a solid to begin with.
pub enum Simplified {
    Solid(Solid),
    Raw(Vec<[f64; 3]>, Vec<[u32; 3]>),
}

/// `manifold_from_arrays`: the mesh as a solid, or `None` when it isn't a closed 2-manifold.
fn solid_from(v: &[f64], f: &[u32]) -> Option<Solid> {
    if f.is_empty() {
        return None;
    }
    let m = MeshArrays { positions: v.iter().map(|&x| x as f32).collect(), triangles: f.to_vec() };
    Solid::from_mesh(&m).ok()
}

/// Reduce the triangle count. `v` and `f` are the body's mesh and `solid` its solid, if it is
/// one. `target_ratio` is the fraction of triangles to keep (default 0.5, clamped to
/// [0.001, 1]); a `tolerance` without a ratio simplifies the solid with Manifold instead.
pub fn simplify(
    v: &[[f32; 3]],
    f: &[[u32; 3]],
    solid: Option<&Solid>,
    target_ratio: Option<f64>,
    tolerance: Option<f64>,
) -> Simplified {
    if let (Some(tol), Some(s), None) = (tolerance, solid, target_ratio) {
        return Simplified::Solid(s.simplify(tol));
    }
    let ratio = target_ratio.unwrap_or(0.5).clamp(0.001, 1.0);
    let (sv, sf) = fqms_simplify(v, f, 1.0 - ratio);
    let flat_v: Vec<f64> = sv.iter().flatten().copied().collect();
    let flat_f: Vec<u32> = sf.iter().flatten().copied().collect();
    let (nv, nf) = weld(&flat_v, &flat_f, 1e-5);
    let mut man = solid_from(&nv, &nf);
    if man.is_none() {
        let (rv, rf) = quick_repair(&nv, &nf);
        man = solid_from(&rv, &rf);
    }
    if let (None, Some(s)) = (&man, solid) {
        // fall back to Manifold's own (topology-safe) simplification
        let axis = |a: usize| {
            let hi = v.iter().map(|p| p[a]).fold(f32::NEG_INFINITY, f32::max);
            let lo = v.iter().map(|p| p[a]).fold(f32::INFINITY, f32::min);
            if v.is_empty() { 0.0 } else { hi - lo }
        };
        let d = [axis(0), axis(1), axis(2)];
        // np.linalg.norm of a float32 vector stays in float32
        let diag = f64::from((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt());
        let mut tol = diag * 1e-3;
        let limit = f.len() as f64 * ratio * 1.15;
        let mut out = s.simplify(tol);
        for _ in 1..12 {
            if out.num_tri() as f64 <= limit {
                break;
            }
            tol *= 1.8;
            out = s.simplify(tol);
        }
        man = Some(out);
    }
    match man {
        Some(s) => Simplified::Solid(s),
        None => Simplified::Raw(
            nv.chunks_exact(3).map(|p| [p[0], p[1], p[2]]).collect(),
            nf.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect(),
        ),
    }
}
