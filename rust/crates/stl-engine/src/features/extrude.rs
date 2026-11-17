//! 1.x `f_extrude`: push a sketch profile out into a solid, then add it, cut it or keep the
//! overlap.

use serde_json::{Map, Value};
use stl_kernel::Shape2d;
use stl_sketch::P2;

use super::common::{apply_tool, big_extent, float, frame_matrix, material_volume, pick, sketch, strings, truthy};
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn extrude(
    state: &mut State,
    fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let sk = sketch(&state.sketches, p.get("sketch").and_then(Value::as_str))?.clone();
    let (only, regions) = pick(p);
    let cs = stl_sketch::profile::profile_with(&sk.entities, only.as_deref(), regions.as_deref())
        .map_err(|e| FeatureError(e.0))?;
    let op = match p.get("operation") {
        None => "new".to_string(),
        Some(v) => v.as_str().unwrap_or("").to_string(),
    };
    let through_all = truthy(p.get("through_all"));
    let mut d1 = if through_all {
        big_extent(state.bodies.values())
    } else {
        float(p, "distance", Some(10.0))?
    };
    let mut direction = match p.get("direction") {
        None => "one".to_string(),
        Some(v) => v.as_str().unwrap_or("").to_string(),
    };
    let mut d2 = if direction == "two" {
        float(p, "distance2", Some(0.0))?
    } else {
        0.0
    };
    if direction == "two" && through_all {
        d2 = d1;
    }
    if d1.abs() < 1e-9 && d2.abs() < 1e-9 {
        return Err(FeatureError("Distance can't be zero.".into()));
    }
    if d1 < 0.0 {
        direction = match direction.as_str() {
            "one" => "reverse".into(),
            "reverse" => "one".into(),
            other => other.to_string(),
        };
        d1 = -d1;
    }
    let start = float(p, "start_offset", Some(0.0))?;
    let (z0, h) = match direction.as_str() {
        "one" => (0.0, d1),
        "reverse" => (-d1, d1),
        "symmetric" => (-d1 / 2.0, d1),
        "two" => (-d2, d1 + d2),
        other => return Err(FeatureError(format!("Unknown direction {other}."))),
    };
    let taper = if truthy(p.get("taper")) {
        float(p, "taper", Some(0.0))?
    } else {
        0.0
    };
    let man = if taper != 0.0 {
        // approximate taper as a uniform scale of the profile about its centroid
        let (lo, hi) = cs.bounds().unwrap_or(([0.0; 2], [0.0; 2]));
        let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
        let half = f64::max(hi[0] - lo[0], hi[1] - lo[1]) / 2.0;
        let k = if half > 0.0 {
            f64::max(0.0, (half - h * taper.to_radians().tan()) / half)
        } else {
            1.0
        };
        let moved: Vec<Vec<P2>> = cs
            .contours
            .iter()
            .map(|c0| c0.iter().map(|q| [q[0] - c[0], q[1] - c[1]]).collect())
            .collect();
        Shape2d { contours: moved }
            .extrude_scaled(h, k)
            .translate([c[0], c[1], z0 + start])
    } else {
        cs.extrude(h).translate([0.0, 0.0, z0 + start])
    };
    let m = frame_matrix(&sk.frame);
    let mut tool = man.transform(m);
    let auto_flip = p.get("auto_flip").is_none_or(|v| truthy(Some(v)));
    if (op == "cut" || op == "intersect") && (direction == "one" || direction == "reverse") && auto_flip {
        // sketching on a face and cutting: go into the material, whichever side that is
        let targets = strings(p.get("targets"));
        let eps = f64::max(tool.volume() * 1e-5, 1e-9);
        if material_volume(&state.bodies, &tool, &targets) <= eps {
            let flipped = man.mirror([0.0, 0.0, 1.0]).transform(m);
            if material_volume(&state.bodies, &flipped, &targets) > eps {
                tool = flipped;
                res.flipped = true;
            }
        }
    }
    let name = match p.get("body_name") {
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        _ => "Extrude".to_string(),
    };
    if op == "cut" {
        res.set_cutter(ctx.preview, &tool);
        if taper == 0.0 && (direction == "one" || direction == "reverse") {
            tool = open_the_face(state, p, &cs, &tool, m, direction == "one", z0 + start, h, res.flipped);
        }
    }
    apply_tool(state, fid, tool, &op, p.get("targets"), &name, res)
}

/// How far a cut starting on the part's surface reaches back out of it.
const OVERSHOOT: f64 = 0.01;

/// A one-sided cut sketched on a face starts exactly on that face, and the face picked from
/// the mesh can sit a rounding error inside the part (x = 9.999999999999998 on a face at 10).
/// The cut then stops just under the surface and leaves a skin thinner than a hair over the
/// pocket. When the space just outside the cut's start is empty, start the cut `OVERSHOOT`
/// further out. A start inside the material (a sketch on a plane through the part) keeps its
/// exact depth.
#[allow(clippy::too_many_arguments)]
fn open_the_face(
    state: &State,
    p: &Map<String, Value>,
    cs: &Shape2d,
    tool: &stl_kernel::Solid,
    m: [[f64; 4]; 3],
    one: bool,
    lo: f64,
    h: f64,
    flipped: bool,
) -> stl_kernel::Solid {
    let place = |s: stl_kernel::Solid| {
        if flipped {
            s.mirror([0.0, 0.0, 1.0]).transform(m)
        } else {
            s.transform(m)
        }
    };
    // the start face is the bottom of a "one" cut, the top of a "reverse" one (before flipping)
    let slab_at = if one { lo - OVERSHOOT } else { lo + h };
    let slab = place(cs.extrude(OVERSHOOT).translate([0.0, 0.0, slab_at]));
    let targets = strings(p.get("targets"));
    if material_volume(&state.bodies, &slab, &targets) > slab.volume() * 0.5 {
        return tool.clone();
    }
    let from = if one { lo - OVERSHOOT } else { lo };
    place(cs.extrude(h + OVERSHOOT).translate([0.0, 0.0, from]))
}

pub fn register(r: Registry) -> Registry {
    r.with("extrude", extrude)
}
