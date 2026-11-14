//! Sketch keys (`sketcher.js` `onKey`) and the ribbon's one-shot sketch actions (`sk_delete`,
//! `construction`). The tool in use sees the key first (a line chain finishes on Enter or Esc,
//! a dimension drops its picks on Esc); what it leaves is handled here.

use egui::{Key, Modifiers};
use stl_sketch::solver::{refs_exist, World};

use super::session::{Env, SketchMode};
use super::{SketchEffect, SketchInput, SketchSession};

/// The one-letter tool keys (`map` in `onKey`).
pub fn tool_for_letter(key: Key) -> Option<&'static str> {
    Some(match key {
        Key::L => "line",
        Key::A => "arc3",
        Key::R => "rect",
        Key::C => "circle",
        Key::P => "polygon",
        Key::S => "slot",
        Key::T => "trim",
        Key::D => "c_dimension",
        _ => return None,
    })
}

/// `deleteSelected()`: the selected relation, else the selected shapes (and relations that lose a
/// shape they use).
pub fn delete_selected(s: &mut SketchSession) -> Vec<SketchEffect> {
    if let Some(i) = s.sel_c {
        return super::relations::delete_constraint(s, i);
    }
    if s.sel.is_empty() {
        return Vec::new();
    }
    s.push();
    let sel = s.sel.clone();
    s.entities.retain(|e| !e.id().is_some_and(|id| sel.contains(id)));
    let ents = super::solve::all_ents(s);
    let w = World::new(&ents);
    s.constraints.retain(|c| refs_exist(c, &w));
    s.sel.clear();
    vec![SketchEffect::Changed]
}

/// `toggleConstruction()`.
pub fn toggle_construction(s: &mut SketchSession) -> Vec<SketchEffect> {
    if s.sel.is_empty() {
        return Vec::new();
    }
    s.push();
    let sel = s.sel.clone();
    for e in s.entities.iter_mut().filter(|e| e.id().is_some_and(|id| sel.contains(id))) {
        let on = !e.construction();
        set_construction(e, on);
    }
    vec![SketchEffect::Changed]
}

fn set_construction(e: &mut super::Entity, on: bool) {
    use super::Entity::*;
    let v = Some(on);
    match e {
        Line(x) => x.construction = v,
        Arc(x) => x.construction = v,
        Circle(x) => x.construction = v,
        Rect(x) => x.construction = v,
        Polygon(x) => x.construction = v,
        Slot(x) => x.construction = v,
        Polyline(x) => x.construction = v,
        Point(x) => x.construction = v,
    }
}

/// `onKey`. Returns true when the key was used (the app keeps it from anything else).
pub fn on_key(m: &mut SketchMode, env: &Env, key: Key, mods: Modifiers) -> (bool, Vec<SketchEffect>) {
    let (handled, mut out) = m.input(&SketchInput::Key { key, mods }, env);
    if handled {
        return (true, out);
    }
    let ctrl = mods.ctrl || mods.mac_cmd;
    if key == Key::Escape {
        if m.s.tool != "select" {
            out.extend(m.set_tool("select", env));
        } else if !m.s.sel.is_empty() || m.s.sel_c.is_some() {
            m.s.sel.clear();
            m.s.sel_c = None;
            m.dirty = true;
        }
        return (true, out);
    }
    if matches!(key, Key::Delete | Key::Backspace) && (!m.s.sel.is_empty() || m.s.sel_c.is_some()) {
        out.extend(delete_selected(&mut m.s));
        return (true, out);
    }
    if !ctrl {
        if let Some(t) = tool_for_letter(key) {
            out.extend(m.set_tool(t, env));
            return (true, out);
        }
    }
    if ctrl && key == Key::Z {
        // `undoLocal`: the sketch's own history, never the document's
        m.s.undo_local();
        out.push(SketchEffect::Changed);
        return (true, out);
    }
    (false, out)
}
