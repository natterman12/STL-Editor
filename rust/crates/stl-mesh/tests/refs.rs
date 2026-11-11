//! Part-edge refs against `rust/tests/parity/refs_golden.json` (from 1.x `meshtools.ref_geometry`).

use std::path::PathBuf;

use serde_json::Value;
use stl_kernel::{Frame, MeshArrays, Solid};
use stl_mesh::refs::{
    ents_from_polys, fit_circle, outline, outline_entities, ref_geometry_max, round_py,
    OutlineEntity, RefEntity,
};
use stl_mesh::weld;

fn tests_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests")
}

fn golden() -> Value {
    let path = tests_dir().join("parity/refs_golden.json");
    let text = std::fs::read_to_string(&path).expect("refs_golden.json");
    serde_json::from_str(&text).expect("parse refs_golden.json")
}

fn read_binary_stl(name: &str) -> (Vec<f64>, Vec<u32>) {
    let data = std::fs::read(tests_dir().join("models").join(name)).expect("model file");
    let n = u32::from_le_bytes(data[80..84].try_into().unwrap()) as usize;
    assert_eq!(data.len(), 84 + 50 * n, "{name} is not a binary STL");
    let mut positions = Vec::with_capacity(n * 9);
    let mut triangles = Vec::with_capacity(n * 3);
    for t in 0..n {
        let rec = &data[84 + 50 * t + 12..84 + 50 * t + 48];
        let base = (positions.len() / 3) as u32;
        for b in rec.chunks_exact(4) {
            positions.push(f32::from_le_bytes(b.try_into().unwrap()) as f64);
        }
        triangles.extend_from_slice(&[base, base + 1, base + 2]);
    }
    weld(&positions, &triangles, 1e-5)
}

fn load_solid(name: &str) -> Solid {
    let (v, f) = read_binary_stl(name);
    let m = MeshArrays {
        positions: v.iter().map(|&x| x as f32).collect(),
        triangles: f,
    };
    Solid::from_mesh(&m).expect("manifold solid")
}

fn plane_frame(o: [f64; 3], n: [f64; 3], xdir: Option<[f64; 3]>) -> Frame {
    fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }
    fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
        [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
    }
    fn unit(v: [f64; 3]) -> [f64; 3] {
        let n = dot(v, v).sqrt();
        [v[0] / n, v[1] / n, v[2] / n]
    }
    let n = unit(n);
    let mut xdir = xdir;
    let use_default = match xdir {
        None => true,
        Some(xd) => {
            let xn = dot(xd, xd).sqrt();
            xn < 1e-9 || dot(unit(xd), n).abs() > 0.999
        }
    };
    if use_default {
        if n[2].abs() < 0.9 {
            let y = unit([-n[0] * n[2], -n[1] * n[2], 1.0 - n[2] * n[2]]);
            let x = unit(cross(y, n));
            return Frame { o, x, y, n };
        }
        xdir = Some([1.0, 0.0, 0.0]);
    }
    let xd = xdir.unwrap_or([1.0, 0.0, 0.0]);
    let d = dot(xd, n);
    let x = unit([xd[0] - n[0] * d, xd[1] - n[1] * d, xd[2] - n[2] * d]);
    let y = cross(n, x);
    Frame { o, x, y, n }
}

