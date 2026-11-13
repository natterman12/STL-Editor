//! One file per region of the window.

pub mod browser;
pub mod busy;
pub mod dialogs;
pub mod files;
pub mod keys;
pub mod menu;
pub mod params;
pub mod ribbon;
pub mod start;
pub mod status;
pub mod timeline;
pub mod toasts;
pub mod topbar;

/// What a region tells the rest of the app.
#[derive(Clone, Debug, PartialEq)]
pub enum UiEvent {
    OpenTool(String),
    Select(Vec<String>),
    Hover(Option<String>),
}

/// Which theme the user asked for (View > Theme). `System` follows the operating system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemePref {
    Light,
    Dark,
    System,
}

/// What a menu or button asks the app to do. The app handles view, theme, undo and redo itself;
/// the file ones wait in `Shell::file_requests` for `ui/files.rs` (Task 9).
#[derive(Clone, Debug, PartialEq)]
pub enum UiAction {
    New,
    Open,
    OpenRecent(String),
    Insert,
    Save,
    SaveAs,
    /// `"stl"`, `"3mf"`, `"obj"` or `"step"`.
    Export(String),
    Quit,
    Recover,
    ShowShortcuts,
    EditParams,
    SelectAllBodies,
    RemoveSelected,
}

/// The state of the shell regions (top bar, ribbon, start page, menus, dialogs, hint, HUD).
pub struct Shell {
    /// The ribbon tab: `"solid"`, `"print"`, `"supports"` or `"mesh"`.
    pub tab: String,
    pub drop_last: std::collections::HashMap<String, String>,
    /// A tool shown as pressed in the ribbon (set by the tool panel in 4b).
    pub active_tool: Option<String>,
    pub menu: Option<menu::OpenMenu>,
    pub query: String,
    pub search_hot: usize,
    pub focus_search: bool,
    pub dialogs: dialogs::Dialogs,
    /// The hint line (`#hint`); empty hides it.
    pub hint: String,
    /// The HUD (`#hud`); 1.x never fills it, so it stays hidden.
    pub hud: Option<String>,
    pub doc_name: String,
    pub dirty: bool,
    /// Recent files, newest first (files.rs fills it).
    pub recent: Vec<String>,
    pub selection: Vec<String>,
    pub theme_pref: ThemePref,
    pub inch: bool,
    pub show_edges: bool,
    pub show_grid: bool,
    /// Events for the tool panel (4b) and the panels; `OpenTool` does nothing in 4a. Keeps the last 64.
    pub events: Vec<UiEvent>,
    /// File actions waiting for Task 9's `files.rs`.
    pub file_requests: Vec<UiAction>,
    /// A file is being dragged over the window.
    pub dragging_files: bool,
}

impl Default for Shell {
    fn default() -> Self {
        Shell {
            tab: "solid".into(),
            drop_last: Default::default(),
            active_tool: None,
            menu: None,
            query: String::new(),
            search_hot: 0,
            focus_search: false,
            dialogs: Default::default(),
            hint: String::new(),
            hud: None,
            doc_name: "Untitled".into(),
            dirty: false,
            recent: Vec::new(),
            selection: Vec::new(),
            theme_pref: ThemePref::Dark,
            inch: false,
            show_edges: true,
            show_grid: true,
            events: Vec::new(),
            file_requests: Vec::new(),
            dragging_files: false,
        }
    }
}

impl Shell {
    pub fn push_event(&mut self, e: UiEvent) {
        self.events.push(e);
        if self.events.len() > 64 {
            self.events.remove(0);
        }
    }
}
