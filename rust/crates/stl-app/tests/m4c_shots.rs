//! M4c side-by-side 2.0 screenshots (`dev/out/m4c-shots/2x/<theme>/<state>.png`).
//!
//! Ignored by default: needs a GPU. Run with:
//! `cargo test -p stl-app --test m4c_shots -- --ignored`
//!
//! 1.x pairs come from `node dev/m4c_shots/capture_m4c_shots.mjs`.
//! Each state is set up the same way that script sets up 1.x (same Box, Top face, clicks).
#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Event, PointerButton, Pos2};
use egui_kittest::Harness;
use serde_json::json;
use stl_app::app::StlApp;
use stl_app::engine_link::native::NativeLink;
use stl_app::engine_link::{Command, EngineLink, Update};
use stl_app::platform::{Pending, PickedFile, Platform};
use stl_app::sketch::{dims, Entity, SketchDialog, SketchEffect};
use stl_app::ui::ThemePref;
use stl_app::viewport::renderer::Headless;

#[path = "support/wgpu_renderer.rs"]
mod wgpu_renderer;

const SIZE: egui::Vec2 = egui::vec2(1280.0, 800.0);

const STATES: &[&str] = &[
    "plane_prompt",
    "sketch_empty",
    "draw_line",
    "draw_arc",
    "draw_rect",
    "draw_circle",
    "draw_polygon",
    "draw_slot",
    "sketch_under",
    "sketch_full",
    "sketch_over",
    "modify_dialog",
    "text_dialog",
    "useedge_refs",
    "missing_ref",
    "inch_dims",
    "finish_extrude",
];

const THEMES: &[&str] = &["dark", "light"];

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn out_dir() -> PathBuf {
    repo().join("dev/out/m4c-shots")
}

fn state_ready(_state: &str) -> bool {
    true
}

fn settle(h: &mut Harness<'static, StlApp>, link: &NativeLink) {
    let end = Instant::now() + Duration::from_secs(60);
    loop {
        h.run_steps(1);
        if !link.is_busy() {
            h.run_steps(1);
            if !link.is_busy() {
                h.state_mut().viewport.flush();
                h.run_steps(1);
                return;
            }
        }
        assert!(Instant::now() < end, "engine busy");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn sketch_changed(h: &mut Harness<'static, StlApp>, link: &NativeLink) {
    h.state_mut().sketch_apply(vec![SketchEffect::Changed]);
    settle(h, link);
}

fn wait_sketch_closed(h: &mut Harness<'static, StlApp>, link: &NativeLink) {
    let end = Instant::now() + Duration::from_secs(60);
    loop {
        settle(h, link);
        if h.state().sketch().is_none() {
            return;
        }
        assert!(Instant::now() < end, "sketch did not close");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// 1.x `newBox`: 40×20×10 corner box on XY, then iso + fit.
fn new_box(h: &mut Harness<'static, StlApp>, link: &NativeLink) {
    h.state_mut().link.send(Command::AddFeature {
        kind: "primitive".into(),
        params: json!({
            "shape": "box",
            "length": 40,
            "width": 20,
            "height": 10,
            "anchor": "corner",
            "plane": "XY",
            "operation": "new",
            "offset": [0, 0, 0],
            "rotate": [0, 0, 0],
            "targets": []
        }),
        index: None,
        name: None,
    });
    settle(h, link);
    assert!(!h.state().bodies.is_empty(), "box body");
    if let Some(id) = h.state().bodies.first().map(|b| b.id.clone()) {
        h.state_mut().shell.selection = vec![id.clone()];
        h.state_mut().viewport.set_selection(&[id]);
    }
    h.state_mut().viewport.set_view("iso");
    h.state_mut().viewport.flush();
    h.run_steps(2);
}

fn top_face(h: &Harness<'static, StlApp>) -> serde_json::Value {
    let bid = h.state().bodies[0].id.clone();
    json!({
        "origin": [0.0, 0.0, 10.0],
        "normal": [0.0, 0.0, 1.0],
        "xdir": [1.0, 0.0, 0.0],
        "label": "Top",
        "body": bid
    })
}

/// 1.x `enterTopSketch` + often `setView('top')`.
fn enter_top_sketch(h: &mut Harness<'static, StlApp>, link: &NativeLink) {
    let face = top_face(h);
    h.state_mut().enter_sketch(Some(face), None);
    settle(h, link);
    h.state_mut().viewport.set_view("top");
    h.state_mut().viewport.flush();
    h.run_steps(2);
    assert!(h.state().sketch().is_some(), "top sketch open");
}

fn sketch_uv_screen(h: &Harness<'static, StlApp>, uv: [f64; 2]) -> Pos2 {
    let s = h.state().sketch().expect("sketch open");
    h.state().viewport.world_to_screen(s.to_3(uv))
}

fn click_at(h: &mut Harness<'static, StlApp>, at: Pos2) {
    h.event(Event::PointerMoved(at));
    h.step();
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Default::default() });
    h.step();
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Default::default() });
    h.run_steps(2);
}

