//! Open, import, save, export and drag and drop.
//!
//! The engine does the work (`Command::Open`, `Import`, `Save`, `Export`); this file asks the
//! platform for files and file names and turns the answers into commands and messages. Nothing
//! here blocks: a dialog is a [`Pending`] that [`Files::poll`] checks every frame.

use crate::engine_link::{Command, EngineLink, ExportFormat, Level, Update};
use crate::platform::{Pending, PickedFile, Platform, SaveOutcome};

/// What the open dialog lists (`.stlx` and the mesh formats), as in the 1.x open dialog.
pub const OPEN_FILTERS: &[(&str, &[&str])] =
    &[("STL Editor project or mesh", &["stlx", "stl", "obj", "3mf", "ply", "off", "glb", "gltf"])];
/// What the Insert Mesh dialog lists (1.x `MESH_ACCEPT`).
pub const MESH_FILTERS: &[(&str, &[&str])] = &[("Mesh", &["stl", "obj", "3mf", "ply", "off", "glb", "gltf"])];

/// The text shown on the drop overlay (`#drop-overlay` in `web/index.html`).
pub const DROP_TEXT: &str = "Drop to insert meshes";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileAction {
    Open,
    /// Insert Mesh: adds a body.
    Import,
    Save,
    SaveAs,
    Export(ExportFormat),
}

/// What the app should do after [`Files::poll`] or a drop.
#[derive(Default, Debug)]
pub struct FileOutcome {
    /// Toast these.
    pub messages: Vec<(Level, String)>,
    /// A project or mesh was opened and its bodies have arrived: set the isometric view
    /// (1.x `setView('iso')`).
    pub view_iso: bool,
    /// An import has arrived: fit the view (1.x `viewer.fit()`).
    pub fit: bool,
    /// A project write succeeded this frame: the document has no unsaved changes.
    pub saved: bool,
}

enum Waiting {
    Open(Pending<Option<(PickedFile, Option<String>)>>),
    Import(Pending<Option<(PickedFile, Option<String>)>>),
}

struct Opening {
    /// The file name the top bar shows once it opens (1.x shows `basename(path)`); `None` for a new project.
    title: Option<String>,
    /// Name of the project (without the extension) once it opens; `None` for a new project.
    project_name: Option<String>,
    project_path: Option<String>,
    /// The file to put in Recent once it opens.
    recent: Option<PickedFile>,
    /// The engine has answered (bodies or an error); the command is over at its next `Busy(false)`.
    seen: bool,
    failed: bool,
    /// Commands already running or queued when this Open was sent; their `Busy(false)` (and
    /// their bodies or errors) are not the Open's answer.
    ahead: usize,
}

struct Saving {
    /// A project save: put the file in Recent once the write succeeds.
    recent: Option<PickedFile>,
    pending: Pending<Result<SaveOutcome, String>>,
    /// A project save: remember where it went so the next Save goes back there.
    project: bool,
    /// The toast after a good write: the file name, and how many bodies for an export.
    done: String,
}

#[derive(Default)]
pub struct Files {
    waiting: Vec<Waiting>,
    saving: Vec<Saving>,
    /// The name of the open project (without the extension), for the Save suggestion.
    pub project_name: Option<String>,
    /// The file name the top bar and the browser show (`basic.stlx`, as 1.x does); `None` is "Untitled".
    pub title: Option<String>,
    /// Where the open project lives on disk (desktop only). Save writes here without asking;
    /// Save As asks and moves it. `None` for a new project, and always on the web.
    project_path: Option<String>,
    /// The Save in flight is a Save As (it asks even when the path is known).
    save_as: bool,
    /// A file is being dragged over the window: show the drop overlay.
    pub dragging: bool,
    /// An Open / New / Recent sent to the engine and not yet answered. Its name, path and Recent
    /// entry are only taken on once the engine has accepted the file.
    opening: Option<Opening>,
    /// The engine is between a `Busy(true)` and its `Busy(false)`.
    engine_busy: bool,
    fit_after_bodies: bool,
    /// Files opened or saved in this session, newest first (at most [`RECENT_MAX`]). The platform
    /// has no way to read a path back, so the bytes are kept for File > Recent.
    recent: Vec<PickedFile>,
}

/// How many recent files are kept (1.x shows the last six).
pub const RECENT_MAX: usize = 6;

fn is_project(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".stlx")
}

fn stem(name: &str) -> String {
    match name.rfind('.') {
        Some(i) if i > 0 => name[..i].to_string(),
        _ => name.to_string(),
    }
}

impl Files {
    pub fn new() -> Self {
        Self::default()
    }

    /// The names of this session's recent files, newest first.
    pub fn recent_names(&self) -> Vec<String> {
        self.recent.iter().map(|f| f.name.clone()).collect()
    }

    fn remember(&mut self, name: &str, bytes: &[u8]) {
        self.recent.retain(|f| f.name != name);
        self.recent.insert(0, PickedFile { name: name.to_string(), bytes: bytes.to_vec() });
        self.recent.truncate(RECENT_MAX);
    }

