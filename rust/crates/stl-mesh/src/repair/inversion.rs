//! trimesh `repair.fix_inversion`: turn a closed shell the right way out when its volume is
//! negative.

use super::{edge, edge_key, face_components, is_watertight, volume};

/// `graph.is_watertight` on the edges of some of the faces only: every edge is used exactly
/// twice, and the two uses run in opposite directions.
fn group_is_closed_and_wound(f: &[u32], faces: &[u32]) -> bool {
    let mut edges: Vec<(u32, u32)> =
        faces.iter().flat_map(|&t| (0..3).map(move |k| edge(f, 3 * t as usize + k))).collect();
    edges.sort_by_key(|&e| edge_key(e));
    let mut i = 0;
    while i < edges.len() {
        if i + 1 >= edges.len() || edge_key(edges[i + 1]) != edge_key(edges[i]) {
            return false;
        }
        if i + 2 < edges.len() && edge_key(edges[i + 2]) == edge_key(edges[i]) {
            return false;
        }
        if edges[i].1 != edges[i + 1].0 {
            return false;
        }
        i += 2;
    }
    true
}

/// trimesh `repair.fix_inversion`, in place. Without `multibody` the whole mesh is reversed
/// when it is watertight with a negative volume; with it, every connected group of faces that
/// is closed and consistently wound is checked on its own.
pub fn fix_inversion(v: &[f64], f: &mut Vec<u32>, multibody: bool) {
    if !multibody {
        if f.len() >= 3 && is_watertight(f).0 && volume(v, f, None) < 0.0 {
            for t in f.chunks_exact_mut(3) {
                t.swap(0, 2);
            }
        }
        return;
    }
    let mut flip: Vec<u32> = Vec::new();
    for group in face_components(f, false) {
        // the smallest closed body is a tetrahedron
        if group.len() < 4 || !group_is_closed_and_wound(f, &group) {
            continue;
        }
        // a negative volume means the body is inside out (or garbage)
        if volume(v, f, Some(&group)) < 0.0 {
            flip.extend_from_slice(&group);
        }
    }
    for t in flip {
        let k = 3 * t as usize;
        f.swap(k, k + 2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // a tetrahedron on the origin, facing inwards
    const V: [f64; 12] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    const INSIDE_OUT: [u32; 12] = [0, 1, 2, 0, 3, 1, 1, 3, 2, 2, 3, 0];

    #[test]
    fn turns_an_inside_out_shell() {
        for multibody in [false, true] {
            let mut f = INSIDE_OUT.to_vec();
            assert!(volume(&V, &f, None) < 0.0);
            fix_inversion(&V, &mut f, multibody);
            assert!(volume(&V, &f, None) > 0.0);
            let before = f.clone();
            fix_inversion(&V, &mut f, multibody);
            assert_eq!(f, before);
        }
    }

    #[test]
    fn leaves_an_open_shell_alone() {
        let mut f = INSIDE_OUT[..9].to_vec();
        fix_inversion(&V, &mut f, true);
        assert_eq!(f, INSIDE_OUT[..9].to_vec());
        fix_inversion(&V, &mut f, false);
        assert_eq!(f, INSIDE_OUT[..9].to_vec());
    }
}
