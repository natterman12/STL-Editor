//! `geometry.quick_repair` from 1.x.

use super::process::{keep_faces, nondegenerate_faces, unique_faces};
use super::{fill_holes, fix_inversion, fix_winding, process};
use crate::weld;

/// Weld, remove degenerate and duplicate faces, unify winding, fill small holes. Takes the raw
/// triangles; the result may still be open (the caller checks whether it is a solid).
pub fn quick_repair(v: &[f64], f: &[u32]) -> (Vec<f64>, Vec<u32>) {
    let (v, f) = weld(v, f, 1e-5);
    let (mut v, mut f) = process(&v, &f);
    // trimesh leaves an empty mesh alone in each of the next three calls
    if !v.is_empty() && !f.is_empty() {
        f = keep_faces(&f, &nondegenerate_faces(&v, &f));
    }
    if !v.is_empty() && !f.is_empty() {
        f = keep_faces(&f, &unique_faces(&f));
    }
    if !v.is_empty() && !f.is_empty() {
        // remove_unreferenced_vertices
        let nv = v.len() / 3;
        let mut referenced = vec![false; nv];
        for &i in &f {
            referenced[i as usize] = true;
        }
        if referenced.iter().any(|r| !r) {
            let mut inverse = vec![0u32; nv];
            let mut kept = Vec::new();
            for i in 0..nv {
                if referenced[i] {
                    inverse[i] = (kept.len() / 3) as u32;
                    kept.extend_from_slice(&v[3 * i..3 * i + 3]);
                }
            }
            for i in f.iter_mut() {
                *i = inverse[*i as usize];
            }
            v = kept;
        }
    }
    fix_winding(&v, &mut f);
    fix_inversion(&v, &mut f, true);
    fill_holes(&mut v, &mut f);
    // fix_normals(multibody=True)
    fix_winding(&v, &mut f);
    fix_inversion(&v, &mut f, true);
    (v, f)
}
