//! 1.x's `info` route (Properties): one body's size, area, volume and genus.

use serde_json::{json, Value};
use stl_kernel::MeshArrays;

use crate::{Document, FeatureError, State};

/// `{"name", "solid": bool, "size": [x, y, z], "bbox": [[min], [max]], "triangles", "vertices",
/// "area", "volume": f64 | null, "genus": i64 (-1 unknown)}`, as 1.x's `r_info` (`Body.info()`).
/// A body that isn't a watertight solid has no volume and no genus.
pub fn body_info(state: &State, id: &str) -> Result<Value, FeatureError> {
    let b = state.body(id)?;
    let mesh: MeshArrays = match (&b.solid, &b.mesh) {
        (Some(s), _) => s.to_mesh(),
        (None, Some(m)) => (**m).clone(),
        (None, None) => MeshArrays { positions: Vec::new(), triangles: Vec::new() },
    };
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for p in mesh.positions.chunks_exact(3) {
        for i in 0..3 {
            lo[i] = lo[i].min(f64::from(p[i]));
            hi[i] = hi[i].max(f64::from(p[i]));
        }
    }
    if mesh.positions.len() < 3 {
        (lo, hi) = ([0.0; 3], [0.0; 3]);
    }
    let size = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    let (volume, area, genus) = match &b.solid {
        Some(s) => (Value::from(s.volume()), s.surface_area(), i64::from(s.genus())),
        None => (Value::Null, mesh_area(&mesh), -1),
    };
    Ok(json!({
        "name": b.name,
        "solid": b.solid.is_some(),
        "size": size,
        "bbox": [lo, hi],
        "triangles": mesh.triangles.len() / 3,
        "vertices": mesh.positions.len() / 3,
        "area": area,
        "volume": volume,
        "genus": genus,
    }))
}

/// The summed area of a mesh's triangles (`_area` in 1.x geometry.py).
fn mesh_area(m: &MeshArrays) -> f64 {
    let p = |i: u32| {
        let k = i as usize * 3;
        [f64::from(m.positions[k]), f64::from(m.positions[k + 1]), f64::from(m.positions[k + 2])]
    };
    m.triangles
        .chunks_exact(3)
        .map(|t| {
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
            let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt() / 2.0
        })
        .sum()
}

impl Document {
    /// [`body_info`] on the current model, under the name the user gave the body (`r_info`).
    pub fn body_info(&self, id: &str) -> Result<Value, FeatureError> {
        let mut v = body_info(&self.state, id)?;
        v["name"] = Value::from(self.body_name(id));
        Ok(v)
    }
}
