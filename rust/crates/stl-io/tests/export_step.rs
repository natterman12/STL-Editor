//! Faceted STEP parity with `web/engine/step.js` (golden: rust/tests/parity/export/*/out.step).

mod common;
use common::*;
use std::collections::HashMap;
use stl_kernel::MeshArrays;
use stl_io::{ExportBody, ExportError, ExportFormat, ExportOptions, export_bodies};

/// The STEP golden comes from `step_golden.mjs`, which numbers vertices in order of first use
/// (exact match, -0 equal to 0). STEP edge directions and ids depend on that numbering, so
/// re-index the same way before writing.
fn js_index(m: &MeshArrays) -> MeshArrays {
    let mut seen: HashMap<[u32; 3], u32> = HashMap::new();
    let (mut positions, mut triangles) = (Vec::new(), Vec::new());
    for &t in &m.triangles {
        let i = t as usize * 3;
        let v = [m.positions[i], m.positions[i + 1], m.positions[i + 2]];
        let key = v.map(|x| (x + 0.0).to_bits());
        let id = *seen.entry(key).or_insert_with(|| {
            positions.extend_from_slice(&v);
            (positions.len() / 3 - 1) as u32
        });
        triangles.push(id);
    }
    MeshArrays { positions, triangles }
}

fn step_of(name: &str, case: &serde_json::Value) -> Result<Vec<u8>, ExportError> {
    let bodies: Vec<(String, MeshArrays)> =
        exported_bodies(case).into_iter().map(|(n, m)| (n, js_index(&m))).collect();
    let refs: Vec<ExportBody> = bodies.iter().map(|(n, m)| ExportBody { name: n.clone(), mesh: m }).collect();
    let opts = ExportOptions {
        uuid_seed: 1,
        step_title: golden(name)["base"].as_str().unwrap().to_string(),
        step_date: "2026-01-01T00:00:00".into(),
    };
    export_bodies(&refs, ExportFormat::Step, &opts)
}

#[test]
fn step_matches_js() {
    let (mut checked, mut failures) = (0, Vec::new());
    for (name, case) in cases() {
        if exported_bodies(&case).is_empty() || !golden(&name)["step_error"].is_null() {
            continue;
        }
        let got = match step_of(&name, &case) {
            Ok(b) => b,
            Err(e) => {
                failures.push(format!("{name}: unexpected error {e:?}"));
                continue;
            }
        };
        checked += 1;
        match golden_file(&name, "out.step") {
            (Some(want), _) => {
                // The golden may have been checked out with CRLF.
                let want: Vec<u8> = String::from_utf8(want).unwrap().replace("\r\n", "\n").into_bytes();
                if got != want {
                    let (g, w) = (String::from_utf8_lossy(&got), String::from_utf8_lossy(&want));
                    let line = g.lines().zip(w.lines()).position(|(a, b)| a != b);
                    failures.push(format!("{name}: STEP differs (first differing line {line:?})"));
                }
            }
            (None, Some(entry)) => {
                if entry["sha256"].as_str() != Some(sha256_hex(&got).as_str()) {
                    failures.push(format!("{name}: STEP sha256 differs from golden"));
                }
            }
            _ => failures.push(format!("{name}: no STEP golden")),
        }
    }
    println!("step cases checked: {checked}");
    assert!(checked > 0);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn step_errors_match_js() {
    let mut checked = 0;
    for (name, case) in cases() {
        let Some(want) = golden(&name)["step_error"].as_str().map(str::to_string) else { continue };
        checked += 1;
        match step_of(&name, &case) {
            Err(ExportError::Geometry(m)) => assert_eq!(m, want, "{name}"),
            other => panic!("{name}: expected error {want:?}, got {:?}", other.map(|b| b.len())),
        }
    }
    assert!(checked > 0, "no step_error case");
}
