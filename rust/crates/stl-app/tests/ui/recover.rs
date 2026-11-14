//! Recover Unsaved Work through the whole app (M5 Task 8; the dialog's own details are in
//! `tests/recover.rs`).
//!
//! 1.x behaviour matched (`web/js/app.js` `checkRecovery`; `stleditor/server.py` `recovery/*`):
//!
//! - At start-up, when another session did not close normally, a modal titled
//!   "Recover unsaved work?" lists each session, with the text "STL Editor didn’t close normally
//!   last time. This work was saved automatically:" (the apostrophe is the curly ’).
//! - Each row: the title in bold, then "N step(s) · autosaved <local date and time>"; the buttons
//!   "Discard" and "Recover"; the footer has "Later".
//! - After a recover: the toast "Recovered. Save it to keep it."
//! - Help › "Recover Unsaved Work…" runs the same check on demand; with nothing to recover it
//!   says "There is no unsaved work to recover."
//!
//! Every test here works in the throwaway `STLEDITOR2_CONFIG_DIR` the harness sets, so the
//! autosave folders are never Nathan's. Tests that plant or read the autosave folder take `LOCK`.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use serde_json::json;
use stl_app::app::StlApp;
use stl_app::engine_link::native::NativeLink;
use stl_app::engine_link::{Command, EngineLink};
use stl_app::platform::native::NativePlatform;
use stl_app::platform::Platform;
use stl_app::ui::ThemePref;

use super::harness::{project, settle, wait_for, SharedLink};

/// The autosave folder is shared by every test in this process.
static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn autosave_root() -> std::path::PathBuf {
    stl_platform::paths::data_dir().join("autosave")
}

fn sessions() -> usize {
    std::fs::read_dir(autosave_root()).map(|d| d.flatten().filter(|e| e.path().join("document.json").is_file()).count()).unwrap_or(0)
}

/// A window that crashed: it opened `basic.stlx`, added a box, wrote its copy and went away
/// without a clean exit (dropping the link keeps unsaved work, as a killed window does).
fn crashed_session(title: &str) {
    let root = autosave_root();
    let _ = std::fs::remove_dir_all(&root);
    let link = NativeLink::new();
    let bytes = std::fs::read(project("basic.stlx")).unwrap();
    link.send(Command::Open { name: "basic.stlx".into(), bytes });
    link.send(Command::AddFeature { kind: "primitive".into(), params: json!({"shape": "box", "size": [4.0, 4.0, 4.0]}), index: None, name: None });
    link.send(Command::Autosave { path: None, title: Some(title.into()) });
    let end = std::time::Instant::now() + Duration::from_secs(120);
    while link.is_busy() {
        assert!(std::time::Instant::now() < end, "the engine never went idle");
        std::thread::sleep(Duration::from_millis(5));
    }
    drop(link);
    assert_eq!(sessions(), 1, "the crashed window left its copy");
}

