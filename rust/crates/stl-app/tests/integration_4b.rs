//! M4b, Task 7: the tools, the gizmo, Sculpt, Support Fins, coverage and Draw Walls working
//! together inside the real app (`StlApp::layout` in a kittest harness, a recording engine link).
//! Each test follows one wiring point of 1.x's `app.js` from the user's input to what the app
//! sends the engine and what it shows.
#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{Event, PointerButton, Pos2};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use serde_json::{json, Value};
use stl_app::app::StlApp;
use stl_app::engine_link::{BodyView, Command, EngineLink, TimelineItem, TimelineView, Update};
use stl_app::platform::NullPlatform;
use stl_app::supports;
use stl_app::tools::gizmo::GizmoKind;
use stl_app::ui::{ribbon, ThemePref, UiEvent};
use stl_app::viewport::camera;
use stl_app::viewport::gizmo_draw::handle_unit;
use stl_app::viewport::{PreviewMesh, PreviewPack};
use stl_kernel::MeshArrays;

// ------------------------------------------------------------------ fixtures

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

impl Rec {
    fn commands(&self) -> Vec<Command> {
        self.sent.lock().unwrap().clone()
    }
    fn push(&self, u: Update) {
        self.inbox.lock().unwrap().push(u);
    }
    fn adds(&self, kind: &str) -> Vec<Value> {
        self.commands()
            .into_iter()
            .filter_map(|c| match c {
                Command::AddFeature { kind: k, params, .. } if k == kind => Some(params),
                _ => None,
            })
            .collect()
    }
    fn previews(&self) -> Vec<(u64, String, Value)> {
        self.commands()
            .into_iter()
            .filter_map(|c| match c {
                Command::Preview { seq, kind, params, .. } => Some((seq, kind, params)),
                _ => None,
            })
            .collect()
    }
}

/// A box `s` mm on a side, standing on the bed, centred on the origin in X and Y.
fn cube(s: f32) -> Arc<MeshArrays> {
    let h = s / 2.0;
    let p = [[-h, -h, 0.0], [h, -h, 0.0], [h, h, 0.0], [-h, h, 0.0], [-h, -h, s], [h, -h, s], [h, h, s], [-h, h, s]];
    let t: [u32; 36] = [0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 1, 2, 6, 1, 6, 5, 2, 3, 7, 2, 7, 6, 3, 0, 4, 3, 4, 7];
    Arc::new(MeshArrays { positions: p.iter().flatten().copied().collect(), triangles: t.to_vec() })
}

/// A flat `n` x `n` mm sheet of 2 n² triangles at z 5, facing down.
fn plate(n: u32) -> Arc<MeshArrays> {
    let mut positions = Vec::new();
    for j in 0..=n {
        for i in 0..=n {
            positions.extend([i as f32, j as f32, 5.0]);
        }
    }
    let at = |i: u32, j: u32| j * (n + 1) + i;
    let mut triangles = Vec::new();
    for j in 0..n {
        for i in 0..n {
            triangles.extend([at(i, j), at(i + 1, j + 1), at(i + 1, j), at(i, j), at(i, j + 1), at(i + 1, j + 1)]);
        }
    }
    Arc::new(MeshArrays { positions, triangles })
}

fn body(id: &str, mesh: Arc<MeshArrays>) -> BodyView {
    BodyView { id: id.into(), name: id.into(), color: Some("#9aa7b4".into()), hidden: false, solid: true, mesh, revision: 1 }
}

fn step_item(id: &str, kind: &str, params: Value) -> TimelineItem {
    TimelineItem { id: id.into(), kind: kind.into(), label: format!("{kind} {id}"), active: true, params, ..Default::default() }
}

fn timeline(items: Vec<TimelineItem>) -> Update {
    Update::Timeline(TimelineView { items, marker: None, can_undo: false, can_redo: false })
}

/// The app on a recording link with one 20 mm cube `b1`, drawn for a few frames.
fn scene(rec: &Rec) -> Harness<'static, StlApp> {
    let app = StlApp::new(&egui::Context::default(), Box::new(NullPlatform), Box::new(rec.clone()));
    rec.push(Update::Bodies(vec![body("b1", cube(20.0))]));
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
    h
}

fn pointer(h: &mut Harness<'_, StlApp>, at: Pos2, pressed: Option<bool>) {
    h.event(Event::PointerMoved(at));
    if let Some(p) = pressed {
        h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: p, modifiers: Default::default() });
    }
    h.step();
}

