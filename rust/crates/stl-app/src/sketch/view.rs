//! Sketch mode in the viewport: the plane grid, the shapes in 1.x's colours, the closed-profile
//! fill and the rubber band (`sketcher.js` `_buildPlaneGrid`, `redraw`, `_fill`, `colorOf`). They
//! go through the viewport's tagged overlays and are rebuilt only when something changed, so a
//! drag stays inside the frame.
//! Part edges, picks and relation glyphs are drawn by `relations::paint`, dimensions by
//! `dims::paint`.

use egui::Color32;

use super::geom::outline;
use crate::viewport::scene;
use super::session::SketchMode;
use super::{Entity, SketchSession};
use crate::viewport::overlays::SketchOutline;
use crate::viewport::Viewport;

/// Sketch colours (spec 2.3): near-white (dark theme) or near-black (light theme) lines while the
/// shape can still move, a different shade when it is fully defined, the selection blue for what
/// is selected, red when relations conflict.
pub const COL_UNDER: u32 = scene::SKETCH_LINE_DARK;
pub const COL_BAD: u32 = 0xe03131;
/// The selection blue.
pub const COL_SEL: u32 = scene::FACE_HL;
/// The shape being drawn: a green of 2.0's own, apart from the orange plane, grid and part edges
/// (Nathan, 2026-11-16: orange lines vanished on the orange plane).
pub const COL_PREV: u32 = 0x2db86a;
pub const COL_HOVER: u32 = 0x38c99a;
/// Sketch lines are this many screen points wide (thin, as SolidCraft's; was 2 with a 4 casing).
pub const LINE_PX: f32 = 1.5;
/// The squares at shapes' ends, corners and centres, this many points across.
pub const POINT_PX: f32 = 4.0;

const TAG_ENTS: &str = "sketch:ents";
const TAG_PREV: &str = "sketch:prev";
const TAG_GUIDE: &str = "sketch:guides";

/// The closed-profile fill: peach, faint (the plane's own alpha). The same in light and dark, and
/// for selected, under-defined, conflicting and hovered shapes (only the strokes change).
const COL_FILL: u32 = 0xf6c7a8;
const FILL_ALPHA: f32 = scene::PLANE_SKETCH_ALPHA;

