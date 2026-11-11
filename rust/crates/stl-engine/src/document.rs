//! The design document: imported sources and an ordered, editable feature history.
//!
//! A port of `Document` in `stleditor/document.py` (everything but the file handling, which is
//! `.stlx` and platform work). Messages keep the 1.x wording; deliberate differences are in
//! `rust/tests/EXCEPTIONS.md`.

use indexmap::IndexMap;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::labels::label;
use crate::params::{Param, RESERVED, Resolved, is_valid_name, resolve_deep, resolve_params, uses_expr};
use crate::{Ctx, Feature, FeatureError, FeatureResult, Registry, RollbackAfter, Source, State};

/// Undo steps kept.
const UNDO_LIMIT: usize = 200;
/// Recent results kept by hash, so a trial run or a preview is reused when it is committed.
const MEMO_LIMIT: usize = 6;

/// What undo and redo put back.
#[derive(Clone)]
struct Snapshot {
    features: Vec<Feature>,
    rollback: Option<usize>,
    ui: Map<String, Value>,
    params: Vec<Param>,
}

pub struct Document {
    pub features: Vec<Feature>,
    /// Evaluate `features[..rollback]` only.
    pub rollback: Option<usize>,
    /// Body names, colours and the hidden list: `{"names": {}, "colors": {}, "hidden": []}`.
    pub ui: Map<String, Value>,
    /// The project's parameters.
    pub params: Vec<Param>,
    pub sources: BTreeMap<String, Source>,
    /// The bodies after the last active feature.
    pub state: State,
    /// Shown when the document came from a mesh file.
    pub title: Option<String>,
    pub dirty: bool,
    /// Top-level keys of a loaded project file that this version doesn't know, written back as they were.
    pub extra: Map<String, Value>,

    registry: Registry,
    next_id: u64,
    /// The parameters as they were when their values were last worked out, and those values.
    pv: Option<(Vec<Param>, Resolved)>,
    /// Per active feature: its hash and the state after it.
    cache: Vec<(String, Arc<State>)>,
    memo: IndexMap<String, (Arc<State>, FeatureResult)>,
    results: Vec<FeatureResult>,
    /// The state `self.state` was copied from, so an unchanged rebuild copies nothing.
    current: Arc<State>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

fn fail<T>(m: impl Into<String>) -> Result<T, FeatureError> {
    Err(FeatureError(m.into()))
}

/// Python's `str.strip()`: it also strips the four ASCII separators Rust doesn't count as space.
fn py_strip(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}

/// Python's `str.capitalize()`.
fn capitalize(s: &str) -> String {
    let mut it = s.chars();
    match it.next() {
        Some(c) => c.to_uppercase().chain(it.flat_map(char::to_lowercase)).collect(),
        None => String::new(),
    }
}

/// Python's `os.path.splitext(name)[1].lower()`.
fn ext_of(name: &str) -> String {
    let base = &name[name.rfind(['/', '\\']).map_or(0, |i| i + 1)..];
    let stem = base.trim_start_matches('.');
    stem.rfind('.').map_or_else(String::new, |i| stem[i..].to_lowercase())
}

/// A step's cache key: a fixed-size digest of the previous step's key and this feature (with the
/// parameter values it uses), so keys don't grow with the length of the timeline.
fn fhash(feature: &Value, prev: &str) -> String {
    let mut h = Sha256::new();
    h.update(prev.as_bytes());
    h.update(b"|");
    h.update(serde_json::to_string(feature).unwrap_or_default().as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn panic_text(p: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = p.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = p.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown".to_string()
    }
}

/// Run an evaluator. A panic in it is a bug, but it must not take the document down: on native
/// targets it becomes that feature's error.
#[cfg(not(target_arch = "wasm32"))]
fn guarded(f: impl FnOnce() -> Result<(), FeatureError>) -> Result<(), FeatureError> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(p) => fail(format!("Internal error: {}", panic_text(p.as_ref()))),
    }
}

