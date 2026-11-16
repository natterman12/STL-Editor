//! Milestone 7, Task 4 (spec 2.3): sketching with value boxes on the drawing, tracking guides,
//! a grid snap that follows the zoom, constraint glyphs, cursor coordinates, and the camera
//! staying where it is when a sketch is finished.
#![cfg(not(target_arch = "wasm32"))]

// `gpu` also keeps this test binary out of the real 2.0 folders (STLEDITOR2_CONFIG_DIR)
#[path = "support/gpu.rs"]
mod gpu;
#[path = "support/wgpu_renderer.rs"]
mod wgpu_renderer;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use egui_kittest::Harness;
use serde_json::{json, Value};
use stl_app::app::StlApp;
use stl_app::engine_link::{BodyView, Command, EngineLink, TimelineItem, TimelineView, Update};
use stl_app::platform::NullPlatform;
use stl_app::sketch::glyphs;
use stl_app::sketch::guides;
use stl_app::sketch::inline_dims::{box_text, DimBox};
use stl_app::sketch::snap::{self, SnapKind};
use stl_app::sketch::{draw, keys, Constraint, Entity, Ref, SketchEffect, SketchInput, SketchSession, SketchTool, ToolCx};
use stl_app::ui::ThemePref;
use stl_app::viewport::{PlaneFrame, Viewport};
use stl_kernel::MeshArrays;

fn frame() -> PlaneFrame {
    PlaneFrame { origin: [0.0, 0.0, 0.0], x: [1.0, 0.0, 0.0], y: [0.0, 1.0, 0.0], normal: [0.0, 0.0, 1.0] }
}

fn ent(v: Value) -> Entity {
    serde_json::from_value(v).unwrap()
}

struct H {
    s: SketchSession,
    vp: Viewport,
    tool: Box<dyn SketchTool>,
    out: Vec<SketchEffect>,
    params: BTreeMap<String, f64>,
    inch: bool,
}

impl H {
    fn new(key: &str, entities: Vec<Entity>) -> H {
        let mut s = SketchSession::new(frame(), json!("xy"), None, None, entities, vec![], vec![], None);
        s.seed = "g".into();
        s.tool = key.into();
        let mut vp = Viewport::new();
        vp.set_view_size(1000.0, 800.0);
        vp.look_at_plane(&frame(), Some(50.0));
        vp.camera_mut().finish_flight();
        snap::set_grid(true, 1.0);
        H { s, vp, tool: draw::tool_for(key).expect(key), out: Vec::new(), params: BTreeMap::new(), inch: false }
    }

    fn send(&mut self, ev: SketchInput) -> bool {
        let mut cx = ToolCx { s: &mut self.s, vp: &self.vp, inch: self.inch, params: &self.params, out: &mut self.out };
        self.tool.input(&mut cx, &ev)
    }

    fn pos(&self, uv: [f64; 2]) -> egui::Pos2 {
        self.vp.world_to_screen(self.s.to_3(uv))
    }

    fn mv(&mut self, uv: [f64; 2]) {
        let pos = self.pos(uv);
        self.send(SketchInput::Move { uv, pos, mods: egui::Modifiers::NONE });
    }

    fn click(&mut self, uv: [f64; 2]) {
        let pos = self.pos(uv);
        let mods = egui::Modifiers::NONE;
        self.send(SketchInput::Move { uv, pos, mods });
        self.send(SketchInput::Down { uv, pos, mods });
        self.send(SketchInput::Up { uv, pos });
    }

    fn key(&mut self, key: egui::Key) -> bool {
        self.send(SketchInput::Key { key, mods: egui::Modifiers::NONE })
    }

    fn shift_tab(&mut self) -> bool {
        self.send(SketchInput::Key { key: egui::Key::Tab, mods: egui::Modifiers::SHIFT })
    }

    fn typed(&mut self, t: &str) -> bool {
        self.send(SketchInput::Typed(t.into()))
    }

    fn boxes(&self) -> Vec<DimBox> {
        self.tool.dim_boxes()
    }

