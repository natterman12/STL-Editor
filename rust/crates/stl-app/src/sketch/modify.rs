//! The modify tools: select (click, Shift-click, drag points and shapes), trim, mirror, round
//! corner; and the one-shot actions offset, delete, construction (`sketcher.js` `onClick`,
//! `dragStart` / `dragMove` / `dragEnd`, `roundCorner`, `toggleConstruction`, `deleteSelected`,
//! `replaceEntities`; `app.js` `sketchOffset`, `sketchServerOp`).

use std::collections::BTreeSet;

use serde_json::{json, Value};
use stl_sketch::entities::{parse_entities, point, point_keys, Entity, P2};
use stl_sketch::solver::{refs_exist, Lock, Ref, World};

use super::draw::{hint_for, CLICK_SLOP};
use super::{snap, SketchDialog, SketchEffect, SketchInput, SketchSession, SketchTool, ToolCx};
use crate::engine_link::Level;
use crate::viewport::overlays::SketchOutline;

/// The modify tools (the ribbon keys that stay active until another tool is picked).
pub fn tool_for(key: &str) -> Option<Box<dyn SketchTool>> {
    Some(match key {
        "select" => Box::new(SelectTool::default()),
        "trim" => Box::new(PickTool::new("trim")),
        "sk_mirror" => Box::new(PickTool::new("sk_mirror")),
        "sk_corner" => Box::new(PickTool::new("sk_corner")),
        _ => return None,
    })
}

/// The ribbon keys that act at once instead of becoming the tool (app.js lines 326–328).
pub const ACTION_KEYS: [&str; 3] = ["sk_offset", "sk_delete", "construction"];

/// Run a one-shot ribbon action; false for any other key.
pub fn action(key: &str, cx: &mut ToolCx) -> bool {
    match key {
        "sk_offset" => offset_start(cx),
        "sk_delete" => delete_selected(cx),
        "construction" => toggle_construction(cx),
        _ => return false,
    }
    true
}

/// The offset prompt's text and default (app.js `sketchOffset`).
pub const OFFSET_PROMPT: &str = "Offset distance (mm, negative = inward)";
pub const OFFSET_DEFAULT: &str = "2";

/// Offset: ask for the distance (`SketchDialog::Offset`), or say what to select first.
pub fn offset_start(cx: &mut ToolCx) {
    if cx.s.sel.is_empty() {
        cx.out.push(SketchEffect::Toast(Level::Warning, "Select the shapes to offset first (Select tool, Shift-click for more).".into()));
        return;
    }
    cx.out.push(SketchEffect::Dialog(SketchDialog::Offset));
}

/// The offset dialog's OK: send `sketch_op offset` with the typed distance (mm, as 1.x's prompt).
pub fn offset_send(cx: &mut ToolCx, text: &str) {
    let d = parse_num(text);
    let ids: Vec<&String> = cx.s.sel.iter().collect();
    let data = json!({ "ids": ids, "distance": d });
    send_op(cx, "offset", data);
}

/// app.js `parseNum`: arithmetic or a leading number, 0 when it isn't one.
pub fn parse_num(text: &str) -> f64 {
    let s = text.trim().replacen(',', ".", 1);
    if s.is_empty() {
        return 0.0;
    }
    if s.chars().all(|c| "0123456789+-*/(). ".contains(c)) {
        if let Some(r) = super::typed::arith(&s).filter(|r| r.is_finite()) {
            return r;
        }
    }
    let f = super::typed::parse_float(&s);
    if f.is_finite() { f } else { 0.0 }
}

/// The `sketch_op` answers the modify tools ask for.
pub const OP_KEYS: [&str; 3] = ["trim", "offset", "mirror"];

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

