//! `Solid::union_all` puts parts whose boxes do not overlap together first (a compose per group)
//! and unites the results. The groups are composed side by side, with their mesh IDs set aside in
//! group order. The meshes must be the ones the one-after-another loop gave: these hashes were
//! recorded from that loop (the code before the parallel groups), on the same cases. The hash
//! covers every position and index and, for every triangle, the rank of its mesh ID among the
//! solid's IDs, which is where an ID handed out in another order would show.

use stl_kernel::Solid;

/// FNV-1a over the little-endian bits of every position, index and mesh-ID rank.
fn hash(s: &Solid) -> (usize, u64) {
    let m = s.to_mesh();
    let mut h = 0xcbf29ce484222325u64;
    let mut eat = |bytes: &[u8]| {
        for b in bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    };
    for p in &m.positions {
        eat(&p.to_bits().to_le_bytes());
    }
    for t in &m.triangles {
        eat(&t.to_le_bytes());
    }
    for r in s.mesh_id_ranks() {
        eat(&r.to_le_bytes());
    }
    (m.triangles.len() / 3, h)
}

fn part() -> Solid {
    Solid::cube([10.0; 3], true).difference(&Solid::sphere(6.0, 48))
}

/// Nine copies in a row, 8 mm apart: every second one is clear of the ones before it, so
/// the row makes two groups of more than one part.
fn row() -> Vec<Solid> {
    let p = part();
    (0..9)
        .map(|i| {
            let a = (7.0 * i as f64).to_radians();
            let (c, s) = (a.cos(), a.sin());
            p.transform([[c, -s, 0.0, 8.0 * i as f64], [s, c, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]])
        })
        .collect()
}

/// Twelve copies round an axis, like Pattern circular.
fn ring() -> Vec<Solid> {
    let p = part().translate([12.0, 0.0, 0.0]);
    (0..12)
        .map(|i| {
            let a = (30.0 * i as f64).to_radians();
            let (c, s) = (a.cos(), a.sin());
            p.transform([[c, -s, 0.0, 0.0], [s, c, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]])
        })
        .collect()
}

#[test]
fn union_of_a_row_is_unchanged() {
    let u = Solid::union_all(&row());
    assert_eq!(hash(&u), EXPECTED_ROW, "row: {:?}", hash(&u));
}

#[test]
fn union_of_a_ring_is_unchanged() {
    let u = Solid::union_all(&ring());
    assert_eq!(hash(&u), EXPECTED_RING, "ring: {:?}", hash(&u));
}

/// Twice in a row gives the same bytes: the order the groups finish in does not show.
#[test]
fn union_of_groups_repeats() {
    for parts in [row(), ring()] {
        let a = hash(&Solid::union_all(&parts));
        for _ in 0..4 {
            assert_eq!(hash(&Solid::union_all(&parts)), a);
        }
    }
}

const EXPECTED_ROW: (usize, u64) = (11042, 11494398696467308062);
/// The ring's bytes differ between debug and release builds of the kernel (the code before the
/// parallel groups gives the same two values), so each build has its own recorded hash.
const EXPECTED_RING: (usize, u64) =
    if cfg!(debug_assertions) { (15682, 4315223761225570891) } else { (15682, 17949423041799652355) };
