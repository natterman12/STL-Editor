//! M4c, Task 2: sketch mode inside the real app (`StlApp::layout` in a kittest harness on a
//! recording engine link): the plane prompt, entering from a face or an origin plane, Finish,
//! Cancel, Ctrl+Z, the sketch ribbon, the tool search and the gizmo.
#![cfg(not(target_arch = "wasm32"))]

// keeps this test binary out of the real 2.0 folders (STLEDITOR2_CONFIG_DIR)
#[path = "support/test_env.rs"]
mod test_env;

use std::sync::{Arc, Mutex};

use egui::Pos2;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use serde_json::{json, Value};
use stl_app::app::StlApp;
use stl_app::engine_link::{BodyView, Command, EngineLink, TimelineItem, TimelineView, Update};
use stl_app::platform::NullPlatform;
use stl_app::sketch::{Entity, SketchSession};
use stl_app::ui::ribbon;
use stl_app::ui::ThemePref;
use stl_kernel::MeshArrays;

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
    fn adds(&self) -> Vec<(String, Value)> {
        self.commands()
            .into_iter()
            .filter_map(|c| match c {
                Command::AddFeature { kind, params, .. } => Some((kind, params)),
                _ => None,
            })
            .collect()
    }
}

fn cube(s: f32) -> Arc<MeshArrays> {
    let h = s / 2.0;
    let p = [[-h, -h, 0.0], [h, -h, 0.0], [h, h, 0.0], [-h, h, 0.0], [-h, -h, s], [h, -h, s], [h, h, s], [-h, h, s]];
    let t: [u32; 36] = [0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 1, 2, 6, 1, 6, 5, 2, 3, 7, 2, 7, 6, 3, 0, 4, 3, 4, 7];
    Arc::new(MeshArrays { positions: p.iter().flatten().copied().collect(), triangles: t.to_vec() })
}

fn body(id: &str) -> BodyView {
    BodyView { id: id.into(), name: id.into(), color: Some("#9aa7b4".into()), hidden: false, solid: true, mesh: cube(20.0), revision: 1 }
}

fn item(id: &str, kind: &str, params: Value) -> TimelineItem {
    TimelineItem { id: id.into(), kind: kind.into(), label: format!("{kind} {id}"), active: true, params, ..Default::default() }
}

fn timeline(items: Vec<TimelineItem>, marker: Option<usize>) -> Update {
    Update::Timeline(TimelineView { items, marker, can_undo: true, can_redo: false })
}

/// The app on a recording link with one cube, drawn for a few frames (the viewport has flushed).
fn scene(rec: &Rec) -> Harness<'static, StlApp> {
    let app = StlApp::new(&egui::Context::default(), Box::new(NullPlatform), Box::new(rec.clone()));
    rec.push(Update::Bodies(vec![body("b1")]));
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

fn line(id: &str, p1: [f64; 2], p2: [f64; 2]) -> Entity {
    serde_json::from_value(json!({ "id": id, "type": "line", "p1": p1, "p2": p2 })).unwrap()
}

fn rect(id: &str) -> Entity {
    serde_json::from_value(json!({ "id": id, "type": "rect", "x": 0, "y": 0, "w": 10, "h": 5 })).unwrap()
}

fn polyline_open(id: &str) -> Entity {
    serde_json::from_value(json!({ "id": id, "type": "polyline", "closed": false, "pts": [[0, 0], [5, 0], [5, 5]] })).unwrap()
}

fn sess<'a>(h: &'a mut Harness<'_, StlApp>) -> &'a mut SketchSession {
    &mut h.state_mut().sketch_mode_mut().expect("a sketch is open").s
}

// ------------------------------------------------------------------ the tests

#[test]
fn search_lists_sketch_tools() {
    // while a sketch is open the tool search offers the sketch tools (app.js:165) and nothing else
    let keys: Vec<String> = ribbon::all_commands_in(true).into_iter().map(|c| c.k).collect();
    for k in ["line", "arc3", "rect", "rect_center", "circle", "polygon", "slot", "text", "trim", "select", "c_dimension", "c_hv", "finish", "finish_extrude", "sk_cancel", "look"] {
        assert!(keys.iter().any(|x| x == k), "{k} is searchable in a sketch");
    }
    assert!(!keys.iter().any(|k| k == "primitive_box" || k == "extrude" || k == "hole"), "model tools are not listed: {keys:?}");
    assert!(ribbon::search_in("trim", true).iter().any(|c| c.k == "trim"));
    assert!(ribbon::search_in("constr", true).iter().any(|c| c.k == "construction"));
    assert!(ribbon::search_in("box", true).is_empty());
    // outside a sketch the search is the model's
    assert!(ribbon::search_in("box", false).iter().any(|c| c.k == "primitive_box"));
    // and it is the same list the app searches while a sketch is open
    let rec = Rec::default();
    let mut h = scene(&rec);
    h.state_mut().enter_sketch(Some(json!("XY")), None);
    h.run_steps(2);
    assert_eq!(h.state().ribbon_tab(), "sketch");
}

