//! Sketch mode's session: what is open, the tool in use, solving, the part-edge refs and the
//! pointer / key dispatcher (`sketcher.js` `start`, `setTool`, `onKey`, `solveAll`, `setRefs`,
//! and the sketch flow of `app.js`). The `StlApp` side (engine commands, toasts, the panel) is in
//! `app_wiring.rs`; this module has no window in it, so tests drive it directly.

use std::collections::BTreeMap;

use serde_json::Value;
use stl_sketch::entities::{parse_entities, Entity};
use web_time::Instant;

use super::solve::{self, SolveState};
use super::{Constraint, RefEnt, SketchEffect, SketchInput, SketchSession, SketchTool, ToolCx};
use crate::engine_link::Command;
use crate::viewport::Viewport;

/// A solve that took less than this runs on the UI thread, as 1.x does; a slower one goes to the
/// engine thread (`Command::Solve`) with the newest answer winning.
pub const INLINE_MS: f64 = 4.0;

/// `updateEntity`'s warning when the relations can't all be kept.
pub const UNKEPT: &str = "Some constraints could not be kept.";

/// What a sent solve was asked about; its answer is used only while this still matches (a drag
/// changes only the locks, so the answer to the one before still counts).
#[derive(Clone, Debug, PartialEq)]
pub struct Snap {
    hist: usize,
    ids: Vec<Option<String>>,
    cons: Vec<Constraint>,
}

impl Snap {
    pub fn of(s: &SketchSession) -> Snap {
        Snap { hist: s.history.len(), ids: s.entities.iter().map(|e| e.id().map(str::to_owned)).collect(), cons: s.constraints.clone() }
    }
}

pub struct SolveCtl {
    /// How long the last solve took (0 at the start: the first one runs inline).
    pub last_ms: f64,
    pub seq: u64,
    /// The one solve in flight: its `seq`, what it was asked about and when it was sent.
    pub inflight: Option<(u64, Snap, Instant)>,
    /// The session changed since the last send.
    pub stale: bool,
    /// The solve in flight follows a typed-field edit (see `SketchSession::warn_unkept`).
    pub warn_inflight: bool,
}

impl Default for SolveCtl {
    fn default() -> Self {
        SolveCtl { last_ms: 0.0, seq: 0, inflight: None, stale: false, warn_inflight: false }
    }
}

/// The open sketch.
pub struct SketchMode {
    pub s: SketchSession,
    pub tool: Option<Box<dyn SketchTool>>,
    /// The rollback marker to put back on Cancel (editing a step).
    pub prev_rollback: Option<usize>,
    /// The saved part edges of an edited sketch, for matching the fresh ones (`savedRefs`).
    pub saved_refs: Vec<Entity>,
    /// The `refs` question that has not been answered.
    pub refs_seq: Option<u64>,
    /// A question a tool asked (`sketch_op`): its seq and op name.
    pub op: Option<(u64, String)>,
    pub solve: SolveCtl,
    pub state: SolveState,
    pub hint: String,
    pub hud: Option<String>,
    /// Where the pointer is (snapped, plane coordinates, mm), for the status line.
    pub cursor: Option<[f64; 2]>,
    /// The pointer sits on a snapped point (`this.snapped`).
    pub snapped: bool,
    /// The last status text the app showed (it only repaints the line when this changes).
    pub last_status: String,
    /// The sketch has to be drawn again.
    pub dirty: bool,
    /// The pointer went down on the sketch and has not come up.
    pub down: bool,
    pub touch: super::touch::TouchState,
    pub panel: super::panel::PanelState,
    /// Finish was pressed and the engine has not taken the step yet; `Some(extrude)`.
    pub finishing: Option<bool>,
    /// A toast the solver wants shown (the app drains it).
    pub toast: Option<String>,
}

impl SketchMode {
    pub fn new(s: SketchSession, prev_rollback: Option<usize>, saved_refs: Vec<Entity>) -> SketchMode {
        let mut m = SketchMode {
            s,
            tool: None,
            prev_rollback,
            saved_refs,
            refs_seq: None,
            op: None,
            solve: SolveCtl::default(),
            state: SolveState::default(),
            hint: String::new(),
            hud: None,
            cursor: None,
            snapped: false,
            last_status: String::new(),
            dirty: true,
            down: false,
            touch: Default::default(),
            panel: Default::default(),
            finishing: None,
            toast: None,
        };
        m.start_solve();
        m.hint = tool_hint(&m.s.tool);
        m
    }

