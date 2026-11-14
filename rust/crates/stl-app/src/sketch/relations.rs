//! Sketch tools: relations and Smart Dimension (`sketcher.js` `constraintClick`, `dimClick`,
//! `updatePending`, `addConstraint`, `deleteConstraint`, `pickRef`, the part-edge drawing `_refs`
//! and the relation glyphs of `buildDims`).

use egui::{vec2, Color32, FontId, Rect, Sense, Shape, Stroke, StrokeKind};
use serde_json::Value;
use stl_sketch::entities::{point, point_keys, P2};
use stl_sketch::solver::{glyph_spots, infer_dimension, make_constraint, refs_exist, solve, Constraint, Outcome, Ref, World};

use super::dims::{self, Pend};
use super::geom::{dist_to_entity, outline, seg_dist};
use super::solve::{all_ents, fixed_ents, fixed_ids};
use super::{Entity, SketchEffect, SketchInput, SketchSession, SketchTool, ToolCx};
use crate::engine_link::Level;
use crate::viewport::overlays::SketchOutline;
use crate::viewport::Viewport;

/// `CTOOLS`: key, label, hint.
pub const CTOOLS: [(&str, &str, &str); 11] = [
    ("c_coincident", "Coincident", "Click two points (or a point, then a line or circle) to join them"),
    ("c_hv", "Horiz / Vert", "Click a line (or two points) to make it horizontal or vertical"),
    ("c_parallel", "Parallel", "Click two lines"),
    ("c_perpendicular", "Perpendicular", "Click two lines"),
    ("c_equal", "Equal", "Click two lines (same length) or two circles / arcs (same size)"),
    ("c_tangent", "Tangent", "Click a line and an arc / circle, or two arcs / circles"),
    ("c_concentric", "Concentric", "Click two circles or arcs"),
    ("c_midpoint", "Midpoint", "Click a point, then a line"),
    ("c_fix", "Fix", "Click a point or shape to lock it in place"),
    ("c_symmetric", "Symmetric", "Click two points, then the line they mirror about"),
    (
        "c_dimension",
        "Smart Dimension",
        "Click a line, circle or arc (or two points, a point and a line, or two lines), move to place it, click, then type the value",
    ),
];

const HINT_PLACE: &str = "Move to place the dimension and click · or click a second item (point, line, circle)";
const HINT_SECOND: &str = "Now click a second item: a point, line or circle";

fn warn(msg: impl Into<String>) -> SketchEffect {
    SketchEffect::Toast(Level::Warning, msg.into())
}

// ------------------------------------------------------------------ picking

#[derive(Clone, Copy)]
pub struct PickOpts {
    /// Part edges and the origin can be picked too.
    pub refs: bool,
    /// Rectangles answer with the side under the pointer.
    pub sides: bool,
    pub axes: bool,
    pub points: bool,
}

impl Default for PickOpts {
    fn default() -> Self {
        PickOpts { refs: false, sides: false, axes: true, points: true }
    }
}

fn is_axis(id: &str) -> bool {
    id == "_h" || id == "_v"
}