/// wasm32 has no unwinding here: a panic aborts the worker (EXCEPTIONS.md; milestone 4).
#[cfg(target_arch = "wasm32")]
fn guarded(f: impl FnOnce() -> Result<(), FeatureError>) -> Result<(), FeatureError> {
    let _ = panic_text; // only used where panics can be caught
    f()
}

/// Times a feature. `std::time::Instant` panics on wasm32-unknown-unknown, so there the time
/// is 0 until the platform layer supplies a clock.
#[cfg(not(target_arch = "wasm32"))]
struct Clock(std::time::Instant);
#[cfg(target_arch = "wasm32")]
struct Clock;

impl Clock {
    #[cfg(not(target_arch = "wasm32"))]
    fn start() -> Clock {
        Clock(std::time::Instant::now())
    }

    #[cfg(target_arch = "wasm32")]
    fn start() -> Clock {
        Clock
    }

    /// Milliseconds since `start`, to one decimal.
    #[cfg(not(target_arch = "wasm32"))]
    fn ms(&self) -> f64 {
        (self.0.elapsed().as_secs_f64() * 10_000.0).round() / 10.0
    }

    #[cfg(target_arch = "wasm32")]
    fn ms(&self) -> f64 {
        0.0
    }
}

impl Drop for Document {
    /// Closing a project: free its solids first so their pooled buffers return, then empty the pool.
    fn drop(&mut self) {
        self.cache = Vec::new();
        self.memo = IndexMap::new();
        self.results = Vec::new();
        self.undo = Vec::new();
        self.redo = Vec::new();
        self.state = State::default();
        self.current = Arc::new(State::default());
        stl_kernel::release_scratch();
    }
}

impl Document {
    pub fn new(registry: Registry) -> Document {
        let ui = match json!({"names": {}, "colors": {}, "hidden": []}) {
            Value::Object(m) => m,
            _ => Map::new(),
        };
        Document {
            features: Vec::new(),
            rollback: None,
            ui,
            params: Vec::new(),
            sources: BTreeMap::new(),
            state: State::default(),
            title: None,
            dirty: false,
            extra: Map::new(),
            registry,
            next_id: 1,
            pv: None,
            cache: Vec::new(),
            memo: IndexMap::new(),
            results: Vec::new(),
            current: Arc::new(State::default()),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    // -- ids & sources -----------------------------------------------------------------------
    /// A fresh id (`f3`, `s4`): features and sources share one counter, and ids aren't reused.
    pub fn new_id(&mut self, prefix: &str) -> String {
        loop {
            let id = format!("{prefix}{}", self.next_id);
            self.next_id += 1;
            if !self.sources.contains_key(&id) && !self.features.iter().any(|f| f.id == id) {
                return id;
            }
        }
    }

    /// After a project is loaded: new ids continue after the highest number already used by a
    /// feature or a source.
    pub fn resume_ids(&mut self) {
        let top = self
            .features
            .iter()
            .map(|f| f.id.as_str())
            .chain(self.sources.keys().map(String::as_str))
            .filter_map(|id| {
                let mut it = id.chars();
                it.next()?;
                let n = it.as_str();
                if n.is_empty() || !n.chars().all(|c| c.is_ascii_digit()) { None } else { n.parse::<u64>().ok() }
            })
            .max()
            .unwrap_or(0);
        self.next_id = top + 1;
    }

    pub fn add_source(&mut self, name: &str, data: Vec<u8>) -> String {
        let sid = self.new_id("s");
        self.sources.insert(sid.clone(), Source { name: name.to_string(), ext: ext_of(name), data });
        sid
    }

    // -- undo --------------------------------------------------------------------------------
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            features: self.features.clone(),
            rollback: self.rollback,
            ui: self.ui.clone(),
            params: self.params.clone(),
        }
    }

    fn checkpoint(&mut self) {
        let snap = self.snapshot();
        self.undo.push(snap);
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
    }

    fn restore(&mut self, snap: Snapshot) {
        self.features = snap.features;
        self.rollback = snap.rollback;
        self.ui = snap.ui;
        self.params = snap.params;
        self.rebuild();
    }

