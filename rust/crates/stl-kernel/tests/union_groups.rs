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
    // the ring's last bits follow the platform's sin and cos (Linux's glibc rounds a few
    // differently), so its hash was recorded on Windows; elsewhere the size is checked and
    // `union_of_groups_repeats` checks the bytes don't depend on the order groups finish in
    if cfg!(windows) {
        assert!(EXPECTED_RING.contains(&hash(&u)), "ring: {:?}", hash(&u));
    } else {
        assert_eq!(hash(&u).0, EXPECTED_RING[0].0, "ring: {:?}", hash(&u));
    }
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
/// The ring's bytes come out as one of two values, by how the kernel was built (the code before
/// the parallel groups gives the same two). The first is what debug builds give, and optimised
/// builds without fat LTO (the test-fast profile CI uses) or with the MSVC toolchain; the second
/// is the release profile (fat LTO) with the GNU toolchain. A test cannot ask which build it is
/// in, so either is accepted; any other value means the union changed.
const EXPECTED_RING: [(usize, u64); 2] = [(15682, 4315223761225570891), (15682, 17949423041799652355)];
