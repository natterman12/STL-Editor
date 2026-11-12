//! `arc_path`, `wall_sections`, `walls` and `wall` from 1.x `supportfins.py`: hand-drawn walls.

use std::borrow::Cow;
use std::cell::OnceCell;
use std::f64::consts::TAU;

use serde_json::{json, Map, Value};
use stl_kernel::Solid;

use super::orient::nearest_face;
use super::results::solids;
use super::{
    cached_wall, call, call_json, decode_triangles, floats, has, mesh_key, options, parse_json, soup_json, truthy, MeshData, P3,
};
use crate::{FeatureError, FinsHost};

/// Python `x % m` for a positive `m`.
fn pymod(x: f64, m: f64) -> f64 {
    x.rem_euclid(m)
}

/// The circle through a, b and m in plan, sampled as a path from a to b (`None` when in line).
pub fn arc_path(a: P3, b: P3, m: P3, step: f64) -> Option<Vec<P3>> {
    let d = 2.0 * (a[0] * (b[1] - m[1]) + b[0] * (m[1] - a[1]) + m[0] * (a[1] - b[1]));
    if d.abs() < 1e-9 {
        return None;
    }
    let (sa, sb, sm) = (a[0] * a[0] + a[1] * a[1], b[0] * b[0] + b[1] * b[1], m[0] * m[0] + m[1] * m[1]);
    let cx = (sa * (b[1] - m[1]) + sb * (m[1] - a[1]) + sm * (a[1] - b[1])) / d;
    let cy = (sa * (m[0] - b[0]) + sb * (a[0] - m[0]) + sm * (b[0] - a[0])) / d;
    let r = (a[0] - cx).hypot(a[1] - cy);
    if !(r < 1e4) {
        return None;
    }
    let ang = |p: P3| (p[1] - cy).atan2(p[0] - cx);
    let a0 = ang(a);
    let (to_b, to_m) = (pymod(ang(b) - a0, TAU), pymod(ang(m) - a0, TAU));
    let sweep = if to_m < to_b { to_b } else { to_b - TAU };
    let fm = if sweep > 0.0 { to_m / sweep } else { (TAU - to_m) / -sweep };
    let n = ((sweep.abs() * r / step).ceil() as i64).max(2);
    let mut out = Vec::new();
    for k in 0..=n {
        let t = k as f64 / n as f64;
        let th = a0 + sweep * t;
        let z = if t <= fm {
            a[2] + (m[2] - a[2]) * (if fm > 0.0 { t / fm } else { 1.0 })
        } else {
            m[2] + (b[2] - m[2]) * ((t - fm) / (1.0 - fm))
        };
        out.push([cx + r * th.cos(), cy + r * th.sin(), z]);
    }
    Some(out)
}

/// One piece of a drawn wall: from `a` to `b`, through `m` when it is a curve.
pub type Section = (P3, P3, Option<P3>);

fn p3(v: &[f64]) -> P3 {
    [v.first().copied().unwrap_or(0.0), v.get(1).copied().unwrap_or(0.0), v.get(2).copied().unwrap_or(0.0)]
}

fn points_of(w: &Value) -> Vec<Vec<f64>> {
    w.get("points").and_then(Value::as_array).map(|a| a.iter().map(|q| floats(Some(q))).collect()).unwrap_or_default()
}

/// `wall_sections`: the pieces a drawn wall is built from.
pub fn wall_sections(w: &Value, curves: bool) -> Vec<Section> {
    let p: Vec<P3> = points_of(w).iter().map(|q| p3(q)).collect();
    if w.get("shape").and_then(Value::as_str) == Some("curve") && p.len() == 3 {
        if curves {
            return vec![(p[0], p[1], Some(p[2]))];
        }
        // an engine without the curve extra: short straight walls round the same arc
        return match arc_path(p[0], p[1], p[2], 5.0) {
            Some(path) => path.windows(2).map(|w| (w[0], w[1], None)).collect(),
            None => vec![(p[0], p[1], None)],
        };
    }
    (1..p.len()).map(|i| (p[i - 1], p[i], None)).collect()
}

