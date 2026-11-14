//! The drawing tools: line, arc (3-point), rectangle, centre rectangle, circle, polygon, slot
//! (`sketcher.js` `onClick`, `updateDraw`, `commit`, `commitTyped`, `addLinePoint`, `finishLine`,
//! `autoConstrain`). The modify tools are in `modify.rs`; [`tool_for`] returns both.

use std::cell::Cell;

use serde_json::{json, Value};
use stl_sketch::entities::{point, Circle, Entity, Line, Polygon, Rect, Slot, P2};
use stl_sketch::solver::{Constraint, Ref};

use super::typed::{self, fmt3, fmt_len, parse_expr, Typed};
use super::{geom, snap, SketchEffect, SketchInput, SketchSession, SketchTool, ToolCx};
use crate::engine_link::Level;
use crate::viewport::overlays::SketchOutline;

/// The drawing tool keys.
pub const DRAW_KEYS: [&str; 7] = ["line", "arc3", "rect", "rect_center", "circle", "polygon", "slot"];

/// Every drawing and modify tool of the sketch ribbon (text, relations, Project and Use Edge
/// come from their own modules). `sk_offset`, `sk_delete` and `construction` are one-shot
/// actions in 1.x, not tools: see [`super::modify::action`].
pub fn tool_for(key: &str) -> Option<Box<dyn SketchTool>> {
    if let Some(k) = DRAW_KEYS.iter().find(|k| **k == key) {
        return Some(Box::new(DrawTool::new(k)));
    }
    super::modify::tool_for(key)
}

/// `toolHint` for the drawing and modify tools.
pub fn hint_for(key: &str) -> &'static str {
    match key {
        "select" => "Click a shape to select it · Delete removes it · Right-drag to orbit, middle-drag to pan",
        "line" => "Click points to draw connected lines · click the first point to close · Enter or Esc to stop · type a length",
        "arc3" => "Click the start, then the end, then a point the arc passes through · or type a radius",
        "trim" => "Click the piece of a line, arc or shape to cut away (up to where it crosses something)",
        "sk_mirror" => "Select shapes first, then click the straight line to mirror them across",
        "sk_corner" => "Click a corner of a polyline to round it (2 mm radius)",
        "rect" => "Click two corners · or type width, Tab, height",
        "rect_center" => "Click the centre, then a corner · or type width, Tab, height",
        "circle" => "Click the centre, then the edge · or type a diameter",
        "polygon" => "Click the centre, then a corner · type radius, Tab, number of sides",
        "slot" => "Click both arc centres, then the width",
        _ => "",
    }
}

thread_local! {
    /// `this.polySides`: the last polygon's side count, kept for the next one.
    static POLY_SIDES: Cell<Option<f64>> = const { Cell::new(None) };
}

/// A pointer press, to tell a click (moved at most 5 points) from a drag.
pub(crate) const CLICK_SLOP: f32 = 5.0;

/// The drawing in progress (`this.draw`).
#[derive(Clone, Debug, Default)]
pub struct Draw {
    pub pts: Vec<P2>,
    pub refs: Vec<Option<Ref>>,
    pub preview: Option<Entity>,
    pub first_line: Option<String>,
    pub last_line: Option<String>,
    pub made: u32,
    pub slot_len: bool,
}

pub struct DrawTool {
    key: &'static str,
    pub draw: Option<Draw>,
    pub typed: Typed,
    pub cursor: Option<P2>,
    pub snap_ref: Option<Ref>,
    down: Option<egui::Pos2>,
    hud: Vec<(String, String)>,
}

pub(crate) fn set_id(e: &mut Entity, id: &str) {
    let v = Some(id.to_string());
    match e {
        Entity::Line(x) => x.id = v,
        Entity::Arc(x) => x.id = v,
        Entity::Circle(x) => x.id = v,
        Entity::Rect(x) => x.id = v,
        Entity::Polygon(x) => x.id = v,
        Entity::Slot(x) => x.id = v,
        Entity::Polyline(x) => x.id = v,
        Entity::Point(x) => x.id = v,
    }
}

/// An `auto: true` relation added while drawing.
pub(crate) fn auto_c(s: &mut SketchSession, kind: &str, a: Ref, b: Option<Ref>) -> Constraint {
    let mut c = Constraint::new(kind).with(Some(a), b);
    c.id = Some(Value::String(s.new_id()));
    c.extra.insert("auto".into(), json!(true));
    c
}

