//! SVG/DXF import against `rust/tests/parity/import2d_golden.json` (from 1.x `import2d.js`).

use std::path::PathBuf;

use serde_json::Value;
use stl_sketch::entities::{Entity, Polyline};
use stl_sketch::import2d::{import_dxf, import_svg, svg_drawing_size_mm};

fn tests_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests")
}

fn golden() -> Value {
    let path = tests_dir().join("parity/import2d_golden.json");
    let text = std::fs::read_to_string(&path).expect("import2d_golden.json");
    serde_json::from_str(&text).expect("parse import2d_golden.json")
}

fn entity_type(e: &Entity) -> &'static str {
    match e {
        Entity::Line(_) => "line",
        Entity::Arc(_) => "arc",
        Entity::Circle(_) => "circle",
        Entity::Rect(_) => "rect",
        Entity::Polygon(_) => "polygon",
        Entity::Slot(_) => "slot",
        Entity::Polyline(_) => "polyline",
        Entity::Point(_) => "point",
    }
}

fn golden_type(v: &Value) -> &str {
    v.get("type").and_then(|t| t.as_str()).unwrap_or("")
}

/// Spec "drawing size" from the golden entities' bbox (never from the output under test).
fn golden_drawing_size(want_ents: &[Value]) -> f64 {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let push = |x: f64, y: f64, min_x: &mut f64, min_y: &mut f64, max_x: &mut f64, max_y: &mut f64| {
        *min_x = min_x.min(x);
        *min_y = min_y.min(y);
        *max_x = max_x.max(x);
        *max_y = max_y.max(y);
    };
    for w in want_ents {
        match golden_type(w) {
            "line" => {
                if let (Some(p1), Some(p2)) = (w.get("p1").and_then(|a| a.as_array()), w.get("p2").and_then(|a| a.as_array())) {
                    push(p1[0].as_f64().unwrap_or(0.0), p1[1].as_f64().unwrap_or(0.0), &mut min_x, &mut min_y, &mut max_x, &mut max_y);
                    push(p2[0].as_f64().unwrap_or(0.0), p2[1].as_f64().unwrap_or(0.0), &mut min_x, &mut min_y, &mut max_x, &mut max_y);
                }
            }
            "circle" | "arc" => {
                let cx = w.get("cx").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let cy = w.get("cy").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let r = w.get("r").and_then(|v| v.as_f64()).unwrap_or(0.0);
                push(cx - r, cy - r, &mut min_x, &mut min_y, &mut max_x, &mut max_y);
                push(cx + r, cy + r, &mut min_x, &mut min_y, &mut max_x, &mut max_y);
            }
            "polyline" => {
                if let Some(pts) = golden_pts(w) {
                    for pt in pts {
                        push(pt[0], pt[1], &mut min_x, &mut min_y, &mut max_x, &mut max_y);
                    }
                }
            }
            _ => {}
        }
    }
    if !min_x.is_finite() {
        return 0.0;
    }
    (max_x - min_x).max(max_y - min_y)
}

fn pts_of(e: &Entity) -> Option<&[[f64; 2]]> {
    match e {
        Entity::Polyline(Polyline { pts, .. }) => Some(pts.as_slice()),
        _ => None,
    }
}

