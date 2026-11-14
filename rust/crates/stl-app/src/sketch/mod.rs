//! Sketch mode (milestone 4c): the session, the tool interface and the entity geometry.
//! The fixed interfaces of Task 1; the other modules fill in behind them.

pub mod dims;
pub mod draw;
pub mod geom;
pub mod import;
pub mod keys;
pub mod modify;
pub mod panel;
pub mod project;
pub mod relations;
pub mod session;
pub mod snap;
pub mod solve;
pub mod text;
pub mod touch;
pub mod typed;
pub mod view;

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use crate::viewport::PlaneFrame;

pub use stl_sketch::entities::Entity;
pub use stl_sketch::solver::{Constraint, Ref};

/// A part edge to measure from (`sketch_op` `refs`); red when the part no longer has it.
#[derive(Clone, Debug, PartialEq)]
pub struct RefEnt {
    pub ent: Entity,
    pub missing: bool,
}

/// An open sketch (1.x `Sketcher` state).
#[derive(Clone, Debug)]
pub struct SketchSession {
    pub frame: PlaneFrame,
    /// The step's `plane`, as 1.x.
    pub plane_ref: Value,
    pub feature_id: Option<String>,
    pub name: Option<String>,
    pub entities: Vec<Entity>,
    pub constraints: Vec<Constraint>,
    pub refs: Vec<RefEnt>,
    pub ref_kind: Option<String>,
    pub sel: BTreeSet<String>,
    pub sel_c: Option<usize>,
    /// 1.x tool keys: `"select"`, `"line"`, `"arc3"`, `"c_dimension"`, ...
    pub tool: String,
    pub history: Vec<(Vec<Entity>, Vec<Constraint>)>,
    /// Base-36 time, as `Date.now().toString(36)`.
    pub seed: String,
    /// Drag and typed-value locks for the next solve (sketcher.js `solveAll(locks)`).
    pub locks: Vec<stl_sketch::solver::Lock>,
    /// While a drag is on: the shapes as they were when it began. Every drag step solves from
    /// this copy, as 1.x's `dragMove` solves from `d.base`, so where a loose shape ends up
    /// doesn't depend on the pointer's path.
    pub solve_from: Option<Vec<Entity>>,
    /// The next solve follows a typed-field edit: if the relations can't be kept, say so
    /// (`updateEntity`'s "Some constraints could not be kept.").
    pub warn_unkept: bool,
    next_id: u32,
}

/// `history.length > 100` shifts the oldest (sketcher.js `push`).
const HISTORY_MAX: usize = 100;

fn base36(mut n: u64) -> String {
    if n == 0 {
        return "0".into();
    }
    let mut out = Vec::new();
    while n > 0 {
        out.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[(n % 36) as usize]);
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

impl SketchSession {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        frame: PlaneFrame,
        plane_ref: Value,
        feature_id: Option<String>,
        name: Option<String>,
        entities: Vec<Entity>,
        constraints: Vec<Constraint>,
        refs: Vec<RefEnt>,
        ref_kind: Option<String>,
    ) -> Self {
        let ms = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        SketchSession {
            frame,
            plane_ref,
            feature_id,
            name,
            entities,
            constraints,
            refs,
            ref_kind,
            sel: BTreeSet::new(),
            sel_c: None,
            tool: "select".into(),
            history: Vec::new(),
            seed: base36(ms),
            locks: Vec::new(),
            solve_from: None,
            warn_unkept: false,
            next_id: 1,
        }
    }

    /// `"e<seed><n>"`, skipping ids in use (routes.js `newId`).
    pub fn new_id(&mut self) -> String {
        loop {
            let id = format!("e{}{}", self.seed, self.next_id);
            self.next_id += 1;
            if !self.entities.iter().any(|e| e.id() == Some(id.as_str())) {
                return id;
            }
        }
    }

    /// History before an edit (`push`).
    pub fn push(&mut self) {
        self.history.push((self.entities.clone(), self.constraints.clone()));
        if self.history.len() > HISTORY_MAX {
            self.history.remove(0);
        }
    }

    /// `undoLocal`: false when there is nothing to undo.
    pub fn undo_local(&mut self) -> bool {
        let Some((e, c)) = self.history.pop() else { return false };
        self.entities = e;
        self.constraints = c;
        self.sel.clear();
        self.sel_c = None;
        true
    }

    pub fn to_uv(&self, p: [f64; 3]) -> [f64; 2] {
        let f = &self.frame;
        let d = [p[0] - f.origin[0], p[1] - f.origin[1], p[2] - f.origin[2]];
        let dot = |a: [f64; 3]| d[0] * a[0] + d[1] * a[1] + d[2] * a[2];
        [dot(f.x), dot(f.y)]
    }

    pub fn to_3(&self, uv: [f64; 2]) -> [f64; 3] {
        let f = &self.frame;
        [0, 1, 2].map(|i| f.origin[i] + f.x[i] * uv[0] + f.y[i] * uv[1])
    }

    /// The part edges some constraint uses (`usedRefs`), as the JSON the step stores.
    pub fn used_refs(&self) -> Vec<Value> {
        let mut used: BTreeSet<&str> = BTreeSet::new();
        for c in &self.constraints {
            for r in [&c.a, &c.b, &c.c].into_iter().flatten() {
                used.insert(r.e.as_str());
            }
        }
        self.refs
            .iter()
            .filter(|r| r.ent.id().is_some_and(|id| used.contains(id)))
            .filter_map(|r| serde_json::to_value(&r.ent).ok())
            .collect()
    }

    /// What `finishSketch` sends: `{plane, entities, refs, constraints}`.
    pub fn step_params(&self) -> Value {
        json!({
            "plane": self.plane_ref,
            "entities": self.entities,
            "refs": self.used_refs(),
            "constraints": self.constraints,
        })
    }
}

