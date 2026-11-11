//! Loft (1.x `f_loft`): a solid through two or more sketch profiles on parallel planes.

use serde_json::{Map, Value};
use stl_kernel::{cross, dot, triangulate, Vec3};

use super::common::{align, apply_tool, mesh_solid, profile_loop, resample, sketch};
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn norm(v: Vec3) -> f64 {
    dot(v, v).sqrt()
}

fn f_loft(
    state: &mut State,
    fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let ids: Vec<String> = match p.get("profiles") {
        Some(Value::Array(a)) => {
            a.iter().filter_map(|x| x.as_str()).filter(|s| !s.is_empty()).map(String::from).collect()
        }
        _ => Vec::new(),
    };
    if ids.len() < 2 {
        return Err(FeatureError("A loft needs at least two sketch profiles, at different heights.".into()));
    }
    let mut sks = Vec::with_capacity(ids.len());
    for sid in &ids {
        sks.push(sketch(&state.sketches, Some(sid))?.clone());
    }
    let mut lines = Vec::with_capacity(sks.len());
    for (i, sk) in sks.iter().enumerate() {
        lines.push(profile_loop(sk, i + 1)?);
    }
    let nn = lines.iter().map(|l| l.len()).max().unwrap_or(0);
    let n_ref = sks[0].frame.n;
    let mut loops: Vec<Vec<Vec3>> = Vec::new();
    let mut heights: Vec<f64> = Vec::new();
    for (sk, lp) in sks.iter().zip(lines.iter()) {
        let f = &sk.frame;
        if dot(f.n, n_ref).abs() < 1.0 - 1e-6 {
            return Err(FeatureError(
                "The loft profiles must lie on parallel planes (sketch them on the same plane or on parallel planes)."
                    .into(),
            ));
        }
        let mut pts = resample(lp, nn);
        if dot(f.n, n_ref) < 0.0 {
            pts.reverse(); // keep every loop turning the same way round the loft direction
        }
        let mut p3: Vec<Vec3> = pts
            .iter()
            .map(|q| {
                let mut v = [0.0; 3];
                for k in 0..3 {
                    v[k] = f.o[k] + q[0] * f.x[k] + q[1] * f.y[k];
                }
                v
            })
            .collect();
        if let Some(last) = loops.last() {
            p3 = align(last, &p3);
        }
        loops.push(p3);
        heights.push(dot(f.o, n_ref));
    }
    let d: Vec<f64> = heights.windows(2).map(|w| w[1] - w[0]).collect();
    let all_pos = d.iter().all(|&x| x > 0.0);
    let all_neg = d.iter().all(|&x| x < 0.0);
    if d.iter().any(|x| x.abs() < 1e-6) || !(all_pos || all_neg) {
        return Err(FeatureError(
            "The loft profiles must be at different heights, one after another in a single direction (in the order you picked them)."
                .into(),
        ));
    }
    let s = if d[0] > 0.0 { 1.0 } else { -1.0 };
    let jn = loops.len();
    let verts: Vec<Vec3> = loops.iter().flatten().copied().collect();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for j in 0..jn - 1 {
        let (lo, up) = if s > 0.0 { (j, j + 1) } else { (j + 1, j) };
        for k in 0..nn {
            let k2 = (k + 1) % nn;
            let (a, b, c, e) = (lo * nn + k, lo * nn + k2, up * nn + k2, up * nn + k);
            tris.push([a as u32, b as u32, c as u32]);
            tris.push([a as u32, c as u32, e as u32]);
        }
    }
    let mut e1 = cross(n_ref, [0.0, 0.0, 1.0]);
    if norm(e1) < 1e-6 {
        e1 = cross(n_ref, [1.0, 0.0, 0.0]);
    }
    let l = norm(e1);
    let e1 = [e1[0] / l, e1[1] / l, e1[2] / l];
    let e2 = cross(n_ref, e1);
    for (j, keep) in [(0usize, s < 0.0), (jn - 1, s > 0.0)] {
        // the end caps: triangles wound to face away from the solid
        let p2: Vec<[f64; 2]> = loops[j].iter().map(|q| [dot(*q, e1), dot(*q, e2)]).collect();
        let base = (j * nn) as u32;
        for t in triangulate(&[p2]) {
            let (a, b, c) = (t[0], t[1], t[2]);
            tris.push(if keep { [base + a, base + b, base + c] } else { [base + a, base + c, base + b] });
        }
    }
    let man = mesh_solid(&verts, &tris, "loft")?;
    let operation = p.get("operation").and_then(Value::as_str).unwrap_or("new");
    let name = p.get("body_name").and_then(Value::as_str).filter(|s| !s.is_empty()).unwrap_or("Loft");
    apply_tool(state, fid, man, operation, p.get("targets"), name, res)
}

pub fn register(r: Registry) -> Registry {
    r.with("loft", f_loft)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;
    use stl_kernel::Frame;
    use crate::features::common::SketchRec;

    fn rec(frame: Frame, ents: Value) -> SketchRec {
        SketchRec {
            name: "s".into(),
            frame,
            entities: serde_json::from_value(ents).unwrap(),
            refs: Vec::new(),
        }
    }

    fn xy(z: f64) -> Frame {
        Frame { o: [0.0, 0.0, z], x: [1.0, 0.0, 0.0], y: [0.0, 1.0, 0.0], n: [0.0, 0.0, 1.0] }
    }

    fn run(state: &mut State, p: Value) -> Result<FeatureResult, FeatureError> {
        let src = BTreeMap::new();
        let ctx = Ctx { sources: &src, preview: false };
        let mut res = FeatureResult::default();
        f_loft(state, "f9", p.as_object().unwrap(), &mut res, &ctx).map(|_| res)
    }

    #[test]
    fn loft_two_squares_is_a_prismatoid() {
        let mut st = State::default();
        st.sketches.insert("a".into(), rec(xy(0.0), json!([{"id":"r","type":"rect","x":-5,"y":-5,"w":10,"h":10}])));
        st.sketches.insert("b".into(), rec(xy(10.0), json!([{"id":"r","type":"rect","x":-4,"y":-4,"w":8,"h":8}])));
        run(&mut st, json!({"profiles": ["a", "b"]})).unwrap();
        let v = st.bodies["f9"].solid.as_ref().unwrap().volume();
        assert!((v - 10.0 / 3.0 * (100.0 + 64.0 + 80.0)).abs() < 1e-3, "{v}");
    }

    #[test]
    fn loft_errors() {
        let mut st = State::default();
        st.sketches.insert("a".into(), rec(xy(0.0), json!([{"id":"r","type":"rect","x":-5,"y":-5,"w":10,"h":10}])));
        st.sketches.insert("b".into(), rec(xy(0.0), json!([{"id":"r","type":"rect","x":-4,"y":-4,"w":8,"h":8}])));
        assert!(run(&mut st, json!({"profiles": ["a"]})).unwrap_err().0.starts_with("A loft needs at least two"));
        assert!(run(&mut st, json!({"profiles": ["a", "b"]})).unwrap_err().0.contains("at different heights"));
        assert!(run(&mut st, json!({"profiles": ["a", "zz"]})).is_err());
    }
}