#[test]
fn begin_prompts_for_a_plane() {
    let rec = Rec::default();
    let mut h = scene(&rec);
    h.state_mut().begin_sketch(None);
    h.run_steps(3);
    let t = h.state().tool().expect("the plane prompt is a tool panel");
    assert_eq!(t.def.key, "sketch");
    assert_eq!(t.title(), "Create Sketch");
    assert_eq!(t.ok_label, "Start");
    assert!(h.state().viewport.base_planes_visible(), "the origin planes show, to be picked");
    assert_eq!(h.state().shell.hint, "Select a plane or flat face to sketch on");
    assert!(h.query_by_label("Start").is_some(), "the Start button");
    assert!(h.query_by_label_contains("Click a flat face of a body, a construction plane, or one of the origin planes (XY, XZ, YZ).").is_some(), "the info line, word for word");
    // nothing is added by choosing a plane: Start enters the sketch
    h.state_mut().tool_set_value("plane", json!("XZ"));
    h.state_mut().tool_ok();
    h.run_steps(2);
    assert!(h.state().tool().is_none());
    let s = h.state().sketch().expect("the sketch opened on the plane");
    assert_eq!(s.plane_ref, json!("XZ"));
    assert!(rec.adds().is_empty());
    assert!(!h.state().viewport.base_planes_visible(), "the planes go once the sketch is open");
}

