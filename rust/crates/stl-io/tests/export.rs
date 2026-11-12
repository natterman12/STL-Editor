//! Export parity with 1.x `geometry.export_bodies` (golden: rust/tests/parity/export/).

mod common;
use common::*;
use stl_io::ExportFormat;
use stl_kernel::MeshArrays;

#[test]
fn sha256_known() {
    assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
}

#[test]
fn stl_matches_1x() {
    let mut checked = 0;
    let mut failures = Vec::new();
    for (name, case) in cases() {
        let bodies = exported_bodies(&case);
        if bodies.is_empty() {
            continue;
        }
        let got = export_bodies_of(&bodies, ExportFormat::Stl);
        checked += 1;
        match golden_file(&name, "out.stl") {
            (Some(want), _) => {
                if got != want {
                    failures.push(format!("{name}: STL bytes differ ({} vs {} bytes)", got.len(), want.len()));
                }
            }
            (None, Some(entry)) => {
                if entry["size"].as_u64() != Some(got.len() as u64) {
                    failures.push(format!("{name}: STL size {} vs golden {}", got.len(), entry["size"]));
                }
                if entry["sha256"].as_str() != Some(sha256_hex(&got).as_str()) {
                    failures.push(format!("{name}: STL sha256 differs from golden"));
                }
            }
            _ => failures.push(format!("{name}: no STL golden")),
        }
    }
    println!("stl cases checked: {checked}");
    assert!(checked > 0, "no case with bodies");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Hand-made mesh, expected bytes from 1.x `geometry.stl_bytes` (float32 numpy): includes a
/// zero-area triangle (normal 0), -0.0, 1e-5 and 123456789.
#[test]
fn stl_hand_made_matches_1x() {
    let mesh = MeshArrays {
        positions: vec![
            0.0, 0.0, 0.0, 1.1, 0.0, 0.0, 0.0, 2.3, 0.0, 0.0, 0.0, 3.7, -0.0, 1e-5, 123456789.0, 2.0, 0.0, 0.0,
            4.0, 0.0, 0.0,
        ],
        triangles: vec![0, 1, 2, 0, 1, 3, 0, 3, 2, 1, 2, 3, 0, 5, 6, 4, 1, 3],
    };
    let got = export_bodies_of(&[("a".into(), mesh)], ExportFormat::Stl);
    let want = "0600000000000000000000000000803f000000000000000000000000cdcc8c3f0000000000000000000000003333134000000000000000000000000080bf00000000000000000000000000000000cdcc8c3f00000000000000000000000000000000cdcc6c400000000080bf00000000000000000000000000000000000000000000000000000000cdcc6c4000000000333313400000000000001d105f3f535dd53ecca1843ecdcc8c3f00000000000000000000000033331340000000000000000000000000cdcc6c4000000000000000000000000000000000000000000000000000000000004000000000000000000000804000000000000000000000000000000000803f5165b6a900000080acc52737a379eb4ccdcc8c3f00000000000000000000000000000000cdcc6c400000";
    let hex: String = got[80..].iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(hex, want);
    assert_eq!(&got[..80], &{
        let mut h = [b' '; 80];
        h[..21].copy_from_slice(b"STL Editor binary STL");
        h
    }[..]);
}