fn click(h: &mut Harness<'_, StlApp>, at: Pos2) {
    pointer(h, at, None);
    pointer(h, at, Some(true));
    pointer(h, at, Some(false));
    h.run_steps(2);
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// What Support Fins' own engine says about itself (its settings list).
fn fins_info() -> Value {
    let js = repo().join("web/vendor/support-fins/engine.iife.js");
    let host = stl_platform::fins::start_fins_host(&js);
    stl_engine::features::common::fins::info(&*host).expect("the bundled Support Fins engine answers")
}

fn install_fins() {
    if !supports::is_installed() {
        supports::install(&fins_info()).expect("the schema installs");
    }
}

// ------------------------------------------------------------------ 1. Rotate, search, Edit

#[test]
fn rotate_opens_move_with_the_rings() {
    let rec = Rec::default();
    let mut h = scene(&rec);
    h.state_mut().shell.selection = vec!["b1".into()];
    // the ribbon button, a More-menu row, a dropdown row and the tool search all end in on_ribbon
    h.state_mut().on_ribbon("rotate", Pos2::ZERO);
    h.run_steps(3);
    let t = h.state().tool().expect("Move is open");
    assert_eq!(t.def.key, "move");
    assert_eq!(t.values.get("_gizmo"), Some(&json!("rotate")));
    assert_eq!(h.state().shell.active_tool.as_deref(), Some("move"));
    // the rings, not the arrows
    assert_eq!(h.state().viewport.gizmo().map(|g| g.mode), Some(stl_app::viewport::gizmo_draw::GizmoMode::Rotate));
    // and W switches to the arrows (app.js:2471)
    h.key_press(egui::Key::W);
    h.run_steps(2);
    assert_eq!(h.state().viewport.gizmo().map(|g| g.mode), Some(stl_app::viewport::gizmo_draw::GizmoMode::Translate));
    // Move itself opens with the arrows
    h.state_mut().tool_cancel();
    h.state_mut().on_ribbon("move", Pos2::ZERO);
    h.run_steps(2);
    assert_eq!(h.state().viewport.gizmo().map(|g| g.mode), Some(stl_app::viewport::gizmo_draw::GizmoMode::Translate));
}

#[test]
fn editing_a_step_rolls_back_and_cancel_puts_it_back() {
    let rec = Rec::default();
    let mut h = scene(&rec);
    rec.push(timeline(vec![step_item("f1", "primitive", json!({"shape": "box", "size": [20, 20, 20]})), step_item("f2", "hole", json!({}))]));
    h.run_steps(2);
    assert!(h.state_mut().edit_feature("f2"));
    assert!(rec.commands().iter().any(|c| matches!(c, Command::Rollback(Some(1)))), "{:?}", rec.commands());
    h.state_mut().tool_cancel();
    assert!(matches!(rec.commands().last(), Some(Command::ClearPreview) | Some(Command::Rollback(None))), "{:?}", rec.commands().last());
    assert!(rec.commands().iter().rev().take(3).any(|c| matches!(c, Command::Rollback(None))), "Cancel restores the marker");
}

// ------------------------------------------------------------------ 2. the gizmo

/// Open Move on `b1` and return where the X arrow can be grabbed and how far 15 mm is on screen.
fn move_scene(rec: &Rec) -> (Harness<'static, StlApp>, Pos2, Pos2) {
    let mut h = scene(rec);
    let mut v = serde_json::Map::new();
    v.insert("bodies".into(), json!(["b1"]));
    assert!(h.state_mut().open_tool("move", Some(v)));
    h.run_steps(3);
    let vp = &h.state().viewport;
    let c = vp.gizmo().expect("the handles show on the part").position;
    let u = handle_unit(vp.camera(), c);
    let grab = camera::add(c, [0.45 * u, 0.0, 0.0]);
    let to = camera::add(grab, [15.0, 0.0, 0.0]);
    let (g, t) = (vp.world_to_screen(grab), vp.world_to_screen(to));
    (h, g, t)
}

#[test]
fn gizmo_drop_writes_the_fields_and_adds_the_step() {
    let rec = Rec::default();
    let (mut h, grab, to) = move_scene(&rec);
    pointer(&mut h, grab, None);
    pointer(&mut h, grab, Some(true));
    assert!(h.state().tool().unwrap().dragging || h.state().viewport.camera_locked(), "a handle is held");
    pointer(&mut h, to, None);
    // the numbers show while it is dragged, nothing is sent yet
    let live = h.state().tool().unwrap().values["translate"].clone();
    assert!((live[0].as_f64().unwrap() - 15.0).abs() < 0.5, "{live}");
    assert!(rec.adds("move").is_empty());
    pointer(&mut h, to, Some(false));
    h.run_steps(2);
    let adds = rec.adds("move");
    assert_eq!(adds.len(), 1, "letting go adds the step: {:?}", rec.commands());
    assert!((adds[0]["translate"][0].as_f64().unwrap() - 15.0).abs() < 0.5, "{}", adds[0]);
    let t = h.state().tool().expect("the tool stays open");
    assert_eq!((t.ok_label.as_str(), t.cancel_label.as_str()), ("Done", "Close"));
    // the step shows up in the timeline: the tool goes on editing that one
    rec.push(timeline(vec![step_item("f9", "move", adds[0].clone())]));
    h.run_steps(2);
    let t = h.state().tool().unwrap();
    assert_eq!(t.edit_id.as_deref(), Some("f9"));
    assert!(rec.commands().iter().any(|c| matches!(c, Command::Rollback(Some(0)))));
    // the next drop edits it instead of adding another
    let vp = &h.state().viewport;
    let c = vp.gizmo().expect("handles").position;
    let u = handle_unit(vp.camera(), c);
    let grab = vp.world_to_screen(camera::add(c, [0.45 * u, 0.0, 0.0]));
    let to = vp.world_to_screen(camera::add(c, [0.45 * u + 5.0, 0.0, 0.0]));
    pointer(&mut h, grab, None);
    pointer(&mut h, grab, Some(true));
    pointer(&mut h, to, None);
    pointer(&mut h, to, Some(false));
    h.run_steps(2);
    assert_eq!(rec.adds("move").len(), 1, "no second step: {:?}", rec.commands());
    let edits: Vec<_> = rec.commands().into_iter().filter(|c| matches!(c, Command::EditFeature { id, .. } if id == "f9")).collect();
    assert_eq!(edits.len(), 1, "the drop edits the step it added: {:?}", rec.commands());
}

#[test]
fn a_late_preview_during_a_drag_is_dropped() {
    let rec = Rec::default();
    let (mut h, grab, to) = move_scene(&rec);
    // a preview of the old numbers is on its way
    h.state_mut().run_preview_now(web_time::Instant::now());
    let seq = rec.previews().last().expect("a preview was asked for").0;
    pointer(&mut h, grab, None);
    pointer(&mut h, grab, Some(true));
    pointer(&mut h, to, None);
    let moved = h.state().viewport.body_transform("b1").copied().expect("the part follows the handle");
    rec.push(Update::Preview {
        seq,
        pack: PreviewPack { meshes: vec![PreviewMesh { id: "b1".into(), mesh: cube(20.0) }], ..Default::default() },
        msg: Some("late".into()),
        error: None,
        warn: false,
        result: Value::Null,
    });
    h.run_steps(2);
    assert_eq!(h.state().viewport.preview_meshes().count(), 0, "nothing is drawn while a handle is held");
    assert_eq!(h.state().viewport.body_transform("b1").copied(), Some(moved), "the part does not snap back");
    assert!(h.state().tool().unwrap().msg.is_none());
    pointer(&mut h, to, Some(false));
}

// ------------------------------------------------------------------ 3. Sculpt

#[test]
fn a_sculpt_stroke_previews_at_once() {
    let rec = Rec::default();
    let mut h = scene(&rec);
    assert!(h.state_mut().open_tool("sculpt", None));
    h.run_steps(3);
    let before = rec.previews().len();
    let vp = &h.state().viewport;
    let a = vp.world_to_screen([-5.0, -10.0, 20.0]);
    let b = vp.world_to_screen([5.0, 0.0, 20.0]);
    pointer(&mut h, a, None);
    pointer(&mut h, a, Some(true));
    pointer(&mut h, Pos2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0), None);
    pointer(&mut h, b, None);
    pointer(&mut h, b, Some(false));
    h.run_steps(1);
    let strokes = h.state().tool().unwrap().values["strokes"].as_array().cloned().unwrap_or_default();
    assert_eq!(strokes.len(), 1, "one stroke was drawn");
    assert_eq!(h.state().tool().unwrap().values["body"], json!("b1"));
    // asked for at once, not after the 280 ms wait
    assert!(rec.previews().len() > before, "{:?}", rec.commands());
    // no other click handling: the click did not select the body
    assert!(h.state().shell.selection.is_empty());
}

// ------------------------------------------------------------------ 4. Support Fins

#[test]
fn support_fins_opens_with_its_schema_and_edits_instead_of_stacking() {
    install_fins_after_asking();
}

