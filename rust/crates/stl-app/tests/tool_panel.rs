//! The tool panel, its plain fields and the preview flow (milestone 4b, task 2).
//! Rows of rust/tests/UI_PARITY.md: the form and preview parts of tools.js / app.js (`openTool`,
//! `renderForm`, `fieldHTML`, `onFormChange`, `schedulePreview`, `runPreview`, `commitTool`).
//!
//! Tests with a name starting `registry_` use the real tool table (Task 1); the others build a small
//! tool of their own so they do not depend on it.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use serde_json::{json, Value};
use stl_app::app::StlApp;
use stl_app::engine_link::{Command, EngineLink, ParamView, TimelineItem, TimelineView, Update};
use stl_app::theme;
use stl_app::tools::fields::number::{commit_number, expr_text, parse_len, shown_number};
use stl_app::tools::panel::{OpenTool, PanelEnv};
use stl_app::tools::preview::MsgKind;
use stl_app::tools::{tool, Ctx, Field, FieldKind, ToolDef, ToolFlags, Values};
use stl_app::ui::ThemePref;
use stl_app::viewport::{PreviewMesh, PreviewPack};
use stl_kernel::MeshArrays;
use web_time::{Duration, Instant};

#[path = "support/gpu.rs"]
mod gpu;
#[path = "support/wgpu_renderer.rs"]
mod wgpu_renderer;

// ------------------------------------------------------------------ a link that records

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

fn app_with(rec: &Rec) -> StlApp {
    let mut app = StlApp::new(&egui::Context::default(), Box::new(stl_app::platform::NullPlatform), Box::new(rec.clone()));
    app.shell.theme_pref = ThemePref::Dark;
    app
}

// ------------------------------------------------------------------ a tool of our own

fn show_depth(v: &Values) -> bool {
    v.get("shape").and_then(Value::as_str) != Some("ball")
}

fn defaults(_: &Ctx) -> Values {
    let Value::Object(m) = json!({"shape": "box", "width": 10.0, "depth": 6.0, "move": [0.0, 0.0, 0.0], "fill": true, "note": "", "angle": 30.0}) else { unreachable!() };
    m
}

fn to_params(v: &Values, _: &Ctx) -> Value {
    json!({"shape": v["shape"], "width": v["width"], "depth": v["depth"], "move": v["move"], "fill": v["fill"], "angle": v["angle"], "_hidden": 1})
}

fn from_params(p: Value) -> Values {
    match p {
        Value::Object(m) => m,
        _ => Values::new(),
    }
}

fn on_change(key: &str, _v: &Values, _: &Ctx) -> Option<Values> {
    (key == "shape").then(|| {
        let Value::Object(m) = json!({"depth": 99.0}) else { unreachable!() };
        m
    })
}

fn opts(items: &[(&str, &str)]) -> Vec<(String, String, Option<String>)> {
    items.iter().map(|(a, b)| (a.to_string(), b.to_string(), None)).collect()
}

fn test_tool(no_live: bool) -> &'static ToolDef {
    Box::leak(Box::new(ToolDef {
        key: "t_box",
        label: "Test Box",
        icon: "primitive",
        kind: "primitive",
        fields: vec![
            Field { key: "shape", label: "Shape", kind: FieldKind::Select { options: opts(&[("box", "Box"), ("ball", "Ball")]), grouped: false }, help: None, show: None, placeholder: None },
            Field { key: "width", label: "Width", kind: FieldKind::Number { unit: Some("mm") }, help: Some("The size across."), show: None, placeholder: None },
            Field { key: "depth", label: "Depth", kind: FieldKind::Number { unit: Some("mm") }, help: None, show: Some(show_depth), placeholder: None },
            Field { key: "angle", label: "Angle", kind: FieldKind::Number { unit: Some("°") }, help: None, show: None, placeholder: None },
            Field { key: "move", label: "Move", kind: FieldKind::Vec3 { unit: Some("mm") }, help: None, show: None, placeholder: None },
            Field { key: "fill", label: "Fill", kind: FieldKind::Bool, help: None, show: None, placeholder: None },
            Field { key: "note", label: "Note", kind: FieldKind::Text { placeholder: None }, help: None, show: None, placeholder: None },
        ],
        hint: Some("Test hint."),
        extra: None,
        flags: ToolFlags { no_live_preview: no_live, ..Default::default() },
        defaults,
        to_params,
        from_params,
        on_change: Some(on_change),
    }))
}

