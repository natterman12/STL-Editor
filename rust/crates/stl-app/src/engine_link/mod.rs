//! The engine (a `Document`) on a worker thread or Web Worker. The UI only sends `Command`s and
//! draws the `Update`s it gets back.

#[cfg(not(target_arch = "wasm32"))]
pub mod native;
pub mod views;
#[cfg(target_arch = "wasm32")]
pub mod web;
pub mod worker;

use std::sync::Arc;
use stl_engine::params::Param;
pub use stl_io::ExportFormat;
use stl_kernel::MeshArrays;

pub use crate::viewport::overlays::SketchOutline;

#[derive(Clone, Debug)]
pub enum Command {
    Open { name: String, bytes: Vec<u8> },
    Import { name: String, bytes: Vec<u8> },
    AddFeature { kind: String, params: serde_json::Value, index: Option<usize> },
    EditFeature { id: String, params: serde_json::Value },
    DeleteFeature { id: String },
    /// Switch a step off (or on again): it keeps its place but does nothing. Undoable.
    SetSuppressed { id: String, suppressed: bool },
    MoveFeature { id: String, index: usize },
    SetParams(Vec<Param>),
    Rollback(Option<usize>),
    Undo,
    Redo,
    BodyProps { id: String, name: Option<String>, color: Option<String>, hidden: Option<bool> },
    Save,
    Export { format: ExportFormat, ids: Option<Vec<String>>, date: String },
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Debug)]
pub enum Update {
    Bodies(Vec<BodyView>),
    Timeline(TimelineView),
    Params(Vec<ParamView>),
    Planes(Vec<PlaneView>),
    Sketches(Vec<SketchView>),
    Message { level: Level, text: String },
    Progress { done: usize, total: usize, label: String },
    Busy(bool),
    Saved { bytes: Vec<u8> },
    Exported { name: String, bytes: Vec<u8>, count: usize },
    Error(String),
    EngineLost,
}

/// `revision` changes only when the mesh does, so the viewport uploads only changed bodies.
#[derive(Clone, Debug)]
pub struct BodyView {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub hidden: bool,
    /// Watertight solid; `false` is a raw mesh that needs Repair (the "needs repair" tag).
    pub solid: bool,
    pub mesh: Arc<MeshArrays>,
    pub revision: u64,
}

#[derive(Clone, Debug, Default)]
pub struct TimelineItem {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub error: Option<String>,
    /// The step is switched off (it keeps its place but does nothing).
    pub suppressed: bool,
    /// A note that is not an error (1.x `f.warning`).
    pub warning: Option<String>,
    /// Before the rollback marker, so it counts in the model (1.x `f.active`).
    pub active: bool,
}

#[derive(Clone, Debug, Default)]
pub struct TimelineView {
    pub items: Vec<TimelineItem>,
    /// Number of features before the rollback marker (`None`: at the end).
    pub marker: Option<usize>,
    pub can_undo: bool,
    pub can_redo: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ParamView {
    pub name: String,
    pub expr: String,
    pub note: String,
    pub value: Option<f64>,
    pub error: Option<String>,
    /// A step in the timeline uses this parameter in a `=...` field (1.x `used`, app.js:1155).
    pub used: bool,
}

#[derive(Clone, Debug, Default)]
pub struct PlaneView {
    pub id: String,
    pub name: String,
    pub origin: [f64; 3],
    pub x: [f64; 3],
    pub y: [f64; 3],
    pub normal: [f64; 3],
    pub visible: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SketchView {
    pub id: String,
    pub name: String,
    pub plane: String,
    pub hidden: bool,
    /// The sketch plane: origin, x, y and normal (world space, mm).
    pub frame: [[f64; 3]; 4],
    /// The sketch's curves in plane coordinates, construction ones flagged. Shared, and the same
    /// `Arc` as long as the sketch does not change.
    pub outlines: Arc<[SketchOutline]>,
}

pub trait EngineLink {
    fn send(&self, c: Command);
    fn poll(&self) -> Vec<Update>;
    fn restart(&self);
}

/// Does nothing; for tests and the empty window.
pub struct NullLink;

impl EngineLink for NullLink {
    fn send(&self, _c: Command) {}
    fn poll(&self) -> Vec<Update> {
        Vec::new()
    }
    fn restart(&self) {}
}