    /// `start`: solve once, and keep the result only when the relations can be met.
    fn start_solve(&mut self) {
        let fixed = solve::fixed_ents(&self.s);
        let ents = if self.s.constraints.is_empty() {
            self.s.entities.clone()
        } else {
            let r = stl_sketch::solver::solve(&self.s.entities, &self.s.constraints, &[], &fixed);
            if r.ok { r.entities } else { self.s.entities.clone() }
        };
        self.state = solve::apply(&mut self.s, ents, true, 0.0);
    }

    /// After `SketchEffect::Changed`: solve now when solves are quick, else ask the engine thread
    /// (one at a time). Returns the command to send.
    pub fn changed(&mut self) -> Option<Command> {
        self.dirty = true;
        if self.solve.last_ms < INLINE_MS && self.solve.inflight.is_none() {
            let t = Instant::now();
            let warn = std::mem::take(&mut self.s.warn_unkept);
            let fixed = solve::fixed_ents(&self.s);
            let r = stl_sketch::solver::solve(&self.s.entities, &self.s.constraints, &self.s.locks, &fixed);
            let ok = r.ok;
            self.state = solve::apply(&mut self.s, r.entities, ok, r.error);
            if warn && !ok {
                self.toast = Some(UNKEPT.into());
            }
            self.s.locks.clear();
            self.solve.last_ms = t.elapsed().as_secs_f64() * 1000.0;
            self.solve.stale = false;
            return None;
        }
        self.solve.stale = true;
        self.send_solve()
    }

    fn send_solve(&mut self) -> Option<Command> {
        if self.solve.inflight.is_some() || !self.solve.stale {
            return None;
        }
        self.solve.seq += 1;
        let seq = self.solve.seq;
        self.solve.warn_inflight = std::mem::take(&mut self.s.warn_unkept);
        let cmd = solve::request(&self.s, seq);
        self.solve.inflight = Some((seq, Snap::of(&self.s), Instant::now()));
        self.solve.stale = false;
        self.s.locks.clear();
        Some(cmd)
    }

    /// `Update::Solved`: apply it when it is the answer to the solve in flight and nothing but
    /// the locks changed since; then send the next one if the session changed meanwhile.
    pub fn solved(&mut self, seq: u64, entities: Vec<Entity>, ok: bool, error: f64) -> Option<Command> {
        let Some((want, snap, sent)) = self.solve.inflight.take() else { return None };
        if want != seq {
            self.solve.inflight = Some((want, snap, sent));
            return None;
        }
        let warn = std::mem::take(&mut self.solve.warn_inflight);
        if warn && !ok {
            self.toast = Some(UNKEPT.into());
        }
        if snap == Snap::of(&self.s) {
            self.state = solve::apply(&mut self.s, entities, ok, error);
            self.dirty = true;
            let ms = sent.elapsed().as_secs_f64() * 1000.0;
            if ms < INLINE_MS {
                self.solve.last_ms = ms;
            }
        } else {
            self.solve.stale = true;
        }
        self.send_solve()
    }

    /// The engine thread died or answered a command with an error: drop the solve in flight so
    /// solving can't stay stuck (the next change asks again).
    pub fn solve_lost(&mut self) {
        if self.solve.inflight.take().is_some() {
            self.solve.warn_inflight = false;
            self.solve.stale = true;
        }
    }

    /// The part edges came back (`setRefs`): match them to the saved ones, red where one is gone.
    pub fn set_refs(&mut self, fresh: &[Entity], kind: Option<String>) -> usize {
        let (list, missing) = match_refs(fresh, &self.saved_refs);
        let mut refs: Vec<RefEnt> = list.into_iter().map(|ent| RefEnt { ent, missing: false }).collect();
        let lost = missing.len();
        refs.extend(missing.into_iter().map(|ent| RefEnt { ent, missing: true }));
        self.s.refs = refs;
        self.s.ref_kind = kind;
        lost
    }
}

/// `hasProfile` (app.js `finishSketch`): something that is not construction and not an open polyline.
pub fn has_profile(entities: &[Entity]) -> bool {
    entities.iter().any(|e| !e.construction() && !matches!(e, Entity::Polyline(p) if p.closed != Some(true)))
}

/// The part-edge entities of a `sketch_op refs` answer.
pub fn refs_of(result: &Value) -> (Vec<Entity>, Option<String>) {
    let list: Vec<Value> = result.get("refs").and_then(Value::as_array).cloned().unwrap_or_default();
    // the route's entities carry only what a line or circle has; anything else is dropped as 1.x does
    let list: Vec<Value> = list.into_iter().filter(|v| v.get("type").is_some()).collect();
    (parse_entities(&list).unwrap_or_default(), result.get("kind").and_then(Value::as_str).map(str::to_owned))
}

