//! Open, import, save, export and drag and drop.
//!
//! The engine does the work (`Command::Open`, `Import`, `Save`, `Export`); this file asks the
//! platform for files and file names and turns the answers into commands and messages. Nothing
//! here blocks: a dialog is a [`Pending`] that [`Files::poll`] checks every frame.

use crate::engine_link::{Command, EngineLink, ExportFormat, Level, Update};
use crate::platform::{Pending, PickedFile, Platform, SaveOutcome};

/// `settings.json` key of the Recent list (1.x `recent`).
pub const RECENT_KEY: &str = "recent";
/// What a missing file's error starts with (see `platform::native::MISSING_FILE_PREFIX`).
const MISSING: &str = "FileNotFoundError:";

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
    /// File > Recent on desktop: the file is being read from this path.
    Recent(String, Pending<Result<PickedFile, String>>),
}

/// One entry of the Recent list. On desktop it is a path (remembered in `settings.json`, read
/// from disk when opened); on the web there is no path, so the session keeps the bytes.
#[derive(Clone)]
struct RecentFile {
    /// The path, or the file name when the platform has no paths.
    key: String,
    /// Only without a path.
    bytes: Option<Vec<u8>>,
}

struct Opening {
    /// The file name the top bar shows once it opens (1.x shows `basename(path)`); `None` for a new project.
    title: Option<String>,
    /// Name of the project (without the extension) once it opens; `None` for a new project.
    project_name: Option<String>,
    project_path: Option<String>,
    /// The file to put in Recent once it opens.
    recent: Option<PickedFile>,
    /// Where it came from (any file that has a path, projects or meshes), for Recent.
    recent_path: Option<String>,
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
    /// Files opened or saved, newest first (at most [`RECENT_MAX`]). With a path they last
    /// across restarts (`settings.json`); without one (web) only for this session, from bytes.
    recent: Vec<RecentFile>,
}

/// How many recent files are kept (1.x `_remember` keeps ten; the menu shows the first six).
pub const RECENT_MAX: usize = 10;

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

    /// What the menu and the start view open from, newest first: paths on desktop, file names
    /// where the platform has no paths (hand one back to [`Files::open_recent`]).
    pub fn recent_keys(&self) -> Vec<String> {
        self.recent.iter().map(|f| f.key.clone()).collect()
    }

    /// The file names of the recent files, newest first.
    pub fn recent_names(&self) -> Vec<String> {
        self.recent.iter().map(|f| f.key.rsplit(['/', '\\']).next().unwrap_or(&f.key).to_owned()).collect()
    }

    /// Read the remembered list (call once at start-up).
    pub fn load_recent(&mut self, platform: &dyn Platform) {
        let Some(serde_json::Value::Array(list)) = platform.setting_json(RECENT_KEY) else { return };
        self.recent = list
            .iter()
            .filter_map(|v| v.as_str())
            .filter(|p| !p.is_empty())
            .take(RECENT_MAX)
            .map(|p| RecentFile { key: p.to_owned(), bytes: None })
            .collect();
    }

    fn save_recent(&self, platform: &dyn Platform) {
        // only paths last; a web session's list (names and bytes) is not stored
        let paths: Vec<_> = self.recent.iter().filter(|f| f.bytes.is_none()).map(|f| serde_json::Value::String(f.key.clone())).collect();
        platform.set_setting_json(RECENT_KEY, serde_json::Value::Array(paths));
    }

    /// Put a file first in Recent (1.x `_remember`): by its path when there is one, else by name
    /// with its bytes.
    fn remember(&mut self, platform: &dyn Platform, name: &str, path: Option<&str>, bytes: &[u8]) {
        let entry = match path {
            Some(p) => RecentFile { key: p.to_owned(), bytes: None },
            None => RecentFile { key: name.to_owned(), bytes: Some(bytes.to_vec()) },
        };
        self.recent.retain(|f| f.key != entry.key);
        self.recent.insert(0, entry);
        self.recent.truncate(RECENT_MAX);
        self.save_recent(platform);
    }

    /// File > Recent. A path is read from disk (the answer comes through [`Files::poll`]; a file
    /// that is gone is dropped from the list there); a name is opened from the session's bytes.
    /// False when it isn't in the list.
    pub fn open_recent(&mut self, platform: &dyn Platform, link: &dyn EngineLink, key: &str) -> bool {
        let Some(f) = self.recent.iter().find(|f| f.key == key).cloned() else { return false };
        match f.bytes {
            Some(bytes) => {
                let file = PickedFile { name: f.key, bytes };
                self.open_bytes(link, file, None);
            }
            None => self.waiting.push(Waiting::Recent(f.key.clone(), platform.read_path(&f.key))),
        }
        true
    }

    /// File > New: an empty project replaces the document (the engine has no separate "new").
    pub fn new_document(&mut self, link: &dyn EngineLink) {
        let bytes = stl_engine::stlx::to_bytes(&stl_engine::Document::new(stl_engine::Registry::builtin())).unwrap_or_default();
        self.opening = Some(Opening { title: None, project_name: None, project_path: None, recent: None, recent_path: None, seen: false, failed: false, ahead: usize::from(self.engine_busy) });
        link.send(Command::Open { name: "Untitled.stlx".to_string(), bytes });
    }

    /// Where the open project lives on disk (desktop). `None` for a new project, a mesh, and on
    /// the web.
    pub fn project_path(&self) -> Option<&str> {
        self.project_path.as_deref()
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
                Waiting::Recent(path, p) => {
                    let (path, p) = (path.clone(), p.clone());
                    match p.take() {
                        None => still.push(w),
                        Some(Ok(file)) => self.open_bytes(link, file, Some(path.clone())),
                        Some(Err(e)) => {
                            // a moved or deleted file: say so, and take it off the list
                            if e.starts_with(MISSING) {
                                self.recent.retain(|f| f.key != *path);
                                self.save_recent(platform);
                            }
                            out.messages.push((Level::Error, e));
                        }
                    }
                    continue;
                }
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
                            let path = self.project_path.clone().or(o.recent_path);
                            self.remember(platform, &f.name, path.as_deref(), &f.bytes);
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
                        // a project saved over its known path (`Done(None)`) stays under that path
                        let at = if s.project { self.project_path.clone() } else { None };
                        self.remember(platform, &f.name, at.as_deref(), &f.bytes);
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
            project_path: path.clone().filter(|_| is_project(&file.name)),
            recent: Some(file.clone()),
            recent_path: path,
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