fn golden_pts(v: &Value) -> Option<Vec<[f64; 2]>> {
    let arr = v.get("pts")?.as_array()?;
    let mut out = Vec::with_capacity(arr.len());
    for p in arr {
        let a = p.as_array()?;
        out.push([a[0].as_f64()?, a[1].as_f64()?]);
    }
    Some(out)
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn compare_entity_svg(got: &Entity, want: &Value, size: f64, file: &str, i: usize) {
    assert_eq!(entity_type(got), golden_type(want), "{file} entity {i} type");
    let tol = (0.01_f64).max(size * 0.001);
    match got {
        Entity::Line(l) => {
            let p1 = want.get("p1").unwrap().as_array().unwrap();
            let p2 = want.get("p2").unwrap().as_array().unwrap();
            assert!(close(l.p1[0], p1[0].as_f64().unwrap(), tol) && close(l.p1[1], p1[1].as_f64().unwrap(), tol), "{file} entity {i} p1");
            assert!(close(l.p2[0], p2[0].as_f64().unwrap(), tol) && close(l.p2[1], p2[1].as_f64().unwrap(), tol), "{file} entity {i} p2");
        }
        Entity::Circle(c) => {
            assert!(close(c.cx, want["cx"].as_f64().unwrap(), tol), "{file} entity {i} cx");
            assert!(close(c.cy, want["cy"].as_f64().unwrap(), tol), "{file} entity {i} cy");
            assert!(close(c.r, want["r"].as_f64().unwrap(), tol), "{file} entity {i} r");
        }
        Entity::Arc(a) => {
            assert!(close(a.cx, want["cx"].as_f64().unwrap(), tol), "{file} entity {i} cx");
            assert!(close(a.cy, want["cy"].as_f64().unwrap(), tol), "{file} entity {i} cy");
            assert!(close(a.r, want["r"].as_f64().unwrap(), tol), "{file} entity {i} r");
            assert!(close(a.a0, want["a0"].as_f64().unwrap(), tol), "{file} entity {i} a0");
            assert!(close(a.a1, want["a1"].as_f64().unwrap(), tol), "{file} entity {i} a1");
        }
        Entity::Polyline(p) => {
            assert_eq!(p.is_closed(), want.get("closed").and_then(|c| c.as_bool()).unwrap_or(false), "{file} entity {i} closed");
            let gp = golden_pts(want).expect("polyline pts");
            assert_eq!(p.pts.len(), gp.len(), "{file} entity {i} point count");
            for (k, (a, b)) in p.pts.iter().zip(gp.iter()).enumerate() {
                assert!(
                    close(a[0], b[0], tol) && close(a[1], b[1], tol),
                    "{file} entity {i} pt {k}: got {:?} want {:?} tol {tol}",
                    a,
                    b
                );
            }
        }
        other => panic!("{file} entity {i}: unexpected type {:?}", entity_type(other)),
    }
}

fn compare_entity_dxf(got: &Entity, want: &Value, file: &str, i: usize) {
    assert_eq!(entity_type(got), golden_type(want), "{file} entity {i} type");
    let tol = 1e-9;
    match got {
        Entity::Line(l) => {
            let p1 = want.get("p1").unwrap().as_array().unwrap();
            let p2 = want.get("p2").unwrap().as_array().unwrap();
            assert!(close(l.p1[0], p1[0].as_f64().unwrap(), tol) && close(l.p1[1], p1[1].as_f64().unwrap(), tol), "{file} entity {i} p1");
            assert!(close(l.p2[0], p2[0].as_f64().unwrap(), tol) && close(l.p2[1], p2[1].as_f64().unwrap(), tol), "{file} entity {i} p2");
        }
        Entity::Circle(c) => {
            assert!(close(c.cx, want["cx"].as_f64().unwrap(), tol), "{file} entity {i} cx");
            assert!(close(c.cy, want["cy"].as_f64().unwrap(), tol), "{file} entity {i} cy");
            assert!(close(c.r, want["r"].as_f64().unwrap(), tol), "{file} entity {i} r");
        }
        Entity::Arc(a) => {
            assert!(close(a.cx, want["cx"].as_f64().unwrap(), tol), "{file} entity {i} cx");
            assert!(close(a.cy, want["cy"].as_f64().unwrap(), tol), "{file} entity {i} cy");
            assert!(close(a.r, want["r"].as_f64().unwrap(), tol), "{file} entity {i} r");
            assert!(close(a.a0, want["a0"].as_f64().unwrap(), tol), "{file} entity {i} a0");
            assert!(close(a.a1, want["a1"].as_f64().unwrap(), tol), "{file} entity {i} a1");
        }
        Entity::Polyline(p) => {
            assert_eq!(p.is_closed(), want.get("closed").and_then(|c| c.as_bool()).unwrap_or(false), "{file} entity {i} closed");
            let gp = golden_pts(want).expect("polyline pts");
            assert_eq!(p.pts.len(), gp.len(), "{file} entity {i} point count");
            for (k, (a, b)) in p.pts.iter().zip(gp.iter()).enumerate() {
                assert!(
                    close(a[0], b[0], tol) && close(a[1], b[1], tol),
                    "{file} entity {i} pt {k}: got {:?} want {:?}",
                    a,
                    b
                );
            }
        }
        other => panic!("{file} entity {i}: unexpected type {:?}", entity_type(other)),
    }
    let _ = pts_of(got);
}

#[test]
fn import2d_matches_golden() {
    let root = golden();
    let obj = root.as_object().expect("golden object");
    let cases_dir = tests_dir().join("parity/import2d");
    for (file, want) in obj {
        let text = std::fs::read_to_string(cases_dir.join(file)).unwrap_or_else(|e| panic!("read {file}: {e}"));
        let is_dxf = file.to_ascii_lowercase().ends_with(".dxf");
        if let Some(err) = want.get("error").and_then(|e| e.as_str()) {
            let got = if is_dxf { import_dxf(&text) } else { import_svg(&text) };
            let msg = got.expect_err(&format!("{file} should error")).message().to_string();
            assert_eq!(msg, err, "{file} error message");
            continue;
        }
        let want_ents = want.get("entities").and_then(|e| e.as_array()).unwrap_or_else(|| panic!("{file}: entities"));
        let got = if is_dxf {
            import_dxf(&text).unwrap_or_else(|e| panic!("{file}: {e}"))
        } else {
            import_svg(&text).unwrap_or_else(|e| panic!("{file}: {e}"))
        };
        assert_eq!(got.len(), want_ents.len(), "{file}: entity count");
        if is_dxf {
            for (i, (g, w)) in got.iter().zip(want_ents.iter()).enumerate() {
                compare_entity_dxf(g, w, file, i);
            }
        } else {
            // Tolerance from golden drawing size (bbox or SVG width/viewBox), never from `got`.
            let size = golden_drawing_size(want_ents).max(svg_drawing_size_mm(&text).unwrap_or(0.0));
            for (i, (g, w)) in got.iter().zip(want_ents.iter()).enumerate() {
                compare_entity_svg(g, w, size, file, i);
            }
        }
    }
}