/// What happened to one drawn wall.
#[derive(Clone, Debug, PartialEq)]
pub struct WallStat {
    pub ok: bool,
    pub length: f64,
    pub height: f64,
    pub tines: i64,
    pub part_attached: bool,
    pub sections: usize,
    pub failed: usize,
    pub reason: Option<String>,
}

pub struct WallsResult {
    pub solids: Vec<Solid>,
    pub by_wall: Vec<Vec<Solid>>,
    pub walls: Vec<WallStat>,
    pub version: String,
}

fn j(v: &impl serde::Serialize) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

/// The body's soup as a JSON string argument, made the first time a call needs it.
fn get_soup<'m>(mesh: &'m MeshData, sc: &'m OnceCell<Cow<'m, str>>) -> &'m str {
    sc.get_or_init(|| soup_json(mesh))
}

fn fail(reason: &str) -> Value {
    json!({"ok": false, "reason": reason})
}

/// `walls`: hand-drawn walls (Support Fins' Draw mode): lines, lines of several sections, curves.
pub fn walls(host: &dyn FinsHost, mesh: &MeshData, wall_list: &[Value], values: &Value) -> Result<WallsResult, FeatureError> {
    let opts = options(host, values)?;
    let mk = mesh_key(mesh);
    let curves = has(host, "drawWallThroughB64");
    let soup_cache: OnceCell<Cow<str>> = OnceCell::new();

    let mut all = Vec::new();
    let mut report = Vec::new();
    let mut by_wall = Vec::new();
    for w in wall_list {
        let mut mine: Vec<Solid> = Vec::new();
        let mut st = WallStat {
            ok: true,
            length: 0.0,
            height: 0.0,
            tines: 0,
            part_attached: false,
            sections: 0,
            failed: 0,
            reason: None,
        };
        let shape = w.get("shape").and_then(Value::as_str).unwrap_or("");
        let hand = shape == "gusset" || shape == "anchor";
        let sections: Vec<Option<Section>> = if hand { vec![None] } else { wall_sections(w, curves).into_iter().map(Some).collect() };
        for sec in sections {
            let r: Value = match sec {
                None => hand_piece(host, mesh, &mk, &soup_cache, w, values, &opts)?,
                Some((a, b, m)) => {
                    let key = format!("{mk}|{}|{opts}", j(&json!([a, b, m])));
                    cached_wall(key, || {
                        // every argument is JSON text: the soup already quoted, the rest quoted here
                        let s = get_soup(mesh, &soup_cache);
                        let (qa, qb, qo) = (j(&j(&a)), j(&j(&b)), j(&opts));
                        let r = if m.is_some() && curves {
                            call_json(host, "drawWallThroughB64", &[s, &qa, &qb, &j(&j(&m.unwrap())), &qo])?
                        } else {
                            call_json(host, "drawWallB64", &[s, &qa, &qb, &qo])?
                        };
                        parse_json(&r)
                    })?
                }
            };
            st.sections += 1;
            if !truthy(r.get("ok")) {
                st.failed += 1;
                if st.reason.is_none() {
                    st.reason = r.get("reason").and_then(Value::as_str).map(str::to_string);
                }
                continue;
            }
            let tri = decode_triangles(r["triangles"].as_str().unwrap_or_default())?;
            let off = |k: &str| r["offset"].get(k).and_then(Value::as_f64).unwrap_or(0.0);
            let (got, _) = solids(&tri, [off("x"), off("y"), off("z")], &[vec![(0, tri.len())]], None);
            if got.is_empty() {
                st.failed += 1;
                if st.reason.is_none() {
                    st.reason = Some("it didn’t come out as a closed solid".into());
                }
                continue;
            }
            all.extend(got.iter().cloned());
            mine.extend(got);
            let s = &r["stats"];
            st.length += s.get("length").and_then(Value::as_f64).unwrap_or(0.0);
            st.height = st.height.max(s.get("height").and_then(Value::as_f64).unwrap_or(0.0));
            st.tines += s.get("tines").and_then(Value::as_f64).unwrap_or(0.0) as i64;
            st.part_attached = st.part_attached || truthy(s.get("partAttached"));
        }
        st.ok = st.failed < st.sections;
        report.push(st);
        by_wall.push(mine);
    }
    Ok(WallsResult { solids: all, by_wall, walls: report, version: super::version(host) })
}