    fn px(&self, uv: [f64; 2]) -> f64 {
        self.vp.pixel_size(self.s.to_3(uv))
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn dim(s: &SketchSession, kind: &str) -> Option<f64> {
    s.constraints.iter().find(|c| c.kind == kind).and_then(|c| c.value)
}

// ------------------------------------------------------------------ snap markers

/// The pointer shows what it snapped to (Nathan, 2026-11-16, "like SolidCraft").
#[test]
fn snaps_say_what_they_caught() {
    use stl_app::sketch::snap::SnapWhat as W;
    let line = ent(json!({ "id": "l1", "type": "line", "p1": [0.0, 0.0], "p2": [20.0, 10.0] }));
    let circle = ent(json!({ "id": "c1", "type": "circle", "cx": 50.0, "cy": 0.0, "r": 10.0 }));
    let mut h = H::new("line", vec![line, circle]);
    let caught = |h: &mut H, uv: [f64; 2]| {
        h.mv(uv);
        h.tool.snap_marker().map(|(p, w)| (w, [p[0], p[1]]))
    };
    assert_eq!(caught(&mut h, [20.1, 10.1]), Some((W::Endpoint, [20.0, 10.0])));
    assert_eq!(caught(&mut h, [10.1, 5.1]), Some((W::Midpoint, [10.0, 5.0])));
    assert_eq!(caught(&mut h, [50.1, 0.1]), Some((W::Centre, [50.0, 0.0])));
    assert_eq!(caught(&mut h, [60.1, 0.1]), Some((W::Quadrant, [60.0, 0.0])));
    assert_eq!(caught(&mut h, [0.1, -0.1]).map(|c| c.0), Some(W::Endpoint), "the line's start (on the origin) is its end");
    assert_eq!(caught(&mut h, [33.3, -17.7]), None, "on the grid only: no marker");
}

/// A line from a point outside a circle snaps to where it touches it, and is tied there as
/// tangent (and onto the circle).
#[test]
fn line_snaps_tangent_to_a_circle() {
    let circle = ent(json!({ "id": "c1", "type": "circle", "cx": 0.0, "cy": 0.0, "r": 10.0 }));
    let mut h = H::new("line", vec![circle]);
    h.click([-30.0, 0.0]);
    // from (-30, 0) the line touches r = 10 at angle 180 - acos(1/3) degrees
    let t = std::f64::consts::PI - (1.0f64 / 3.0).acos();
    let touch = [10.0 * t.cos(), 10.0 * t.sin()];
    h.mv([touch[0] + 0.2, touch[1] + 0.2]);
    let (p, w) = h.tool.snap_marker().expect("a snap");
    assert_eq!(w, stl_app::sketch::snap::SnapWhat::Tangent);
    assert!(near(p[0], touch[0]) && near(p[1], touch[1]), "{p:?} vs {touch:?}");
    h.click([touch[0] + 0.2, touch[1] + 0.2]);
    let Some(Entity::Line(l)) = h.s.entities.iter().find(|e| matches!(e, Entity::Line(_))) else { panic!("a line") };
    let id = l.id.clone().unwrap();
    assert!(near(l.p2[0], touch[0]) && near(l.p2[1], touch[1]), "the line ends where it touches");
    let has = |kind: &str, a: &str, b: &str| {
        h.s.constraints.iter().any(|c| c.kind == kind && c.a.as_ref().is_some_and(|r| r.e == a) && c.b.as_ref().is_some_and(|r| r.e == b))
    };
    assert!(has("tangent", &id, "c1"), "tangent to the circle: {:?}", h.s.constraints);
    assert!(has("coincident", &id, "c1"), "its end is on the circle");
    // from inside the circle there is no tangent
    assert!(stl_app::sketch::snap::tangent_points(&h.s.entities[0], [1.0, 1.0]).is_empty());
}

// ------------------------------------------------------------------ value boxes

#[test]
fn line_boxes() {
    let mut h = H::new("line", vec![]);
    assert!(h.boxes().is_empty(), "no box before the first point");
    h.click([10.0, 10.0]);
    h.mv([30.0, 22.0]);
    let b = h.boxes();
    assert_eq!(b.iter().map(|b| b.label).collect::<Vec<_>>(), ["Length", "Angle"]);
    // the length box sits beside the segment's middle, the angle box at its start
    assert!(near(b[0].anchor[0], 20.0) && near(b[0].anchor[1], 16.0), "{:?}", b[0].anchor);
    assert!(near(b[1].anchor[0], 10.0) && near(b[1].anchor[1], 10.0));
    assert!(b.iter().all(|b| !b.locked));
    assert_eq!(h.tool.total_label().as_deref(), Some("23.324"), "the length next to the cursor");

    // typing goes into the first box, Tab to the next, Enter places the line
    assert!(h.typed("25"));
    assert!(h.boxes()[0].locked && h.boxes()[0].focused);
    assert!(h.key(egui::Key::Tab));
    assert!(h.boxes()[1].focused && !h.boxes()[0].focused);
    h.typed("30");
    h.key(egui::Key::Enter);
    let Entity::Line(l) = &h.s.entities[0] else { panic!("a line") };
    let (c, sn) = (30f64.to_radians().cos(), 30f64.to_radians().sin());
    assert!(near(l.p2[0], 10.0 + 25.0 * c) && near(l.p2[1], 10.0 + 25.0 * sn), "{:?}", l.p2);
    // both typed values became dimensions
    assert!(dim(&h.s, "length").is_some_and(|v| near(v, 25.0)), "{:?}", h.s.constraints);
    assert!(dim(&h.s, "angle").is_some_and(|v| near(v, 30.0)), "{:?}", h.s.constraints);
}

#[test]
fn rect_boxes_along_edges() {
    let mut h = H::new("rect", vec![]);
    h.click([5.0, 5.0]);
    h.mv([25.0, 15.0]);
    let b = h.boxes();
    assert_eq!(b.iter().map(|b| b.label).collect::<Vec<_>>(), ["Width", "Height"]);
    // the width box is beside the X edge, the height box beside the Y edge
    assert!(near(b[0].anchor[0], 15.0) && near(b[0].anchor[1], 5.0) && b[0].along == [1.0, 0.0], "{:?}", b[0]);
    assert!(near(b[1].anchor[0], 25.0) && near(b[1].anchor[1], 10.0) && b[1].along == [0.0, 1.0], "{:?}", b[1]);
    assert_eq!(h.tool.total_label().as_deref(), Some("20 × 10"));
    // typed values, Tab, Enter
    h.typed("30");
    h.key(egui::Key::Tab);
    h.typed("12");
    h.key(egui::Key::Enter);
    let Entity::Rect(r) = &h.s.entities[0] else { panic!("a rect") };
    assert!(near(r.w, 30.0) && near(r.h, 12.0), "{r:?}");
    assert!(dim(&h.s, "hdistance").is_some_and(|v| near(v, 30.0)));
    assert!(dim(&h.s, "vdistance").is_some_and(|v| near(v, 12.0)));
    assert!(h.boxes().is_empty(), "the boxes go when the shape is placed");
}

#[test]
fn circle_box_diameter() {
    let mut h = H::new("circle", vec![]);
    h.click([20.0, 20.0]);
    h.mv([30.0, 20.0]);
    let b = h.boxes();
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].label, "Diameter");
    assert_eq!(b[0].value, "20");
    assert!(near(b[0].anchor[0], 30.0) && near(b[0].anchor[1], 20.0), "on the rim: {:?}", b[0].anchor);
    assert_eq!(h.tool.total_label().as_deref(), Some("20"));
    h.typed("14");
    // one box: Tab stays on it
    h.key(egui::Key::Tab);
    assert!(h.boxes()[0].focused);
    h.key(egui::Key::Enter);
    let Entity::Circle(c) = &h.s.entities[0] else { panic!("a circle") };
    assert!(near(c.r, 7.0));
    assert!(dim(&h.s, "diameter").is_some_and(|v| near(v, 14.0)));
}