fn dblclick_at(h: &mut Harness<'static, StlApp>, at: Pos2) {
    for _ in 0..2 {
        h.event(Event::PointerMoved(at));
        h.step();
        h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Default::default() });
        h.step();
        h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Default::default() });
        h.step();
    }
    h.run_steps(2);
}

fn sketch_click_uv(h: &mut Harness<'static, StlApp>, uv: [f64; 2]) {
    h.state_mut().viewport.flush();
    h.run_steps(1);
    let at = sketch_uv_screen(h, uv);
    click_at(h, at);
}

fn sketch_move_uv(h: &mut Harness<'static, StlApp>, uv: [f64; 2]) {
    h.state_mut().viewport.flush();
    h.run_steps(1);
    let at = sketch_uv_screen(h, uv);
    h.event(Event::PointerMoved(at));
    h.run_steps(3);
}

/// 1.x `seedRectSketch`: rect (5,5)–(25,15) drawn with the rect tool, optional width 30 +
/// height 12. Returns the rect's id: the tool gives it a fresh `e…` id, as 1.x's `newId()`
/// does (a hard-coded `r0` would be a part edge's id on a face sketch, which never moves).
fn seed_rect_sketch(h: &mut Harness<'static, StlApp>, link: &NativeLink, dims: bool) -> String {
    enter_top_sketch(h, link);
    h.state_mut().sketch_set_tool("rect");
    h.run_steps(2);
    sketch_click_uv(h, [5.0, 5.0]);
    sketch_click_uv(h, [25.0, 15.0]);
    settle(h, link);
    let id = {
        let s = h.state().sketch().expect("sketch open");
        let e = s.entities.iter().find(|e| matches!(e, Entity::Rect(_))).expect("rect drawn");
        e.id().expect("rect id").to_owned()
    };
    if dims {
        let m = h.state_mut().sketch_mode_mut().unwrap();
        m.s.constraints.push(serde_json::from_value(json!({
            "id": "dw", "type": "hdistance",
            "a": {"e": id, "k": "c0"}, "b": {"e": id, "k": "c1"},
            "off": -4, "value": 30
        })).unwrap());
        m.s.constraints.push(serde_json::from_value(json!({
            "id": "dh", "type": "vdistance",
            "a": {"e": id, "k": "c1"}, "b": {"e": id, "k": "c2"},
            "off": 4, "value": 12
        })).unwrap());
    }
    sketch_changed(h, link);
    h.state_mut().viewport.set_view("top");
    h.state_mut().viewport.flush();
    h.run_steps(2);
    id
}

#[derive(Clone, Default)]
struct FakePlatform {
    picks: Arc<std::sync::Mutex<Vec<Option<PickedFile>>>>,
    saved: Arc<std::sync::Mutex<Vec<(String, Vec<u8>)>>>,
    settings: Arc<std::sync::Mutex<std::collections::BTreeMap<String, String>>>,
}

impl Platform for FakePlatform {
    fn open_file(&self, _filters: &[(&str, &[&str])]) -> Pending<Option<PickedFile>> {
        Pending::ready(self.picks.lock().unwrap().pop().unwrap_or(None))
    }
    fn save_file(&self, suggested: &str, bytes: Vec<u8>) -> Pending<Result<bool, String>> {
        self.saved.lock().unwrap().push((suggested.to_string(), bytes));
        Pending::ready(Ok(true))
    }
    fn now_iso_utc(&self) -> String {
        "2026-11-14T00:00:00".into()
    }
    fn is_web(&self) -> bool {
        false
    }
    fn setting(&self, key: &str) -> Option<String> {
        self.settings.lock().unwrap().get(key).cloned()
    }
    fn set_setting(&self, key: &str, value: &str) {
        self.settings.lock().unwrap().insert(key.to_owned(), value.to_owned());
    }
}