fn later(ms: u64) -> Instant {
    Instant::now() + Duration::from_millis(ms)
}

/// Let the debounce pass: the request is noticed on one frame and runs a few hundred ms later.
fn settle(app: &mut StlApp, ms: u64) {
    app.run_previews(later(ms));
    app.run_previews(later(ms + 400));
}

fn harness(app: StlApp, w: f32, h: f32) -> Harness<'static, StlApp> {
    let mut init = false;
    Harness::builder().with_size(egui::vec2(w, h)).with_accessibility_check(false).build_ui_state(
        move |ui, a: &mut StlApp| {
            if !init {
                init = true;
                stl_app::fonts::install(ui.ctx());
                a.set_theme(ui.ctx(), ThemePref::Dark);
            }
            a.layout(ui)
        },
        app,
    )
}

fn timeline_with(items: Vec<TimelineItem>, marker: Option<usize>) -> TimelineView {
    TimelineView { items, marker, can_undo: false, can_redo: false }
}

fn step(id: &str, kind: &str, params: Value) -> TimelineItem {
    TimelineItem { id: id.into(), kind: kind.into(), label: kind.into(), active: true, params, ..Default::default() }
}

fn pack_of(id: &str) -> PreviewPack {
    PreviewPack {
        meshes: vec![PreviewMesh { id: id.into(), mesh: Arc::new(MeshArrays { positions: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0], triangles: vec![0, 1, 2] }) }],
        ..Default::default()
    }
}

fn answer(rec: &Rec, seq: u64, msg: Option<&str>, error: Option<&str>) {
    rec.inbox.lock().unwrap().push(Update::Preview { seq, pack: if error.is_some() { Default::default() } else { pack_of("pv") }, msg: msg.map(str::to_owned), error: error.map(str::to_owned), warn: false, result: Value::Null });
}

fn shown_keys(t: &OpenTool) -> Vec<&'static str> {
    t.shown_fields().map(|f| f.key).collect()
}

// ------------------------------------------------------------------ tests

#[test]
fn opens_with_defaults() {
    let rec = Rec::default();
    let mut app = app_with(&rec);
    app.open_tool_def(test_tool(false), None, None);
    let t = app.tool().expect("the tool is open");
    assert_eq!(Value::Object(t.values.clone()), json!({"shape": "box", "width": 10.0, "depth": 6.0, "move": [0.0, 0.0, 0.0], "fill": true, "note": "", "angle": 30.0}));
    assert_eq!(t.title(), "Test Box");
    assert_eq!(app.shell.hint, "Test hint.");
    assert_eq!(app.shell.active_tool.as_deref(), Some("t_box"));
    // the first preview runs at once, with the keys that start with _ left out
    app.run_previews(Instant::now());
    let p = rec.previews();
    assert_eq!(p.len(), 1, "{:?}", rec.commands());
    assert_eq!((p[0].0, p[0].1.as_str()), (1, "primitive"));
    assert!(p[0].2.get("_hidden").is_none());
    assert_eq!(p[0].2["width"], json!(10.0));
}

#[test]
fn show_rules_hide_fields() {
    let rec = Rec::default();
    let mut app = app_with(&rec);
    app.open_tool_def(test_tool(false), None, None);
    assert!(shown_keys(app.tool().unwrap()).contains(&"depth"));
    app.tool_set_value("shape", json!("ball"));
    let t = app.tool().unwrap();
    assert!(!shown_keys(t).contains(&"depth"));
    // and the form really leaves it out
    let mut h = harness(app, 900.0, 700.0);
    h.run_steps(3);
    assert!(h.query_by_label("Width").is_some());
    assert!(h.query_by_label("Depth").is_none());
}

#[test]
fn on_change_patches_values() {
    let rec = Rec::default();
    let mut app = app_with(&rec);
    app.open_tool_def(test_tool(false), None, None);
    app.tool_set_value("shape", json!("ball"));
    assert_eq!(app.tool().unwrap().values["depth"], json!(99.0), "the onChange patch is applied");
    assert_eq!(app.tool_mut().unwrap().take_changed(), vec!["shape".to_string()]);
}