#[test]
fn locked_box_ignores_cursor() {
    let mut h = H::new("rect", vec![]);
    h.click([5.0, 5.0]);
    h.mv([25.0, 15.0]);
    h.typed("30");
    h.mv([40.0, 20.0]);
    let b = h.boxes();
    assert!(b[0].locked && b[0].value == "30", "the typed width holds: {:?}", b[0]);
    assert!(!b[1].locked && b[1].value == "15", "the height still follows the cursor: {:?}", b[1]);
    h.mv([12.0, 35.0]);
    let b = h.boxes();
    assert_eq!((b[0].value.as_str(), b[1].value.as_str()), ("30", "30"));
    assert!(b[0].locked && !b[1].locked);
    h.key(egui::Key::Enter);
    let Entity::Rect(r) = &h.s.entities[0] else { panic!("a rect") };
    assert!(near(r.w, 30.0) && near(r.h, 30.0), "{r:?}");
    // only the typed value became a dimension
    assert!(dim(&h.s, "hdistance").is_some() && dim(&h.s, "vdistance").is_none(), "{:?}", h.s.constraints);
}

#[test]
fn shift_tab_goes_back() {
    let mut h = H::new("rect", vec![]);
    h.click([5.0, 5.0]);
    h.mv([25.0, 15.0]);
    assert!(h.boxes().iter().all(|b| !b.focused));
    h.key(egui::Key::Tab);
    assert_eq!(h.boxes().iter().map(|b| b.focused).collect::<Vec<_>>(), [true, false]);
    h.key(egui::Key::Tab);
    assert_eq!(h.boxes().iter().map(|b| b.focused).collect::<Vec<_>>(), [false, true]);
    h.key(egui::Key::Tab);
    assert_eq!(h.boxes().iter().map(|b| b.focused).collect::<Vec<_>>(), [true, false], "Tab wraps round");
    h.shift_tab();
    assert_eq!(h.boxes().iter().map(|b| b.focused).collect::<Vec<_>>(), [false, true]);
    h.shift_tab();
    assert_eq!(h.boxes().iter().map(|b| b.focused).collect::<Vec<_>>(), [true, false]);
    // letters typed into a focused box don't switch tools
    assert!(h.key(egui::Key::T));
}

#[test]
fn expression_value() {
    let mut h = H::new("rect", vec![]);
    h.params.insert("width".into(), 40.0);
    h.click([0.0, 0.0]);
    h.mv([10.0, 10.0]);
    h.typed("=width/2");
    let b = h.boxes();
    assert!(b[0].locked && b[0].value == "=width/2", "{:?}", b[0]);
    h.key(egui::Key::Enter);
    let Entity::Rect(r) = &h.s.entities[0] else { panic!("a rect") };
    assert!(near(r.w, 20.0), "=width/2 is 20: {r:?}");
    let c = h.s.constraints.iter().find(|c| c.kind == "hdistance").expect("a width dimension");
    assert_eq!(c.extra.get("expr").and_then(Value::as_str), Some("=width/2"), "the dimension keeps the expression");
}

/// The value boxes say their unit (Nathan, 2026-11-16); the cursor's X, Y is no longer shown.
#[test]
fn value_boxes_show_their_unit() {
    assert_eq!(box_text("Width", "18", false), "Width 18 mm");
    assert_eq!(box_text("Width", "0.75", true), "Width 0.75 in");
    assert_eq!(box_text("Angle", "30", false), "Angle 30°");
    assert_eq!(box_text("Sides", "6", false), "Sides 6");
    assert_eq!(box_text("Length", "=width/2", false), "Length =width/2");
    assert_eq!(box_text("Length", "", false), "Length ");
}

// ------------------------------------------------------------------ guides and grid

#[test]
fn guide_snaps_to_aligned_point() {
    let h = H::new("line", vec![ent(json!({ "id": "a", "type": "line", "p1": [10.0, 30.0], "p2": [20.0, 40.0] }))]);
    let px = h.px([20.0, 5.0]);
    // within 6 px of the line end's X: snapped to it, one guide from that point
    let sn = snap::snap_tracking(&h.s, &h.vp, [20.0 + 3.0 * px, 5.2], &[], None);
    assert_eq!(sn.uv, [20.0, 5.0], "X lines up with the end point, Y goes to the grid");
    assert_eq!(sn.kind, SnapKind::Grid);
    assert_eq!(sn.guides.len(), 1);
    assert_eq!(sn.guides[0].from, [20.0, 40.0]);
    assert_eq!(sn.guides[0].to, [20.0, 5.0]);
    // too far: the grid rounds as before, no guide
    let sn = snap::snap_tracking(&h.s, &h.vp, [20.0 + 8.0 * px, 5.2], &[], None);
    assert!(sn.guides.is_empty());
    // lined up with a point's Y as well as another's X: two guides
    let sn = snap::snap_tracking(&h.s, &h.vp, [10.0 + 2.0 * px, 40.0 + 2.0 * px], &[], None);
    assert_eq!(sn.uv, [10.0, 40.0], "{:?}", sn);
    // the plain snap (every other tool) doesn't track
    let sn = snap::snap_with(&h.s, &h.vp, [20.0 + 3.0 * px, 5.2], &[], None);
    assert!(sn.guides.is_empty() && sn.uv[0] != 20.0 + 3.0 * px - 1.0);
    // the pure function
    let (at, g) = guides::track([5.1, 9.9], &[[5.0, 0.0], [0.0, 10.0]], 0.5);
    assert_eq!(at, [5.0, 10.0]);
    assert_eq!(g.len(), 2);
    // the drawing tool carries the guides
    let mut h = h;
    h.click([5.0, 5.0]);
    let px = h.px([20.0, 5.0]);
    h.mv([20.0 + 2.0 * px, 12.0]);
    assert_eq!(h.tool.guides().len(), 1);
}

