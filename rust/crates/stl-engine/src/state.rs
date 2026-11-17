use std::sync::Arc;

use indexmap::IndexMap;
use stl_kernel::{MeshArrays, Solid};

use crate::FeatureError;
use crate::features::common::{PlaneDef, SketchRec, ToolRecord};

/// A body is either a watertight solid, or a raw, broken mesh that only displays and exports
/// until it is repaired.
#[derive(Clone)]
pub struct Body {
    pub name: String,
    pub solid: Option<Solid>,
    pub mesh: Option<Arc<MeshArrays>>,
}

impl Body {
    pub fn require_solid(&self) -> Result<&Solid, FeatureError> {
        self.solid.as_ref().ok_or_else(|| {
            FeatureError(format!(
                "\"{}\" is not a watertight solid. Run Repair on it first.",
                self.name
            ))
        })
    }
}

/// What the timeline has built so far: the bodies, in the order they were made.
#[derive(Clone, Default)]
pub struct State {
    pub bodies: IndexMap<String, Body>,
    /// Planes the user made, by feature id.
    pub planes: IndexMap<String, PlaneDef>,
    /// Sketches the Sketch features stored, by feature id.
    pub sketches: IndexMap<String, SketchRec>,
    /// The tool solid each feature used, so mirror and pattern can repeat the feature.
    pub tools: IndexMap<String, ToolRecord>,
    /// Supports made as a body of their own (Support Fins or Draw Walls with output
    /// "separate"), by body id: the part they hold up. A Split shares them out between the
    /// halves and a Move of the part carries them along
    /// ([`crate::features::supports_follow`]).
    pub attached: IndexMap<String, String>,
}

/// A 3x4 affine transform (rows), as `affine` builds them.
pub type Mat34 = [[f64; 4]; 3];

impl State {
    /// A body was moved, rotated or mirrored in place: the sketches on its faces go with it
    /// (spec 2.9, Task 8).
    ///
    /// Each sketch whose plane was picked on `body` gets its frame moved by `m`: the origin by
    /// the whole transform, the x, y and normal directions by its 3x3 part. The stored plane
    /// value is never rewritten; this runs every time the project runs, so a sketch follows
    /// every later Move, Rotate or Mirror of its body in timeline order. A mirror leaves the
    /// frame left-handed, which keeps the sketch's points exactly on the mirrored face.
    pub fn record_transform(&mut self, body: &str, m: Mat34) {
        let lin = |v: [f64; 3]| [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2]);
        for s in self.sketches.values_mut() {
            if s.body.as_deref() != Some(body) {
                continue;
            }
            let o = lin(s.frame.o);
            s.frame.o = [o[0] + m[0][3], o[1] + m[1][3], o[2] + m[2][3]];
            s.frame.x = lin(s.frame.x);
            s.frame.y = lin(s.frame.y);
            s.frame.n = lin(s.frame.n);
        }
    }

    pub fn body(&self, bid: &str) -> Result<&Body, FeatureError> {
        self.bodies
            .get(bid)
            .ok_or_else(|| FeatureError(format!("Body '{bid}' no longer exists.")))
    }
}