#[test]
fn registry_on_change_patches_values() {
    // hole presets and thread sizes come from tools.js
    let hole = tool("hole").expect("waiting on Task 1: the tool registry has no `hole` yet");
    let rec = Rec::default();
    let mut app = app_with(&rec);
    app.open_tool_def(hole, None, None);
    let before = app.tool().unwrap().values.clone();
    let preset = hole.fields.iter().find(|f| matches!(f.kind, FieldKind::Select { .. }) && f.key.contains("preset")).map(|f| f.key).expect("a preset field");
    let FieldKind::Select { options, .. } = &hole.fields.iter().find(|f| f.key == preset).unwrap().kind else { unreachable!() };
    let other = options.iter().find(|(k, _, _)| Some(k.as_str()) != before.get(preset).and_then(Value::as_str)).expect("another preset").0.clone();
    app.tool_set_value(preset, json!(other));
    assert_ne!(app.tool().unwrap().values, before, "choosing a preset changes other values");

    let thread = tool("thread").expect("waiting on Task 1: no `thread`");
    app.open_tool_def(thread, None, None);
    let before = app.tool().unwrap().values.clone();
    let size = thread.fields.iter().find(|f| f.key.contains("size")).map(|f| f.key).expect("a size field");
    let FieldKind::Select { options, .. } = &thread.fields.iter().find(|f| f.key == size).unwrap().kind else { unreachable!() };
    let other = options.iter().find(|(k, _, _)| Some(k.as_str()) != before.get(size).and_then(Value::as_str)).unwrap().0.clone();
    app.tool_set_value(size, json!(other));
    assert_ne!(app.tool().unwrap().values, before, "choosing a thread size changes other values");
}

#[test]
fn inch_mode_shows_inches_sends_mm() {
    // 25.4 mm is "1" in inch mode; typing "1" there is 25.4 mm; the engine always gets mm
    assert_eq!(shown_number(&json!(25.4), true, true), "1");
    assert_eq!(shown_number(&json!(25.4), true, false), "25.4");
    assert_eq!(parse_len("1", true), Some(25.4));
    assert_eq!(parse_len("1 3/8", true).map(|v| (v * 1000.0).round() / 1000.0), Some(34.925));
    assert_eq!(parse_len("1/4", true).map(|v| (v * 1000.0).round() / 1000.0), Some(6.35));
    assert_eq!(commit_number("2", true, true), json!(50.8));
    assert_eq!(commit_number("12", false, true), json!("12"));
    let rec = Rec::default();
    let mut app = app_with(&rec);
    app.shell.inch = true;
    app.open_tool_def(test_tool(false), None, None);
    app.tool_set_value("width", commit_number("1", true, true));
    settle(&mut app, 1000);
    let last = rec.previews().pop().unwrap();
    assert_eq!(last.2["width"], json!(25.4));
    // and the box shows inches
    let mut h = harness(app, 900.0, 700.0);
    h.run_steps(3);
    assert!(h.query_all_by_value("1").count() >= 1, "the width box shows 1 (inch)");
}

#[test]
fn expression_fields() {
    assert!(expr_text("=width/2"));
    assert!(expr_text("width"));
    assert!(!expr_text("12"));
    assert!(!expr_text("1 3/8"));
    assert!(!expr_text("25mm"));
    assert_eq!(commit_number("width/2", true, false), json!("=width/2"));
    let rec = Rec::default();
    let mut app = app_with(&rec);
    app.param_views = vec![ParamView { name: "width".into(), expr: "20".into(), value: Some(20.0), ..Default::default() }];
    app.open_tool_def(test_tool(false), None, None);
    // a valid expression reaches the engine as the string, and OK goes through
    app.tool_set_value("width", commit_number("=width/2", true, false));
    settle(&mut app, 1000);
    assert_eq!(rec.previews().pop().unwrap().2["width"], json!("=width/2"));
    app.tool_ok();
    assert!(app.tool().is_some(), "OK waits for the engine: a refusal would leave the tool open");
    rec.inbox.lock().unwrap().push(Update::Timeline(Default::default()));
    app.poll();
    assert!(app.tool().is_none(), "the engine took it");
    assert!(rec.commands().iter().any(|c| matches!(c, Command::AddFeature { params, .. } if params["width"] == json!("=width/2"))));
    // an invalid one shows 1.x's text and blocks OK
    app.open_tool_def(test_tool(false), None, None);
    app.tool_set_value("width", commit_number("=nope", true, false));
    let adds = rec.commands().iter().filter(|c| matches!(c, Command::AddFeature { .. })).count();
    app.tool_ok();
    let t = app.tool().expect("still open");
    let m = t.msg.as_ref().expect("a message");
    assert_eq!((m.kind, m.text.as_str()), (MsgKind::Err, "there's no parameter \"nope\" (in =nope)"));
    assert_eq!(rec.commands().iter().filter(|c| matches!(c, Command::AddFeature { .. })).count(), adds);
}

