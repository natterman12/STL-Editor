//! Send to Slicer (1.x `sendMenu`, `slicerItems`, `send_to`) and To STEP (`toStep`, `r_to_step`),
//! desktop only. A child of `app.rs`.
//!
//! The export goes through the engine like any export (`Command::Export`); its `Update::Exported`
//! is taken here, before the file dialogs see it, and handed to the platform, which opens it in
//! the slicer or converts it. The platform's answers are polled each frame and shown as toasts.

use super::*;

use crate::engine_link::ExportFormat;
use crate::platform::{Pending, SaveOutcome};

/// What the engine's next `Exported` answer is for.
enum Asked {
    /// Send to Slicer: open the 3MF in the slicer with this key.
    Slicer { key: String },
    /// To STEP: convert the STL, offering `suggested` as the STEP's name.
    Step { suggested: String },
}

/// Hand-offs waiting for an answer.
#[derive(Default)]
pub(super) struct HandoffState {
    /// An export asked for by a hand-off; its `Update::Exported` is taken, not saved.
    asked: Option<Asked>,
    /// Launches of a slicer, each answered with the message to show when it failed.
    launches: Vec<Pending<Result<(), String>>>,
    /// To STEP conversions, each with where it was saved.
    conversions: Vec<Pending<Result<SaveOutcome, String>>>,
}

impl StlApp {
    /// The ribbon's Send to Slicer button: the menu of the slicers found (`sendMenu`).
    pub fn open_send_menu(&mut self, anchor: Pos2) {
        if self.platform.is_web() {
            return self.desktop_only("Send to Slicer");
        }
        let items = self.slicer_menu(None);
        self.shell.menu = Some(OpenMenu::new("send", items, anchor));
    }

    /// The ribbon's To STEP button (`toStep`): the first selected body.
    pub fn to_step(&mut self) {
        let id = self.shell.selection.first().cloned();
        self.to_step_of(id);
    }

    /// The body menu's Convert to STEP: that body.
    // "to STEP" is the tool's name here, not a conversion of `self`
    #[allow(clippy::wrong_self_convention)]
    pub(super) fn to_step_body(&mut self, id: &str) {
        self.to_step_of(Some(id.to_owned()));
    }

    #[allow(clippy::wrong_self_convention)]
    fn to_step_of(&mut self, id: Option<String>) {
        if self.platform.is_web() {
            return self.desktop_only("This tool");
        }
        if self.handoff.asked.is_some() {
            return;
        }
        let Some(id) = id else {
            self.toasts.push(Level::Warning, "Select a body first.".to_owned());
            return;
        };
        if !self.platform.step_available() {
            self.toasts.push(Level::Error, "SolidCraft isn't installed.".to_owned());
            return;
        }
        let name = self
            .bodies
            .iter()
            .find(|b| b.id == id)
            .map(|b| b.name.clone())
            .unwrap_or_else(|| "body".to_owned());
        self.ask_export(
            Asked::Step {
                suggested: format!("{name}.step"),
            },
            ExportFormat::Stl,
            Some(vec![id]),
        );
    }

    /// File › Send to Slicer: the items to add to the File menu (none when nothing is found).
    pub(super) fn send_file_items(&self) -> Vec<MenuItem> {
        let found = self.platform.slicers();
        if found.is_empty() {
            return Vec::new();
        }
        let mut items = vec![MenuItem::Hdr("Send to Slicer".to_owned())];
        items.extend(
            found
                .into_iter()
                .map(|(key, name)| MenuItem::item(format!("send:{key}"), name).icon("slicer")),
        );
        items
    }

    /// A menu item `send:<key>` was picked: export the selected bodies (or all visible ones) and
    /// open them in that slicer when the export answers.
    pub(super) fn send_pick(&mut self, key: &str) {
        if self.handoff.asked.is_some() {
            return;
        }
        let ids = self.export_ids();
        self.ask_export(Asked::Slicer { key: key.to_owned() }, ExportFormat::ThreeMf, ids);
    }

    /// A slicer was picked from the body menu: export just that body and open it there.
    pub(super) fn send_pick_body(&mut self, key: &str, id: &str) {
        if self.handoff.asked.is_some() {
            return;
        }
        self.ask_export(
            Asked::Slicer { key: key.to_owned() },
            ExportFormat::ThreeMf,
            Some(vec![id.to_owned()]),
        );
    }

