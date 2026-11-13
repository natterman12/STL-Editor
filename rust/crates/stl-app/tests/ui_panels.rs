//! egui_kittest tests for browser, timeline, parameters, status, toasts and busy.
#![cfg(not(target_arch = "wasm32"))]

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use stl_app::engine_link::native::NativeLink;
use stl_app::engine_link::{BodyView, Command, EngineLink, Level, ParamView, TimelineView, Update};
use stl_app::engine_link::{PlaneView, SketchView, TimelineItem};
use stl_app::theme;
use stl_app::ui::browser::{Browser, BrowserView};
use stl_app::ui::browser::show as show_browser;
use stl_app::ui::busy::{Busy, show as show_busy};
use stl_app::ui::params::{Params, show as show_params};
use stl_app::ui::status::show as show_status;
use stl_app::ui::timeline::{Timeline, show as show_timeline};
use stl_app::ui::toasts::{Toasts, show as show_toasts, toast_colours};
use stl_app::ui::UiEvent;
use stl_kernel::MeshArrays;

#[path = "support/wgpu_renderer.rs"]
mod wgpu_renderer;
#[path = "support/gpu.rs"]
mod gpu;

fn empty_mesh() -> Arc<MeshArrays> {
    Arc::new(MeshArrays { positions: Vec::new(), triangles: Vec::new() })
}

fn param_view(name: &str, v: f64) -> ParamView {
    ParamView { name: name.into(), expr: format!("{v}"), note: String::new(), value: Some(v), error: None, used: false }
}

fn test_body(id: &str, name: &str, hidden: bool) -> BodyView {
    BodyView {
        id: id.into(),
        name: name.into(),
        color: Some("#9aa7b4".into()),
        hidden,
        solid: true,
        mesh: empty_mesh(),
        revision: 1,
    }
}

fn item(id: &str, kind: &str, label: &str, active: bool) -> TimelineItem {
    TimelineItem { id: id.into(), kind: kind.into(), label: label.into(), error: None, suppressed: false, warning: None, active }
}

fn sample_timeline(marker: Option<usize>) -> TimelineView {
    let on = |i: usize| marker.map_or(true, |m| i < m);
    TimelineView {
        items: vec![item("f1", "import", "Import", on(0)), item("f2", "box", "Box", on(1)), item("f3", "hole", "Hole", on(2))],
        marker,
        can_undo: marker.is_some(),
        can_redo: false,
    }
}

struct TimelineUi {
    tl: Timeline,
    view: TimelineView,
    sketch_active: bool,
    dark: bool,
    commands: Rc<RefCell<Vec<Command>>>,
    events: Rc<RefCell<Vec<UiEvent>>>,
}

impl TimelineUi {
    fn draw(&mut self, ui: &mut egui::Ui) {
        theme::apply(ui.ctx(), &theme::tokens(self.dark));
        let tokens = theme::tokens(self.dark);
        let ctx = ui.ctx().clone();
        show_timeline(
            ui,
            &ctx,
            &tokens,
            &mut self.tl,
            &self.view,
            false,
            self.sketch_active,
            &mut self.commands.borrow_mut(),
            &mut self.events.borrow_mut(),
        );
    }
}

fn click_label(h: &Harness<'_, impl Sized>, label: &str) {
    h.get_by_label(label).click();
}

fn timeline_harness(state: TimelineUi, w: f32, h: f32) -> Harness<'static, TimelineUi> {
    let mut fonts = false;
    Harness::builder()
        .with_size(egui::vec2(w, h))
        .with_accessibility_check(false)
        .build_ui_state(
            move |ui, st| {
                if !fonts {
                    fonts = true;
                    stl_app::fonts::install(ui.ctx());
                }
                st.draw(ui);
            },
            state,
        )
}

struct BrowserUi {
    browser: Browser,
    doc_name: String,
    bodies: Vec<BodyView>,
    planes: Vec<PlaneView>,
    sketches: Vec<SketchView>,
    selection: Vec<String>,
    origin_visible: bool,
    params: Vec<ParamView>,
    dark: bool,
    commands: Rc<RefCell<Vec<Command>>>,
    events: Rc<RefCell<Vec<UiEvent>>>,
}

impl BrowserUi {
    fn draw(&mut self, ui: &mut egui::Ui) {
        theme::apply(ui.ctx(), &theme::tokens(self.dark));
        let tokens = theme::tokens(self.dark);
        let ctx = ui.ctx().clone();
        let view = BrowserView {
            doc_name: &self.doc_name,
            bodies: &self.bodies,
            selection: &self.selection,
            origin_visible: self.origin_visible,
            planes: &self.planes,
            sketches: &self.sketches,
            params: &self.params,
        };
        show_browser(
            ui,
            &ctx,
            &tokens,
            &mut self.browser,
            view,
            &mut self.commands.borrow_mut(),
            &mut self.events.borrow_mut(),
        );
    }
}

fn browser_harness(state: BrowserUi, w: f32, h: f32) -> Harness<'static, BrowserUi> {
    let mut fonts = false;
    Harness::builder()
        .with_size(egui::vec2(w, h))
        .with_accessibility_check(false)
        .build_ui_state(
            move |ui, st| {
                if !fonts {
                    fonts = true;
                    stl_app::fonts::install(ui.ctx());
                }
                st.draw(ui);
            },
            state,
        )
}

