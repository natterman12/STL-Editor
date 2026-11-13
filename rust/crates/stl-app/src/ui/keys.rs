//! Keyboard shortcuts: a port of `App.onKey` in `web/js/app.js`, key by key.
//!
//! [`map_key`] is the pure rule table (the same order as the JavaScript); [`handle`] reads the
//! key presses of one frame from egui and returns the actions. The app turns each action into a
//! `Command`, a `UiEvent`, a file action or a call on the part of the window that owns it.

use egui::{Key, Modifiers};

use crate::engine_link::Command;
use crate::ui::files::FileAction;
use crate::ui::UiEvent;

/// One key press, as the sketcher and the wall editor receive it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyPress {
    pub key: Key,
    pub mods: Modifiers,
}

/// What the window looks like right now (the JavaScript reads `this.…` for these).
#[derive(Clone, Copy, Debug, Default)]
pub struct KeyState {
    /// A text box has focus (`inField`). [`handle`] fills this from egui.
    pub in_field: bool,
    /// The focused box is one of the sketch's length/angle boxes (`dyn-a`, `dyn-b`).
    pub dyn_field_focused: bool,
    pub sketcher_active: bool,
    /// The Support Fins wall editor is drawing, and has points waiting.
    pub walls_active: bool,
    pub walls_pending: bool,
    /// A tool panel is open (`this.tool`).
    pub tool_open: bool,
    /// The open tool has the move/rotate gizmo.
    pub tool_gizmo_move: bool,
    /// The window is in a mode other than modelling (`this.mode !== 'model'`, e.g. Info).
    pub mode_not_model: bool,
    pub selection_nonempty: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum KeyAction {
    Open,
    Import,
    New,
    Save,
    SaveAs,
    /// Focus the tool search (Ctrl+K, or `/`).
    FocusSearch,
    /// The sketcher gets the key. The app tells it to `preventDefault` when it used it.
    Sketcher(KeyPress),
    /// The wall editor uses this key (finish, cancel or undo a point).
    Walls(KeyPress),
    /// W / R with the move tool: switch the gizmo (`rotate` true for R).
    Gizmo { rotate: bool },
    /// Close any open tool first, then undo.
    Undo,
    Redo,
    /// Escape with a tool or a mode open: cancel it.
    PanelCancel,
    /// Escape with nothing open: clear the selection.
    ClearSelection,
    /// Enter with a tool open: OK.
    PanelOk,
    /// Open a tool by its name.
    Tool(&'static str),
    BeginSketch,
    Fit,
    ToggleCut,
    View(&'static str),
}

impl KeyAction {
    pub fn command(&self) -> Option<Command> {
        match self {
            KeyAction::Undo => Some(Command::Undo),
            KeyAction::Redo => Some(Command::Redo),
            _ => None,
        }
    }

    pub fn event(&self) -> Option<UiEvent> {
        match self {
            KeyAction::Tool(t) => Some(UiEvent::OpenTool((*t).to_string())),
            KeyAction::ClearSelection => Some(UiEvent::Select(Vec::new())),
            _ => None,
        }
    }

    pub fn file_action(&self) -> Option<FileAction> {
        match self {
            KeyAction::Open => Some(FileAction::Open),
            KeyAction::Import => Some(FileAction::Import),
            KeyAction::Save => Some(FileAction::Save),
            KeyAction::SaveAs => Some(FileAction::SaveAs),
            _ => None,
        }
    }
}

fn letter(key: Key) -> Option<char> {
    Some(match key {
        Key::A => 'a',
        Key::B => 'b',
        Key::C => 'c',
        Key::D => 'd',
        Key::E => 'e',
        Key::F => 'f',
        Key::G => 'g',
        Key::H => 'h',
        Key::I => 'i',
        Key::J => 'j',
        Key::K => 'k',
        Key::L => 'l',
        Key::M => 'm',
        Key::N => 'n',
        Key::O => 'o',
        Key::P => 'p',
        Key::Q => 'q',
        Key::R => 'r',
        Key::S => 's',
        Key::T => 't',
        Key::U => 'u',
        Key::V => 'v',
        Key::W => 'w',
        Key::X => 'x',
        Key::Y => 'y',
        Key::Z => 'z',
        _ => return None,
    })
}

fn view_for(key: Key) -> Option<&'static str> {
    Some(match key {
        Key::Num1 => "front",
        Key::Num2 => "back",
        Key::Num3 => "left",
        Key::Num4 => "right",
        Key::Num5 => "top",
        Key::Num6 => "bottom",
        Key::Num0 => "iso",
        _ => return None,
    })
}

/// The rule table. Same order as `App.onKey`: file keys and search first (they work in text
/// boxes too), then the sketcher, then nothing while a text box has focus, then the rest.
pub fn map_key(key: Key, mods: Modifiers, st: &KeyState) -> Option<KeyAction> {
    // JavaScript's `mod(e)`: Ctrl, or Cmd on a Mac.
    let ctrl = mods.ctrl || mods.mac_cmd;
    let l = letter(key);
    if ctrl {
        match l {
            Some('s') => return Some(if mods.shift { KeyAction::SaveAs } else { KeyAction::Save }),
            Some('o') => return Some(KeyAction::Open),
            Some('i') => return Some(KeyAction::Import),
            Some('n') => return Some(KeyAction::New),
            Some('k') => return Some(KeyAction::FocusSearch),
            _ => {}
        }
    }
    // `k === '/'` is false when Shift is down (the key then reads '?').
    if key == Key::Slash && !mods.shift && !st.in_field {
        return Some(KeyAction::FocusSearch);
    }
    let press = KeyPress { key, mods };
    if st.sketcher_active {
        if st.in_field && !st.dyn_field_focused {
            return None;
        }
        return Some(KeyAction::Sketcher(press));
    }
    if st.in_field {
        return None;
    }
    if st.walls_active {
        let used = match key {
            Key::Enter | Key::Escape => st.walls_pending,
            Key::Backspace => true,
            _ => ctrl && l == Some('z'),
        };
        if used {
            return Some(KeyAction::Walls(press));
        }
    }
    if st.tool_gizmo_move && !ctrl {
        if let Some(c @ ('w' | 'r')) = l {
            return Some(KeyAction::Gizmo { rotate: c == 'r' });
        }
    }
    if ctrl && l == Some('z') {
        return Some(KeyAction::Undo);
    }
    if ctrl && (l == Some('y') || (mods.shift && l == Some('z'))) {
        return Some(KeyAction::Redo);
    }
    // As in 1.x, Ctrl+Z is tested first, so Ctrl+Shift+Z undoes (the Shift+Z redo clause is
    // never reached there either).
    if key == Key::Escape {
        return Some(if st.tool_open || st.mode_not_model { KeyAction::PanelCancel } else { KeyAction::ClearSelection });
    }
    if key == Key::Enter && st.tool_open {
        return Some(KeyAction::PanelOk);
    }
    if key == Key::Delete && st.selection_nonempty && !st.tool_open {
        return Some(KeyAction::Tool("delete"));
    }
    if !ctrl {
        match l {
            Some('s') => return Some(KeyAction::BeginSketch),
            Some('e') => return Some(KeyAction::Tool("extrude")),
            Some('h') => return Some(KeyAction::Tool("hole")),
            Some('m') => return Some(KeyAction::Tool("move")),
            Some('f') => return Some(KeyAction::Fit),
            Some('x') => return Some(KeyAction::ToggleCut),
            _ => {}
        }
        // `views[k]` compares the typed character, so Shift+1 ('!') is not a view key.
        if !mods.shift {
            if let Some(v) = view_for(key) {
                return Some(KeyAction::View(v));
            }
        }
    }
    None
}

/// Reads this frame's key presses (repeats included, as `keydown` fires them) and maps each.
pub fn handle(ctx: &egui::Context, state: &KeyState) -> Vec<KeyAction> {
    let mut st = *state;
    st.in_field = ctx.egui_wants_keyboard_input();
    let presses: Vec<(Key, Modifiers)> = ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Key { key, pressed: true, modifiers, .. } => Some((*key, *modifiers)),
                _ => None,
            })
            .collect()
    });
    presses.into_iter().filter_map(|(k, m)| map_key(k, m, &st)).collect()
}