    /// Take the engine's answer to a hand-off (its `Update::Exported`) before the file dialogs
    /// see it. An error answer ends the wait and stays in the list, so it is shown as usual.
    pub(super) fn handoff_updates(&mut self, updates: &mut Vec<Update>) {
        if self.handoff.asked.is_none() {
            return;
        }
        let Some(i) = updates
            .iter()
            .position(|u| matches!(u, Update::Exported { .. } | Update::Error(_)))
        else {
            return;
        };
        if matches!(updates[i], Update::Error(_)) {
            self.handoff.asked = None;
            return;
        }
        let Update::Exported { name, bytes, .. } = updates.remove(i) else {
            return;
        };
        let Some(asked) = self.handoff.asked.take() else {
            return;
        };
        match asked {
            Asked::Slicer { key } => {
                let pending = self.platform.send_to_slicer(&key, &name, bytes);
                self.handoff.launches.push(pending);
            }
            Asked::Step { suggested } => {
                self.toasts.push(Level::Info, "Converting with SolidCraft…".to_owned());
                let pending = self.platform.to_step(&suggested, bytes);
                self.handoff.conversions.push(pending);
            }
        }
    }

    /// Poll the launches and conversions and show what they answered (each frame).
    pub(super) fn handoff_frame(&mut self) {
        for pending in std::mem::take(&mut self.handoff.launches) {
            match pending.take() {
                None => self.handoff.launches.push(pending),
                Some(Ok(())) => self.toasts.push(Level::Info, "Opened".to_owned()),
                Some(Err(message)) => self.toasts.push(Level::Error, message),
            }
        }
        for pending in std::mem::take(&mut self.handoff.conversions) {
            match pending.take() {
                None => self.handoff.conversions.push(pending),
                Some(Ok(SaveOutcome::Done(Some(path)))) => {
                    let file = path.rsplit(['/', '\\']).next().unwrap_or(&path).to_owned();
                    self.toasts.push(Level::Info, format!("Saved STEP: {file}"));
                }
                Some(Ok(_)) => {}
                Some(Err(message)) => self.toasts.push(Level::Error, message),
            }
        }
    }

    /// The slicers found, as the menu lists them (1.x `slicerItems`): only what is installed,
    /// SolidCraft after a separator.
    ///
    /// `body`: the menu of one body (the body menu's Send to Slicer), as 1.x `slicerItems([id])`.
    pub(super) fn slicer_menu(&self, body: Option<&str>) -> Vec<MenuItem> {
        let n = self.shell.selection.len();
        let header = if body.is_some() {
            "Send this body to".to_owned()
        } else if n > 0 {
            format!("Send {n} selected to")
        } else {
            "Send all visible bodies to".to_owned()
        };
        let pick = |key: &str| match body {
            Some(id) => format!("bsend:{key}:{id}"),
            None => format!("send:{key}"),
        };
        let mut items = vec![MenuItem::Hdr(header)];
        let found = self.platform.slicers();
        if found.is_empty() {
            items.push(
                MenuItem::item("send:none", "No slicer found: install Bambu Studio or OrcaSlicer").disabled(true),
            );
            return items;
        }
        let (solidcraft, slicers): (Vec<_>, Vec<_>) = found.into_iter().partition(|(key, _)| key == "solidcraft");
        let separate = !slicers.is_empty() && !solidcraft.is_empty();
        items.extend(
            slicers
                .into_iter()
                .map(|(key, name)| MenuItem::item(pick(&key), name).icon("slicer")),
        );
        if separate {
            items.push(MenuItem::Sep);
        }
        items.extend(
            solidcraft
                .into_iter()
                .map(|(key, name)| MenuItem::item(pick(&key), name)),
        );
        items
    }

    /// The bodies a hand-off exports: the selection, or all visible bodies (`None`).
    fn export_ids(&self) -> Option<Vec<String>> {
        (!self.shell.selection.is_empty()).then(|| self.shell.selection.clone())
    }

    fn ask_export(&mut self, asked: Asked, format: ExportFormat, ids: Option<Vec<String>>) {
        self.handoff.asked = Some(asked);
        let date = self.platform.now_iso_utc();
        self.link.send(Command::Export { format, ids, date });
    }

    /// The alert that says what the desktop app does (1.x `desktopOnly`).
    fn desktop_only(&mut self, what: &str) {
        let text = format!(
            "{what} runs in the desktop app, which can open your slicer and SolidCraft directly.\n\nSave your project (File › Save) and open it in the desktop app: everything carries over, timeline included."
        );
        let links = vec![("Get the desktop app (Windows, free)".to_owned(), DESKTOP_URL.to_owned())];
        self.shell
            .dialogs
            .open_alert("desktop-only", "Desktop app tool", &text, links);
    }
}