/// The tool waits for the engine's answer, builds its panel from it, and a second open of the same
/// part edits the step it already has.
fn install_fins_after_asking() {
    let rec = Rec::default();
    let mut h = scene(&rec);
    h.state_mut().shell.selection = vec!["b1".into()];
    assert!(h.state_mut().open_tool("supportfins", None));
    if !supports::is_installed() {
        assert!(h.state().tool().is_none(), "nothing opens before the engine has said what it can do");
        let seq = rec
            .commands()
            .iter()
            .find_map(|c| match c {
                Command::Fins { seq, ask } if ask.name() == "info" => Some(*seq),
                _ => None,
            })
            .expect("the engine was asked for Support Fins' settings");
        rec.push(Update::Fins { seq, ask: "info".into(), result: Ok(fins_info()) });
        h.run_steps(3);
    } else {
        h.run_steps(3);
    }
    install_fins();
    let t = h.state().tool().expect("Support Fins is open");
    assert_eq!(t.def.key, "supportfins");
    assert!(t.sf.is_some(), "drawn by the Support Fins form, not the generic one");
    assert!(t.credit.as_ref().is_some_and(|c| c.0.starts_with("Support Fins ")), "{:?}", t.credit);
    assert_eq!(t.values["body"], json!(["b1"]));
    assert!(supports::support_tools().iter().any(|d| d.key == "supportfins") && supports::support_tools().iter().any(|d| d.key == "supportwall"));
    h.state_mut().tool_cancel();
    // the part already has a Support Fins step: opening it again edits that one
    let params = json!({"body": "b1", "walls": [], "removed": [], "output": "join", "options": {}});
    rec.push(timeline(vec![step_item("s1", "supportfins", params)]));
    h.run_steps(2);
    assert!(h.state_mut().open_tool("supportfins", None));
    let t = h.state().tool().expect("open");
    assert_eq!(t.edit_id.as_deref(), Some("s1"), "it opened the step it has, not a second set");
    assert_eq!(t.title(), "Edit Support Fins");
    // and a double-click of the step in the timeline goes the same way
    h.state_mut().tool_cancel();
    assert!(h.state_mut().edit_feature("s1"));
    assert_eq!(h.state().tool().and_then(|t| t.edit_id.clone()).as_deref(), Some("s1"));
}

#[test]
fn support_fins_about_text_comes_from_its_engine() {
    install_fins();
    let a = supports::about(false).expect("about");
    assert_eq!(a.title, "Support Fins");
    assert!(a.paragraphs[0].starts_with("Support Fins "));
}

// ------------------------------------------------------------------ 5/6. coverage, off the UI thread

#[test]
fn coverage_reaches_the_held_card_off_the_ui_thread() {
    install_fins();
    let rec = Rec::default();
    let mut h = scene(&rec);
    // a part with a big downward face, so the check takes a while
    let mut plate = body("b1", plate(215));
    plate.revision = 2;
    rec.push(Update::Bodies(vec![plate]));
    let end = Instant::now() + Duration::from_secs(20);
    while h.state().viewport.body_mesh("b1").is_none_or(|m| m.triangles.len() != 215 * 215 * 6) {
        h.step();
        assert!(Instant::now() < end, "the plate never reached the screen");
        std::thread::sleep(Duration::from_millis(5));
    }
    h.state_mut().shell.selection = vec!["b1".into()];
    assert!(h.state_mut().open_tool("supportfins", None));
    h.run_steps(3);
    assert_eq!(h.state().tool().map(|t| t.def.key), Some("supportfins"));
    let n_faces = (215 * 215 * 2) as u32;
    let seq = rec.previews().last().map(|p| p.0).unwrap_or_else(|| {
        h.state_mut().run_preview_now(web_time::Instant::now());
        rec.previews().last().unwrap().0
    });
    // the engine's exact answer: the bottom faces need support, a fin stands beside the part
    let fin = Arc::new(MeshArrays { positions: vec![30.0, 0.0, 0.0, 31.0, 0.0, 0.0, 30.0, 0.0, 10.0], triangles: vec![0, 1, 2] });
    rec.push(Update::Preview {
        seq,
        pack: PreviewPack { meshes: vec![PreviewMesh { id: "fin1".into(), mesh: fin }], ..Default::default() },
        msg: None,
        error: None,
        warn: false,
        result: json!({"paint": {"body": "b1", "over": (0..n_faces).collect::<Vec<u32>>(), "small": []}, "walls": [], "pieces": {}, "support": {"volume": 12.5, "pieces": 1}}),
    });
    let t0 = Instant::now();
    h.state_mut().poll();
    let polled = t0.elapsed();
    let t1 = Instant::now();
    h.step();
    let first_frame = t1.elapsed();
    eprintln!("preview answer applied in {polled:?}, the next frame {first_frame:?}");
    let busy = h.state().tool().and_then(|t| t.sf.as_ref()).and_then(|s| s.cover.clone()).map(|c| c.busy);
    assert_eq!(busy, Some(true), "the check is running when the frame ends: it didn't block it");
    let mut worker = None;
    // the answer lands on a later frame
    let end = Instant::now() + Duration::from_secs(20);
    loop {
        h.step();
        worker = worker.or(h.state().coverage_worker_thread());
        let c = h.state().tool().and_then(|t| t.sf.as_ref()).and_then(|s| s.cover.clone());
        if c.as_ref().is_some_and(|c| !c.busy) {
            let c = c.unwrap();
            assert!(c.error.is_none(), "{:?}", c.error);
            assert!(c.total > 0.0, "the check measured the overhang: {c:?}");
            break;
        }
        assert!(Instant::now() < end, "the coverage check never finished");
        std::thread::sleep(Duration::from_millis(5));
    }
    let w = worker.expect("the check ran on a worker thread of its own");
    assert_ne!(w, std::thread::current().id(), "never on the UI thread");
    let budget = frame_budget();
    assert!(polled < budget, "applying the answer (and starting the check) took {polled:?}");
    assert!(first_frame < budget, "the frame after it took {first_frame:?}");
    // the plastic card has the engine's numbers
    let s = h.state().tool().unwrap().sf.as_ref().unwrap();
    assert_eq!(s.plastic.as_ref().map(|p| p.pieces), Some(1));
}

// ------------------------------------------------------------------ 7. Draw Walls

