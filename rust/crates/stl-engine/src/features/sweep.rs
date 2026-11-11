//! Sweep (1.x `f_sweep`): a profile carried along an open path of lines and arcs.

use serde_json::{Map, Value};
use stl_kernel::{cross, dot, triangulate, Vec3};

use super::common::{apply_tool, mesh_solid, path_points, path_self_crosses, profile_loop, rot_between, sketch};
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn norm(v: Vec3) -> f64 {
    dot(v, v).sqrt()
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: Vec3, s: f64) -> Vec3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

/// `v - 2 / c * (w . v) * w`, in the order numpy evaluates it.
fn reflect(v: Vec3, w: Vec3, c: f64) -> Vec3 {
    let k = 2.0 / c * dot(w, v);
    sub(v, scale(w, k))
}

fn f_sweep(
    state: &mut State,
    fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let psk = sketch(&state.sketches, p.get("profile").and_then(Value::as_str))?.clone();
    let lp = profile_loop(&psk, 1)?;
    let kn = lp.len();
    let path_sk = sketch(&state.sketches, p.get("path").and_then(Value::as_str))?.clone();
    let (po, px, py) = (path_sk.frame.o, path_sk.frame.x, path_sk.frame.y);
    let p2 = path_points(&path_sk)?;
    if path_self_crosses(&p2) {
        return Err(FeatureError("The sweep path crosses itself. Move the path so it doesn't cross.".into()));
    }
    let p3: Vec<Vec3> = p2
        .iter()
        .map(|q| {
            let mut v = [0.0; 3];
            for k in 0..3 {
                v[k] = po[k] + q[0] * px[k] + q[1] * py[k];
            }
            v
        })
        .collect();
    let m = p3.len();
    let dirs: Vec<Vec3> = (0..m - 1)
        .map(|i| {
            let d = sub(p3[i + 1], p3[i]);
            let l = norm(d);
            [d[0] / l, d[1] / l, d[2] / l]
        })
        .collect();
    let mut t = vec![[0.0; 3]; m];
    t[0] = dirs[0];
    t[m - 1] = dirs[m - 2];
    for i in 1..m - 1 {
        let sm = [dirs[i - 1][0] + dirs[i][0], dirs[i - 1][1] + dirs[i][1], dirs[i - 1][2] + dirs[i][2]];
        let l = norm(sm);
        t[i] = if l > 1e-9 { [sm[0] / l, sm[1] / l, sm[2] / l] } else { dirs[i] };
    }
    // the profile starts at the path's first point, turned to face along the path
    let (o_s, x_s, y_s, n_s) = (psk.frame.o, psk.frame.x, psk.frame.y, psk.frame.n);
    // the path's first point lands on the profile point under it, so a profile drawn around the
    // path start stays where it was drawn relative to the path
    let d0 = sub(p3[0], o_s);
    let shift = [dot(d0, x_s), dot(d0, y_s)];
    let lp: Vec<[f64; 2]> = lp.iter().map(|q| [q[0] - shift[0], q[1] - shift[1]]).collect();
    let a = rot_between(n_s, t[0]);
    let mut r = vec![[0.0; 3]; m];
    r[0] = [dot(a[0], x_s), dot(a[1], x_s), dot(a[2], x_s)];
    // rotation-minimising frames along the path (double reflection)
    for i in 0..m - 1 {
        let v1 = sub(p3[i + 1], p3[i]);
        let c1 = dot(v1, v1);
        let r_l = reflect(r[i], v1, c1);
        let t_l = reflect(t[i], v1, c1);
        let v2 = sub(t[i + 1], t_l);
        let c2 = dot(v2, v2);
        let rn = if c2 > 1e-18 { reflect(r_l, v2, c2) } else { r_l };
        let rn = sub(rn, scale(t[i + 1], dot(rn, t[i + 1])));
        let l = norm(rn);
        r[i + 1] = [rn[0] / l, rn[1] / l, rn[2] / l];
    }
    let sv: Vec<Vec3> = (0..m).map(|i| cross(t[i], r[i])).collect();
    let mut verts: Vec<Vec3> = Vec::with_capacity(m * kn);
    for i in 0..m {
        let mut q: Vec<Vec3> = lp
            .iter()
            .map(|l| {
                let mut v = [0.0; 3];
                for k in 0..3 {
                    v[k] = l[0] * r[i][k] + l[1] * sv[i][k];
                }
                v
            })
            .collect();
        if i > 0 && i < m - 1 {
            // at a bend the profile is stretched across the bend so the corner is mitred, not cut
            let c = dot(dirs[i - 1], dirs[i]);
            if c < -0.99 {
                return Err(FeatureError("The sweep path turns back on itself too sharply. Open up the bend.".into()));
            }
            let wv = sub(dirs[i], dirs[i - 1]);
            let l = norm(wv);
            if l > 1e-9 {
                let wv = [wv[0] / l, wv[1] / l, wv[2] / l];
                let f = 1.0 / ((1.0 + c) / 2.0).sqrt() - 1.0;
                for v in q.iter_mut() {
                    let g = f * dot(*v, wv);
                    for k in 0..3 {
                        v[k] += g * wv[k];
                    }
                }
            }
        }
        for v in q {
            verts.push([p3[i][0] + v[0], p3[i][1] + v[1], p3[i][2] + v[2]]);
        }
    }
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for i in 0..m - 1 {
        for k in 0..kn {
            let k2 = (k + 1) % kn;
            let (a, b, c, e) = (i * kn + k, i * kn + k2, (i + 1) * kn + k2, (i + 1) * kn + k);
            tris.push([a as u32, b as u32, c as u32]);
            tris.push([a as u32, c as u32, e as u32]);
        }
    }
    for (i, keep) in [(0usize, false), (m - 1, true)] {
        let base = (i * kn) as u32;
        for tr in triangulate(&[lp.clone()]) {
            let (a, b, c) = (tr[0], tr[1], tr[2]);
            tris.push(if keep { [base + a, base + b, base + c] } else { [base + a, base + c, base + b] });
        }
    }
    let man = mesh_solid(&verts, &tris, "sweep")?;
    let operation = p.get("operation").and_then(Value::as_str).unwrap_or("new");
    let name = p.get("body_name").and_then(Value::as_str).filter(|s| !s.is_empty()).unwrap_or("Sweep");
    apply_tool(state, fid, man, operation, p.get("targets"), name, res)
}