#[test]
fn stale_preview_dropped() {
    let rec = Rec::default();
    let mut app = app_with(&rec);
    app.open_tool_def(test_tool(false), None, None);
    app.run_previews(Instant::now()); // seq 1
    app.tool_set_value("width", json!(12.0));
    settle(&mut app, 1000); // seq 2
    let seqs: Vec<u64> = rec.previews().iter().map(|p| p.0).collect();
    assert_eq!(seqs, vec![1, 2]);
    // the older answer arrives after the newer one was asked for: nothing is drawn
    answer(&rec, 1, Some("old"), None);
    app.poll();
    assert!(app.tool().unwrap().msg.is_none());
    assert_eq!(app.viewport.preview_meshes().count(), 0);
    // the newest lands
    answer(&rec, 2, Some("new"), None);
    app.poll();
    assert_eq!(app.tool().unwrap().msg.as_ref().unwrap().text, "new");
    app.viewport.flush(); // prepared on a worker thread
    assert_eq!(app.viewport.preview_meshes().count(), 1);
    // after Cancel a late preview draws nothing, and the engine was told
    settle(&mut app, 2000);
    app.tool_set_value("width", json!(13.0));
    settle(&mut app, 3000);
    app.tool_cancel();
    assert_eq!(app.viewport.preview_meshes().count(), 0);
    assert!(rec.commands().iter().any(|c| matches!(c, Command::ClearPreview)));
    answer(&rec, rec.previews().last().unwrap().0, Some("late"), None);
    app.poll();
    assert_eq!(app.viewport.preview_meshes().count(), 0);
    assert!(app.tool().is_none());
    // and not on the next tool either
    app.open_tool_def(test_tool(false), None, None);
    answer(&rec, rec.previews().last().unwrap().0, Some("late"), None);
    app.poll();
    assert!(app.tool().unwrap().msg.is_none());
}

#[test]
fn no_live_preview_button() {
    let rec = Rec::default();
    let mut app = app_with(&rec);
    app.open_tool_def(test_tool(true), None, None);
    app.run_previews(Instant::now());
    app.tool_set_value("width", json!(15.0));
    settle(&mut app, 2000);
    assert!(rec.previews().is_empty(), "no preview without the button");
    let mut h = harness(app, 900.0, 700.0);
    h.run_steps(3);
    h.get_by_label("Preview").click();
    h.run_steps(3);
    let p = rec.previews();
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].2["width"], json!(15.0));
}

#[test]
fn ok_adds_feature() {
    let rec = Rec::default();
    let mut app = app_with(&rec);
    app.open_tool_def(test_tool(false), None, None);
    app.tool_set_value("width", json!(12.0));
    app.tool_ok();
    rec.inbox.lock().unwrap().push(Update::Timeline(Default::default()));
    app.poll();
    assert!(app.tool().is_none());
    assert_eq!(app.shell.active_tool, None);
    let adds: Vec<_> = rec.commands().into_iter().filter(|c| matches!(c, Command::AddFeature { .. })).collect();
    assert_eq!(adds.len(), 1);
    let Command::AddFeature { kind, params, index, .. } = &adds[0] else { unreachable!() };
    assert_eq!((kind.as_str(), *index), ("primitive", None));
    assert_eq!(params["width"], json!(12.0));
    assert!(params.get("_hidden").is_none());
    // editing a step sends EditFeature and puts the rollback marker back
    let mut app = app_with(&rec);
    app.timeline_view = timeline_with(vec![step("f1", "primitive", json!({"shape": "box", "width": 5.0, "depth": 6.0, "move": [0, 0, 0], "fill": true, "angle": 1.0})), step("f2", "primitive", json!({}))], Some(2));
    app.open_tool_def(test_tool(false), Some(("f1".into(), app.timeline_view.items[0].params.clone())), None);
    assert_eq!(app.tool().unwrap().title(), "Edit Test Box");
    app.tool_ok();
    let cmds = rec.commands();
    let at = cmds.iter().rposition(|c| matches!(c, Command::EditFeature { id, .. } if id == "f1")).expect("EditFeature");
    assert!(matches!(&cmds[at], Command::EditFeature { rollback_after: Some(Some(2)), .. }), "one transaction: the edit carries the marker: {:?}", &cmds[at..]);
    assert!(!cmds[at..].iter().any(|c| matches!(c, Command::Rollback(_))), "no separate Rollback after OK: {:?}", &cmds[at..]);
}