#[test]
fn enter_from_face_and_origin_plane() {
    let rec = Rec::default();
    let mut h = scene(&rec);
    // an origin plane
    h.state_mut().enter_sketch(Some(json!("XY")), None);
    h.run_steps(2);
    {
        let s = h.state().sketch().expect("open");
        assert_eq!(s.frame.normal, [0.0, 0.0, 1.0]);
        assert_eq!(s.tool, "select");
        assert!(s.entities.is_empty() && s.feature_id.is_none());
    }
    // the part's edges are asked for, with the plane's frame (routes.js sketch_op `refs`)
    let ask = rec.commands().into_iter().find_map(|c| match c {
        Command::SketchOp { op, data, .. } if op == "refs" => Some(data),
        _ => None,
    });
    let ask = ask.expect("refs was asked");
    assert_eq!(ask["frame"], json!([[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]));
    assert_eq!(ask["entities"], json!([]));
    // the panel, the hint and the ribbon are the sketch's
    assert_eq!(h.state().ribbon_tab(), "sketch");
    assert!(h.query_by_label("Finish Sketch").is_some());
    assert!(h.query_by_label("Finish & Extrude").is_some());
    assert!(h.state().shell.hint.starts_with("Click a shape to select it"));
    assert_eq!(h.state().viewport.mouse_mode, stl_app::viewport::MouseMode::Sketch);
    h.state_mut().cancel_sketch();
    h.run_steps(2);
    assert!(h.state().sketch().is_none());
    assert_eq!(h.state().ribbon_tab(), "solid");
    assert_eq!(h.state().viewport.mouse_mode, stl_app::viewport::MouseMode::Model);

    // a flat face of a body
    let face = json!({ "origin": [0.0, 0.0, 20.0], "normal": [0.0, 0.0, 1.0], "xdir": [1.0, 0.0, 0.0], "label": "Face of b1", "body": "b1" });
    h.state_mut().enter_sketch(Some(face.clone()), None);
    h.run_steps(2);
    let s = h.state().sketch().expect("open on the face");
    assert_eq!(s.frame.origin, [0.0, 0.0, 20.0]);
    assert_eq!(s.frame.y, [0.0, 1.0, 0.0]);
    assert_eq!(s.plane_ref, face);
    // a plane that is not there says so
    h.state_mut().cancel_sketch();
    h.state_mut().enter_sketch(Some(json!("nope")), None);
    assert!(h.state().sketch().is_none());
    h.run_steps(2);
    assert!(h.query_by_label("Could not use that plane.").is_some());
}

#[test]
fn finish_adds_sketch_step() {
    let rec = Rec::default();
    let mut h = scene(&rec);
    h.state_mut().enter_sketch(Some(json!("XY")), None);
    h.run_steps(2);
    {
        let s = sess(&mut h);
        s.push();
        s.entities.push(line("a", [0.0, 0.0], [10.0, 0.0]));
    }
    let expect = h.state().sketch().unwrap().step_params();
    assert_eq!(expect["plane"], json!("XY"));
    assert_eq!(expect["entities"].as_array().map(|a| a.len()), Some(1));
    h.state_mut().finish_sketch(false);
    let adds = rec.adds();
    assert_eq!(adds.len(), 1, "one step goes out");
    assert_eq!(adds[0].0, "sketch");
    assert_eq!(adds[0].1, expect, "the step is what `step_params` says");
    // the sketch stays open until the engine has taken the step
    h.run_steps(1);
    assert!(h.state().sketch().is_some());
    rec.push(timeline(vec![item("f1", "sketch", adds[0].1.clone())], None));
    h.run_steps(2);
    assert!(h.state().sketch().is_none(), "the engine took it: the sketch closes");
    assert!(h.state().shell.hint.is_empty());
    // a refused step leaves it open (1.x stops the sketch only after the add worked)
    h.state_mut().enter_sketch(Some(json!("XY")), None);
    h.state_mut().finish_sketch(false);
    rec.push(Update::Error("no".into()));
    h.run_steps(2);
    assert!(h.state().sketch().is_some());
}

#[test]
fn edit_then_cancel_restores() {
    let rec = Rec::default();
    let mut h = scene(&rec);
    let params = json!({ "plane": "XY", "entities": [{ "id": "a", "type": "line", "p1": [0, 0], "p2": [10, 0] }], "refs": [], "constraints": [] });
    // the model is rolled back to the end of the second step; the sketch is the first
    rec.push(timeline(vec![item("f1", "sketch", params.clone()), item("f2", "hole", json!({}))], Some(2)));
    h.run_steps(2);
    let tl_before = h.state().timeline_view.clone();
    assert!(h.state_mut().edit_feature("f1"));
    h.run_steps(2);
    assert!(rec.commands().iter().any(|c| matches!(c, Command::Rollback(Some(0)))), "the timeline rolls back to the sketch: {:?}", rec.commands());
    {
        let s = h.state().sketch().expect("editing");
        assert_eq!(s.feature_id.as_deref(), Some("f1"));
        assert_eq!(s.entities.len(), 1);
    }
    assert!(h.query_by_label("Finish Sketch").is_some());
    h.run_steps(1);
    // a change inside the sketch, then Cancel
    sess(&mut h).entities.push(line("b", [0.0, 0.0], [0.0, 10.0]));
    h.state_mut().cancel_sketch();
    h.run_steps(2);
    assert!(h.state().sketch().is_none());
    assert!(matches!(rec.commands().last(), Some(Command::Rollback(Some(2)))), "Cancel puts the rollback back: {:?}", rec.commands().last());
    assert!(rec.commands().iter().all(|c| !matches!(c, Command::EditFeature { .. })), "nothing was sent as an edit");
    // the step is what it was
    assert_eq!(h.state().timeline_view.items[0].params, params);
    assert_eq!(format!("{:?}", h.state().timeline_view.items), format!("{:?}", tl_before.items));
    // the step is edited again, and Finish sends one edit that puts the old rollback back
    rec.push(timeline(vec![item("f1", "sketch", params.clone()), item("f2", "hole", json!({}))], Some(2)));
    h.run_steps(2);
    assert!(h.state_mut().edit_feature("f1"));
    h.state_mut().finish_sketch(false);
    let edit = rec.commands().into_iter().rev().find_map(|c| match c {
        Command::EditFeature { id, rollback_after, .. } => Some((id, rollback_after)),
        _ => None,
    });
    assert_eq!(edit, Some(("f1".to_owned(), Some(Some(2)))));
}

#[test]
fn ctrl_z_in_sketch_is_local() {
    let rec = Rec::default();
    let mut h = scene(&rec);
    rec.push(timeline(vec![item("f0", "primitive", json!({"shape": "box"}))], None));
    h.run_steps(2);
    h.state_mut().enter_sketch(Some(json!("XY")), None);
    h.run_steps(2);
    {
        let s = sess(&mut h);
        s.push();
        s.entities.push(line("a", [0.0, 0.0], [10.0, 0.0]));
    }
    h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::Z);
    h.run_steps(2);
    assert!(h.state().sketch().expect("still open").entities.is_empty(), "the sketch's last edit is undone");
    assert!(rec.commands().iter().all(|c| !matches!(c, Command::Undo)), "the document's history is not touched");
    // Finish; then Ctrl+Z undoes the sketch step
    {
        let s = sess(&mut h);
        s.push();
        s.entities.push(line("a", [0.0, 0.0], [10.0, 0.0]));
    }
    h.state_mut().finish_sketch(false);
    let params = rec.adds()[0].1.clone();
    rec.push(timeline(vec![item("f0", "primitive", json!({})), item("f1", "sketch", params)], None));
    h.run_steps(2);
    assert!(h.state().sketch().is_none());
    h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::Z);
    h.run_steps(2);
    assert!(rec.commands().iter().any(|c| matches!(c, Command::Undo)), "after Finish Ctrl+Z is the document's");
}