// ------------------------------------------------------------------ matchRefs (sketcher.js)

type P2 = [f64; 2];

fn dist(a: P2, b: P2) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn ends(e: &Entity) -> Option<(P2, P2)> {
    match e {
        Entity::Line(l) => Some((l.p1, l.p2)),
        _ => None,
    }
}

fn circ(e: &Entity) -> Option<(P2, f64)> {
    match e {
        Entity::Circle(c) => Some(([c.cx, c.cy], c.r)),
        Entity::Arc(c) => Some(([c.cx, c.cy], c.r)),
        _ => None,
    }
}

fn same_type(a: &Entity, b: &Entity) -> bool {
    a.kind() == b.kind()
}

/// `{d, flip}` of a saved edge against a fresh one of the same kind.
fn score(s: &Entity, f: &Entity) -> Option<(f64, bool)> {
    if !same_type(s, f) {
        return None;
    }
    if let (Some((a1, a2)), Some((b1, b2))) = (ends(s), ends(f)) {
        let d1 = dist(a1, b1) + dist(a2, b2);
        let d2 = dist(a1, b2) + dist(a2, b1);
        return Some(if d1 <= d2 { (d1, false) } else { (d2, true) });
    }
    let ((sc, sr), (fc, fr)) = (circ(s)?, circ(f)?);
    Some((dist(sc, fc) + (sr - fr).abs(), false))
}

/// A moved edge: how far it moved sideways, plus a little for sliding along or changing length.
fn near(s: &Entity, f: &Entity) -> Option<(f64, bool)> {
    let (Some((s1, s2)), Some((f1, f2))) = (ends(s), ends(f)) else { return score(s, f) };
    let u = [s2[0] - s1[0], s2[1] - s1[1]];
    let l = u[0].hypot(u[1]);
    let l1 = if l == 0.0 { 1.0 } else { l };
    let n = [-u[1] / l1, u[0] / l1];
    let ms = [(s1[0] + s2[0]) / 2.0, (s1[1] + s2[1]) / 2.0];
    let mf = [(f1[0] + f2[0]) / 2.0, (f1[1] + f2[1]) / 2.0];
    let side = ((mf[0] - ms[0]) * n[0] + (mf[1] - ms[1]) * n[1]).abs();
    let along = ((mf[0] - ms[0]) * u[0] / l1 + (mf[1] - ms[1]) * u[1] / l1).abs();
    let fl = (f2[0] - f1[0]).hypot(f2[1] - f1[1]);
    let flip = (f2[0] - f1[0]) * u[0] + (f2[1] - f1[1]) * u[1] < 0.0;
    Some((side + 0.25 * along + 0.1 * (fl - l).abs(), flip))
}

fn parallel(s: &Entity, f: &Entity) -> bool {
    let (Some((s1, s2)), Some((f1, f2))) = (ends(s), ends(f)) else { return true };
    let a = [s2[0] - s1[0], s2[1] - s1[1]];
    let b = [f2[0] - f1[0], f2[1] - f1[1]];
    let den = a[0].hypot(a[1]) * b[0].hypot(b[1]);
    let den = if den == 0.0 { 1.0 } else { den };
    (a[0] * b[1] - a[1] * b[0]).abs() / den < 0.02
}

fn set_id(e: &mut Entity, id: String) {
    match e {
        Entity::Line(x) => x.id = Some(id),
        Entity::Arc(x) => x.id = Some(id),
        Entity::Circle(x) => x.id = Some(id),
        Entity::Rect(x) => x.id = Some(id),
        Entity::Polygon(x) => x.id = Some(id),
        Entity::Slot(x) => x.id = Some(id),
        Entity::Polyline(x) => x.id = Some(id),
        Entity::Point(x) => x.id = Some(id),
    }
}