#[test]
fn cancel_restores() {
    let rec = Rec::default();
    let mut app = app_with(&rec);
    app.timeline_view = timeline_with(vec![step("f1", "primitive", json!({"shape": "box"})), step("f2", "primitive", json!({}))], Some(2));
    app.open_tool_def(test_tool(false), Some(("f1".into(), json!({"shape": "box"}))), None);
    assert!(matches!(rec.commands().last(), Some(Command::Rollback(Some(0)))), "the timeline rolls back to the step");
    app.tool_cancel();
    let cmds = rec.commands();
    assert!(cmds.iter().any(|c| matches!(c, Command::ClearPreview)));
    assert!(matches!(cmds.last(), Some(Command::Rollback(Some(2)))), "{cmds:?}");
    assert!(app.tool().is_none());
    // Cancel of a new tool restores nothing
    let n = rec.commands().len();
    app.open_tool_def(test_tool(false), None, None);
    app.tool_cancel();
    assert!(!rec.commands()[n..].iter().any(|c| matches!(c, Command::Rollback(_))));
}

#[test]
fn open_tool_frame_under_budget() {
    let rec = Rec::default();
    let mut app = app_with(&rec);
    app.open_tool_def(test_tool(false), None, None);
    let mut h = harness(app, 1280.0, 800.0);
    let mut worst = StdDuration::ZERO;
    for _ in 0..30 {
        let t0 = std::time::Instant::now();
        h.step();
        worst = worst.max(t0.elapsed());
    }
    // the budgets are for a release build; a debug build is several times slower
    let limit = if cfg!(debug_assertions) { 250.0 } else { 16.7 };
    assert!(worst.as_secs_f64() * 1000.0 < limit, "slowest frame {:.2} ms", worst.as_secs_f64() * 1000.0);
}

/// The panel as a picture, dark and light; skipped without a GPU adapter.
#[test]
fn tool_panel_snapshots_dark_light() {
    let Some(gpu) = gpu::headless() else {
        eprintln!("no GPU adapter: skipped tool panel snapshots");
        return;
    };
    struct Ui {
        tool: OpenTool,
        dark: bool,
        fonts: bool,
    }
    for (dark, name) in [(true, "tool_panel_dark"), (false, "tool_panel_light")] {
        let renderer = wgpu_renderer::WgpuRenderer::new(gpu.render_state.clone());
        let mut h = Harness::builder()
            .with_size(egui::vec2(330.0, 520.0))
            .with_render_cursor(false)
            .with_accessibility_check(false)
            .renderer(renderer)
            .build_ui_state(
                |ui, st: &mut Ui| {
                    if !st.fonts {
                        st.fonts = true;
                        stl_app::fonts::install(ui.ctx());
                        theme::apply(ui.ctx(), &theme::tokens(st.dark));
                    }
                    let tk = theme::tokens(st.dark);
                    ui.painter().rect_filled(ui.max_rect(), 0.0, tk.bg);
                    let ctx = Ctx { bodies: &[], planes: &[], sketches: &[], features: &[], selection: &[], last_sketch: None, sketch_on_body: false, inch: false };
                    let params = BTreeMap::new();
                    let ws = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(330.0, 560.0));
                    let env = PanelEnv { tk: &tk, inch: false, params: &params, ctx: &ctx, ws };
                    let c = ui.ctx().clone();
                    st.tool.show(&c, &env);
                },
                {
                    let mut tool = OpenTool::new(test_tool(false), defaults(&Ctx { bodies: &[], planes: &[], sketches: &[], features: &[], selection: &[], last_sketch: None, sketch_on_body: false, inch: false }), None, None);
                    tool.values.insert("width".into(), json!("=w/2"));
                    Ui { tool, dark, fonts: false }
                },
            );
        h.run();
        gpu::snapshot_2d(&mut h, &gpu, name);
        let errors: Vec<String> = h.take_snapshot_results().into_inner().iter().map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "{}", errors.join("\n"));
    }
}
