use serde_json::{Map, Value};

use super::common::frames::{cross, dot, unit};
use super::common::{float, plane_frame, Frame, PlaneDef, Vec3};
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: Vec3, s: f64) -> Vec3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn neg(a: Vec3) -> Vec3 {
    [-a[0], -a[1], -a[2]]
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().map(|x| x != 0.0).unwrap_or(false),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(m)) => !m.is_empty(),
    }
}

fn points3(p: &Map<String, Value>) -> Result<[Vec3; 3], FeatureError> {
    let Some(Value::Array(pts)) = p.get("points") else {
        return Err(FeatureError("Pick three points.".into()));
    };
    if pts.len() != 3 {
        return Err(FeatureError("Pick three points.".into()));
    }
    let mut out = [[0.0; 3]; 3];
    for (i, pt) in pts.iter().enumerate() {
        let Value::Array(a) = pt else {
            return Err(FeatureError("Pick three points.".into()));
        };
        if a.len() != 3 {
            return Err(FeatureError("Pick three points.".into()));
        }
        for j in 0..3 {
            out[i][j] = a[j].as_f64().ok_or_else(|| FeatureError("Pick three points.".into()))?;
        }
    }
    Ok(out)
}

fn plane_size(p: &Map<String, Value>) -> Result<f64, FeatureError> {
    // 1.x: float(p.get("size", 0) or 0) — a non-numeric string raises ValueError.
    let Some(v) = p.get("size") else {
        return Ok(0.0);
    };
    // Falsy values (None/null, 0, "", False, …) become 0 before float().
    let falsy = match v {
        Value::Null => true,
        Value::Bool(false) => true,
        Value::Number(n) => n.as_f64() == Some(0.0),
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(m) => m.is_empty(),
        _ => false,
    };
    if falsy {
        return Ok(0.0);
    }
    match v {
        Value::Number(n) => Ok(n.as_f64().unwrap_or(0.0)),
        Value::Bool(true) => Ok(1.0),
        // 1.x leaks Python's "could not convert string to float: '...'"; see EXCEPTIONS.md.
        Value::String(s) => s.parse::<f64>().map_err(|_| FeatureError("Plane size must be a number.".into())),
        _ => Err(FeatureError("Plane size must be a number.".into())),
    }
}

fn f_plane(
    state: &mut State,
    fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let mode = p.get("mode").and_then(Value::as_str).unwrap_or("offset");
    let mut fr = match mode {
        "offset" => {
            let base = state.frame(p.get("base"))?;
            let d = float(p, "distance", Some(0.0))?;
            Frame { o: add(base.o, scale(base.n, d)), x: base.x, y: base.y, n: base.n }
        }
        "angle" => {
            let base = state.frame(p.get("base"))?;
            let a = float(p, "angle", Some(45.0))?.to_radians();
            // 1.x: `axis = p.get("axis", "x"); if axis == "x": ... else: ...`
            // Missing key defaults to "x"; present null / non-string takes the else (y) branch.
            let axis_is_x = match p.get("axis") {
                None => true,
                Some(Value::String(s)) => s == "x",
                _ => false,
            };
            let mut fr = if axis_is_x {
                let n2 = add(scale(base.n, a.cos()), scale(neg(base.y), a.sin()));
                let y2 = cross(n2, base.x);
                Frame { o: base.o, x: base.x, y: y2, n: n2 }
            } else {
                let n2 = add(scale(base.n, a.cos()), scale(base.x, a.sin()));
                let x2 = cross(base.y, n2);
                Frame { o: base.o, x: x2, y: base.y, n: n2 }
            };
            if truthy(p.get("distance")) {
                let d = float(p, "distance", Some(0.0))?;
                fr.o = add(fr.o, scale(fr.n, d));
            }
            fr
        }
        "three_points" => {
            let pts = points3(p)?;
            let n = cross(
                [pts[1][0] - pts[0][0], pts[1][1] - pts[0][1], pts[1][2] - pts[0][2]],
                [pts[2][0] - pts[0][0], pts[2][1] - pts[0][1], pts[2][2] - pts[0][2]],
            );
            if (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt() < 1e-9 {
                return Err(FeatureError("The three points are in a line.".into()));
            }
            let xdir = [pts[1][0] - pts[0][0], pts[1][1] - pts[0][1], pts[1][2] - pts[0][2]];
            plane_frame(pts[0], n, Some(xdir))?
        }
        "midplane" => {
            let a = state.frame(p.get("base"))?;
            let b = state.frame(p.get("base2"))?;
            let b_n = if dot(a.n, b.n) >= 0.0 { b.n } else { neg(b.n) };
            // 1.x: G.unit(a.n + b.n) before plane_frame normalises again.
            let mid_n = unit(add(a.n, b_n))?;
            let o = scale(add(a.o, b.o), 0.5);
            plane_frame(o, mid_n, Some(a.x))?
        }
        "face" => {
            let mut fr = state.frame(p.get("base"))?;
            if truthy(p.get("distance")) {
                let d = float(p, "distance", Some(0.0))?;
                fr.o = add(fr.o, scale(fr.n, d));
            }
            fr
        }
        other => return Err(FeatureError(format!("Unknown plane mode {other}."))),
    };
    if truthy(p.get("flip")) {
        fr = Frame { o: fr.o, x: fr.x, y: neg(fr.y), n: neg(fr.n) };
    }
    let mut extra = Map::new();
    let name = p.get("name").and_then(Value::as_str).filter(|s| !s.is_empty()).unwrap_or("Plane");
    extra.insert("name".into(), Value::String(name.into()));
    let size = plane_size(p)?;
    extra.insert("size".into(), Value::from(size));
    state.planes.insert(fid.to_string(), PlaneDef { frame: fr, extra });
    res.created_plane = Some(fid.to_string());
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("plane", f_plane)
}
