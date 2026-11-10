//! Reader parity with 1.x `load_mesh_file` (golden: rust/tests/parity/readers_golden.json).

use std::path::PathBuf;

use serde_json::Value;
use stl_io::load_mesh_file;
use stl_mesh::weld;
use stl_parity::{Exactness, MeshStats, compare_mesh, mesh_stats};

fn tests_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests")
}

#[test]
fn readers_match_golden() {
    let path = tests_dir().join("parity/readers_golden.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        panic!("{} is missing: run dev/parity/make_reader_golden.py", path.display());
    };
    let golden: Value = serde_json::from_str(&text).unwrap();
    let mut failures = Vec::new();
    let mut checked = 0;
    for (file, want) in golden.as_object().unwrap() {
        let model = tests_dir().join("models/readers").join(file);
        let data = std::fs::read(&model).unwrap_or_else(|e| panic!("{}: {e}", model.display()));
        let got = load_mesh_file(&data, file);
        checked += 1;
        if let Some(err) = want.get("error") {
            match got {
                Err(e) if Value::String(e.0.clone()) == *err => {}
                other => failures.push(format!("{file}: expected error {err}, got {other:?}")),
            }
            continue;
        }
        let parts = match got {
            Ok(p) => p,
            Err(e) => {
                failures.push(format!("{file}: unexpected error {:?}", e.0));
                continue;
            }
        };
        let wp = want["parts"].as_array().unwrap();
        if wp.len() != parts.len() {
            failures.push(format!("{file}: {} parts, golden has {}", parts.len(), wp.len()));
            continue;
        }
        for (i, (w, p)) in wp.iter().zip(&parts).enumerate() {
            if w["name"].as_str() != Some(p.name.as_str()) {
                failures.push(format!("{file}[{i}]: name {:?}, golden {}", p.name, w["name"]));
            }
            let gs: MeshStats = serde_json::from_value(w["stats"].clone()).unwrap();
            let pos32: Vec<f32> = p.positions.iter().map(|v| *v as f32).collect();
            let gs_got = mesh_stats(&pos32, &p.triangles);
            for d in compare_mesh(&gs, &gs_got, Exactness::Exact) {
                failures.push(format!("{file}[{i}]: {d}"));
            }
        }
    }
    println!("readers checked: {checked}");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn weld_matches_numpy_rounding() {
    let tol = 1e-5;
    // numpy: round(0.5e-5 / 1e-5) == round(0.5) == 0 (half to even); 1.5e-5 -> 2; 2.5e-5 -> 2
    assert_eq!(0.5e-5 / tol, 0.5);
    assert_eq!(1.5e-5 / tol, 1.5);
    assert_eq!(2.5e-5 / tol, 2.5);
    // Triangle with corner 0 at x = 0 and corner 1 at x = 0.5e-5 (same cell, 0), corner 2 apart,
    // plus two vertices at 1.5e-5 and 2.5e-5 (cells 2 and 2: merged).
    let p = [
        0.0, 0.0, 0.0, // a
        0.5e-5, 0.0, 0.0, // b: cell 0 with a
        1.0, 0.0, 0.0, // c
        1.5e-5, 5.0, 0.0, // d: cell 2
        2.5e-5, 5.0, 0.0, // e: cell 2 with d
        1.0, 5.0, 1.0, // f
    ];
    let (v, f) = weld(&p, &[0, 2, 3, 1, 2, 5, 3, 4, 5], tol);
    // a and b merge (b would be cell 1 with round-half-away); d and e merge
    assert_eq!(v.len() / 3, 4);
    // the first vertex of a cell supplies the coordinate
    assert_eq!(&v[0..3], &[0.0, 0.0, 0.0]);
    assert_eq!(&v[3..6], &[1.5e-5, 5.0, 0.0]);
    // triangle (3,4,5) collapses to (d,d,f): degenerate and dropped; the others stay
    assert_eq!(f.len(), 6);
}