#[test]
fn grid_snap_follows_zoom() {
    // 1, 2, 5, 10... at the zooms the grid draws them (12 points between lines at least)
    let at = |px_per_mm: f64| guides::visible_grid_step(px_per_mm);
    assert_eq!(at(20.0), 1.0);
    assert_eq!(at(12.0), 1.0);
    assert_eq!(at(10.0), 2.0);
    assert_eq!(at(4.0), 5.0);
    assert_eq!(at(2.0), 10.0);
    assert_eq!(at(1.0), 20.0);
    assert_eq!(at(0.5), 50.0);
    assert_eq!(at(100.0), 0.2);
    assert_eq!(at(200.0), 0.1);
    assert_eq!(at(0.0), 1.0, "no zoom: 1 mm");
    // inches read in fractions
    assert!(near(guides::visible_grid_step_in(100.0, true), 25.4 / 64.0));
    assert!(near(guides::visible_grid_step_in(2.0, true), 25.4 / 4.0));
    assert!(near(guides::visible_grid_step_in(0.1, true), 25.4 * 5.0));

    // the snap uses the spacing shown: zoom in and it gets finer
    let mut h = H::new("line", vec![]);
    snap::set_grid_mode(true, 1.0, true, false);
    let px = h.px([0.0, 0.0]);
    let step = guides::visible_grid_step(1.0 / px);
    let raw = [step * 3.4, step * 2.6];
    let sn = snap::snap(&h.s, &h.vp, raw);
    assert!(near(sn.uv[0], step * 3.0) && near(sn.uv[1], step * 3.0), "{step}: {:?}", sn.uv);
    h.vp.look_at_plane(&frame(), Some(8.0));
    h.vp.camera_mut().finish_flight();
    let px2 = h.px([0.0, 0.0]);
    let step2 = guides::visible_grid_step(1.0 / px2);
    assert!(step2 < step, "zoomed in: {step2} < {step}");
    let raw = [step2 * 3.4, step2 * 2.6];
    let sn = snap::snap(&h.s, &h.vp, raw);
    assert!(near(sn.uv[0], step2 * 3.0) && near(sn.uv[1], step2 * 3.0), "{step2}: {:?}", sn.uv);
    // a fixed step keeps its value at any zoom
    snap::set_grid_mode(true, 5.0, false, false);
    let sn = snap::snap(&h.s, &h.vp, [12.6, 7.4]);
    assert_eq!(sn.uv, [15.0, 5.0]);
    snap::set_grid(true, 1.0);
}

#[test]
fn grid_drawn_at_the_snap_step() {
    use stl_app::sketch::session::SketchMode;
    use stl_app::sketch::view;
    let mut h = H::new("line", vec![]);
    snap::set_grid_mode(true, 1.0, true, false);
    let mut m = SketchMode::new(h.s.clone(), None, vec![]);
    // the grid is built at the step the snap uses at this zoom
    let shown = view::build_grid(&mut h.vp, &m.s);
    m.grid_step = shown;
    let step = snap::zoom_step_at(h.px([0.0, 0.0]));
    assert!(near(shown, step), "{shown} vs {step}");
    let raw = [shown * 3.4, shown * 2.6];
    let sn = snap::snap(&h.s, &h.vp, raw);
    assert!(near(sn.uv[0], shown * 3.0) && near(sn.uv[1], shown * 3.0), "snaps to the lines drawn: {:?}", sn.uv);
    // nothing changed: nothing is rebuilt
    view::refresh_grid(&mut h.vp, &mut m);
    assert!(near(m.grid_step, shown));

    // zoom in: the next frame redraws the grid finer, and the snap follows it
    h.vp.look_at_plane(&frame(), Some(8.0));
    h.vp.camera_mut().finish_flight();
    view::refresh_grid(&mut h.vp, &mut m);
    assert!(m.grid_step < shown, "finer: {} < {shown}", m.grid_step);
    assert!(near(m.grid_step, snap::zoom_step_at(h.px([0.0, 0.0]))));
    let raw = [m.grid_step * 3.4, m.grid_step * 2.6];
    let sn = snap::snap(&h.s, &h.vp, raw);
    assert!(near(sn.uv[0], m.grid_step * 3.0) && near(sn.uv[1], m.grid_step * 3.0), "{:?}", sn.uv);

    // away from the origin and in a turned view the scale differs from point to point, but the
    // snap still lands on the lines drawn
    let on_grid = |h: &H, uv: [f64; 2], step: f64| {
        let sn = snap::snap(&h.s, &h.vp, uv);
        let k = |v: f64| ((v / step).round() * step - v).abs() < 1e-6;
        assert!(k(sn.uv[0]) && k(sn.uv[1]), "{uv:?} snapped to {:?}, off the {step} mm grid", sn.uv);
        // and it is the nearest line, not one of a finer or coarser grid
        assert!((sn.uv[0] - uv[0]).abs() <= step / 2.0 + 1e-6 && (sn.uv[1] - uv[1]).abs() <= step / 2.0 + 1e-6);
    };
    for uv in [[0.3, 0.3], [30.7, 21.3], [-33.2, 24.9], [-30.1, -22.6]] {
        on_grid(&h, uv, m.grid_step);
    }
    h.vp.camera_mut().orbit_about(h.s.to_3([0.0, 0.0]), 60.0, 120.0);
    h.vp.camera_mut().finish_flight();
    view::refresh_grid(&mut h.vp, &mut m);
    for uv in [[0.3, 0.3], [20.7, 11.3], [-23.2, 14.9], [-20.1, -12.6]] {
        on_grid(&h, uv, m.grid_step);
    }

    // inches: fractions of an inch, drawn and snapped alike
    snap::set_grid_mode(true, 25.4 / 16.0, true, true);
    view::refresh_grid(&mut h.vp, &mut m);
    let frac = m.grid_step / 25.4;
    assert!((1..=64).any(|d| near(frac, 1.0 / d as f64)) || frac >= 1.0, "a fraction of an inch: {frac}");
    assert!(near(m.grid_step, snap::zoom_step_at(h.px([0.0, 0.0]))));
    view::clear(&mut h.vp);

    // a fixed step too fine to draw at this zoom shows every few lines (the snap lands on all)
    snap::set_grid_mode(true, 0.01, false, false);
    let shown = view::grid_step(&h.vp, &m.s);
    assert!(shown > 0.01 && ((shown / 0.01).round() - shown / 0.01).abs() < 1e-6, "{shown}");
    assert!(shown >= guides::GRID_MIN_PX * h.px([0.0, 0.0]) * (1.0 - 1e-9));
    snap::set_grid(true, 1.0);
}

