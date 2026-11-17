//! The body's right-click menu (1.x `bodyMenu`, app.js): Rename, Hide, Colour, Properties, the
//! mesh tools, Export, Send to Slicer, Convert to STEP and Remove. A child of `app.rs`; the browser
//! tree asks for it (`UiEvent::BodyMenu`) and the picks come back through `on_menu_pick`.
//!
//! Row ids are `body:<what>:<body id>`. Send to Slicer opens the list of slicers in place of the
//! menu (1.x opens it beside the row), asking for just this body.

use super::*;

impl StlApp {
    /// Right-click on a body: select it when it isn't, then show its menu at `at`.
    pub(super) fn open_body_menu(&mut self, id: &str, at: Pos2) {
        let Some(b) = self.bodies.iter().find(|b| b.id == id).cloned() else {
            return;
        };
        if !self.shell.selection.iter().any(|s| s == id) {
            self.shell.selection = vec![id.to_owned()];
            self.viewport.set_selection(&self.shell.selection);
            self.shell.push_event(UiEvent::Select(vec![id.to_owned()]));
        }
        let web = self.platform.is_web();
        let desk = ribbon::DeskTools::of(&*self.platform);
        let row = |what: &str, label: &str| MenuItem::item(format!("body:{what}:{id}"), label);
        let mut items = vec![
            MenuItem::Hdr(b.name.clone()),
            row("rename", "Rename…"),
            row("hide", if b.hidden { "Show" } else { "Hide" }),
            row("colour", "Colour…"),
            row("info", "Properties"),
            MenuItem::Sep,
        ];
        if !b.solid {
            items.push(row("repair", "Repair…"));
        }
        items.extend([
            row("move", "Move / Rotate…"),
            row("scale", "Scale…"),
            row("simplify", "Simplify…"),
            MenuItem::Sep,
            row("stl", "Export STL…"),
            row("3mf", "Export 3MF…"),
        ]);
        if !web {
            if desk.send {
                items.push(row("send", "Send to Slicer").icon("slicer"));
            }
            if desk.to_step {
                items.push(row("step", "Convert to STEP (SolidCraft)…").disabled(!desk.step_ready));
            }
        }
        items.extend([MenuItem::Sep, row("remove", "Remove")]);
        self.shell.menu = Some(OpenMenu::new("body", items, at));
    }

    /// A right-click in the 3D view (1.x `pointerup`, button 2): on a body its menu (the body is
    /// selected first), anywhere else the view menu (`viewMenu`).
    pub(super) fn open_viewport_menu(&mut self, at: Pos2) {
        if let Some(crate::viewport::pick::Hit::Body { id, .. }) = self.viewport.pick(at, true, false) {
            return self.open_body_menu(&id, at);
        }
        let items = vec![
            MenuItem::item("view.fit", "Fit All").key("F"),
            MenuItem::item("view.iso", "Isometric"),
            MenuItem::item("view.top", "Top"),
            MenuItem::Sep,
            MenuItem::item("file.insert", "Insert Mesh…"),
            MenuItem::item("view.sketch", "Create Sketch"),
        ];
        self.shell.menu = Some(OpenMenu::new("view", items, at));
    }

    /// A row of the body menu was picked (`rest` is `<what>:<body id>`).
    pub(super) fn on_body_menu_pick(&mut self, rest: &str, at: Pos2) {
        let Some((what, id)) = rest.split_once(':') else { return };
        let Some((index, b)) = self
            .bodies
            .iter()
            .enumerate()
            .find(|(_, b)| b.id == id)
            .map(|(i, b)| (i, b.clone()))
        else {
            return;
        };
        match what {
            "rename" => self.browser.begin_rename(id, &b.name),
            "hide" => self.send(Command::BodyProps {
                id: id.to_owned(),
                name: None,
                color: None,
                hidden: Some(!b.hidden),
            }),
            "colour" => self.browser.begin_colour(id, browser::swatch_rgb(&b, index)),
            "info" => self.open_info(Some(id.to_owned())),
            "repair" => {
                let mut v = Values::new();
                v.insert("body".to_owned(), serde_json::json!([id]));
                self.open_tool("repair", Some(v));
            }
            "move" | "scale" | "simplify" => {
                self.open_tool(what, None);
            }
            "stl" | "3mf" => {
                if let Some(f) = crate::engine_link::ExportFormat::from_ext(what) {
                    self.files.start(
                        &*self.platform,
                        &*self.link,
                        FileAction::Export(f),
                        Some(vec![id.to_owned()]),
                    );
                }
            }
            "send" => {
                let items = self.slicer_menu(Some(id));
                self.shell.menu = Some(OpenMenu::new("send-body", items, at));
            }
            "step" => self.to_step_body(id),
            "remove" => {
                self.open_tool("delete", None);
            }
            _ => {}
        }
    }
}
