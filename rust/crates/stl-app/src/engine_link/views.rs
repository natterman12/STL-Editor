//! Builds the views the UI draws from a `Document`.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use stl_engine::labels::label;
use stl_engine::Document;
use stl_kernel::{Frame, MeshArrays, Solid};
use stl_sketch::contours::entity_path;
use stl_sketch::Entity;

use super::{BodyView, ParamView, PlaneView, SketchOutline, SketchView, TimelineItem, TimelineView};

/// What a body looked like last time, so an unchanged body keeps its mesh and its revision.
struct Cached {
    solid: Option<Solid>,
    raw: Option<Arc<MeshArrays>>,
    mesh: Arc<MeshArrays>,
    revision: u64,
}

/// Remembers the bodies it has shown. A body's `revision` changes only when its mesh does, so
/// the viewport uploads only the bodies that changed.
#[derive(Default)]
pub struct Views {
    next_revision: u64,
    cache: HashMap<String, Cached>,
    sketch_cache: HashMap<String, CachedSketch>,
}

/// A sketch's curves for the entities and plane they were made from.
struct CachedSketch {
    frame: Frame,
    entities: Vec<Entity>,
    outlines: Arc<[SketchOutline]>,
}

fn same_mesh(a: &MeshArrays, b: &MeshArrays) -> bool {
    a.triangles == b.triangles && a.positions == b.positions
}

fn hidden_ids(doc: &Document) -> Vec<String> {
    let list = doc.ui.get("hidden").and_then(Value::as_array);
    list.into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)).collect()
}

impl Views {
    pub fn new() -> Views {
        Views::default()
    }

    /// Forget every body (a new project): the next revisions are all fresh.
    pub fn clear(&mut self) {
        self.cache.clear();
        self.sketch_cache.clear();
    }

