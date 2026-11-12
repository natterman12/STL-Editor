//! `nearest_face`, `suggest` and `strength` from 1.x `supportfins.py`: the orientation helpers
//! the Support Fins panel uses (milestone 4 draws them).

use serde_json::Value;
use stl_mesh::nearest::closest_point_on_triangles;

use super::{call, call_json, cached_json, has, mesh_key, options, parse_json, soup, MeshData, P3};
use crate::{FeatureError, FinsHost};

/// The triangle nearest `p` whose normal is close to `n` (how a hand-placed anchor finds its face
/// again), or -1.
pub fn nearest_face(mesh: &MeshData, p: P3, n: P3) -> i64 {
    let sub = |a: P3, b: P3| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let ln_n = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    let nn = if ln_n == 0.0 { n } else { [n[0] / ln_n, n[1] / ln_n, n[2] / ln_n] };
    let mut idx = Vec::new();
    let mut tris = Vec::new();
    for (i, f) in mesh.faces.iter().enumerate() {
        let (a, b, c) = (mesh.verts[f[0]], mesh.verts[f[1]], mesh.verts[f[2]]);
        let (u, v) = (sub(b, a), sub(c, a));
        let nrm = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let ln = (nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]).sqrt();
        if ln > 0.0 && (nrm[0] * nn[0] + nrm[1] * nn[1] + nrm[2] * nn[2]) / ln >= 0.85 {
            idx.push(i);
            tris.push([a, b, c]);
        }
    }
    if idx.is_empty() {
        return -1;
    }
    let pts = vec![p; idx.len()];
    let closest = closest_point_on_triangles(&tris, &pts);
    let mut best = (f64::INFINITY, 0usize);
    for (k, c) in closest.iter().enumerate() {
        let d = ((c[0] - p[0]).powi(2) + (c[1] - p[1]).powi(2) + (c[2] - p[2]).powi(2)).sqrt();
        if d < best.0 {
            best = (d, k);
        }
    }
    if best.0 < 1.0 { idx[best.1] as i64 } else { -1 }
}

/// Support Fins' Suggest: up to `top` print orientations, best first (rot: applied to the part as
/// it is).
pub fn suggest(host: &dyn FinsHost, mesh: &MeshData, values: &Value, top: i64) -> Result<Value, FeatureError> {
    if !has(host, "suggestPosesJson") {
        return Err(FeatureError("This Support Fins engine can’t suggest orientations.".into()));
    }
    let opts = options(host, values)?;
    let key = format!("{}|suggest{top}|{opts}", mesh_key(mesh));
    cached_json(key, || {
        let (q, o) = (serde_json::to_string(&soup(mesh)).unwrap_or_default(), serde_json::to_string(&opts).unwrap_or_default());
        parse_json(&call_json(host, "suggestPosesJson", &[&q, &o, &top.to_string()])?)
    })
}

/// How strong the current pose is, and for a load pointing `load` (as the part sits), if given.
pub fn strength(host: &dyn FinsHost, mesh: &MeshData, values: &Value, load: Option<&Value>) -> Result<Value, FeatureError> {
    if !has(host, "strengthOfJson") {
        return Err(FeatureError("This Support Fins engine has no strength check.".into()));
    }
    let opts = options(host, values)?;
    let s = soup(mesh);
    let load = load.filter(|v| super::truthy(Some(v)));
    let load_json = load.map(|v| serde_json::to_string(v).unwrap_or_default()).unwrap_or_default();
    let mut r = parse_json(&call(host, "strengthOfJson", &[&s, &opts, &load_json])?)?;
    let quality_good = r.get("load").and_then(|l| l.get("quality")).and_then(Value::as_str) == Some("good");
    let strongest = match load {
        Some(_) if !quality_good => parse_json(&call(host, "strongestPoseJson", &[&s, &opts, &load_json])?)?,
        _ => Value::Null,
    };
    if let Value::Object(o) = &mut r {
        o.insert("strongest".into(), strongest);
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_face_it_stood_on() {
        // two triangles of a unit square at z = 0 (normal +z) and one wall facing +x
        let mesh = MeshData {
            verts: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0], [1.0, 0.0, 1.0]],
            faces: vec![[0, 1, 2], [0, 2, 3], [1, 2, 4]],
            memo: None,
        };
        assert_eq!(nearest_face(&mesh, [0.2, 0.5, 0.3], [0.0, 0.0, 1.0]), 1);
        assert_eq!(nearest_face(&mesh, [0.2, 0.5, 5.0], [0.0, 0.0, 1.0]), -1); // too far
        assert_eq!(nearest_face(&mesh, [0.2, 0.5, 0.0], [0.0, 0.0, -1.0]), -1); // no face looks that way
    }
}
