//! The application: owns the theme, the platform, the engine link and the viewport, and lays the
//! window out like 1.x's web/index.html: top bar, ribbon, then the workspace where the viewport
//! fills everything and the browser, tool panel and timeline bar float over it.

use crate::engine_link::{Command, EngineLink, Level, NullLink, ParamView, PlaneView, SketchView, TimelineView, Update};
use crate::engine_link::BodyView;
use crate::platform::{NullPlatform, Platform};
use crate::theme::{self, Tokens};
use crate::ui::dialogs::{self, DialogResult};
use crate::ui::menu::{self, MenuItem, MenuResult, OpenMenu};
use crate::ui::start::{self, StartPick, KOFI};
use crate::ui::files::{FileAction, FileOutcome, Files};
use crate::ui::keys::{self, KeyAction, KeyState};
use crate::ui::{browser, busy, params, ribbon, status, timeline, toasts, topbar};
use crate::ui::{Shell, ThemePref, UiAction, UiEvent};
use crate::viewport::Viewport;
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
    /// A file from the command line, until its bodies arrive. `true` is an insert (fit the view);
    /// `false` is a project (isometric view).
    launch_file: Option<bool>,
    /// 1.x `app.js` turns the view to isometric once at startup (`viewer.setView('iso')`); done
    /// on the first frame the workspace has a size, since the distance depends on the view's shape.
    start_view_done: bool,
}