#[test]
fn draw_walls_clicks_write_the_walls() {
    install_fins();
    let rec = Rec::default();
    let mut h = scene(&rec);
    h.state_mut().shell.selection = vec!["b1".into()];
    assert!(h.state_mut().open_tool("supportwall", None));
    h.run_steps(3);
    assert_eq!(h.state().tool().map(|t| t.def.key), Some("supportwall"));
    assert_eq!(h.state().wall_drawer().shape, "line", "Line is the first shape");
    let vp = &h.state().viewport;
    let p1 = vp.world_to_screen([-6.0, -10.0, 20.0]);
    let p2 = vp.world_to_screen([6.0, 0.0, 20.0]);
    click(&mut h, p1);
    assert_eq!(h.state().wall_drawer().pending.len(), 1, "the first click starts a wall");
    assert!(h.state().tool().unwrap().values["walls"].as_array().is_none_or(|a| a.is_empty()));
    click(&mut h, p2);
    let walls = h.state().tool().unwrap().values["walls"].as_array().cloned().unwrap_or_default();
    assert_eq!(walls.len(), 1, "the second click finishes the line");
    assert_eq!(walls[0]["shape"], json!("line"));
    assert_eq!(walls[0]["points"].as_array().map(|a| a.len()), Some(2));
    assert!(h.state().wall_drawer().pending.is_empty());
    // the wall is built on the way
    h.state_mut().run_previews(web_time::Instant::now() + web_time::Duration::from_secs(2));
    assert!(rec.previews().iter().any(|p| p.1 == "supportwall" && p.2["walls"].as_array().is_some_and(|w| w.len() == 1)), "{:?}", rec.previews());
    // Ctrl+Z takes the wall back
    ctrl_z(&mut h);
    let walls = h.state().tool().unwrap().values["walls"].as_array().cloned().unwrap_or_default();
    assert!(walls.is_empty(), "Ctrl+Z took the wall back: {walls:?}");
}

// ------------------------------------------------------------------ the 41 tools in a frame

#[test]
fn opening_every_tool_stays_inside_a_frame() {
    let rec = Rec::default();
    let mut h = scene(&rec);
    rec.push(timeline(vec![step_item("f1", "primitive", json!({"shape": "box", "size": [20, 20, 20]}))]));
    h.state_mut().shell.selection = vec!["b1".into()];
    h.run_steps(2);
    let mut worst = (Duration::ZERO, "");
    let keys: Vec<&'static str> = stl_app::tools::tools().iter().map(|d| d.key).collect();
    assert_eq!(keys.len(), 41, "the 41 tools of 1.x");
    for key in keys {
        let def = stl_app::tools::tool(key).unwrap();
        h.state_mut().open_tool_def(def, None, None);
        for _ in 0..3 {
            let t0 = Instant::now();
            h.step();
            let dt = t0.elapsed();
            if dt > worst.0 {
                worst = (dt, key);
            }
        }
        h.state_mut().tool_cancel();
        h.step();
    }
    eprintln!("slowest frame while opening a tool: {:?} ({})", worst.0, worst.1);
    assert!(worst.0 < frame_budget(), "{} took {:?} to open", worst.1, worst.0);
}

#[test]
fn gizmo_kinds_follow_the_tool() {
    assert_ne!(GizmoKind::Move, GizmoKind::Pose);
}

// ------------------------------------------------------------------ picking a moved body, red paint speed

