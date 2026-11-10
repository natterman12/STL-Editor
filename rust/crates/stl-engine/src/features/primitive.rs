use serde_json::{Map, Value};
use stl_kernel::{Shape2d, Solid};

use super::common::{affine, apply_tool, float, place, rotation, Vec3};
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
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

/// Clamp a signed segment count for Manifold: negatives become 0 (auto, same as <3).
/// Without this, `i as u32` wraps to ~4e9 and hangs.
fn seg_u32(i: i64) -> u32 {
    if i < 0 {
        0
    } else {
        i as u32
    }
}

/// 1.x `int(p.get("segments", 96))` — numeric strings like `"64"` parse; truncates toward zero.
fn segments(p: &Map<String, Value>) -> Result<u32, FeatureError> {
    match p.get("segments") {
        None | Some(Value::Null) => Ok(96),
        Some(Value::Number(n)) => {
            let f = n.as_f64().unwrap_or(96.0);
            Ok(seg_u32(f as i64))
        }
        Some(Value::Bool(b)) => Ok(if *b { 1 } else { 0 }),
        Some(Value::String(s)) => {
            let t = s.trim();
            // Python int() accepts optional sign and digit underscores in 3.x for literals via
            // int(); plain decimal strings only (not "64.5").
            t.parse::<i64>()
                .map(seg_u32)
                .map_err(|_| FeatureError(format!("invalid literal for int() with base 10: '{s}'")))
        }
        _ => Err(FeatureError("segments must be an integer.".into())),
    }
}

fn f_primitive(
    state: &mut State,
    fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let shape = p.get("shape").and_then(Value::as_str).unwrap_or("box");
    let seg = segments(p)?;
    let anchor = p.get("anchor").and_then(Value::as_str).unwrap_or("base");
    let mut man = match shape {
        "box" => {
            let l = float(p, "length", Some(10.0))?;
            let w = float(p, "width", Some(10.0))?;
            let h = float(p, "height", Some(10.0))?;
            let man = Solid::cube([l, w, h], false);
            let shift = match anchor {
                "corner" => [0.0, 0.0, 0.0],
                "center" => [-l / 2.0, -w / 2.0, -h / 2.0],
                _ => [-l / 2.0, -w / 2.0, 0.0], // base
            };
            man.translate(shift)
        }
        "cylinder" => {
            let r = float(p, "radius", Some(5.0))?;
            let h = float(p, "height", Some(10.0))?;
            let man = Solid::cylinder(h, r, r, seg, false);
            if anchor == "center" {
                man.translate([0.0, 0.0, -h / 2.0])
            } else {
                man
            }
        }
        "cone" => {
            let r1 = float(p, "radius", Some(5.0))?;
            let r2 = float(p, "radius2", Some(0.0))?;
            let h = float(p, "height", Some(10.0))?;
            let man = Solid::cylinder(h, r1, r2.max(0.0), seg, false);
            if anchor == "center" {
                man.translate([0.0, 0.0, -h / 2.0])
            } else {
                man
            }
        }
        "sphere" => {
            let r = float(p, "radius", Some(5.0))?;
            let man = Solid::sphere(r, seg);
            if anchor == "base" {
                man.translate([0.0, 0.0, r])
            } else {
                man
            }
        }
        "torus" => {
            let big_r = float(p, "radius", Some(10.0))?;
            let r = float(p, "radius2", Some(2.0))?;
            if r >= big_r {
                return Err(FeatureError("Tube radius must be smaller than the ring radius.".into()));
            }
            let circ_seg = (seg / 2).max(24);
            let mut circ = Shape2d::circle(r, circ_seg);
            for c in &mut circ.contours {
                for pt in c {
                    pt[0] += big_r;
                }
            }
            let man = circ.revolve(seg, 360.0);
            if anchor == "base" {
                man.translate([0.0, 0.0, r])
            } else {
                man
            }
        }
        other => return Err(FeatureError(format!("Unknown shape {other}."))),
    };
    let rot = vec3(p, "rotate", [0.0; 3])?;
    let r = rotation(rot[0], rot[1], rot[2]);
    man = man.transform(affine(r, [0.0; 3]));
    let offset = vec3(p, "offset", [0.0; 3])?;
    man = place(&man, &state.frame(p.get("plane"))?, offset);
    let name = match p.get("body_name").and_then(Value::as_str) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => capitalize(shape),
    };
    let op = p.get("operation").and_then(Value::as_str).unwrap_or("new");
    apply_tool(state, fid, man, op, p.get("targets"), &name, res)
}

pub fn register(r: Registry) -> Registry {
    r.with("primitive", f_primitive)
}