#[test]
fn finish_empty_and_open_profile() {
    let rec = Rec::default();
    let mut h = scene(&rec);
    // an empty sketch: the warning, and the step is added all the same (as 1.x)
    h.state_mut().enter_sketch(Some(json!("XY")), None);
    h.state_mut().finish_sketch(false);
    h.run_steps(2);
    assert!(h.query_by_label("The sketch is empty.").is_some());
    assert_eq!(rec.adds().len(), 1);
    assert_eq!(rec.adds()[0].1["entities"], json!([]));
    rec.push(timeline(vec![item("f1", "sketch", rec.adds()[0].1.clone())], None));
    h.run_steps(2);
    assert!(h.state().sketch().is_none());

    // Finish & Extrude with only an open curve: the sketch is added, Extrude does not open
    h.state_mut().enter_sketch(Some(json!("XY")), None);
    sess(&mut h).entities.push(polyline_open("p"));
    h.state_mut().finish_sketch(true);
    assert_eq!(rec.adds().len(), 2);
    rec.push(timeline(vec![item("f1", "sketch", json!({})), item("f2", "sketch", rec.adds()[1].1.clone())], None));
    h.run_steps(2);
    assert!(h.state().sketch().is_none());
    assert!(h.state().tool().is_none(), "no closed profile: Extrude stays shut");

    // with a closed shape it opens, on the new sketch
    h.state_mut().enter_sketch(Some(json!("XY")), None);
    sess(&mut h).entities.push(rect("r"));
    h.state_mut().finish_sketch(true);
    assert_eq!(rec.adds().len(), 3);
    rec.push(timeline(vec![item("f1", "sketch", json!({})), item("f2", "sketch", json!({})), item("f3", "sketch", rec.adds()[2].1.clone())], None));
    h.run_steps(3);
    let t = h.state().tool().expect("Extrude opened");
    assert_eq!(t.def.key, "extrude");
    assert_eq!(t.values.get("sketch"), Some(&json!("f3")));
}

#[test]
fn ribbon_is_sketch_tab() {
    // the groups and labels of ribbonDef's sketch branch (app.js:105-118)
    let d = ribbon::sketch_def();
    let names: Vec<&str> = d.groups.iter().map(|g| g.label).collect();
    assert_eq!(names, ["Draw", "Modify", "Dimension", "Relations", "Finish"]);
    assert_eq!(d.tabs, vec![("sketch", "Sketch")]);
    assert!(d.pinned.items.is_empty(), "no Inspect group in a sketch");
    let labels = |g: usize| d.groups[g].tools().iter().map(|t| t.label).collect::<Vec<_>>();
    assert_eq!(labels(0), ["Line", "Arc", "Rectangle", "Center Rectangle", "Circle", "Polygon", "Slot", "Text", "Project", "Use Edge", "Import SVG / DXF"]);
    assert_eq!(labels(1), ["Select", "Trim", "Offset", "Mirror", "Round corner", "Delete", "Construction", "Look At"]);
    assert_eq!(labels(2), ["Smart Dimension"]);
    assert_eq!(labels(3), ["Coincident", "Horiz / Vert", "Parallel", "Perpendicular", "Equal", "Tangent", "Concentric", "Midpoint", "Fix", "Symmetric"]);
    assert_eq!(labels(4), ["Finish", "Finish & Extrude", "Cancel"]);
    let keys: Vec<&str> = d.groups[0].tools().iter().map(|t| t.key).collect();
    assert_eq!(keys, ["line", "arc3", "rect", "rect_center", "circle", "polygon", "slot", "text", "sk_project", "sk_useedge", "sk_import"]);
    match &d.groups[3].items[0] {
        ribbon::Item::Grid { tools, cols } => assert_eq!((*cols, tools.len()), (5, 10)),
        other => panic!("the relations are a 5-column grid: {other:?}"),
    }
    match &d.groups[0].items[2] {
        ribbon::Item::Drop { id, label, .. } => assert_eq!((*id, *label), ("sk_rect", "Rectangle")),
        other => panic!("Rectangle is a dropdown: {other:?}"),
    }
    // and the app draws it: its buttons are found by name
    let rec = Rec::default();
    let mut h = scene(&rec);
    assert!(h.query_by_label("Extrude").is_some(), "the model tab before");
    h.state_mut().enter_sketch(Some(json!("XY")), None);
    h.run_steps(3);
    for l in ["Line", "Trim", "Smart Dimension", "Finish", "Finish & Extrude", "Cancel", "Coincident"] {
        assert!(h.query_all_by_label(l).next().is_some(), "the {l} button");
    }
    assert!(h.query_by_label("Extrude").is_none(), "no model tools in a sketch");
    // choosing a tool in the ribbon sets it, with its hint
    h.state_mut().on_ribbon("line", Pos2::ZERO);
    assert_eq!(h.state().sketch().unwrap().tool, "line");
    assert!(h.state().shell.hint.starts_with("Click points to draw connected lines"));
    // one more: Escape leaves the tool, back to Select
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert_eq!(h.state().sketch().unwrap().tool, "select");
}

#[test]
fn gizmo_hidden_in_sketch() {
    let rec = Rec::default();
    let mut h = scene(&rec);
    h.state_mut().shell.selection = vec!["b1".into()];
    h.state_mut().on_ribbon("move", Pos2::ZERO);
    h.run_steps(3);
    assert!(h.state().viewport.gizmo().is_some(), "Move shows its arrows");
    // a sketch closes the tool, and the handles go with it
    h.state_mut().begin_sketch(Some(json!("XY")));
    h.run_steps(3);
    assert!(h.state().sketch().is_some());
    assert!(h.state().viewport.gizmo().is_none(), "no handles while a sketch is open");
    assert!(h.state().tool().is_none());
}