struct ParamsUi {
    params: Params,
    views: Vec<ParamView>,
    used: HashSet<String>,
    dark: bool,
    commands: Rc<RefCell<Vec<Command>>>,
}

impl ParamsUi {
    fn draw(&mut self, ui: &mut egui::Ui) {
        theme::apply(ui.ctx(), &theme::tokens(self.dark));
        let tokens = theme::tokens(self.dark);
        show_params(
            ui,
            &tokens,
            &mut self.params,
            &self.views,
            &self.used,
            &mut self.commands.borrow_mut(),
        );
    }
}

#[test]
fn timeline_rollback_start() {
    let commands = Rc::new(RefCell::new(Vec::new()));
    let mut harness = timeline_harness(
        TimelineUi {
            tl: Timeline::default(),
            view: sample_timeline(Some(2)),
            sketch_active: false,
            dark: true,
            commands: commands.clone(),
            events: Rc::new(RefCell::new(Vec::new())),
        },
        720.0,
        80.0,
    );
    harness.run();
    click_label(&harness, "Roll back to start");
    harness.run();
    assert!(matches!(commands.borrow().as_slice(), [Command::Rollback(Some(0))]));
}

#[test]
fn timeline_rollback_prev() {
    let commands = Rc::new(RefCell::new(Vec::new()));
    let mut harness = timeline_harness(
        TimelineUi {
            tl: Timeline::default(),
            view: sample_timeline(Some(2)),
            sketch_active: false,
            dark: true,
            commands: commands.clone(),
            events: Rc::new(RefCell::new(Vec::new())),
        },
        720.0,
        80.0,
    );
    harness.run();
    click_label(&harness, "Step back");
    harness.run();
    assert!(matches!(commands.borrow().as_slice(), [Command::Rollback(Some(1))]));
}

#[test]
fn timeline_rollback_next() {
    let commands = Rc::new(RefCell::new(Vec::new()));
    let mut harness = timeline_harness(
        TimelineUi {
            tl: Timeline::default(),
            view: sample_timeline(Some(1)),
            sketch_active: false,
            dark: true,
            commands: commands.clone(),
            events: Rc::new(RefCell::new(Vec::new())),
        },
        720.0,
        80.0,
    );
    harness.run();
    click_label(&harness, "Step forward");
    harness.run();
    assert!(matches!(commands.borrow().as_slice(), [Command::Rollback(Some(2))]));
}

#[test]
fn timeline_rollback_end() {
    let commands = Rc::new(RefCell::new(Vec::new()));
    let mut harness = timeline_harness(
        TimelineUi {
            tl: Timeline::default(),
            view: sample_timeline(Some(1)),
            sketch_active: false,
            dark: true,
            commands: commands.clone(),
            events: Rc::new(RefCell::new(Vec::new())),
        },
        720.0,
        80.0,
    );
    harness.run();
    click_label(&harness, "Roll forward to end");
    harness.run();
    assert!(matches!(commands.borrow().as_slice(), [Command::Rollback(None)]));
}

#[test]
fn timeline_nav_locked_during_sketch() {
    let commands = Rc::new(RefCell::new(Vec::new()));
    let mut harness = timeline_harness(
        TimelineUi {
            tl: Timeline::default(),
            view: sample_timeline(Some(2)),
            sketch_active: true,
            dark: true,
            commands: commands.clone(),
            events: Rc::new(RefCell::new(Vec::new())),
        },
        720.0,
        80.0,
    );
    harness.run();
    for tip in ["Roll back to start", "Step back", "Step forward", "Roll forward to end"] {
        assert!(harness.get_by_label(tip).accesskit_node().is_disabled(), "{tip} should be disabled while a sketch is open");
    }
    click_label(&harness, "Step back");
    harness.run();
    assert!(commands.borrow().is_empty());
}

#[test]
fn timeline_context_menu_rollback() {
    let commands = Rc::new(RefCell::new(Vec::new()));
    let mut harness = timeline_harness(
        TimelineUi {
            tl: Timeline::default(),
            view: sample_timeline(None),
            sketch_active: false,
            dark: true,
            commands: commands.clone(),
            events: Rc::new(RefCell::new(Vec::new())),
        },
        720.0,
        400.0,
    );
    harness.run();
    harness
        .get_by(|n| n.role() == egui::accesskit::Role::Button && n.label() == Some("Box".into()))
        .click_secondary();
    harness.run();
    harness.get_by_label("Roll Back to Here").click();
    harness.run();
    assert!(matches!(commands.borrow().last(), Some(Command::Rollback(Some(1)))));
}

#[test]
fn timeline_click_sets_status_message() {
    let commands = Rc::new(RefCell::new(Vec::new()));
    let mut harness = timeline_harness(
        TimelineUi {
            tl: Timeline::default(),
            view: sample_timeline(None),
            sketch_active: false,
            dark: true,
            commands: commands.clone(),
            events: Rc::new(RefCell::new(Vec::new())),
        },
        720.0,
        80.0,
    );
    harness.run();
    click_label(&harness, "Import");
    harness.run();
    assert_eq!(harness.state_mut().tl.take_status().as_deref(), Some("Import"));
}