/// A short fingerprint of the history, to tell whether it changed since the last save.
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
    let edition = if web {
        format!("Web edition {version}
CAD-style editing for STL, 3MF and OBJ meshes, running entirely in your browser: your files never leave your computer.")
    } else {
        format!("Version {version}
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
            files: Files::new(),
            dialog_result: None,
            last_fit: None,
            clean_sig: history_sig(&TimelineView::default()),
            pending_discard: None,
            launch_file: None,
            start_view_done: false,
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
        let mark_clean = out.saved || out.view_iso;
        self.apply_file_outcome(out);
        for u in updates {
            match u {
                Update::Bodies(b) => {
                    self.viewport.sync_bodies(&b);
                    self.bodies = b;
                }
                Update::Timeline(t) => self.timeline_view = t,
                Update::Params(p) => self.param_views = p,
                Update::Planes(p) => {
                    self.viewport.sync_planes(&p);
                    self.planes = p;
                }
                Update::Sketches(s) => {
                    self.viewport.sync_sketches(&s, None);
                    self.sketches = s;
                }
                Update::Message { level, text } => {
                    self.status_text = text.clone();
                    self.toasts.push(level, text);
                }
                Update::Progress { done, total, label } => self.busy.set_progress(done, total, &label),
                Update::Busy(on) => self.busy.set_busy(on),
                Update::Error(e) => self.toasts.push(Level::Error, e),
                Update::EngineLost => self.engine_lost = true,
                // TASK 9 (files.rs): Saved { bytes } and Exported { name, bytes, count } go to the
                // platform's save dialog.
                Update::Saved { .. } | Update::Exported { .. } => {}
            }
        }
        if mark_clean {
            self.clean_sig = history_sig(&self.timeline_view);
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
                    self.shell.push_event(UiEvent::OpenTool("delete".into()));
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
                // Recover Unsaved Work belongs to milestone 5 (autosave)
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
                if !self.files.open_recent(&*self.link, &name) {
                    self.toasts.push(Level::Error, format!("{name} is no longer in this session's recent files."));
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
            sketcher_active: false,
            walls_active: false,
            walls_pending: false,
            tool_open: self.shell.active_tool.is_some(),
            // `tool.def.gizmo === 'move'`: only the Move tool has the gizmo
            tool_gizmo_move: self.shell.active_tool.as_deref() == Some("move"),
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
                self.shell.push_event(e);
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
                    KeyAction::BeginSketch => self.shell.push_event(UiEvent::OpenTool("sketch".into())),
                    // sketcher, walls, gizmo, panel keys: 4b and 4c
                    _ => {}
                }
            }
        }
    }

    // ------------------------------------------------------------------ actions

    fn send(&self, c: Command) {
        self.link.send(c);
    }

    /// A ribbon / search / dropdown tool was chosen (`App.onRibbon`).
    fn on_ribbon(&mut self, key: &str, anchor: Pos2) {
        match key {
            "import" => self.shell.file_requests.push(UiAction::Insert),
            "export" => self.open_export_menu(anchor),
            "params" => { self.params_open = true; self.params.set_open(true); }
            // Send to Slicer is desktop-only and not part of 4a
            "send" => {}
            k => self.shell.push_event(UiEvent::OpenTool(k.to_owned())),
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
            "units.mm" | "units.in" => {
                self.shell.inch = id == "units.in";
                // TASK 8 toasts: 1.x says "Showing inches. Models are still saved in millimetres."
                let text = if self.shell.inch { "Showing inches. Models are still saved in millimetres." } else { "Showing millimetres." };
                self.toasts.push(Level::Info, text.to_owned());
            }
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
            "help.kofi" => ctx.open_url(egui::OpenUrl::new_tab(KOFI)),
            "help.about" => {
                let web = self.platform.is_web();
                let mut links = Vec::new();
                if web {
                    links.push(("source on GitHub".to_owned(), "https://github.com/natterman12/STL-Editor".to_owned()));
                }
                links.push(("Buy me a coffee on Ko-fi".to_owned(), KOFI.to_owned()));
                self.shell.dialogs.open_alert("about", "STL Editor", &about_text(web), links);
            }
            _ => {}
        }
    }

    /// Choosing a tool-search result: switch to its tab when the current one doesn't have it.
    fn run_search_command(&mut self, ctx: &egui::Context, key: &str, anchor: Pos2) {
        self.shell.query.clear();
        self.shell.menu = None;
        ctx.memory_mut(|m| m.surrender_focus(Id::unique(topbar::SEARCH_ID)));
        if let Some(c) = ribbon::all_commands().into_iter().find(|c| c.k == key) {
            if let Some(tab) = c.tab {
                let here = ribbon::def(&self.shell.tab).groups.iter().any(|g| g.tools().iter().any(|x| x.key == key));
                if tab != self.shell.tab && !here {
                    self.shell.tab = tab.to_owned();
                }
            }
        }
        self.on_ribbon(key, anchor);
    }

    fn update_search_menu(&mut self, rect: Rect) {
        let results = ribbon::search(&self.shell.query);
        if self.shell.query.trim().is_empty() {
            if self.shell.menu.as_ref().is_some_and(|m| m.tag == "search") {
                self.shell.menu = None;
            }
            return;
        }
        let tab_name = |t: &str| ribbon::TABS.iter().find(|x| x.0 == t).map(|x| x.1.to_owned());
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
            Pick::Tab(k) => self.shell.tab = k.to_owned(),
            Pick::Tool(k) => self.on_ribbon(k, Pos2::new(200.0, 140.0)),
            Pick::Drop(id, r) => {
                let d = ribbon::def(&self.shell.tab);
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
                    for (label, tools) in ribbon::folded_groups(&self.shell.tab, f) {
                        rows.push(MenuItem::Hdr(label.to_owned()));
                        rows.extend(tools.iter().map(|x| MenuItem::item(format!("tool:{}", x.key), x.label).icon(x.icon)));
                    }
                }
                self.shell.menu = Some(OpenMenu::new("more", rows, r.left_bottom() + Vec2::new(0.0, 2.0)).keep(r));
            }
            Pick::GroupMenu(gi, r) => {
                let d = ribbon::def(&self.shell.tab);
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
        let drop = self.files.handle_drop(&ctx, &*self.link);
        self.shell.dragging_files = self.files.dragging;
        self.apply_file_outcome(drop);
        self.shell.doc_name = self.files.title.clone().unwrap_or_else(|| "Untitled".to_owned());
        self.shell.recent = self.files.recent_names();
        self.run_keys(&ctx);
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
            let n = ribbon::search(&self.shell.query).len();
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
                    if let Some(c) = ribbon::search(&self.shell.query).get(self.shell.search_hot) {
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
        egui::Panel::top("ribbon").exact_size(ribbon::height(&self.shell.tab)).frame(Frame::NONE).show_separator_line(false).show(ui, |ui| {
            let st = ribbon::RibbonState { tab: &self.shell.tab, active: self.shell.active_tool.as_deref(), drop_last: &self.shell.drop_last, web: self.platform.is_web() };
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
                // 1.x flies there during load; 2.0 starts in place, so the first picture is it
                self.start_view_done = true;
                self.viewport.set_view_size(avail.x, avail.y);
                self.viewport.set_view("iso");
                self.viewport.camera_mut().finish_flight();
            }
            let vr = self.viewport.show(ui);
            let ws = vr.response.rect;
            if vr.section_clicked {
                let on = self.viewport.section_enabled();
                self.viewport.set_section_enabled(!on);
            }
            self.overlays(&ctx, &tk, ws, &mut commands, &mut events);
            let empty = self.timeline_view.items.is_empty() && self.shell.active_tool.is_none();
            if empty {
                start_pick = start::show(&ctx, &tk, ws, &self.shell.recent);
            }
        });
        match start_pick {
            Some(StartPick::Open) => self.shell.file_requests.push(UiAction::Insert),
            Some(StartPick::Box) => self.shell.push_event(UiEvent::OpenTool("primitive_box".into())),
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
            self.shell.push_event(e);
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
        if let Some(url) = dialogs::show(&ctx, &tk, &mut self.shell.dialogs) {
            ctx.open_url(egui::OpenUrl::new_tab(url));
        }
        self.run_requests(&ctx);
        if let Some((tag, res)) = self.shell.dialogs.take_result() {
            if tag == DISCARD_TAG {
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
        for c in busy_cmds {
            self.send(c);
        }
        if self.engine_lost {
            self.engine_lost_dialog(&ctx, &tk);
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
        Area::new(Id::unique("browser-panel")).order(Order::Middle).fixed_pos(ws.min + Vec2::new(8.0, 8.0)).show(ctx, |ui| {
            panel_frame().inner_margin(Margin::symmetric(9, 6)).show(ui, |ui| {
                // 238 wide in all (220 + the 9 px margins and the 1 px borders). Not inside a
                // `ui.horizontal`: that gives its content one row of height and clips the tree.
                ui.set_width(220.0);
                ui.set_max_height((ws.height() - 60.0).max(40.0));
                // TASK 8 call site: browser tree
                let view = browser::BrowserView { doc_name: &self.shell.doc_name, bodies: &self.bodies, selection: &self.shell.selection, planes: &self.planes, sketches: &self.sketches, params: &self.param_views, origin_visible: self.viewport.base_planes_visible() };
                browser::show(ui, ctx, tk, &mut self.browser, view, commands, events);
                // the Origin eye turns the three base planes on and off (app.js:2435)
                if let Some(v) = self.browser.take_origin_toggle() {
                    self.viewport.set_base_planes_visible(v);
                }
                if self.browser.take_open_params() {
                    self.params_open = true;
                    self.params.set_open(true);
                }
            });
        });

        // TASK 4b call site: the floating tool panel (right, 290 wide) goes here; the view cube moves
        // left with `viewport.set_viewcube_inset(298.0)` while it is open.

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
                timeline::show(&mut tl_ui, ctx, tk, &mut self.timeline, &self.timeline_view, self.shell.active_tool.is_some(), false, commands, events);
                // clicking a step puts its name (and any error) in the status line (app.js:2124)
                if let Some(m) = self.timeline.take_status() {
                    self.status_text = m;
                }
                self.timeline.take_edit_feature(); // the tool opens through the OpenTool event (4b)
                let mut st_ui = ui.new_child(egui::UiBuilder::new().max_rect(st_rect));
                // TASK 8 call site: status line
                status::show(&mut st_ui, tk, &self.status_text);
            });
        });

        // hint line
        if !self.shell.hint.is_empty() {
            let pos = Pos2::new(ws.center().x, ws.max.y - 54.0);
            Area::new(Id::unique("hint")).order(Order::Middle).interactable(false).pivot(Align2::CENTER_BOTTOM).fixed_pos(pos).show(ctx, |ui| {
                Frame::new().fill(tk.float).stroke(Stroke::new(1.0, tk.line)).corner_radius(CornerRadius::same(14)).shadow(tk.shadow).inner_margin(Margin::symmetric(12, 5)).show(ui, |ui| {
                    ui.set_max_width(ws.width() * 0.6);
                    ui.label(RichText::new(self.shell.hint.as_str()).size(12.0).color(tk.strong));
                });
            });
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
}