    /// File > Recent: open a file from this session's list again. False when it isn't there.
    pub fn open_recent(&mut self, link: &dyn EngineLink, name: &str) -> bool {
        let Some(f) = self.recent.iter().find(|f| f.name == name).cloned() else { return false };
        self.opening = Some(Opening { title: Some(f.name.clone()), project_name: Some(stem(&f.name)), project_path: None, recent: Some(f.clone()), seen: false, failed: false, ahead: usize::from(self.engine_busy) });
        link.send(Command::Open { name: f.name, bytes: f.bytes });
        true
    }

    /// File > New: an empty project replaces the document (the engine has no separate "new").
    pub fn new_document(&mut self, link: &dyn EngineLink) {
        let bytes = stl_engine::stlx::to_bytes(&stl_engine::Document::new(stl_engine::Registry::builtin())).unwrap_or_default();
        self.opening = Some(Opening { title: None, project_name: None, project_path: None, recent: None, seen: false, failed: false, ahead: usize::from(self.engine_busy) });
        link.send(Command::Open { name: "Untitled.stlx".to_string(), bytes });
    }

    /// True while a dialog is open or a file is being written.
    pub fn busy(&self) -> bool {
        !self.waiting.is_empty() || !self.saving.is_empty()
    }

    /// Start a file action. `ids` limits an export to those bodies (`None`: the selection is
    /// the app's business; pass `None` for all visible bodies).
    pub fn start(&mut self, platform: &dyn Platform, link: &dyn EngineLink, action: FileAction, ids: Option<Vec<String>>) {
        match action {
            FileAction::Open => self.waiting.push(Waiting::Open(platform.open_file_path(OPEN_FILTERS))),
            FileAction::Import => self.waiting.push(Waiting::Import(platform.open_file_path(MESH_FILTERS))),
            // The engine answers with `Update::Saved`; `poll` then writes it to the known path
            // (Save) or asks where (Save As, or no path yet).
            FileAction::Save | FileAction::SaveAs => {
                self.save_as = action == FileAction::SaveAs;
                link.send(Command::Save);
            }
            FileAction::Export(format) => link.send(Command::Export { format, ids, date: platform.now_iso_utc() }),
        }
    }

    /// Call every frame with the updates the link returned (they are only read, so the other
    /// regions can read them too).
    pub fn poll(&mut self, platform: &dyn Platform, link: &dyn EngineLink, updates: &[Update]) -> FileOutcome {
        let mut out = FileOutcome::default();

        // dialogs that have answered
        let mut still = Vec::new();
        for w in std::mem::take(&mut self.waiting) {
            let (pending, import) = match &w {
                Waiting::Open(p) => (p.clone(), false),
                Waiting::Import(p) => (p.clone(), true),
            };
            match pending.take() {
                None => still.push(w),
                Some(None) => {}
                Some(Some((file, path))) => self.send_file(link, file, path, import, &mut out),
            }
        }
        self.waiting.extend(still);

        // updates that need a file name or a toast
        for u in updates {
            match u {
                Update::Saved { bytes } => {
                    let name = format!("{}.stlx", self.project_name.as_deref().unwrap_or("Untitled"));
                    let recent = PickedFile { name: name.clone(), bytes: bytes.clone() };
                    let ask = std::mem::take(&mut self.save_as);
                    let pending = match (&self.project_path, ask) {
                        (Some(path), false) => platform.save_to(path, bytes.clone()).map(|r| r.map(|_| SaveOutcome::Done(None))),
                        _ => platform.save_file_path(&name, bytes.clone()),
                    };
                    self.saving.push(Saving { recent: Some(recent), pending, project: true, done: format!("Saved {name}") });
                }
                Update::Exported { name, bytes, count } => {
                    let done = format!("Exported {count} bod{} to {name}", if *count > 1 { "ies" } else { "y" });
                    let pending = platform
                        .save_file(name, bytes.clone())
                        .map(|r| r.map(|ok| if ok { SaveOutcome::Done(None) } else { SaveOutcome::Cancelled }));
                    self.saving.push(Saving { recent: None, pending, project: false, done });
                }
                Update::Error(_) => {
                    self.save_as = false;
                    if let Some(o) = self.opening.as_mut().filter(|o| o.ahead == 0) {
                        o.seen = true;
                        o.failed = true;
                    }
                }
                Update::Bodies(_) => {
                    if let Some(o) = self.opening.as_mut().filter(|o| o.ahead == 0) {
                        o.seen = true;
                    }
                    out.fit |= std::mem::take(&mut self.fit_after_bodies);
                }
                // The engine sends an error before the bodies and timeline that follow it, and the
                // bodies come either way, so only the end of the command says whether the file was
                // accepted.
                Update::Busy(true) => self.engine_busy = true,
                Update::Busy(false) if self.opening.as_ref().is_some_and(|o| o.ahead > 0) => {
                    self.engine_busy = false;
                    if let Some(o) = &mut self.opening {
                        o.ahead -= 1;
                    }
                }
                Update::Busy(false) if self.opening.as_ref().is_some_and(|o| o.seen) => {
                    self.engine_busy = false;
                    let o = self.opening.take().expect("checked");
                    if !o.failed {
                        self.project_name = o.project_name;
                        self.title = o.title;
                        self.project_path = o.project_path;
                        if let Some(f) = o.recent {
                            self.remember(&f.name, &f.bytes);
                        }
                        out.view_iso = true;
                    }
                }
                Update::Busy(false) => self.engine_busy = false,
                _ => {}
            }
        }

        // writes that have finished
        let mut still = Vec::new();
        for s in std::mem::take(&mut self.saving) {
            match s.pending.take() {
                None => still.push(s),
                Some(Ok(SaveOutcome::Done(path))) => {
                    if s.project && path.is_some() {
                        if let Some(n) = path.as_deref().and_then(|p| std::path::Path::new(p).file_name()) {
                            self.project_name = Some(stem(&n.to_string_lossy()));
                            self.title = Some(n.to_string_lossy().into_owned());
                        }
                        self.project_path = path;
                    }
                    if let Some(f) = &s.recent {
                        self.remember(&f.name, &f.bytes);
                    }
                    out.saved |= s.project;
                    out.messages.push((Level::Success, s.done));
                }
                Some(Ok(SaveOutcome::Cancelled)) => {}
                Some(Err(e)) => out.messages.push((Level::Error, e)),
            }
        }
        self.saving.extend(still);
        out
    }