#[test]
fn timeline_drag_marker_rollback() {
    let commands = Rc::new(RefCell::new(Vec::new()));
    let mut harness = timeline_harness(
        TimelineUi {
            tl: Timeline::default(),
            view: sample_timeline(None),
            sketch_active: false,
            dark: true,
            commands: commands.clone(),
            events: Rc::new(RefCell::new(Vec::new())),
        },
        720.0,
        80.0,
    );
    harness.run();
    let marker = harness.get_by_label("Rollback marker");
    let start = marker.rect().center();
    let target = harness.get_by_label("Import").rect().center();
    harness.drag_at(start);
    harness.hover_at(target);
    harness.drop_at(target);
    harness.run();
    assert!(matches!(
        commands.borrow().last(),
        Some(Command::Rollback(Some(1)))
    ));
}

#[test]
fn timeline_marker_tooltip_matches_1x() {
    // 1.x: "End of history" when the marker sits after the last feature, else "Rollback marker".
    let commands = Rc::new(RefCell::new(Vec::new()));
    let make = |marker| {
        timeline_harness(
            TimelineUi {
                tl: Timeline::default(),
                view: sample_timeline(marker),
                sketch_active: false,
                dark: true,
                commands: commands.clone(),
                events: Rc::new(RefCell::new(Vec::new())),
            },
            720.0,
            80.0,
        )
    };
    // Marker at the end (no rollback): hovering shows "End of history".
    let mut harness = make(None);
    harness.run();
    let end_marker = harness.get_by_label("Rollback marker").rect().center();
    harness.hover_at(end_marker);
    for t in [0.0, 0.5, 1.0, 1.5, 2.0] {
        harness.input_mut().time = Some(t);
        harness.step();
    }
    assert!(harness.query_by_label("End of history").is_some(), "end marker tooltip");
    assert!(harness.query_by_label("Rollback marker").is_some());

    // Marker mid-history (rolled back to item 1): hovering shows "Rollback marker".
    let mut harness = make(Some(1));
    harness.run();
    let mid_marker = harness.get_by_label("Rollback marker").rect().center();
    harness.hover_at(mid_marker);
    for t in [0.0, 0.5, 1.0, 1.5, 2.0] {
        harness.input_mut().time = Some(t);
        harness.step();
    }
    assert!(harness.query_by_label("End of history").is_none(), "mid marker must not say End of history");
}

#[test]
fn browser_hide_body() {
    let commands = Rc::new(RefCell::new(Vec::new()));
    let mut harness = browser_harness(
        BrowserUi {
            browser: Browser::default(),
            doc_name: "Test".into(),
            bodies: vec![test_body("b1", "Part", false)],
            planes: Vec::new(),
            sketches: Vec::new(),
            selection: Vec::new(),
            origin_visible: true,
            params: Vec::new(),
            dark: true,
            commands: commands.clone(),
            events: Rc::new(RefCell::new(Vec::new())),
        },
        280.0,
        220.0,
    );
    harness.run();
    harness.get_by_label("Part").click_secondary();
    harness.run();
    harness.get_by_label("Hide").click();
    harness.run();
    assert!(matches!(
        commands.borrow().last(),
        Some(Command::BodyProps { id, hidden: Some(true), .. }) if id == "b1"
    ));
}

#[test]
fn browser_rename_body() {
    let commands = Rc::new(RefCell::new(Vec::new()));
    let mut harness = browser_harness(
        BrowserUi {
            browser: Browser::with_rename("b1", "Bracket"),
            doc_name: "Test".into(),
            bodies: vec![test_body("b1", "Part", false)],
            planes: Vec::new(),
            sketches: Vec::new(),
            selection: Vec::new(),
            origin_visible: true,
            params: Vec::new(),
            dark: true,
            commands: commands.clone(),
            events: Rc::new(RefCell::new(Vec::new())),
        },
        320.0,
        220.0,
    );
    harness.run();
    harness.get_by_label("OK").click();
    harness.run();
    assert!(matches!(
        commands.borrow().last(),
        Some(Command::BodyProps { id, name: Some(n), .. }) if id == "b1" && n == "Bracket"
    ));
}

#[test]
fn browser_select_body() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut harness = browser_harness(
        BrowserUi {
            browser: Browser::default(),
            doc_name: "Test".into(),
            bodies: vec![test_body("b1", "Part", false)],
            planes: Vec::new(),
            sketches: Vec::new(),
            selection: Vec::new(),
            origin_visible: true,
            params: Vec::new(),
            dark: true,
            commands: Rc::new(RefCell::new(Vec::new())),
            events: events.clone(),
        },
        280.0,
        220.0,
    );
    harness.run();
    harness.get_by_label("Part").click();
    harness.run();
    assert_eq!(*events.borrow(), vec![UiEvent::Select(vec!["b1".into()])]);
}

#[test]
fn browser_origin_eye() {
    let mut harness = browser_harness(
        BrowserUi {
            browser: Browser::default(),
            doc_name: "Test".into(),
            bodies: vec![test_body("b1", "Part", false)],
            planes: Vec::new(),
            sketches: Vec::new(),
            selection: Vec::new(),
            origin_visible: true,
            params: Vec::new(),
            dark: true,
            commands: Rc::new(RefCell::new(Vec::new())),
            events: Rc::new(RefCell::new(Vec::new())),
        },
        280.0,
        220.0,
    );
    harness.run();
    click_label(&harness, "Hide origin planes");
    harness.run();
    assert_eq!(harness.state_mut().browser.take_origin_toggle(), Some(false));
}