#[test]
fn a_body_moved_by_the_gizmo_is_picked_where_it_is_drawn() {
    use stl_app::viewport::pick::Hit;
    use stl_app::viewport::Viewport;
    let mut vp = Viewport::new();
    vp.set_pick_trees(false);
    vp.set_view_size(800.0, 600.0);
    vp.sync_bodies(&[body("b1", cube(20.0))]);
    vp.flush();
    vp.set_view("iso");
    vp.camera_mut().finish_flight();
    let top = [0.0, 0.0, 20.0];
    let at = vp.world_to_screen(top);
    assert!(matches!(vp.pick(at, true, false), Some(Hit::Body { .. })), "picked where it is");
    // moved 60 mm along X on screen: the old place is empty, the new one is the body
    let mut m = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [60.0, 0.0, 0.0, 1.0]];
    vp.set_body_transform("b1", Some(m));
    let moved = vp.world_to_screen([60.0, 0.0, 20.0]);
    match vp.pick(moved, true, false) {
        Some(Hit::Body { id, point, normal, .. }) => {
            assert_eq!(id, "b1");
            assert!((point[0] - 60.0).abs() < 1.0 && (point[2] - 20.0).abs() < 1e-6, "{point:?}");
            assert!(normal[2] > 0.99, "{normal:?}");
        }
        other => panic!("{other:?}"),
    }
    assert!(vp.pick(at, true, false).is_none(), "nothing is left where it was");
    // turned: a 90 degree turn about Z keeps the hit on the part
    m = [[0.0, 1.0, 0.0, 0.0], [-1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
    vp.set_body_transform("b1", Some(m));
    assert!(matches!(vp.pick(at, true, false), Some(Hit::Body { .. })));
}

#[test]
fn red_overhang_paint_fits_a_frame() {
    use stl_app::viewport::Viewport;
    let mut vp = Viewport::new();
    vp.set_pick_trees(false);
    vp.set_view_size(800.0, 600.0);
    // about the bench mesh's size: 91,000 faces, all facing down
    // a foot on the bed, so the sheet is above it (faces on the bed aren't overhangs)
    let mut sheet = plate(215);
    let m = Arc::make_mut(&mut sheet);
    let base = (m.positions.len() / 3) as u32;
    m.positions.extend([0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
    m.triangles.extend([base, base + 1, base + 2]);
    vp.sync_bodies(&[body("b1", sheet)]);
    vp.flush();
    let t0 = Instant::now();
    assert!(vp.show_overhangs_async("b1", 45.0).is_none(), "worked out on another thread");
    let ask = t0.elapsed();
    let end = Instant::now() + Duration::from_secs(30);
    let (mut worst, mut landed) = (Duration::ZERO, None);
    while landed.is_none() {
        let t = Instant::now();
        landed = vp.poll_overhangs();
        worst = worst.max(t.elapsed());
        assert!(Instant::now() < end, "the paint never landed");
        std::thread::sleep(Duration::from_millis(2));
    }
    let (_, n) = landed.unwrap();
    assert_eq!(n, 215 * 215 * 2);
    eprintln!("ask {ask:?}, landing frame {worst:?}");
    // the same ask again is answered from what was worked out
    let t = Instant::now();
    assert_eq!(vp.show_overhangs_async("b1", 45.0).map(|f| f.len()), Some(n));
    let again = t.elapsed();
    let budget = frame_budget();
    assert!(ask < budget && worst < budget && again < budget, "{ask:?} {worst:?} {again:?}");
}

// ------------------------------------------------------------------ integration gaps against 1.x

/// A worker on its own, with every update it sends kept.
fn manual_worker() -> (stl_app::engine_link::worker::Worker, Arc<Mutex<Vec<Update>>>) {
    let log = Arc::new(Mutex::new(Vec::new()));
    let l2 = log.clone();
    let sink: stl_app::engine_link::worker::Sink = Arc::new(move |u| l2.lock().unwrap().push(u));
    (stl_app::engine_link::worker::Worker::new(sink, Arc::new(std::sync::atomic::AtomicBool::new(false))), log)
}

fn last_timeline(log: &Arc<Mutex<Vec<Update>>>) -> TimelineView {
    log.lock().unwrap().iter().rev().find_map(|u| if let Update::Timeline(t) = u { Some(t.clone()) } else { None }).expect("a timeline")
}

fn last_body_ids(log: &Arc<Mutex<Vec<Update>>>) -> Vec<String> {
    log.lock().unwrap().iter().rev().find_map(|u| if let Update::Bodies(b) = u { Some(b.iter().map(|b| b.id.clone()).collect()) } else { None }).unwrap_or_default()
}

fn add(kind: &str, params: Value, index: Option<usize>, name: Option<&str>) -> Command {
    Command::AddFeature { kind: kind.into(), params, index, name: name.map(str::to_owned) }
}

fn box_cmd(w: f64) -> Command {
    add("primitive", json!({"shape": "box", "size": [w, 10.0, 10.0]}), None, None)
}

#[test]
fn a_turn_of_the_part_adds_a_move_step_named_rotate() {
    install_fins();
    let rec = Rec::default();
    let mut h = scene(&rec);
    h.state_mut().shell.selection = vec!["b1".into()];
    assert!(h.state_mut().open_tool("supportfins", None));
    h.run_steps(3);
    // a quarter turn about X (column-major 3x3)
    h.state_mut().use_pose(&json!([1, 0, 0, 0, 0, 1, 0, -1, 0]));
    let named: Vec<_> = rec
        .commands()
        .into_iter()
        .filter_map(|c| match c {
            Command::AddFeature { kind, name, .. } if kind == "move" => Some(name),
            _ => None,
        })
        .collect();
    assert_eq!(named, vec![Some("Rotate".to_string())], "{:?}", rec.commands());
}

#[test]
fn the_worker_names_the_step_and_puts_it_where_it_is_told() {
    let (mut w, log) = manual_worker();
    w.handle(box_cmd(4.0));
    let b = last_body_ids(&log)[0].clone();
    w.handle(box_cmd(5.0));
    w.handle(box_cmd(6.0));
    // before the second step, with a name: the marker stays at the end
    w.handle(add("move", json!({"bodies": [b], "translate": [1.0, 0.0, 0.0]}), Some(1), Some("Rotate")));
    let t = last_timeline(&log);
    assert_eq!(t.items.len(), 4);
    assert_eq!(t.items[1].label, "Rotate", "{:?}", t.items.iter().map(|i| &i.label).collect::<Vec<_>>());
    assert_eq!(t.items[1].kind, "move");
    assert_eq!(t.marker, None);
    // with the marker part way: the step goes before it and the marker moves one step on
    w.handle(Command::Rollback(Some(2)));
    w.handle(add("move", json!({"bodies": [b], "translate": [0.0, 1.0, 0.0]}), Some(1), None));
    let t = last_timeline(&log);
    assert_eq!(t.items[1].kind, "move");
    assert_eq!(t.marker, Some(3));
    assert_eq!(t.items.len(), 5);
}

#[test]
fn a_move_goes_before_the_supports_of_the_part() {
    let rec = Rec::default();
    let mut h = scene(&rec);
    let sf = json!({"body": "b1", "walls": [], "removed": [], "output": "join", "options": {}});
    rec.push(timeline(vec![step_item("f1", "primitive", json!({})), step_item("s1", "supportfins", sf)]));
    h.run_steps(2);
    let mut v = serde_json::Map::new();
    v.insert("bodies".into(), json!(["b1"]));
    assert!(h.state_mut().open_tool("move", Some(v.clone())));
    h.run_steps(2);
    h.state_mut().tool_ok();
    let index = rec.commands().into_iter().find_map(|c| match c {
        Command::AddFeature { kind, index, .. } if kind == "move" => Some(index),
        _ => None,
    });
    assert_eq!(index, Some(Some(1)), "before the support step: {:?}", rec.commands());
    // another part's supports don't count, and neither does a copy
    rec.push(timeline(vec![step_item("f1", "primitive", json!({})), step_item("s1", "supportfins", json!({"body": "b2"}))]));
    h.run_steps(2);
    assert!(h.state_mut().open_tool("move", Some(v)));
    h.run_steps(2);
    h.state_mut().tool_ok();
    let last = rec.commands().into_iter().rev().find_map(|c| match c {
        Command::AddFeature { kind, index, .. } if kind == "move" => Some(index),
        _ => None,
    });
    assert_eq!(last, Some(None));
}

#[test]
fn a_project_with_support_fins_steps_is_built_with_the_engine() {
    let (mut w, log) = manual_worker();
    w.handle(box_cmd(20.0));
    let b = last_body_ids(&log)[0].clone();
    w.handle(add("supportfins", json!({"body": b, "walls": [], "removed": [], "output": "join", "options": {}}), None, None));
    log.lock().unwrap().clear();
    w.handle(Command::Save);
    let bytes = log.lock().unwrap().iter().find_map(|u| if let Update::Saved { bytes } = u { Some(bytes.clone()) } else { None }).expect("saved");
    // a worker that has never started the engine opens it
    let (mut w2, log2) = manual_worker();
    w2.handle(Command::Open { name: "p.stlx".into(), bytes });
    let t = last_timeline(&log2);
    let sf = t.items.iter().find(|i| i.kind == "supportfins").expect("the support step is there");
    assert_eq!(sf.error, None, "the engine was started for it");
}

#[test]
fn overhangs_worked_out_a_slice_at_a_time_match_the_whole() {
    use stl_app::viewport::overhang::{overhang_faces, OverhangWork};
    let mut sheet = plate(120);
    let m = Arc::make_mut(&mut sheet);
    let base = (m.positions.len() / 3) as u32;
    m.positions.extend([0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
    m.triangles.extend([base, base + 1, base + 2]);
    let whole = overhang_faces(&sheet, 45.0);
    assert_eq!(whole.len(), 120 * 120 * 2);
    let mut w = OverhangWork::new(&sheet, 45.0);
    let (mut slices, mut worst) = (0, Duration::ZERO);
    let faces = loop {
        let t = Instant::now();
        let r = w.step(&sheet, Some(Duration::from_micros(300)));
        worst = worst.max(t.elapsed());
        slices += 1;
        if let Some(f) = r {
            break f;
        }
    };
    assert_eq!(faces, whole);
    assert!(slices > 1, "the work was spread over {slices} slices");
    eprintln!("{slices} slices, worst {worst:?}");
}

// ------------------------------------------------------------------ review findings

/// A frame budget that also holds in a debug build (about ten times slower than release).
fn frame_budget() -> Duration {
    if cfg!(debug_assertions) { Duration::from_millis(250) } else { Duration::from_micros(16_700) }
}

struct SharedNative(Arc<stl_app::engine_link::native::NativeLink>);
impl EngineLink for SharedNative {
    fn send(&self, c: Command) {
        self.0.send(c);
    }
    fn poll(&self) -> Vec<Update> {
        self.0.poll()
    }
    fn restart(&self) {}
}

/// The app on the real engine.
fn native_scene() -> (Harness<'static, StlApp>, Arc<stl_app::engine_link::native::NativeLink>) {
    let link = Arc::new(stl_app::engine_link::native::NativeLink::new());
    let app = StlApp::new(&egui::Context::default(), Box::new(NullPlatform), Box::new(SharedNative(link.clone())));
    let mut init = false;
    let h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_accessibility_check(false).build_ui_state(
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
    (h, link)
}

fn settle_native(h: &mut Harness<'static, StlApp>, link: &stl_app::engine_link::native::NativeLink) {
    let end = Instant::now() + Duration::from_secs(60);
    loop {
        h.run_steps(2);
        if !link.is_busy() {
            h.run_steps(2);
            if !link.is_busy() {
                return;
            }
        }
        assert!(Instant::now() < end, "the engine never went idle");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn ctrl_z(h: &mut Harness<'static, StlApp>) {
    h.key_combination_modifiers(egui::Modifiers::CTRL, &[egui::Key::Z]);
    h.run_steps(2);
}

/// OK on an edit is one transaction: Ctrl+Z brings back the steps that came after the edited one
/// (the edit session had rolled the timeline back to just before it).
#[test]
fn edit_ok_then_undo_restores_the_later_steps() {
    let (mut h, link) = native_scene();
    for len in [10, 20] {
        h.state_mut().link.send(add("primitive", json!({"shape": "box", "length": len, "width": 10, "height": 10, "plane": "XY", "offset": [0, 0, 0], "rotate": [0, 0, 0], "anchor": "base", "operation": "new", "targets": []}), None, None));
        settle_native(&mut h, &link);
    }
    let first = h.state().timeline_view.items[0].id.clone();
    assert_eq!(h.state().bodies.len(), 2);
    assert!(h.state_mut().edit_feature(&first));
    settle_native(&mut h, &link);
    assert_eq!(h.state().bodies.len(), 0, "an edit session shows the part rolled back to before the step");
    h.state_mut().tool_set_value("length", json!(12.0));
    h.state_mut().tool_ok();
    settle_native(&mut h, &link);
    let t = &h.state().timeline_view;
    assert_eq!((t.items.len(), t.marker), (2, None), "the marker is back at the end");
    assert_eq!(t.items[0].params["length"].as_f64(), Some(12.0));
    assert_eq!(h.state().bodies.len(), 2);
    ctrl_z(&mut h);
    settle_native(&mut h, &link);
    let t = &h.state().timeline_view;
    assert_eq!((t.items.len(), t.marker), (2, None), "undo lands on the timeline as the user left it");
    assert_eq!(t.items[0].params["length"].as_f64(), Some(10.0), "the edit is undone");
    assert_eq!(h.state().bodies.len(), 2, "and the steps after it are back");
}

/// Undo with a tool open closes the tool first (an edit session is put back as it was).
#[test]
fn undo_with_a_tool_open_closes_the_tool_first() {
    let rec = Rec::default();
    let mut h = scene(&rec);
    rec.push(timeline(vec![step_item("f1", "primitive", json!({"shape": "box", "size": [20, 20, 20]})), step_item("f2", "hole", json!({}))]));
    h.run_steps(2);
    assert!(h.state_mut().edit_feature("f1"));
    h.run_steps(2);
    assert!(h.state().tool().is_some());
    ctrl_z(&mut h);
    assert!(h.state().tool().is_none(), "the tool is closed");
    let cmds = rec.commands();
    let undo = cmds.iter().rposition(|c| matches!(c, Command::Undo)).expect("Undo was sent");
    let back = cmds.iter().rposition(|c| matches!(c, Command::Rollback(None))).expect("the marker was put back");
    assert!(back < undo, "the tool closed before the undo: {cmds:?}");
}

/// The engine says no to a command: the error comes before the timeline that follows it.
#[test]
fn a_failed_command_reports_its_error_before_the_timeline() {
    let (mut w, log) = manual_worker();
    w.handle(box_cmd(5.0));
    log.lock().unwrap().clear();
    w.handle(add("move", json!({"bodies": ["nothing"], "translate": [1.0, 0.0, 0.0]}), None, Some("Rotate")));
    let ups = log.lock().unwrap();
    let err = ups.iter().position(|u| matches!(u, Update::Error(_))).expect("an error");
    let tl = ups.iter().position(|u| matches!(u, Update::Timeline(_))).expect("a timeline");
    assert!(err < tl, "the app counts a command as done when the timeline shows it");
}

/// A Support Fins turn the engine refuses is not counted: the next turn starts from the pose
/// before it, and the part and rings are usable again.
#[test]
fn a_failed_turn_is_not_counted() {
    install_fins();
    let rec = Rec::default();
    let mut h = scene(&rec);
    h.state_mut().shell.selection = vec!["b1".into()];
    assert!(h.state_mut().open_tool("supportfins", None));
    h.run_steps(3);
    let quarter = json!([1, 0, 0, 0, 0, 1, 0, -1, 0]);
    h.state_mut().use_pose(&quarter);
    assert_eq!(rec.adds("move").len(), 1);
    // the engine refuses; its error, then the timeline as it was
    rec.push(Update::Error("That turn cannot be built.".into()));
    rec.push(timeline(vec![]));
    h.run_steps(2);
    assert!(format!("{:?}", h.state().tool().unwrap().msg).contains("That turn cannot be built."), "the panel says why");
    // not blocked, and the same turn again starts from the same pose
    h.state_mut().use_pose(&quarter);
    let adds = rec.adds("move");
    assert_eq!(adds.len(), 2, "the second turn went out");
    assert_eq!(adds[0]["rotate"], adds[1]["rotate"], "a refused turn is not part of the pose");
    assert_eq!(adds[0]["translate"], adds[1]["translate"]);
}

/// A handle's first Add that fails does not leave Move stuck: the tool says why and the next drop adds.
#[test]
fn a_failed_first_add_does_not_stick_the_tool() {
    let rec = Rec::default();
    let (mut h, grab, to) = move_scene(&rec);
    pointer(&mut h, grab, None);
    pointer(&mut h, grab, Some(true));
    pointer(&mut h, to, None);
    pointer(&mut h, to, Some(false));
    h.run_steps(2);
    assert_eq!(rec.adds("move").len(), 1);
    assert!(h.state().tool().unwrap().accepting.is_some());
    rec.push(Update::Error("Nothing to move.".into()));
    rec.push(timeline(vec![]));
    h.run_steps(2);
    let t = h.state().tool().unwrap();
    assert!(t.accepting.is_none(), "not waiting for an add that failed");
    assert_eq!((t.ok_label.as_str(), t.cancel_label.as_str()), ("OK", "Cancel"), "still a new tool");
    assert!(format!("{:?}", t.msg).contains("Nothing to move."), "the panel says why: {:?}", t.msg);
    // and a drag after it adds again
    let vp = &h.state().viewport;
    let c = vp.gizmo().expect("handles").position;
    let u = handle_unit(vp.camera(), c);
    let grab = vp.world_to_screen(camera::add(c, [0.45 * u, 0.0, 0.0]));
    let to = vp.world_to_screen(camera::add(c, [0.45 * u + 10.0, 0.0, 0.0]));
    pointer(&mut h, grab, None);
    pointer(&mut h, grab, Some(true));
    pointer(&mut h, to, None);
    pointer(&mut h, to, Some(false));
    h.run_steps(2);
    assert_eq!(rec.adds("move").len(), 2, "the next drop adds the step");
}

/// A Move put before the part's supports says the supports are rebuilt (1.x `addFeatureSmart`).
#[test]
fn a_move_before_the_supports_says_so() {
    let rec = Rec::default();
    let mut h = scene(&rec);
    let walls = json!({"body": "b1", "walls": [{"shape": "line", "points": [[0, 0, 0], [1, 0, 0]]}], "removed": [], "output": "join", "options": {}});
    let plain = json!({"body": "b1", "walls": [], "removed": [], "output": "join", "options": {}});
    for (sf, expect) in [
        (plain, "is rebuilt for the new position"),
        (walls, "Walls you drew by hand stay where they were drawn: check them in"),
    ] {
        rec.push(timeline(vec![step_item("f1", "primitive", json!({})), step_item("s1", "supportfins", sf)]));
        h.run_steps(2);
        let mut v = serde_json::Map::new();
        v.insert("bodies".into(), json!(["b1"]));
        assert!(h.state_mut().open_tool("move", Some(v)));
        h.run_steps(2);
        h.state_mut().tool_ok();
        // the step is in: the supports moved one place on
        rec.push(timeline(vec![step_item("f1", "primitive", json!({})), step_item("m9", "move", json!({})), step_item("s1", "supportfins", json!({}))]));
        h.run_steps(2);
        let toasts = format!("{:?}", h.state().toasts);
        assert!(toasts.contains("supportfins s1 is rebuilt for the new position"), "{toasts}");
        assert!(toasts.contains(expect), "{toasts}");
        h.state_mut().toasts.clear();
    }
}

/// The quick red paint still being worked out cannot land over the engine's exact faces.
#[test]
fn a_late_quick_paint_does_not_cover_the_exact_one() {
    use stl_app::viewport::Viewport;
    let mut vp = Viewport::new();
    vp.set_pick_trees(false);
    vp.set_view_size(800.0, 600.0);
    vp.sync_bodies(&[body("b1", plate(60))]);
    vp.flush();
    assert!(vp.show_overhangs_async("b1", 45.0).is_none());
    assert!(vp.overhangs_pending());
    vp.paint_overhangs("b1", &[0, 1], &[]);
    assert!(!vp.overhangs_pending(), "the exact paint took the quick one's place");
    std::thread::sleep(Duration::from_millis(100));
    assert!(vp.poll_overhangs().is_none(), "nothing lands later");
}

/// A tool's first preview on a big mesh (500k triangles) is prepared off the UI thread: the
/// call that shows it and the frame after it stay under one 60 Hz frame, and the preview is
/// there once the worker is done.
#[test]
fn a_big_preview_does_not_stall_the_ui_thread() {
    use stl_app::viewport::Viewport;
    let mut vp = Viewport::new();
    vp.set_view_size(800.0, 600.0);
    let mesh = plate(500);
    assert!(mesh.triangles.len() / 3 >= 500_000);
    vp.sync_bodies(&[body("b1", mesh.clone())]);
    vp.flush();
    let pack = PreviewPack { meshes: vec![PreviewMesh { id: "b1".into(), mesh }], ..Default::default() };
    let t = Instant::now();
    vp.show_preview(&pack);
    let show = t.elapsed();
    let t = Instant::now();
    let _ = vp.frame_for_size(800.0, 600.0, 1.0);
    let frame = t.elapsed();
    assert!(show < Duration::from_micros(16_700), "show_preview took {show:?}");
    assert!(frame < Duration::from_micros(16_700), "the frame after it took {frame:?}");
    assert_eq!(vp.preview_meshes().count(), 0, "not on screen until it is ready");
    vp.flush();
    assert_eq!(vp.preview_meshes().next().map(|(_, m)| m.triangles.len() / 3), Some(500_000));
    assert!(vp.scene_box().is_some());
    // a different preview over the first keeps the first until it is ready, and costs no more
    let second = PreviewPack { meshes: vec![PreviewMesh { id: "b1".into(), mesh: plate(400) }], ..Default::default() };
    let t = Instant::now();
    vp.show_preview(&second);
    assert!(t.elapsed() < Duration::from_micros(16_700), "a replacing preview took {:?}", t.elapsed());
    assert_eq!(vp.preview_meshes().next().map(|(_, m)| m.triangles.len() / 3), Some(500_000), "the old preview stays up meanwhile");
    vp.flush();
    assert_eq!(vp.preview_meshes().next().map(|(_, m)| m.triangles.len() / 3), Some(320_000), "then the new one replaces it");
    assert_eq!(vp.preview_meshes().count(), 1);
}

/// A preview of several meshes appears, and the bodies it replaces go, in one step: never a
/// frame with some of its meshes drawn over bodies that are still visible, and nothing to pick
/// before it is complete.
#[test]
fn a_preview_of_several_meshes_switches_atomically() {
    use stl_app::viewport::Viewport;
    let mut vp = Viewport::new();
    vp.set_view_size(800.0, 600.0);
    vp.sync_bodies(&[body("a", plate(300)), body("b", plate(300))]);
    vp.flush();
    let pack = PreviewPack {
        meshes: vec![PreviewMesh { id: "a".into(), mesh: plate(300) }, PreviewMesh { id: "b".into(), mesh: plate(300) }],
        ..Default::default()
    };
    vp.show_preview(&pack);
    let drawn = |vp: &mut Viewport| {
        let f = vp.frame_for_size(800.0, 600.0, 1.0);
        let previews = f.bodies.iter().filter(|d| d.key.contains("preview:") && d.visible).count();
        let bodies = f.bodies.iter().filter(|d| !d.key.contains("preview:") && d.visible).count();
        (previews, bodies)
    };
    let mut saw_switch = false;
    for _ in 0..2000 {
        let (previews, bodies) = drawn(&mut vp);
        assert!(
            (previews, bodies) == (0, 2) || (previews, bodies) == (2, 0),
            "{previews} preview meshes beside {bodies} bodies"
        );
        assert_eq!(vp.preview_meshes().count(), if previews == 2 { 2 } else { 0 }, "pickable only when drawn");
        assert!(previews == 2 || vp.pick_preview(egui::pos2(400.0, 300.0)).is_none());
        if previews == 2 {
            saw_switch = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(saw_switch, "the preview never appeared");
}

/// A preview replaced before its worker got going is never built: dragging a slider on a big
/// mesh does not pile up work.
#[test]
fn a_replaced_preview_stops_its_worker() {
    use stl_app::viewport::Viewport;
    let mut vp = Viewport::new();
    vp.set_view_size(800.0, 600.0);
    let pack = |n: u32| PreviewPack { meshes: vec![PreviewMesh { id: "b1".into(), mesh: plate(n) }], ..Default::default() };
    for n in 300..310 {
        vp.show_preview(&pack(n));
    }
    vp.flush();
    assert_eq!(vp.preview_meshes().next().map(|(_, m)| m.triangles.len() / 3), Some(2 * 309 * 309));
    assert!(vp.preview_builds() <= 3, "{} of 10 previews were built", vp.preview_builds());
    // cleared before it is ready: its worker stops too
    let before = vp.preview_builds();
    for n in 300..305 {
        vp.show_preview(&pack(n));
        vp.clear_preview();
    }
    vp.flush();
    assert_eq!(vp.preview_meshes().count(), 0);
    assert!(vp.preview_builds() <= before + 1, "cleared previews were built anyway");
}

/// Restart now with unsaved work asks first and does nothing until told to.
#[test]
fn restart_asks_before_dropping_unsaved_work() {
    let rec = Rec::default();
    let mut h = scene(&rec);
    h.state_mut().shell.dirty = true;
    h.state_mut().restart_app(&egui::Context::default());
    h.run_steps(2);
    assert!(h.state().shell.dialogs.is_open(), "the question is on screen");
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert!(!h.state().shell.dialogs.is_open());
}

// ------------------------------------------------------------------ second review

/// The engine refuses an edit's OK: the tool stays open with its values and says why, and Cancel
/// puts the timeline marker back (1.x commitTool awaits the edit).
#[test]
fn a_refused_edit_keeps_the_tool_open_and_cancel_restores_the_marker() {
    let rec = Rec::default();
    let mut h = scene(&rec);
    rec.push(timeline(vec![
        step_item("f1", "primitive", json!({"shape": "box", "length": 10, "width": 10, "height": 10, "plane": "XY", "offset": [0, 0, 0], "rotate": [0, 0, 0], "anchor": "base", "operation": "new", "targets": []})),
        step_item("f2", "hole", json!({})),
    ]));
    h.run_steps(2);
    assert!(h.state_mut().edit_feature("f1"));
    h.run_steps(2);
    h.state_mut().tool_set_value("length", json!(12.0));
    h.state_mut().tool_ok();
    assert!(h.state().tool().is_some(), "the tool waits for the engine");
    assert!(rec.commands().iter().any(|c| matches!(c, Command::EditFeature { rollback_after: Some(None), .. })));
    // the engine says no: its error, then the timeline as it was (rolled back to the step)
    rec.push(Update::Error("That box cannot be built.".into()));
    rec.push(Update::Timeline(TimelineView { items: vec![step_item("f1", "primitive", json!({})), step_item("f2", "hole", json!({}))], marker: Some(0), can_undo: false, can_redo: false }));
    h.run_steps(2);
    let t = h.state().tool().expect("still open");
    assert_eq!(t.values.get("length"), Some(&json!(12.0)), "its values are kept");
    assert!(format!("{:?}", t.msg).contains("That box cannot be built."), "{:?}", t.msg);
    // OK can be tried again
    h.state_mut().tool_ok();
    assert_eq!(rec.commands().iter().filter(|c| matches!(c, Command::EditFeature { .. })).count(), 2);
    rec.push(Update::Error("Still no.".into()));
    rec.push(Update::Timeline(TimelineView { items: vec![step_item("f1", "primitive", json!({})), step_item("f2", "hole", json!({}))], marker: Some(0), can_undo: false, can_redo: false }));
    h.run_steps(2);
    // Cancel puts the marker back where it was before the session (the end)
    h.state_mut().tool_cancel();
    assert!(matches!(rec.commands().last(), Some(Command::Rollback(None)) | Some(Command::ClearPreview)), "{:?}", rec.commands().last());
    assert!(rec.commands().iter().rev().take(3).any(|c| matches!(c, Command::Rollback(None))), "Cancel restores the marker");
}

/// A new tool's OK that the engine refuses leaves the tool open too; the one that is taken closes it.
#[test]
fn a_refused_new_step_keeps_the_tool_open() {
    let rec = Rec::default();
    let mut h = scene(&rec);
    h.state_mut().shell.selection = vec!["b1".into()];
    let mut v = serde_json::Map::new();
    v.insert("bodies".into(), json!(["b1"]));
    assert!(h.state_mut().open_tool("move", Some(v)));
    h.run_steps(2);
    h.state_mut().tool_ok();
    assert!(h.state().tool().is_some());
    rec.push(Update::Error("Nothing to move.".into()));
    rec.push(timeline(vec![]));
    h.run_steps(2);
    assert!(h.state().tool().is_some(), "kept open");
    h.state_mut().tool_ok();
    rec.push(timeline(vec![step_item("m1", "move", json!({}))]));
    h.run_steps(2);
    assert!(h.state().tool().is_none(), "closed once the engine took it");
}

/// A drop that came while the first Add was on its way still goes out when that Add fails
/// (1.x do/while).
#[test]
fn a_second_drop_queued_during_a_failed_add_is_still_sent() {
    let rec = Rec::default();
    let (mut h, grab, to) = move_scene(&rec);
    pointer(&mut h, grab, None);
    pointer(&mut h, grab, Some(true));
    pointer(&mut h, to, None);
    pointer(&mut h, to, Some(false));
    h.run_steps(2);
    assert_eq!(rec.adds("move").len(), 1);
    // a second drag, let go before the engine has answered the first
    let vp = &h.state().viewport;
    let c = vp.gizmo().expect("handles").position;
    let u = handle_unit(vp.camera(), c);
    let g2 = vp.world_to_screen(camera::add(c, [0.45 * u, 0.0, 0.0]));
    let t2 = vp.world_to_screen(camera::add(c, [0.45 * u + 8.0, 0.0, 0.0]));
    pointer(&mut h, g2, None);
    pointer(&mut h, g2, Some(true));
    pointer(&mut h, t2, None);
    pointer(&mut h, t2, Some(false));
    h.run_steps(2);
    assert_eq!(rec.adds("move").len(), 1, "one at a time: the second waits");
    assert!(h.state().tool().unwrap().accept_again, "and is remembered");
    rec.push(Update::Error("Nothing to move.".into()));
    rec.push(timeline(vec![]));
    h.run_steps(2);
    assert_eq!(rec.adds("move").len(), 2, "the queued drop went out after the failure: {:?}", rec.commands());
}
