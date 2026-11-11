//! Runs every case in `rust/tests/parity/kernel_cases.json` through `Solid` and compares each
//! stack entry with the 1.x golden stats (`kernel_golden.json`). Twin of
//! `dev/parity/make_kernel_golden.py`: keep the ops identical.

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::Value;
use stl_kernel::{KernelError, MeshArrays, Shape2d, Solid};
use stl_parity::{Exactness, MeshStats, compare_mesh, mesh_stats};

fn tests_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests")
}

fn f(v: &Value) -> f64 {
    v.as_f64().expect("number")
}

fn vec3(v: &Value) -> [f64; 3] {
    let a = v.as_array().expect("array of 3");
    [f(&a[0]), f(&a[1]), f(&a[2])]
}

/// Binary STL -> triangle soup welded by identical f32 bit patterns, first-seen order.
fn load_model(name: &str) -> Result<Solid, KernelError> {
    let data = std::fs::read(tests_dir().join("models").join(name)).expect("model file");
    let n = u32::from_le_bytes(data[80..84].try_into().unwrap()) as usize;
    let mut seen: HashMap<[u32; 3], u32> = HashMap::new();
    let mut positions: Vec<f32> = Vec::new();
    let mut triangles: Vec<u32> = Vec::with_capacity(n * 3);
    for t in 0..n {
        let base = 84 + t * 50 + 12;
        for k in 0..3 {
            let off = base + k * 12;
            let mut c = [0f32; 3];
            for (j, slot) in c.iter_mut().enumerate() {
                *slot = f32::from_le_bytes(data[off + j * 4..off + j * 4 + 4].try_into().unwrap());
            }
            let key = [c[0].to_bits(), c[1].to_bits(), c[2].to_bits()];
            let next = seen.len() as u32;
            let idx = *seen.entry(key).or_insert_with(|| {
                positions.extend_from_slice(&c);
                next
            });
            triangles.push(idx);
        }
    }
    Solid::from_mesh(&MeshArrays { positions, triangles })
}

fn run_case(case: &Value) -> Vec<Solid> {
    let mut stack: Vec<Solid> = Vec::new();
    for op in case["ops"].as_array().expect("ops") {
        let kind = op["op"].as_str().expect("op name");
        match kind {
            "cube" => stack.push(Solid::cube(vec3(&op["size"]), op["center"].as_bool().unwrap())),
            "sphere" => stack.push(Solid::sphere(f(&op["r"]), op["segments"].as_u64().unwrap() as u32)),
            "cylinder" => stack.push(Solid::cylinder(
                f(&op["h"]),
                f(&op["r1"]),
                f(&op["r2"]),
                op["segments"].as_u64().unwrap() as u32,
                op["center"].as_bool().unwrap(),
            )),
            "load" => match load_model(op["model"].as_str().unwrap()) {
                Ok(s) => stack.push(s),
                Err(KernelError::NotClosed) => return Vec::new(),
                Err(e) => panic!("{}: load failed: {e}", case["name"]),
            },
            "union" | "difference" | "intersection" => {
                let b = stack.pop().unwrap();
                let a = stack.pop().unwrap();
                stack.push(match kind {
                    "union" => a.union(&b),
                    "difference" => a.difference(&b),
                    _ => a.intersection(&b),
                });
            }
            "translate" => unary(&mut stack, |s| s.translate(vec3(&op["v"]))),
            "rotate" => unary(&mut stack, |s| s.rotate_deg(vec3(&op["deg"]))),
            "scale" => unary(&mut stack, |s| s.scale(vec3(&op["v"]))),
            "mirror" => unary(&mut stack, |s| s.mirror(vec3(&op["normal"]))),
            "refine_to_length" => unary(&mut stack, |s| s.refine_to_length(f(&op["len"]))),
            "simplify" => unary(&mut stack, |s| s.simplify(f(&op["tolerance"]))),
            "minkowski_sum" | "minkowski_difference" => {
                let b = stack.pop().unwrap();
                let a = stack.pop().unwrap();
                stack.push(match kind {
                    "minkowski_sum" => a.minkowski_sum(&b),
                    _ => a.minkowski_difference(&b),
                });
            }
            "offset_miter_extrude" => {
                let contours: Vec<Vec<[f64; 2]>> = op["contours"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| {
                        c.as_array()
                            .unwrap()
                            .iter()
                            .map(|p| {
                                let a = p.as_array().unwrap();
                                [f(&a[0]), f(&a[1])]
                            })
                            .collect()
                    })
                    .collect();
                let shape = Shape2d::from_even_odd(&contours)
                    .offset_miter(f(&op["delta"]), f(&op["miter_limit"]));
                stack.push(shape.extrude(f(&op["height"])));
            }
            "hull" => unary(&mut stack, |s| s.hull()),
            "trim_by_plane" => {
                unary(&mut stack, |s| s.trim_by_plane(vec3(&op["normal"]), f(&op["offset"])))
            }
            "slice_extrude" => unary(&mut stack, |s| s.slice(f(&op["z"])).extrude(f(&op["height"]))),
            "split_by_plane" => {
                let s = stack.pop().unwrap();
                let (first, second) = s.split_by_plane(vec3(&op["normal"]), f(&op["offset"]));
                stack.push(first);
                stack.push(second);
            }
            "decompose" => {
                let mut parts = stack.pop().unwrap().decompose();
                parts.sort_by(|a, b| b.volume().partial_cmp(&a.volume()).unwrap());
                stack.extend(parts);
            }
            other => panic!("unknown op {other:?} in case {}", case["name"]),
        }
    }
    stack
}

fn unary(stack: &mut [Solid], op: impl FnOnce(&Solid) -> Solid) {
    let top = stack.last_mut().expect("stack not empty");
    *top = op(top);
}

#[test]
fn corpus_matches_golden() {
    let parity = tests_dir().join("parity");
    let cases: Vec<Value> =
        serde_json::from_slice(&std::fs::read(parity.join("kernel_cases.json")).unwrap()).unwrap();
    let golden: HashMap<String, Vec<MeshStats>> =
        serde_json::from_slice(&std::fs::read(parity.join("kernel_golden.json")).unwrap()).unwrap();

    let mut failures: Vec<String> = Vec::new();
    for case in &cases {
        let name = case["name"].as_str().unwrap();
        let ex = match case["exactness"].as_str().unwrap() {
            "exact" => Exactness::Exact,
            "approx" => Exactness::Approx,
            other => panic!("{name}: unknown exactness {other:?}"),
        };
        let Some(want) = golden.get(name) else {
            failures.push(format!("{name}: no golden entry"));
            continue;
        };
        let got = run_case(case);
        if got.len() != want.len() {
            failures.push(format!("{name}: stack length: golden {} got {}", want.len(), got.len()));
            continue;
        }
        for (i, (g, s)) in want.iter().zip(&got).enumerate() {
            let m = s.to_mesh();
            let stats = mesh_stats(&m.positions, &m.triangles);
            for mismatch in compare_mesh(g, &stats, ex) {
                failures.push(format!("{name}: entry {i}: {mismatch}"));
            }
        }
    }
    assert!(failures.is_empty(), "corpus mismatches:\n{}", failures.join("\n"));
}