#[test]
fn browser_lists_planes_and_sketches() {
    let mut harness = browser_harness(
        BrowserUi {
            browser: Browser::default(),
            doc_name: "Doc".into(),
            bodies: vec![test_body("b1", "Part", false)],
            planes: vec![PlaneView {
                id: "p1".into(),
                name: "Top".into(),
                origin: [0.0; 3],
                x: [1.0, 0.0, 0.0],
                y: [0.0, 1.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                visible: true,
            }],
            sketches: vec![SketchView { id: "s1".into(), name: "Sketch1".into(), plane: "Top".into(), hidden: false, ..Default::default() }],
            selection: Vec::new(),
            origin_visible: true,
            params: vec![param_view("dx", 5.0), param_view("spare", 1.0)],
            dark: true,
            commands: Rc::new(RefCell::new(Vec::new())),
            events: Rc::new(RefCell::new(Vec::new())),
        },
        280.0,
        280.0,
    );
    harness.run();
    harness.get_by_label("Top");
    harness.get_by_label("Sketch1");
    harness.get_by_label("Parameters");
}

fn fixture(name: &str) -> Vec<u8> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/projects").join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn drain(link: &NativeLink) -> Vec<Update> {
    let end = Instant::now() + Duration::from_secs(60);
    let mut all = Vec::new();
    loop {
        all.extend(link.poll());
        if !link.is_busy() {
            all.extend(link.poll());
            return all;
        }
        assert!(Instant::now() < end, "engine idle timeout");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn browser_basic_fixture_context_colour() {
    let link = NativeLink::new();
    link.send(Command::Open { name: "basic.stlx".into(), bytes: fixture("basic.stlx") });
    let ups = drain(&link);
    let bodies: Vec<BodyView> = ups
        .iter()
        .rev()
        .find_map(|u| if let Update::Bodies(b) = u { Some(b.clone()) } else { None })
        .expect("bodies");
    assert!(!bodies.is_empty());
    let mut harness = browser_harness(
        BrowserUi {
            browser: Browser::default(),
            doc_name: "basic.stlx".into(),
            bodies,
            planes: Vec::new(),
            sketches: Vec::new(),
            selection: Vec::new(),
            origin_visible: true,
            params: vec![param_view("dx", 5.0)],
            dark: true,
            commands: Rc::new(RefCell::new(Vec::new())),
            events: Rc::new(RefCell::new(Vec::new())),
        },
        300.0,
        320.0,
    );
    harness.run();
    let name = harness.state().bodies[0].name.clone();
    harness.get_by_label(name.as_str()).click_secondary();
    harness.run();
    harness.get_by_label("Colour…").click();
    harness.run();
    assert!(harness.state_mut().browser.take_origin_toggle().is_none());
}

#[test]
fn params_edit_expr() {
    let commands = Rc::new(RefCell::new(Vec::new()));
    let mut params = Params::default();
    params.set_open(true);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(800.0, 340.0))
        .with_accessibility_check(false)
        .build_ui_state(
        |ui, st: &mut ParamsUi| st.draw(ui),
        ParamsUi {
            params,
            views: vec![ParamView {
                name: "width".into(),
                expr: "40".into(),
                note: String::new(),
                value: Some(40.0),
                error: None,
                used: false,
            }],
            used: HashSet::new(),
            dark: true,
            commands: commands.clone(),
        },
    );
    harness.run();
    harness
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .find(|n| n.value() == Some("40".to_string()))
        .expect("expr field")
        .click();
    harness.run();
    harness
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .find(|n| n.value() == Some("40".to_string()))
        .expect("expr field")
        .set_text_value_accesskit("50");
    harness.run();
    harness.get_by_label("Apply").click();
    harness.run();
    assert!(matches!(
        commands.borrow().last(),
        Some(Command::SetParams(p)) if p.len() == 1 && p[0].name == "width" && p[0].expr == "50"
    ));
}

#[test]
fn params_resolved_shows_used_marker() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(800.0, 340.0))
        .with_accessibility_check(false)
        .build_ui_state(
        |ui, st: &mut ParamsUi| st.draw(ui),
        ParamsUi {
            params: {
                let mut p = Params::default();
                p.set_open(true);
                p
            },
            views: vec![
                ParamView {
                    name: "width".into(),
                    expr: "40".into(),
                    note: String::new(),
                    value: Some(40.0),
                    error: None,
                    used: false,
                },
                ParamView {
                    name: "height".into(),
                    expr: "=width".into(),
                    note: String::new(),
                    value: Some(40.0),
                    error: None,
                    used: true,
                },
            ],
            used: HashSet::from(["width".into()]),
            dark: true,
            commands: Rc::new(RefCell::new(Vec::new())),
        },
    );
    harness.run();
    harness.get_by_label("= 40 · used");
}

