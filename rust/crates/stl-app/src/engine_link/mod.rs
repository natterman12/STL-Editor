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
pub use crate::viewport::PreviewPack;

#[derive(Clone, Debug)]
pub enum Command {
    Open { name: String, bytes: Vec<u8> },
    Import { name: String, bytes: Vec<u8> },
    /// `index`: put the step before the one now at that place (1.x `insertBeforeSupports`); the
    /// rollback marker moves one step on. `name`: the step's name (default: made from the kind).
    AddFeature { kind: String, params: serde_json::Value, index: Option<usize>, name: Option<String> },
    /// `rollback_after`: `Some(marker)` puts the rollback marker there as part of the same edit, so
    /// one Ctrl+Z undoes the whole OK of an edit session (1.x `feature/edit` with `rollback_after`);
    /// `None` leaves the marker alone.
    EditFeature { id: String, params: serde_json::Value, rollback_after: Option<Option<usize>> },
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
    /// Evaluate a tool's feature without committing it (1.x `post('preview', ...)`). Never enters
    /// the undo history and never changes the document. `seq` grows with every request: only the
    /// newest answer is shown.
    Preview { seq: u64, kind: String, params: serde_json::Value, edit_id: Option<String> },
    /// The tool closed: previews still queued are skipped (native), and any that arrive are dropped.
    ClearPreview,
    /// A question for the Support Fins engine (its schema, the strength of a pose, orientation
    /// suggestions). Never changes the document and shows no Busy overlay; the answer is
    /// [`Update::Fins`] with the same `seq`.
    Fins { seq: u64, ask: FinsAsk },
    /// 1.x `post('sketch_op', {op, ...data})`: trim, offset, mirror, explode, project, refs, text, faces.
    SketchOp { seq: u64, op: String, data: serde_json::Value },
    /// Solve a sketch off the UI thread (`stl_sketch::solver::solve`); only the newest `seq` counts.
    Solve {
        seq: u64,
        entities: Vec<stl_sketch::entities::Entity>,
        constraints: Vec<stl_sketch::solver::Constraint>,
        locks: Vec<stl_sketch::solver::Lock>,
        fixed: Vec<stl_sketch::entities::Entity>,
    },
    Cancel,
    /// The worker writes an autosave if the document changed since the last one. `path` is where
    /// the open project lives on disk (desktop; `None` for a new project and on the web) and
    /// `title` its name; both go into the copy so Recover Unsaved Work can say what it was.
    Autosave { path: Option<String>, title: Option<String> },
}

/// What the Support Fins panel asks the engine (1.x `supportfins/info`, `/strength`, `/suggest`).
#[derive(Clone, Debug, PartialEq)]
pub enum FinsAsk {
    Info,
    Strength { body: String, options: serde_json::Value, load: serde_json::Value },
    Suggest { body: String, options: serde_json::Value, top: i64 },
}

impl FinsAsk {
    pub fn name(&self) -> &'static str {
        match self {
            FinsAsk::Info => "info",
            FinsAsk::Strength { .. } => "strength",
            FinsAsk::Suggest { .. } => "suggest",
        }
    }
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
    /// The answer to `Command::Preview`. `error` is the feature's error text; `msg` is the note to
    /// show (1.x: the warning, else "Cutting into the material (direction flipped)." when flipped,
    /// else the info) and `warn` says which colour it takes. `result` is 1.x's whole `pack.result`
    /// record (Support Fins reads `paint`, `walls`, `pieces`, `support` from it).
    Preview { seq: u64, pack: PreviewPack, msg: Option<String>, error: Option<String>, warn: bool, result: serde_json::Value },
    /// The answer to [`Command::Fins`]: `ask` is `"info"`, `"strength"` or `"suggest"`.
    Fins { seq: u64, ask: String, result: Result<serde_json::Value, String> },
    /// The answer to [`Command::SketchOp`]: the route's JSON, or its error text.
    SketchOp { seq: u64, result: Result<serde_json::Value, String> },
    /// The answer to [`Command::Solve`] (`SolveReport`'s fields).
    Solved { seq: u64, entities: Vec<stl_sketch::entities::Entity>, ok: bool, error: f64 },
    /// The answer to [`Command::Autosave`] (only when something was written or failed).
    Autosaved { ok: bool, error: Option<String> },
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
    /// The step's params (1.x `f.params`).
    pub params: serde_json::Value,
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