fn load_case(name: &str) -> Value {
    let path = tests_dir().join("parity/refs").join(format!("{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn frame_from_case(case: &Value) -> Frame {
    let frame = case.get("frame").cloned().unwrap_or(Value::Object(Default::default()));
    let origin = [
        frame.get("origin").and_then(|o| o[0].as_f64()).unwrap_or(0.0),
        frame.get("origin").and_then(|o| o[1].as_f64()).unwrap_or(0.0),
        frame.get("origin").and_then(|o| o[2].as_f64()).unwrap_or(0.0),
    ];
    let normal = [
        frame.get("normal").and_then(|o| o[0].as_f64()).unwrap_or(0.0),
        frame.get("normal").and_then(|o| o[1].as_f64()).unwrap_or(0.0),
        frame.get("normal").and_then(|o| o[2].as_f64()).unwrap_or(1.0),
    ];
    let xdir = frame.get("xdir").and_then(|x| {
        let a = x.as_array()?;
        Some([a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?])
    });
    plane_frame(origin, normal, xdir)
}

fn load_solids(case: &Value) -> Vec<(String, Solid)> {
    let models: Vec<String> = if let Some(arr) = case.get("models").and_then(|m| m.as_array()) {
        arr.iter().map(|v| v.as_str().unwrap().to_string()).collect()
    } else {
        vec![case["model"].as_str().unwrap().to_string()]
    };
    models.into_iter().map(|m| (m.clone(), load_solid(&m))).collect()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9
}

fn compare_ref_entity(got: &RefEntity, want: &Value, case: &str, i: usize) {
    match got {
        RefEntity::Circle { cx, cy, r } => {
            assert_eq!(want["type"], "circle", "{case} entity {i}");
            assert!(close(*cx, want["cx"].as_f64().unwrap()), "{case} entity {i} cx got={cx} want={}", want["cx"]);
            assert!(close(*cy, want["cy"].as_f64().unwrap()), "{case} entity {i} cy");
            assert!(close(*r, want["r"].as_f64().unwrap()), "{case} entity {i} r");
        }
        RefEntity::Line { p1, p2 } => {
            assert_eq!(want["type"], "line", "{case} entity {i}");
            let wp1 = want["p1"].as_array().unwrap();
            let wp2 = want["p2"].as_array().unwrap();
            assert!(
                close(p1[0], wp1[0].as_f64().unwrap()) && close(p1[1], wp1[1].as_f64().unwrap()),
                "{case} entity {i} p1 got={p1:?} want={wp1:?}"
            );
            assert!(
                close(p2[0], wp2[0].as_f64().unwrap()) && close(p2[1], wp2[1].as_f64().unwrap()),
                "{case} entity {i} p2 got={p2:?} want={wp2:?}"
            );
        }
    }
}

fn compare_outline_entity(got: &OutlineEntity, want: &Value, case: &str, i: usize) {
    match got {
        OutlineEntity::Circle { id, cx, cy, r, construction, projected } => {
            assert_eq!(want["type"], "circle", "{case} entity {i}");
            assert_eq!(*id, want["id"].as_i64().unwrap(), "{case} entity {i} id");
            assert!(close(*cx, want["cx"].as_f64().unwrap()), "{case} entity {i} cx");
            assert!(close(*cy, want["cy"].as_f64().unwrap()), "{case} entity {i} cy");
            assert!(close(*r, want["r"].as_f64().unwrap()), "{case} entity {i} r");
            assert_eq!(*construction, want.get("construction").and_then(|v| v.as_bool()).unwrap_or(false));
            assert_eq!(*projected, want.get("projected").and_then(|v| v.as_bool()).unwrap_or(false));
        }
        OutlineEntity::Polyline { id, pts, closed, construction, projected } => {
            assert_eq!(want["type"], "polyline", "{case} entity {i}");
            assert_eq!(*id, want["id"].as_i64().unwrap(), "{case} entity {i} id");
            assert_eq!(*closed, want.get("closed").and_then(|v| v.as_bool()).unwrap_or(true));
            assert_eq!(*construction, want.get("construction").and_then(|v| v.as_bool()).unwrap_or(false));
            assert_eq!(*projected, want.get("projected").and_then(|v| v.as_bool()).unwrap_or(false));
            let gp = want["pts"].as_array().unwrap();
            assert_eq!(pts.len(), gp.len(), "{case} entity {i} pts len");
            for (k, (a, b)) in pts.iter().zip(gp.iter()).enumerate() {
                let b = b.as_array().unwrap();
                assert!(
                    close(a[0], b[0].as_f64().unwrap()) && close(a[1], b[1].as_f64().unwrap()),
                    "{case} entity {i} pt {k}"
                );
            }
        }
    }
}

#[test]
fn round_py_matches_python_ties() {
    // Python 3 round half-to-even on the exact binary value (format-then-parse).
    assert_eq!(round_py(0.12345, 4), 0.1235);
    assert_eq!(round_py(2.675, 2), 2.67);
    assert_eq!(round_py(-2.675, 2), -2.67);
    assert_eq!(round_py(-0.12345, 4), -0.1235);
}

#[test]
fn fit_circle_far_and_collinear() {
    // Small circle far from the origin (mean-centred Kasa fit).
    let mut pts = Vec::new();
    for k in 0..24 {
        let t = std::f64::consts::TAU * (k as f64) / 24.0;
        pts.push([1000.0 + t.cos(), 1000.0 + t.sin()]);
    }
    let (cx, cy, r) = fit_circle(&pts);
    assert!((cx - 1000.0).abs() < 1e-6, "cx={cx}");
    assert!((cy - 1000.0).abs() < 1e-6, "cy={cy}");
    assert!((r - 1.0).abs() < 1e-6, "r={r}");

    // Collinear: min-norm-ish like np.linalg.lstsq (finite, not NaN).
    let col = [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]];
    let (cx, cy, r) = fit_circle(&col);
    assert!(cx.is_finite() && cy.is_finite() && r.is_finite(), "collinear {cx} {cy} {r}");
}

#[test]
fn refs_match_golden() {
    let root = golden();
    let obj = root.as_object().expect("golden object");
    for (name, want) in obj {
        let case = load_case(name);
        let op = case.get("op").and_then(|v| v.as_str()).unwrap_or("ref_geometry");
        if op == "outline_entities" {
            let solids = load_solids(&case);
            let frame = frame_from_case(&case);
            let refs: Vec<&Solid> = solids.iter().map(|(_, s)| s).collect();
            let mode = case.get("mode").and_then(|v| v.as_str()).unwrap_or("section");
            let tol = case.get("tolerance").and_then(|v| v.as_f64()).unwrap_or(0.02);
            let polys = outline(&refs, &frame, mode, tol).unwrap_or_else(|e| panic!("{name}: {e}"));
            let construction = case.get("construction").and_then(|v| v.as_bool()).unwrap_or(false);
            let mut next = 1i64;
            let got = outline_entities(&polys, || {
                let id = next;
                next += 1;
                id
            }, construction);
            let want_ents = want["entities"].as_array().unwrap();
            assert_eq!(got.len(), want_ents.len(), "{name}: entity count");
            for (i, (g, w)) in got.iter().zip(want_ents.iter()).enumerate() {
                compare_outline_entity(g, w, name, i);
            }
            continue;
        }
        if op == "ents_evenodd" {
            let polys: Vec<Vec<[f64; 2]>> = case["polys"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| {
                    p.as_array()
                        .unwrap()
                        .iter()
                        .map(|q| {
                            let a = q.as_array().unwrap();
                            [a[0].as_f64().unwrap(), a[1].as_f64().unwrap()]
                        })
                        .collect()
                })
                .collect();
            let tol = case.get("tol").and_then(|v| v.as_f64()).unwrap_or(0.005);
            let got = ents_from_polys(&polys, tol);
            let want_ents = want["entities"].as_array().unwrap();
            assert_eq!(got.len(), want_ents.len(), "{name}: entity count got={:?} want={:?}", got, want_ents);
            for (i, (g, w)) in got.iter().zip(want_ents.iter()).enumerate() {
                compare_ref_entity(g, w, name, i);
            }
            continue;
        }

        let solids = load_solids(&case);
        let frame = frame_from_case(&case);
        let bodies: Vec<(&str, &Solid)> = solids.iter().map(|(n, s)| (n.as_str(), s)).collect();
        let max_segs = case.get("max_segs").and_then(|v| v.as_u64()).unwrap_or(3000) as usize;
        let result = ref_geometry_max(&bodies, &frame, max_segs);
        if let Some(err) = want.get("error").and_then(|e| e.as_str()) {
            panic!("{name}: expected error {err:?}, got {} entities", result.entities.len());
        }
        if let Some(Value::String(w)) = want.get("kind") {
            assert_eq!(result.kind.as_deref(), Some(w.as_str()), "{name} kind");
        } else if want.get("kind") == Some(&Value::Null) {
            assert_eq!(result.kind, None, "{name} kind");
        }
        let want_ents = want["entities"].as_array().unwrap();
        assert_eq!(result.entities.len(), want_ents.len(), "{name}: entity count got={:?} want={:?}", result.entities, want_ents);
        for (i, (g, w)) in result.entities.iter().zip(want_ents.iter()).enumerate() {
            compare_ref_entity(g, w, name, i);
        }
    }
}
