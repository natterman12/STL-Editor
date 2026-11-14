//! The application: owns the theme, the platform, the engine link and the viewport, and lays the
//! window out like 1.x's web/index.html: top bar, ribbon, then the workspace where the viewport
//! fills everything and the browser, tool panel and timeline bar float over it.

use crate::engine_link::{Command, EngineLink, Level, NullLink, ParamView, PlaneView, SketchView, TimelineView, Update};
use crate::engine_link::BodyView;
use crate::platform::{NullPlatform, PickedFile, Platform};
use crate::recover::{self, Recover, RecoverAction, Recovered};
use crate::theme::{self, Tokens};
use crate::ui::dialogs::{self, DialogResult};
use crate::ui::menu::{self, MenuItem, MenuResult, OpenMenu};
use crate::ui::start::{self, StartPick, KOFI};
use crate::ui::files::{FileAction, FileOutcome, Files};
use crate::ui::keys::{self, KeyAction, KeyState};
use crate::ui::{browser, busy, params, ribbon, status, timeline, toasts, topbar};
use crate::ui::{Shell, ThemePref, UiAction, UiEvent};
use crate::viewport::Viewport;
use crate::sketch::session::SketchMode;
use crate::tools::panel::OpenTool;
use crate::tools::gizmo::Gizmo;
use crate::tools::sculpt::Sculpt;
use crate::tools::{Ctx, ToolDef, Values};
#[path = "app_wiring.rs"]
mod wiring;
use std::collections::{BTreeMap, HashMap};
use egui::{Align2, Area, CornerRadius, Frame, Id, Margin, Order, Pos2, Rect, RichText, Sense, Stroke, Vec2};

const DESKTOP_URL: &str = "https://natterman12.github.io/STL-Editor/#download";
const GUIDE_URL: &str = "https://github.com/natterman12/STL-Editor/blob/main/USER_GUIDE.md";
const ISSUES_URL: &str = "https://github.com/natterman12/STL-Editor/issues/new/choose";

pub struct StlApp {
    pub dark: bool,
    pub tokens: Tokens,
    pub platform: Box<dyn Platform>,
    pub link: Box<dyn EngineLink>,
    pub viewport: Viewport,
    pub shell: Shell,
    // what the engine last told us (the panels draw from these)
    pub bodies: Vec<BodyView>,
    pub timeline_view: TimelineView,
    pub param_views: Vec<ParamView>,
    pub planes: Vec<PlaneView>,
    pub sketches: Vec<SketchView>,
    pub status_text: String,
    pub engine_lost: bool,
    // Task 8 panel state
    pub browser: browser::Browser,
    pub timeline: timeline::Timeline,
    pub params: params::Params,
    pub params_open: bool,
    pub toasts: toasts::Toasts,
    pub busy: busy::Busy,
    pub files: Files,
    /// The last finished dialog: `(tag, result)`. The caller that opened it takes it.
    pub dialog_result: Option<(String, DialogResult)>,
    last_fit: Option<ribbon::Fit>,
    /// What the history looked like when the project was last opened, saved or started (`doc.dirty`
    /// is "the history is different now"; the engine doesn't send it).
    clean_sig: String,
    /// New / Open Recent waiting for the answer to "Discard unsaved changes?".
    pending_discard: Option<UiAction>,
    /// Autosave and Recover Unsaved Work; `None` until [`StlApp::enable_autosave`], so tests and
    /// the empty window never read or write the autosave folders.
    recover: Option<Recover>,
    /// When the autosave command was last sent (every 60 s, as 1.x).
    last_autosave: web_time::Instant,
    /// A recovered project is on its way to the engine: when it opens it counts as unsaved work.
    recovering: bool,
    /// The update check, banner and settings (desktop). `None` until [`StlApp::enable_autosave`],
    /// so test windows never take a window lock.
    #[cfg(not(target_arch = "wasm32"))]
    updates: Option<crate::updates::Updates>,
    /// The start-up update check waits a few seconds (1.x: 6 s).
    #[cfg(not(target_arch = "wasm32"))]
    update_start: Option<web_time::Instant>,
    /// Update now is done: the new exe, when the restart was asked for, and whether the autosave
    /// answered.
    #[cfg(not(target_arch = "wasm32"))]
    restart: Option<(std::path::PathBuf, web_time::Instant, bool)>,
    /// The busy note for Check for Updates (the engine's own overlay is `busy`).
    #[cfg(not(target_arch = "wasm32"))]
    update_busy: busy::Busy,
    /// The first frame has been drawn (the old exe of an update is deleted after it).
    first_frame_done: bool,
    /// A file from the command line, until its bodies arrive. `true` is an insert (fit the view);
    /// `false` is a project (isometric view).
    launch_file: Option<bool>,
    /// 1.x `app.js` turns the view to isometric once at startup (`viewer.setView('iso')`); done
    /// on the first frame the workspace has a size, since the distance depends on the view's shape.
    start_view_done: bool,
    /// The open tool (`this.tool`).
    pub tool: Option<crate::tools::panel::OpenTool>,
    /// Shared by all tools: only the answer to the newest one lands.
    preview_seq: u64,
    /// A rollback marker sent and not yet seen in the timeline.
    pending_marker: Option<Option<usize>>,
    /// OK was pressed and the engine has not answered: the tool stays open until it does (1.x
    /// `commitTool` awaits the edit), so a refusal leaves it open with its values.
    committing: bool,
    runtime_tools: HashMap<String, &'static ToolDef>,
    /// The handles of Move / Rotate / Scale (and Support Fins' rings) while that tool is open.
    gizmo: Option<Gizmo>,
    sculpt: Sculpt,
    /// Support Fins, coverage and Draw Walls (`app_wiring.rs`).
    sf: wiring::Sf,
    /// The open sketch (milestone 4c): `begin_sketch` / `enter_sketch` open it, Finish and Cancel close it.
    sketch: Option<SketchMode>,
    /// What the sketch keeps beside it: engine sequence, the text dialog, the import dialog.
    sk: wiring::Sk,
}

/// A short fingerprint of the history, to tell whether it changed since the last save.
fn arr_empty(v: Option<&serde_json::Value>) -> bool {
    v.and_then(|x| x.as_array()).map_or(true, |a| a.is_empty())
}

/// `ctx()` (app.js:581): what a tool's defaults and rules look at.
pub fn make_ctx<'a>(bodies: &'a [BodyView], planes: &'a [PlaneView], sketches: &'a [SketchView], features: &'a [crate::engine_link::TimelineItem], selection: &'a [String], inch: bool) -> Ctx<'a> {
    let last = features.iter().rev().find(|f| f.kind == "sketch" && f.active);
    let sk = last.and_then(|f| sketches.iter().find(|s| s.id == f.id));
    Ctx {
        bodies,
        planes,
        sketches,
        features,
        selection,
        last_sketch: sk.map(|s| s.id.as_str()),
        sketch_on_body: last.is_some_and(|f| f.params.get("plane").and_then(|p| p.as_object()).is_some_and(|o| o.contains_key("body"))),
        inch,
    }
}

fn history_sig(t: &TimelineView) -> String {
    format!("{:?}|{:?}", t.items, t.marker)
}

/// 1.x `showHelp()` (app.js:2676-2680), word for word.
const HELP_TITLE: &str = "Keyboard & mouse";
const HELP_ROWS: &[(&str, &str)] = &[
    ("Left-drag", "Orbit"),
    ("Right / middle-drag", "Pan"),
    ("Wheel", "Zoom"),
    ("Click", "Select a body (Shift adds)"),
    ("Double-click a flat face", "Sketch on it"),
    ("S", "Sketch"),
    ("E", "Extrude"),
    ("H", "Hole"),
    ("M", "Move"),
    ("F", "Fit all"),
    ("0–6", "Standard views"),
    ("Ctrl+K or /", "Search tools"),
    ("Ctrl+Z / Ctrl+Y", "Undo / Redo"),
    ("Ctrl+S", "Save"),
    ("Esc", "Cancel"),
    ("In a sketch: L R C P S", "Line, Rectangle, Circle, Polygon, Slot"),
    ("In a sketch: type a number", "Set an exact size, Tab for the next value, Enter to place"),
    ("In a sketch: D", "Smart Dimension: click, move to place, click, type the value"),
    ("Double-click a dimension", "Change it (a parameter name works too)"),
];

/// What 2.0 is built from, each with its licence (read from the workspace's `Cargo.toml` files and
/// the crates' own metadata). Shown in Help > About; nothing from 1.x's Python or JavaScript stack.
pub const CREDITS: &[(&str, &str)] = &[
    ("Manifold geometry, the manifold-rust port", "Apache-2.0"),
    ("egui and eframe, the user interface", "MIT or Apache-2.0"),
    ("wgpu, the 3D drawing", "MIT or Apache-2.0"),
    ("Selawik font", "SIL Open Font Licence 1.1"),
    ("Noto Emoji, a small piece of it for the heart and warning marks", "SIL Open Font Licence 1.1"),
    ("rayon, ttf-parser, serde", "MIT or Apache-2.0"),
    ("zip, quick-xml, reading and writing project and 3MF files", "MIT"),
    ("usvg, resvg, for SVG shapes in sketches", "Apache-2.0 or MIT"),
];