    /// Every sketch with its curves; a sketch that did not change keeps its `Arc`.
    pub fn sketches(&mut self, doc: &Document) -> Vec<SketchView> {
        let hidden = hidden_ids(doc);
        let mut old = std::mem::take(&mut self.sketch_cache);
        let mut out = Vec::new();
        for (id, s) in &doc.state.sketches {
            let outlines = match old.remove(id) {
                Some(c) if c.frame == s.frame && c.entities == s.entities => {
                    let o = c.outlines.clone();
                    self.sketch_cache.insert(id.clone(), c);
                    o
                }
                _ => {
                    let o: Arc<[SketchOutline]> = s.entities.iter().filter_map(outline).collect();
                    let c = CachedSketch { frame: s.frame, entities: s.entities.clone(), outlines: o.clone() };
                    self.sketch_cache.insert(id.clone(), c);
                    o
                }
            };
            let plane = doc
                .features
                .iter()
                .find(|f| f.id == *id)
                .and_then(|f| f.params.get("plane"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            out.push(SketchView {
                id: id.clone(),
                name: if s.name.is_empty() { feature_name(doc, id) } else { s.name.clone() },
                plane,
                hidden: hidden.contains(id),
                frame: [s.frame.o, s.frame.x, s.frame.y, s.frame.n],
                outlines,
            });
        }
        out
    }

    pub fn bodies(&mut self, doc: &Document) -> Vec<BodyView> {
        let hidden = hidden_ids(doc);
        let colors = doc.ui.get("colors").and_then(Value::as_object);
        let mut out = Vec::with_capacity(doc.state.bodies.len());
        let mut fresh: HashMap<String, Cached> = HashMap::new();
        for (id, body) in &doc.state.bodies {
            let old = self.cache.remove(id);
            let entry = self.entry(old, body.solid.as_ref(), body.mesh.as_ref());
            out.push(BodyView {
                id: id.clone(),
                name: doc.body_name(id),
                color: colors.and_then(|c| c.get(id)).and_then(Value::as_str).map(str::to_string),
                hidden: hidden.contains(id),
                solid: body.solid.is_some(),
                mesh: entry.mesh.clone(),
                revision: entry.revision,
            });
            fresh.insert(id.clone(), entry);
        }
        self.cache = fresh;
        out
    }

    fn entry(&mut self, old: Option<Cached>, solid: Option<&Solid>, raw: Option<&Arc<MeshArrays>>) -> Cached {
        if let Some(o) = old {
            let same_source = match (solid, &o.solid, raw, &o.raw) {
                (Some(s), Some(p), _, _) => s.shares_with(p),
                (None, None, Some(r), Some(p)) => Arc::ptr_eq(r, p),
                _ => false,
            };
            if same_source {
                return o;
            }
            let mesh = Self::mesh_of(solid, raw);
            // a different object can still hold the same mesh (a parameter changed another body)
            if same_mesh(&mesh, &o.mesh) {
                return Cached { solid: solid.cloned(), raw: raw.cloned(), mesh: o.mesh, revision: o.revision };
            }
            return self.fresh(solid, raw, mesh);
        }
        let mesh = Self::mesh_of(solid, raw);
        self.fresh(solid, raw, mesh)
    }

    fn fresh(&mut self, solid: Option<&Solid>, raw: Option<&Arc<MeshArrays>>, mesh: MeshArrays) -> Cached {
        self.next_revision += 1;
        Cached { solid: solid.cloned(), raw: raw.cloned(), mesh: Arc::new(mesh), revision: self.next_revision }
    }

    fn mesh_of(solid: Option<&Solid>, raw: Option<&Arc<MeshArrays>>) -> MeshArrays {
        match (solid, raw) {
            (Some(s), _) => s.to_mesh(),
            (None, Some(m)) => (**m).clone(),
            (None, None) => MeshArrays { positions: Vec::new(), triangles: Vec::new() },
        }
    }
}

pub fn timeline(doc: &Document) -> TimelineView {
    let results = doc.results();
    let active_count = doc.rollback.map_or(doc.features.len(), |r| r.min(doc.features.len()));
    let items = doc
        .features
        .iter()
        .enumerate()
        .map(|(i, f)| TimelineItem {
            id: f.id.clone(),
            kind: f.kind.clone(),
            label: f.name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| label(&f.kind).to_string()),
            error: results.get(i).and_then(|r| r.error.clone()),
            suppressed: f.suppressed,
            warning: results.get(i).and_then(|r| r.warning.clone()),
            active: i < active_count,
            params: serde_json::Value::Object(f.params.clone()),
        })
        .collect();
    TimelineView {
        items,
        marker: doc.rollback.filter(|&r| r < doc.features.len()),
        can_undo: doc.can_undo(),
        can_redo: doc.can_redo(),
    }
}

/// Every name written in a `"=..."` string anywhere in the steps' settings (app.js:1155).
fn names_used(doc: &Document) -> std::collections::HashSet<String> {
    fn walk(v: &Value, out: &mut std::collections::HashSet<String>) {
        match v {
            Value::String(s) => {
                if let Some(e) = s.strip_prefix('=') {
                    let mut cur = String::new();
                    for c in e.chars().chain(std::iter::once(' ')) {
                        let ok = c == '_' || c.is_ascii_alphabetic() || (!cur.is_empty() && c.is_ascii_digit());
                        if ok {
                            cur.push(c);
                        } else if !cur.is_empty() {
                            out.insert(std::mem::take(&mut cur));
                        }
                    }
                }
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            Value::Object(o) => o.values().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    let mut out = std::collections::HashSet::new();
    for f in &doc.features {
        for v in f.params.values() {
            walk(v, &mut out);
        }
    }
    out
}

pub fn params(doc: &mut Document) -> Vec<ParamView> {
    let used = names_used(doc);
    let list = doc.params.clone();
    let r = doc.param_values();
    list.into_iter()
        .map(|p| ParamView {
            value: r.values.get(&p.name).copied(),
            error: r.errors.get(&p.name).cloned(),
            used: used.contains(&p.name),
            name: p.name,
            expr: p.expr,
            note: p.note,
        })
        .collect()
}

fn feature_name(doc: &Document, id: &str) -> String {
    doc.features
        .iter()
        .find(|f| f.id == id)
        .map(|f| f.name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| label(&f.kind).to_string()))
        .unwrap_or_else(|| id.to_string())
}

/// The planes the user made. The three base planes are not sent: the viewport draws them
/// itself, only while the Origin toggle is on (1.x `setBasePlanesVisible`). A hidden plane
/// has `visible: false` (1.x `syncPlanes` skips `p.hidden`).
pub fn planes(doc: &Document) -> Vec<PlaneView> {
    let mut out = Vec::new();
    let hidden = hidden_ids(doc);
    for (id, p) in &doc.state.planes {
        let f = p.frame;
        out.push(PlaneView {
            id: id.clone(),
            name: feature_name(doc, id),
            origin: f.o,
            x: f.x,
            y: f.y,
            normal: f.n,
            visible: !hidden.contains(id),
        });
    }
    out
}

/// One entity as a polyline in plane coordinates (1.x `entityOutline`).
fn outline(e: &Entity) -> Option<SketchOutline> {
    let (pts, closed) = entity_path(e);
    (pts.len() >= 2).then(|| SketchOutline { pts, closed, construction: e.construction() })
}

/// What a preview shows (1.x `packMeshes` in the `preview` route): the bodies the feature made or
/// changed, and the ones it removed. `state` is what `Document::preview` returned.
pub fn preview_pack(state: &stl_engine::State, res: &stl_engine::FeatureResult) -> super::PreviewPack {
    let mut ids: Vec<&String> = Vec::new();
    for id in res.created.iter().chain(res.modified.iter()) {
        if !ids.contains(&id) && state.bodies.contains_key(id) {
            ids.push(id);
        }
    }
    let meshes = ids
        .into_iter()
        .map(|id| {
            let b = &state.bodies[id];
            crate::viewport::PreviewMesh { id: id.clone(), mesh: Arc::new(Views::mesh_of(b.solid.as_ref(), b.mesh.as_ref())) }
        })
        .collect();
    let colors = res
        .extra
        .get("colors")
        .and_then(Value::as_object)
        .map(|o| o.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect())
        .unwrap_or_default();
    let ghosts = res.extra.get("ghosts").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
    super::PreviewPack { meshes, removed: res.removed.clone(), hide: Vec::new(), colors, ghosts }
}

/// 1.x's `pack.result`: the feature's record under its 1.x key names.
pub fn result_json(res: &stl_engine::FeatureResult) -> Value {
    let mut m = serde_json::Map::new();
    m.insert("id".into(), Value::String(res.id.clone()));
    m.insert("error".into(), res.error.clone().map_or(Value::Null, Value::String));
    m.insert("warning".into(), res.warning.clone().map_or(Value::Null, Value::String));
    m.insert("info".into(), res.info.clone().map_or(Value::Null, Value::String));
    m.insert("flipped".into(), Value::Bool(res.flipped));
    m.insert("created".into(), Value::from(res.created.clone()));
    m.insert("modified".into(), Value::from(res.modified.clone()));
    m.insert("removed".into(), Value::from(res.removed.clone()));
    for (k, v) in &res.extra {
        m.insert(k.clone(), v.clone());
    }
    Value::Object(m)
}

/// The note a preview shows under the form, and whether it is a warning (1.x `runPreview`).
pub fn preview_msg(res: &stl_engine::FeatureResult) -> (Option<String>, bool) {
    if let Some(w) = res.warning.clone().filter(|w| !w.is_empty()) {
        return (Some(w), true);
    }
    if res.flipped {
        return (Some("Cutting into the material (direction flipped).".to_string()), false);
    }
    (res.info.clone().filter(|i| !i.is_empty()), false)
}
