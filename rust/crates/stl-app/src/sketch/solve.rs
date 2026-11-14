//! Solving and the definition state (`sketcher.js` `solveAll`, `updateDefinition`, `defText`).
//!
//! `solve_all` solves on the calling thread; `request` / `apply` are the same work split in two
//! for the engine thread (`Command::Solve` / `Update::Solved`), used when a solve is slow.

use std::collections::BTreeSet;

use serde_json::json;
use stl_sketch::solver::{analyze, solve, unsatisfied};

use super::{Constraint, Entity, SketchSession};
use crate::engine_link::Command;

/// How well the sketch is pinned down (`defState`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Def {
    /// Degrees of freedom left: "Under defined: N more dimensions needed".
    Under(u32),
    Full,
    /// Some relations can't be met.
    Over,
}

/// The result of analysing a sketch after a solve.
#[derive(Clone, Debug, PartialEq)]
pub struct SolveState {
    pub def: Def,
    /// The definition line (`defText`); `None` when there is nothing to say (no shapes yet).
    pub msg: Option<String>,
    /// Indices into `SketchSession::constraints` of the relations that can't be met (red).
    pub conflicts: Vec<usize>,
    /// Shapes that can still move (drawn blue, `defn.under`).
    pub under: BTreeSet<String>,
    /// Shapes of the sketch that unmet relations touch (`this.bad`).
    pub bad: BTreeSet<String>,
}

impl Default for SolveState {
    fn default() -> Self {
        SolveState { def: Def::Full, msg: None, conflicts: Vec::new(), under: BTreeSet::new(), bad: BTreeSet::new() }
    }
}

const AXIS: f64 = 1e5;

thread_local! {
    /// The newest state, for the painters (`dims::paint` marks conflicting dimensions with it).
    static LAST: std::cell::RefCell<SolveState> = std::cell::RefCell::new(SolveState::default());
}

/// The state the last `solve_all` / `apply` on this thread produced.
pub fn last() -> SolveState {
    LAST.with(|l| l.borrow().clone())
}

/// What every sketch can measure from: the origin and the two axes (`baseRefs`).
pub fn base_refs() -> [Entity; 3] {
    let mk = |v: serde_json::Value| -> Entity { serde_json::from_value(v).expect("base ref") };
    [
        mk(json!({"id": "_o", "type": "point", "x": 0, "y": 0, "ref": "origin"})),
        mk(json!({"id": "_h", "type": "line", "p1": [-AXIS, 0], "p2": [AXIS, 0], "ref": "axis"})),
        mk(json!({"id": "_v", "type": "line", "p1": [0, -AXIS], "p2": [0, AXIS], "ref": "axis"})),
    ]
}

/// `refEnts`: the origin, the part edges (the missing ones too: their dimensions hold to where
/// they were), then the axes. None of them moves.
pub fn fixed_ents(s: &SketchSession) -> Vec<Entity> {
    let [o, h, v] = base_refs();
    let mut out = vec![o];
    out.extend(s.refs.iter().map(|r| r.ent.clone()));
    out.push(h);
    out.push(v);
    out
}

/// The sketch's shapes followed by `fixed_ents` (`allEnts`).
pub fn all_ents(s: &SketchSession) -> Vec<Entity> {
    let mut v = s.entities.clone();
    v.extend(fixed_ents(s));
    v
}

/// Ids that never move (`fixedIds`).
pub fn fixed_ids(s: &SketchSession) -> BTreeSet<String> {
    fixed_ents(s).iter().filter_map(|e| e.id().map(str::to_string)).collect()
}

/// Solve with the session's locks and write the result back (`solveAll`), then analyse.
pub fn solve_all(s: &mut SketchSession) -> SolveState {
    let fixed = fixed_ents(s);
    let r = solve(s.solve_from.as_ref().unwrap_or(&s.entities), &s.constraints, &s.locks, &fixed);
    s.entities = r.entities;
    state_of(s, &fixed)
}

/// `Command::Solve` with the inputs `solve_all` would use.
pub fn request(s: &SketchSession, seq: u64) -> Command {
    Command::Solve {
        seq,
        entities: s.solve_from.clone().unwrap_or_else(|| s.entities.clone()),
        constraints: s.constraints.clone(),
        locks: s.locks.clone(),
        fixed: fixed_ents(s),
    }
}

/// An engine-thread answer: write the entities, then analyse (the tail of `solve_all`).
/// `ok` and `error` are the solver's own report; the definition line comes from `analyze`.
pub fn apply(s: &mut SketchSession, entities: Vec<Entity>, _ok: bool, _error: f64) -> SolveState {
    s.entities = entities;
    let fixed = fixed_ents(s);
    state_of(s, &fixed)
}

fn same(a: &Constraint, b: &Constraint) -> bool {
    match (&a.id, &b.id) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

/// `updateDefinition` + `defText`.
fn state_of(s: &SketchSession, fixed: &[Entity]) -> SolveState {
    let an = analyze(&s.entities, &s.constraints, fixed);
    let bad = unsatisfied(&s.entities, &s.constraints, fixed);
    let conflicts: Vec<usize> =
        s.constraints.iter().enumerate().filter(|(_, c)| bad.iter().any(|b| same(b, c))).map(|(i, _)| i).collect();
    let fx: BTreeSet<&str> = fixed.iter().filter_map(|e| e.id()).collect();
    let mut bad_ents = BTreeSet::new();
    for c in &bad {
        for r in [&c.a, &c.b, &c.c].into_iter().flatten() {
            if !fx.contains(r.e.as_str()) {
                bad_ents.insert(r.e.clone());
            }
        }
    }
    let def = if !conflicts.is_empty() {
        Def::Over
    } else if an.dof > 0 {
        Def::Under(an.dof as u32)
    } else {
        Def::Full
    };
    let shown = s.entities.iter().any(|e| !matches!(e, Entity::Polyline(_)));
    let mut st = SolveState { def, msg: None, conflicts, under: an.under, bad: bad_ents };
    if shown {
        st.msg = Some(text_of(&st));
    }
    LAST.with(|l| *l.borrow_mut() = st.clone());
    st
}

fn text_of(st: &SolveState) -> String {
    let plural = |n: usize| if n > 1 { "s" } else { "" };
    match st.def {
        Def::Over => format!(
            "Over defined: {} relation{} can't be met (red)",
            st.conflicts.len(),
            plural(st.conflicts.len())
        ),
        Def::Under(n) => format!("Under defined: {n} more dimension{} needed", plural(n as usize)),
        Def::Full => "Fully defined".to_string(),
    }
}

/// `defText`: the definition line, empty when the sketch has no shapes.
pub fn def_text(st: &SolveState) -> String {
    st.msg.clone().unwrap_or_default()
}