/// 1.x's About text (app.js:2546-2547) plus `THANKS` (app.js:31), as plain lines, with the credits of
/// what 2.0 actually uses in place of 1.x's library list.
pub fn about_text(web: bool) -> String {
    let version = env!("CARGO_PKG_VERSION");
    let thanks = "

STL Editor is free and made in spare time. If it saves you time, you can buy me a coffee on Ko-fi ♥";
    let credits: String = CREDITS.iter().map(|(what, lic)| format!("
{what} ({lic})")).collect();
    let (date, commit) = (crate::build_info::build_date(), crate::build_info::commit());
    let edition = if web {
        format!("Web edition {version}
CAD-style editing for STL, 3MF and OBJ meshes, running entirely in your browser: your files never leave your computer.")
    } else {
        format!("Version {version} (test build {date} {commit})
CAD-style editing for STL, 3MF and OBJ meshes.")
    };
    let gpl = if web { "

Free software under the GPL-3.0: source on GitHub." } else { "" };
    format!("{edition}

Built with:{credits}{gpl}{thanks}")
}

const DISCARD_TAG: &str = "discard-unsaved";

impl StlApp {
    /// Works without a window: tests pass `&egui::Context::default()`.
    pub fn new(ctx: &egui::Context, platform: Box<dyn Platform>, link: Box<dyn EngineLink>) -> Self {
        let dark = true;
        let tokens = theme::tokens(dark);
        // the Recent list lasts across restarts (desktop: paths in settings.json)
        let mut files = Files::new();
        files.load_recent(&*platform);
        crate::fonts::install(ctx);
        theme::apply(ctx, &tokens);
        StlApp {
            dark,
            tokens,
            platform,
            link,
            viewport: Viewport::new(),
            shell: Shell::default(),
            bodies: Vec::new(),
            timeline_view: TimelineView::default(),
            param_views: Vec::new(),
            planes: Vec::new(),
            sketches: Vec::new(),
            status_text: String::new(),
            engine_lost: false,
            browser: Default::default(),
            timeline: Default::default(),
            params: Default::default(),
            params_open: false,
            toasts: Default::default(),
            busy: Default::default(),
            files,
            dialog_result: None,
            last_fit: None,
            clean_sig: history_sig(&TimelineView::default()),
            pending_discard: None,
            recover: None,
            #[cfg(not(target_arch = "wasm32"))]
            updates: None,
            #[cfg(not(target_arch = "wasm32"))]
            update_start: None,
            #[cfg(not(target_arch = "wasm32"))]
            restart: None,
            #[cfg(not(target_arch = "wasm32"))]
            update_busy: Default::default(),
            first_frame_done: false,
            last_autosave: web_time::Instant::now(),
            recovering: false,
            launch_file: None,
            start_view_done: false,
            tool: None,
            preview_seq: 0,
            pending_marker: None,
            committing: false,
            runtime_tools: HashMap::new(),
            gizmo: None,
            sculpt: Sculpt::new(),
            sf: Default::default(),
            sketch: None,
            sk: Default::default(),
        }
    }

    /// A path from the command line. A `.stlx` is opened as a project (`Command::Open`); any other
    /// file is inserted (`Command::Import`). The view follows when the bodies arrive, the same
    /// way File > Open and Insert do.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open_launch_file(&mut self, path: &std::path::Path) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                let msg = format!("{name}: {e}");
                eprintln!("{msg}");
                self.toasts.push(Level::Error, msg);
                return;
            }
        };
        let project = name.to_ascii_lowercase().ends_with(".stlx");
        if project {
            // goes through the same path as File > Open: the title, Recent, the isometric view and
            // "clean" follow once the engine has accepted the file (a refused file changes nothing)
            let file = crate::platform::PickedFile { name, bytes };
            self.files.open_bytes(&*self.link, file, Some(path.display().to_string()));
        } else {
            self.launch_file = Some(true);
            self.link.send(Command::Import { name, bytes });
        }
    }

    /// Turn autosave and Recover Unsaved Work on (the real window does this once at start-up; the
    /// empty window and tests don't, so they never touch the autosave folders). Offers what an
    /// earlier session left behind.
    pub fn enable_autosave(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        let mut r = Recover::desktop();
        #[cfg(target_arch = "wasm32")]
        let mut r = Recover::web();
        r.start();
        self.recover = Some(r);
        self.last_autosave = web_time::Instant::now();
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.updates = Some(crate::updates::Updates::new());
            self.update_start = Some(web_time::Instant::now());
        }
    }

    /// The update check: the start-up check after a few seconds, the banner and settings dialog,
    /// and what Update now asks of the window.
    #[cfg(not(target_arch = "wasm32"))]
    fn updates_frame(&mut self, ctx: &egui::Context, tk: &Tokens) {
        use crate::updates::{now_secs, UpdateAction};
        let Some(u) = self.updates.as_mut() else { return };
        if let Some(t) = self.update_start {
            let wait = std::time::Duration::from_secs(6);
            if t.elapsed() >= wait {
                self.update_start = None;
                u.start_background(&*self.platform, now_secs());
            } else {
                ctx.request_repaint_after(wait - t.elapsed());
            }
        }
        let action = u.show(ctx, tk, &*self.platform, now_secs());
        let note = u.busy_note();
        if u.is_busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        self.update_busy.set_busy(note.is_some());
        if let Some(n) = note {
            self.update_busy.label = n.to_string();
        }
        match action {
            Some(UpdateAction::Toast { text, warn }) => self.toasts.push(if warn { Level::Warning } else { Level::Info }, text),
            Some(UpdateAction::OpenUrl(url)) => ctx.open_url(egui::OpenUrl::new_tab(url)),
            // the copy of unsaved work is written first and kept: the new window offers it
            Some(UpdateAction::Restart { exe }) => {
                if self.shell.dirty {
                    self.link.send(self.autosave_command());
                }
                self.restart = Some((exe, web_time::Instant::now(), !self.shell.dirty));
            }
            None => {}
        }
        if let Some((exe, since, answered)) = self.restart.clone() {
            if answered || since.elapsed() >= std::time::Duration::from_millis(1500) {
                self.restart = None;
                // Closing the window drops the engine link, which keeps unsaved work for Recover.
                match wiring::restart_helper(&exe, std::process::id()).spawn() {
                    Ok(_) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                    Err(e) => self.toasts.push(Level::Error, format!("Couldn't restart STL Editor: {e}")),
                }
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
            }
        }
    }

    /// The autosave command with what the copy should say about the open project.
    fn autosave_command(&self) -> Command {
        Command::Autosave { path: self.files.project_path().map(str::to_string), title: self.files.project_name.clone() }
    }

    /// Every 60 s the engine is asked to write an autosave copy (it does so off this thread, and
    /// only when something changed).
    fn autosave_tick(&mut self, ctx: &egui::Context) {
        if self.recover.is_none() {
            return;
        }
        let every = std::time::Duration::from_secs(60);
        if self.last_autosave.elapsed() >= every {
            self.last_autosave = web_time::Instant::now();
            self.link.send(self.autosave_command());
        }
        ctx.request_repaint_after(every.saturating_sub(self.last_autosave.elapsed()));
    }

    /// A project came back from Recover Unsaved Work: open it like File > Open, and keep it marked
    /// unsaved (see `recovering`).
    fn open_recovered(&mut self, r: Recovered) {
        self.close_tool(false);
        self.shell.active_tool = None;
        self.shell.selection.clear();
        self.viewport.set_selection(&[]);
        self.viewport.set_section_enabled(false);
        self.recovering = true;
        // the file it was saved as, or `<title>.stlx` for work that never was
        let name = r.path.as_deref().and_then(|p| std::path::Path::new(p).file_name()).map_or(r.name, |n| n.to_string_lossy().into_owned());
        self.files.open_bytes(&*self.link, PickedFile { name, bytes: r.bytes }, r.path);
        self.toasts.push(Level::Info, recover::RECOVERED_TOAST.to_string());
    }

    /// The empty window: no engine, no files.
    pub fn empty(ctx: &egui::Context) -> Self {
        Self::new(ctx, Box::new(NullPlatform), Box::new(NullLink))
    }

    // ------------------------------------------------------------------ theme

    /// View > Theme: sets the preference, the egui style and the viewport colours.
    pub fn set_theme(&mut self, ctx: &egui::Context, pref: ThemePref) {
        self.shell.theme_pref = pref;
        let dark = match pref {
            ThemePref::Dark => true,
            ThemePref::Light => false,
            ThemePref::System => ctx.system_theme() != Some(egui::Theme::Light),
        };
        self.dark = dark;
        self.tokens = theme::tokens(dark);
        theme::apply(ctx, &self.tokens);
        self.viewport.set_theme(dark);
    }

    // ------------------------------------------------------------------ engine updates

    /// Apply the engine's updates. Returns true when there were any (the caller repaints).
    pub fn poll(&mut self) -> bool {
        let updates = self.link.poll();
        let mut any = !updates.is_empty();
        let out = self.files.poll(&*self.platform, &*self.link, &updates);
        any |= self.files.busy() || !out.messages.is_empty();
        // clean once the file is written (not when the engine hands over the bytes), or when a
        // project, a new document or a recent file just arrived
        let mut mark_clean = out.saved || out.view_iso;
        // a recovered project that has just opened is unsaved work, not a clean project
        let recovered_open = self.recovering && out.view_iso;
        if recovered_open {
            self.recovering = false;
            mark_clean = out.saved;
        }
        self.apply_file_outcome(out);
        for u in updates {
            match u {
                Update::Bodies(b) => {
                    self.viewport.sync_bodies(&b);
                    self.bodies = b;
                }
                Update::Timeline(t) => {
                    self.timeline_view = t;
                    self.pending_marker = None;
                    self.after_timeline();
                    self.sketch_after_timeline();
                }
                Update::Params(p) => self.param_views = p,
                Update::Planes(p) => {
                    self.viewport.sync_planes(&p);
                    self.planes = p;
                }
                Update::Sketches(s) => {
                    self.viewport.sync_sketches(&s, self.sketch.as_ref().and_then(|m| m.s.feature_id.as_deref()));
                    self.sketches = s;
                }
                Update::Message { level, text } => {
                    self.status_text = text.clone();
                    self.toasts.push(level, text);
                }
                Update::Progress { done, total, label } => self.busy.set_progress(done, total, &label),
                Update::Busy(on) => self.busy.set_busy(on),
                Update::Error(e) => {
                    self.recovering = false;
                    // a refused Finish leaves the sketch open (1.x stops it only after the add or edit worked)
                    if let Some(m) = self.sketch.as_mut() {
                        m.finishing = None;
                        m.solve_lost();
                    }
                    // the error belongs to one command: a turn, a handle's add or edit, or OK
                    if !self.pose_failed(&e) && !self.accept_failed(&e) {
                        self.commit_failed(&e);
                    }
                    self.toasts.push(Level::Error, e);
                }
                Update::Fins { seq, ask, result } => self.on_fins(seq, &ask, result),
                Update::SketchOp { seq, result } => self.on_sketch_op(seq, result),
                Update::Solved { seq, entities, ok, error } => self.on_solved(seq, entities, ok, error),
                Update::EngineLost => {
                    self.engine_lost = true;
                    if let Some(m) = self.sketch.as_mut() {
                        m.solve_lost();
                    }
                }
                Update::Preview { seq, pack, msg, error, warn, result } => self.on_preview(seq, pack, msg, error, warn, result),
                // TASK 9 (files.rs): Saved { bytes } and Exported { name, bytes, count } go to the
                // platform's save dialog.
                Update::Saved { .. } | Update::Exported { .. } => {}
                Update::Autosaved { ok: true, .. } => {
                    #[cfg(not(target_arch = "wasm32"))]
                    if let Some(r) = self.restart.as_mut() {
                        r.2 = true;
                    }
                }
                Update::Autosaved { ok: false, error } => {
                    #[cfg(not(target_arch = "wasm32"))]
                    if let Some(r) = self.restart.as_mut() {
                        r.2 = true;
                    }
                    self.toasts.push(Level::Error, error.unwrap_or_else(|| "Couldn't save an autosave copy.".to_string()));
                }
            }
        }
        if mark_clean {
            self.clean_sig = history_sig(&self.timeline_view);
        }
        if recovered_open {
            self.clean_sig = "recovered".to_string();
        }
        self.shell.dirty = history_sig(&self.timeline_view) != self.clean_sig;
        if self.launch_file.is_some() && !self.bodies.is_empty() {
            let import = self.launch_file.take() == Some(true);
            self.viewport.set_view("iso");
            if import {
                self.viewport.fit_all();
            }
        }
        any
    }

    fn apply_file_outcome(&mut self, out: FileOutcome) {
        for (l, t) in out.messages {
            self.status_text = t.clone();
            self.toasts.push(l, t);
        }
        if out.view_iso {
            self.viewport.set_view("iso");
        }
        if out.fit {
            self.viewport.fit_all();
        }
    }

    /// TASK 9 call site (files.rs / keys.rs): run what the menus, buttons and keys asked for.
    fn run_requests(&mut self, ctx: &egui::Context) {
        use crate::engine_link::ExportFormat;
        let reqs = std::mem::take(&mut self.shell.file_requests);
        for r in reqs {
            let act = match r {
                // 1.x asks "Discard unsaved changes?" before the file dialog (api.js 'open')
                UiAction::Open if self.shell.dirty => {
                    self.shell.dialogs.open_confirm(DISCARD_TAG, "Discard unsaved changes?");
                    self.pending_discard = Some(r);
                    None
                }
                UiAction::Open => Some((FileAction::Open, None)),
                UiAction::Insert => Some((FileAction::Import, None)),
                UiAction::Save => Some((FileAction::Save, None)),
                UiAction::SaveAs => Some((FileAction::SaveAs, None)),
                UiAction::Export(f) => ExportFormat::from_ext(&f).map(|f| (FileAction::Export(f), (!self.shell.selection.is_empty()).then(|| self.shell.selection.clone()))),
                UiAction::RemoveSelected => {
                    self.push_ui_event(UiEvent::OpenTool("delete".into()));
                    None
                }
                UiAction::New | UiAction::OpenRecent(_) => {
                    // 1.x asks the engine, which answers "Discard unsaved changes?" when there are some
                    if self.shell.dirty {
                        self.shell.dialogs.open_confirm(DISCARD_TAG, "Discard unsaved changes?");
                        self.pending_discard = Some(r);
                    } else {
                        self.replace_document(r);
                    }
                    None
                }
                // 1.x showHelp(), app.js:2676
                UiAction::ShowShortcuts => {
                    self.shell.dialogs.open_alert_rows("shortcuts", HELP_TITLE, HELP_ROWS);
                    None
                }
                // 1.x `checkRecovery(true)`: lists the copies, or says there is nothing
                UiAction::Recover => {
                    if let Some(r) = self.recover.as_mut() {
                        r.open_manual();
                    }
                    None
                }
                _ => None,
            };
            if let Some((a, ids)) = act {
                self.files.start(&*self.platform, &*self.link, a, ids);
            }
        }
        let _ = ctx;
    }

    /// New, or a recent file: the project is replaced (1.x `cmd('new' | 'open')`: close the tool,
    /// clear the section cut and the selection; the isometric view follows when the bodies arrive).
    fn replace_document(&mut self, what: UiAction) {
        if what == UiAction::Open {
            // the file dialog comes first; the tool and selection stay until a file is chosen
            self.files.start(&*self.platform, &*self.link, FileAction::Open, None);
            return;
        }
        self.shell.active_tool = None;
        self.shell.selection.clear();
        self.viewport.set_selection(&[]);
        self.viewport.set_section_enabled(false);
        match what {
            UiAction::OpenRecent(name) => {
                if !self.files.open_recent(&*self.platform, &*self.link, &name) {
                    self.toasts.push(Level::Error, "That file is no longer in the recent list.".to_string());
                }
            }
            _ => self.files.new_document(&*self.link),
        }
    }

    /// What `App.onKey` reads from `this`. `in_field` is filled by `keys::handle` (egui knows).
    fn key_state(&self) -> KeyState {
        KeyState {
            in_field: false,
            // `sketcher.active`, `walls.*` and the sketch's length boxes: the sketcher and the
            // Support Fins wall editor arrive in 4c / 4b, so these stay false until then
            dyn_field_focused: false,
            sketcher_active: self.sketch.is_some(),
            walls_active: self.walls_on(),
            walls_pending: self.walls_pending(),
            tool_open: self.tool.is_some() || self.shell.active_tool.is_some(),
            // `tool.def.gizmo === 'move'`: only the Move tool has the gizmo
            tool_gizmo_move: self.tool.as_ref().is_some_and(|t| t.def.key == "move"),
            // `mode !== 'model'` is the Info mode, which comes with the Inspect tools in 4b
            mode_not_model: false,
            selection_nonempty: !self.shell.selection.is_empty(),
        }
    }

    fn run_keys(&mut self, ctx: &egui::Context) {
        let st = self.key_state();
        for a in keys::handle(ctx, &st) {
            if let Some(c) = a.command() {
                self.send(c);
            } else if let Some(f) = a.file_action() {
                if f == FileAction::Open && self.shell.dirty {
                    self.shell.file_requests.push(UiAction::Open);
                    continue;
                }
                self.files.start(&*self.platform, &*self.link, f, None);
            } else if let Some(e) = a.event() {
                if let UiEvent::Select(ids) = &e {
                    self.shell.selection = ids.clone();
                    self.viewport.set_selection(ids);
                }
                self.push_ui_event(e);
            } else {
                match a {
                    KeyAction::FocusSearch => self.shell.focus_search = true,
                    KeyAction::Fit => self.viewport.fit_all(),
                    KeyAction::View(v) => self.viewport.set_view(v),
                    KeyAction::ToggleCut => {
                        let on = self.viewport.section_enabled();
                        self.viewport.set_section_enabled(!on);
                    }
                    KeyAction::New => self.shell.file_requests.push(UiAction::New),
                    KeyAction::BeginSketch => self.push_ui_event(UiEvent::OpenTool("sketch".into())),
                    KeyAction::PanelCancel => self.tool_cancel(),
                    KeyAction::PanelOk => self.tool_ok(),
                    KeyAction::Walls(_) | KeyAction::Gizmo { .. } => self.tool_key(&a),
                    KeyAction::Sketcher(press) => self.sketch_key(press),
                    _ => {}
                }
            }
        }
    }

    // ------------------------------------------------------------------ tools

    /// Push a UI event; tool events open the tool at once (Task 3's ribbon, search, keys and
    /// timeline all come through here).
    pub fn push_ui_event(&mut self, e: UiEvent) {
        match &e {
            UiEvent::OpenTool(k) => {
                self.open_tool(k, None);
            }
            UiEvent::OpenToolWith(k, v) => {
                self.open_tool(k, Some(v.clone()));
            }
            UiEvent::EditFeature(id) => {
                self.edit_feature(id);
            }
            _ => {}
        }
        self.shell.push_event(e);
    }

    /// Run-time tool definitions (Support Fins builds its tools from the engine's schema).
    pub fn register_tool_def(&mut self, def: &'static ToolDef) {
        self.runtime_tools.insert(def.key.to_owned(), def);
    }

    pub fn find_tool(&self, key: &str) -> Option<&'static ToolDef> {
        self.runtime_tools.get(key).copied().or_else(|| crate::tools::tool(key)).or_else(|| crate::supports::support_tool(key))
    }

    pub fn tool(&self) -> Option<&OpenTool> {
        self.tool.as_ref()
    }

    /// `setValue(key, val)` on the open tool: `onChange` runs and a preview is scheduled.
    pub fn tool_set_value(&mut self, key: &str, val: serde_json::Value) {
        let ctx = make_ctx(&self.bodies, &self.planes, &self.sketches, &self.timeline_view.items, &self.shell.selection, self.shell.inch);
        if let Some(t) = self.tool.as_mut() {
            t.set_value(key, val, &ctx);
        }
    }

    /// Several values at once (the gizmo's `translate` / `rotate` / `size`); one preview follows.
    pub fn tool_set_values(&mut self, patch: Values) {
        let ctx = make_ctx(&self.bodies, &self.planes, &self.sketches, &self.timeline_view.items, &self.shell.selection, self.shell.inch);
        if let Some(t) = self.tool.as_mut() {
            t.set_values(patch, &ctx);
        }
    }

    pub fn tool_set_active_field(&mut self, key: Option<String>) {
        if let Some(t) = self.tool.as_mut() {
            t.active_field = key;
        }
    }

    pub fn tool_mut(&mut self) -> Option<&mut OpenTool> {
        self.tool.as_mut()
    }

    /// The rollback marker as it is now, counting one we sent and haven't heard back.
    fn marker_now(&self) -> Option<usize> {
        self.pending_marker.unwrap_or(self.timeline_view.marker)
    }

    /// `openTool(key, { values })`. False when there is no such tool.
    pub fn open_tool(&mut self, key: &str, values: Option<Values>) -> bool {
        if key == "sketch" {
            self.begin_sketch(None);
            return true;
        }
        if matches!(key, "supportfins" | "supportwall") {
            // Support Fins' settings are read from its engine first (once)
            self.open_support(key, None, values);
            return true;
        }
        match self.find_tool(key) {
            Some(def) => {
                self.open_tool_def(def, None, values);
                true
            }
            None => false,
        }
    }

    /// `openTool(key, { editFeature })`: reopen the tool of a timeline step.
    pub fn edit_feature(&mut self, id: &str) -> bool {
        let Some(item) = self.timeline_view.items.iter().find(|i| i.id == id).cloned() else { return false };
        if item.kind == "sketch" {
            self.close_tool(false);
            self.enter_sketch(None, Some(&item));
            return true;
        }
        let Some(key) = crate::tools::panel::tool_key_for(&item.kind, &item.params) else { return false };
        if matches!(key.as_str(), "supportfins" | "supportwall") {
            self.open_support(&key, Some(item.id), None);
            return true;
        }
        let Some(def) = self.find_tool(&key) else { return false };
        self.open_tool_def(def, Some((item.id, item.params)), None);
        true
    }

    /// Open any `ToolDef`, also one built at run time (`edit`: the step's id and params).
    pub fn open_tool_def(&mut self, def: &'static ToolDef, edit: Option<(String, serde_json::Value)>, values: Option<Values>) {
        self.close_tool(false);
        let ctx = make_ctx(&self.bodies, &self.planes, &self.sketches, &self.timeline_view.items, &self.shell.selection, self.shell.inch);
        let mut v = match &edit {
            Some((_, p)) => (def.from_params)(p.clone()),
            None => (def.defaults)(&ctx),
        };
        if let Some(extra) = values {
            for (k, x) in extra {
                v.insert(k, x);
            }
        }
        let prev = self.marker_now();
        let edit_id = edit.map(|(id, _)| id);
        if let Some(id) = &edit_id {
            if let Some(idx) = self.timeline_view.items.iter().position(|f| &f.id == id) {
                self.link.send(Command::Rollback(Some(idx)));
                self.pending_marker = Some(Some(idx));
            }
        }
        let mut t = OpenTool::new(def, v, edit_id, prev);
        t.preview.schedule(0, def.flags.no_live_preview);
        self.shell.hint = def.hint.unwrap_or("").to_owned();
        self.shell.active_tool = Some(def.key.to_owned());
        self.viewport.set_viewcube_inset(crate::tools::panel::VIEWCUBE_INSET);
        self.tool = Some(t);
        self.after_open(def);
    }

    /// `closeTool(committed)`: Cancel puts the rollback back when a step was being edited.
    pub fn close_tool(&mut self, committed: bool) {
        self.committing = false;
        let Some(t) = self.tool.take() else { return };
        self.before_close(&t);
        self.viewport.clear_preview();
        self.link.send(Command::ClearPreview);
        if t.edit_id.is_some() && !committed {
            self.link.send(Command::Rollback(t.prev_rollback));
            self.pending_marker = Some(t.prev_rollback);
        }
        self.shell.hint.clear();
        self.shell.active_tool = None;
        self.viewport.set_viewcube_inset(0.0);
    }

    pub fn tool_cancel(&mut self) {
        self.close_tool(false);
    }

    /// `commitTool()`: the OK button.
    pub fn tool_ok(&mut self) {
        use crate::tools::preview::{MsgKind, ToolMsg};
        let params: BTreeMap<String, f64> = self.param_views.iter().filter_map(|p| p.value.map(|v| (p.name.clone(), v))).collect();
        let ctx = make_ctx(&self.bodies, &self.planes, &self.sketches, &self.timeline_view.items, &self.shell.selection, self.shell.inch);
        if self.committing {
            return; // the first OK is still on its way
        }
        if self.tool.as_ref().is_some_and(|t| t.def.key == "sketch") {
            return self.sketch_prompt_ok(); // Start: no step is added, the sketch opens
        }
        let Some(t) = self.tool.as_mut() else { return };
        if !t.valid() {
            t.msg = Some(ToolMsg::new(MsgKind::Err, "Fill in the highlighted selections first."));
            return;
        }
        if let Some(e) = t.expr_error(&ctx, &params) {
            t.msg = Some(ToolMsg::new(MsgKind::Err, e));
            return;
        }
        if t.accepting.is_some() {
            // a handle's Add is still on its way: OK goes out as an edit of that step once it is
            // in (`after_timeline`), not as a second Add, and the Add's answer isn't taken as OK's
            t.ok_after_add = true;
            t.msg = Some(ToolMsg::new(MsgKind::Busy, format!("{}…", t.def.label)));
            return;
        }
        let p = t.params(&ctx);
        let kind = t.def.kind.to_owned();
        match t.edit_id.clone() {
            Some(id) => {
                // one transaction (1.x commitTool: feature/edit with rollback_after): Ctrl+Z puts back
                // the steps that came after it
                self.link.send(Command::EditFeature { id, params: p, rollback_after: Some(t.prev_rollback) });
                t.msg = Some(ToolMsg::new(MsgKind::Busy, format!("Updating {}…", t.def.label)));
            }
            None => {
                t.msg = Some(ToolMsg::new(MsgKind::Busy, format!("{}…", t.def.label)));
                self.add_step(kind, p);
            }
        }
        // closed by `after_timeline` when the engine has taken it; kept open if it refuses
        self.committing = true;
    }

    /// `schedulePreview` fired: check the clock. Tests pass their own `now`.
    pub fn run_previews(&mut self, now: web_time::Instant) {
        use crate::tools::preview::{MsgKind, ToolMsg};
        let fire = {
            let Some(t) = self.tool.as_mut() else { return };
            if t.preview.busy_shown(now) {
                t.msg = Some(ToolMsg::new(MsgKind::Busy, if t.def.flags.wall_tool { "Building the walls…" } else { "Building supports…" }));
            }
            t.preview.tick(now)
        };
        if fire {
            self.run_preview_now(now);
        }
    }

    /// `runPreview()` at once (the Preview button, the delay passing).
    pub fn run_preview_now(&mut self, now: web_time::Instant) {
        let ctx = make_ctx(&self.bodies, &self.planes, &self.sketches, &self.timeline_view.items, &self.shell.selection, self.shell.inch);
        let Some(t) = self.tool.as_mut() else { return };
        self.preview_seq += 1;
        let seq = self.preview_seq;
        if !t.valid() || (t.def.flags.sculpt && arr_empty(t.values.get("strokes"))) {
            // a preview still on its way is out of date now: it mustn't land after this
            t.preview.invalidate(seq);
            t.msg = None;
            let paint_no_body = t.def.flags.paint && crate::supports::body_id(&t.values).is_none();
            self.viewport.clear_preview();
            // no part picked: its red overhangs and the coverage go too (with a part, they stay)
            if paint_no_body {
                self.sf_quick_paint();
            }
            return;
        }
        t.preview.start(seq, now, t.def.flags.paint);
        let params = t.params(&ctx);
        self.link.send(Command::Preview { seq, kind: t.def.kind.to_owned(), params, edit_id: t.edit_id.clone() });
    }

    /// A click in the viewport while a tool is open (`feedPick`): the active pick field takes it.
    fn tool_click(&mut self, pos: Pos2) {
        use crate::tools::fields::bodies as pk;
        if self.tool.is_none() {
            return;
        }
        let planes = self.viewport.base_planes_visible() || !self.planes.is_empty();
        let Some(hit) = self.viewport.pick(pos, true, planes) else { return };
        let ctx = make_ctx(&self.bodies, &self.planes, &self.sketches, &self.timeline_view.items, &self.shell.selection, self.shell.inch);
        let Some(t) = self.tool.as_mut() else { return };
        let out = pk::feed(t.def, t.active_field.as_deref(), &mut t.values, &hit, &ctx);
        if !out.used {
            return;
        }
        if out.changed {
            if out.advance {
                if let Some(cur) = t.active_field.clone() {
                    t.active_field = pk::advance_field(t.def, &t.values, &cur);
                }
            }
            t.schedule_preview(60);
        }
        if let Some((l, m)) = &out.message {
            t.msg = Some(crate::tools::preview::ToolMsg::new(if *l == Level::Error { crate::tools::preview::MsgKind::Err } else { crate::tools::preview::MsgKind::Info }, m.clone()));
        }
        pk::apply_face(&mut self.viewport, &out);
        // the plane prompt: a picked plane starts the sketch (`onPlane`)
        if out.changed && self.tool.as_ref().is_some_and(|t| t.def.key == "sketch") {
            return self.sketch_prompt_ok();
        }
        // Support Fins: a part that already has supports opens those instead of a second set
        if let Some(bid) = out.reopen_supports.clone() {
            let key = self.tool.as_ref().filter(|t| t.edit_id.is_none()).map(|t| t.def.key);
            if let Some(key) = key {
                if self.sf_reopen(key, &bid, None) {
                    return;
                }
            }
        }
        if out.quick_paint {
            self.sf_quick_paint();
        }
    }

    /// `renderOverlayForTool`: what the fields want drawn, redrawn only when the values change.
    fn tool_marks(&mut self) {
        use crate::tools::fields::bodies as pk;
        let ctx = make_ctx(&self.bodies, &self.planes, &self.sketches, &self.timeline_view.items, &self.shell.selection, self.shell.inch);
        let Some(t) = self.tool.as_mut() else { return };
        let sig = format!("{:?}|{:?}|{}", t.values, t.active_field, self.bodies.len() + self.planes.len() + self.sketches.len());
        if t.marks_sig == sig {
            return;
        }
        t.marks_sig = sig;
        let marks = pk::tool_marks(t.def, &t.values, &ctx);
        pk::apply_marks(&mut self.viewport, &marks);
    }

    /// Draw the tool panel and act on its buttons.
    fn tool_panel(&mut self, egui_ctx: &egui::Context, tk: &Tokens, ws: Rect) {
        let params: BTreeMap<String, f64> = self.param_views.iter().filter_map(|p| p.value.map(|v| (p.name.clone(), v))).collect();
        let ctx = make_ctx(&self.bodies, &self.planes, &self.sketches, &self.timeline_view.items, &self.shell.selection, self.shell.inch);
        let Some(t) = self.tool.as_mut() else { return };
        let env = crate::tools::panel::PanelEnv { tk, inch: self.shell.inch, params: &params, ctx: &ctx, ws };
        let out = t.show(egui_ctx, &env);
        if out.clear_face {
            self.viewport.clear_overlay("face");
        }
        self.tool_marks();
        if !out.sf_events.is_empty() {
            self.sf_panel_events(out.sf_events.clone());
        }
        let now = web_time::Instant::now();
        if out.ok {
            self.tool_ok();
        } else if out.cancel {
            self.tool_cancel();
        } else if out.preview_now {
            self.run_preview_now(now);
        }
    }

    /// `runPreview`'s answer. Only the newest preview of the open tool lands; one for a closed
    /// tool, or while the gizmo is dragged, is dropped.
    fn on_preview(&mut self, seq: u64, pack: crate::viewport::PreviewPack, msg: Option<String>, error: Option<String>, warn: bool, result: serde_json::Value) {
        use crate::tools::preview::{MsgKind, ToolMsg};
        {
            let Some(t) = self.tool.as_mut() else { return };
            if !t.preview.accepts(seq) {
                return;
            }
            if t.dragging {
                // no longer waiting for it (no "Building…" left up); asked for again on let-go
                t.preview.done();
                t.preview_dropped = true;
                return;
            }
        }
        // a handle is held: the late preview must not snap the part back (the moved bodies are
        // put back first when one lands)
        if let Some(g) = self.gizmo.as_mut() {
            if !g.accept_preview(&mut self.viewport) {
                if let Some(t) = self.tool.as_mut() {
                    t.preview.done();
                    t.preview_dropped = true;
                }
                return;
            }
        }
        if let Some(t) = self.tool.as_mut() {
            t.preview_dropped = false;
        }
        let Some(t) = self.tool.as_mut() else { return };
        t.preview.done();
        if let Some(e) = error {
            t.last_result = result;
            t.msg = Some(ToolMsg::new(MsgKind::Err, e));
            self.viewport.clear_preview();
            self.sf_on_preview_error();
            return;
        }
        self.viewport.show_preview(&pack);
        let (key, wall_tool) = (t.def.key, t.def.flags.wall_tool);
        let mode = crate::supports::mode_of(&t.values).to_owned();
        let (mut msg, mut warn) = (msg.filter(|m| !m.is_empty()), warn);
        if warn {
            if let Some(m) = msg.clone() {
                if key == "supportfins" && mode == "rotate" {
                    // "no overhangs in this position" is said by the Held card, not again up here
                    let w = crate::supports::warning_line("rotate", &m);
                    if w.trim().is_empty() {
                        warn = false;
                        msg = if result.get("flipped").and_then(|v| v.as_bool()).unwrap_or(false) {
                            Some("Cutting into the material (direction flipped).".to_owned())
                        } else {
                            result.get("info").and_then(|v| v.as_str()).filter(|i| !i.is_empty()).map(str::to_owned)
                        };
                    } else {
                        msg = Some(w.trim().to_owned());
                    }
                } else if wall_tool {
                    if let Some(info) = result.get("info").and_then(|v| v.as_str()).filter(|i| !i.is_empty()) {
                        msg = Some(format!("{info}. {m}"));
                    }
                }
            }
        }
        if let Some(t) = self.tool.as_mut() {
            t.msg = msg.map(|m| ToolMsg::new(if warn { MsgKind::Warn } else { MsgKind::Info }, m));
        }
        self.sf_on_preview(&result, &pack);
        // the record is kept for whoever reads it next (Support Fins reads paint, walls, pieces)
        if let Some(t) = self.tool.as_mut() {
            t.last_result = result;
        }
    }

    // ------------------------------------------------------------------ actions

    fn send(&mut self, c: Command) {
        // inside a sketch Undo is the sketch's own (`undoLocal`); the document's history waits
        if matches!(c, Command::Undo | Command::Redo) && self.sketch.is_some() {
            if matches!(c, Command::Undo) {
                if let Some(m) = self.sketch.as_mut() {
                    m.s.undo_local();
                }
                self.sketch_apply(vec![crate::sketch::SketchEffect::Changed]);
            }
            return;
        }
        // 1.x: Undo and Redo close the open tool first (a rolled-back edit session is put back)
        if matches!(c, Command::Undo | Command::Redo) && self.tool.is_some() {
            self.close_tool(false);
        }
        self.link.send(c);
    }

    /// The ribbon's tab: the sketch tab while a sketch is open (the model tab comes back by itself).
    pub fn ribbon_tab(&self) -> &str {
        if self.sketch.is_some() {
            ribbon::SKETCH_TAB
        } else {
            &self.shell.tab
        }
    }

    /// A ribbon / search / dropdown tool was chosen (`App.onRibbon`).
    pub fn on_ribbon(&mut self, key: &str, anchor: Pos2) {
        if self.sketch.is_some() {
            return self.sketch_ribbon(key);
        }
        match key {
            "import" => self.shell.file_requests.push(UiAction::Insert),
            "export" => self.open_export_menu(anchor),
            "params" => { self.params_open = true; self.params.set_open(true); }
            // Send to Slicer is desktop-only and not part of 4a
            "send" => {}
            "sf_about" => self.show_sf_about(),
            // Rotate opens Move with the rings (`_gizmo: "rotate"`)
            k => self.push_ui_event(ribbon::tool_event(k)),
        }
    }

    fn open_export_menu(&mut self, at: Pos2) {
        let n = self.shell.selection.len();
        let hdr = if n > 0 { format!("Selected bodies ({n})") } else { "All visible bodies".to_owned() };
        let items = vec![
            MenuItem::Hdr(hdr),
            MenuItem::item("export:stl", "STL (.stl)"),
            MenuItem::item("export:3mf", "3MF (.3mf)"),
            MenuItem::item("export:obj", "OBJ (.obj)"),
            MenuItem::item("export:step", "STEP (.step), flat faces").title("Curves become flat faces. Opens in CAD programs as a solid."),
        ];
        self.shell.menu = Some(OpenMenu::new("export", items, at));
    }

    fn top_menu(&self, key: &str) -> Vec<MenuItem> {
        let web = self.platform.is_web();
        let tv = &self.timeline_view;
        let tick = |on: bool, s: &str| format!("{}{s}", if on { "✓ " } else { "" });
        fn it(id: impl AsRef<str>, l: impl AsRef<str>) -> MenuItem {
            MenuItem::item(id.as_ref(), l.as_ref())
        }
        match key {
            "file" => {
                let mut v = vec![it("file.new", "New").key("Ctrl+N"), it("file.open", "Open…").key("Ctrl+O")];
                if self.shell.recent.is_empty() {
                    v.push(MenuItem::Sep);
                } else {
                    v.push(MenuItem::Hdr("Recent".into()));
                    for p in self.shell.recent.iter().take(6) {
                        let name = p.rsplit(['/', '\\']).next().unwrap_or(p).to_owned();
                        v.push(it(format!("recent:{p}"), name).title(p.clone()));
                    }
                    v.push(MenuItem::Sep);
                }
                v.extend([
                    it("file.insert", "Insert Mesh…").key("Ctrl+I"),
                    it("file.save", "Save").key("Ctrl+S"),
                    it("file.save_as", "Save As…").key("Ctrl+Shift+S"),
                    MenuItem::Sep,
                    it("export:stl", "Export STL…"),
                    it("export:3mf", "Export 3MF…"),
                    it("export:obj", "Export OBJ…"),
                ]);
                if !web {
                    v.extend([MenuItem::Sep, it("file.quit", "Quit")]);
                }
                v
            }
            "edit" => vec![
                it("edit.undo", "Undo").key("Ctrl+Z").disabled(!tv.can_undo),
                it("edit.redo", "Redo").key("Ctrl+Y").disabled(!tv.can_redo),
                MenuItem::Sep,
                it("edit.params", "Parameters…"),
                MenuItem::Sep,
                it("edit.select_all", "Select All Bodies"),
                it("edit.remove", "Remove Selected Bodies").key("Del").disabled(self.shell.selection.is_empty()),
            ],
            "view" => {
                let s = &self.shell;
                vec![
                    it("view.fit", "Fit All").key("F"),
                    it("view.iso", "Isometric").key("0"),
                    it("view.top", "Top").key("5"),
                    it("view.front", "Front").key("1"),
                    it("view.right", "Right").key("4"),
                    it("view.section", tick(self.viewport.section_enabled(), "Section View")).key("X"),
                    MenuItem::Sep,
                    MenuItem::Hdr("Units".into()),
                    it("units.mm", tick(!s.inch, "Millimetres")),
                    it("units.in", tick(s.inch, "Inches")),
                    MenuItem::Hdr("Theme".into()),
                    it("theme.light", tick(s.theme_pref == ThemePref::Light, "Light")),
                    it("theme.dark", tick(s.theme_pref == ThemePref::Dark, "Dark")),
                    it("theme.system", tick(s.theme_pref == ThemePref::System, "Match Windows")),
                    MenuItem::Sep,
                    it("view.edges", tick(s.show_edges, "Show Edges")),
                    it("view.grid", tick(s.show_grid, "Show Grid")),
                    it("view.planes", tick(self.viewport.base_planes_visible(), "Show Origin Planes")),
                ]
            }
            _ => {
                let mut v = vec![it("help.shortcuts", "Keyboard Shortcuts")];
                if web {
                    v.extend([it("help.guide", "User Guide"), it("help.desktop", "Get the Desktop App"), it("help.issue", "Report a Problem")]);
                }
                v.extend(crate::updates::help_menu_items(web).into_iter().map(|(id, label)| it(id, label)));
                v.extend([it("help.recover", "Recover Unsaved Work…"), it("help.kofi", "Support STL Editor ♥"), it("help.about", "About STL Editor")]);
                v
            }
        }
    }

    fn on_menu_pick(&mut self, ctx: &egui::Context, tag: &str, id: &str, anchor: Pos2) {
        let a = |s: &mut Self, x: UiAction| s.shell.file_requests.push(x);
        if let Some(k) = id.strip_prefix("tool:") {
            return self.on_ribbon(k, anchor);
        }
        if let Some(rest) = id.strip_prefix("drop:") {
            if let Some((d, k)) = rest.split_once(':') {
                self.shell.drop_last.insert(d.to_owned(), k.to_owned());
                self.on_ribbon(k, anchor);
            }
            return;
        }
        if let Some(k) = id.strip_prefix("search:") {
            return self.run_search_command(ctx, k, anchor);
        }
        if let Some(f) = id.strip_prefix("export:") {
            return a(self, UiAction::Export(f.to_owned()));
        }
        if let Some(p) = id.strip_prefix("recent:") {
            return a(self, UiAction::OpenRecent(p.to_owned()));
        }
        let _ = tag;
        match id {
            "file.new" => a(self, UiAction::New),
            "file.open" => a(self, UiAction::Open),
            "file.insert" => a(self, UiAction::Insert),
            "file.save" => a(self, UiAction::Save),
            "file.save_as" => a(self, UiAction::SaveAs),
            "file.quit" => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            "edit.undo" => self.send(Command::Undo),
            "edit.redo" => self.send(Command::Redo),
            "edit.params" => { self.params_open = true; self.params.set_open(true); }
            "edit.select_all" => {
                let ids: Vec<String> = self.bodies.iter().map(|b| b.id.clone()).collect();
                self.viewport.set_selection(&ids);
                self.shell.selection = ids.clone();
                self.shell.push_event(UiEvent::Select(ids));
            }
            "edit.remove" => a(self, UiAction::RemoveSelected),
            "view.fit" => self.viewport.fit_all(),
            "view.iso" => self.viewport.set_view("iso"),
            "view.top" => self.viewport.set_view("top"),
            "view.front" => self.viewport.set_view("front"),
            "view.right" => self.viewport.set_view("right"),
            "view.section" => {
                let on = self.viewport.section_enabled();
                self.viewport.set_section_enabled(!on);
            }
            "units.mm" | "units.in" => self.set_units_inch(id == "units.in"),
            "theme.light" => self.set_theme(ctx, ThemePref::Light),
            "theme.dark" => self.set_theme(ctx, ThemePref::Dark),
            "theme.system" => self.set_theme(ctx, ThemePref::System),
            "view.edges" => {
                self.shell.show_edges = !self.shell.show_edges;
                self.viewport.set_edges_visible(self.shell.show_edges);
            }
            "view.grid" => {
                self.shell.show_grid = !self.shell.show_grid;
                self.viewport.set_grid_visible(self.shell.show_grid);
            }
            "view.planes" => {
                let on = self.viewport.base_planes_visible();
                self.viewport.set_base_planes_visible(!on);
            }
            "help.shortcuts" => a(self, UiAction::ShowShortcuts),
            "help.guide" => ctx.open_url(egui::OpenUrl::new_tab(GUIDE_URL)),
            "help.desktop" => ctx.open_url(egui::OpenUrl::new_tab(DESKTOP_URL)),
            "help.issue" => ctx.open_url(egui::OpenUrl::new_tab(ISSUES_URL)),
            "help.recover" => a(self, UiAction::Recover),
            #[cfg(not(target_arch = "wasm32"))]
            "help.check_updates" => {
                if let Some(u) = self.updates.as_mut() {
                    u.check_now(&*self.platform, crate::updates::now_secs());
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            "help.update_settings" => {
                if let Some(u) = self.updates.as_mut() {
                    u.open_settings(&*self.platform);
                }
            }
            "help.kofi" => ctx.open_url(egui::OpenUrl::new_tab(KOFI)),
            "help.about" => {
                let web = self.platform.is_web();
                let mut links = Vec::new();
                if web {
                    links.push(("source on GitHub".to_owned(), "https://github.com/natterman12/STL-Editor".to_owned()));
                }
                links.push(("Buy me a coffee on Ko-fi".to_owned(), KOFI.to_owned()));
                self.shell.dialogs.open_alert("about", &crate::build_info::title(), &about_text(web), links);
            }
            _ => {}
        }
    }

    /// Choosing a tool-search result: switch to its tab when the current one doesn't have it.
    fn run_search_command(&mut self, ctx: &egui::Context, key: &str, anchor: Pos2) {
        self.shell.query.clear();
        self.shell.menu = None;
        ctx.memory_mut(|m| m.surrender_focus(Id::unique(topbar::SEARCH_ID)));
        if let Some(c) = ribbon::all_commands_in(self.sketch.is_some()).into_iter().find(|c| c.k == key) {
            if let Some(tab) = c.tab.filter(|_| self.sketch.is_none()) {
                let here = ribbon::def(&self.shell.tab).groups.iter().any(|g| g.tools().iter().any(|x| x.key == key));
                if tab != self.shell.tab && !here {
                    self.shell.tab = tab.to_owned();
                }
            }
        }
        self.on_ribbon(key, anchor);
    }

    fn update_search_menu(&mut self, rect: Rect) {
        let results = ribbon::search_in(&self.shell.query, self.sketch.is_some());
        if self.shell.query.trim().is_empty() {
            if self.shell.menu.as_ref().is_some_and(|m| m.tag == "search") {
                self.shell.menu = None;
            }
            return;
        }
        let tab_name = |t: &str| if t == ribbon::SKETCH_TAB { Some("Sketch".to_owned()) } else { ribbon::TABS.iter().find(|x| x.0 == t).map(|x| x.1.to_owned()) };
        let items: Vec<MenuItem> = if results.is_empty() {
            vec![MenuItem::item("none", "No matching tools").disabled(true)]
        } else {
            results.iter().map(|c| MenuItem::item(format!("search:{}", c.k), c.l.clone()).icon(c.ic.clone()).key(c.tab.and_then(tab_name).unwrap_or_else(|| c.group.clone()))).collect()
        };
        let hot = if results.is_empty() { None } else { Some(self.shell.search_hot.min(results.len() - 1)) };
        let mut m = OpenMenu::new("search", items, Pos2::new(rect.min.x, rect.max.y + 2.0)).hot(hot).keep(rect);
        if let Some(old) = &self.shell.menu {
            if old.tag == "search" {
                m = OpenMenu { ..m };
            }
        }
        self.shell.menu = Some(m);
    }

    fn dispatch_pick(&mut self, ctx: &egui::Context, p: ribbon::Pick) {
        use ribbon::Pick;
        match p {
            Pick::Tab(k) => {
                if self.sketch.is_none() {
                    self.shell.tab = k.to_owned();
                }
            }
            Pick::Tool(k) => self.on_ribbon(k, Pos2::new(200.0, 140.0)),
            Pick::Drop(id, r) => {
                let d = ribbon::def(self.ribbon_tab());
                for g in &d.groups {
                    for item in &g.items {
                        if let ribbon::Item::Drop { id: did, items, .. } = item {
                            if *did == id {
                                let rows = items.iter().map(|x| MenuItem::item(format!("drop:{id}:{}", x.key), x.label).icon(x.icon)).collect();
                                self.shell.menu = Some(OpenMenu::new(format!("drop:{id}"), rows, r.left_bottom() + Vec2::new(0.0, 2.0)).keep(r));
                            }
                        }
                    }
                }
            }
            Pick::More(r) => {
                let mut rows = Vec::new();
                if let Some(f) = &self.last_fit {
                    for (label, tools) in ribbon::folded_groups(self.ribbon_tab(), f) {
                        rows.push(MenuItem::Hdr(label.to_owned()));
                        rows.extend(tools.iter().map(|x| MenuItem::item(format!("tool:{}", x.key), x.label).icon(x.icon)));
                    }
                }
                self.shell.menu = Some(OpenMenu::new("more", rows, r.left_bottom() + Vec2::new(0.0, 2.0)).keep(r));
            }
            Pick::GroupMenu(gi, r) => {
                let d = ribbon::def(self.ribbon_tab());
                if let Some(g) = d.groups.get(gi) {
                    let rows = g.tools().iter().map(|x| MenuItem::item(format!("tool:{}", x.key), x.label).icon(x.icon)).collect();
                    self.shell.menu = Some(OpenMenu::new("group", rows, r.left_bottom() + Vec2::new(0.0, 2.0)).keep(r));
                }
            }
        }
        let _ = ctx;
    }

    // ------------------------------------------------------------------ layout

    /// Lay the whole window out. `eframe::App::ui` calls this; tests call it directly.
    pub fn layout(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.poll() {
            ctx.request_repaint();
        }
        self.autosave_tick(&ctx);
        let drop = self.files.handle_drop(&ctx, &*self.link);
        self.shell.dragging_files = self.files.dragging;
        self.apply_file_outcome(drop);
        self.shell.doc_name = self.files.title.clone().unwrap_or_else(|| "Untitled".to_owned());
        self.shell.recent = self.files.recent_keys();
        self.run_keys(&ctx);
        self.run_previews(web_time::Instant::now());
        if let Some(t) = &self.tool {
            if let Some(d) = t.preview.next_wake(web_time::Instant::now()) {
                ctx.request_repaint_after(d);
            }
        }
        let tk = self.tokens.clone();

        // ---- top bar
        let mut tb_out = topbar::TopbarOut::empty();
        egui::Panel::top("topbar").exact_size(topbar::HEIGHT).frame(Frame::NONE).show_separator_line(false).show(ui, |ui| {
            let open = self.shell.menu.as_ref().and_then(|m| m.tag.strip_prefix("top:"));
            let focus = std::mem::take(&mut self.shell.focus_search);
            let mut st = topbar::TopbarState {
                title: &self.shell.doc_name,
                dirty: self.shell.dirty,
                can_undo: self.timeline_view.can_undo,
                can_redo: self.timeline_view.can_redo,
                open_menu: open.map(|s| s.to_owned()).as_deref().and_then(|s| topbar::MENUS.iter().find(|m| m.0 == s).map(|m| m.0)),
                query: &mut self.shell.query,
                focus_search: focus,
            };
            tb_out = topbar::show(ui, &tk, &mut st);
        });
        if let Some((k, r)) = tb_out.menu {
            let items = self.top_menu(k);
            self.shell.menu = Some(OpenMenu::new(format!("top:{k}"), items, r.left_bottom() + Vec2::new(0.0, 2.0)).keep(r));
        }
        if let Some((q, r)) = tb_out.quick {
            let at = r.left_bottom() + Vec2::new(0.0, 2.0);
            match q {
                "save" => self.shell.file_requests.push(UiAction::Save),
                "undo" => self.send(Command::Undo),
                "redo" => self.send(Command::Redo),
                "insert" => self.shell.file_requests.push(UiAction::Insert),
                _ => self.open_export_menu(at),
            }
        }
        // tool search: arrows move the highlight, Enter runs it, Escape clears
        if tb_out.search_changed {
            self.shell.search_hot = 0;
            self.update_search_menu(tb_out.search_rect);
        }
        if let Some(k) = tb_out.search_key {
            let n = ribbon::search_in(&self.shell.query, self.sketch.is_some()).len();
            match k {
                egui::Key::ArrowDown | egui::Key::ArrowUp if n > 0 => {
                    if !self.shell.menu.as_ref().is_some_and(|m| m.tag == "search") {
                        self.update_search_menu(tb_out.search_rect);
                    } else {
                        let h = self.shell.search_hot.min(n - 1);
                        self.shell.search_hot = if k == egui::Key::ArrowDown { (h + 1) % n } else { (h + n - 1) % n };
                        self.update_search_menu(tb_out.search_rect);
                    }
                }
                egui::Key::Enter => {
                    if let Some(c) = ribbon::search_in(&self.shell.query, self.sketch.is_some()).get(self.shell.search_hot) {
                        let key = c.k.clone();
                        self.run_search_command(&ctx, &key, tb_out.search_rect.left_bottom());
                    }
                }
                egui::Key::Escape => {
                    self.shell.query.clear();
                    self.shell.menu = None;
                    ctx.memory_mut(|m| m.surrender_focus(Id::unique(topbar::SEARCH_ID)));
                }
                _ => {}
            }
        }

        // ---- ribbon
        let mut rb_pick = None;
        let rb_tab = self.ribbon_tab().to_owned();
        let rb_active: Option<String> = match &self.sketch {
            Some(m) => Some(m.s.tool.clone()),
            None => self.shell.active_tool.clone(),
        };
        egui::Panel::top("ribbon").exact_size(ribbon::height(&rb_tab)).frame(Frame::NONE).show_separator_line(false).show(ui, |ui| {
            let st = ribbon::RibbonState { tab: &rb_tab, active: rb_active.as_deref(), drop_last: &self.shell.drop_last, web: self.platform.is_web() };
            let (p, fit) = ribbon::show(ui, &tk, &st);
            rb_pick = p;
            self.last_fit = Some(fit);
        });
        if let Some(p) = rb_pick {
            self.dispatch_pick(&ctx, p);
        }

        // ---- workspace
        let mut start_pick = None;
        let mut commands: Vec<Command> = Vec::new();
        let mut events: Vec<UiEvent> = Vec::new();
        egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
            let avail = ui.available_size();
            if !self.start_view_done && avail.x >= 1.0 && avail.y >= 1.0 {
                // the size first; the opening flight to iso starts after `show` (below)
                self.start_view_done = true;
                self.viewport.set_view_size(avail.x, avail.y);
            }
            let vr = self.viewport.show(ui);
            if !self.sk.fly_started {
                // the first frame after the viewport shows: the opening fly-in (`start()` in 1.x)
                self.sk.fly_started = true;
                self.viewport.start_fly_in();
            }
            let ws = vr.response.rect;
            if vr.section_clicked {
                let on = self.viewport.section_enabled();
                self.viewport.set_section_enabled(!on);
            }
            // the handles, the Sculpt brush and Draw Walls' pointer first: a click on one of
            // them isn't a click on the part
            let mut consumed = self.tool_frame(ui, &vr.response);
            if let Some(g) = self.gizmo.as_mut() {
                consumed |= g.consume_click();
            }
            self.sketch_pointer(ui, &vr.response);
            self.sketch_frame(ui);
            consumed |= self.sketch.is_some();
            if vr.response.clicked() && !consumed {
                if let Some(pos) = vr.response.interact_pointer_pos() {
                    if !self.sf_click(pos) {
                        self.tool_click(pos);
                    }
                }
            }
            self.overlays(&ctx, &tk, ws, &mut commands, &mut events);
            let empty = self.timeline_view.items.is_empty() && self.shell.active_tool.is_none() && self.sketch.is_none();
            if empty {
                start_pick = start::show(&ctx, &tk, ws, &self.shell.recent);
            }
        });
        match start_pick {
            Some(StartPick::Open) => self.shell.file_requests.push(UiAction::Insert),
            Some(StartPick::Box) => self.push_ui_event(UiEvent::OpenTool("primitive_box".into())),
            Some(StartPick::Recent(p)) => self.shell.file_requests.push(UiAction::OpenRecent(p)),
            Some(StartPick::OpenUrl(u)) => ctx.open_url(egui::OpenUrl::new_tab(u)),
            None => {}
        }
        for c in commands {
            self.send(c);
        }
        for e in events {
            if let UiEvent::Select(ids) = &e {
                self.shell.selection = ids.clone();
                self.viewport.set_selection(ids);
            }
            if let UiEvent::Hover(h) = &e {
                self.viewport.set_hover(h.as_deref());
            }
            self.push_ui_event(e);
        }

        // ---- popups on top of everything
        if let Some(mut m) = self.shell.menu.take() {
            let anchor = m.pos;
            match menu::show(&ctx, &tk, &mut m) {
                MenuResult::None => self.shell.menu = Some(m),
                MenuResult::Close => {}
                MenuResult::Pick(tag, id) => self.on_menu_pick(&ctx, &tag, &id, anchor),
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.updates_frame(&ctx, &tk);
        let recovery = self.recover.as_mut().and_then(|r| r.show(&ctx, &tk, self.shell.dirty));
        match recovery {
            Some(RecoverAction::Open(r)) => self.open_recovered(r),
            Some(RecoverAction::Message { text, warn }) => self.toasts.push(if warn { Level::Warning } else { Level::Info }, text),
            None => {}
        }
        if let Some(url) = dialogs::show(&ctx, &tk, &mut self.shell.dialogs) {
            if url == wiring::RESTART_URL {
                self.restart_app(&ctx);
            } else {
                ctx.open_url(egui::OpenUrl::new_tab(url));
            }
        }
        self.run_requests(&ctx);
        if let Some((tag, res)) = self.shell.dialogs.take_result() {
            if tag == wiring::RESTART_TAG {
                if res == DialogResult::Yes {
                    self.restart_now(&ctx);
                }
            } else if tag == wiring::SK_OFFSET_TAG {
                if let DialogResult::Text(t) = res {
                    self.sketch_offset_answer(&t);
                }
            } else if tag == DISCARD_TAG {
                if let Some(what) = self.pending_discard.take().filter(|_| res == DialogResult::Yes) {
                    self.replace_document(what);
                }
            } else {
                // 4b callers match on the tag ("rename-body", ...) and take it; 4a has no prompt callers.
                self.dialog_result = Some((tag, res));
            }
        }
        if self.params_open {
            self.params_dialog(&ctx, &tk);
        }
        // TASK 8 call sites: toasts and the busy overlay
        toasts::show(&ctx, &tk, &mut self.toasts);
        let mut busy_cmds = Vec::new();
        busy::show(&ctx, &tk, &mut self.busy, &mut busy_cmds);
        #[cfg(not(target_arch = "wasm32"))]
        busy::show(&ctx, &tk, &mut self.update_busy, &mut Vec::new());
        for c in busy_cmds {
            self.send(c);
        }
        if self.engine_lost {
            self.engine_lost_dialog(&ctx, &tk);
        }
        if !self.first_frame_done {
            self.first_frame_done = true;
            // an update's old exe is deleted once the new one has drawn a frame
            #[cfg(not(target_arch = "wasm32"))]
            if self.updates.is_some() {
                crate::updates::finish_swap_after_first_frame();
            }
        }
    }

    /// The Parameters dialog: Task 8's table in a modal card.
    fn params_dialog(&mut self, ctx: &egui::Context, tk: &Tokens) {
        let mut cmds = Vec::new();
        let mut close = false;
        Area::new(Id::unique("params-scrim")).order(Order::Foreground).fixed_pos(ctx.content_rect().min).show(ctx, |ui| {
            let (r, _) = ui.allocate_exact_size(ctx.content_rect().size(), Sense::click_and_drag());
            ui.painter().rect_filled(r, 0.0, tk.scrim);
        });
        Area::new(Id::unique("params-card")).order(Order::Tooltip).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            Frame::new().fill(tk.panel).stroke(Stroke::new(1.0, tk.line)).corner_radius(CornerRadius::same(9)).shadow(tk.shadow_lg).inner_margin(Margin { left: 20, right: 20, top: 18, bottom: 14 }).show(ui, |ui| {
                ui.set_max_width(720.0);
                let used: std::collections::HashSet<String> =
                    self.param_views.iter().filter(|p| p.used).map(|p| p.name.clone()).collect();
                params::show(ui, tk, &mut self.params, &self.param_views, &used, &mut cmds);
                if !self.params.is_open() {
                    close = true; // Apply or Cancel closed the table
                }
            });
        });
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            close = true;
        }
        for c in cmds {
            self.send(c);
        }
        if close {
            self.params_open = false;
            self.params.set_open(false);
        }
    }

    /// "The engine stopped unexpectedly. Your last saved project is safe." with Restart (pending Nathan's OK).
    fn engine_lost_dialog(&mut self, ctx: &egui::Context, tk: &Tokens) {
        let mut restart = false;
        Area::new(Id::unique("engine-lost")).order(Order::Tooltip).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            Frame::new().fill(tk.panel).stroke(Stroke::new(1.0, tk.line)).corner_radius(CornerRadius::same(9)).shadow(tk.shadow_lg).inner_margin(Margin { left: 20, right: 20, top: 18, bottom: 14 }).show(ui, |ui| {
                ui.label(RichText::new("The engine stopped unexpectedly. Your last saved project is safe.").size(12.0).color(tk.text));
                ui.add_space(14.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if dialogs::button(ui, tk, "Restart", true).clicked() {
                        restart = true;
                    }
                });
            });
        });
        if restart {
            self.link.restart();
            self.engine_lost = false;
        }
    }

    /// The floating regions over the viewport: browser, tool panel, history bar, hint, HUD, drop overlay.
    fn overlays(&mut self, ctx: &egui::Context, tk: &Tokens, ws: Rect, commands: &mut Vec<Command>, events: &mut Vec<UiEvent>) {
        let panel_frame = || Frame::new().fill(tk.panel).stroke(Stroke::new(1.0, tk.line)).corner_radius(CornerRadius::same(tk.radius as u8)).shadow(tk.shadow);

        // browser: 240 wide, 8px from the top left, leaves room for the history bar
        let view = browser::BrowserView { doc_name: &self.shell.doc_name, bodies: &self.bodies, selection: &self.shell.selection, planes: &self.planes, sketches: &self.sketches, params: &self.param_views, origin_visible: self.viewport.base_planes_visible() };
        browser::show_panel(ctx, tk, ws, &mut self.browser, view, commands, events);
        // the Origin eye turns the three base planes on and off (app.js:2435)
        if let Some(v) = self.browser.take_origin_toggle() {
            self.viewport.set_base_planes_visible(v);
        }
        if self.browser.take_open_params() {
            self.params_open = true;
            self.params.set_open(true);
        }

        self.tool_panel(ctx, tk, ws);
        self.sketch_overlays(ctx, tk, ws);

        // history bar along the bottom
        let bar = Rect::from_min_max(Pos2::new(ws.min.x + 8.0, ws.max.y - 8.0 - 36.0), Pos2::new(ws.max.x - 8.0, ws.max.y - 8.0));
        Area::new(Id::unique("history-bar")).order(Order::Middle).fixed_pos(bar.min).show(ctx, |ui| {
            panel_frame().inner_margin(Margin::ZERO).show(ui, |ui| {
                ui.set_width(bar.width() - 2.0);
                ui.set_height(34.0);
                let status_w = 420.0f32.min(bar.width() * 0.4);
                let inner = ui.max_rect();
                let tl_rect = Rect::from_min_max(inner.min + Vec2::new(10.0, 4.0), Pos2::new(inner.max.x - status_w - 8.0, inner.max.y - 4.0));
                let st_rect = Rect::from_min_max(Pos2::new(inner.max.x - status_w - 6.0, inner.min.y), Pos2::new(inner.max.x - 8.0, inner.max.y));
                let mut tl_ui = ui.new_child(egui::UiBuilder::new().max_rect(tl_rect));
                // TASK 8 call site: history bar
                self.timeline.features_pick = self.tool.as_ref().is_some_and(|t| t.shown_fields().any(|f| matches!(f.kind, crate::tools::FieldKind::Features)));
                timeline::show(&mut tl_ui, ctx, tk, &mut self.timeline, &self.timeline_view, self.shell.active_tool.is_some(), self.sketch.is_some(), commands, events);
                // clicking a step puts its name (and any error) in the status line (app.js:2124)
                if let Some(m) = self.timeline.take_status() {
                    self.status_text = m;
                }
                if let Some(id) = self.timeline.take_ticked() {
                    if let Some(t) = self.tool.as_mut() {
                        let key = t.shown_fields().find(|f| matches!(f.kind, crate::tools::FieldKind::Features)).map(|f| f.key);
                        if let Some(key) = key {
                            crate::tools::fields::features::toggle(&mut t.values, key, &id);
                            t.schedule_preview(crate::tools::preview::DELAY_MS);
                        }
                    }
                }
                self.timeline.take_edit_feature(); // the tool opens through the OpenTool event (4b)
                let mut st_ui = ui.new_child(egui::UiBuilder::new().max_rect(st_rect));
                // sketch mode: pointer position and definition (sketcher.js showStatus), repainted on change
                let inch = self.shell.inch;
                if let Some(m) = self.sketch.as_mut() {
                    let t = m.status_line(inch);
                    if t != m.last_status {
                        m.last_status = t.clone();
                        self.status_text = t;
                    }
                }
                // TASK 8 call site: status line
                status::show(&mut st_ui, tk, &self.status_text);
            });
        });

        // hint line
        if !self.shell.hint.is_empty() {
            let pos = Pos2::new(ws.center().x, ws.max.y - 54.0);
            Area::new(Id::unique("hint")).order(Order::Middle).interactable(false).pivot(Align2::CENTER_BOTTOM).fixed_pos(pos).show(ctx, |ui| {
                Frame::new().fill(tk.float).stroke(Stroke::new(1.0, tk.line)).corner_radius(CornerRadius::same(14)).shadow(tk.shadow).inner_margin(Margin::symmetric(12, 5)).show(ui, |ui| {
                    // 1.x: `left: 50%` makes the shrink-to-fit width at most half the viewport,
                    // padding and border included (that binds before `max-width: 60%`).
                    ui.set_max_width((ws.width() * 0.5 - 26.0).max(40.0));
                    ui.label(RichText::new(self.shell.hint.as_str()).size(12.0).color(tk.strong));
                });
            });
            // 1.x stacks the tool panel (z-index 5) above the hint (z-index 3): a long hint runs
            // under the panel instead of over its footer
            if self.tool.is_some() {
                ctx.move_to_top(egui::LayerId::new(Order::Middle, Id::unique("tool-panel")));
            }
        }
        // HUD (1.x leaves it empty; the sketch HUD is 4c)
        if let Some(h) = &self.shell.hud {
            Area::new(Id::unique("hud")).order(Order::Middle).interactable(false).fixed_pos(ws.min + Vec2::new(256.0, 8.0)).show(ctx, |ui| {
                Frame::new().fill(tk.float).stroke(Stroke::new(1.0, tk.line)).corner_radius(CornerRadius::same(tk.radius as u8)).shadow(tk.shadow).inner_margin(Margin::symmetric(10, 6)).show(ui, |ui| {
                    ui.set_min_width(160.0);
                    ui.label(RichText::new(h.as_str()).size(12.0).color(tk.text));
                });
            });
        }
        // drop overlay while a file is dragged over the window
        if self.shell.dragging_files {
            Area::new(Id::unique("drop-overlay")).order(Order::Foreground).interactable(false).fixed_pos(ws.min).show(ctx, |ui| {
                let (r, _) = ui.allocate_exact_size(ws.size(), Sense::hover());
                ui.painter().rect_filled(r, 0.0, tk.accent_soft);
                let dash = Stroke::new(3.0, tk.accent);
                let rr = r.shrink(1.5);
                for (a, b) in [(rr.left_top(), rr.right_top()), (rr.right_top(), rr.right_bottom()), (rr.right_bottom(), rr.left_bottom()), (rr.left_bottom(), rr.left_top())] {
                    ui.painter().add(egui::Shape::dashed_line(&[a, b], dash, 9.0, 6.0));
                }
                ui.painter().text(r.center(), Align2::CENTER_CENTER, "Drop to insert meshes", egui::FontId::new(18.0, egui::FontFamily::Name("bold".into())), tk.accent);
            });
        }
    }
}

impl eframe::App for StlApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.layout(ui);
    }

    /// The window is closing: write the newest autosave copy, so that unsaved work is kept for
    /// Recover Unsaved Work (the engine link removes the folder only when nothing is unsaved).
    fn on_exit(&mut self) {
        if self.recover.is_some() && self.shell.dirty {
            self.link.send(self.autosave_command());
        }
    }
}
