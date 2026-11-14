//! Task 4: sketch text dialog and touch drawing.

use std::collections::BTreeMap;

use egui::{Event, InputOptions, Modifiers, PointerButton, Pos2};
use serde_json::json;
use stl_app::sketch::draw;
use stl_app::sketch::text::{self, TextDialogState, TextForm};
use stl_app::sketch::touch::{self, TouchModelAction, TouchState};
use stl_app::sketch::{SketchDialog, SketchEffect, SketchInput, SketchSession, ToolCx};
use stl_app::viewport::PlaneFrame;
use web_time::Instant;

fn frame() -> PlaneFrame {
    PlaneFrame {
        origin: [0.0, 0.0, 0.0],
        x: [1.0, 0.0, 0.0],
        y: [0.0, 1.0, 0.0],
        normal: [0.0, 0.0, 1.0],
    }
}

fn session(seed: &str) -> SketchSession {
    let mut s = SketchSession::new(frame(), json!("xy"), None, None, vec![], vec![], vec![], None);
    s.seed = seed.into();
    s
}

fn sketch_vp() -> stl_app::viewport::Viewport {
    let mut vp = stl_app::viewport::Viewport::new();
    vp.set_view_size(1000.0, 800.0);
    vp.look_at_plane(&frame(), Some(50.0));
    vp.camera_mut().finish_flight();
    vp
}

fn tap(vp: &stl_app::viewport::Viewport, s: &SketchSession, st: &mut TouchState, pos: Pos2, opts: &InputOptions) -> Vec<SketchInput> {
    let pointer = egui::PointerState::default();
    let mut out = Vec::new();
    for ev in [
        Event::PointerMoved(pos),
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ] {
        if let Some(i) = touch::input(&ev, vp, s, st, Modifiers::NONE, &pointer, opts) {
            out.push(i);
        }
    }
    out
}

#[test]
fn text_sends_sketch_op_text() {
    let s = session("g");
    let form = TextForm {
        text: "Hi".into(),
        font: "DejaVu Sans".into(),
        size: 10.0,
        bold: false,
        italic: false,
        spacing: 0.0,
        align: "left".into(),
        angle: 0.0,
        x: 0.0,
        y: 0.0,
    };
    let data = text::sketch_op_data(&s, &form, None);
    let want = json!({
        "op": "text",
        "seed": "g",
        "entities": [],
        "text": "Hi",
        "font": "DejaVu Sans",
        "size": 10,
        "bold": false,
        "italic": false,
        "spacing": 0,
        "x": 0,
        "y": 0,
        "align": "left",
        "angle": 0
    });
    assert_eq!(data, want);
}

#[test]
fn text_edit_replaces_group() {
    let s = session("g");
    let form = TextForm {
        text: "Hi".into(),
        font: "DejaVu Sans".into(),
        size: 10.0,
        bold: false,
        italic: false,
        spacing: 0.0,
        align: "left".into(),
        angle: 0.0,
        x: 0.0,
        y: 0.0,
    };
    let data = text::sketch_op_data(&s, &form, Some("tg"));
    assert_eq!(data["group"], json!("tg"));
    let mut state = TextDialogState::default();
    let meta = json!({
        "text": "Hi",
        "font": "DejaVu Sans",
        "size": 10,
        "bold": false,
        "italic": false,
        "spacing": 0,
        "align": "left",
        "angle": 0,
        "x": 0,
        "y": 0
    });
    text::open_edit(&mut state, "tg", &meta, false);
    let data2 = text::sketch_op_data(&s, &state.form, state.edit_group.as_deref());
    assert_eq!(data2["group"], json!("tg"));
}

#[test]
fn text_height_non_number_toast() {
    let mut state = TextDialogState::default();
    state.form.text = "Hi".into();
    state.form.size = 10.0;
    state.bufs.insert("size".into(), "not-a-number".into());
    assert_eq!(text::validate_for_commit(&state), Err("Enter a text height."));
}

