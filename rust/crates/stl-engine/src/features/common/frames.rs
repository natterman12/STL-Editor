use serde_json::{Map, Value};

use crate::{FeatureError, State};

pub use stl_kernel::{cross, dot, Frame, Vec3};

const EPS: f64 = 1e-9;

/// A plane the user made. Other keys stored with it (`extra`) are kept as they are.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaneDef {
    pub frame: Frame,
    pub extra: Map<String, Value>,
}

fn norm(a: Vec3) -> f64 {
    dot(a, a).sqrt()
}

fn neg(a: Vec3) -> Vec3 {
    [-a[0], -a[1], -a[2]]
}

/// geometry.unit
pub(crate) fn unit(v: Vec3) -> Result<Vec3, FeatureError> {
    let n = norm(v);
    if n < EPS {
        return Err(FeatureError("Zero-length direction.".into()));
    }
    Ok([v[0] / n, v[1] / n, v[2] / n])
}

/// geometry.BASE_PLANES as `(name, origin, normal, xdir)`.
pub fn base_planes() -> [(&'static str, Vec3, Vec3, Vec3); 3] {
    [
        ("XY", [0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        ("XZ", [0.0; 3], [0.0, -1.0, 0.0], [1.0, 0.0, 0.0]),
        ("YZ", [0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ]
}

/// geometry.plane_frame: an orthonormal right-handed frame for a plane.
pub fn plane_frame(o: Vec3, n: Vec3, xdir: Option<Vec3>) -> Result<Frame, FeatureError> {
    let n = unit(n)?;
    let mut xdir = xdir;
    let use_default = match xdir {
        None => true,
        Some(xd) => norm(xd) < EPS || dot(unit(xd)?, n).abs() > 0.999,
    };
    if use_default {
        if n[2].abs() < 0.9 {
            // wall-like plane: keep world Z as the sketch's "up" (y) direction
            let y = unit([-n[0] * n[2], -n[1] * n[2], 1.0 - n[2] * n[2]])?;
            let x = unit(cross(y, n))?;
            return Ok(Frame { o, x, y, n });
        }
        xdir = Some([1.0, 0.0, 0.0]);
    }
    let xd = xdir.unwrap_or([1.0, 0.0, 0.0]);
    let d = dot(xd, n);
    let x = unit([xd[0] - n[0] * d, xd[1] - n[1] * d, xd[2] - n[2] * d])?;
    let y = cross(n, x);
    Ok(Frame { o, x, y, n })
}

fn vec3(v: &Value) -> Option<Vec3> {
    let a = v.as_array()?;
    if a.len() != 3 {
        return None;
    }
    Some([a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?])
}

fn invalid() -> FeatureError {
    FeatureError("Invalid plane reference.".into())
}

impl State {
    /// 1.x `State.frame`: resolve a plane reference (a name, `{plane}` or
    /// `{origin, normal, xdir?, flip?}`; none means XY) into a frame.
    pub fn frame(&self, r: Option<&Value>) -> Result<Frame, FeatureError> {
        let default = Value::String("XY".into());
        let r = match r {
            None | Some(Value::Null) => &default,
            Some(v) => v,
        };
        match r {
            Value::String(name) => {
                if let Some((_, o, n, x)) = base_planes().iter().find(|p| p.0 == name) {
                    return plane_frame(*o, *n, Some(*x));
                }
                if let Some(p) = self.planes.get(name) {
                    return Ok(p.frame);
                }
                Err(FeatureError(format!("Plane '{name}' no longer exists.")))
            }
            Value::Object(d) => {
                if let Some(p) = d.get("plane") {
                    return self.frame(Some(p));
                }
                if let (Some(o), Some(n)) = (d.get("origin"), d.get("normal")) {
                    let xdir = match d.get("xdir") {
                        None | Some(Value::Null) => None,
                        Some(x) => Some(vec3(x).ok_or_else(invalid)?),
                    };
                    let fr = plane_frame(vec3(o).ok_or_else(invalid)?, vec3(n).ok_or_else(invalid)?, xdir)?;
                    let flip = match d.get("flip") {
                        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
                        Some(Value::Bool(true)) => true,
                        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
                        Some(Value::String(s)) => !s.is_empty(),
                        Some(Value::Array(a)) => !a.is_empty(),
                        Some(Value::Object(m)) => !m.is_empty(),
                    };
                    return Ok(if flip { Frame { o: fr.o, x: fr.x, y: neg(fr.y), n: neg(fr.n) } } else { fr });
                }
                Err(invalid())
            }
            _ => Err(invalid()),
        }
    }
}