/// `matchRefs`: the part edges a sketch used again after the model changed: the same edge if
/// it is still there, otherwise the nearest one like it. Saved ones not found come back as
/// `missing`.
pub fn match_refs(fresh: &[Entity], saved: &[Entity]) -> (Vec<Entity>, Vec<Entity>) {
    let mut list: Vec<Entity> = fresh.to_vec();
    let mut pts: Vec<P2> = Vec::new();
    for e in fresh.iter().chain(saved.iter()) {
        if let Some((a, b)) = ends(e) {
            pts.extend([a, b]);
        } else if let Some((c, r)) = circ(e) {
            pts.extend([[c[0] - r, c[1] - r], [c[0] + r, c[1] + r]]);
        }
    }
    let mut size = 100.0;
    if !pts.is_empty() {
        let xs = pts.iter().map(|p| p[0]);
        let ys = pts.iter().map(|p| p[1]);
        let (x0, x1) = (xs.clone().fold(f64::INFINITY, f64::min), xs.fold(f64::NEG_INFINITY, f64::max));
        let (y0, y1) = (ys.clone().fold(f64::INFINITY, f64::min), ys.fold(f64::NEG_INFINITY, f64::max));
        size = (x1 - x0).hypot(y1 - y0).max(1.0);
    }
    let mut taken = vec![false; list.len()];
    let mut missing: Vec<Entity> = Vec::new();
    let mut left: Vec<&Entity> = Vec::new();
    let flip_ends = |e: &mut Entity| {
        if let Entity::Line(l) = e {
            std::mem::swap(&mut l.p1, &mut l.p2);
        }
    };
    for s in saved {
        let mut best: Option<(usize, f64, bool)> = None;
        for (i, f) in list.iter().enumerate() {
            if taken[i] {
                continue;
            }
            if let Some((d, flip)) = score(s, f) {
                if best.map_or(true, |b| d < b.1) {
                    best = Some((i, d, flip));
                }
            }
        }
        match best {
            Some((i, d, flip)) if d < 1e-3_f64.max(size * 1e-5) => {
                if let Some(id) = s.id() {
                    set_id(&mut list[i], id.to_owned());
                }
                if flip {
                    flip_ends(&mut list[i]);
                }
                taken[i] = true;
            }
            _ => left.push(s),
        }
    }
    // the rest: best pairs first, so each edge goes to the one most like it
    let mut pairs: Vec<(usize, usize, f64, bool)> = Vec::new();
    for (si, s) in left.iter().enumerate() {
        for (i, f) in list.iter().enumerate() {
            if taken[i] || !parallel(s, f) {
                continue;
            }
            if let Some((d, flip)) = near(s, f) {
                if d < size * 0.35 {
                    pairs.push((si, i, d, flip));
                }
            }
        }
    }
    pairs.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
    let mut done = vec![false; left.len()];
    for (si, i, _, flip) in pairs {
        if !done[si] && !taken[i] {
            if let Some(id) = left[si].id() {
                set_id(&mut list[i], id.to_owned());
            }
            if flip {
                flip_ends(&mut list[i]);
            }
            taken[i] = true;
            done[si] = true;
        }
    }
    for (si, s) in left.iter().enumerate() {
        if !done[si] {
            missing.push((*s).clone());
        }
    }
    let mut n = 0u32;
    for s in saved {
        if let Some(num) = s.id().and_then(|id| id.strip_prefix('r')).and_then(|d| d.parse::<u32>().ok()) {
            n = n.max(num + 1);
        }
    }
    for f in list.iter_mut() {
        if f.id().is_none() {
            set_id(f, format!("r{n}"));
            n += 1;
        }
    }
    (list, missing)
}

// ------------------------------------------------------------------ the tool

/// `toolHint()`, word for word (the relation tools' hints are `CTOOLS`' own).
pub fn tool_hint(tool: &str) -> String {
    match tool {
        "select" => "Click a shape to select it · Delete removes it · Right-drag to orbit, middle-drag to pan",
        "line" => "Click points to draw connected lines · click the first point to close · Enter or Esc to stop · type a length",
        "arc3" => "Click the start, then the end, then a point the arc passes through · or type a radius",
        "trim" => "Click the piece of a line, arc or shape to cut away (up to where it crosses something)",
        "sk_mirror" => "Select shapes first, then click the straight line to mirror them across",
        "sk_corner" => "Click a corner of a polyline to round it (2 mm radius)",
        "sk_useedge" => "Click an orange part edge to turn it into a sketch line or circle (it stays tied to the edge)",
        "rect" => "Click two corners · or type width, Tab, height",
        "rect_center" => "Click the centre, then a corner · or type width, Tab, height",
        "circle" => "Click the centre, then the edge · or type a diameter",
        "polygon" => "Click the centre, then a corner · type radius, Tab, number of sides",
        "slot" => "Click both arc centres, then the width",
        "text" => "Click where the text should start",
        "c_coincident" => "Click two points (or a point, then a line or circle) to join them",
        "c_hv" => "Click a line (or two points) to make it horizontal or vertical",
        "c_parallel" | "c_perpendicular" => "Click two lines",
        "c_equal" => "Click two lines (same length) or two circles / arcs (same size)",
        "c_tangent" => "Click a line and an arc / circle, or two arcs / circles",
        "c_concentric" => "Click two circles or arcs",
        "c_midpoint" => "Click a point, then a line",
        "c_fix" => "Click a point or shape to lock it in place",
        "c_symmetric" => "Click two points, then the line they mirror about",
        "c_dimension" => "Click a line, circle or arc (or two points, a point and a line, or two lines), move to place it, click, then type the value",
        _ => "",
    }
    .to_owned()
}