fn line(p1: P2, p2: P2) -> Entity {
    Entity::Line(Line { id: None, p1, p2, construction: None, extra: Default::default() })
}

fn dist(a: P2, b: P2) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn sign_or1(x: f64) -> f64 {
    if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { 1.0 }
}

impl DrawTool {
    pub fn new(key: &'static str) -> Self {
        DrawTool { key, draw: None, typed: Typed::default(), cursor: None, snap_ref: None, down: None, hud: Vec::new() }
    }

    /// The HUD rows (label, value shown) while drawing.
    pub fn hud(&self) -> &[(String, String)] {
        &self.hud
    }

    fn set_hud(&mut self, cx: &mut ToolCx, rows: &[(&str, String)]) {
        let fields = [&self.typed.a, &self.typed.b];
        self.hud = rows
            .iter()
            .enumerate()
            .map(|(i, (l, v))| {
                let t = fields[i.min(1)];
                // a field shows what was typed, else the live value (its placeholder)
                (l.to_string(), if self.typed.typed && !t.is_empty() { t.clone() } else { v.clone() })
            })
            .collect();
        let text = self.hud.iter().map(|(l, v)| format!("{l}\t{v}")).collect::<Vec<_>>().join("\n");
        cx.out.push(SketchEffect::Hud(Some(text)));
    }

    fn hide_hud(&mut self, cx: &mut ToolCx) {
        self.hud.clear();
        self.typed.focus = None;
        cx.out.push(SketchEffect::Hud(None));
    }

    fn snap_at(&mut self, cx: &mut ToolCx, raw: P2) -> P2 {
        let pts = self.draw.as_ref().map(|d| d.pts.clone()).unwrap_or_default();
        let straight = (self.key == "line").then(|| pts.last().copied()).flatten();
        let sn = snap::snap_with(cx.s, cx.vp, raw, &pts, straight);
        self.snap_ref = sn.on.clone();
        sn.uv
    }

    fn on_move(&mut self, cx: &mut ToolCx, raw: P2) {
        let uv = self.snap_at(cx, raw);
        self.cursor = Some(uv);
        if self.draw.is_some() && !self.typed.typed {
            self.update_draw(cx);
        }
    }

