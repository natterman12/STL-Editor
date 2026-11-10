//! `geometry.local_repair` from 1.x: fix small defects without remeshing.

use stl_kernel::Solid;

use super::{
    cap_holes, face_components, fill_holes, fix_winding, merge_overlapping_shells, reversed, to_solid,
    touching_shells,
};
use crate::weld;

/// Drop faces on non-manifold edges, unify winding, cap the holes, and keep each shell's
/// original triangles. Takes the raw triangles (it welds them first). Returns a solid, or
/// `None` when the mesh needs more than this.
pub fn local_repair(v: &[f64], f: &[u32]) -> Option<Solid> {
    let (mut v, mut f) = weld(v, f, 1e-5);
    if let Some(touching) = touching_shells(&v, &f) {
        return Some(touching);
    }
    for _ in 0..3 {
        // faces on an edge that more than two faces share go
        let nf = f.len() / 3;
        let key = |k: usize| {
            let (a, b) = (f[k], f[k - k % 3 + (k + 1) % 3]);
            (a.min(b), a.max(b))
        };
        let mut order: Vec<usize> = (0..3 * nf).collect();
        order.sort_by_key(|&k| key(k));
        let mut bad_face = vec![false; nf];
        let mut i = 0;
        while i < order.len() {
            let mut j = i + 1;
            while j < order.len() && key(order[j]) == key(order[i]) {
                j += 1;
            }
            if j - i > 2 {
                for &k in &order[i..j] {
                    bad_face[k / 3] = true;
                }
            }
            i = j;
        }
        if bad_face.iter().any(|&b| b) {
            f = f.chunks_exact(3).zip(&bad_face).filter(|(_, bad)| !**bad).flat_map(|(t, _)| t.iter().copied()).collect();
        }
        fix_winding(&v, &mut f);
        let (cv, cf, _) = cap_holes(&v, &f);
        v = cv;
        f = cf;
        if let Some(man) = to_solid(&v, &f) {
            let man = if man.signed_volume() < 0.0 { to_solid(&v, &reversed(&f)) } else { Some(man) };
            return man.map(|m| merge_overlapping_shells(&m).0);
        }
    }
    // last try: shell by shell, union of the shells that are (or became) closed. The shells
    // are trimesh's `split(only_watertight=False)`, whose `submesh(repair=True)` runs
    // `fill_holes()` on every shell that has four faces or more.
    let mut solids: Vec<Solid> = Vec::new();
    for shell in face_components(&f, true) {
        // the shell as its own mesh: its vertices in index order (trimesh `submesh`)
        let mut used: Vec<u32> =
            shell.iter().flat_map(|&t| f[3 * t as usize..3 * t as usize + 3].iter().copied()).collect();
        used.sort_unstable();
        used.dedup();
        let mut sv: Vec<f64> = used.iter().flat_map(|&i| v[3 * i as usize..3 * i as usize + 3].iter().copied()).collect();
        let mut sf: Vec<u32> = shell
            .iter()
            .flat_map(|&t| f[3 * t as usize..3 * t as usize + 3].iter().map(|i| used.binary_search(i).unwrap_or(0) as u32))
            .collect();
        if sf.len() / 3 >= 4 {
            fill_holes(&mut sv, &mut sf);
        }
        let mut m = to_solid(&sv, &sf);
        if m.is_none() {
            let (cv, cf, _) = cap_holes(&sv, &sf);
            m = to_solid(&cv, &cf);
        }
        let mut m = m?;
        if m.signed_volume() < 0.0 {
            // 1.x reverses the shell as `split` gave it (holes filled, not capped); if that is
            // not a solid, 1.x fails with a Python error, and here the repair gives up
            m = to_solid(&sv, &reversed(&sf))?;
        }
        solids.push(m);
    }
    match solids.len() {
        0 => None,
        1 => solids.pop(),
        _ => Some(Solid::union_all(&solids)),
    }
}