/// `pickRef`: what's under the pointer: a point (`e` and `k`), else a shape (rectangles with the side).
pub fn pick_ref(s: &SketchSession, vp: &Viewport, uv: P2, o: PickOpts) -> Option<Ref> {
    let px = vp.pixel_size(s.to_3(uv));
    let px = if px.is_finite() && px > 0.0 { px } else { 0.1 };
    let fixed = fixed_ents(s);
    let pool: Vec<&Entity> = if o.refs { s.entities.iter().chain(fixed.iter()).collect() } else { s.entities.iter().collect() };
    let is_ref = |id: &str| fixed.iter().any(|f| f.id() == Some(id));
    if o.points {
        let (mut best, mut bd) = (None, 9.0 * px);
        for e in &pool {
            let Some(id) = e.id() else { continue };
            if is_axis(id) {
                continue;
            }
            for k in point_keys(e) {
                if k == "m" {
                    continue;
                }
                let Some(q) = point(e, &k) else { continue };
                let d = (q[0] - uv[0]).hypot(q[1] - uv[1]);
                if d < bd {
                    bd = d;
                    best = Some(Ref::pt(id, &k));
                }
            }
        }
        if best.is_some() {
            return best;
        }
    }
    let (mut be, mut bed) = (None::<&Entity>, 8.0 * px);
    for e in &pool {
        let Some(id) = e.id() else { continue };
        if matches!(e, Entity::Point(_)) {
            continue;
        }
        if is_axis(id) && !o.axes {
            continue;
        }
        // the sketch's own shapes win over part edges, and those over the axes, when close
        let pen = if is_axis(id) { 4.0 * px } else if is_ref(id) { 2.0 * px } else { 0.0 };
        let d = dist_to_entity(e, uv) + pen;
        if d < bed {
            bed = d;
            be = Some(e);
        }
    }
    let be = be?;
    let id = be.id()?;
    if let (true, Entity::Rect(r)) = (o.sides, be) {
        if r.angle.unwrap_or(0.0) == 0.0 {
            let c: Vec<P2> = ["c0", "c1", "c2", "c3"].iter().filter_map(|k| point(be, k)).collect();
            let (mut side, mut sd) = (0i64, f64::INFINITY);
            for i in 0..4 {
                let d = seg_dist(uv, c[i], c[(i + 1) % 4]);
                if d < sd {
                    sd = d;
                    side = i as i64;
                }
            }
            return Some(Ref { e: id.to_string(), k: None, side: Some(side) });
        }
    }
    Some(Ref::ent(id))
}

// ------------------------------------------------------------------ edits

/// `addConstraint`: solve with it; a conflict undoes it with the 1.x message.
pub fn add_constraint(s: &mut SketchSession, c: Constraint) -> Vec<SketchEffect> {
    s.push();
    let mut cc = c;
    cc.id = Some(Value::String(s.new_id()));
    let mut cons = s.constraints.clone();
    cons.push(cc.clone());
    let r = solve(&s.entities, &cons, &[], &fixed_ents(s));
    if !r.ok {
        s.history.pop();
        return vec![warn("That would conflict with the existing constraints (over-constrained).")];
    }
    s.constraints.push(cc);
    s.entities = r.entities;
    vec![SketchEffect::Changed]
}

/// `deleteConstraint` (by position in `constraints`).
pub fn delete_constraint(s: &mut SketchSession, idx: usize) -> Vec<SketchEffect> {
    if idx >= s.constraints.len() {
        return vec![];
    }
    s.push();
    s.constraints.remove(idx);
    s.sel_c = None;
    vec![SketchEffect::Changed]
}

/// The warning `enterSketch` shows when part edges the sketch measured from are gone.
pub fn missing_toast(s: &SketchSession) -> Option<SketchEffect> {
    let lost = s.refs.iter().filter(|r| r.missing).count();
    if lost == 0 {
        return None;
    }
    let m = lost > 1;
    Some(warn(format!(
        "{lost} part edge{} this sketch measured from {} changed (shown red). Its dimensions hold to where {}: delete them and dimension to the new edge{}.",
        if m { "s" } else { "" },
        if m { "have" } else { "has" },
        if m { "they were" } else { "it was" },
        if m { "s" } else { "" },
    )))
}

// ------------------------------------------------------------------ the tools

struct RelTool {
    key: &'static str,
    hint: &'static str,
    picks: Vec<Ref>,
    cand: Option<Constraint>,
}

/// One tool for each `CTOOLS` key.
pub fn tool_for(key: &str) -> Option<Box<dyn SketchTool>> {
    let (k, _, hint) = CTOOLS.iter().find(|(k, _, _)| *k == key)?;
    Some(Box::new(RelTool { key: k, hint, picks: Vec::new(), cand: None }))
}

impl RelTool {
    fn sync(&self) {
        let mut p = dims::pending();
        p.picks = self.picks.clone();
        p.cand = self.cand.clone();
        dims::set_pending(p);
    }

