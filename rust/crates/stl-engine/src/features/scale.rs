use serde_json::{Map, Value};

use super::common::{affine, pivot, transform_body, Vec3};
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn string_list(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
        _ => Vec::new(),
    }
}

fn body_size(b: &crate::state::Body) -> Vec3 {
    let solid_mesh;
    let mesh = match (&b.solid, &b.mesh) {
        (Some(s), _) => {
            solid_mesh = s.to_mesh();
            &solid_mesh
        }
        (None, Some(m)) => m.as_ref(),
        (None, None) => return [0.0; 3],
    };
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in mesh.positions.chunks_exact(3) {
        for i in 0..3 {
            let v = p[i] as f64;
            lo[i] = lo[i].min(v);
            hi[i] = hi[i].max(v);
        }
    }
    // 1.x: float32 verts → `v.max(0) - v.min(0)` stays float32; cast back for f64 math.
    [
        ((hi[0] - lo[0]) as f32) as f64,
        ((hi[1] - lo[1]) as f32) as f64,
        ((hi[2] - lo[2]) as f32) as f64,
    ]
}

fn factor3(p: &Map<String, Value>) -> Result<Vec3, FeatureError> {
    match p.get("factor") {
        None | Some(Value::Null) => Ok([1.0, 1.0, 1.0]),
        Some(Value::Number(n)) => {
            let k = n.as_f64().ok_or_else(|| FeatureError("factor must be a number.".into()))?;
            Ok([k, k, k])
        }
        Some(Value::Array(a)) if a.len() == 3 => Ok([
            a[0].as_f64().ok_or_else(|| FeatureError("factor must be a number.".into()))?,
            a[1].as_f64().ok_or_else(|| FeatureError("factor must be a number.".into()))?,
            a[2].as_f64().ok_or_else(|| FeatureError("factor must be a number.".into()))?,
        ]),
        Some(Value::Bool(b)) => {
            let k = if *b { 1.0 } else { 0.0 };
            Ok([k, k, k])
        }
        _ => Err(FeatureError("factor must be a number.".into())),
    }
}

fn vec3_size(p: &Map<String, Value>, fallback: Vec3) -> Result<Vec3, FeatureError> {
    match p.get("size") {
        None | Some(Value::Null) => Ok(fallback),
        Some(Value::Array(a)) if a.len() == 3 => Ok([
            a[0].as_f64().ok_or_else(|| FeatureError("size must be a number.".into()))?,
            a[1].as_f64().ok_or_else(|| FeatureError("size must be a number.".into()))?,
            a[2].as_f64().ok_or_else(|| FeatureError("size must be a number.".into()))?,
        ]),
        _ => Err(FeatureError("size must be a number.".into())),
    }
}

/// 1.x `p.get("uniform", True)`: missing key → True; present `None`/`null` → falsy.
fn uniform_flag(p: &Map<String, Value>) -> bool {
    match p.get("uniform") {
        None => true,
        Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().map(|x| x != 0.0).unwrap_or(false),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(m)) => !m.is_empty(),
    }
}

fn matmul_vec(r: &[[f64; 3]; 3], v: Vec3) -> Vec3 {
    [
        r[0][0] * v[0] + r[0][1] * v[1] + r[0][2] * v[2],
        r[1][0] * v[0] + r[1][1] * v[1] + r[1][2] * v[2],
        r[2][0] * v[0] + r[2][1] * v[1] + r[2][2] * v[2],
    ]
}

fn f_scale(
    state: &mut State,
    _fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let bids = string_list(p.get("bodies"));
    if bids.is_empty() {
        return Err(FeatureError("Choose a body to scale.".into()));
    }
    let pivot_v = p.get("pivot").cloned().unwrap_or(Value::String("base".into()));
    for bid in &bids {
        let b = state.body(bid)?.clone();
        let size = body_size(&b);
        let mode = p.get("mode").and_then(Value::as_str).unwrap_or("factor");
        let k = match mode {
            "factor" => factor3(p)?,
            "fit" => {
                let target = vec3_size(p, size)?;
                let mut k = [1.0; 3];
                for i in 0..3 {
                    if size[i] > 1e-9 {
                        k[i] = target[i] / size[i];
                    }
                }
                if uniform_flag(p) {
                    // 1.x: `axis = int(p.get("axis", int(np.argmax(...))))` then `k[axis]`.
                    // Numpy allows negative indices (-1 → 2); axis outside [-3, 2] raises IndexError.
                    let axis = match p.get("axis") {
                        Some(Value::Number(n)) => {
                            n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)).unwrap_or(0) as isize
                        }
                        Some(Value::Bool(b)) => if *b { 1 } else { 0 },
                        Some(Value::String(s)) => s
                            .trim()
                            .parse::<i64>()
                            .map(|i| i as isize)
                            .unwrap_or(0),
                        _ => {
                            let mut best = 0usize;
                            let mut best_abs = (target[0] - size[0]).abs();
                            for i in 1..3 {
                                let d = (target[i] - size[i]).abs();
                                if d > best_abs {
                                    best_abs = d;
                                    best = i;
                                }
                            }
                            best as isize
                        }
                    };
                    let axis = if axis < 0 {
                        let wrapped = axis + 3;
                        if wrapped < 0 || wrapped > 2 {
                            return Err(FeatureError(format!(
                                "index {axis} is out of bounds for axis 0 with size 3"
                            )));
                        }
                        wrapped as usize
                    } else if axis > 2 {
                        return Err(FeatureError(format!(
                            "index {axis} is out of bounds for axis 0 with size 3"
                        )));
                    } else {
                        axis as usize
                    };
                    [k[axis]; 3]
                } else {
                    k
                }
            }
            "units" => {
                let units = p.get("units").and_then(Value::as_str).unwrap_or("in_to_mm");
                let s = match units {
                    "in_to_mm" => 25.4,
                    "mm_to_in" => 1.0 / 25.4,
                    "cm_to_mm" => 10.0,
                    "m_to_mm" => 1000.0,
                    _ => 1.0,
                };
                [s, s, s]
            }
            other => return Err(FeatureError(format!("Unknown scale mode {other}."))),
        };
        if k.iter().any(|&x| x <= 0.0) {
            return Err(FeatureError("Scale factors must be positive.".into()));
        }
        let c = pivot(&b, &pivot_v)?;
        let r = [[k[0], 0.0, 0.0], [0.0, k[1], 0.0], [0.0, 0.0, k[2]]];
        let rc = matmul_vec(&r, c);
        let m = affine(r, [c[0] - rc[0], c[1] - rc[1], c[2] - rc[2]]);
        state.bodies.insert(bid.clone(), transform_body(&b, m));
        res.modified.push(bid.clone());
    }
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("scale", f_scale)
}