#[test]
fn params_cancel_closes() {
    let commands = Rc::new(RefCell::new(Vec::new()));
    let mut harness = Harness::builder()
        .with_size(egui::vec2(800.0, 340.0))
        .with_accessibility_check(false)
        .build_ui_state(
        |ui, st: &mut ParamsUi| st.draw(ui),
        ParamsUi {
            params: {
                let mut p = Params::default();
                p.set_open(true);
                p
            },
            views: vec![ParamView {
                name: "width".into(),
                expr: "40".into(),
                note: String::new(),
                value: Some(40.0),
                error: None,
                used: false,
            }],
            used: HashSet::new(),
            dark: true,
            commands: commands.clone(),
        },
    );
    harness.run();
    harness.get_by_label("Cancel").click();
    harness.run();
    assert!(!harness.state().params.is_open());
    assert!(commands.borrow().is_empty());
}

#[test]
fn status_and_toasts_visible() {
    let mut toasts = Toasts::default();
    toasts.push(Level::Success, "Saved".into());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(640.0, 120.0))
        .with_accessibility_check(false)
        .build_ui_state(
        move |ui, app: &mut (Toasts, bool)| {
            let (toasts, dark) = app;
            theme::apply(ui.ctx(), &theme::tokens(*dark));
            let tokens = theme::tokens(*dark);
            show_status(ui, &tokens, "Box — last feature in the history");
            show_toasts(ui.ctx(), &tokens, toasts);
        },
        (toasts, true),
    );
    harness.run();
    harness.get_by_label("Saved");
    harness.get_by_label("Box — last feature in the history");
}

#[test]
fn busy_cancel_after_delay() {
    let commands = Rc::new(RefCell::new(Vec::new()));
    let mut busy = Busy::default();
    busy.set_busy(true);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(400.0, 300.0))
        .with_accessibility_check(false)
        .build_ui_state(
        move |ui, app: &mut (Busy, Rc<RefCell<Vec<Command>>>, bool)| {
            let (busy, cmds, dark) = app;
            theme::apply(ui.ctx(), &theme::tokens(*dark));
            let tokens = theme::tokens(*dark);
            show_busy(ui.ctx(), &tokens, busy, &mut cmds.borrow_mut());
        },
        (busy, commands.clone(), true),
    );
    // the overlay waits 350 ms of egui time; the spinner repaints forever, so step instead of run
    harness.input_mut().time = Some(0.0);
    harness.step();
    harness.input_mut().time = Some(0.2);
    harness.step();
    assert!(harness.query_by_label("Cancel").is_none(), "the overlay must not show before the delay");
    harness.input_mut().time = Some(0.4);
    harness.step();
    harness.input_mut().time = Some(0.5);
    harness.step();
    harness.get_by_label("Cancel").click();
    harness.step();
    harness.step();
    assert!(matches!(commands.borrow().as_slice(), [Command::Cancel]));
}

fn menu_item_disabled(h: &Harness<'_, impl Sized>, label: &str) -> bool {
    h.get_by_label(label).accesskit_node().is_disabled()
}

#[test]
fn timeline_menu_move_buttons_follow_position() {
    let mut harness = timeline_harness(
        TimelineUi {
            tl: Timeline::default(),
            view: sample_timeline(None),
            sketch_active: false,
            dark: true,
            commands: Rc::new(RefCell::new(Vec::new())),
            events: Rc::new(RefCell::new(Vec::new())),
        },
        720.0,
        400.0,
    );
    harness.run();
    let button = |h: &Harness<'_, TimelineUi>, name: &str| {
        h.get_by(|n| n.role() == egui::accesskit::Role::Button && n.label() == Some(name.into())).rect().center()
    };
    // the last step cannot move right, the first cannot move left
    let hole = button(&harness, "Hole");
    harness.hover_at(hole);
    harness.get_by(|n| n.role() == egui::accesskit::Role::Button && n.label() == Some("Hole".into())).click_secondary();
    harness.run();
    assert!(menu_item_disabled(&harness, "Move right"));
    assert!(!menu_item_disabled(&harness, "Move left"));
    assert!(!menu_item_disabled(&harness, "Roll Back to Here"));
    // marker at the end: nothing to roll forward
    assert!(menu_item_disabled(&harness, "Roll Forward to End"));
}

#[test]
fn timeline_menu_first_step_cannot_move_left() {
    let mut harness = timeline_harness(
        TimelineUi {
            tl: Timeline::default(),
            view: sample_timeline(Some(1)),
            sketch_active: false,
            dark: true,
            commands: Rc::new(RefCell::new(Vec::new())),
            events: Rc::new(RefCell::new(Vec::new())),
        },
        720.0,
        400.0,
    );
    harness.run();
    harness.get_by(|n| n.role() == egui::accesskit::Role::Button && n.label() == Some("Import".into())).click_secondary();
    harness.run();
    assert!(menu_item_disabled(&harness, "Move left"));
    assert!(!menu_item_disabled(&harness, "Move right"));
    assert!(!menu_item_disabled(&harness, "Roll Forward to End"), "marker is not at the end");
    assert!(!menu_item_disabled(&harness, "Suppress"), "Suppress is available when no sketch is open");
}

