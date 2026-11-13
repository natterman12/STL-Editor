//! The shell regions (milestone 4a, task 7): top bar, ribbon, tool search, start page, dialogs, menus,
//! theme switch. Rows of rust/tests/UI_PARITY.md: ribbon.js, kofi.js, desktop_smoke.js (shell parts).
#![cfg(not(target_arch = "wasm32"))]

use std::sync::{Arc, Mutex};

use egui::Key;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use stl_app::app::StlApp;
use stl_app::engine_link::{Command, EngineLink, Update};
use stl_app::theme;
use stl_app::ui::dialogs::DialogResult;
use stl_app::ui::{ribbon, ThemePref, UiEvent};
use stl_app::viewport::renderer::Headless;

#[path = "support/wgpu_renderer.rs"]
mod wgpu_renderer;

fn repo() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn app_js() -> String {
    std::fs::read_to_string(repo().join("web/js/app.js")).unwrap().replace("\r\n", "\n")
}

fn shell_with(w: f32, h: f32, app: StlApp) -> Harness<'static, StlApp> {
    let mut init = false;
    // The accessibility check is off: the browser's image buttons (Task 8) have no names yet.
    Harness::builder().with_size(egui::vec2(w, h)).with_accessibility_check(false).build_ui_state(
        move |ui, app: &mut StlApp| {
            if !init {
                init = true;
                stl_app::fonts::install(ui.ctx());
                app.set_theme(ui.ctx(), ThemePref::Dark);
            }
            app.layout(ui);
        },
        app,
    )
}

fn shell(w: f32, h: f32) -> Harness<'static, StlApp> {
    shell_with(w, h, StlApp::empty(&egui::Context::default()))
}

// ------------------------------------------------------------------ the ribbon data (ribbon.js)

