//! Constraint icons beside constrained lines (spec 2.3): one small glyph per constraint, hidden
//! when lines are dense; clicking one selects that constraint so Delete removes it.
//!
//! This module works out *which* glyphs there are and where; the chips are painted and clicked
//! in `relations::paint`, which asks [`visible`] for the ones to show.

use stl_sketch::entities::P2;
use stl_sketch::solver::{glyph_spots, refs_exist, World};

use super::SketchSession;

/// The glyphs' text (the constraint kinds a line can carry).
pub const KINDS: [&str; 8] = ["horizontal", "vertical", "equal", "parallel", "perpendicular", "tangent", "coincident", "fix"];

/// One glyph on the drawing.
#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    /// The constraint (index in `SketchSession::constraints`).
    pub constraint: usize,
    pub kind: String,
    pub text: &'static str,
    /// Where it sits (sketch coordinates; the chip is drawn a little beside it).
    pub at: P2,
}

/// The glyph of a constraint kind.
pub fn text_of(kind: &str) -> Option<&'static str> {
    super::relations::glyph(kind)
}

struct SpotsCache {
    ents: Vec<stl_sketch::entities::Entity>,
    cons: Vec<stl_sketch::solver::Constraint>,
    refs: Vec<stl_sketch::entities::Entity>,
    glyphs: Vec<Glyph>,
}

thread_local! {
    static SPOTS: std::cell::RefCell<Option<SpotsCache>> = const { std::cell::RefCell::new(None) };
    static BUILDS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// How many times [`spots`] has worked the glyphs out afresh (on this thread).
pub fn spot_builds() -> u32 {
    BUILDS.with(|b| b.get())
}

/// Every glyph the sketch's relations have (dimensions have their own labels). Kept until the
/// shapes, the relations or the part edges change, so a frame with no change does no solver work.
pub fn spots(s: &SketchSession) -> Vec<Glyph> {
    // no relation to show (a sketch of dimensions only): nothing to work out or keep
    if !s.constraints.iter().any(|c| !super::dims::is_dim(&c.kind) && text_of(&c.kind).is_some()) {
        return Vec::new();
    }
    SPOTS.with(|c| {
        let mut c = c.borrow_mut();
        if let Some(k) = c.as_ref() {
            if k.ents == s.entities && k.cons == s.constraints && k.refs.len() == s.refs.len() && k.refs.iter().zip(&s.refs).all(|(a, b)| *a == b.ent) {
                return k.glyphs.clone();
            }
        }
        BUILDS.with(|b| b.set(b.get() + 1));
        let glyphs = compute_spots(s);
        *c = Some(SpotsCache { ents: s.entities.clone(), cons: s.constraints.clone(), refs: s.refs.iter().map(|r| r.ent.clone()).collect(), glyphs: glyphs.clone() });
        glyphs
    })
}

fn compute_spots(s: &SketchSession) -> Vec<Glyph> {
    let ents = super::solve::all_ents(s);
    let w = World::new(&ents);
    let mut out = Vec::new();
    for (i, c) in s.constraints.iter().enumerate() {
        if super::dims::is_dim(&c.kind) || !refs_exist(c, &w) {
            continue;
        }
        let Some(text) = text_of(&c.kind) else { continue };
        for at in glyph_spots(c, &ents) {
            out.push(Glyph { constraint: i, kind: c.kind.clone(), text, at });
        }
    }
    out
}

/// A glyph is hidden when another one is closer than this many screen points: the lines are dense.
pub const DENSE_PX: f64 = 16.0;

/// The glyphs to show where one screen point is `px` mm: the selected constraint's always, the
/// others unless an earlier one is closer than [`DENSE_PX`].
pub fn visible(glyphs: &[Glyph], selected: Option<usize>, px: f64) -> Vec<Glyph> {
    let gap = DENSE_PX * px;
    let mut shown: Vec<Glyph> = Vec::new();
    let mut order: Vec<&Glyph> = glyphs.iter().collect();
    order.sort_by_key(|g| selected != Some(g.constraint));
    for g in order {
        let crowded = selected != Some(g.constraint) && shown.iter().any(|o| (o.at[0] - g.at[0]).hypot(o.at[1] - g.at[1]) < gap);
        if !crowded {
            shown.push(g.clone());
        }
    }
    shown.sort_by_key(|g| g.constraint);
    shown
}

/// Click a glyph: its constraint is selected (clicking the selected one again clears it), so
/// Delete removes it.
pub fn select(s: &mut SketchSession, constraint: usize) {
    s.sel_c = if s.sel_c == Some(constraint) { None } else { Some(constraint) };
    s.sel.clear();
}

thread_local! {
    static SHOWN: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

/// The sketch panel's Show › Relations: off, no relation chips are drawn.
pub fn set_shown(on: bool) {
    SHOWN.with(|c| c.set(on));
}

pub fn shown() -> bool {
    SHOWN.with(|c| c.get())
}
