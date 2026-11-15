//! The value boxes drawn on the sketch next to the edges they measure (spec 2.3), replacing the
//! fixed HUD box. The app draws `dim_boxes()` and `total_label()` at their anchors.
//!
//! Typing still goes through the keyboard path (`keys.rs`, `typed.rs`, `DrawTool.typed`): the
//! boxes only show what the tool holds. A box with a typed value is *locked* (the pointer no
//! longer moves it); the others follow the pointer. Tab / Shift+Tab move between boxes, Enter
//! places the shape, and the typed values become sketch dimensions.

use serde_json::Value;
use stl_sketch::entities::{Entity, P2};
use stl_sketch::solver::{infer_dimension, Constraint, Outcome, Ref};

use super::draw::DrawTool;
use super::{typed, SketchSession};

/// One value box: what it measures, the text shown, whether a typed value holds it, where it sits
/// (sketch coordinates) and the direction of the edge it measures.
#[derive(Clone, Debug, PartialEq)]
pub struct DimBox {
    pub label: &'static str,
    pub value: String,
    pub locked: bool,
    pub anchor: P2,
    pub along: P2,
    /// Typing goes into this box now.
    pub focused: bool,
}

/// What a tool measures at this stage: the box, before the typed text is looked at.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub label: &'static str,
    /// The typed field behind the box (0 = first, 1 = second); `None`: it can't be typed.
    pub field: Option<u8>,
    /// The value as the pointer has it now.
    pub live: String,
    pub anchor: P2,
    pub along: P2,
}

impl Row {
    pub fn new(label: &'static str, field: Option<u8>, live: String, anchor: P2, along: P2) -> Row {
        Row { label, field, live, anchor, along }
    }
}