#[test]
fn project_dialog_as_1x() {
    let rec = Rec::default();
    let mut h = scene(&rec);
    h.state_mut().enter_sketch(Some(json!("XY")), None);
    h.run_steps(2);
    // no visible solid body: a warning, no dialog
    let mut hidden = body("b1");
    hidden.hidden = true;
    rec.push(Update::Bodies(vec![hidden]));
    h.run_steps(2);
    h.state_mut().on_ribbon("sk_project", Pos2::ZERO);
    h.run_steps(2);
    assert!(h.query_by_label("There are no solid bodies to project.").is_some());
    assert!(h.query_by_label("Project outline").is_none());
    // with a solid body: the dialog, 1.x's labels and defaults
    rec.push(Update::Bodies(vec![body("b1")]));
    h.run_steps(2);
    h.state_mut().on_ribbon("sk_project", Pos2::ZERO);
    h.run_steps(2);
    for l in [
        "Project outline",
        "Adds the outline of the model to this sketch as shapes you can offset, trim or extrude.",
        "Reference only (not part of the profile)",
        "Offset grows (+) or shrinks (−) the outline, e.g. +0.3 for a lid that fits over the part. Round outlines come in as circles.",
    ] {
        assert!(h.query_by_label(l).is_some(), "{l}");
    }
    h.key_press(egui::Key::Enter); // the ribbon has a "Project" button too
    h.run_steps(2);
    assert!(h.query_by_label("Project outline").is_none(), "the dialog closed");
    let (seq, data) = rec
        .commands()
        .into_iter()
        .rev()
        .find_map(|c| match c {
            Command::SketchOp { seq, op, data } if op == "project" => Some((seq, data)),
            _ => None,
        })
        .expect("project was sent");
    assert_eq!(data["mode"], "section");
    assert_eq!(data["offset"], 0.0);
    assert_eq!(data["construction"], false);
    assert!(data.get("bodies").is_none(), "all visible bodies");
    // the answer replaces the shapes and says how many outlines came in
    rec.push(Update::SketchOp {
        seq,
        result: Ok(json!({ "entities": [{ "id": "p1", "type": "circle", "cx": 0, "cy": 0, "r": 5 }], "added": ["p1"] })),
    });
    h.run_steps(3);
    assert_eq!(h.state().sketch().unwrap().entities.len(), 1);
    assert!(h.query_by_label("Added 1 outline.").is_some());
}

#[test]
fn over_defined_dimension_asks_first() {
    use stl_app::sketch::{dims, Constraint, SketchEffect};
    let rec = Rec::default();
    let mut h = scene(&rec);
    h.state_mut().enter_sketch(Some(json!("XY")), None);
    h.run_steps(2);
    let dim = |id: &str| -> Constraint {
        serde_json::from_value(json!({ "id": id, "type": "hdistance", "a": { "e": "L", "k": "p1" }, "b": { "e": "L", "k": "p2" }, "off": 8, "slide": 0, "value": 10 })).unwrap()
    };
    {
        let s = sess(&mut h);
        s.entities.push(line("L", [0.0, 0.0], [10.0, 0.0]));
        s.constraints.push(dim("k1"));
    }
    // the same size again: the sketch already sets it, so it asks
    let fx = dims::place_dimension(sess(&mut h), &dim("k2"), String::new());
    assert!(fx.iter().any(|e| matches!(e, SketchEffect::Dialog(stl_app::sketch::SketchDialog::OverDefined(_)))));
    assert_eq!(sess(&mut h).constraints.len(), 1, "nothing added until it is answered");
    h.state_mut().sketch_apply(fx);
    h.run_steps(2);
    for l in [
        "Over defined",
        "The sketch's other dimensions and relations already set this size, so another dimension can't drive it.",
        "Add as reference",
    ] {
        assert!(h.query_by_label(l).is_some(), "{l}");
    }
    // Cancel (Escape; the ribbon has a Cancel button too) discards it
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert!(h.query_by_label("Over defined").is_none());
    assert_eq!(sess(&mut h).constraints.len(), 1);
    // Add as reference adds it as a driven dimension
    let fx = dims::place_dimension(sess(&mut h), &dim("k3"), String::new());
    h.state_mut().sketch_apply(fx);
    h.run_steps(2);
    h.get_by_label("Add as reference").click();
    h.run_steps(2);
    let s = sess(&mut h);
    assert_eq!(s.constraints.len(), 2);
    assert!(s.constraints[1].has_ref(), "a reference dimension");
    assert_eq!(s.sel_c, Some(1));
}