/// `sketchServerOp`: the op with the sketch's entities and a fresh seed (`Date.now().toString(36)`).
pub fn send_op(cx: &mut ToolCx, op: &str, mut data: Value) {
    let ms = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    if let Some(m) = data.as_object_mut() {
        m.insert("entities".into(), serde_json::to_value(&cx.s.entities).unwrap_or(Value::Null));
        m.insert("seed".into(), json!(base36(ms)));
    }
    cx.out.push(SketchEffect::SketchOp { op: op.into(), data });
}

/// `replaceEntities` with a `sketch_op` answer (trim, offset, mirror): history, the new
/// entities, the relations whose shapes are gone dropped, the selection kept where it still
/// exists. False when the answer has no entities.
pub fn apply_op_result(cx: &mut ToolCx, _op: &str, result: &Value) -> bool {
    let Some(list) = result.get("entities").and_then(Value::as_array) else { return false };
    let Ok(ents) = parse_entities(list) else { return false };
    cx.s.push();
    cx.s.entities = ents;
    prune_constraints(cx.s);
    let ids: BTreeSet<String> = cx.s.entities.iter().filter_map(|e| e.id().map(str::to_string)).collect();
    cx.s.sel.retain(|i| ids.contains(i));
    cx.out.push(SketchEffect::Changed);
    true
}

/// Drop relations and dimensions whose shapes are gone (`refsExist` against the sketch and its
/// fixed references).
fn prune_constraints(s: &mut SketchSession) {
    let mut all = s.entities.clone();
    all.extend(snap::fixed_ents(s));
    let w = World::new(&all);
    s.constraints.retain(|c| refs_exist(c, &w));
}

/// `deleteSelected`: the selected relation or dimension, else the selected shapes.
pub fn delete_selected(cx: &mut ToolCx) {
    if let Some(i) = cx.s.sel_c {
        if i < cx.s.constraints.len() {
            cx.s.push();
            cx.s.constraints.remove(i);
        }
        cx.s.sel_c = None;
        cx.out.push(SketchEffect::Changed);
        return;
    }
    if cx.s.sel.is_empty() {
        return;
    }
    cx.s.push();
    let sel = std::mem::take(&mut cx.s.sel);
    cx.s.entities.retain(|e| !e.id().is_some_and(|id| sel.contains(id)));
    prune_constraints(cx.s);
    cx.out.push(SketchEffect::Changed);
}

fn set_construction(e: &mut Entity, v: bool) {
    let v = Some(v);
    match e {
        Entity::Line(x) => x.construction = v,
        Entity::Arc(x) => x.construction = v,
        Entity::Circle(x) => x.construction = v,
        Entity::Rect(x) => x.construction = v,
        Entity::Polygon(x) => x.construction = v,
        Entity::Slot(x) => x.construction = v,
        Entity::Polyline(x) => x.construction = v,
        Entity::Point(x) => x.construction = v,
    }
}

/// `toggleConstruction`: the selected shapes become construction geometry, or stop being it.
pub fn toggle_construction(cx: &mut ToolCx) {
    if cx.s.sel.is_empty() {
        return;
    }
    cx.s.push();
    let sel = cx.s.sel.clone();
    for e in cx.s.entities.iter_mut() {
        if e.id().is_some_and(|id| sel.contains(id)) {
            let on = e.construction();
            set_construction(e, !on);
        }
    }
    cx.out.push(SketchEffect::Changed);
}

/// `groupOf`: text is one closed outline per letter part; they select together.
pub fn group_of(s: &SketchSession, id: &str) -> Vec<String> {
    let g = s.entities.iter().find(|e| e.id() == Some(id)).and_then(|e| snap::extra(e).get("group").cloned());
    match g {
        Some(g) if !g.is_null() && g != json!(false) && g != json!("") => s
            .entities
            .iter()
            .filter(|e| snap::extra(e).get("group") == Some(&g))
            .filter_map(|e| e.id().map(str::to_string))
            .collect(),
        _ => vec![id.to_string()],
    }
}

