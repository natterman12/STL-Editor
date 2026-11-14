//! Touch and tap input for sketch mode and model mode (1.x tablet taps in `viewer.js` / `tap.js`).

use egui::{Event, InputOptions, Modifiers, PointerButton, Pos2, TouchPhase};
use web_time::Instant;

use crate::viewport::Viewport;

use super::{SketchInput, SketchSession};

/// 1.x `sketcher.js` pointerup: movement over 5 px is a drag, not a tap.
const TAP_MOVE_PX: f32 = 5.0;

/// Tracks the last tap for model-mode double-tap when egui has not classified the click yet.
#[derive(Default, Debug)]
pub struct TouchState {
    last_tap: Option<(Instant, Pos2)>,
    down: Option<Pos2>,
    /// When the next pointer-up is the second half of a double-tap (sketch profile in model mode).
    pending_double: bool,
}

/// What model mode should do after a double-tap in the viewport (Task 2 / `app.rs`).
#[derive(Clone, Debug, PartialEq)]
pub enum TouchModelAction {
    /// 1.x `quickSketchOnFace`: double-tap a flat face while no tool is open.
    QuickSketch { plane: serde_json::Value },
    /// Double-tap a sketch profile shown on the part in model mode (`editFeature` on a sketch).
    EditSketch { feature_id: String },
}

/// Whether this release is the second tap of a double-tap (uses [`InputOptions`] like egui's pointer).
pub fn classify_tap(state: &mut TouchState, pos: Pos2, now: Instant, opts: &InputOptions) -> bool {
    let delay = opts.max_double_click_delay;
    let dist = opts.max_click_dist;
    let double = state.last_tap.is_some_and(|(t, p)| {
        now.duration_since(t).as_secs_f64() < delay && p.distance(pos) < dist
    });
    state.last_tap = Some((now, pos));
    double
}

/// After [`classify_tap`] returns true, pick the model action (sketch profile wins over face).
pub fn model_action(double: bool, face_plane: Option<serde_json::Value>, sketch_feature: Option<String>) -> Option<TouchModelAction> {
    if !double {
        return None;
    }
    if let Some(id) = sketch_feature {
        return Some(TouchModelAction::EditSketch { feature_id: id });
    }
    face_plane.map(|plane| TouchModelAction::QuickSketch { plane })
}

/// Turn viewport pointer / touch events into [`SketchInput`] while a sketch is open.
/// `pointer` / `opts` come from the current frame (`ui.input`); double-tap uses [`egui::PointerState::button_double_clicked`] with [`InputOptions`] limits.
pub fn input(
    ev: &Event,
    vp: &Viewport,
    session: &SketchSession,
    state: &mut TouchState,
    mods: Modifiers,
    pointer: &egui::PointerState,
    opts: &InputOptions,
) -> Option<SketchInput> {
    let frame = &session.frame;
    match ev {
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers,
            ..
        } => {
            state.down = Some(*pos);
            let world = vp.ray_to_plane(*pos, frame)?;
            let uv = session.to_uv(world);
            Some(SketchInput::Down { uv, pos: *pos, mods: *modifiers })
        }
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            ..
        } => {
            let down_pos = state.down.take()?;
            if down_pos.distance(*pos) > TAP_MOVE_PX {
                return None;
            }
            let world = vp.ray_to_plane(*pos, frame)?;
            let uv = session.to_uv(world);
            if state.pending_double {
                state.pending_double = false;
                return Some(SketchInput::DoubleClick { uv, pos: *pos });
            }
            if pointer.button_double_clicked(PointerButton::Primary) {
                return Some(SketchInput::DoubleClick { uv, pos: *pos });
            }
            let now = Instant::now();
            if classify_tap(state, *pos, now, opts) {
                return Some(SketchInput::DoubleClick { uv, pos: *pos });
            }
            Some(SketchInput::Up { uv, pos: *pos })
        }
        Event::PointerMoved(pos) => {
            let down_pos = state.down?;
            if down_pos.distance(*pos) <= TAP_MOVE_PX {
                return None;
            }
            let world = vp.ray_to_plane(*pos, frame)?;
            let uv = session.to_uv(world);
            Some(SketchInput::Move { uv, pos: *pos, mods })
        }
        Event::Touch {
            phase: TouchPhase::Start,
            pos,
            ..
        } => {
            state.down = Some(*pos);
            let world = vp.ray_to_plane(*pos, frame)?;
            let uv = session.to_uv(world);
            Some(SketchInput::Down { uv, pos: *pos, mods })
        }
        Event::Touch {
            phase: TouchPhase::End,
            pos,
            ..
        } => {
            let down_pos = state.down.take()?;
            if down_pos.distance(*pos) > TAP_MOVE_PX {
                return None;
            }
            let world = vp.ray_to_plane(*pos, frame)?;
            let uv = session.to_uv(world);
            let now = Instant::now();
            if classify_tap(state, *pos, now, opts) {
                return Some(SketchInput::DoubleClick { uv, pos: *pos });
            }
            Some(SketchInput::Up { uv, pos: *pos })
        }
        Event::Touch {
            phase: TouchPhase::Move,
            pos,
            ..
        } => {
            let down_pos = state.down?;
            if down_pos.distance(*pos) <= TAP_MOVE_PX {
                return None;
            }
            let world = vp.ray_to_plane(*pos, frame)?;
            let uv = session.to_uv(world);
            Some(SketchInput::Move { uv, pos: *pos, mods })
        }
        _ => None,
    }
}

/// Call when a sketch-step profile was hit on the first tap of a would-be double-tap, so the second tap edits that sketch instead of starting a face sketch.
pub fn note_sketch_profile_hit(state: &mut TouchState) {
    state.pending_double = true;
}

/// Test helper: map a plane hit without a viewport ray (unit tests only).
pub fn input_at_uv(
    ev: &Event,
    uv: [f64; 2],
    pos: Pos2,
    state: &mut TouchState,
    mods: Modifiers,
    pointer: &egui::PointerState,
    opts: &InputOptions,
) -> Option<SketchInput> {
    match ev {
        Event::PointerButton {
            button: PointerButton::Primary,
            pressed: true,
            ..
        } => Some(SketchInput::Down { uv, pos, mods }),
        Event::PointerButton {
            button: PointerButton::Primary,
            pressed: false,
            ..
        } => {
            if pointer.button_double_clicked(PointerButton::Primary) {
                return Some(SketchInput::DoubleClick { uv, pos });
            }
            let now = Instant::now();
            if classify_tap(state, pos, now, opts) {
                return Some(SketchInput::DoubleClick { uv, pos });
            }
            Some(SketchInput::Up { uv, pos })
        }
        _ => None,
    }
}