fn col(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

/// The colour of a fully defined shape.
pub fn col_def(dark: bool) -> u32 {
    if dark { scene::SKETCH_CONSTRAINED_DARK } else { scene::SKETCH_CONSTRAINED_LIGHT }
}

/// The colour of a shape that can still move.
pub fn col_free(dark: bool) -> u32 {
    if dark { scene::SKETCH_LINE_DARK } else { scene::SKETCH_LINE_LIGHT }
}

fn outline_of(e: &Entity) -> SketchOutline {
    let (pts, closed) = outline(e);
    SketchOutline { pts, closed, construction: e.construction() }
}

/// Ray cast from `sketcher.js` `inside`: a point on an edge is not inside.
fn point_in_ring(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
    let mut inside = false;
    let mut j = poly.len().wrapping_sub(1);
    for i in 0..poly.len() {
        let (xi, yi) = (poly[i][0], poly[i][1]);
        let (xj, yj) = (poly[j][0], poly[j][1]);
        if ((yi > p[1]) != (yj > p[1])) && p[0] < (xj - xi) * (p[1] - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn nest_depth(shapes: &[Vec<[f64; 2]>], i: usize) -> usize {
    let p = shapes[i][0];
    shapes.iter().enumerate().filter(|(j, s)| *j != i && point_in_ring(p, s)).count()
}

/// `_fill`: one face per closed non-construction outline at an even nesting depth, with the
/// outlines one level deeper as holes. Separate lines that merely meet are not filled (that is
/// `profile::faces`, which extrude uses, not this).
fn fill_faces(entities: &[Entity]) -> Vec<Vec<Vec<[f64; 2]>>> {
    let shapes: Vec<Vec<[f64; 2]>> = entities
        .iter()
        .filter(|e| !e.construction())
        .filter_map(|e| {
            let (pts, closed) = outline(e);
            (closed && pts.len() >= 3).then_some(pts)
        })
        .collect();
    let mut faces = Vec::new();
    for i in 0..shapes.len() {
        let depth = nest_depth(&shapes, i);
        if depth % 2 == 1 {
            continue;
        }
        let mut rings = vec![shapes[i].clone()];
        for j in 0..shapes.len() {
            if j == i {
                continue;
            }
            let p = shapes[j][0];
            if point_in_ring(p, &shapes[i]) && nest_depth(&shapes, j) == depth + 1 {
                rings.push(shapes[j].clone());
            }
        }
        faces.push(rings);
    }
    faces
}

/// `_buildPlaneGrid`: lines every `step`, every fifth one stronger, the two axes, and a pale sheet
/// (the viewport's own sketch-plane grid, built once from the scene box as 1.x does).
///
/// The lines are drawn at the spacing Grid snap uses at this zoom ([`grid_step`]); returns it.
pub fn build_grid(vp: &mut Viewport, s: &SketchSession) -> f64 {
    let step = grid_step(vp, s);
    vp.set_sketch_plane_grid(Some(s.frame), step);
    super::snap::set_shown_step(Some(step));
    step
}

/// The spacing the plane grid shows at the current zoom, in mm, worked out where the plane's origin
/// is: the 1-2-5 steps that follow the zoom ([`super::snap::zoom_step_at`]), or the fixed step the
/// panel chose. Once drawn, Grid snap uses that same spacing everywhere on the plane. A fixed step too fine to draw at this zoom shows every few lines instead (the
/// snap still lands on all of them).
pub fn grid_step(vp: &Viewport, s: &SketchSession) -> f64 {
    let px = super::snap::px_at(s, vp, [0.0, 0.0]);
    let step = if super::snap::grid_auto().0 { super::snap::zoom_step_at(px) } else { super::snap::grid().1 };
    if !(px.is_finite() && px > 0.0 && step.is_finite() && step > 0.0) {
        return if step.is_finite() && step > 0.0 { step } else { 1.0 };
    }
    let min_mm = super::guides::GRID_MIN_PX * px;
    if step * (1.0 - 1e-9) >= min_mm { step } else { step * (min_mm / step).ceil() }
}

/// Draw the plane grid again when the zoom has changed the spacing it shows.
pub fn refresh_grid(vp: &mut Viewport, m: &mut super::session::SketchMode) {
    let step = grid_step(vp, &m.s);
    if (step - m.grid_step).abs() > 1e-9 * step.max(1.0) {
        m.grid_step = build_grid(vp, &m.s);
    }
}

/// Take the sketch's overlays off the screen (leaving the sketch).
pub fn clear(vp: &mut Viewport) {
    vp.set_sketch_plane_grid(None, 1.0);
    super::snap::set_shown_step(None);
    for t in [TAG_ENTS, TAG_PREV, TAG_GUIDE] {
        vp.clear_overlay(t);
    }
}

/// Which colour bucket a shape is in: 0 selected, 1 conflicting, 2 still free, 3 defined, 4 hovered.
fn bucket_of(m: &SketchMode, e: &Entity, picked: &std::collections::BTreeSet<&str>, in_sel_c: &std::collections::BTreeSet<&str>) -> Option<usize> {
    let s = &m.s;
    let id = e.id()?;
    Some(if s.sel.contains(id) || picked.contains(id) || in_sel_c.contains(id) {
        0
    } else if m.hover() == Some(id) {
        4
    } else if m.state.bad.contains(id) {
        1
    } else if !matches!(e, Entity::Polyline(_)) && m.state.under.contains(id) {
        2
    } else {
        3
    })
}

fn bucket_colours(dark: bool) -> [u32; 5] {
    [COL_SEL, COL_BAD, col_free(dark), col_def(dark), COL_HOVER]
}

/// The outlines of the sketch's shapes, worked out once and kept until a shape changes. Colours
/// (selection, hover) are looked up each frame; only the outlines (arc tessellation) are cached.
#[derive(Default)]
pub struct OutlineCache {
    inner: std::cell::RefCell<(Vec<Entity>, Vec<SketchOutline>)>,
    builds: std::cell::Cell<u32>,
}

impl OutlineCache {
    /// How many times the outlines have been rebuilt.
    pub fn builds(&self) -> u32 {
        self.builds.get()
    }

    fn sync(&self, ents: &[Entity]) {
        let mut g = self.inner.borrow_mut();
        let (kept, outlines) = &mut *g;
        if kept.len() == ents.len() && outlines.len() == ents.len() {
            // the same shapes in the same order: only the ones that moved are worked out again
            // (a drag moves one shape per frame)
            let mut changed = false;
            for (i, e) in ents.iter().enumerate() {
                if kept[i] != *e {
                    kept[i] = e.clone();
                    outlines[i] = outline_of(e);
                    changed = true;
                }
            }
            if changed {
                self.builds.set(self.builds.get() + 1);
            }
        } else {
            *outlines = ents.iter().map(outline_of).collect();
            *kept = ents.to_vec();
            self.builds.set(self.builds.get() + 1);
        }
    }

    /// The outline of each shape, in the order of `ents`.
    fn get(&self, ents: &[Entity]) -> std::cell::Ref<'_, Vec<SketchOutline>> {
        self.sync(ents);
        std::cell::Ref::map(self.inner.borrow(), |g| &g.1)
    }
}

/// The colour buckets as indices into the sketch's shapes (and so into the cached outlines).
fn buckets_of(m: &SketchMode) -> [Vec<usize>; 5] {
    let pend = super::dims::pending();
    let picked: std::collections::BTreeSet<&str> = pend.picks.iter().map(|p| p.e.as_str()).collect();
    let in_sel_c: std::collections::BTreeSet<&str> = m
        .s
        .sel_c
        .and_then(|i| m.s.constraints.get(i))
        .map(|c| [&c.a, &c.b, &c.c].into_iter().flatten().map(|r| r.e.as_str()).collect())
        .unwrap_or_default();
    let mut out: [Vec<usize>; 5] = Default::default();
    for (i, e) in m.s.entities.iter().enumerate() {
        if let Some(b) = bucket_of(m, e, &picked, &in_sel_c) {
            out[b].push(i);
        }
    }
    out
}

/// A polyline cut into dashes of `dash` and gaps of `gap` (plane millimetres, measured along it).
pub fn dashes(pts: &[[f64; 2]], closed: bool, dash: f64, gap: f64) -> Vec<Vec<[f64; 2]>> {
    let mut path: Vec<[f64; 2]> = pts.to_vec();
    if closed && !path.is_empty() {
        path.push(path[0]);
    }
    let period = dash + gap;
    let mut out: Vec<Vec<[f64; 2]>> = Vec::new();
    // no usable zoom: a solid line, not a loop that never ends
    let total: f64 = path.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum();
    if !(dash.is_finite() && gap.is_finite() && dash > 1e-9 && gap > 1e-9) || !total.is_finite() || total / period > 20_000.0 {
        return if path.len() >= 2 { vec![path] } else { out };
    }
    let mut along = 0.0;
    for w in path.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = (b[0] - a[0]).hypot(b[1] - a[1]);
        if len <= 0.0 {
            continue;
        }
        let at = |t: f64| [a[0] + (b[0] - a[0]) * t / len, a[1] + (b[1] - a[1]) * t / len];
        // the dashes that start in this piece's stretch of the whole line: [k * period, k * period + dash]
        let (lo, hi) = (along, along + len);
        let mut k = (lo / period).floor() as i64;
        while (k as f64) * period < hi {
            let (ds, de) = ((k as f64) * period, (k as f64) * period + dash);
            let (s0, e0) = (ds.max(lo), de.min(hi));
            if e0 > s0 {
                out.push(vec![at(s0 - lo), at(e0 - lo)]);
            }
            k += 1;
        }
        along += len;
    }
    out
}

/// `redraw`: the shapes by colour (selected, conflicting, still free, defined), construction
/// shapes dashed in grey, the tracking guides and the rubber band.
pub fn redraw(vp: &mut Viewport, m: &SketchMode) {
    let dark = vp.dark();
    vp.clear_overlay(TAG_ENTS);
    vp.clear_overlay(TAG_PREV);
    vp.clear_overlay(TAG_GUIDE);
    let s = &m.s;
    let faces = fill_faces(&s.entities);
    if !faces.is_empty() {
        vp.add_regions_tinted(&s.frame, &faces, TAG_ENTS, col(COL_FILL), FILL_ALPHA);
    }
    let px = super::snap::px_at(s, vp, [0.0, 0.0]);
    let (dash, gap) = (5.0 * px, 3.5 * px);
    let colours = bucket_colours(dark);
    let outlines = m.outlines.get(&s.entities);
    for (b, c) in buckets_of(m).iter().zip(colours) {
        let mut solid: Vec<SketchOutline> = Vec::new();
        let mut cons: Vec<SketchOutline> = Vec::new();
        for &i in b {
            let o = &outlines[i];
            if o.construction {
                for d in dashes(&o.pts, o.closed, dash, gap) {
                    cons.push(SketchOutline { pts: d, closed: false, construction: false });
                }
            } else {
                solid.push(o.clone());
            }
        }
        if !solid.is_empty() {
            vp.add_sketch_lines(&s.frame, &solid, col(c), 1.0, true, TAG_ENTS);
        }
        if !cons.is_empty() {
            // construction lines are grey, unless selected or hovered
            let cc = if c == colours[2] || c == colours[3] { scene::SKETCH_CONSTRUCTION } else { c };
            vp.add_sketch_lines(&s.frame, &cons, col(cc), 1.0, true, TAG_ENTS);
        }
    }
    let prev = m.preview();
    if !prev.is_empty() {
        vp.add_sketch_lines(&s.frame, &prev, col(COL_PREV), 1.0, true, TAG_PREV);
    }
    // dotted guides to the points the pointer lines up with
    let guides: Vec<SketchOutline> = m
        .guides()
        .iter()
        .flat_map(|g| dashes(&[g.from, g.to], false, 2.0 * px, 3.0 * px))
        .map(|pts| SketchOutline { pts, closed: false, construction: false })
        .collect();
    if !guides.is_empty() {
        vp.add_sketch_lines(&s.frame, &guides, col(GUIDE_COL), 1.0, true, TAG_GUIDE);
    }
}

/// The tracking guides' colour (a soft blue that reads on both themes).
pub const GUIDE_COL: u32 = 0x4b8fe0;

/// The sketch's lines, `LINE_PX` wide, painted over the thin overlay lines each frame (egui strokes
/// have a width; the viewport's overlay lines are one pixel), with small squares at their ends,
/// corners and centres. Construction shapes are dashed. The shape being drawn is green.
pub fn paint(ui: &egui::Ui, vp: &Viewport, m: &SketchMode) {
    let dark = vp.dark();
    let painter = ui.painter().with_clip_rect(vp.rect());
    let f = m.s.frame;
    let sc = |p: [f64; 2]| vp.world_to_screen([0, 1, 2].map(|i| f.origin[i] + f.x[i] * p[0] + f.y[i] * p[1]));
    let colours = bucket_colours(dark);
    let mut solid: Vec<(Vec<egui::Pos2>, Color32)> = Vec::new();
    let outlines = m.outlines.get(&m.s.entities);
    for (ents, c) in buckets_of(m).iter().zip(colours) {
        for &i in ents {
            let o = &outlines[i];
            let mut p: Vec<egui::Pos2> = o.pts.iter().map(|q| sc(*q)).collect();
            if o.closed && !p.is_empty() {
                p.push(p[0]);
            }
            if o.construction {
                let cc = if c == colours[2] || c == colours[3] { scene::SKETCH_CONSTRUCTION } else { c };
                painter.extend(egui::Shape::dashed_line(&p, egui::Stroke::new(LINE_PX * 0.75, col(cc)), 5.0, 3.5));
            } else {
                solid.push((p, col(c)));
            }
        }
    }
    for (p, c) in solid {
        painter.add(egui::Shape::line(p, egui::Stroke::new(LINE_PX, c)));
    }
    for (ents, c) in buckets_of(m).iter().zip(colours) {
        for &i in ents {
            let e = &m.s.entities[i];
            if e.construction() {
                continue;
            }
            for k in shown_point_keys(e) {
                if let Some(q) = stl_sketch::entities::point(e, &k) {
                    painter.rect_filled(egui::Rect::from_center_size(sc(q), egui::Vec2::splat(POINT_PX)), 0.0, col(c));
                }
            }
        }
    }
    for o in m.preview() {
        let mut p: Vec<egui::Pos2> = o.pts.iter().map(|q| sc(*q)).collect();
        if o.closed && !p.is_empty() {
            p.push(p[0]);
        }
        painter.add(egui::Shape::line(p, egui::Stroke::new(LINE_PX, col(COL_PREV))));
    }
    if let Some((uv, what)) = m.tool.as_ref().and_then(|t| t.snap_marker()) {
        paint_snap(&painter, sc(uv), what, dark);
    }
}

/// The snap marker's colour (the guides' blue) and size in points.
pub const SNAP_COL: u32 = GUIDE_COL;
const SNAP_PX: f32 = 9.0;

/// The marker where the pointer snapped, its shape telling the kind (square: endpoint, triangle:
/// midpoint, circle: centre, diamond: quadrant, circle and line: tangent, a cross: origin or
/// point), with its name next to it.
fn paint_snap(painter: &egui::Painter, at: egui::Pos2, what: super::snap::SnapWhat, dark: bool) {
    use super::snap::SnapWhat as W;
    let c = col(SNAP_COL);
    let st = egui::Stroke::new(1.5, c);
    let h = SNAP_PX / 2.0;
    match what {
        W::Endpoint => {
            painter.rect_stroke(egui::Rect::from_center_size(at, egui::Vec2::splat(SNAP_PX)), 0.0, st, egui::StrokeKind::Middle);
        }
        W::Midpoint => {
            let p = vec![at + egui::vec2(0.0, -h), at + egui::vec2(h, h * 0.8), at + egui::vec2(-h, h * 0.8)];
            painter.add(egui::Shape::closed_line(p, st));
        }
        W::Centre => {
            painter.circle_stroke(at, h, st);
        }
        W::Quadrant => {
            let p = vec![at + egui::vec2(0.0, -h), at + egui::vec2(h, 0.0), at + egui::vec2(0.0, h), at + egui::vec2(-h, 0.0)];
            painter.add(egui::Shape::closed_line(p, st));
        }
        W::Tangent => {
            painter.circle_stroke(at + egui::vec2(0.0, h * 0.4), h * 0.8, st);
            painter.line_segment([at + egui::vec2(-h * 1.3, -h * 0.4), at + egui::vec2(h * 1.3, -h * 0.4)], st);
        }
        W::Origin | W::Point => {
            painter.line_segment([at + egui::vec2(-h, -h), at + egui::vec2(h, h)], st);
            painter.line_segment([at + egui::vec2(-h, h), at + egui::vec2(h, -h)], st);
        }
    }
    let text = if dark { Color32::from_gray(0xe6) } else { Color32::from_gray(0x22) };
    painter.text(at + egui::vec2(h + 5.0, h + 3.0), egui::Align2::LEFT_TOP, what.label(), egui::FontId::proportional(11.0), text);
}

/// The points marked with a square: ends, corners and the centres of round shapes (not a line's
/// middle or a rectangle's centre).
fn shown_point_keys(e: &Entity) -> Vec<String> {
    let round = matches!(e, Entity::Arc(_) | Entity::Circle(_) | Entity::Polygon(_));
    stl_sketch::entities::point_keys(e).into_iter().filter(|k| k != "m" && (k != "c" || round)).collect()
}