/// `roundCorner`: round the polyline corner nearest `uv` with a 2 mm arc (short segments).
/// The solve follows from `Changed` (1.x locks nothing for `pts`).
pub fn round_corner(cx: &mut ToolCx, id: &str, uv: P2) {
    let r = 2.0;
    let Some(Entity::Polyline(e)) = cx.s.entities.iter().find(|e| e.id() == Some(id)) else { return };
    let p_ = e.pts.clone();
    let n = p_.len();
    let closed = e.is_closed();
    let (mut i, mut bd) = (0usize, f64::INFINITY);
    for (k, p) in p_.iter().enumerate() {
        let d = (p[0] - uv[0]).hypot(p[1] - uv[1]);
        if d < bd {
            bd = d;
            i = k;
        }
    }
    if bd > 3.0 || (!closed && (i == 0 || i == n - 1)) {
        cx.out.push(SketchEffect::Toast(Level::Warning, "Click on a corner in the middle of the shape.".into()));
        return;
    }
    let p = p_[i];
    let a = p_[(i + n - 1) % n];
    let b = p_[(i + 1) % n];
    let la = (a[0] - p[0]).hypot(a[1] - p[1]);
    let lb = (b[0] - p[0]).hypot(b[1] - p[1]);
    let u = [(a[0] - p[0]) / la, (a[1] - p[1]) / la];
    let v = [(b[0] - p[0]) / lb, (b[1] - p[1]) / lb];
    let th = (u[0] * v[0] + u[1] * v[1]).clamp(-1.0, 1.0).acos();
    let tl = r / (th / 2.0).tan();
    if th < 0.02 || th > std::f64::consts::PI - 0.02 || tl > la.min(lb) * 0.95 {
        cx.out.push(SketchEffect::Toast(
            Level::Warning,
            "That corner is too sharp or too flat, or its lines are too short, for a 2 mm round.".into(),
        ));
        return;
    }
    let s0 = [p[0] + u[0] * tl, p[1] + u[1] * tl];
    let f = [p[0] + v[0] * tl, p[1] + v[1] * tl];
    let (bx, by) = (u[0] + v[0], u[1] + v[1]);
    let bl = bx.hypot(by);
    let dc = r / (th / 2.0).sin();
    let c = [p[0] + bx / bl * dc, p[1] + by / bl * dc];
    let a0 = (s0[1] - c[1]).atan2(s0[0] - c[0]);
    let mut sw = (f[1] - c[1]).atan2(f[0] - c[0]) - a0;
    if sw > std::f64::consts::PI {
        sw -= 2.0 * std::f64::consts::PI;
    } else if sw < -std::f64::consts::PI {
        sw += 2.0 * std::f64::consts::PI;
    }
    let mut pts: Vec<P2> = p_[..i].to_vec();
    pts.push(s0);
    for j in 1..8 {
        let t = a0 + sw * j as f64 / 8.0;
        pts.push([c[0] + r * t.cos(), c[1] + r * t.sin()]);
    }
    pts.push(f);
    pts.extend_from_slice(&p_[i + 1..]);
    cx.s.push();
    if let Some(Entity::Polyline(e)) = cx.s.entities.iter_mut().find(|e| e.id() == Some(id)) {
        e.pts = pts;
    }
    // 1.x runs this through `updateEntity`: say so when the relations can't all be kept
    cx.s.warn_unkept = true;
    cx.out.push(SketchEffect::Changed);
}

/// A drag in progress (`this._drag`).
#[derive(Clone, Debug)]
pub struct Drag {
    pub r: Ref,
    pub start: P2,
    pub base: Vec<Entity>,
    pub moved: bool,
}

/// The Select tool: click to select (Shift adds or removes), drag points and shapes.
#[derive(Default)]
pub struct SelectTool {
    down: Option<(egui::Pos2, bool)>,
    pub drag: Option<Drag>,
    /// The shape under the pointer (`hoverId`).
    pub hover: Option<String>,
}

