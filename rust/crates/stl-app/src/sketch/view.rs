//! Sketch mode in the viewport: the plane grid, the shapes in 1.x's colours, the closed-profile
//! fill and the rubber band (`sketcher.js` `_buildPlaneGrid`, `redraw`, `_fill`, `colorOf`). They
//! go through the viewport's tagged overlays and are rebuilt only when something changed, so a
//! drag stays inside the frame.
//! Part edges, picks and relation glyphs are drawn by `relations::paint`, dimensions by
//! `dims::paint`.

use egui::Color32;

use super::geom::outline;
use super::session::SketchMode;
use super::{Entity, SketchSession};
use crate::viewport::overlays::SketchOutline;
use crate::viewport::Viewport;

/// Sketch colours as in SOLIDWORKS: dark when fully defined, blue while it can still move, red
/// when relations conflict.
pub const COL_UNDER: u32 = 0x1c6fd6;
pub const COL_BAD: u32 = 0xe03131;
pub const COL_SEL: u32 = 0x0ca678;
pub const COL_PREV: u32 = 0xe8590c;
pub const COL_HOVER: u32 = 0x38c99a;

const TAG_ENTS: &str = "sketch:ents";
const TAG_PREV: &str = "sketch:prev";

/// `_fill`: peach, opacity 0.38. The same in light and dark, and for selected, under-defined,
/// conflicting and hovered shapes (only the strokes change).
const COL_FILL: u32 = 0xf6c7a8;
const FILL_ALPHA: f32 = 0.38;

fn col(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

/// `colDef()`: the colour of a fully defined shape.
pub fn col_def(dark: bool) -> u32 {
    if dark { 0xe9ecef } else { 0x1d2433 }
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
pub fn build_grid(vp: &mut Viewport, s: &SketchSession) {
    vp.set_sketch_plane_grid(Some(s.frame));
}

/// Take the sketch's overlays off the screen (leaving the sketch).
pub fn clear(vp: &mut Viewport) {
    vp.set_sketch_plane_grid(None);
    for t in [TAG_ENTS, TAG_PREV] {
        vp.clear_overlay(t);
    }
}

/// `redraw`: the shapes by colour (selected, conflicting, still free, defined) and the rubber band.
pub fn redraw(vp: &mut Viewport, m: &SketchMode) {
    let dark = vp.dark();
    vp.clear_overlay(TAG_ENTS);
    vp.clear_overlay(TAG_PREV);
    let s = &m.s;
    let faces = fill_faces(&s.entities);
    if !faces.is_empty() {
        vp.add_regions_tinted(&s.frame, &faces, TAG_ENTS, col(COL_FILL), FILL_ALPHA);
    }
    let pend = super::dims::pending();
    let picked: std::collections::BTreeSet<&str> = pend.picks.iter().map(|p| p.e.as_str()).collect();
    let in_sel_c: std::collections::BTreeSet<&str> = s
        .sel_c
        .and_then(|i| s.constraints.get(i))
        .map(|c| [&c.a, &c.b, &c.c].into_iter().flatten().map(|r| r.e.as_str()).collect())
        .unwrap_or_default();
    let hover = m.hover();
    let mut buckets: [Vec<SketchOutline>; 5] = Default::default();
    for e in &s.entities {
        let Some(id) = e.id() else { continue };
        let b = if s.sel.contains(id) || picked.contains(id) || in_sel_c.contains(id) {
            0
        } else if hover == Some(id) {
            4
        } else if m.state.bad.contains(id) {
            1
        } else if !matches!(e, Entity::Polyline(_)) && m.state.under.contains(id) {
            2
        } else {
            3
        };
        buckets[b].push(outline_of(e));
    }
    let colours = [col(COL_SEL), col(COL_BAD), col(COL_UNDER), col(col_def(dark)), col(COL_HOVER)];
    for (b, c) in buckets.iter().zip(colours) {
        if !b.is_empty() {
            vp.add_sketch_lines(&s.frame, b, c, 1.0, true, TAG_ENTS);
        }
    }
    let prev = m.preview();
    if !prev.is_empty() {
        vp.add_sketch_lines(&s.frame, &prev, col(COL_PREV), 1.0, true, TAG_PREV);
    }
}