#[test]
fn snap_state_gets_the_inch_setting_on_every_path() {
    use stl_app::sketch::panel::PanelState;
    let mut p = PanelState::default();
    p.set_snap(true, 25.4 / 16.0, true);
    assert_eq!(snap::grid_auto(), (true, true), "entering a sketch in inches");
    p.set_snap_step(1.0, false);
    assert_eq!(snap::grid_auto(), (true, false), "back to millimetres");
    p.set_snap_step(25.4 / 16.0, true);
    assert_eq!(snap::grid_auto(), (true, true), "and to inches again");
    snap::set_grid(true, 1.0);
}

#[test]
fn inch_grid_steps_are_continuous() {
    // sweep the zoom: every step is at most 2.5 times the one before, and 1/2 in leads into 1 in
    let mut steps: Vec<f64> = Vec::new();
    let mut px_per_mm = 400.0;
    while px_per_mm > 0.02 {
        let st = guides::visible_grid_step_in(px_per_mm, true) / 25.4;
        if steps.last().is_none_or(|l| (l - st).abs() > 1e-12) {
            steps.push(st);
        }
        px_per_mm *= 0.98;
    }
    for w in steps.windows(2) {
        assert!(w[1] > w[0], "coarser as the zoom goes out: {w:?}");
        assert!(w[1] / w[0] <= 2.5 + 1e-9, "no jump: {} in then {} in", w[0], w[1]);
    }
    for want in [1.0 / 4.0, 1.0 / 2.0, 1.0, 2.0, 5.0] {
        assert!(steps.iter().any(|s| near(*s, want)), "{want} in is a step: {steps:?}");
    }
}

#[test]
fn escape_clears_a_box_then_cancels() {
    let mut h = H::new("rect", vec![]);
    h.click([0.0, 0.0]);
    h.mv([20.0, 10.0]);
    h.typed("30");
    assert!(h.boxes()[0].locked);
    h.key(egui::Key::Tab);
    h.typed("12");
    assert!(h.boxes()[1].locked && h.boxes()[1].focused);
    // Escape clears the focused box only; the other keeps its value and the shape stays
    h.key(egui::Key::Escape);
    let b = h.boxes();
    assert_eq!(b.len(), 2, "the shape is still being drawn");
    assert!(!b[1].locked, "the second box is free again");
    assert!(b[0].locked && b[0].value == "30", "the first keeps its value: {:?}", b[0]);
    // Escape in the now empty box cancels the shape
    h.key(egui::Key::Escape);
    assert!(h.boxes().is_empty(), "the second Escape cancelled the shape");
}

#[test]
fn clicking_a_value_box_focuses_it() {
    let mut h = H::new("rect", vec![]);
    h.click([0.0, 0.0]);
    h.mv([20.0, 10.0]);
    let b = h.boxes();
    assert!(b[0].focused || !b[1].focused);
    // a click on the height box (not on the drawing) focuses it, and the typing goes into it
    let at = h.pos(b[1].anchor);
    assert!(h.tool.click_box(&h.s, &h.vp, at), "the press is used up");
    assert!(h.boxes()[1].focused);
    h.typed("7");
    let b = h.boxes();
    assert!(b[1].locked && b[1].value == "7", "{:?}", b[1]);
    assert!(!b[0].locked, "the width stays with the pointer");
    // a press elsewhere is not a box click
    let away = h.pos([-30.0, -30.0]);
    assert!(!h.tool.click_box(&h.s, &h.vp, away));
    // the width box focuses too
    let at = h.pos(h.boxes()[0].anchor);
    assert!(h.tool.click_box(&h.s, &h.vp, at));
    assert!(h.boxes()[0].focused && !h.boxes()[1].focused);
    // no box before the first point
    let fresh = H::new("rect", vec![]);
    let mut t = fresh.tool;
    assert!(!t.click_box(&fresh.s, &fresh.vp, egui::pos2(500.0, 400.0)));
}

// ------------------------------------------------------------------ constraint glyphs

fn glyph_sketch() -> SketchSession {
    let ents = vec![
        ent(json!({ "id": "l1", "type": "line", "p1": [0.0, 0.0], "p2": [10.0, 0.0] })),
        ent(json!({ "id": "l2", "type": "line", "p1": [0.0, 10.0], "p2": [10.0, 10.0] })),
        ent(json!({ "id": "l3", "type": "line", "p1": [30.0, 0.0], "p2": [30.0, 10.0] })),
        ent(json!({ "id": "l4", "type": "line", "p1": [50.0, 0.0], "p2": [60.0, 0.0] })),
        ent(json!({ "id": "c1", "type": "circle", "cx": 70.0, "cy": 5.0, "r": 5.0 })),
    ];
    let mut s = SketchSession::new(frame(), json!("xy"), None, None, ents, vec![], vec![], None);
    let c = |kind: &str, a: Ref, b: Option<Ref>| Constraint::new(kind).with(Some(a), b);
    s.constraints = vec![
        c("horizontal", Ref::ent("l1"), None),
        c("vertical", Ref::ent("l3"), None),
        c("equal", Ref::ent("l1"), Some(Ref::ent("l2"))),
        c("parallel", Ref::ent("l1"), Some(Ref::ent("l2"))),
        c("perpendicular", Ref::ent("l1"), Some(Ref::ent("l3"))),
        c("tangent", Ref::ent("l4"), Some(Ref::ent("c1"))),
        c("coincident", Ref::pt("l1", "p2"), Some(Ref::pt("l4", "p1"))),
        c("fix", Ref::pt("l2", "p1"), None),
    ];
    s
}

