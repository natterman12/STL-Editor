use std::sync::Arc;

use indexmap::IndexMap;
use stl_kernel::{MeshArrays, Solid};

use crate::features::common::{PlaneDef, ToolRecord};
use crate::FeatureError;

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
            FeatureError(format!("\"{}\" is not a watertight solid. Run Repair on it first.", self.name))
        })
    }
}

/// What the timeline has built so far: the bodies, in the order they were made.
#[derive(Clone, Default)]
pub struct State {
    pub bodies: IndexMap<String, Body>,
    /// Planes the user made, by feature id.
    pub planes: IndexMap<String, PlaneDef>,
    /// The tool solid each feature used, so mirror and pattern can repeat the feature.
    pub tools: IndexMap<String, ToolRecord>,
}

impl State {
    pub fn body(&self, bid: &str) -> Result<&Body, FeatureError> {
        self.bodies.get(bid).ok_or_else(|| FeatureError(format!("Body '{bid}' no longer exists.")))
    }
}