    pub fn undo(&mut self) -> bool {
        let Some(snap) = self.undo.pop() else {
            return false;
        };
        let now = self.snapshot();
        self.redo.push(now);
        self.restore(snap);
        self.dirty = true;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(snap) = self.redo.pop() else {
            return false;
        };
        let now = self.snapshot();
        self.undo.push(now);
        self.restore(snap);
        self.dirty = true;
        true
    }

    // -- parameters --------------------------------------------------------------------------
    /// The values and errors of the project's parameters (worked out once per change).
    pub fn param_values(&mut self) -> &Resolved {
        if !matches!(&self.pv, Some((key, _)) if *key == self.params) {
            self.pv = None;
        }
        let params = &self.params;
        &self.pv.get_or_insert_with(|| (params.clone(), resolve_params(params))).1
    }

    pub fn set_params(&mut self, list: Vec<Param>) -> Result<(), FeatureError> {
        let clean: Vec<Param> = list
            .into_iter()
            .map(|q| Param { name: py_strip(&q.name).to_string(), expr: py_strip(&q.expr).to_string(), note: q.note })
            .filter(|q| !q.name.is_empty() || !q.expr.is_empty())
            .collect();
        let mut seen = BTreeSet::new();
        for q in &clean {
            let n = q.name.as_str();
            if !is_valid_name(n) {
                return fail(format!(
                    "\"{n}\" can't be a parameter name: use letters, digits and _, starting with a letter."
                ));
            }
            if RESERVED.contains(&n) {
                return fail(format!("\"{n}\" is a built-in name; pick another."));
            }
            if q.expr.is_empty() {
                return fail(format!("Give \"{n}\" a value."));
            }
            if !seen.insert(n) {
                return fail(format!("\"{n}\" is there twice."));
            }
        }
        self.checkpoint();
        self.params = clean;
        self.rebuild();
        Ok(())
    }

    /// A feature's hash: the feature, plus the parameter values when it uses expressions (so a
    /// parameter change rebuilds it).
    fn hash(&mut self, f: &Feature, prev: &str) -> String {
        let mut v = serde_json::to_value(f).unwrap_or(Value::Null);
        if f.params.values().any(uses_expr) {
            if let Value::Object(o) = &mut v {
                o.insert("$params".to_string(), json!(self.param_values().values));
            }
        }
        fhash(&v, prev)
    }

    // -- evaluation --------------------------------------------------------------------------
    /// How many features are active (those before the rollback bar).
    fn active_len(&self) -> usize {
        self.rollback.map_or(self.features.len(), |r| r.min(self.features.len()))
    }

    /// Where a new feature goes: at the rollback bar, or at the end.
    fn insert_at(&self) -> usize {
        self.active_len()
    }

    /// Rebuild the active features, reusing every step whose hash didn't change.
    pub fn evaluate(&mut self) -> Vec<FeatureResult> {
        self.rebuild();
        self.results.clone()
    }

    /// The results of the last rebuild, one per active feature.
    pub fn results(&self) -> &[FeatureResult] {
        &self.results
    }

    fn rebuild(&mut self) {
        // the features are moved out while they run, so running one can borrow the document
        let feats = std::mem::take(&mut self.features);
        let end = self.rollback.map_or(feats.len(), |r| r.min(feats.len()));
        let old_cache = std::mem::take(&mut self.cache);
        let old_results = std::mem::take(&mut self.results);
        let mut state = Arc::new(State::default());
        let mut results = Vec::with_capacity(end);
        let mut cache = Vec::with_capacity(end);
        let mut h = String::new();
        let mut ran = false;
        for (i, f) in feats[..end].iter().enumerate() {
            h = self.hash(f, &h);
            if i < old_cache.len() && old_cache[i].0 == h && i < old_results.len() {
                state = old_cache[i].1.clone();
                results.push(old_results[i].clone());
                cache.push((h.clone(), state.clone()));
                continue;
            }
            let res = match self.memo.get(&h) {
                Some((st, res)) => {
                    state = st.clone();
                    res.clone()
                }
                None => {
                    ran = true;
                    let (st, res) = self.run_feature(&state, f, false);
                    if let Some(st) = st {
                        state = Arc::new(st);
                    }
                    res
                }
            };
            results.push(res);
            cache.push((h.clone(), state.clone()));
        }
        self.features = feats;
        self.cache = cache;
        self.results = results;
        if !Arc::ptr_eq(&self.current, &state) {
            self.state = (*state).clone();
            self.current = state;
        }
        // drop unused cached states first so their pooled buffers can return, then empty the pool (no-op if another operation is running)
        drop(old_cache);
        drop(old_results);
        // A rebuild that only reused cached or just-computed results did no kernel work, so it leaves the pool
        // to the idle timer; one that evaluated features empties it now.
        if ran {
            stl_kernel::release_scratch();
        } else {
            // the dropped cache may have returned buffers to the pool: make sure a timer is armed
            stl_kernel::touch_scratch();
        }
    }