#[test]
fn timeline_menu_suppress_sends_set_suppressed() {
    let commands = Rc::new(RefCell::new(Vec::new()));
    let mut harness = timeline_harness(
        TimelineUi {
            tl: Timeline::default(),
            view: sample_timeline(Some(1)),
            sketch_active: false,
            dark: true,
            commands: commands.clone(),
            events: Rc::new(RefCell::new(Vec::new())),
        },
        720.0,
        400.0,
    );
    harness.run();
    harness.get_by(|n| n.role() == egui::accesskit::Role::Button && n.label() == Some("Import".into())).click_secondary();
    harness.run();
    harness.get_by_label("Suppress").click();
    harness.run();
    assert!(
        matches!(commands.borrow().last(), Some(Command::SetSuppressed { suppressed: true, .. })),
        "Suppress must send SetSuppressed with suppressed: true, got {:?}",
        commands.borrow().last().map(|_| "a different command")
    );
}

#[test]
fn timeline_double_click_opens_the_step() {
    let events = Rc::new(RefCell::new(Vec::new()));
    // egui calls two clicks a double-click when they are under 0.3 s apart; the default test frame is 0.25 s
    let mut harness = Harness::builder().with_size(egui::vec2(720.0, 80.0)).with_step_dt(0.05).with_accessibility_check(false).build_ui_state(
        |ui, st: &mut TimelineUi| {
            stl_app::fonts::install(ui.ctx());
            st.draw(ui);
        },
        TimelineUi {
            tl: Timeline::default(),
            view: sample_timeline(None),
            sketch_active: false,
            dark: true,
            commands: Rc::new(RefCell::new(Vec::new())),
            events: events.clone(),
        },
    );
    harness.run();
    let box_node = |h: &Harness<'_, TimelineUi>| {
        h.get_by(|n| n.role() == egui::accesskit::Role::Button && n.label() == Some("Box".into())).rect().center()
    };
    let at = box_node(&harness);
    for _ in 0..2 {
        harness.event(egui::Event::PointerMoved(at));
        harness.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
        harness.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
        harness.step();
    }
    harness.run();
    assert!(events.borrow().contains(&UiEvent::OpenTool("box".into())));
    assert_eq!(harness.state_mut().tl.take_edit_feature().as_deref(), Some("f2"));
}

#[test]
fn timeline_click_on_failed_step_shows_the_error() {
    let mut view = sample_timeline(None);
    view.items[1].error = Some("The cut misses the part".into());
    let mut harness = timeline_harness(
        TimelineUi {
            tl: Timeline::default(),
            view,
            sketch_active: false,
            dark: true,
            commands: Rc::new(RefCell::new(Vec::new())),
            events: Rc::new(RefCell::new(Vec::new())),
        },
        720.0,
        80.0,
    );
    harness.run();
    click_label(&harness, "Box");
    harness.run();
    assert_eq!(harness.state_mut().tl.take_status().as_deref(), Some("Box — The cut misses the part"));
}

#[test]
fn browser_marks_a_body_that_is_not_watertight() {
    let mut bad = test_body("b2", "Scan", false);
    bad.solid = false;
    let mut harness = browser_harness(
        BrowserUi {
            browser: Browser::default(),
            doc_name: "Doc".into(),
            bodies: vec![test_body("b1", "Part", false), bad],
            planes: Vec::new(),
            sketches: Vec::new(),
            selection: Vec::new(),
            origin_visible: true,
            params: Vec::new(),
            dark: true,
            commands: Rc::new(RefCell::new(Vec::new())),
            events: Rc::new(RefCell::new(Vec::new())),
        },
        280.0,
        260.0,
    );
    harness.run();
    harness.get_by_label("needs repair");
    assert_eq!(harness.query_all_by_label("needs repair").count(), 1, "only the broken body is tagged");
    harness.get_by_label("2 bodies");
}

#[test]
fn params_show_the_1x_hint_and_three_decimals() {
    let mut params = Params::default();
    params.set_open(true);
    let mut harness = Harness::builder().with_size(egui::vec2(800.0, 340.0)).with_accessibility_check(false).build_ui_state(
        |ui, st: &mut ParamsUi| st.draw(ui),
        ParamsUi {
            params,
            views: vec![ParamView { name: "r".into(), expr: "1.23456".into(), value: Some(1.23456), ..Default::default() }],
            used: HashSet::new(),
            dark: true,
            commands: Rc::new(RefCell::new(Vec::new())),
        },
    );
    harness.run();
    // web/js/app.js:1157, word for word
    harness.get_by_label("Name a value once and use it in any number: type the name (or =width/2 + 3) in a tool's field. Change a value here and every step that uses it is rebuilt. Lengths are in mm, angles in degrees; a parameter can use others.");
    harness.get_by_label("= 1.235");
    // the list changes in the engine (undo, a new file): the table follows it
    harness.state_mut().views = vec![ParamView { name: "r".into(), expr: "7".into(), value: Some(7.0), ..Default::default() }];
    harness.run();
    harness.get_by_label("= 7");
}