pub fn register(r: Registry) -> Registry {
    r.with("sweep", f_sweep)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;
    use stl_kernel::Frame;
    use crate::features::common::SketchRec;

    fn rec(frame: Frame, ents: Value) -> SketchRec {
        SketchRec { name: "s".into(), frame, entities: serde_json::from_value(ents).unwrap(), refs: Vec::new() }
    }

    fn run(state: &mut State, p: Value) -> Result<(), FeatureError> {
        let src = BTreeMap::new();
        let ctx = Ctx { sources: &src, preview: false };
        let mut res = FeatureResult::default();
        f_sweep(state, "f9", p.as_object().unwrap(), &mut res, &ctx)
    }

    fn setup(path: Value) -> State {
        let mut st = State::default();
        let xy = Frame { o: [0.0; 3], x: [1.0, 0.0, 0.0], y: [0.0, 1.0, 0.0], n: [0.0, 0.0, 1.0] };
        let xz = Frame { o: [0.0; 3], x: [1.0, 0.0, 0.0], y: [0.0, 0.0, 1.0], n: [0.0, -1.0, 0.0] };
        st.sketches.insert("pr".into(), rec(xy, json!([{"id":"r","type":"rect","x":-2,"y":-2,"w":4,"h":4}])));
        st.sketches.insert("pa".into(), rec(xz, path));
        st
    }

    #[test]
    fn straight_sweep_is_a_prism() {
        let mut st = setup(json!([{"id":"l","type":"line","p1":[0,0],"p2":[0,10]}]));
        run(&mut st, json!({"profile": "pr", "path": "pa"})).unwrap();
        let v = st.bodies["f9"].solid.as_ref().unwrap().volume();
        assert!((v - 160.0).abs() < 1e-3, "{v}");
    }

    #[test]
    fn crossing_and_missing_paths_are_refused() {
        let mut st = setup(json!([
            {"id":"a","type":"line","p1":[0,0],"p2":[10,10]},
            {"id":"b","type":"line","p1":[10,10],"p2":[10,0]},
            {"id":"c","type":"line","p1":[10,0],"p2":[0,10]}
        ]));
        let e = run(&mut st, json!({"profile": "pr", "path": "pa"})).unwrap_err();
        assert_eq!(e.0, "The sweep path crosses itself. Move the path so it doesn't cross.");
        assert!(run(&mut st, json!({"profile": "pr", "path": "nope"})).is_err());
    }
}