#[test]
fn dialog_blocks_sketch_keys() {
    use stl_app::sketch::{dims, Constraint, SketchEffect};
    let rec = Rec::default();
    let mut h = scene(&rec);
    h.state_mut().enter_sketch(Some(json!("XY")), None);
    h.run_steps(2);
    let dim = |id: &str| -> Constraint {
        serde_json::from_value(json!({ "id": id, "type": "hdistance", "a": { "e": "L", "k": "p1" }, "b": { "e": "L", "k": "p2" }, "off": 8, "slide": 0, "value": 10 })).unwrap()
    };
    {
        let s = sess(&mut h);
        s.entities.push(line("L", [0.0, 0.0], [10.0, 0.0]));
        s.constraints.push(dim("k1"));
        s.sel.insert("L".into());
    }
    let fx = dims::place_dimension(sess(&mut h), &dim("k2"), String::new());
    assert!(fx.iter().any(|e| matches!(e, SketchEffect::Dialog(stl_app::sketch::SketchDialog::OverDefined(_)))));
    h.state_mut().sketch_apply(fx);
    h.run_steps(2);
    assert!(h.query_by_label("Over defined").is_some());
    // Delete would remove the selected line; the open question takes the key instead
    h.key_press(egui::Key::Delete);
    h.run_steps(2);
    assert!(sess(&mut h).entities.iter().any(|e| e.id() == Some("L")), "the sketch tools got no key");
    assert!(h.query_by_label("Over defined").is_some(), "still asking");
    // once answered, keys reach the sketch again
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert!(h.query_by_label("Over defined").is_none());
    h.key_press(egui::Key::Delete);
    h.run_steps(2);
    assert!(!sess(&mut h).entities.iter().any(|e| e.id() == Some("L")), "Delete works again");
}

#[test]
fn dimension_box_applies_on_outside_press() {
    use stl_app::sketch::Constraint;
    let rec = Rec::default();
    let mut h = scene(&rec);
    h.state_mut().enter_sketch(Some(json!("XY")), None);
    h.run_steps(2);
    {
        let s = sess(&mut h);
        s.entities.push(line("L", [0.0, 0.0], [10.0, 0.0]));
        let c: Constraint = serde_json::from_value(json!({ "id": "k1", "type": "hdistance", "a": { "e": "L", "k": "p1" }, "b": { "e": "L", "k": "p2" }, "off": 8, "slide": 0, "value": 10 })).unwrap();
        s.constraints.push(c);
    }
    h.state_mut().sketch_mode_mut().unwrap().panel.modify = Some(stl_app::sketch::panel::Modify { idx: 0, text: "20".into(), fresh: true });
    h.run_steps(3);
    assert!(h.state_mut().sketch_mode_mut().unwrap().panel.modify.is_some(), "the box stays open until something happens");
    // a press elsewhere applies the typed value
    let pos = Pos2::new(640.0, 700.0);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
        h.run_steps(1);
    }
    h.run_steps(2);
    assert!(h.state_mut().sketch_mode_mut().unwrap().panel.modify.is_none(), "the box closed");
    assert_eq!(sess(&mut h).constraints[0].value, Some(20.0), "the value was applied");
}

#[test]
fn inch_mode_snaps_to_a_sixteenth() {
    // 1.x `setUnits('in')`: the sketch's grid snap becomes 1/16 in (1 mm back in millimetres),
    // for the open sketch and the next one.
    let rec = Rec::default();
    let mut h = scene(&rec);
    h.state_mut().enter_sketch(Some(json!("XY")), None);
    h.run_steps(2);
    assert_eq!(h.state().sketch_mode().unwrap().panel.snap_step, 1.0);
    h.state_mut().set_units_inch(true);
    h.run_steps(2);
    assert!(h.state().shell.inch);
    assert_eq!(h.state().sketch_mode().unwrap().panel.snap_step, 25.4 / 16.0);
    assert!(h.query_by_label("Showing inches. Models are still saved in millimetres.").is_some());
    h.state_mut().cancel_sketch();
    h.state_mut().enter_sketch(Some(json!("XY")), None);
    h.run_steps(2);
    let p = &h.state().sketch_mode().unwrap().panel;
    assert!(p.snap_grid);
    assert_eq!(p.snap_step, 25.4 / 16.0, "the next sketch keeps the inch step");
    h.state_mut().set_units_inch(false);
    h.run_steps(2);
    assert_eq!(h.state().sketch_mode().unwrap().panel.snap_step, 1.0);
}