    fn hint_now(&self) -> String {
        if self.key == "c_dimension" {
            if self.cand.is_some() {
                return HINT_PLACE.into();
            }
            if !self.picks.is_empty() {
                return HINT_SECOND.into();
            }
            return self.hint.into();
        }
        if self.picks.is_empty() { self.hint.into() } else { format!("{} · {} picked", self.hint, self.picks.len()) }
    }

    /// `constraintClick` for the relation tools.
    fn constraint_click(&mut self, cx: &mut ToolCx, uv: P2) {
        let Some(pick) = pick_ref(cx.s, cx.vp, uv, PickOpts { refs: true, ..Default::default() }) else { return };
        let fixed = fixed_ids(cx.s);
        if self.key == "c_fix" && fixed.contains(&pick.e) {
            cx.out.push(warn("Part edges and the origin are fixed already."));
            return;
        }
        self.picks.push(pick);
        let all = all_ents(cx.s);
        match make_constraint(self.key, &self.picks, &all) {
            Outcome::Pending => {
                cx.out.push(SketchEffect::Hint(self.hint_now()));
            }
            Outcome::Message(m) => {
                self.picks.clear();
                cx.out.push(warn(m));
            }
            Outcome::Constraint(c) => {
                self.picks.clear();
                let refs = [&c.a, &c.b, &c.c];
                if refs.into_iter().flatten().all(|r| fixed.contains(&r.e)) {
                    cx.out.push(warn("Pick at least one shape of the sketch: part edges and the origin don't move."));
                } else {
                    let fx = add_constraint(cx.s, c);
                    cx.out.extend(fx);
                }
            }
        }
    }

    /// `dimClick`.
    fn dim_click(&mut self, cx: &mut ToolCx, uv: P2) {
        let o = PickOpts { refs: true, sides: true, axes: self.cand.is_none(), points: true };
        let pick = pick_ref(cx.s, cx.vp, uv, o);
        let same = pick.as_ref().is_some_and(|p| self.picks.iter().any(|q| q.e == p.e && q.k == p.k && q.side == p.side));
        if let Some(p) = pick.filter(|_| !same && self.picks.len() < 2) {
            self.picks.push(p);
            let all = all_ents(cx.s);
            match infer_dimension(&self.picks, &all, Some(uv), &fixed_ids(cx.s)) {
                Outcome::Message(m) => {
                    cx.out.push(warn(m));
                    self.picks.clear();
                    self.cand = None;
                }
                Outcome::Pending => {
                    self.cand = None;
                    cx.out.push(SketchEffect::Hint(self.hint_now()));
                }
                Outcome::Constraint(c) => {
                    self.cand = Some(dims::prep_dim(cx.s, &c, uv));
                    cx.out.push(SketchEffect::Hint(self.hint_now()));
                }
            }
            return;
        }
        if let Some(c) = self.cand.take() {
            self.picks.clear();
            let fx = dims::place_dimension(cx.s, &c, self.hint.to_string());
            cx.out.extend(fx);
            return;
        }
        if self.picks.is_empty() {
            self.cand = None;
        }
    }

    /// `updatePending`: the candidate follows the pointer.
    fn update_pending(&mut self, cx: &mut ToolCx, uv: P2) {
        if self.picks.is_empty() {
            return;
        }
        let all = all_ents(cx.s);
        if let Outcome::Constraint(c) = infer_dimension(&self.picks, &all, Some(uv), &fixed_ids(cx.s)) {
            self.cand = Some(dims::prep_dim(cx.s, &c, uv));
        }
    }
}