/// The tools the ribbon switches to (`onRibbon`'s first list, plus `sk_project`).
pub fn is_tool_key(k: &str) -> bool {
    matches!(k, "line" | "arc3" | "rect" | "rect_center" | "circle" | "polygon" | "slot" | "select" | "trim" | "sk_mirror" | "sk_corner" | "text" | "sk_useedge" | "sk_project") || k.starts_with("c_")
}

/// Everything a tool call needs from the app.
pub struct Env<'a> {
    pub vp: &'a Viewport,
    pub inch: bool,
    pub params: &'a BTreeMap<String, f64>,
}

impl SketchMode {
    /// sketcher.js `showStatus`: "Sketch  X 25  Y 15 mm  ·  snapped   ·   Under defined: ...".
    pub fn status_line(&self, inch: bool) -> String {
        let at = match self.cursor {
            Some(uv) => format!(
                "Sketch  X {}  Y {} {}{}",
                super::typed::fmt_len(uv[0], inch),
                super::typed::fmt_len(uv[1], inch),
                if inch { "in" } else { "mm" },
                if self.snapped { "  · snapped" } else { "" }
            ),
            None => "Sketch".to_owned(),
        };
        let d = solve::def_text(&self.state);
        if d.is_empty() { at } else { format!("{at}   ·   {d}") }
    }

    /// `setTool(t)`: the old tool stops, the new one starts, relation picks are cleared.
    pub fn set_tool(&mut self, key: &str, env: &Env) -> Vec<SketchEffect> {
        let mut out = Vec::new();
        if let Some(mut t) = self.tool.take() {
            let mut cx = ToolCx { s: &mut self.s, vp: env.vp, inch: env.inch, params: env.params, out: &mut out };
            t.cancel(&mut cx);
        }
        super::dims::set_pending(Default::default());
        self.s.tool = key.to_owned();
        self.s.sel_c = None;
        self.tool = super::tool_for(key);
        self.hint = self.tool.as_ref().map(|t| t.hint(&self.s)).filter(|h| !h.is_empty()).unwrap_or_else(|| tool_hint(key));
        self.hud = None;
        self.dirty = true;
        out.push(SketchEffect::Hint(self.hint.clone()));
        out
    }

    /// A pointer or key input for the tool in use. Returns what the tool wants done.
    pub fn input(&mut self, ev: &SketchInput, env: &Env) -> (bool, Vec<SketchEffect>) {
        let mut out = Vec::new();
        let mut handled = false;
        if let SketchInput::Move { uv, .. } | SketchInput::Down { uv, .. } = ev {
            let sn = super::snap::snap(&self.s, env.vp, *uv);
            self.cursor = Some(sn.uv);
            self.snapped = sn.snapped();
        }
        if let Some(t) = self.tool.as_mut() {
            let mut cx = ToolCx { s: &mut self.s, vp: env.vp, inch: env.inch, params: env.params, out: &mut out };
            handled = t.input(&mut cx, ev);
        }
        if handled {
            self.dirty = true;
        }
        (handled, out)
    }

    /// A tool's `sketch_op` answer.
    pub fn op_result(&mut self, op: &str, result: &Value, env: &Env) -> Vec<SketchEffect> {
        let mut out = Vec::new();
        if let Some(t) = self.tool.as_mut() {
            let mut cx = ToolCx { s: &mut self.s, vp: env.vp, inch: env.inch, params: env.params, out: &mut out };
            t.sketch_op_result(&mut cx, op, result);
        }
        self.dirty = true;
        out
    }

    /// What the tool in use draws while it works (the rubber band).
    pub fn preview(&self) -> Vec<crate::viewport::overlays::SketchOutline> {
        self.tool.as_ref().map(|t| t.preview(&self.s)).unwrap_or_default()
    }

    /// The shape the select tool hovers.
    pub fn hover(&self) -> Option<&str> {
        self.tool.as_ref().and_then(|t| t.hover())
    }

    /// `pickingTool()`: dimension labels let clicks through while picking geometry.
    pub fn picking(&self) -> bool {
        self.tool.as_ref().is_some_and(|t| t.picks_geometry())
    }
}
