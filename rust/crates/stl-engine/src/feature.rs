use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

use crate::FeatureError;

/// One step of the timeline, as it is stored in a project. Keys this version doesn't know are
/// kept in `extra`, so a project from another version saves back unchanged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "RawFeature")]
pub struct Feature {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub params: Map<String, Value>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub suppressed: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// How a feature is read. A stored `"suppressed": false` (1.x writes it after an edit) is parked
/// in `extra`, so it is written back; absent still means false, and `Document` doesn't write it.
#[derive(Deserialize)]
struct RawFeature {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    params: Map<String, Value>,
    #[serde(default)]
    suppressed: Option<bool>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

impl From<RawFeature> for Feature {
    fn from(r: RawFeature) -> Self {
        let mut extra = r.extra;
        if r.suppressed == Some(false) {
            extra.insert("suppressed".into(), Value::Bool(false));
        }
        Feature {
            id: r.id,
            kind: r.kind,
            name: r.name,
            params: r.params,
            suppressed: r.suppressed.unwrap_or(false),
            extra,
        }
    }
}

/// An imported file kept inside the project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub name: String,
    /// Lower case, with the dot (`".stl"`).
    pub ext: String,
    pub data: Vec<u8>,
}

/// What running one feature gave.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FeatureResult {
    pub id: String,
    pub error: Option<String>,
    pub warning: Option<String>,
    pub created: Vec<String>,
    pub modified: Vec<String>,
    pub removed: Vec<String>,
    /// Feature id of a plane this step created (1.x `created_plane`).
    pub created_plane: Option<String>,
    pub ms: f64,
    pub suppressed: bool,
    /// The result is a preview-only picture (not what committing would give), so it isn't reused.
    pub preview_only: bool,
}

/// What an evaluator can reach besides the state.
pub struct Ctx<'a> {
    pub sources: &'a BTreeMap<String, Source>,
    pub preview: bool,
}

impl Ctx<'_> {
    pub fn source(&self, sid: &str) -> Result<&Source, FeatureError> {
        self.sources
            .get(sid)
            .ok_or_else(|| FeatureError("The imported file is missing from the project.".into()))
    }
}

/// What `edit_feature` does with the rollback bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollbackAfter {
    Keep,
    Set(Option<usize>),
}