    fn send_file(&mut self, link: &dyn EngineLink, file: PickedFile, path: Option<String>, import: bool, out: &mut FileOutcome) {
        if import {
            if is_project(&file.name) {
                out.messages.push((Level::Warning, "Use File › Open for projects.".to_string()));
                return;
            }
            self.fit_after_bodies = true;
            link.send(Command::Import { name: file.name, bytes: file.bytes });
        } else {
            self.open_bytes(link, file, path);
        }
    }

    /// Open a project or mesh whose bytes are known (File > Open, a file on the command line).
    /// The name, path and Recent entry are taken on once the engine accepts it.
    pub fn open_bytes(&mut self, link: &dyn EngineLink, file: PickedFile, path: Option<String>) {
        self.opening = Some(Opening {
            title: Some(file.name.clone()),
            project_name: Some(stem(&file.name)),
            project_path: path.filter(|_| is_project(&file.name)),
            recent: Some(file.clone()),
            seen: false,
            failed: false,
            ahead: usize::from(self.engine_busy),
        });
        link.send(Command::Open { name: file.name, bytes: file.bytes });
    }

    /// Dropped files, as 1.x: a project is refused with a hint; any other file is inserted.
    /// Also keeps `dragging` up to date for the overlay. Call once per frame.
    ///
    /// On the web egui can only read a dropped file asynchronously, so there the web platform
    /// reads it and calls [`Files::accept_dropped`]; this then only updates `dragging`.
    pub fn handle_drop(&mut self, ctx: &egui::Context, link: &dyn EngineLink) -> FileOutcome {
        let (hovering, dropped) = ctx.input(|i| (!i.raw.hovered_files.is_empty(), i.raw.dropped_files.clone()));
        self.dragging = hovering && dropped.is_empty();
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut out = FileOutcome::default();
            for f in dropped {
                let name = f.path().file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                match f.bytes() {
                    Err(e) => out.messages.push((Level::Error, format!("{name}: {e}"))),
                    Ok(bytes) => self.accept_dropped(link, name, bytes, &mut out),
                }
            }
            out
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (dropped, link);
            FileOutcome::default()
        }
    }

    /// One dropped file whose contents are known.
    pub fn accept_dropped(&mut self, link: &dyn EngineLink, name: String, bytes: Vec<u8>, out: &mut FileOutcome) {
        self.send_file(link, PickedFile { name, bytes }, None, true, out);
    }

    /// The dashed accent frame with "Drop to insert meshes" while a file is dragged over.
    /// `rect` is the viewport (1.x `#drop-overlay` sits inside `#viewport`, not over the top bar).
    pub fn drop_overlay(&self, ctx: &egui::Context, t: &crate::theme::Tokens, rect: egui::Rect) {
        if !self.dragging {
            return;
        }
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("drop-overlay")));
        painter.rect_filled(rect, 0.0, t.accent_soft);
        let stroke = egui::Stroke::new(3.0, t.accent);
        let r = rect.shrink(1.5);
        let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
        painter.extend(egui::Shape::dashed_line(&pts, stroke, 8.0, 5.0));
        painter.text(rect.center(), egui::Align2::CENTER_CENTER, DROP_TEXT, egui::FontId::proportional(18.0), t.accent);
    }
}