#[test]
fn params_dialog_matches_1x_layout() {
    let long_note = "X shift for the second blob";
    let mut params = Params::default();
    params.set_open(true);
    let mut harness = Harness::builder().with_size(egui::vec2(800.0, 340.0)).with_accessibility_check(false).build_ui_state(
        |ui, st: &mut ParamsUi| st.draw(ui),
        ParamsUi {
            params,
            views: vec![
                ParamView { name: "dx".into(), expr: "=width/2 + 3".into(), note: long_note.into(), value: Some(5.0), ..Default::default() },
                ParamView { name: "spare_length".into(), expr: "1".into(), note: "unused".into(), value: Some(1.0), ..Default::default() },
            ],
            used: HashSet::new(),
            dark: true,
            commands: Rc::new(RefCell::new(Vec::new())),
        },
    );
    harness.run();
    // like 1.x: Cancel then Apply at the bottom right, below "+ Add parameter"
    let add = harness.get_by_label("+ Add parameter").rect();
    let cancel = harness.get_by_label("Cancel").rect();
    let apply = harness.get_by_label("Apply").rect();
    assert!(cancel.max.x < apply.min.x, "Cancel is left of Apply: {cancel:?} {apply:?}");
    assert!(apply.min.y > add.max.y && cancel.min.y > add.max.y, "the action row is below the add button");
    assert!((apply.center().y - cancel.center().y).abs() < 1.0, "one row");
    // the table is as wide as 1.x's card (720 px): Apply ends at its right edge, Add starts at its left
    assert!(apply.max.x - add.min.x > 700.0, "buttons span the card: {add:?} {apply:?}");
    // the cells are as wide as 1.x's, so the text is not cut off
    let cells: Vec<_> = harness.get_all_by_role(egui::accesskit::Role::TextInput).collect();
    let find = |v: &str| cells.iter().find(|n| n.value().as_deref() == Some(v)).unwrap_or_else(|| panic!("no field {v:?}")).rect();
    assert!(find("spare_length").width() >= 150.0, "name cell width {:?}", find("spare_length"));
    assert!(find("=width/2 + 3").width() >= 178.0, "value cell width {:?}", find("=width/2 + 3"));
    assert!(find(long_note).width() >= 174.0, "note cell width {:?}", find(long_note));
    // and the long note really fits in its cell
    let font = egui::FontId::proportional(13.0);
    let need = harness.ctx.fonts_mut(|f| f.layout_no_wrap(long_note.into(), font, egui::Color32::WHITE).size().x);
    assert!(need + 10.0 <= find(long_note).width(), "{long_note:?} needs {need}px");
}

/// The dialog as a picture (snapshot), dark and light; skipped without a GPU adapter.
#[test]
fn params_dialog_snapshots_dark_light() {
    let Some(gpu) = gpu::headless() else {
        eprintln!("no GPU adapter: skipped params snapshots");
        return;
    };
    for (dark, name) in [(true, "params_dialog_dark"), (false, "params_dialog_light")] {
        let renderer = wgpu_renderer::WgpuRenderer::new(gpu.render_state.clone());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(780.0, 300.0))
            .with_render_cursor(false)
            .with_accessibility_check(false)
            .renderer(renderer)
            .build_ui_state(
                |ui, st: &mut ParamsUi| {
                    ui.painter().rect_filled(ui.max_rect(), 0.0, theme::tokens(st.dark).panel);
                    ui.add_space(14.0);
                    st.draw(ui);
                },
                ParamsUi {
                    params: {
                        let mut p = Params::default();
                        p.set_open(true);
                        p
                    },
                    views: vec![
                        ParamView { name: "dx".into(), expr: "5".into(), note: "X shift".into(), value: Some(5.0), ..Default::default() },
                        ParamView { name: "spare".into(), expr: "1".into(), note: "unused".into(), value: Some(1.0), ..Default::default() },
                    ],
                    used: HashSet::from(["dx".to_string()]),
                    dark,
                    commands: Rc::new(RefCell::new(Vec::new())),
                },
            );
        harness.run();
        gpu::snapshot_2d(&mut harness, &gpu, name);
        let errors: Vec<String> = harness.take_snapshot_results().into_inner().iter().map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "{}", errors.join("
"));
    }
}

#[test]
fn success_toast_uses_the_default_toast_background() {
    let tk = theme::tokens(true);
    assert_eq!(toast_colours(Level::Success, &tk).0, tk.toast);
    assert_eq!(toast_colours(Level::Info, &tk).0, tk.toast);
    assert_eq!(toast_colours(Level::Warning, &tk).0, tk.toast_warn);
    assert_eq!(toast_colours(Level::Error, &tk).0, tk.danger);
}

#[test]
fn status_line_with_long_multibyte_text_does_not_panic() {
    let text = "é—".repeat(400);
    let mut harness = Harness::builder().with_size(egui::vec2(300.0, 60.0)).with_accessibility_check(false).build_ui_state(
        |ui, text: &mut String| {
            let tokens = theme::tokens(true);
            show_status(ui, &tokens, text);
        },
        text,
    );
    harness.run();
}

fn panel_picture(dark: bool) -> Option<image::RgbaImage> {
    let gpu = gpu::headless()?;
    let renderer = wgpu_renderer::WgpuRenderer::new(gpu.render_state.clone());
    let commands = Rc::new(RefCell::new(Vec::new()));
    let mut harness = Harness::builder()
        .with_size(egui::vec2(720.0, 96.0))
        .with_render_cursor(false)
        .with_accessibility_check(false)
        .renderer(renderer)
        .build_ui_state(
            |ui, st: &mut TimelineUi| st.draw(ui),
            TimelineUi {
                tl: Timeline::default(),
                view: sample_timeline(Some(2)),
                sketch_active: false,
                dark,
                commands,
                events: Rc::new(RefCell::new(Vec::new())),
            },
        );
    harness.run();
    harness.render().ok()
}