/// The app as the real window starts it (autosave and Recover on), with its settings in a
/// folder of its own and the update check off (so nothing can reach the network).
fn start_app(name: &str) -> (Harness<'static, StlApp>, Arc<NativeLink>) {
    let cfg = std::env::temp_dir().join(format!("stle2-ui-recover-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cfg);
    let platform = NativePlatform::with_config_dir(&cfg);
    platform.set_setting_json("update_check", json!(false));
    let link = Arc::new(NativeLink::new());
    let mut app = StlApp::new(&egui::Context::default(), Box::new(platform), Box::new(SharedLink { inner: link.clone() }));
    app.enable_autosave();
    let mut init = false;
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_accessibility_check(false).build_ui_state(
        move |ui, app: &mut StlApp| {
            if !init {
                init = true;
                stl_app::fonts::install(ui.ctx());
                app.set_theme(ui.ctx(), ThemePref::Dark);
            }
            app.layout(ui);
        },
        app,
    );
    h.run_steps(4);
    (h, link)
}

/// Closes the window cleanly and waits until its engine has finished with the autosave folder,
/// so the next test never meets a late write or delete (CI's slower runners outlast the 3 s a
/// dropped link waits).
fn close(h: Harness<'static, StlApp>, link: Arc<NativeLink>) {
    drop(h);
    link.end_session(Duration::from_secs(60));
    let end = std::time::Instant::now() + Duration::from_secs(60);
    while link.is_busy() && std::time::Instant::now() < end {
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Waits for the Recover dialog's list to arrive (the folder is read off the UI thread).
fn wait_dialog(h: &mut Harness<'static, StlApp>, link: &NativeLink) {
    wait_for(h, link, "the Recover dialog", |h| h.query_by_label("Recover unsaved work?").is_some());
}

fn help_recover(h: &mut Harness<'static, StlApp>) {
    h.get_by_label("Help").click();
    h.run_steps(4);
    h.get_by_label("Recover Unsaved Work…").click();
    h.run_steps(4);
}

#[test]
fn recover_dialog_lists_sessions() {
    let _g = lock();
    crashed_session("Bracket");
    let (mut h, link) = start_app("lists");
    wait_dialog(&mut h, &link);
    h.get_by_label("STL Editor didn’t close normally last time. This work was saved automatically:");
    h.get_by_label("Bracket");
    // basic.stlx has three steps; the box is the fourth
    h.get_by_label_contains("4 steps · autosaved 20");
    h.get_by_label("Discard");
    h.get_by_label("Recover");
    h.get_by_label("Later");
    assert!(h.state().bodies.is_empty(), "nothing is opened until Recover is pressed");

    // Recover: the project comes back as it was, and says so
    h.get_by_label("Recover").click();
    wait_for(&mut h, &link, "the recovered project", |h| h.state().bodies.len() == 3);
    assert!(h.query_by_label("Recover unsaved work?").is_none(), "the dialog closes");
    assert!(h.query_all_by_label_contains("Recovered. Save it to keep it.").count() > 0, "the toast says so");
    assert_eq!(h.state().timeline_view.items.len(), 4, "every step is back");
    // the loaded copy is discarded: opening it wrote a new one for this window
    settle(&mut h, &link);
    assert!(sessions() <= 1, "the old copy is gone");
    close(h, link);
}

#[test]
fn recover_later_discard_and_help_menu() {
    let _g = lock();
    crashed_session("Lid");
    let (mut h, link) = start_app("later");
    wait_dialog(&mut h, &link);

    // Later keeps the copy
    h.get_by_label("Later").click();
    h.run_steps(4);
    assert!(h.query_by_label("Recover unsaved work?").is_none());
    assert_eq!(sessions(), 1, "Later keeps every session");

    // Help > Recover Unsaved Work… shows the same list on demand
    help_recover(&mut h);
    wait_dialog(&mut h, &link);
    h.get_by_label("Lid");

    // Discard removes the copy; with nothing left the dialog goes
    h.get_by_label("Discard").click();
    h.run_steps(6);
    assert_eq!(sessions(), 0, "Discard deletes the copy");

    // nothing left: 1.x's toast
    h.state_mut().toasts.clear();
    h.run_steps(2);
    help_recover(&mut h);
    wait_for(&mut h, &link, "the nothing-to-recover toast", |h| h.query_all_by_label_contains("There is no unsaved work to recover.").count() > 0);
    assert!(h.query_by_label("Recover unsaved work?").is_none());
    close(h, link);
}

#[test]
fn a_clean_start_shows_no_dialog() {
    let _g = lock();
    let _ = std::fs::remove_dir_all(autosave_root());
    let (mut h, link) = start_app("clean");
    settle(&mut h, &link);
    h.run_steps(6);
    assert!(h.query_by_label("Recover unsaved work?").is_none(), "nothing to offer, nothing shown");
    assert!(h.query_by_label("There is no unsaved work to recover.").is_none(), "and no message at start-up");
    close(h, link);
}