#[test]
fn seeded_dims_solve_to_size() {
    // A rect with a typed width and height: the change solves it to that size, "Fully defined"
    // once its corner is on the origin (the m4c sketch_full state).
    let rec = Rec::default();
    let mut h = scene(&rec);
    h.state_mut().enter_sketch(Some(json!("XY")), None);
    h.run_steps(2);
    {
        let s = sess(&mut h);
        s.entities.push(serde_json::from_value(json!({"id": "ea", "type": "rect", "x": 5, "y": 5, "w": 20, "h": 10})).unwrap());
        for c in [
            json!({"id": "dw", "type": "hdistance", "a": {"e": "ea", "k": "c0"}, "b": {"e": "ea", "k": "c1"}, "value": 30}),
            json!({"id": "dh", "type": "vdistance", "a": {"e": "ea", "k": "c1"}, "b": {"e": "ea", "k": "c2"}, "value": 12}),
            json!({"id": "cf", "type": "coincident", "a": {"e": "ea", "k": "c0"}, "b": {"e": "_o", "k": "p"}}),
        ] {
            s.constraints.push(serde_json::from_value(c).unwrap());
        }
    }
    h.state_mut().sketch_apply(vec![stl_app::sketch::SketchEffect::Changed]);
    h.run_steps(2);
    let m = h.state().sketch_mode().unwrap();
    assert_eq!(m.state.msg.as_deref(), Some("Fully defined"));
    let Entity::Rect(r) = &m.s.entities[0] else { panic!("rect") };
    assert!((r.w.abs() - 30.0).abs() < 1e-6 && (r.h.abs() - 12.0).abs() < 1e-6, "{} × {}", r.w, r.h);
    assert!(r.x.abs() < 1e-6 && r.y.abs() < 1e-6, "corner on the origin: {}, {}", r.x, r.y);
}

/// The `missing_ref` flow: the box grows (40×20 to 55×28) and a saved top-edge reference is
/// matched to the same (moved) top edge, so a dimension to its end still holds the rect 6 mm in
/// from that end, as 1.x does (verified against 1.x's `matchRefs`).
#[test]
fn match_refs_follows_a_grown_box() {
    let saved = vec![line("r0", [40.0, 20.0], [0.0, 20.0])];
    let fresh = vec![
        line("r0", [55.0, 28.0], [0.0, 28.0]),
        line("r1", [0.0, 28.0], [0.0, 0.0]),
        line("r2", [0.0, 0.0], [55.0, 0.0]),
        line("r3", [55.0, 0.0], [55.0, 28.0]),
    ];
    let (list, missing) = stl_app::sketch::session::match_refs(&fresh, &saved);
    assert!(missing.is_empty());
    let ids: Vec<_> = list.iter().map(|e| e.id().unwrap().to_owned()).collect();
    assert_eq!(ids, ["r0", "r1", "r2", "r3"]);
    match &list[0] {
        Entity::Line(l) => assert_eq!((l.p1, l.p2), ([55.0, 28.0], [0.0, 28.0])),
        _ => panic!("line"),
    }
}

/// 1.x `_fill`: a closed rect is a peach sheet (0xf6c7a8 at 0.38) under the strokes. An open
/// curve and a construction rect add no fill, and selection, under-defined and light theme
/// leave the colour alone.
#[test]
fn closed_rect_produces_a_fill() {
    let rec = Rec::default();
    let mut h = scene(&rec);
    h.state_mut().enter_sketch(Some(json!("XY")), None);
    {
        let s = sess(&mut h);
        s.entities.push(rect("r"));
        s.entities.push(polyline_open("p"));
        s.entities.push(serde_json::from_value(json!({ "id": "c", "type": "rect", "x": 20, "y": 0, "w": 4, "h": 4, "construction": true })).unwrap());
    }
    h.state_mut().sketch_mode_mut().unwrap().dirty = true;
    h.run_steps(2);

    let peach = [0xf6, 0xc7, 0xa8, (0.38_f32 * 255.0).round() as u8];
    let area = fill_area(&h);
    assert!((area - 50.0).abs() < 1e-3, "10×5 rect, not the open curve or the construction rect: {area}");
    assert!(fill_is_peach_under_strokes(&h, peach));

    {
        let m = h.state_mut().sketch_mode_mut().unwrap();
        m.s.sel.insert("r".into());
        m.state.under.insert("r".into());
        m.dirty = true;
    }
    h.state_mut().viewport.set_theme(false);
    h.run_steps(2);
    assert!((fill_area(&h) - 50.0).abs() < 1e-3);
    assert!(fill_is_peach_under_strokes(&h, peach), "fill stays peach when selected, under-defined, or in the light theme");
}

fn fill_area(h: &Harness<'_, StlApp>) -> f64 {
    h.state()
        .viewport
        .overlay_items()
        .iter()
        .filter(|o| o.tag == "sketch:ents")
        .flat_map(|o| o.tris.chunks_exact(3))
        .map(|t| {
            let (a, b, c) = (t[0].pos, t[1].pos, t[2].pos);
            let (ux, uy, uz) = ((b[0] - a[0]) as f64, (b[1] - a[1]) as f64, (b[2] - a[2]) as f64);
            let (vx, vy, vz) = ((c[0] - a[0]) as f64, (c[1] - a[1]) as f64, (c[2] - a[2]) as f64);
            let (cx, cy, cz) = (uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx);
            0.5 * (cx * cx + cy * cy + cz * cz).sqrt()
        })
        .sum()
}