#[test]
fn panel_snapshots_dark_light() {
    let Some(dark) = panel_picture(true) else {
        eprintln!("no GPU adapter: skipped panel snapshots");
        return;
    };
    let Some(light) = panel_picture(false) else {
        eprintln!("no GPU adapter: skipped panel snapshots");
        return;
    };
    assert_ne!(dark.as_raw(), light.as_raw(), "timeline dark vs light");
    assert!(dark.width() > 100 && dark.height() > 40);
}

// ------------------------------------------------------------------ params.js with the real engine

fn last_params(ups: &[Update]) -> Vec<ParamView> {
    ups.iter().rev().find_map(|u| if let Update::Params(p) = u { Some(p.clone()) } else { None }).expect("params update")
}

fn last_timeline_view(ups: &[Update]) -> TimelineView {
    ups.iter().rev().find_map(|u| if let Update::Timeline(t) = u { Some(t.clone()) } else { None }).expect("timeline update")
}

fn param(name: &str, expr: &str) -> stl_engine::params::Param {
    stl_engine::params::Param { name: name.into(), expr: expr.into(), note: String::new() }
}

/// Real engine: a box sized by `width` and `height`; the Parameters table sends the commands.
#[test]
fn params_undo_and_removing_a_used_parameter_with_the_engine() {
    let link = NativeLink::new();
    link.send(Command::Open { name: "basic.stlx".into(), bytes: fixture("basic.stlx") });
    drain(&link);
    link.send(Command::SetParams(vec![param("width", "40"), param("height", "width / 4 + 5")]));
    link.send(Command::AddFeature {
        kind: "primitive".into(),
        params: serde_json::json!({"shape": "box", "size": ["=width", 10.0, "=height*2"]}),
        index: None,
    });
    let ups = drain(&link);
    let views = last_params(&ups);
    assert_eq!(views.iter().map(|p| p.value).collect::<Vec<_>>(), vec![Some(40.0), Some(15.0)]);
    assert!(views.iter().all(|p| p.used), "both parameters are used by the new box");
    assert!(last_timeline_view(&ups).items.iter().find(|i| i.kind == "primitive").unwrap().error.is_none());

    let commands = Rc::new(RefCell::new(Vec::new()));
    let mut params = Params::default();
    params.set_open(true);
    let mut harness = Harness::builder().with_size(egui::vec2(800.0, 340.0)).with_accessibility_check(false).build_ui_state(
        |ui, st: &mut ParamsUi| st.draw(ui),
        ParamsUi {
            params,
            used: views.iter().filter(|p| p.used).map(|p| p.name.clone()).collect(),
            views: views.clone(),
            dark: true,
            commands: commands.clone(),
        },
    );
    harness.run();

    // width = 60 changes the live values and, once applied, the engine's value
    harness
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .find(|n| n.value() == Some("40".to_string()))
        .expect("width field")
        .set_text_value_accesskit("60");
    harness.run();
    harness.get_by_label("Apply").click();
    harness.run();
    let sent: Vec<Command> = commands.borrow_mut().drain(..).collect();
    assert!(matches!(sent.last(), Some(Command::SetParams(p)) if p[0].expr == "60"), "no confirm when nothing used is removed");
    for c in sent {
        link.send(c);
    }
    let after = last_params(&drain(&link));
    assert_eq!(after[0].value, Some(60.0));
    assert_eq!(after[1].value, Some(20.0));

    // undo puts the old value back
    link.send(Command::Undo);
    let undone = last_params(&drain(&link));
    assert_eq!(undone[0].expr, "40");
    assert_eq!(undone[0].value, Some(40.0));
    assert_eq!(undone[1].value, Some(15.0));

    // removing `height`, which the box uses, asks first (1.x wording) and sends nothing until OK
    {
        let st = harness.state_mut();
        st.views = undone.clone();
        st.params.set_open(false);
        st.params.set_open(true);
    }
    harness.run();
    harness.get_all_by_label("×").nth(1).expect("remove height").click();
    harness.run();
    harness.get_by_label("Apply").click();
    harness.run();
    harness.get_by_label("height is used in the timeline. Remove it anyway? Those steps will show an error until you fix them.");
    assert!(commands.borrow().is_empty(), "nothing is sent before OK");
    // Cancel keeps the table (without the row) and still sends nothing
    harness.get_by_label("Cancel").click();
    harness.run();
    assert!(harness.query_by_label("Apply").is_some() && commands.borrow().is_empty());
    harness.get_by_label("Apply").click();
    harness.run();
    harness.get_by_label("OK").click();
    harness.run();
    let sent: Vec<Command> = commands.borrow_mut().drain(..).collect();
    assert!(matches!(sent.last(), Some(Command::SetParams(p)) if p.len() == 1 && p[0].name == "width"));
    for c in sent {
        link.send(c);
    }
    let ups = drain(&link);
    let tl = last_timeline_view(&ups);
    let err = tl.items.iter().find(|i| i.kind == "primitive").unwrap().error.clone().unwrap_or_else(|| panic!("the box step shows an error: {tl:?}"));
    assert!(err.starts_with("Parameter:"), "{err}");
    assert_eq!(last_params(&ups).len(), 1);
}
