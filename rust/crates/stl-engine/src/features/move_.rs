use serde_json::{Map, Value};

use super::common::{affine, pivot, rotation, transform_body, Vec3};
use crate::state::Body;
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

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

fn string_list(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
        _ => Vec::new(),
    }
}

fn vec3(p: &Map<String, Value>, k: &str, default: Vec3) -> Result<Vec3, FeatureError> {
    match p.get(k) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Array(a)) if a.len() == 3 => Ok([
            a[0].as_f64().ok_or_else(|| FeatureError(format!("{k} must be a number.")))?,
            a[1].as_f64().ok_or_else(|| FeatureError(format!("{k} must be a number.")))?,
            a[2].as_f64().ok_or_else(|| FeatureError(format!("{k} must be a number.")))?,
        ]),
        _ => Err(FeatureError(format!("{k} must be a number."))),
    }
}

fn matmul_vec(r: &[[f64; 3]; 3], v: Vec3) -> Vec3 {
    [
        r[0][0] * v[0] + r[0][1] * v[1] + r[0][2] * v[2],
        r[1][0] * v[0] + r[1][1] * v[1] + r[1][2] * v[2],
        r[2][0] * v[0] + r[2][1] * v[1] + r[2][2] * v[2],
    ]
}

/// 1.x uses `nb.mesh()[0][:, 2].min()` — min Z of mesh vertices, not the AABB.
fn min_z(b: &Body) -> f64 {
    let solid_mesh;
    let mesh = match (&b.solid, &b.mesh) {
        (Some(s), _) => {
            solid_mesh = s.to_mesh();
            &solid_mesh
        }
        (None, Some(m)) => m.as_ref(),
        (None, None) => return 0.0,
    };
    mesh.positions.chunks_exact(3).map(|p| p[2] as f64).fold(f64::INFINITY, f64::min)
}

fn f_move(
    state: &mut State,
    fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let bids = string_list(p.get("bodies"));
    if bids.is_empty() {
        return Err(FeatureError("Choose a body to move.".into()));
    }
    let rot = vec3(p, "rotate", [0.0; 3])?;
    let r = rotation(rot[0], rot[1], rot[2]);
    let t = vec3(p, "translate", [0.0; 3])?;
    let pivot_v = p.get("pivot").cloned().unwrap_or(Value::String("center".into()));
    let do_copy = truthy(p.get("copy"));
    let bed = truthy(p.get("bed"));
    let has_rot = rot.iter().any(|a| a.abs() > 1e-9);
    for (i, bid) in bids.iter().enumerate() {
        let b = state.body(bid)?.clone();
        let c = pivot(&b, &pivot_v)?;
        let rc = matmul_vec(&r, c);
        let m = affine(r, [c[0] - rc[0] + t[0], c[1] - rc[1] + t[1], c[2] - rc[2] + t[2]]);
        let mut nb = transform_body(&b, m);
        // Land on bed: a turned part rests on the bed (Move Z then lifts it off), so it never
        // hangs through the bed after a rotation
        if bed && has_rot {
            let dz = t[2] - min_z(&nb);
            if dz.abs() > 1e-9 {
                let eye = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
                nb = transform_body(&nb, affine(eye, [0.0, 0.0, dz]));
            }
        }
        if do_copy {
            let nid = if bids.len() == 1 { fid.to_string() } else { format!("{fid}.{}", i + 1) };
            nb.name = format!("{} (copy)", b.name);
            state.bodies.insert(nid.clone(), nb);
            res.created.push(nid);
        } else {
            state.bodies.insert(bid.clone(), nb);
            res.modified.push(bid.clone());
        }
    }
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("move", f_move)
}