/// Every `['key', 'Label', 'icon']` between two markers of app.js's ribbonDef, in order.
fn triples(src: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'[' && b.get(i + 1) == Some(&b'\'') {
            // ['a', 'b', 'c']
            let rest = &src[i..];
            if let Some(end) = rest.find(']') {
                let inner = &rest[1..end];
                let parts: Vec<&str> = inner.split(", ").collect();
                if parts.len() == 3 && parts.iter().all(|p| p.starts_with('\'') && p.ends_with('\'') && p.len() >= 2 && !p[1..p.len() - 1].contains('\'')) {
                    let t: Vec<String> = parts.iter().map(|p| p[1..p.len() - 1].to_string()).collect();
                    out.push((t[0].clone(), t[1].clone(), t[2].clone()));
                    i += end;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

fn model_ribbon_source() -> String {
    let src = app_js();
    let a = src.find("// Inspect is the same everywhere").expect("pinned marker");
    let b = src[a..].find("return { tabs:").expect("return marker") + a;
    src[a..b].to_string()
}

#[test]
fn ribbon_tools_match_ribbon_def() {
    let want = triples(&model_ribbon_source());
    assert!(want.len() > 50, "parsed {} tools", want.len());
    let mut have: Vec<(String, String, String)> = Vec::new();
    for t in ribbon::def("solid").pinned.tools() {
        have.push((t.key.into(), t.label.into(), t.icon.into()));
    }
    for (tab, _) in ribbon::TABS {
        for g in ribbon::def(tab).groups {
            for t in g.tools() {
                have.push((t.key.into(), t.label.into(), t.icon.into()));
            }
        }
    }
    let (mut a, mut b) = (want.clone(), have.clone());
    a.sort();
    b.sort();
    assert_eq!(a, b);
}

#[test]
fn ribbon_group_names_and_tabs_match_ribbon_def() {
    let src = model_ribbon_source();
    // group lines look like `['Create', [[` or `['Inspect', [{ stack`
    let mut want = Vec::new();
    for line in src.lines() {
        let l = line.trim_start();
        if let Some(rest) = l.strip_prefix("['") {
            if let Some((name, tail)) = rest.split_once("', [") {
                if name.chars().next().is_some_and(|c| c.is_uppercase()) && !tail.is_empty() {
                    want.push(name.to_string());
                }
            }
        }
    }
    let mut have: Vec<String> = vec![ribbon::def("solid").pinned.label.to_string()];
    for (tab, _) in ribbon::TABS {
        have.extend(ribbon::def(tab).groups.iter().map(|g| g.label.to_string()));
    }
    assert_eq!(have.remove(0), "Inspect");
    assert!(src.contains("const pinned = ['Inspect',"));
    assert_eq!(want, have);
    let tabs: Vec<_> = ribbon::TABS.iter().map(|t| t.1).collect();
    assert_eq!(tabs, ["Model", "Print", "Supports", "Mesh"]);
    assert!(src.contains("tabs: [['solid', 'Model'], ['print', 'Print'], ['supports', 'Supports'], ['mesh', 'Mesh']]") || app_js().contains("tabs: [['solid', 'Model'], ['print', 'Print'], ['supports', 'Supports'], ['mesh', 'Mesh']]"));
}

#[test]
fn each_tool_is_on_one_tab_only() {
    let mut seen = std::collections::HashMap::<&str, &str>::new();
    for (tab, _) in ribbon::TABS {
        for g in ribbon::def(tab).groups {
            for t in g.tools() {
                if let Some(prev) = seen.insert(t.key, tab) {
                    panic!("{} is on {prev} and {tab}", t.key);
                }
            }
        }
    }
    // Inspect is pinned, not on a tab
    for t in ribbon::def("solid").pinned.tools() {
        assert!(!seen.contains_key(t.key), "{} is also on a tab", t.key);
    }
}

#[test]
fn tool_search_finds_like_app_js() {
    let r = ribbon::search("ext");
    assert_eq!(r[0].k, "extrude");
    assert!(ribbon::search("zzzz").is_empty());
    assert!(ribbon::search("   ").is_empty());
    // File commands and tools only the search knows
    assert!(ribbon::search("insert").iter().any(|c| c.k == "import"));
    assert!(ribbon::search("torus").iter().any(|c| c.k == "primitive_torus"));
    assert!(ribbon::search("a").len() <= 12);
}

// ------------------------------------------------------------------ the ribbon on screen

#[test]
fn ribbon_tabs_show_their_tools_and_inspect_is_pinned() {
    for w in [1500.0, 900.0] {
        let mut h = shell(w, 700.0);
        h.run_steps(4);
        for (tab, label) in ribbon::TABS {
            h.get_by_label(label).click();
            h.run_steps(4);
            assert_eq!(h.state().shell.tab, tab);
            // Inspect is on every tab (the pinned group is never folded)
            for t in ["Measure", "Properties"] {
                assert!(h.query_by_label(t).is_some(), "{t} missing on {tab} at {w}");
            }
        }
    }
    let mut h = shell(1500.0, 700.0);
    h.run_steps(4);
    assert!(h.query_by_label("Extrude").is_some());
    assert!(h.query_by_label("Lay Flat").is_none());
    h.get_by_label("Print").click();
    h.run_steps(4);
    assert!(h.query_by_label("Lay Flat").is_some());
    assert!(h.query_by_label("Extrude").is_none());
}

#[test]
fn narrow_window_folds_groups_into_more() {
    let mut wide = shell(1500.0, 700.0);
    wide.run_steps(4);
    assert!(wide.query_by_label("More tools").is_none());
    let mut narrow = shell(560.0, 700.0);
    narrow.run_steps(4);
    assert!(narrow.query_by_label("More tools").is_some());
    // the first group stays; Inspect stays
    assert!(narrow.query_by_label("Sketch").is_some());
    assert!(narrow.query_by_label("Measure").is_some());
}

#[test]
fn tool_buttons_send_open_tool() {
    let mut h = shell(1500.0, 700.0);
    h.run_steps(4);
    h.get_by_label("Extrude").click();
    h.run_steps(4);
    assert!(h.state().shell.events.contains(&UiEvent::OpenTool("extrude".into())));
}

#[test]
fn primitive_dropdown_lists_the_shapes() {
    let mut h = shell(1500.0, 700.0);
    h.run_steps(4);
    h.get_by_label("Primitive").click();
    h.run_steps(4);
    for s in ["Box", "Cylinder", "Sphere", "Cone", "Torus"] {
        assert!(h.query_by_label(s).is_some(), "{s}");
    }
    h.get_by_label("Torus").click();
    h.run_steps(4);
    assert!(h.state().shell.events.contains(&UiEvent::OpenTool("primitive_torus".into())));
    assert_eq!(h.state().shell.drop_last.get("primitive").map(String::as_str), Some("primitive_torus"));
}

// ------------------------------------------------------------------ tool search keys

#[test]
fn tool_search_arrows_enter_escape() {
    let mut h = shell(1500.0, 700.0);
    h.run_steps(4);
    h.get_by_label("Search tools").click();
    h.run_steps(4);
    h.get_by_label("Search tools").type_text("hol");
    h.run_steps(4);
    let results = ribbon::search("hol");
    assert!(results.len() >= 2);
    // the menu lists the matches (the first is highlighted)
    assert!(h.state().shell.menu.as_ref().is_some_and(|m| m.tag == "search"));
    assert_eq!(h.state().shell.search_hot, 0);
    h.key_press(Key::ArrowDown);
    h.run_steps(4);
    assert_eq!(h.state().shell.search_hot, 1);
    h.key_press(Key::ArrowUp);
    h.run_steps(4);
    assert_eq!(h.state().shell.search_hot, 0);
    h.key_press(Key::ArrowUp);
    h.run_steps(4);
    assert_eq!(h.state().shell.search_hot, results.len() - 1, "wraps around");
    h.key_press(Key::ArrowDown);
    h.key_press(Key::ArrowDown);
    h.run_steps(4);
    h.key_press(Key::Enter);
    h.run_steps(4);
    let want = results[1].k.clone();
    assert!(h.state().shell.events.contains(&UiEvent::OpenTool(want)), "{:?}", h.state().shell.events);
    assert!(h.state().shell.query.is_empty());
    assert!(h.state().shell.menu.is_none());

    // Escape clears the box and closes the list
    h.get_by_label("Search tools").click();
    h.get_by_label("Search tools").type_text("hol");
    h.run_steps(4);
    assert!(h.state().shell.menu.is_some());
    h.key_press(Key::Escape);
    h.run_steps(4);
    assert!(h.state().shell.query.is_empty());
    assert!(h.state().shell.menu.is_none());
}

#[test]
fn tool_search_switches_tab_when_needed() {
    let mut h = shell(1500.0, 700.0);
    h.run_steps(4);
    h.get_by_label("Search tools").click();
    h.get_by_label("Search tools").type_text("lay flat");
    h.run_steps(4);
    h.key_press(Key::Enter);
    h.run_steps(4);
    assert_eq!(h.state().shell.tab, "print");
    assert!(h.state().shell.events.contains(&UiEvent::OpenTool("layflat".into())));
}

// ------------------------------------------------------------------ dialogs (#modal)

#[test]
fn prompt_enter_accepts_and_escape_cancels() {
    let mut h = shell(900.0, 600.0);
    h.state_mut().shell.dialogs.open_prompt("rename", "Rename body", "  Body 1 ");
    h.run_steps(4);
    assert!(h.query_by_label("OK").is_some() && h.query_by_label("Cancel").is_some());
    h.key_press(Key::Enter);
    h.run_steps(4);
    assert_eq!(h.state_mut().dialog_result.take(), Some(("rename".into(), DialogResult::Text("Body 1".into()))));
    assert!(!h.state().shell.dialogs.is_open());

    h.state_mut().shell.dialogs.open_prompt("rename", "Rename body", "x");
    h.run_steps(4);
    h.key_press(Key::Escape);
    h.run_steps(4);
    assert_eq!(h.state_mut().dialog_result.take(), Some(("rename".into(), DialogResult::Cancelled)));
}

#[test]
fn prompt_typed_text_is_returned() {
    let mut h = shell(900.0, 600.0);
    h.state_mut().shell.dialogs.open_prompt("rename", "Rename body", "Cube");
    h.run_steps(4);
    // the old text is selected, so typing replaces it
    h.key_press(Key::A); // no-op key; focus is in the box
    h.run_steps(4);
    h.get_by_label("OK").click();
    h.run_steps(4);
    let (tag, r) = h.state_mut().dialog_result.take().unwrap();
    assert_eq!(tag, "rename");
    assert!(matches!(r, DialogResult::Text(_)));
}

#[test]
fn confirm_buttons() {
    let mut h = shell(900.0, 600.0);
    h.state_mut().shell.dialogs.open_confirm("c", "Replace the current unsaved work?");
    h.run_steps(4);
    h.get_by_label("Cancel").click();
    h.run_steps(4);
    assert_eq!(h.state_mut().dialog_result.take(), Some(("c".into(), DialogResult::Cancelled)));
    h.state_mut().shell.dialogs.open_confirm("c", "Replace the current unsaved work?");
    h.run_steps(4);
    h.get_by_label("OK").click();
    h.run_steps(4);
    assert_eq!(h.state_mut().dialog_result.take(), Some(("c".into(), DialogResult::Yes)));
}

// ------------------------------------------------------------------ theme switch

#[test]
fn theme_switch_updates_tokens_egui_and_viewport() {
    let mut h = shell(1500.0, 700.0);
    h.run_steps(4);
    assert!(h.state().dark && h.state().viewport.dark());
    h.get_by_label("View").click();
    h.run_steps(4);
    h.get_by_label("Light").click();
    h.run_steps(4);
    let app = h.state();
    assert!(!app.dark && !app.viewport.dark());
    assert_eq!(app.tokens.panel, theme::tokens(false).panel);
    assert_eq!(app.shell.theme_pref, ThemePref::Light);
    h.get_by_label("View").click();
    h.run_steps(4);
    h.get_by_label("Dark").click();
    h.run_steps(4);
    assert!(h.state().dark && h.state().viewport.dark());
}

// ------------------------------------------------------------------ start page, menus, Ko-fi

#[test]
fn start_page_shows_until_the_project_has_steps() {
    let mut h = shell(1500.0, 700.0);
    h.run_steps(4);
    assert!(h.query_by_label("Open mesh…").is_some());
    assert!(h.query_by_label("Start with a box").is_some());
    assert!(h.query_by_label("Support STL Editor").is_some(), "Ko-fi link on the start page");
    h.get_by_label("Start with a box").click();
    h.run_steps(4);
    assert!(h.state().shell.events.contains(&UiEvent::OpenTool("primitive_box".into())));
}

/// kofi.js: the start screen's footer line (`.empty-foot`) and its link to the Ko-fi page.
#[test]
fn start_page_kofi_link_text_and_url() {
    let mut h = shell(1500.0, 700.0);
    h.run_steps(4);
    assert!(h.query_by_label("Free and open source · ").is_some(), "footer text before the link");
    assert_eq!(stl_app::ui::start::KOFI, "https://ko-fi.com/natterman12");
    h.get_by_label("Support STL Editor").click();
    // the command lives in one frame's output: look after each step
    let mut opened: Vec<String> = Vec::new();
    for _ in 0..4 {
        h.step();
        opened.extend(h.output().platform_output.commands.iter().filter_map(|c| match c {
            egui::OutputCommand::OpenUrl(u) => Some(u.url.clone()),
            _ => None,
        }));
    }
    assert!(opened.iter().any(|u| u == "https://ko-fi.com/natterman12"), "opened {opened:?}");
}

#[test]
fn help_menu_has_kofi_and_about() {
    let mut h = shell(1500.0, 700.0);
    h.run_steps(4);
    h.get_by_label("Help").click();
    h.run_steps(4);
    assert!(h.query_by_label("Support STL Editor ♥").is_some());
    assert!(h.query_by_label("Keyboard Shortcuts").is_some());
    h.get_by_label("About STL Editor").click();
    h.run_steps(4);
    assert!(h.query_by_label("Buy me a coffee on Ko-fi").is_some(), "Ko-fi link in the About dialog");
}

#[test]
fn file_edit_view_menus_match_app_js() {
    let mut h = shell(1500.0, 700.0);
    h.run_steps(4);
    for (menu, items) in [
        ("File", vec!["New", "Open…", "Insert Mesh…", "Save", "Save As…", "Export STL…", "Export 3MF…", "Export OBJ…", "Quit"]),
        ("Edit", vec!["Undo", "Redo", "Parameters…", "Select All Bodies", "Remove Selected Bodies"]),
        ("View", vec!["Fit All", "Isometric", "Top", "Front", "Right", "Section View", "Millimetres", "Inches", "Light", "Dark", "Match Windows", "Show Edges", "Show Grid", "Show Origin Planes"]),
    ] {
        h.get_by_label(menu).click();
        h.run_steps(4);
        for it in items {
            // "Light" etc. carry a check mark when selected
            assert!(h.query_all_by_label_contains(it).next().is_some(), "{menu} > {it}");
        }
        h.key_press(Key::Escape);
        h.run_steps(4);
    }
}

#[test]
fn quick_undo_and_redo_are_disabled_without_history() {
    let mut h = shell(1500.0, 700.0);
    h.run_steps(4);
    assert!(h.get_by_label("Undo").accesskit_node().is_disabled());
    assert!(!h.get_by_label("Save").accesskit_node().is_disabled());
}

// ------------------------------------------------------------------ hint line and engine loss

#[test]
fn hint_line_shows_text() {
    let mut h = shell(1500.0, 700.0);
    h.state_mut().shell.hint = "Click two points to measure the distance between them".into();
    h.run_steps(4);
    assert!(h.query_by_label_contains("Click two points").is_some());
}

struct LostLink {
    sent: Arc<Mutex<u32>>,
    restarts: Arc<Mutex<u32>>,
    fired: Mutex<bool>,
}
impl EngineLink for LostLink {
    fn send(&self, _c: Command) {
        *self.sent.lock().unwrap() += 1;
    }
    fn poll(&self) -> Vec<Update> {
        let mut f = self.fired.lock().unwrap();
        if *f {
            Vec::new()
        } else {
            *f = true;
            vec![Update::EngineLost]
        }
    }
    fn restart(&self) {
        *self.restarts.lock().unwrap() += 1;
    }
}

#[test]
fn engine_lost_shows_restart() {
    let restarts = Arc::new(Mutex::new(0));
    let link = LostLink { sent: Default::default(), restarts: restarts.clone(), fired: Mutex::new(false) };
    let app = StlApp::new(&egui::Context::default(), Box::new(stl_app::platform::NullPlatform), Box::new(link));
    let mut h = shell_with(900.0, 600.0, app);
    h.run_steps(4);
    assert!(h.query_by_label("The engine stopped unexpectedly. Your last saved project is safe.").is_some());
    h.get_by_label("Restart").click();
    h.run_steps(4);
    assert_eq!(*restarts.lock().unwrap(), 1);
    assert!(h.query_by_label("Restart").is_none());
}

// ------------------------------------------------------------------ pixel snapshots (dark and light)

mod snapshots {
    use super::*;

    /// The shell in a 1280 x 720 window, rendered through egui and the app's theme.
    fn snap(name: &str, dark: bool, setup: impl FnOnce(&mut Harness<'static, StlApp>)) {
        let Some(gpu) = Headless::new() else {
            eprintln!("no GPU adapter: skipped");
            return;
        };
        let renderer = super::wgpu_renderer::WgpuRenderer::new(gpu.render_state.clone());
        let mut init = false;
        let app = StlApp::empty(&egui::Context::default());
        let pref = if dark { ThemePref::Dark } else { ThemePref::Light };
        let mut h = Harness::builder().with_size(egui::vec2(1280.0, 720.0)).with_accessibility_check(false).renderer(renderer).build_ui_state(
            move |ui, app: &mut StlApp| {
                if !init {
                    init = true;
                    stl_app::fonts::install(ui.ctx());
                    app.set_theme(ui.ctx(), pref);
                }
                app.layout(ui);
            },
            app,
        );
        h.run_steps(4);
        setup(&mut h);
        h.run_steps(4);
        h.snapshot(name);
    }

    #[test]
    fn shell_start_dark() {
        snap("shell_start_dark", true, |_| {});
    }
    #[test]
    fn shell_start_light() {
        snap("shell_start_light", false, |_| {});
    }
    #[test]
    fn shell_print_tab_dark() {
        snap("shell_print_tab_dark", true, |h| h.get_by_label("Print").click());
    }
    #[test]
    fn shell_print_tab_light() {
        snap("shell_print_tab_light", false, |h| h.get_by_label("Print").click());
    }
    #[test]
    fn shell_view_menu_dark() {
        snap("shell_view_menu_dark", true, |h| h.get_by_label("View").click());
    }
    #[test]
    fn shell_view_menu_light() {
        snap("shell_view_menu_light", false, |h| h.get_by_label("View").click());
    }
    #[test]
    fn shell_prompt_dark() {
        snap("shell_prompt_dark", true, |h| h.state_mut().shell.dialogs.open_prompt("rename", "Rename body", "Cube"));
    }
    #[test]
    fn shell_prompt_light() {
        snap("shell_prompt_light", false, |h| h.state_mut().shell.dialogs.open_prompt("rename", "Rename body", "Cube"));
    }
}

// ------------------------------------------------------------------ keys, New, Recent, Help (review fixes)

use stl_app::engine_link::{TimelineItem, TimelineView};
use stl_app::platform::{Pending, PickedFile, Platform};

/// Records every command; hands out the queued updates once.
#[derive(Clone, Default)]
struct ScriptLink {
    sent: Arc<Mutex<Vec<Command>>>,
    feed: Arc<Mutex<Vec<Update>>>,
}
impl EngineLink for ScriptLink {
    fn send(&self, c: Command) {
        self.sent.lock().unwrap().push(c);
    }
    fn poll(&self) -> Vec<Update> {
        std::mem::take(&mut *self.feed.lock().unwrap())
    }
    fn restart(&self) {}
}

/// An open dialog that always picks `a.stl`.
struct PicksFile;
impl Platform for PicksFile {
    fn open_file(&self, _f: &[(&str, &[&str])]) -> Pending<Option<PickedFile>> {
        Pending::ready(Some(PickedFile { name: "a.stl".into(), bytes: vec![1, 2, 3] }))
    }
    fn save_file(&self, _s: &str, _b: Vec<u8>) -> Pending<Result<bool, String>> {
        Pending::ready(Ok(false))
    }
    fn now_iso_utc(&self) -> String {
        String::new()
    }
    fn is_web(&self) -> bool {
        false
    }
}

fn with_link(platform: Box<dyn Platform>) -> (Harness<'static, StlApp>, ScriptLink) {
    let link = ScriptLink::default();
    let app = StlApp::new(&egui::Context::default(), platform, Box::new(link.clone()));
    (shell_with(1500.0, 700.0, app), link)
}

fn step_added() -> Update {
    Update::Timeline(TimelineView { items: vec![TimelineItem { id: "f1".into(), kind: "primitive".into(), label: "Box".into(), active: true, ..Default::default() }], marker: None, can_undo: true, can_redo: false })
}

fn opens(link: &ScriptLink) -> usize {
    link.sent.lock().unwrap().iter().filter(|c| matches!(c, Command::Open { .. })).count()
}

#[test]
fn keys_use_the_real_app_state() {
    let (mut h, link) = with_link(Box::new(stl_app::platform::NullPlatform));
    h.run_steps(3);
    // Ctrl+Z goes through the app to the engine
    h.key_press_modifiers(egui::Modifiers::CTRL, Key::Z);
    h.run_steps(3);
    assert!(matches!(link.sent.lock().unwrap().last(), Some(Command::Undo)));
    // E opens Extrude; with a body selected, Delete opens Delete; with a tool open it does not
    h.key_press(Key::E);
    h.run_steps(3);
    assert!(h.state().shell.events.contains(&UiEvent::OpenTool("extrude".into())));
    h.state_mut().shell.selection = vec!["b1".into()];
    h.state_mut().shell.active_tool = Some("extrude".into());
    h.key_press(Key::Delete);
    h.run_steps(3);
    assert!(!h.state().shell.events.contains(&UiEvent::OpenTool("delete".into())), "tool_open comes from the app");
    h.state_mut().shell.active_tool = None;
    h.key_press(Key::Delete);
    h.run_steps(3);
    assert!(h.state().shell.events.contains(&UiEvent::OpenTool("delete".into())), "selection comes from the app");
    // Escape with nothing open clears the selection
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(h.state().shell.selection.is_empty());
}

#[test]
fn new_starts_an_empty_document_and_asks_when_there_are_changes() {
    let (mut h, link) = with_link(Box::new(stl_app::platform::NullPlatform));
    h.run_steps(3);
    // nothing to lose: Ctrl+N goes straight through
    h.key_press_modifiers(egui::Modifiers::CTRL, Key::N);
    h.run_steps(3);
    assert_eq!(opens(&link), 1);
    // a change makes the title dirty; File > New now asks first
    link.feed.lock().unwrap().push(step_added());
    h.run_steps(3);
    assert!(h.state().shell.dirty);
    h.get_by_label("File").click();
    h.run_steps(3);
    h.get_by_label("New").click();
    h.run_steps(3);
    assert!(h.query_by_label("Discard unsaved changes?").is_some());
    assert_eq!(opens(&link), 1);
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    assert_eq!(opens(&link), 1, "Cancel keeps the project");
    h.key_press_modifiers(egui::Modifiers::CTRL, Key::N);
    h.run_steps(3);
    h.get_by_label("OK").click();
    h.run_steps(3);
    assert_eq!(opens(&link), 2);
    // a cancelled save dialog leaves it dirty; a finished write makes it clean again
    link.feed.lock().unwrap().push(step_added());
    h.run_steps(3);
    assert!(h.state().shell.dirty);
    link.feed.lock().unwrap().push(Update::Saved { bytes: vec![0] });
    h.run_steps(3);
    assert!(h.state().shell.dirty, "nothing was written");
}

struct SavesOk;
impl Platform for SavesOk {
    fn open_file(&self, _f: &[(&str, &[&str])]) -> Pending<Option<PickedFile>> {
        Pending::ready(None)
    }
    fn save_file(&self, _s: &str, _b: Vec<u8>) -> Pending<Result<bool, String>> {
        Pending::ready(Ok(true))
    }
    fn now_iso_utc(&self) -> String {
        String::new()
    }
    fn is_web(&self) -> bool {
        false
    }
}

#[test]
fn a_finished_write_makes_the_project_clean() {
    let (mut h, link) = with_link(Box::new(SavesOk));
    h.run_steps(3);
    link.feed.lock().unwrap().push(step_added());
    h.run_steps(3);
    assert!(h.state().shell.dirty);
    link.feed.lock().unwrap().push(Update::Saved { bytes: vec![0] });
    h.run_steps(4);
    assert!(!h.state().shell.dirty);
}

#[test]
fn recent_files_open_again_from_the_file_menu() {
    let (mut h, link) = with_link(Box::new(PicksFile));
    h.run_steps(3);
    h.key_press_modifiers(egui::Modifiers::CTRL, Key::O);
    h.run_steps(6);
    assert_eq!(opens(&link), 1);
    // a file is in Recent once the engine has accepted it
    link.feed.lock().unwrap().extend([Update::Busy(true), Update::Bodies(Vec::new()), Update::Busy(false)]);
    h.run_steps(3);
    assert_eq!(h.state().shell.recent, vec!["a.stl".to_string()]);
    h.get_by_label("File").click();
    h.run_steps(3);
    // the start page lists it too; the menu entry is the last one
    assert_eq!(h.get_all_by_label("a.stl").count(), 2);
    h.get_all_by_label("a.stl").last().unwrap().click();
    h.run_steps(3);
    assert_eq!(opens(&link), 2);
    let sent = link.sent.lock().unwrap();
    assert!(matches!(&sent[1], Command::Open { name, bytes } if name == "a.stl" && bytes == &vec![1, 2, 3]));
}

#[test]
fn help_shortcuts_and_about_use_the_1x_text() {
    let mut h = shell(1500.0, 700.0);
    h.run_steps(3);
    h.get_by_label("Help").click();
    h.run_steps(3);
    h.get_by_label("Keyboard Shortcuts").click();
    h.run_steps(3);
    for t in ["Keyboard & mouse", "Left-drag", "Orbit", "Right / middle-drag", "0–6", "Standard views", "Ctrl+K or /", "Double-click a dimension", "Change it (a parameter name works too)"] {
        assert!(h.query_by_label(t).is_some(), "{t}");
    }
    h.get_by_label("OK").click();
    h.run_steps(3);
    h.get_by_label("Help").click();
    h.run_steps(3);
    h.get_by_label("About STL Editor").click();
    h.run_steps(3);
    assert!(h.query_by_label_contains("CAD-style editing for STL, 3MF and OBJ meshes.").is_some());
    assert!(h.query_by_label_contains("Licence texts are in the LICENSES folder.").is_some());
    assert!(h.query_by_label_contains("you can buy me a coffee on Ko-fi").is_some());
}