    /// Keep a just-computed feature result so the next rebuild can reuse it.
    fn remember(&mut self, at: usize, f: &Feature, state: Arc<State>, res: FeatureResult) {
        if at > self.cache.len() {
            self.rebuild();
        }
        let prev = match at.checked_sub(1).and_then(|i| self.cache.get(i)) {
            Some((h, _)) => h.clone(),
            None => String::new(),
        };
        let h = self.hash(f, &prev);
        self.memo.insert(h, (state, res));
        while self.memo.len() > MEMO_LIMIT {
            self.memo.shift_remove_index(0);
        }
    }

    /// Run one feature on `state`. Gives the new state (`None` when the feature is suppressed
    /// or failed, so the state before it stands) and the result.
    fn run_feature(&mut self, state: &State, f: &Feature, preview: bool) -> (Option<State>, FeatureResult) {
        // the kernel keeps big buffers for reuse; this marks an operation so the idle release never runs mid-way
        let _op = stl_kernel::ScratchGuard::begin();
        let mut res = FeatureResult { id: f.id.clone(), ..Default::default() };
        if f.suppressed {
            res.suppressed = true;
            return (None, res);
        }
        let clock = Clock::start();
        let values = match f.params.values().any(uses_expr) {
            true => Some(self.param_values().values.clone()),
            false => None,
        };
        let mut new = state.clone();
        let ctx = Ctx { sources: &self.sources, preview };
        let registry = &self.registry;
        let out = (|| {
            let resolved: Map<String, Value>;
            let params = match &values {
                Some(values) => {
                    resolved = f
                        .params
                        .iter()
                        .map(|(k, v)| Ok((k.clone(), resolve_deep(v, values)?)))
                        .collect::<Result<_, crate::params::ExprError>>()
                        .map_err(|e| FeatureError(format!("Parameter: {e}.")))?;
                    &resolved
                }
                None => &f.params,
            };
            let Some(run) = registry.get(&f.kind) else {
                return fail(format!("Unknown feature {}.", f.kind));
            };
            guarded(|| run(&mut new, &f.id, params, &mut res, &ctx))
        })();
        let new = match out {
            Ok(()) => Some(new),
            Err(e) => {
                res.error = Some(e.0);
                None
            }
        };
        res.ms = clock.ms();
        (new, res)
    }

    // -- editing -----------------------------------------------------------------------------
    /// Add a feature at the rollback bar (or the end). It is tried first: a feature that fails
    /// outright is refused with its message. Gives the new feature's id.
    pub fn add_feature(
        &mut self,
        kind: &str,
        params: Map<String, Value>,
        name: Option<String>,
    ) -> Result<String, FeatureError> {
        if !self.registry.has(kind) {
            return fail(format!("Unknown feature {kind}."));
        }
        let id = self.new_id("f");
        let name = name.filter(|n| !n.is_empty()).unwrap_or_else(|| self.auto_name(kind, &params));
        let f = Feature {
            id: id.clone(),
            kind: kind.to_string(),
            name: Some(name),
            params,
            suppressed: false,
            extra: Map::new(),
        };
        let at = self.insert_at();
        let base = self.state_before(at);
        let (new_state, res) = self.run_feature(&base, &f, false);
        if let Some(e) = res.error {
            return Err(FeatureError(e));
        }
        self.remember(at, &f, new_state.map_or(base, Arc::new), res);
        self.checkpoint();
        if kind == "extrude" || kind == "revolve" {
            // a sketch used by a feature is tucked away, as in other CAD tools
            if let Some(Value::String(sketch)) = f.params.get("sketch") {
                if !sketch.is_empty() {
                    let mut hid = self.hidden();
                    hid.insert(sketch.clone());
                    self.set_hidden(hid);
                }
            }
        }
        self.features.insert(at, f);
        if let Some(r) = self.rollback.as_mut() {
            *r += 1;
        }
        self.rebuild();
        Ok(id)
    }