/// A gusset (off a side, out to an overhang) or an anchor fin (against a side), by hand.
#[allow(clippy::too_many_arguments)]
fn hand_piece<'m>(
    host: &dyn FinsHost,
    mesh: &'m MeshData,
    mk: &str,
    soup_cache: &'m OnceCell<Cow<'m, str>>,
    w: &Value,
    values: &Value,
    opts: &str,
) -> Result<Value, FeatureError> {
    let pts = points_of(w);
    let n: Vec<f64> = match w.get("normal") {
        Some(v) if truthy(Some(v)) => floats(Some(v)),
        _ => vec![0.0, 0.0, 0.0],
    };
    if w.get("shape").and_then(Value::as_str) == Some("gusset") {
        if !has(host, "gussetWallB64") {
            return Ok(fail("this Support Fins engine has no gussets"));
        }
        if pts.len() < 2 {
            return Ok(fail("click the side, then the overhang"));
        }
        return cached_wall(format!("{mk}|g{}|{opts}", j(&json!([pts, n]))), || {
            let s = get_soup(mesh, soup_cache);
            let (a, b, nn, o) = (j(&j(&pts[0])), j(&j(&pts[1])), j(&j(&n)), j(&opts));
            parse_json(&call_json(host, "gussetWallB64", &[s, &a, &b, &nn, &o])?)
        });
    }
    if !has(host, "anchorFinB64") {
        return Ok(fail("this Support Fins engine has no anchor fins"));
    }
    let p0 = p3(&pts[0]);
    let face = nearest_face(mesh, p0, p3(&n));
    if face < 0 {
        return Ok(fail("the face it stood on isn’t there any more"));
    }
    let mut with_sway: Map<String, Value> = values.as_object().cloned().unwrap_or_default();
    with_sway.insert("sway.on".into(), Value::Bool(true));
    let sway = match call(host, "optionsFromDialogJson", &[&j(&Value::Object(with_sway))]) {
        Ok(t) => {
            let v = parse_json(&t)?;
            match v.get("sway") {
                Some(sw) if truthy(Some(sw)) => sw.clone(),
                _ => json!({}),
            }
        }
        Err(_) => json!({}),
    };
    cached_wall(format!("{mk}|a{}|{opts}", j(&json!([pts[0], face]))), || {
        // every argument is JSON text: the strings quoted, the face a number
        let s = get_soup(mesh, soup_cache);
        let (a0, o, sw) = (j(&j(&pts[0])), j(&opts), j(&j(&sway)));
        parse_json(&call_json(host, "anchorFinB64", &[s, &a0, &face.to_string(), &o, &sw])?)
    })
}

/// `wall`: one hand-drawn wall between two points on the body (kept for older callers).
pub fn wall(host: &dyn FinsHost, mesh: &MeshData, a: P3, b: P3, values: &Value) -> Result<(Vec<Solid>, WallStat, String), FeatureError> {
    let r = walls(host, mesh, &[json!({"shape": "line", "points": [a, b]})], values)?;
    let w = r.walls[0].clone();
    if !w.ok {
        return Err(FeatureError(w.reason.clone().unwrap_or_else(|| "Support Fins couldn't place a wall there.".into())));
    }
    Ok((r.solids, w, r.version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_through_three_points() {
        let path = arc_path([10.0, 0.0, 0.0], [-10.0, 0.0, 0.0], [0.0, 10.0, 4.0], 4.0).unwrap();
        assert!(path.len() >= 3);
        let (f, l) = (path[0], path[path.len() - 1]);
        assert!((f[0] - 10.0).abs() < 1e-9 && (l[0] + 10.0).abs() < 1e-9);
        assert!(path.iter().all(|p| (p[0].hypot(p[1]) - 10.0).abs() < 1e-9));
        assert!(arc_path([0.0, 0.0, 0.0], [1.0, 1.0, 0.0], [2.0, 2.0, 0.0], 4.0).is_none());
    }
}