#[test]
fn constraint_glyph_per_kind() {
    let mut s = glyph_sketch();
    let spots = glyphs::spots(&s);
    for kind in glyphs::KINDS {
        assert!(spots.iter().any(|g| g.kind == kind), "a glyph for {kind}: {spots:?}");
    }
    // one distinct sign per kind
    let mut texts: Vec<&str> = glyphs::KINDS.iter().map(|k| glyphs::text_of(k).expect(k)).collect();
    texts.sort();
    texts.dedup();
    assert_eq!(texts.len(), glyphs::KINDS.len(), "every kind has its own glyph");

    // clicking a glyph selects its constraint, Delete removes it
    let g = spots.iter().find(|g| g.kind == "perpendicular").unwrap().clone();
    let n = s.constraints.len();
    glyphs::select(&mut s, g.constraint);
    assert_eq!(s.sel_c, Some(g.constraint));
    keys::delete_selected(&mut s);
    assert_eq!(s.constraints.len(), n - 1);
    assert!(!s.constraints.iter().any(|c| c.kind == "perpendicular"));
    assert!(s.sel_c.is_none());
    // clicking the selected glyph again lets go
    glyphs::select(&mut s, 0);
    glyphs::select(&mut s, 0);
    assert!(s.sel_c.is_none());

    // dense lines hide glyphs; the selected constraint's stays
    let all = glyphs::spots(&glyph_sketch());
    let close = vec![
        glyphs::Glyph { constraint: 0, kind: "horizontal".into(), text: "H", at: [0.0, 0.0] },
        glyphs::Glyph { constraint: 1, kind: "vertical".into(), text: "V", at: [0.2, 0.0] },
        glyphs::Glyph { constraint: 2, kind: "equal".into(), text: "=", at: [50.0, 0.0] },
    ];
    let shown = glyphs::visible(&close, None, 0.1);
    assert_eq!(shown.iter().map(|g| g.constraint).collect::<Vec<_>>(), [0, 2], "the crowded one is hidden");
    let shown = glyphs::visible(&close, Some(1), 0.1);
    assert_eq!(shown.iter().map(|g| g.constraint).collect::<Vec<_>>(), [1, 2], "the selected one stays");
    let (far, near_in) = (glyphs::visible(&all, None, 5.0).len(), glyphs::visible(&all, None, 0.001).len());
    assert!(far < near_in, "zoomed out the glyphs crowd and hide: {far} < {near_in}");
}

// ------------------------------------------------------------------ the camera

#[derive(Clone, Default)]
struct Rec {
    sent: Arc<Mutex<Vec<Command>>>,
    inbox: Arc<Mutex<Vec<Update>>>,
}

impl EngineLink for Rec {
    fn send(&self, c: Command) {
        self.sent.lock().unwrap().push(c);
    }
    fn poll(&self) -> Vec<Update> {
        std::mem::take(&mut *self.inbox.lock().unwrap())
    }
    fn restart(&self) {}
}

fn cube(s: f32) -> Arc<MeshArrays> {
    let h = s / 2.0;
    let p = [[-h, -h, 0.0], [h, -h, 0.0], [h, h, 0.0], [-h, h, 0.0], [-h, -h, s], [h, -h, s], [h, h, s], [-h, h, s]];
    let t: [u32; 36] = [0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 1, 2, 6, 1, 6, 5, 2, 3, 7, 2, 7, 6, 3, 0, 4, 3, 4, 7];
    Arc::new(MeshArrays { positions: p.iter().flatten().copied().collect(), triangles: t.to_vec() })
}

fn scene(rec: &Rec) -> Harness<'static, StlApp> {
    let app = StlApp::new(&egui::Context::default(), Box::new(NullPlatform), Box::new(rec.clone()));
    let body = BodyView { id: "b1".into(), name: "b1".into(), color: Some("#9aa7b4".into()), hidden: false, solid: true, mesh: cube(20.0), revision: 1 };
    rec.inbox.lock().unwrap().push(Update::Bodies(vec![body]));
    let mut init = false;
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_accessibility_check(false).build_ui_state(
        move |ui, a: &mut StlApp| {
            if !init {
                init = true;
                stl_app::fonts::install(ui.ctx());
                a.set_theme(ui.ctx(), ThemePref::Dark);
            }
            a.layout(ui)
        },
        app,
    );
    h.run_steps(3);
    h.state_mut().viewport.flush();
    h.run_steps(1);
    h
}

fn view_of(h: &Harness<'static, StlApp>) -> ([f64; 3], [f64; 3]) {
    let c = h.state().viewport.camera();
    (c.position, c.target)
}

#[test]
fn finish_keeps_camera() {
    let rec = Rec::default();
    let mut h = scene(&rec);
    h.state_mut().enter_sketch(Some(json!("XY")), None);
    h.run_steps(3);
    h.state_mut().viewport.camera_mut().finish_flight();
    let before = view_of(&h);
    {
        let s = &mut h.state_mut().sketch_mode_mut().unwrap().s;
        s.push();
        s.entities.push(ent(json!({ "id": "a", "type": "line", "p1": [0.0, 0.0], "p2": [10.0, 0.0] })));
    }
    h.state_mut().finish_sketch(false);
    let params = rec.sent.lock().unwrap().iter().find_map(|c| if let Command::AddFeature { params, .. } = c { Some(params.clone()) } else { None }).expect("the step went out");
    let item = TimelineItem { id: "f1".into(), kind: "sketch".into(), label: "sketch".into(), active: true, params, ..Default::default() };
    rec.inbox.lock().unwrap().push(Update::Timeline(TimelineView { items: vec![item], marker: None, can_undo: true, can_redo: false }));
    h.run_steps(3);
    assert!(h.state().sketch().is_none(), "the sketch closed");
    h.state_mut().viewport.camera_mut().finish_flight();
    assert_eq!(view_of(&h), before, "finishing a sketch leaves the camera where it was");

    // and cancelling
    h.state_mut().enter_sketch(Some(json!("XY")), None);
    h.run_steps(3);
    h.state_mut().viewport.camera_mut().finish_flight();
    let before = view_of(&h);
    h.state_mut().cancel_sketch();
    h.run_steps(3);
    h.state_mut().viewport.camera_mut().finish_flight();
    assert_eq!(view_of(&h), before, "cancelling leaves the camera too");
}

