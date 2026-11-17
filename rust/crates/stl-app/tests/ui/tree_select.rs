//! Milestone 7 UI tests (Task 10, spec 2.12): the whole browser row and the timeline item select
//! reliably. A click on a row's empty side selects the body; a sketch row selects and highlights
//! its sketch and a double-click edits it; a timeline item click selects its step; clicks still
//! register while the engine is busy.
#![cfg(not(target_arch = "wasm32"))]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use stl_app::app::StlApp;
use stl_app::engine_link::native::NativeLink;
use stl_app::engine_link::{BodyView, Command, SketchView, TimelineItem, TimelineView};
use stl_app::theme;
use stl_app::ui::UiEvent;
use stl_app::ui::browser::{Browser, BrowserView, show as show_browser};
use stl_app::ui::timeline::{Timeline, show as show_timeline};
use stl_kernel::MeshArrays;

use super::harness::{
    app_harness_ex, empty_harness, enter_sketch_plane, finish_sketch_settle, project, settle, sketch_add_rect, wait_for,
};

// ------------------------------------------------------------------ pointer helpers

/// One click (press and release in one frame, as a fast click arrives), then a frame.
fn click<T>(h: &mut Harness<'_, T>, at: egui::Pos2) {
    use egui::{Event, PointerButton};
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    h.event(Event::PointerMoved(at));
    h.event(button(true));
    h.event(button(false));
    h.step();
}

/// Two clicks in one frame: egui reads the second as a double-click.
fn double_click<T>(h: &mut Harness<'_, T>, at: egui::Pos2) {
    use egui::{Event, PointerButton};
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    h.event(Event::PointerMoved(at));
    for _ in 0..2 {
        h.event(button(true));
        h.event(button(false));
    }
    h.step();
    h.run_steps(2);
}

// ------------------------------------------------------------------ browser (no app)

fn body(id: &str, name: &str) -> BodyView {
    BodyView {
        id: id.into(),
        name: name.into(),
        color: Some("#9aa7b4".into()),
        hidden: false,
        solid: true,
        mesh: Arc::new(MeshArrays {
            positions: Vec::new(),
            triangles: Vec::new(),
        }),
        revision: 1,
        holds_up: None,
    }
}

struct BrowserUi {
    browser: Browser,
    bodies: Vec<BodyView>,
    sketches: Vec<SketchView>,
    events: Rc<RefCell<Vec<UiEvent>>>,
    commands: Rc<RefCell<Vec<Command>>>,
}

fn browser_harness(st: BrowserUi) -> Harness<'static, BrowserUi> {
    let mut fonts = false;
    Harness::builder()
        .with_size(egui::vec2(280.0, 220.0))
        .with_step_dt(0.05)
        .with_accessibility_check(false)
        .build_ui_state(
            move |ui, st: &mut BrowserUi| {
                if !fonts {
                    fonts = true;
                    stl_app::fonts::install(ui.ctx());
                }
                let tokens = theme::tokens(true);
                theme::apply(ui.ctx(), &tokens);
                let ctx = ui.ctx().clone();
                let view = BrowserView {
                    doc_name: "Doc",
                    bodies: &st.bodies,
                    selection: &[],
                    origin_visible: true,
                    planes: &[],
                    sketches: &st.sketches,
                    params: &[],
                };
                show_browser(
                    ui,
                    &ctx,
                    &tokens,
                    &mut st.browser,
                    view,
                    &mut st.commands.borrow_mut(),
                    &mut st.events.borrow_mut(),
                );
            },
            st,
        )
}