impl SketchTool for RelTool {
    fn key(&self) -> &'static str {
        self.key
    }

    fn hint(&self, _s: &SketchSession) -> String {
        self.hint_now()
    }

    fn input(&mut self, cx: &mut ToolCx, ev: &SketchInput) -> bool {
        let handled = match ev {
            SketchInput::Down { uv, .. } => {
                // 1.x `eventUV` snaps for every tool
                let uv = super::snap::snap(cx.s, cx.vp, *uv).uv;
                if self.key == "c_dimension" {
                    self.dim_click(cx, uv);
                } else {
                    self.constraint_click(cx, uv);
                }
                true
            }
            SketchInput::Move { uv, .. } if self.key == "c_dimension" => {
                let uv = super::snap::snap(cx.s, cx.vp, *uv).uv;
                self.update_pending(cx, uv);
                false
            }
            _ => false,
        };
        self.sync();
        handled
    }

    fn cancel(&mut self, _cx: &mut ToolCx) {
        self.picks.clear();
        self.cand = None;
        self.sync();
    }

    fn picks_geometry(&self) -> bool {
        if self.key == "c_dimension" { !self.picks.is_empty() } else { true }
    }

    fn preview(&self, _s: &SketchSession) -> Vec<SketchOutline> {
        Vec::new()
    }
}

// ------------------------------------------------------------------ painting

const COL_EDGE: Color32 = Color32::from_rgb(0xf7, 0x67, 0x07);
const COL_BAD: Color32 = Color32::from_rgb(0xe0, 0x31, 0x31);
const COL_SEL: Color32 = Color32::from_rgb(0x0c, 0xa6, 0x78);
const COL_HOVER: Color32 = Color32::from_rgb(0x38, 0xc9, 0x9a);

/// `CTYPES[c.type].glyph`.
pub fn glyph(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "coincident" => "•",
        "onentity" => "⊙",
        "horizontal" => "H",
        "vertical" => "V",
        "parallel" => "∥",
        "perpendicular" => "⊥",
        "equal" => "=",
        "tangent" => "T",
        "concentric" => "◎",
        "midpoint" => "M",
        "fix" => "🔒",
        "symmetric" => "⌯",
        _ => return None,
    })
}

fn outline_shapes(
    sc: &impl Fn(P2) -> egui::Pos2,
    e: &Entity,
    stroke: Stroke,
    dashed: bool,
) -> Vec<Shape> {
    let (pts, closed) = outline(e);
    let mut p: Vec<egui::Pos2> = pts.iter().map(|q| sc(*q)).collect();
    if closed && !p.is_empty() {
        p.push(p[0]);
    }
    if dashed {
        Shape::dashed_line(&p, stroke, 5.0, 3.5)
    } else {
        vec![Shape::line(p, stroke)]
    }
}

