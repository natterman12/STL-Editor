use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

use crate::{Ctx, FeatureError, FeatureResult, State};

/// Runs one feature: `(state, feature id, params, result, ctx)`. It changes `state` in place;
/// on `Err` (or a panic) the document throws that state away and keeps the one before.
/// Evaluators must be deterministic: they re-run when an earlier step changes.
pub type Evaluator =
    fn(&mut State, &str, &Map<String, Value>, &mut FeatureResult, &Ctx<'_>) -> Result<(), FeatureError>;

/// Feature type -> evaluator.
#[derive(Clone, Default)]
pub struct Registry {
    map: BTreeMap<&'static str, Evaluator>,
}

impl Registry {
    pub fn new() -> Registry {
        Registry::default()
    }

    /// The feature types of the app. Empty in milestone 1; milestone 2 fills it.
    pub fn builtin() -> Registry {
        crate::features::register_all(Registry::new())
    }

    pub fn with(mut self, kind: &'static str, f: Evaluator) -> Registry {
        self.map.insert(kind, f);
        self
    }

    pub fn has(&self, kind: &str) -> bool {
        self.map.contains_key(kind)
    }

    pub fn get(&self, kind: &str) -> Option<Evaluator> {
        self.map.get(kind).copied()
    }

    /// Named capabilities the port has (e.g. `"voxel"`, `"offset"`). Add one when
    /// its tools are registered; golden cases whose `requires` are not all in this set stay pending.
    pub fn capabilities(&self) -> BTreeSet<&'static str> {
        BTreeSet::from(["voxel", "offset"])
    }
}