#[test]
fn text_tool_switches_to_select() {
    let mut s = session("abc");
    s.tool = "text".into();
    let vp = sketch_vp();
    let mut out = Vec::new();
    let mut tool = text::tool_for("text").unwrap();
    let params = BTreeMap::new();
    {
        let mut cx = ToolCx {
            s: &mut s,
            vp: &vp,
            inch: false,
            params: &params,
            out: &mut out,
        };
        assert!(tool.input(
            &mut cx,
            &SketchInput::Up {
                uv: [1.0, 1.0],
                pos: Pos2::new(10.0, 20.0),
            },
        ));
    }
    assert_eq!(s.tool, "select");
    s.tool = "text".into();
    {
        let mut cx = ToolCx {
            s: &mut s,
            vp: &vp,
            inch: false,
            params: &params,
            out: &mut out,
        };
        tool.sketch_op_result(&mut cx, "text", &json!({"added": ["eg1"], "ok": true}));
    }
    assert_eq!(s.tool, "select");
}

#[test]
fn tap_draws_line() {
    let opts = InputOptions::default();
    let pointer = egui::PointerState::default();
    let mut state = TouchState::default();
    let uv = [1.0, 2.0];
    let pos = Pos2::new(100.0, 100.0);
    let down = touch::input_at_uv(
        &Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        uv,
        pos,
        &mut state,
        Modifiers::NONE,
        &pointer,
        &opts,
    );
    assert!(matches!(down, Some(SketchInput::Down { uv: u, .. }) if u == uv));
    let up = touch::input_at_uv(
        &Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
        uv,
        pos,
        &mut state,
        Modifiers::NONE,
        &pointer,
        &opts,
    );
    assert!(matches!(up, Some(SketchInput::Up { uv: u, .. }) if u == uv));
}

#[test]
fn double_tap_enters_sketch() {
    let opts = InputOptions::default();
    let mut state = TouchState::default();
    let pos = Pos2::new(50.0, 50.0);
    let t0 = Instant::now();
    assert!(!touch::classify_tap(&mut state, pos, t0, &opts));
    let t1 = t0 + std::time::Duration::from_millis(120);
    assert!(touch::classify_tap(&mut state, pos, t1, &opts));
    let plane = json!({"origin": [0, 0, 10], "normal": [0, 0, 1], "xdir": [1, 0, 0], "label": "Face"});
    let act = touch::model_action(true, Some(plane.clone()), None);
    assert_eq!(act, Some(TouchModelAction::QuickSketch { plane }));
}

#[test]
fn touch_input_rect_by_taps() {
    let opts = InputOptions::default();
    let vp = sketch_vp();
    let mut s = session("g");
    s.tool = "rect".into();
    let mut st = TouchState::default();
    let mut tool = draw::tool_for("rect").expect("rect tool from Task 3");
    let params = BTreeMap::new();
    let corners = [[-10.0, -5.0], [10.0, 5.0]];
    for uv in corners {
        let pos = vp.world_to_screen(s.to_3(uv));
        let inputs = tap(&vp, &s, &mut st, pos, &opts);
        for inp in inputs {
            let mut out = Vec::new();
            let mut cx = ToolCx {
                s: &mut s,
                vp: &vp,
                inch: false,
                params: &params,
                out: &mut out,
            };
            tool.input(&mut cx, &inp);
        }
    }
    assert_eq!(s.entities.len(), 1);
    assert!(matches!(&s.entities[0], stl_sketch::Entity::Rect(_)));
}

#[test]
fn touch_input_tap_produces_up() {
    let opts = InputOptions::default();
    let vp = sketch_vp();
    let s = session("g");
    let mut st = TouchState::default();
    let pos = vp.world_to_screen([0.0, 0.0, 0.0]);
    let events = tap(&vp, &s, &mut st, pos, &opts);
    assert!(events.iter().any(|e| matches!(e, SketchInput::Down { .. })));
    assert!(events.iter().any(|e| matches!(e, SketchInput::Up { .. })));
}

#[test]
fn text_tool_opens_dialog_with_placement() {
    let mut s = session("abc");
    let vp = sketch_vp();
    let mut out = Vec::new();
    let mut tool = text::tool_for("text").unwrap();
    let params = BTreeMap::new();
    let mut cx = ToolCx {
        s: &mut s,
        vp: &vp,
        inch: false,
        params: &params,
        out: &mut out,
    };
    assert!(tool.input(
        &mut cx,
        &SketchInput::Up {
            uv: [3.5, -2.0],
            pos: Pos2::new(10.0, 20.0),
        },
    ));
    assert!(out.iter().any(|e| matches!(e, SketchEffect::Dialog(SketchDialog::Text { group: None }))));
    assert_eq!(text::take_pending_uv(), Some([3.5, -2.0]));
}