struct SharedLink {
    inner: Arc<NativeLink>,
}
impl EngineLink for SharedLink {
    fn send(&self, c: Command) {
        self.inner.send(c);
    }
    fn poll(&self) -> Vec<Update> {
        self.inner.poll()
    }
    fn restart(&self) {
        self.inner.restart();
    }
}

fn apply_state(h: &mut Harness<'static, StlApp>, link: &NativeLink, state: &str) {
    // 1.x `applyState1x` always `newBox` first.
    new_box(h, link);

    match state {
        "plane_prompt" => {
            h.state_mut().begin_sketch(None);
            h.run_steps(4);
        }
        "sketch_empty" => {
            enter_top_sketch(h, link);
            h.run_steps(3);
        }
        "draw_line" | "draw_arc" | "draw_rect" | "draw_circle" | "draw_polygon" | "draw_slot" => {
            enter_top_sketch(h, link);
            let tool_name = state.strip_prefix("draw_").unwrap();
            let key = if tool_name == "arc" { "arc3" } else { tool_name };
            h.state_mut().sketch_set_tool(key);
            h.run_steps(2);
            let (start, end) = match tool_name {
                "line" => ([0.0, 0.0], [18.0, 6.0]),
                "arc" => ([-8.0, 0.0], [8.0, 0.0]),
                "rect" => ([2.0, 2.0], [22.0, 14.0]),
                "circle" => ([10.0, 8.0], [16.0, 8.0]),
                "polygon" => ([-12.0, 6.0], [-4.0, 6.0]),
                _ => ([-5.0, -8.0], [12.0, -8.0]), // slot
            };
            sketch_click_uv(h, start);
            if tool_name == "arc" {
                sketch_click_uv(h, [0.0, 8.0]);
            }
            sketch_move_uv(h, end);
            settle(h, link);
            h.run_steps(2);
        }
        "sketch_under" => {
            seed_rect_sketch(h, link, false);
            h.run_steps(3);
        }
        "sketch_full" => {
            let rid = seed_rect_sketch(h, link, true);
            {
                let m = h.state_mut().sketch_mode_mut().unwrap();
                m.s.constraints.push(serde_json::from_value(json!({
                    "id": "cf", "type": "coincident",
                    "a": {"e": rid, "k": "c0"},
                    "b": {"e": "_o", "k": "p"}
                })).unwrap());
            }
            sketch_changed(h, link);
            {
                // 1.x reaches "Fully defined" with the rect at 30 × 12 from the origin.
                let m = h.state().sketch_mode().unwrap();
                assert_eq!(m.state.msg.as_deref(), Some("Fully defined"), "sketch_full solves");
                let r = m.s.entities.iter().find(|e| e.id() == Some(rid.as_str())).unwrap();
                let Entity::Rect(r) = r else { panic!("rect") };
                assert!((r.w.abs() - 30.0).abs() < 1e-6 && (r.h.abs() - 12.0).abs() < 1e-6, "rect 30 × 12, got {} × {}", r.w, r.h);
            }
            h.state_mut().viewport.set_view("top");
            h.run_steps(3);
        }
        "sketch_over" => {
            let rid = seed_rect_sketch(h, link, true);
            {
                let m = h.state_mut().sketch_mode_mut().unwrap();
                m.s.constraints.push(serde_json::from_value(json!({
                    "id": "cover", "type": "hdistance",
                    "a": {"e": rid, "k": "c0"}, "b": {"e": rid, "k": "c1"},
                    "value": 18
                })).unwrap());
            }
            sketch_changed(h, link);
            h.state_mut().viewport.set_view("top");
            h.run_steps(3);
        }
        "modify_dialog" => {
            seed_rect_sketch(h, link, true);
            // Paint dims once so a label exists, then double-click it (1.x openModify / dim double-click).
            h.run_steps(3);
            h.state_mut().viewport.flush();
            let at = {
                let s = h.state().sketch().unwrap();
                let vp = &h.state().viewport;
                dims::label_screen(s, vp, 0).expect("width dim label")
            };
            dblclick_at(h, at);
            h.run_steps(3);
            if h.state().sketch_mode().is_some_and(|m| m.panel.modify.is_none()) {
                // Fallback: same as 1.x `openModify` when the hit misses.
                h.state_mut().sketch_apply(vec![SketchEffect::Dialog(SketchDialog::DimValue(0))]);
                h.run_steps(3);
            }
            assert!(
                h.state().sketch_mode().is_some_and(|m| m.panel.modify.is_some()),
                "on-canvas modify box should be open"
            );
            h.state_mut().viewport.set_view("top");
            h.run_steps(2);
        }
        "text_dialog" => {
            enter_top_sketch(h, link);
            h.state_mut().sketch_set_tool("text");
            h.run_steps(2);
            sketch_click_uv(h, [5.0, 5.0]);
            h.run_steps(4);
            use egui::accesskit::Role;
            use egui_kittest::kittest::Queryable;
            assert!(h.query_by_label("Add text").is_some(), "text dialog should be open");
            // Default content is "Text"; set to "Hi" like 1.x (multiline may be TextInput or Label/TextRun).
            if let Some(n) = h.get_all_by_role(Role::TextInput).find(|n| n.value().as_deref() == Some("Text")) {
                n.click();
                n.set_text_value_accesskit("Hi");
            } else if let Some(n) = h.get_all_by_value("Text").next() {
                n.click();
                n.set_text_value_accesskit("Hi");
            } else {
                panic!("text dialog content field not found");
            }
            h.run_steps(3);
        }
        "useedge_refs" => {
            enter_top_sketch(h, link);
            h.state_mut().sketch_set_tool("sk_useedge");
            h.run_steps(2);
            sketch_click_uv(h, [40.0, 10.0]);
            settle(h, link);
            h.run_steps(3);
        }
        "missing_ref" => {
            // 1.x: seedRectSketch + dim to a part edge → finish → grow box → reopen sketch.
            let rid = seed_rect_sketch(h, link, true);
            let edge_id = {
                let m = h.state_mut().sketch_mode_mut().unwrap();
                m.s.refs
                    .iter()
                    .find(|r| !r.missing && matches!(r.ent, Entity::Line(_)))
                    .and_then(|r| r.ent.id().map(str::to_owned))
                    .expect("part-edge line after refs")
            };
            {
                let m = h.state_mut().sketch_mode_mut().unwrap();
                m.s.constraints.push(serde_json::from_value(json!({
                    "id": "dref", "type": "hdistance",
                    "a": {"e": rid, "k": "c0"}, "b": {"e": edge_id, "k": "p1"},
                    "value": 6, "off": 8, "slide": 0
                })).unwrap());
            }
            sketch_changed(h, link);
            h.state_mut().finish_sketch(false);
            wait_sketch_closed(h, link);
            let box_id = h
                .state()
                .timeline_view
                .items
                .iter()
                .find(|i| i.kind == "primitive")
                .map(|i| i.id.clone())
                .expect("box step");
            let sk_id = h
                .state()
                .timeline_view
                .items
                .iter()
                .rev()
                .find(|i| i.kind == "sketch")
                .map(|i| i.id.clone())
                .expect("sketch step");
            let mut box_params = h.state().timeline_view.items.iter().find(|i| i.id == box_id).unwrap().params.clone();
            box_params["length"] = json!(55);
            box_params["width"] = json!(28);
            h.state_mut().link.send(Command::EditFeature {
                id: box_id,
                params: box_params,
                rollback_after: None,
            });
            settle(h, link);
            assert!(h.state_mut().edit_feature(&sk_id), "reopen sketch");
            settle(h, link);
            h.state_mut().viewport.set_view("top");
            h.state_mut().viewport.flush();
            // Capture promptly so a missing-ref toast (if match_refs marked any) is still up.
            h.run_steps(3);
        }
        "inch_dims" => {
            h.state_mut().set_units_inch(true); // 1.x `setUnits('in')`
            seed_rect_sketch(h, link, true);
            h.run_steps(3);
        }
        "finish_extrude" => {
            seed_rect_sketch(h, link, false);
            h.state_mut().finish_sketch(true);
            settle(h, link);
            wait_sketch_closed(h, link);
            let end = Instant::now() + Duration::from_secs(30);
            while h.state().tool().map(|t| t.def.key) != Some("extrude") {
                settle(h, link);
                assert!(Instant::now() < end, "extrude tool did not open");
            }
            h.state_mut().tool_set_value("distance", json!(5.0));
            h.state_mut().tool_set_value("operation", json!("join"));
            h.state_mut().viewport.set_view("iso");
            h.state_mut().viewport.flush();
            // Wait for the Extrude preview mesh (1.x waits ~800ms + idle).
            let end = Instant::now() + Duration::from_secs(30);
            loop {
                settle(h, link);
                h.state_mut().viewport.flush();
                h.run_steps(2);
                if h.state().viewport.preview_meshes().next().is_some() {
                    break;
                }
                if Instant::now() >= end {
                    eprintln!("finish_extrude: preview mesh not ready; capturing anyway");
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            h.run_steps(3);
        }
        _ => panic!("unhandled ready state {state}"),
    }
}

fn capture_one(gpu: &Headless, state: &str, theme: &str) -> PathBuf {
    let dark = theme != "light";
    let link = Arc::new(NativeLink::new());
    let ctx = egui::Context::default();
    let mut app = StlApp::new(&ctx, Box::new(FakePlatform::default()), Box::new(SharedLink { inner: link.clone() }));
    app.viewport.init_gpu(&gpu.render_state);
    let renderer = wgpu_renderer::WgpuRenderer::new(gpu.render_state.clone());
    let mut init = false;
    let mut h = Harness::builder()
        .with_size(SIZE)
        .with_render_cursor(false)
        .with_accessibility_check(false)
        .renderer(renderer)
        .build_ui_state(
            move |ui, app: &mut StlApp| {
                if !init {
                    init = true;
                    stl_app::fonts::install(ui.ctx());
                    app.set_theme(ui.ctx(), if dark { ThemePref::Dark } else { ThemePref::Light });
                }
                app.layout(ui);
            },
            app,
        );
    settle(&mut h, &link);
    apply_state(&mut h, &link, state);
    h.run_steps(3);
    h.run();
    let img = h.render().unwrap_or_else(|e| panic!("render {state}/{theme}: {e}"));
    let out = out_dir().join("2x").join(theme).join(format!("{state}.png"));
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    img.save(&out).unwrap_or_else(|e| panic!("save {}: {e}", out.display()));
    eprintln!("wrote {}", out.display());
    out
}

#[test]
fn m4c_shots_skip_list_documents_unbuilt_states() {
    let skipped: Vec<&str> = STATES.iter().copied().filter(|s| !state_ready(s)).collect();
    let ready: Vec<&str> = STATES.iter().copied().filter(|s| state_ready(s)).collect();
    eprintln!("m4c_shots: ready {} ({}); skip {} ({})", ready.len(), ready.join(", "), skipped.len(), skipped.join(", "));
    assert_eq!(ready.len(), STATES.len());
    assert!(skipped.is_empty(), "all m4c states should capture: {skipped:?}");
}

#[test]
#[ignore = "writes GPU screenshots to dev/out/m4c-shots/2x; run with --ignored"]
fn m4c_shots_2x() {
    let Some(gpu) = Headless::new() else {
        panic!("no GPU adapter for m4c_shots");
    };
    std::fs::create_dir_all(out_dir()).unwrap();
    // states_differ: within one theme, no two states may give identical images.
    let mut identical = Vec::new();
    for theme in THEMES {
        let mut seen: Vec<(&str, Vec<u8>)> = Vec::new();
        for state in STATES {
            let bytes = std::fs::read(capture_one(&gpu, state, theme)).unwrap();
            if let Some((other, _)) = seen.iter().find(|(_, b)| *b == bytes) {
                identical.push(format!("{theme}: {other} and {state} are identical"));
            }
            seen.push((state, bytes));
        }
    }
    assert!(identical.is_empty(), "states_differ FAILED: {identical:?}");
}