impl SelectTool {
    /// `dragStart`: grab a point or a shape of the sketch (part edges don't move).
    pub fn drag_start(&mut self, cx: &mut ToolCx, raw: P2) -> bool {
        let Some(r) = snap::pick_own(cx.s, cx.vp, raw) else { return false };
        self.drag = Some(Drag { r, start: raw, base: cx.s.entities.clone(), moved: false });
        true
    }

    /// `dragMove`: set the locks that pull the grabbed point (or every point of the grabbed
    /// shape) to the pointer; the dispatcher solves.
    pub fn drag_move(&mut self, cx: &mut ToolCx, uv: P2) {
        let Some(d) = self.drag.as_mut() else { return };
        let (dx, dy) = (uv[0] - d.start[0], uv[1] - d.start[1]);
        if !d.moved && dx.hypot(dy) < 1e-9 {
            return;
        }
        if !d.moved {
            d.moved = true;
            cx.s.history.push((d.base.clone(), cx.s.constraints.clone()));
            if cx.s.history.len() > 100 {
                cx.s.history.remove(0);
            }
        }
        let lock = |k: &str, at: P2| Lock { e: d.r.e.clone(), k: Some(k.into()), at: Some(at.to_vec()), path: None, value: None };
        let locks = if let Some(k) = d.r.k.as_deref() {
            vec![lock(k, uv)]
        } else {
            let Some(be) = d.base.iter().find(|e| e.id() == Some(d.r.e.as_str())) else { return };
            let is_rect = matches!(be, Entity::Rect(_));
            point_keys(be)
                .into_iter()
                .filter(|k| k != "m" && !(is_rect && k == "c"))
                .filter_map(|k| point(be, &k).map(|q| lock(&k, [q[0] + dx, q[1] + dy])))
                .collect()
        };
        cx.s.locks = locks;
        cx.out.push(SketchEffect::Changed);
    }

    /// `dragEnd`: true when the drag moved something.
    pub fn drag_end(&mut self, cx: &mut ToolCx) -> bool {
        let Some(d) = self.drag.take() else { return false };
        cx.s.locks.clear();
        if d.moved {
            cx.out.push(SketchEffect::Changed);
        }
        d.moved
    }

    fn click(&mut self, cx: &mut ToolCx, uv: P2, shift: bool) {
        let id = snap::entity_at(cx.s, cx.vp, uv);
        cx.s.sel_c = None;
        let ids = id.as_deref().map(|i| group_of(cx.s, i)).unwrap_or_default();
        match id {
            Some(id) if shift => {
                let on = cx.s.sel.contains(&id);
                for g in ids {
                    if on {
                        cx.s.sel.remove(&g);
                    } else {
                        cx.s.sel.insert(g);
                    }
                }
            }
            _ => cx.s.sel = ids.into_iter().collect(),
        }
    }
}

impl SketchTool for SelectTool {
    fn key(&self) -> &'static str {
        "select"
    }

    fn hint(&self, _s: &SketchSession) -> String {
        hint_for("select").into()
    }

    fn hover(&self) -> Option<&str> {
        self.hover.as_deref()
    }

    fn input(&mut self, cx: &mut ToolCx, ev: &SketchInput) -> bool {
        match ev {
            SketchInput::Down { uv, pos, mods } => {
                self.down = Some((*pos, mods.shift));
                self.drag_start(cx, *uv)
            }
            SketchInput::Move { uv, .. } => {
                let sn = snap::snap(cx.s, cx.vp, *uv);
                let hover = snap::entity_at(cx.s, cx.vp, sn.uv);
                let changed = hover != self.hover;
                self.hover = hover;
                if self.drag.is_some() {
                    self.drag_move(cx, sn.uv);
                    return true;
                }
                changed
            }
            SketchInput::Up { uv, pos } => {
                let Some((d, shift)) = self.down.take() else { return false };
                let moved = d.distance(*pos) > CLICK_SLOP;
                let dragged = self.drag_end(cx);
                if !moved && !dragged {
                    let sn = snap::snap(cx.s, cx.vp, *uv);
                    self.click(cx, sn.uv, shift);
                }
                true
            }
            _ => false,
        }
    }

    fn cancel(&mut self, cx: &mut ToolCx) {
        self.drag_end(cx);
        self.down = None;
    }

    fn picks_geometry(&self) -> bool {
        false
    }

    fn preview(&self, _s: &SketchSession) -> Vec<SketchOutline> {
        Vec::new()
    }

    /// Offset is an action: its answer comes back while Select (or any tool) is active.
    fn sketch_op_result(&mut self, cx: &mut ToolCx, op: &str, result: &Value) {
        if OP_KEYS.contains(&op) {
            apply_op_result(cx, op, result);
        }
    }
}