    /// Evaluate a feature without committing it: a new one (`edit_id` is `None`), or the
    /// feature `edit_id` with these params. Gives the state it would leave, and the result.
    pub fn preview(
        &mut self,
        kind: &str,
        params: Map<String, Value>,
        edit_id: Option<&str>,
    ) -> (State, FeatureResult) {
        let take = |a: Arc<State>| Arc::try_unwrap(a).unwrap_or_else(|a| (*a).clone());
        if let Some(fid) = edit_id.filter(|s| !s.is_empty()) {
            let idx = match self.index(fid) {
                Ok(i) => i,
                Err(e) => {
                    let res = FeatureResult { id: fid.to_string(), error: Some(e.0), ..Default::default() };
                    return (self.state.clone(), res);
                }
            };
            let base = self.state_before(idx);
            let f = Feature { params, ..self.features[idx].clone() };
            let (st, res) = self.run_feature(&base, &f, true);
            let Some(st) = st else {
                return (take(base), res);
            };
            // a preview-only result (Support Fins shown beside the part) isn't the real one
            if res.error.is_none() && !res.preview_only {
                let st = Arc::new(st);
                self.remember(idx, &f, st.clone(), res.clone());
                return (take(st), res);
            }
            return (st, res);
        }
        let at = self.insert_at();
        let base = self.state_before(at);
        let f = Feature {
            id: "preview".to_string(),
            kind: kind.to_string(),
            name: Some("preview".to_string()),
            params,
            suppressed: false,
            extra: Map::new(),
        };
        let (st, res) = self.run_feature(&base, &f, true);
        (st.unwrap_or_else(|| take(base)), res)
    }

    /// Change a feature's params, name or suppressed flag. New params are tried first and
    /// refused with their message when they fail.
    pub fn edit_feature(
        &mut self,
        fid: &str,
        params: Option<Map<String, Value>>,
        name: Option<String>,
        suppressed: Option<bool>,
        rollback_after: RollbackAfter,
    ) -> Result<(), FeatureError> {
        let idx = self.index(fid)?;
        let mut f = self.features[idx].clone();
        let retry = params.is_some();
        if let Some(p) = params {
            f.params = p;
        }
        if let Some(n) = name {
            f.name = Some(n);
        }
        if let Some(s) = suppressed {
            f.suppressed = s;
            // keep one "suppressed" key: written when set, and kept as false once it was stored
            f.extra.remove("suppressed");
            if !s && self.features[idx].extra.contains_key("suppressed") {
                f.extra.insert("suppressed".into(), Value::Bool(false));
            }
        }
        if retry {
            let base = self.state_before(idx);
            let (new_state, res) = self.run_feature(&base, &f, false);
            if let Some(e) = res.error {
                return Err(FeatureError(e));
            }
            self.remember(idx, &f, new_state.map_or(base, Arc::new), res);
        }
        if let RollbackAfter::Set(r) = rollback_after {
            // an edit session rolls the timeline back temporarily; undo should land on the
            // timeline as it was before that session, not on the rolled-back view
            self.rollback = r;
        }
        self.checkpoint();
        self.features[idx] = f;
        self.rebuild();
        Ok(())
    }

    pub fn delete_feature(&mut self, fid: &str) -> Result<(), FeatureError> {
        let idx = self.index(fid)?;
        self.checkpoint();
        self.features.remove(idx);
        if let Some(r) = self.rollback.as_mut() {
            if idx < *r {
                *r -= 1;
            }
        }
        self.rebuild();
        Ok(())
    }