#[test]
fn click_row_empty_side_selects() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut h = browser_harness(BrowserUi {
        browser: Browser::default(),
        bodies: vec![body("b1", "Part")],
        sketches: Vec::new(),
        events: events.clone(),
        commands: Rc::new(RefCell::new(Vec::new())),
    });
    h.run();
    // 250 px is past the name and before the triangle tag: the row's empty side
    let row_y = h.get_by_label("Part").rect().center().y;
    click(&mut h, egui::pos2(250.0, row_y));
    h.run();
    assert!(
        events.borrow().contains(&UiEvent::Select(vec!["b1".into()])),
        "{:?}",
        events.borrow()
    );

    // the eye keeps its own click: it hides the body and does not select it
    events.borrow_mut().clear();
    let eye = h.get_by_label("Hide Part").rect().center();
    click(&mut h, eye);
    h.run();
    assert!(
        !events.borrow().iter().any(|e| matches!(e, UiEvent::Select(_))),
        "the eye does not select: {:?}",
        events.borrow()
    );
}

#[test]
fn sketch_row_double_click_edits() {
    // a click on a sketch row selects it; a double-click edits it (the app opens the sketch)
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut h = browser_harness(BrowserUi {
        browser: Browser::default(),
        bodies: Vec::new(),
        sketches: vec![SketchView {
            id: "s1".into(),
            name: "Sketch1".into(),
            plane: "XY".into(),
            hidden: false,
            ..Default::default()
        }],
        events: events.clone(),
        commands: Rc::new(RefCell::new(Vec::new())),
    });
    h.run();
    let y = h.get_by_label("Sketch1").rect().center().y;
    click(&mut h, egui::pos2(250.0, y));
    h.run();
    assert_eq!(events.borrow().as_slice(), &[UiEvent::SelectSketch("s1".into())]);

    events.borrow_mut().clear();
    double_click(&mut h, egui::pos2(250.0, y));
    h.run();
    assert!(
        events.borrow().contains(&UiEvent::EditFeature("s1".into())),
        "{:?}",
        events.borrow()
    );
}

// ------------------------------------------------------------------ timeline (no app)

fn item(id: &str, kind: &str, label: &str) -> TimelineItem {
    TimelineItem {
        id: id.into(),
        kind: kind.into(),
        label: label.into(),
        active: true,
        ..Default::default()
    }
}

struct TimelineUi {
    tl: Timeline,
    view: TimelineView,
    events: Rc<RefCell<Vec<UiEvent>>>,
    commands: Rc<RefCell<Vec<Command>>>,
}

fn timeline_harness(st: TimelineUi) -> Harness<'static, TimelineUi> {
    let mut fonts = false;
    Harness::builder()
        .with_size(egui::vec2(720.0, 80.0))
        .with_step_dt(0.05)
        .with_accessibility_check(false)
        .build_ui_state(
            move |ui, st: &mut TimelineUi| {
                if !fonts {
                    fonts = true;
                    stl_app::fonts::install(ui.ctx());
                }
                let tokens = theme::tokens(true);
                theme::apply(ui.ctx(), &tokens);
                let ctx = ui.ctx().clone();
                show_timeline(
                    ui,
                    &ctx,
                    &tokens,
                    &mut st.tl,
                    &st.view,
                    false,
                    false,
                    &mut st.commands.borrow_mut(),
                    &mut st.events.borrow_mut(),
                );
            },
            st,
        )
}

#[test]
fn feature_click_selects() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let view = TimelineView {
        items: vec![item("f1", "import", "Import"), item("f2", "box", "Box")],
        marker: None,
        can_undo: false,
        can_redo: false,
    };
    let mut h = timeline_harness(TimelineUi {
        tl: Timeline::default(),
        view,
        events: events.clone(),
        commands: Rc::new(RefCell::new(Vec::new())),
    });
    h.run();
    let at = h.get_by_label("Box").rect().center();
    click(&mut h, at);
    h.run();
    assert!(
        events.borrow().contains(&UiEvent::SelectFeature("f2".into())),
        "{:?}",
        events.borrow()
    );
    // the status line still gets the step's name, as before
    assert_eq!(h.state_mut().tl.take_status().as_deref(), Some("Box"));
}

// ------------------------------------------------------------------ the app (real engine)