pub enum SketchInput {
    Down { uv: [f64; 2], pos: egui::Pos2, mods: egui::Modifiers },
    /// Hover and drag.
    Move { uv: [f64; 2], pos: egui::Pos2, mods: egui::Modifiers },
    Up { uv: [f64; 2], pos: egui::Pos2 },
    DoubleClick { uv: [f64; 2], pos: egui::Pos2 },
    Key { key: egui::Key, mods: egui::Modifiers },
    /// Typed size characters while drawing.
    Typed(String),
}

pub enum SketchDialog {
    Modify(String),
    Text { group: Option<String> },
    Offset,
    DimValue(usize),
    /// The Project outline dialog (`sketchProject`): refuses when there are no solid bodies.
    Project,
    /// `placeDimension`'s "Over defined" question for this (already measured) dimension.
    OverDefined(Box<Constraint>),
}

pub enum SketchEffect {
    Hint(String),
    Hud(Option<String>),
    Toast(crate::engine_link::Level, String),
    /// Sent by Task 2 as `Command::SketchOp`.
    SketchOp { op: String, data: Value },
    /// Entities or constraints changed: Task 2 re-solves and redraws.
    Changed,
    Dialog(SketchDialog),
    Finish { extrude: bool },
    Cancel,
}

pub struct ToolCx<'a> {
    pub s: &'a mut SketchSession,
    pub vp: &'a crate::viewport::Viewport,
    pub inch: bool,
    pub params: &'a BTreeMap<String, f64>,
    pub out: &'a mut Vec<SketchEffect>,
}

pub trait SketchTool {
    fn key(&self) -> &'static str;
    /// `toolHint`.
    fn hint(&self, s: &SketchSession) -> String;
    /// True = handled.
    fn input(&mut self, cx: &mut ToolCx, ev: &SketchInput) -> bool;
    /// Escape, tool change.
    fn cancel(&mut self, cx: &mut ToolCx);
    /// `pickingTool()`: dimension labels let clicks through.
    fn picks_geometry(&self) -> bool;
    /// The rubber band.
    fn preview(&self, s: &SketchSession) -> Vec<crate::viewport::overlays::SketchOutline>;
    fn sketch_op_result(&mut self, _cx: &mut ToolCx, _op: &str, _result: &Value) {}
    /// The shape under the pointer, painted in `COL_HOVER` (`hoverId`).
    fn hover(&self) -> Option<&str> {
        None
    }
}

/// One registry: `draw`, `relations`, `text` and `project` each have `tool_for`.
pub fn tool_for(key: &str) -> Option<Box<dyn SketchTool>> {
    draw::tool_for(key)
        .or_else(|| relations::tool_for(key))
        .or_else(|| text::tool_for(key))
        .or_else(|| project::tool_for(key))
}