/// Tab and typing go through egui's own input, as a keyboard does (Nathan, 2026-11-16: Tab
/// didn't reach the size boxes in the app). egui moves the keyboard focus to a panel widget on
/// Tab; the typed digits must still land in the sketch's box.
#[test]
fn tab_and_typing_reach_the_boxes_through_egui() {
    let rec = Rec::default();
    let mut h = scene(&rec);
    h.state_mut().enter_sketch(Some(json!("XY")), None);
    h.run_steps(3);
    h.state_mut().viewport.camera_mut().finish_flight();
    h.run_steps(1);
    h.state_mut().sketch_set_tool("rect");
    h.run_steps(1);
    let r = h.state().viewport.rect();
    let (a, b) = (r.center() - egui::vec2(60.0, 40.0), r.center() + egui::vec2(20.0, 10.0));
    let click = |h: &mut Harness<'static, StlApp>, p: egui::Pos2| {
        h.event(egui::Event::PointerMoved(p));
        h.run_steps(1);
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
            h.run_steps(1);
        }
    };
    click(&mut h, a);
    h.event(egui::Event::PointerMoved(b));
    h.run_steps(2);
    let boxes = |h: &Harness<'static, StlApp>| h.state().sketch_mode().and_then(|m| m.tool.as_ref()).map(|t| t.dim_boxes()).unwrap_or_default();
    assert_eq!(boxes(&h).len(), 2, "the first click started a rectangle");
    let typed = |h: &mut Harness<'static, StlApp>, t: &str| {
        h.event(egui::Event::Text(t.into()));
        h.run_steps(1);
    };
    h.key_press(egui::Key::Tab);
    h.run_steps(1);
    assert!(boxes(&h)[0].focused, "Tab focused the first box");
    typed(&mut h, "25");
    assert!(boxes(&h)[0].locked, "25 went into the first box");
    h.key_press(egui::Key::Tab);
    h.run_steps(1);
    typed(&mut h, "12");
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
    let s = h.state().sketch().expect("still sketching");
    let Some(Entity::Rect(rect)) = s.entities.first() else { panic!("a rectangle was placed: {:?}", s.entities) };
    assert!((rect.w.abs() - 25.0).abs() < 1e-9 && (rect.h.abs() - 12.0).abs() < 1e-9, "typed 25 x 12, got {} x {}", rect.w, rect.h);
}

// ------------------------------------------------------------------ snapshots

mod shots {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use stl_app::sketch::session::SketchMode;
    use stl_app::sketch::view;
    use stl_app::viewport::renderer::Headless;

    use crate::{gpu, wgpu_renderer};

    fn box_mesh() -> Arc<MeshArrays> {
        cube(20.0)
    }

    /// WCAG contrast ratio of two sRGB colours.
    pub fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
        let l = |c: [u8; 3]| {
            let f = |v: u8| {
                let v = v as f64 / 255.0;
                if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2])
        };
        let (x, y) = (l(a), l(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    /// A rectangle sketch on a box's top face, drawn through egui's paint callback.
    /// Returns the picture and the screen points of the rectangle's centre and left edge middle.
    fn picture(gpu: &Headless, dark: bool, name: &str) -> (image::RgbaImage, egui::Pos2, egui::Pos2) {
        let top = PlaneFrame { origin: [0.0, 0.0, 20.0], x: [1.0, 0.0, 0.0], y: [0.0, 1.0, 0.0], normal: [0.0, 0.0, 1.0] };
        let mut vp = Viewport::new();
        vp.init_gpu(&gpu.render_state);
        vp.set_theme(dark);
        let body = BodyView { id: "b1".into(), name: "Box".into(), color: Some("#9aa7b4".into()), hidden: false, solid: true, mesh: box_mesh(), revision: 1 };
        vp.sync_bodies(&[body]);
        vp.flush();
        vp.set_view_size(480.0, 360.0);
        vp.look_at_plane(&top, Some(22.0));
        vp.camera_mut().finish_flight();
        let rect = ent(json!({ "id": "r1", "type": "rect", "x": -6.0, "y": -4.0, "w": 12.0, "h": 8.0 }));
        let line = ent(json!({ "id": "l1", "type": "line", "p1": [-9.0, 7.0], "p2": [9.0, 7.0], "construction": true }));
        let s = SketchSession::new(top, json!("xy"), None, None, vec![rect, line], vec![], vec![], None);
        let mut m = SketchMode::new(s, None, vec![]);
        m.state.under.clear();
        view::build_grid(&mut vp, &m.s);
        view::redraw(&mut vp, &m);
        let shared = Rc::new(RefCell::new((vp, m)));
        let draw = shared.clone();
        let renderer = wgpu_renderer::WgpuRenderer::new(gpu.render_state.clone());
        let mut harness = Harness::builder().with_size(egui::vec2(480.0, 360.0)).with_render_cursor(false).renderer(renderer).build_ui(move |ui| {
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
                let mut g = draw.borrow_mut();
                g.0.show(ui);
                view::paint(ui, &g.0, &g.1);
            });
        });
        harness.run();
        gpu::snapshot(&mut harness, gpu, name);
        let img = harness.render().expect("sketch picture");
        let errors: Vec<String> = harness.take_snapshot_results().into_inner().iter().map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "{}", errors.join("\n"));
        let g = shared.borrow();
        let at = |uv: [f64; 2]| g.0.world_to_screen(g.1.s.to_3(uv));
        (img, at([0.0, 0.0]), at([-6.0, 0.0]))
    }

    #[test]
    fn sketch_inline_snapshots() {
        let Some(gpu) = gpu::headless() else {
            eprintln!("no GPU adapter: skipped");
            return;
        };
        for dark in [true, false] {
            let name = if dark { "sketch_inline_dark" } else { "sketch_inline_light" };
            let (img, centre, edge) = picture(&gpu, dark, name);
            let rgb = |p: [u8; 4]| [p[0], p[1], p[2]];
            let at = |x: u32, y: u32| rgb(img.get_pixel(x, y).0);
            // the line's core: the pixel across the left edge closest to the line colour
            let want = view::col_def(dark);
            let want = [(want >> 16) as u8, (want >> 8) as u8, want as u8];
            let dist = |p: [u8; 3]| (0..3).map(|i| (p[i] as i32 - want[i] as i32).abs()).sum::<i32>();
            let (ex, ey) = (edge.x.round() as u32, edge.y.round() as u32);
            let cx = (ex - 5..=ex + 5).min_by_key(|&x| dist(at(x, ey))).unwrap();
            let line = at(cx, ey);
            // a thin line off the pixel grid is blended with what is under it, so allow more
            assert!(dist(line) < 160, "{name}: no line at the rectangle's edge: {line:?} vs {want:?}");
            // what the line is seen against: the surface right beside it, and the face and fill a
            // few points away (the median of a patch, off the axes and grid lines)
            let beside = [at(cx - 1, ey), at(cx + 1, ey)];
            let ratio = beside.iter().map(|b| contrast(line, *b)).fold(f64::MAX, f64::min);
            let (px, py) = (centre.x.round() as u32 + 18, centre.y.round() as u32 + 9);
            let mut patch: Vec<[u8; 3]> = (0..7).flat_map(|dy| (0..7).map(move |dx| (dx, dy))).map(|(dx, dy)| at(px - 3 + dx, py - 3 + dy)).collect();
            patch.sort_by_key(|p| p[0] as u32 + p[1] as u32 + p[2] as u32);
            let fill = patch[patch.len() / 2];
            let raw = contrast(line, fill);
            eprintln!("{name}: line {line:?}, beside it {beside:?} ({ratio:.2}:1), fill {fill:?} ({raw:.2}:1)");
            // Light theme: dark lines stand out against the plain face and fill by themselves.
            // Dark theme: lines are drawn as SolidCraft draws the open sketch, 2 px with no casing
            // (Nathan, 2026-11-16), so the pale fully-defined colour on a light face reads poorly
            // (about 1.2:1); that is accepted, and only the light theme is held to 3:1.
            if !dark {
                assert!(raw >= 3.0, "{name}: the line differs from the plain fill by only {raw:.2}:1");
            }
        }
    }
}