/// The part edges (orange; red and dashed when they changed), what the tool has picked, what is
/// under the pointer, and the glyph chips of the relations. Click a chip to select its relation.
pub fn paint(ui: &egui::Ui, s: &mut SketchSession, vp: &Viewport, inch: bool) -> Vec<SketchEffect> {
    let _ = inch;
    let mut out = Vec::new();
    let t = crate::theme::tokens(ui.visuals().dark_mode);
    let painter = ui.painter().with_clip_rect(vp.rect());
    let frame = s.frame;
    let to3 = |p: P2| -> [f64; 3] { [0, 1, 2].map(|i| frame.origin[i] + frame.x[i] * p[0] + frame.y[i] * p[1]) };
    let sc = |p: P2| vp.world_to_screen(to3(p));
    let pend: Pend = dims::pending();
    let ents = all_ents(s);
    let w = World::new(&ents);
    let st = super::solve::last();
    let picking = s.tool.starts_with("c_");

    let hover = if picking {
        ui.input(|i| i.pointer.hover_pos())
            .filter(|p| vp.rect().contains(*p))
            .and_then(|p| vp.ray_to_plane(p, &s.frame))
            .and_then(|p3| {
                let o = if s.tool == "c_dimension" {
                    PickOpts { refs: true, sides: true, axes: pend.cand.is_none(), points: true }
                } else {
                    PickOpts { refs: true, ..Default::default() }
                };
                pick_ref(s, vp, s.to_uv(p3), o)
            })
    } else {
        None
    };
    let in_sel: Vec<&str> = s
        .sel_c
        .and_then(|i| s.constraints.get(i))
        .map(|c| [&c.a, &c.b, &c.c].into_iter().flatten().map(|r| r.e.as_str()).collect())
        .unwrap_or_default();
    let picked = |id: &str| pend.picks.iter().any(|p| p.e == id);

    // part edges
    let dot_col = if ui.visuals().dark_mode { Color32::from_rgb(0xe9, 0xec, 0xef) } else { Color32::from_rgb(0x49, 0x50, 0x57) };
    for r in &s.refs {
        let Some(id) = r.ent.id() else { continue };
        let hl = if picked(id) || in_sel.contains(&id) {
            Some(COL_SEL)
        } else if hover.as_ref().is_some_and(|h| h.e == id) {
            Some(COL_HOVER)
        } else {
            None
        };
        if let Entity::Point(p) = &r.ent {
            painter.circle_filled(sc([p.x, p.y]), 3.5, hl.unwrap_or(dot_col));
            continue;
        }
        let (col, dashed) = if r.missing {
            (COL_BAD, true)
        } else {
            (hl.unwrap_or(COL_EDGE.gamma_multiply(0.85)), false)
        };
        for sh in outline_shapes(&sc, &r.ent, Stroke::new(1.5, col), dashed) {
            painter.add(sh);
        }
    }
    // the origin
    if picked("_o") || hover.as_ref().is_some_and(|h| h.e == "_o") {
        painter.circle_filled(sc([0.0, 0.0]), 3.5, if picked("_o") { COL_SEL } else { COL_HOVER });
    }
    // picks and hover on the sketch's own shapes
    let mark = |p: &Ref, col: Color32| {
        let Some(e) = s.entities.iter().find(|e| e.id() == Some(p.e.as_str())) else { return };
        if let Some(q) = p.k.as_deref().and_then(|k| point(e, k)) {
            painter.circle_filled(sc(q), 4.0, col);
        } else {
            for sh in outline_shapes(&sc, e, Stroke::new(2.5, col), false) {
                painter.add(sh);
            }
        }
    };
    for p in &pend.picks {
        mark(p, COL_SEL);
    }
    if let Some(h) = &hover {
        if !pend.picks.contains(h) {
            mark(h, COL_HOVER);
        }
    }

    // glyph chips
    let passive = dims::passive(s);
    let mut clicked = None;
    for (i, c) in s.constraints.iter().enumerate() {
        if dims::is_dim(&c.kind) || (c.kind == "coincident" && s.sel_c != Some(i)) || !refs_exist(c, &w) {
            continue;
        }
        let Some(g) = glyph(&c.kind) else { continue };
        let bad = st.conflicts.contains(&i);
        let sel = s.sel_c == Some(i);
        for (si, at) in glyph_spots(c, &ents).into_iter().enumerate() {
            let centre = sc(at) + vec2(12.0 + si as f32 * 2.0, -12.0);
            let font = FontId::proportional(10.0);
            let g = dims::drawable(ui.ctx(), &font, g);
            let galley = painter.layout_no_wrap(g.clone(), font.clone(), Color32::WHITE);
            let rect = Rect::from_center_size(centre, vec2((galley.size().x + 8.0).max(15.0), 15.0));
            let (bg, fg, line) = if bad {
                (t.cg_bad_bg, t.cg_bad_text, t.cg_bad_line)
            } else if sel {
                (t.accent, Color32::WHITE, t.accent)
            } else {
                (t.cg_bg, t.cg_text, t.cg_line)
            };
            let r = ui.interact(rect, ui.scope_id().with(("sk_glyph", i, si)), if passive { Sense::hover() } else { Sense::click() });
            let name = format!("{} relation", dims::type_label(&c.kind));
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name.clone()));
            let r = r.on_hover_text(format!("{} · click to select, Delete to remove", dims::type_label(&c.kind)));
            painter.rect_filled(rect, 3.0, bg);
            painter.rect_stroke(rect, 3.0, Stroke::new(1.0, line), StrokeKind::Inside);
            let galley = painter.layout_no_wrap(g, font, fg);
            painter.galley(rect.center() - galley.size() / 2.0, galley, fg);
            if r.clicked() {
                clicked = Some(i);
            }
        }
    }
    if let Some(i) = clicked {
        s.sel_c = if s.sel_c == Some(i) { None } else { Some(i) };
        s.sel.clear();
        out.push(SketchEffect::Changed);
    }
    out
}
