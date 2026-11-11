//! Shell (1.x `f_shell`): hollow the body, then open each picked flat face. The hollow space
//! just under the wall (kept inside the face) is pushed out through it. Two passes, so faces
//! that meet at an edge also take the wall strip along that edge. No faces: a closed hollow,
//! like Hollow.

use serde_json::{Map, Value};
use stl_kernel::{Shape2d, Solid};
use stl_sketch::P2;

use super::common::pitch::hollow_pitch;
use super::common::{float, frame_matrix, plane_frame, truthy, Vec3};
use crate::state::Body;
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn vec3(v: Option<&Value>) -> Option<Vec3> {
    let a = v?.as_array()?;
    Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?])
}

fn contours(v: &Value) -> Option<Vec<Vec<P2>>> {
    let mut out = Vec::new();
    for l in v.as_array()? {
        let mut pts = Vec::new();
        for p in l.as_array()? {
            let u = p.as_array()?;
            pts.push([u.first()?.as_f64()?, u.get(1)?.as_f64()?]);
        }
        out.push(pts);
    }
    Some(out)
}

struct Picked {
    i: usize,
    to_local: [[f64; 4]; 3],
    back: [[f64; 4]; 3],
    outline: Shape2d,
}

fn f_shell(
    state: &mut State,
    _fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let bid = p.get("body").and_then(Value::as_str).unwrap_or("");
    if bid.is_empty() {
        return Err(FeatureError("Choose a body to shell.".into()));
    }
    let b = state.body(bid)?.clone();
    let s = b.require_solid()?;
    let t = float(p, "thickness", Some(2.0))?;
    if t <= 0.0 {
        return Err(FeatureError("Wall thickness must be positive.".into()));
    }
    let pitch = hollow_pitch(s, t, p.get("resolution").and_then(Value::as_str));
    let cavity = stl_mesh::voxel::hollow(s, t, Some(pitch), true).map_err(|e| FeatureError(e.to_string()))?;
    let depth = t + 2.0 * pitch;

    let faces: Vec<&Value> = match p.get("faces") {
        Some(Value::Array(a)) => a.iter().filter(|f| truthy(Some(f))).collect(),
        _ => Vec::new(),
    };
    let mut picked: Vec<Picked> = Vec::new();
    for (k, face) in faces.iter().enumerate() {
        let i = k + 1;
        let flat = || FeatureError(format!("Face {i} isn't flat. Shell can only open flat faces."));
        let Some(face) = face.as_object() else { return Err(flat()) };
        let cont = match face.get("contours") {
            Some(c) if truthy(Some(c)) => contours(c).ok_or_else(flat)?,
            _ => return Err(flat()),
        };
        let (Some(o), Some(n)) = (vec3(face.get("origin")), vec3(face.get("normal"))) else {
            return Err(flat());
        };
        let fr = plane_frame(o, n, vec3(face.get("xdir")))?;
        let r = [fr.x, fr.y, fr.n];
        let mut to_local = [[0.0; 4]; 3];
        for a in 0..3 {
            for c in 0..3 {
                to_local[a][c] = r[a][c];
            }
            to_local[a][3] = -(r[a][0] * fr.o[0] + r[a][1] * fr.o[1] + r[a][2] * fr.o[2]);
        }
        picked.push(Picked { i, to_local, back: frame_matrix(&fr), outline: Shape2d::from_even_odd(&cont) });
    }

    let openings = |space: &Solid| -> Result<Vec<Solid>, FeatureError> {
        let mut tools = Vec::new();
        for pk in &picked {
            let hole = space.transform(pk.to_local).slice(-depth).intersection(&pk.outline);
            if hole.is_empty() || hole.area() < 1e-6 {
                return Err(FeatureError(format!(
                    "There's no hollow space behind face {} to open. Try a thinner wall.",
                    pk.i
                )));
            }
            let tool = hole.extrude(depth + pitch).translate([0.0, 0.0, -depth]);
            tools.push(tool.transform(pk.back));
        }
        Ok(tools)
    };

    let mut cut = cavity.clone();
    if !picked.is_empty() {
        let first = openings(&cavity)?;
        let mut both = vec![cavity.clone()];
        both.extend(first);
        let second = openings(&Solid::union_all(&both))?;
        let mut all = vec![cavity.clone()];
        all.extend(second);
        cut = Solid::union_all(&all);
    }
    let out = s.difference(&cut);
    state.bodies.insert(bid.to_string(), Body { name: b.name.clone(), solid: Some(out), mesh: None });
    res.modified.push(bid.to_string());
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("shell", f_shell)
}