    /// `updateDraw`: the preview from the cursor and the typed fields.
    pub fn update_draw(&mut self, cx: &mut ToolCx) {
        let Some(d) = self.draw.as_ref() else { return };
        let inch = cx.inch;
        let f = typed::factor(inch);
        let c = self.cursor.unwrap_or(d.pts[0]);
        let b_is_len = self.key == "rect" || self.key == "rect_center";
        let (araw, braw) = (parse_expr(&self.typed.a), parse_expr(&self.typed.b));
        let a = araw * f;
        let b = if b_is_len { braw * f } else { braw };
        let (has_a, has_b) = (a.is_finite(), b.is_finite());
        let p0 = d.pts[0];
        let mut hud: Vec<(&str, String)> = Vec::new();
        let e: Entity = match self.key {
            "line" => {
                let last = *d.pts.last().unwrap_or(&p0);
                let (dx, dy) = (c[0] - last[0], c[1] - last[1]);
                let mut l = dx.hypot(dy);
                let mut ang = dy.atan2(dx).to_degrees();
                if has_b {
                    ang = b;
                }
                if has_a {
                    l = a;
                }
                let end = if has_a || has_b { [last[0] + l * ang.to_radians().cos(), last[1] + l * ang.to_radians().sin()] } else { c };
                hud.push(("Length", fmt_len(l, inch)));
                hud.push(("Angle", fmt3(ang)));
                line(last, end)
            }
            "arc3" => {
                if d.pts.len() == 1 {
                    hud.push(("Chord", fmt_len(dist(c, p0), inch)));
                    line(p0, c)
                } else {
                    let (pa, pb) = (d.pts[0], d.pts[1]);
                    let mut mid = c;
                    if has_a {
                        let (mx, my) = ((pa[0] + pb[0]) / 2.0, (pa[1] + pb[1]) / 2.0);
                        let ch = dist(pa, pb) / 2.0;
                        let r = a.abs().max(ch + 1e-6);
                        let sag = r - (r * r - ch * ch).sqrt();
                        let (nx, ny) = (-(pb[1] - pa[1]) / (2.0 * ch), (pb[0] - pa[0]) / (2.0 * ch));
                        let side = sign_or1((c[0] - mx) * nx + (c[1] - my) * ny);
                        mid = [mx + nx * sag * side, my + ny * sag * side];
                    }
                    let e = geom::arc_from3(pa, mid, pb).unwrap_or_else(|| line(pa, pb));
                    let r = if let Entity::Arc(x) = &e { x.r } else { 0.0 };
                    hud.push(("Radius", fmt_len(r, inch)));
                    e
                }
            }
            "rect" => {
                let (mut w, mut h) = (c[0] - p0[0], c[1] - p0[1]);
                if has_a {
                    w = sign_or1(w) * a.abs();
                }
                if has_b {
                    h = sign_or1(h) * b.abs();
                }
                hud.push(("Width", fmt_len(w.abs(), inch)));
                hud.push(("Height", fmt_len(h.abs(), inch)));
                Entity::Rect(Rect { id: None, x: p0[0], y: p0[1], w, h, angle: None, fillet: None, construction: None, extra: Default::default() })
            }
            "rect_center" => {
                let (mut w, mut h) = ((c[0] - p0[0]).abs() * 2.0, (c[1] - p0[1]).abs() * 2.0);
                if has_a {
                    w = a.abs();
                }
                if has_b {
                    h = b.abs();
                }
                hud.push(("Width", fmt_len(w, inch)));
                hud.push(("Height", fmt_len(h, inch)));
                Entity::Rect(Rect { id: None, x: p0[0] - w / 2.0, y: p0[1] - h / 2.0, w, h, angle: None, fillet: None, construction: None, extra: Default::default() })
            }
            "circle" => {
                let mut r = dist(c, p0);
                if has_a {
                    r = a.abs() / 2.0;
                }
                hud.push(("Diameter", fmt_len(r * 2.0, inch)));
                Entity::Circle(Circle { id: None, cx: p0[0], cy: p0[1], r, construction: None, extra: Default::default() })
            }
            "polygon" => {
                let mut r = dist(c, p0);
                let ang = (c[1] - p0[1]).atan2(c[0] - p0[0]).to_degrees();
                if has_a {
                    r = a.abs();
                }
                let n = if has_b { 3f64.max(typed::js_round(b)) } else { POLY_SIDES.with(|p| p.get()).unwrap_or(6.0) };
                hud.push(("Radius", fmt_len(r, inch)));
                hud.push(("Sides", typed::js_num(n)));
                Entity::Polygon(Polygon { id: None, cx: p0[0], cy: p0[1], r, n, angle: Some(if has_a { 90.0 } else { ang }), construction: None, extra: Default::default() })
            }
            _ => {
                // slot
                if d.pts.len() == 1 {
                    let (dx, dy) = (c[0] - p0[0], c[1] - p0[1]);
                    let mut l = dx.hypot(dy);
                    let mut ang = dy.atan2(dx).to_degrees();
                    if has_a {
                        l = a;
                    }
                    if has_b {
                        ang = b;
                    }
                    let (x2, y2) = (p0[0] + l * ang.to_radians().cos(), p0[1] + l * ang.to_radians().sin());
                    hud.push(("Length", fmt_len(l, inch)));
                    hud.push(("Angle", fmt3(ang)));
                    Entity::Slot(Slot { id: None, x1: p0[0], y1: p0[1], x2, y2, r: (l * 0.15).max(0.5), construction: None, extra: Default::default() })
                } else {
                    let (p1, p2) = (d.pts[0], d.pts[1]);
                    let seg = [p2[0] - p1[0], p2[1] - p1[1]];
                    let ls = { let l = seg[0].hypot(seg[1]); if l == 0.0 { 1.0 } else { l } };
                    let mut r = ((c[0] - p1[0]) * seg[1] - (c[1] - p1[1]) * seg[0]).abs() / ls;
                    if has_a {
                        r = a.abs() / 2.0;
                    }
                    hud.push(("Width", fmt_len(r * 2.0, inch)));
                    Entity::Slot(Slot { id: None, x1: p1[0], y1: p1[1], x2: p2[0], y2: p2[1], r: r.max(1e-3), construction: None, extra: Default::default() })
                }
            }
        };
        if let Some(d) = self.draw.as_mut() {
            d.preview = Some(e);
        }
        self.set_hud(cx, &hud);
    }

    fn preview_p2(&self) -> Option<P2> {
        match self.draw.as_ref()?.preview.as_ref()? {
            Entity::Line(l) => Some(l.p2),
            _ => None,
        }
    }