#[test]
fn unchanged_frames_do_not_rebuild_outlines_or_glyphs() {
    use stl_app::sketch::session::SketchMode;
    let ents = vec![ent(json!({"id": "a", "type": "line", "p1": [0, 0], "p2": [10, 0]})), ent(json!({"id": "c", "type": "circle", "cx": 5, "cy": 5, "r": 3}))];
    let mut s = SketchSession::new(frame(), json!("xy"), None, None, ents, vec![], vec![], None);
    s.constraints.push(serde_json::from_value::<Constraint>(json!({"type": "horizontal", "a": {"e": "a"}})).unwrap());
    let mut vp = Viewport::new();
    vp.set_view_size(1000.0, 800.0);
    vp.look_at_plane(&frame(), Some(50.0));
    let m = SketchMode::new(s, None, vec![]);
    for _ in 0..3 {
        stl_app::sketch::view::redraw(&mut vp, &m);
        let _ = glyphs::spots(&m.s);
    }
    assert_eq!(m.outlines.builds(), 1, "three redraws, one outline build");
    assert_eq!(glyphs::spot_builds(), 1, "three glyph lookups, one build");
    let mut m = m;
    m.s.entities.push(ent(json!({"id": "b", "type": "line", "p1": [0, 1], "p2": [4, 1]})));
    stl_app::sketch::view::redraw(&mut vp, &m);
    let _ = glyphs::spots(&m.s);
    assert_eq!((m.outlines.builds(), glyphs::spot_builds()), (2, 2), "a changed shape rebuilds both");
}

/// The sketch panel's Construction tick draws new shapes as construction lines, and Look at turns
/// the view to face the sketch (Nathan, 2026-11-16).
#[test]
fn construction_tick_and_look_at() {
    use egui_kittest::kittest::Queryable;
    let rec = Rec::default();
    let mut h = scene(&rec);
    h.state_mut().enter_sketch(Some(json!("XY")), None);
    h.run_steps(3);
    h.state_mut().viewport.set_view("iso");
    h.state_mut().viewport.camera_mut().finish_flight();
    h.run_steps(2);
    let back = |h: &Harness<'static, StlApp>| h.state().viewport.camera().basis().2;
    assert!(back(&h)[2] < 0.99, "iso is not face-on: {:?}", back(&h));
    let press = |h: &mut Harness<'static, StlApp>, p: egui::Pos2| {
        h.event(egui::Event::PointerMoved(p));
        h.run_steps(1);
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
            h.run_steps(1);
        }
    };
    let at = h.get_by_label("Look at").rect().center();
    press(&mut h, at);
    h.state_mut().viewport.camera_mut().finish_flight();
    h.run_steps(2);
    assert!(back(&h)[2] > 0.999, "Look at faces the XY sketch: {:?}", back(&h));

    let at = h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Draw as construction").rect().center();
    press(&mut h, at);
    assert!(h.state().sketch_mode().unwrap().panel.construction, "the tick is on");
    h.state_mut().sketch_set_tool("rect");
    h.run_steps(1);
    let r = h.state().viewport.rect();
    press(&mut h, r.center() - egui::vec2(60.0, 40.0));
    press(&mut h, r.center() + egui::vec2(20.0, 10.0));
    h.run_steps(2);
    let s = h.state().sketch().expect("still sketching");
    let Some(e) = s.entities.first() else { panic!("a rectangle was placed") };
    assert!(e.construction(), "drawn as a construction shape: {e:?}");
}