/// Trim, Mirror and Round corner: one click on a shape does the job.
pub struct PickTool {
    key: &'static str,
    down: Option<egui::Pos2>,
}

impl PickTool {
    fn new(key: &'static str) -> Self {
        PickTool { key, down: None }
    }

    fn click(&mut self, cx: &mut ToolCx, raw: P2) {
        let uv = snap::snap(cx.s, cx.vp, raw).uv;
        let id = snap::entity_at(cx.s, cx.vp, uv);
        let warn = |cx: &mut ToolCx, m: &str| cx.out.push(SketchEffect::Toast(Level::Warning, m.into()));
        match self.key {
            "trim" => {
                let Some(id) = id else { return };
                send_op(cx, "trim", json!({ "id": id, "point": uv }));
            }
            "sk_corner" => {
                let is_poly = id.as_deref().is_some_and(|i| matches!(cx.s.entities.iter().find(|e| e.id() == Some(i)), Some(Entity::Polyline(_))));
                match id {
                    Some(id) if is_poly => round_corner(cx, &id, uv),
                    _ => warn(cx, "Click a corner of a polyline (a shape drawn as connected lines)."),
                }
            }
            _ => {
                let straight = id.as_deref().is_some_and(|i| {
                    matches!(cx.s.entities.iter().find(|e| e.id() == Some(i)), Some(Entity::Line(_) | Entity::Polyline(_)))
                });
                let Some(id) = id.filter(|_| straight) else {
                    return warn(cx, "Click a straight line to mirror across.");
                };
                if cx.s.sel.is_empty() {
                    return warn(cx, "First select the shapes to mirror (Select tool), then choose Mirror and click the mirror line.");
                }
                let ids: Vec<&String> = cx.s.sel.iter().collect();
                let data = json!({ "ids": ids, "axis": id });
                send_op(cx, "mirror", data);
            }
        }
    }
}

impl SketchTool for PickTool {
    fn key(&self) -> &'static str {
        self.key
    }

    fn hint(&self, _s: &SketchSession) -> String {
        hint_for(self.key).into()
    }

    fn input(&mut self, cx: &mut ToolCx, ev: &SketchInput) -> bool {
        match ev {
            SketchInput::Down { pos, .. } => {
                self.down = Some(*pos);
                true
            }
            SketchInput::Up { uv, pos } => {
                let Some(d) = self.down.take() else { return false };
                if d.distance(*pos) <= CLICK_SLOP {
                    self.click(cx, *uv);
                }
                true
            }
            _ => false,
        }
    }

    fn cancel(&mut self, _cx: &mut ToolCx) {
        self.down = None;
    }

    fn picks_geometry(&self) -> bool {
        self.key != "sk_corner"
    }

    fn preview(&self, _s: &SketchSession) -> Vec<SketchOutline> {
        Vec::new()
    }

    fn sketch_op_result(&mut self, cx: &mut ToolCx, op: &str, result: &Value) {
        if OP_KEYS.contains(&op) {
            apply_op_result(cx, op, result);
        }
    }
}