    fn on_click(&mut self, cx: &mut ToolCx, raw: P2) {
        // the click's own point (as eventUV in onClick), and for taps the move it never had
        self.on_move(cx, raw);
        let uv = self.cursor.unwrap_or(raw);
        if self.draw.is_none() {
            self.draw = Some(Draw { pts: vec![uv], refs: vec![self.snap_ref.clone()], ..Default::default() });
            self.typed = Typed::default();
            self.update_draw(cx);
            return;
        }
        let p2 = self.preview_p2();
        let snap_ref = self.snap_ref.clone();
        let read_a = self.typed.read().0;
        let d = self.draw.as_mut().unwrap();
        match self.key {
            "line" => self.add_line_point(cx, p2.unwrap_or(uv)),
            "arc3" if d.pts.len() == 1 => {
                let p = p2.unwrap_or(uv);
                if dist(p, d.pts[0]) < 1e-9 {
                    return;
                }
                d.pts.push(p);
                d.refs.push(snap_ref);
                self.typed.typed = false;
                self.typed.a.clear();
                self.update_draw(cx);
            }
            "arc3" => {
                if !matches!(d.preview, Some(Entity::Arc(_))) {
                    cx.out.push(SketchEffect::Toast(Level::Warning, "The three points are in a line.".into()));
                    return;
                }
                d.refs.push(None);
                self.commit(cx);
            }
            "slot" if d.pts.len() == 1 => {
                d.slot_len = read_a;
                let p = match &d.preview {
                    Some(Entity::Slot(s)) => [s.x2, s.y2],
                    _ => uv,
                };
                d.pts.push(p);
                d.refs.push(snap_ref);
                self.typed.typed = false;
                self.typed.clear();
                self.update_draw(cx);
            }
            _ => {
                d.refs.push(snap_ref);
                self.commit(cx);
            }
        }
    }

    /// `commitTyped`: Enter in a HUD field.
    pub fn commit_typed(&mut self, cx: &mut ToolCx) {
        if self.draw.is_none() {
            return;
        }
        self.update_draw(cx);
        let n = self.draw.as_ref().map_or(0, |d| d.pts.len());
        if self.key == "line" {
            if let Some(p) = self.preview_p2() {
                self.add_line_point(cx, p);
            }
            if self.draw.is_some() {
                self.typed.focus = Some(0);
            }
            return;
        }
        if self.key == "arc3" && n == 1 {
            if let Some(p) = self.preview_p2() {
                self.draw.as_mut().unwrap().pts.push(p);
            }
            self.typed.typed = false;
            self.typed.a.clear();
            self.typed.focus = Some(0);
            self.update_draw(cx);
            return;
        }
        if self.key == "slot" && n == 1 {
            let slot_len = self.typed.read().0;
            let d = self.draw.as_mut().unwrap();
            d.slot_len = slot_len;
            if let Some(Entity::Slot(s)) = &d.preview {
                let p = [s.x2, s.y2];
                d.pts.push(p);
            }
            self.typed.typed = false;
            self.typed.clear();
            self.typed.focus = Some(0);
            self.update_draw(cx);
            return;
        }
        self.commit(cx);
    }

    /// `commit`: the preview becomes a shape, tied to the points it was snapped to.
    pub fn commit(&mut self, cx: &mut ToolCx) {
        let Some(mut e) = self.draw.as_ref().and_then(|d| d.preview.clone()) else {
            return self.cancel_draw(cx);
        };
        let bad = match &e {
            Entity::Rect(r) => r.w.abs() < 1e-6 || r.h.abs() < 1e-6,
            Entity::Circle(c) => c.r < 1e-6,
            Entity::Polygon(p) => p.r < 1e-6,
            _ => false,
        };
        if bad {
            cx.out.push(SketchEffect::Toast(Level::Warning, "Too small: give it a size.".into()));
            return;
        }
        if let Entity::Polygon(p) = &e {
            let n = p.n;
            POLY_SIDES.with(|s| s.set(Some(n)));
        }
        cx.s.push();
        let id = cx.s.new_id();
        set_id(&mut e, &id);
        cx.s.entities.push(e.clone());
        let refs = self.draw.as_ref().map(|d| d.refs.clone()).unwrap_or_default();
        self.auto_constrain(cx, &e, &refs);
        let px = typed::plane_px(cx.s, cx.vp);
        let slot_len = self.draw.as_ref().is_some_and(|d| d.slot_len);
        typed::add_typed_dims(cx.s, &e, &self.typed, slot_len, px);
        self.draw = None;
        self.typed.typed = false;
        self.hide_hud(cx);
        cx.s.sel = [id].into_iter().collect();
        cx.s.sel_c = None;
        cx.out.push(SketchEffect::Changed);
    }