fn fill_is_peach_under_strokes(h: &Harness<'_, StlApp>, peach: [u8; 4]) -> bool {
    let items = h.state().viewport.overlay_items();
    let fills: Vec<_> = items.iter().filter(|o| o.tag == "sketch:ents" && !o.tris.is_empty()).collect();
    let strokes: Vec<_> = items.iter().filter(|o| o.tag == "sketch:ents" && !o.lines.is_empty()).collect();
    fills.len() == 1
        && fills[0].order == 8
        && fills[0].depth == stl_app::viewport::overlays::Depth::Off
        && fills[0].tris.iter().all(|v| v.c0 == peach)
        && strokes.iter().all(|o| o.order > fills[0].order)
}

#[test]
fn status_line_shows_pointer_and_definition() {
    // sketcher.js `showStatus`: "Sketch  X 25  Y 15 mm   ·   Under defined: 4 more dimensions needed",
    // in display units (inch mode too), with "· snapped" on a snapped point.
    let rec = Rec::default();
    let mut h = scene(&rec);
    h.state_mut().enter_sketch(Some(json!("XY")), None);
    h.run_steps(2);
    assert_eq!(h.state().status_text, "Sketch");
    {
        let m = h.state_mut().sketch_mode_mut().unwrap();
        m.cursor = Some([25.0, 15.0]);
        m.state.msg = Some("Under defined: 4 more dimensions needed".into());
    }
    h.run_steps(2);
    assert_eq!(h.state().status_text, "Sketch  X 25  Y 15 mm   ·   Under defined: 4 more dimensions needed");
    h.state_mut().sketch_mode_mut().unwrap().snapped = true;
    h.run_steps(2);
    assert_eq!(h.state().status_text, "Sketch  X 25  Y 15 mm  · snapped   ·   Under defined: 4 more dimensions needed");
    h.state_mut().set_units_inch(true);
    h.run_steps(2);
    {
        let m = h.state_mut().sketch_mode_mut().unwrap();
        m.cursor = Some([25.4, 12.7]);
        m.snapped = false;
    }
    h.run_steps(2);
    assert_eq!(h.state().status_text, "Sketch  X 1  Y 0.5 in   ·   Under defined: 4 more dimensions needed");
}

#[test]
fn finish_sends_no_name_so_the_engine_numbers_it() {
    // 1.x sends `{ type: 'sketch', params }` with no name; the engine names it Sketch1, Sketch2.
    let rec = Rec::default();
    let mut h = scene(&rec);
    h.state_mut().enter_sketch(Some(json!("XY")), None);
    h.run_steps(2);
    h.state_mut().finish_sketch(false);
    let name = rec.commands().into_iter().find_map(|c| match c {
        Command::AddFeature { kind, name, .. } if kind == "sketch" => Some(name),
        _ => None,
    });
    assert_eq!(name, Some(None));
}

#[test]
fn engine_names_sketches_one_and_two() {
    // The browser lists the step's own name ("Sketch1"), not the state's default ("Sketch").
    use std::sync::atomic::AtomicBool;
    use stl_app::engine_link::worker::Worker;
    let log: Arc<Mutex<Vec<Update>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = log.clone();
    let mut w = Worker::new(Arc::new(move |u| sink.lock().unwrap().push(u)), Arc::new(AtomicBool::new(false)));
    let params = json!({"plane": "XY", "entities": [{"id": "r1", "type": "rect", "x": 0, "y": 0, "w": 20, "h": 10}]});
    let names = |log: &Arc<Mutex<Vec<Update>>>| {
        let ups = std::mem::take(&mut *log.lock().unwrap());
        ups.into_iter().rev().find_map(|u| if let Update::Sketches(s) = u { Some(s) } else { None }).unwrap()
    };
    w.handle(Command::AddFeature { kind: "sketch".into(), params: params.clone(), index: None, name: None });
    assert_eq!(names(&log).iter().map(|s| s.name.clone()).collect::<Vec<_>>(), ["Sketch1"]);
    w.handle(Command::AddFeature { kind: "sketch".into(), params, index: None, name: None });
    assert_eq!(names(&log).iter().map(|s| s.name.clone()).collect::<Vec<_>>(), ["Sketch1", "Sketch2"]);
}

#[test]
fn browser_sketch_row_has_a_count_tag() {
    use stl_app::engine_link::{SketchOutline, SketchView};
    let rec = Rec::default();
    let mut h = scene(&rec);
    let o = SketchOutline { pts: vec![[0.0, 0.0], [5.0, 0.0], [5.0, 5.0]], closed: false, construction: false };
    rec.push(Update::Sketches(vec![SketchView {
        id: "s1".into(),
        name: "Sketch1".into(),
        plane: "XY".into(),
        outlines: vec![o; 7].into(),
        ..Default::default()
    }]));
    h.run_steps(3);
    assert!(h.query_by_label("Sketch1").is_some());
    assert!(h.query_by_label("7").is_some(), "the shape count sits at the end of the row");
}