/// An empty document with one rectangle sketch on the XY plane, finished (not extruded).
/// Returns the harness, the engine link, and the sketch's id and row name.
fn sketch_fixture() -> (Harness<'static, StlApp>, Arc<NativeLink>, String, String) {
    let (mut h, link) = empty_harness();
    enter_sketch_plane(&mut h, &link, json!("XY"));
    sketch_add_rect(&mut h, "r1", 0.0, -15.0, 20.0, 20.0);
    finish_sketch_settle(&mut h, &link, false);
    wait_for(&mut h, &link, "the sketch row", |h| !h.state().sketches.is_empty());
    let sk = h.state().sketches[0].clone();
    (h, link, sk.id, sk.name)
}

/// The sketch row in the browser. Its name is a plain label; the timeline item beside it is a
/// button of the same name, so the label is found by role.
fn sketch_row(h: &Harness<'_, StlApp>, name: &str) -> egui::Pos2 {
    h.get_by(|n| n.role() == egui::accesskit::Role::Label && n.value() == Some(name.to_string()))
        .rect()
        .center()
}

#[test]
fn sketch_row_selects_and_highlights() {
    let (mut h, link, id, name) = sketch_fixture();
    h.state_mut().shell.events.clear();
    let at = sketch_row(&h, &name);
    click(&mut h, at);
    settle(&mut h, &link);
    assert_eq!(
        h.state().selected_step.as_deref(),
        Some(id.as_str()),
        "the sketch is the selected step"
    );
    assert!(
        h.state().shell.events.contains(&UiEvent::SelectSketch(id.clone())),
        "{:?}",
        h.state().shell.events
    );
    // the viewport draws the sketch in the selection colour under the "step-sel" tag
    assert!(
        h.state()
            .viewport
            .overlay_items()
            .iter()
            .any(|o| o.tag == "step-sel" && !o.lines.is_empty()),
        "the selected sketch is highlighted"
    );
}

#[test]
fn sketch_row_edit_reopens_sketch() {
    // what a double-click on the row sends (`UiEvent::EditFeature`, shown by the browser test above)
    let (mut h, link, id, _name) = sketch_fixture();
    assert!(h.state().sketch().is_none());
    h.state_mut().push_ui_event(UiEvent::EditFeature(id));
    settle(&mut h, &link);
    assert!(h.state().sketch().is_some(), "EditFeature on the sketch reopens it");
}

#[test]
fn feature_click_selects_in_app() {
    let (mut h, _link) = app_harness_ex(Some(&project("basic.stlx")), egui::vec2(1280.0, 800.0));
    let step = h.state().timeline_view.items[0].clone();
    let at = h
        .get_by(|n| n.role() == egui::accesskit::Role::Button && n.label() == Some(step.label.clone()))
        .rect()
        .center();
    click(&mut h, at);
    h.run_steps(2);
    assert!(
        h.state()
            .shell
            .events
            .contains(&UiEvent::SelectFeature(step.id.clone())),
        "{:?}",
        h.state().shell.events
    );
}

#[test]
fn click_while_busy_registers() {
    let (mut h, link) = app_harness_ex(Some(&project("basic.stlx")), egui::vec2(1280.0, 800.0));
    let step = h.state().timeline_view.items[0].clone();
    // the busy overlay shows once the work has taken 0.35 s; a click on a step still gets through
    h.state_mut().busy.set_progress(1, 2, "Rebuilding");
    h.run_steps(3);
    let at = h
        .get_by(|n| n.role() == egui::accesskit::Role::Button && n.label() == Some(step.label.clone()))
        .rect()
        .center();
    click(&mut h, at);
    h.run_steps(2);
    assert!(
        h.state()
            .shell
            .events
            .contains(&UiEvent::SelectFeature(step.id.clone())),
        "the click registered while busy: {:?}",
        h.state().shell.events
    );
    h.state_mut().busy.set_busy(false);
    h.run_steps(1);
    settle(&mut h, &link);
}