pub(super) fn mid(a: P2, b: P2) -> P2 {
    [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
}

pub(super) fn unit(a: P2) -> P2 {
    let l = a[0].hypot(a[1]);
    if l > 1e-12 { [a[0] / l, a[1] / l] } else { [1.0, 0.0] }
}

impl DrawTool {
    /// The boxes while a shape is being drawn: one beside each edge it measures.
    pub fn dim_boxes(&self) -> Vec<DimBox> {
        if self.draw.is_none() {
            return Vec::new();
        }
        self.rows
            .iter()
            .map(|r| DimBox {
                label: r.label,
                value: self.shown(r),
                locked: r.field.is_some_and(|i| self.field_locked(i)),
                anchor: r.anchor,
                along: r.along,
                focused: r.field.is_some() && r.field == self.typed.focus,
            })
            .collect()
    }

    /// The row whose value box is under the screen point `pos` (the boxes sit centred on their
    /// anchors; the size is the label's, as the app draws it at 11 points).
    pub fn box_at(&self, s: &SketchSession, vp: &crate::viewport::Viewport, pos: egui::Pos2) -> Option<usize> {
        if self.draw.is_none() {
            return None;
        }
        self.rows.iter().position(|r| {
            let at = vp.world_to_screen(s.to_3(r.anchor));
            let chars = format!("{} {}", r.label, self.shown(r)).chars().count() as f32;
            // text (about 6 points a character) + the frame's margin, with a little slack
            let half = egui::vec2((chars * 6.0 + 14.0) / 2.0 + 2.0, 12.0);
            (pos.x - at.x).abs() <= half.x && (pos.y - at.y).abs() <= half.y
        })
    }

    /// A press on a value box focuses it (its text selected, so typing replaces it); the press
    /// is used up either way. A box that cannot be typed in (no field) only takes the press.
    pub fn click_box(&mut self, s: &SketchSession, vp: &crate::viewport::Viewport, pos: egui::Pos2) -> bool {
        let Some(i) = self.box_at(s, vp, pos) else { return false };
        if let Some(f) = self.rows[i].field {
            self.typed.focus = Some(f);
            self.typed.replace = true;
        }
        true
    }

    /// The total size next to the cursor: a line's length, a rectangle's "W × H", a circle's
    /// diameter.
    pub fn total_label(&self) -> Option<String> {
        if self.draw.is_none() {
            return None;
        }
        self.total.clone()
    }
}

/// The cursor's position on the sketch plane, shown next to the cursor (Settings › Sketch).
pub fn cursor_label(p: P2, inch: bool) -> String {
    format!("{}, {}", typed::fmt_len(p[0], inch), typed::fmt_len(p[1], inch))
}

/// A typed value that was a parameter expression keeps it on its dimension ("=width/2"), so a
/// parameter change moves the sketch.
fn keep_expr(c: &mut Constraint, expr: &Option<String>) {
    if let Some(x) = expr {
        c.extra.insert("expr".into(), Value::String(x.clone()));
    }
}

/// An angle dimension between a line and the sketch's X axis, on the wedge the line was drawn in.
fn angle_spec(s: &SketchSession, id: &str) -> Option<Constraint> {
    let mut all = s.entities.clone();
    all.extend(super::snap::fixed_ents(s));
    let Some(Entity::Line(l)) = all.iter().rev().find(|e| e.id() == Some(id)) else { return None };
    let (p1, p2) = (l.p1, l.p2);
    let d = unit([p2[0] - p1[0], p2[1] - p1[1]]);
    // where the line meets the X axis (or its middle when it runs parallel to it)
    let v = if d[1].abs() > 1e-9 { [p1[0] - p1[1] * d[0] / d[1], 0.0] } else { mid(p1, p2) };
    let cursor = [v[0] + d[0] * 10.0 + 10.0, v[1] + d[1] * 10.0];
    let fixed: std::collections::BTreeSet<String> = super::snap::fixed_ents(s).iter().filter_map(|e| e.id().map(str::to_string)).collect();
    match infer_dimension(&[Ref::ent(id), Ref::ent("_h")], &all, Some(cursor), &fixed) {
        Outcome::Constraint(c) if c.kind == "angle" => Some(c),
        _ => None,
    }
}

/// A drawn line's typed length and angle become dimensions on it.
pub(super) fn add_line_dims(s: &mut SketchSession, id: &str, has: [bool; 2], exprs: &[Option<String>; 2], px: f64) {
    let before = s.constraints.len();
    if has[0] {
        typed::add_dims(s, vec![typed::spec("length", Ref::ent(id), None, None)], px);
        if s.constraints.len() > before {
            let i = s.constraints.len() - 1;
            keep_expr(&mut s.constraints[i], &exprs[0]);
        }
    }
    if has[1] {
        if let Some(spec) = angle_spec(s, id) {
            let n = s.constraints.len();
            typed::add_dims(s, vec![spec], px);
            if s.constraints.len() > n {
                keep_expr(&mut s.constraints[n], &exprs[1]);
            }
        }
    }
}

/// `addTypedDims`, with parameter expressions kept on the dimensions they made: the sizes typed
/// while drawing a shape become its driving dimensions.
#[allow(clippy::too_many_arguments)]
pub(super) fn add_typed_dims(s: &mut SketchSession, e: &Entity, has: [bool; 2], exprs: &[Option<String>; 2], slot_len: bool, slot_expr: Option<String>, px: f64) {
    let Some(id) = e.id().map(str::to_string) else { return };
    let sgn = |x: f64| if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { 1.0 };
    let (ha, hb) = (has[0], has[1]);
    // (spec, the expression it was typed as)
    let mut specs: Vec<(Constraint, Option<String>)> = Vec::new();
    match e {
        Entity::Rect(r) => {
            if ha {
                specs.push((typed::spec("hdistance", Ref::pt(&id, "c0"), Some(Ref::pt(&id, "c1")), Some(sgn(r.w))), exprs[0].clone()));
            }
            if hb {
                specs.push((typed::spec("vdistance", Ref::pt(&id, "c1"), Some(Ref::pt(&id, "c2")), Some(sgn(r.h))), exprs[1].clone()));
            }
        }
        Entity::Circle(_) if ha => specs.push((typed::spec("diameter", Ref::ent(&id), None, None), exprs[0].clone())),
        Entity::Arc(_) | Entity::Polygon(_) if ha => specs.push((typed::spec("radius", Ref::ent(&id), None, None), exprs[0].clone())),
        Entity::Slot(_) => {
            if slot_len {
                specs.push((typed::spec("length", Ref::ent(&id), None, None), slot_expr));
            }
            if ha {
                specs.push((typed::spec("diameter", Ref::ent(&id), None, None), exprs[0].clone()));
            }
        }
        _ => {}
    }
    for (spec, expr) in specs {
        let n = s.constraints.len();
        typed::add_dims(s, vec![spec], px);
        if s.constraints.len() > n {
            keep_expr(&mut s.constraints[n], &expr);
        }
    }
}