    /// `autoConstrain`: coincident relations for the points the shape was snapped to while drawing.
    /// The solve follows from `Changed`.
    fn auto_constrain(&mut self, cx: &mut ToolCx, e: &Entity, refs: &[Option<Ref>]) {
        let keys: &[Option<&str>] = match e {
            Entity::Rect(_) if self.key == "rect_center" => &[Some("c"), None],
            Entity::Rect(_) => &[Some("c0"), Some("c2")],
            Entity::Circle(_) | Entity::Polygon(_) => &[Some("c"), None],
            Entity::Slot(_) | Entity::Arc(_) => &[Some("s"), Some("e"), None],
            _ => &[],
        };
        let mut all = cx.s.entities.clone();
        all.extend(snap::fixed_ents(cx.s));
        let Some(id) = e.id().map(str::to_string) else { return };
        for (i, r) in refs.iter().enumerate() {
            let (Some(r), Some(Some(k))) = (r, keys.get(i)) else { continue };
            let Some(target_ent) = all.iter().rev().find(|x| x.id() == Some(r.e.as_str())) else { continue };
            let mut k = k.to_string();
            if matches!(e, Entity::Arc(_)) {
                // the arc may run either way: pair each pick with its nearest end
                let t = point(target_ent, r.k.as_deref().unwrap_or("")).unwrap_or([f64::NAN; 2]);
                let ds: Vec<f64> = ["s", "e"].iter().map(|kk| point(e, kk).map_or(f64::NAN, |q| dist(q, t))).collect();
                k = if ds[0] <= ds[1] { "s".into() } else { "e".into() };
            }
            let c = auto_c(cx.s, "coincident", Ref::pt(&id, &k), Some(r.clone()));
            cx.s.constraints.push(c);
        }
    }

    /// `addLinePoint`: each click adds a separate segment; clicking the chain's first point closes it.
    pub fn add_line_point(&mut self, cx: &mut ToolCx, p: P2) {
        let Some(d) = self.draw.as_ref() else { return };
        let (last, first) = (*d.pts.last().unwrap(), d.pts[0]);
        if dist(p, last) < 1e-9 {
            return;
        }
        let (last_line, first_line, ref0, npts) = (d.last_line.clone(), d.first_line.clone(), d.refs.first().cloned().flatten(), d.pts.len());
        cx.s.push();
        let id = cx.s.new_id();
        let mut e = line(last, p);
        set_id(&mut e, &id);
        cx.s.entities.push(e);
        let mut auto: Vec<(&str, Ref, Option<Ref>)> = Vec::new();
        if let Some(ll) = &last_line {
            auto.push(("coincident", Ref::pt(ll, "p2"), Some(Ref::pt(&id, "p1"))));
        } else if let Some(r0) = ref0 {
            auto.push(("coincident", Ref::pt(&id, "p1"), Some(r0)));
        }
        let closing = npts >= 2 && dist(p, first) < 1e-9;
        match (&first_line, &self.snap_ref) {
            (Some(fl), _) if closing => auto.push(("coincident", Ref::pt(&id, "p2"), Some(Ref::pt(fl, "p1")))),
            (_, Some(sr)) if sr.e != id => auto.push(("coincident", Ref::pt(&id, "p2"), Some(sr.clone()))),
            _ => {}
        }
        if (p[1] - last[1]).abs() < 1e-9 {
            auto.push(("horizontal", Ref::ent(&id), None));
        } else if (p[0] - last[0]).abs() < 1e-9 {
            auto.push(("vertical", Ref::ent(&id), None));
        }
        for (k, a, b) in auto {
            let c = auto_c(cx.s, k, a, b);
            cx.s.constraints.push(c);
        }
        // a typed length becomes a dimension on the line
        if self.typed.read().0 {
            let px = typed::plane_px(cx.s, cx.vp);
            typed::add_dims(cx.s, vec![typed::spec("length", Ref::ent(&id), None, None)], px);
        }
        let d = self.draw.as_mut().unwrap();
        if d.first_line.is_none() {
            d.first_line = Some(id.clone());
        }
        d.last_line = Some(id);
        d.pts.push(p);
        d.made += 1;
        let n = d.pts.len();
        self.typed.typed = false;
        self.typed.clear();
        cx.out.push(SketchEffect::Changed);
        if n >= 3 && dist(p, first) < 1e-9 {
            return self.finish_line(cx);
        }
        self.update_draw(cx);
    }