    pub fn move_feature(&mut self, fid: &str, new_index: usize) -> Result<(), FeatureError> {
        let idx = self.index(fid)?;
        self.checkpoint();
        let f = self.features.remove(idx);
        self.features.insert(new_index.min(self.features.len()), f);
        self.rebuild();
        Ok(())
    }

    /// Roll the timeline back to before feature `index`; `None`, or an index at or past the
    /// end, shows everything.
    pub fn set_rollback(&mut self, index: Option<usize>) {
        self.rollback = index.filter(|&i| i < self.features.len());
        self.rebuild();
    }

    /// The position of a feature in the timeline.
    pub fn index(&self, fid: &str) -> Result<usize, FeatureError> {
        match self.features.iter().position(|f| f.id == fid) {
            Some(i) => Ok(i),
            None => fail("That feature no longer exists."),
        }
    }

    fn state_before(&mut self, idx: usize) -> Arc<State> {
        if idx == 0 {
            return Arc::new(State::default());
        }
        self.rebuild();
        if let Some((_, st)) = self.cache.get(idx - 1) {
            return st.clone();
        }
        // rolled back before idx: evaluate up to idx on the side
        let feats = std::mem::take(&mut self.features);
        let mut state = State::default();
        for f in &feats[..idx.min(feats.len())] {
            if let (Some(st), _) = self.run_feature(&state, f, false) {
                state = st;
            }
        }
        self.features = feats;
        Arc::new(state)
    }

    fn auto_name(&self, kind: &str, params: &Map<String, Value>) -> String {
        if kind == "primitive" {
            let shape = params.get("shape").and_then(Value::as_str).unwrap_or("box");
            let same = |f: &&Feature| f.kind == kind && f.params.get("shape").and_then(Value::as_str) == Some(shape);
            let n = self.features.iter().filter(same).count() + 1;
            return format!("{}{n}", capitalize(shape));
        }
        let n = self.features.iter().filter(|f| f.kind == kind).count() + 1;
        format!("{}{n}", label(kind))
    }

    // -- bodies (UI-level properties) --------------------------------------------------------
    /// The name the user gave a body, or the one its feature gave it ("" when there is no
    /// such body).
    pub fn body_name(&self, bid: &str) -> String {
        let given = self.ui.get("names").and_then(|n| n.get(bid)).and_then(Value::as_str);
        match given.filter(|s| !s.is_empty()) {
            Some(s) => s.to_string(),
            None => self.state.bodies.get(bid).map(|b| b.name.clone()).unwrap_or_default(),
        }
    }

    pub fn set_body_props(&mut self, bid: &str, name: Option<String>, color: Option<String>, hidden: Option<bool>) {
        self.checkpoint();
        if let Some(n) = name {
            self.ui_map("names").insert(bid.to_string(), Value::String(n));
        }
        if let Some(c) = color {
            self.ui_map("colors").insert(bid.to_string(), Value::String(c));
        }
        if let Some(h) = hidden {
            let mut hid = self.hidden();
            if h {
                hid.insert(bid.to_string());
            } else {
                hid.remove(bid);
            }
            self.set_hidden(hid);
        }
    }

    /// `ui[key]` as an object (made, or replaced, when it isn't one).
    fn ui_map(&mut self, key: &str) -> &mut Map<String, Value> {
        let v = self.ui.entry(key.to_string()).or_insert_with(|| Value::Object(Map::new()));
        if !v.is_object() {
            *v = Value::Object(Map::new());
        }
        match v {
            Value::Object(m) => m,
            _ => unreachable!("just made an object"),
        }
    }

    fn hidden(&self) -> BTreeSet<String> {
        let list = self.ui.get("hidden").and_then(Value::as_array);
        list.into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)).collect()
    }

    /// The hidden list is kept sorted, as 1.x does.
    fn set_hidden(&mut self, hid: BTreeSet<String>) {
        self.ui.insert("hidden".to_string(), Value::Array(hid.into_iter().map(Value::String).collect()));
    }
}