    /// `finishLine`: stop the chain (the segments drawn stay).
    pub fn finish_line(&mut self, cx: &mut ToolCx) {
        if self.draw.is_none() {
            return;
        }
        self.draw = None;
        self.hide_hud(cx);
        cx.out.push(SketchEffect::Changed);
    }

    /// `cancelDraw`.
    pub fn cancel_draw(&mut self, cx: &mut ToolCx) {
        self.draw = None;
        self.typed.typed = false;
        self.hide_hud(cx);
    }

    fn on_key(&mut self, cx: &mut ToolCx, key: egui::Key) -> bool {
        use egui::Key;
        let focused = self.typed.focus.is_some() && self.draw.is_some();
        match key {
            Key::Enter if focused => self.commit_typed(cx),
            Key::Tab if focused => {
                // the other field, its text selected
                self.typed.focus = Some(1 - self.typed.focus.unwrap_or(0).min(1));
                self.typed.replace = true;
            }
            Key::Escape if focused => self.cancel_draw(cx),
            Key::Backspace if focused => {
                let all = std::mem::take(&mut self.typed.replace);
                if let Some(f) = self.typed.field_mut() {
                    if all { f.clear() } else { f.pop(); }
                }
                self.typed.typed = true;
                self.update_draw(cx);
            }
            Key::Escape if self.draw.is_some() => {
                if self.key == "line" { self.finish_line(cx) } else { self.cancel_draw(cx) }
            }
            Key::Enter if self.key == "line" && self.draw.is_some() => self.finish_line(cx),
            _ => return false,
        }
        true
    }

    fn on_typed(&mut self, cx: &mut ToolCx, text: &str) -> bool {
        if self.draw.is_none() {
            return false;
        }
        if self.typed.focus.is_none() {
            // a digit, point or minus starts typing in the first field
            if !text.chars().next().is_some_and(|c| c.is_ascii_digit() || c == '.' || c == '-') {
                return false;
            }
            self.typed.a.clear();
            self.typed.focus = Some(0);
            self.typed.replace = false;
        }
        let all = std::mem::take(&mut self.typed.replace);
        if let Some(f) = self.typed.field_mut() {
            if all {
                f.clear();
            }
            f.push_str(text);
        }
        self.typed.typed = true;
        self.update_draw(cx);
        true
    }
}

impl SketchTool for DrawTool {
    fn key(&self) -> &'static str {
        self.key
    }

    fn hint(&self, _s: &SketchSession) -> String {
        hint_for(self.key).to_string()
    }

    fn input(&mut self, cx: &mut ToolCx, ev: &SketchInput) -> bool {
        match ev {
            SketchInput::Move { uv, .. } => {
                self.on_move(cx, *uv);
                self.draw.is_some()
            }
            SketchInput::Down { pos, .. } => {
                self.down = Some(*pos);
                true
            }
            SketchInput::Up { uv, pos } => {
                let Some(d) = self.down.take() else { return false };
                if d.distance(*pos) <= CLICK_SLOP {
                    self.on_click(cx, *uv);
                }
                true
            }
            SketchInput::DoubleClick { .. } => {
                if self.key == "line" && self.draw.is_some() {
                    self.finish_line(cx);
                    return true;
                }
                false
            }
            SketchInput::Key { key, .. } => self.on_key(cx, *key),
            SketchInput::Typed(t) => self.on_typed(cx, t),
        }
    }

    fn cancel(&mut self, cx: &mut ToolCx) {
        if self.draw.is_some() {
            self.cancel_draw(cx);
        }
    }

    fn picks_geometry(&self) -> bool {
        self.draw.is_some()
    }

    /// Offset is an action: its answer can come back while a drawing tool is active.
    fn sketch_op_result(&mut self, cx: &mut ToolCx, op: &str, result: &Value) {
        if super::modify::OP_KEYS.contains(&op) {
            super::modify::apply_op_result(cx, op, result);
        }
    }

    fn preview(&self, _s: &SketchSession) -> Vec<SketchOutline> {
        self.draw
            .as_ref()
            .and_then(|d| d.preview.as_ref())
            .map(|e| {
                let (pts, closed) = geom::outline(e);
                vec![SketchOutline { pts, closed, construction: false }]
            })
            .unwrap_or_default()
    }
}
