//! Open Recent from disk (M5 Task 8), through the whole app with a real `settings.json`.
//!
//! 1.x behaviour matched (`web/js/app.js` File menu, `stleditor/server.py` `_remember` and the
//! `recent` setting):
//!
//! - The File menu lists the recent files by file name (its tooltip is the full path): the first
//!   6 of a list that keeps 10, newest first, no duplicates; opening a file moves it to the top.
//! - The empty-start screen lists the same files under a "Recent" label; a click opens one.
//! - A file that was moved or deleted: 2.0 says "That file isn't there any more (<path>). It was
//!   taken off the Recent list." and drops it (Nathan, 2026-11-14; 1.x showed Python's
//!   `FileNotFoundError: [Errno 2] …` and kept the file listed).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use serde_json::json;
use stl_app::app::StlApp;
use stl_app::engine_link::native::NativeLink;
use stl_app::platform::native::NativePlatform;
use stl_app::platform::Platform;
use stl_app::ui::ThemePref;

use super::harness::{project, settle, wait_for, SharedLink};

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("stle2-ui-recent-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Copies `basic.stlx` (two bodies) to `dir/name`; the path as a string.
fn make_project(dir: &Path, name: &str) -> String {
    let p = dir.join(name);
    std::fs::copy(project("basic.stlx"), &p).unwrap();
    p.to_string_lossy().into_owned()
}

/// The app with the file list a previous run left in `settings.json` (a real file in `cfg`).
/// The update check is off, so nothing reaches the network.
fn app_with_recent(cfg: &Path, recent: &[String]) -> (Harness<'static, StlApp>, Arc<NativeLink>) {
    let platform = NativePlatform::with_config_dir(cfg);
    platform.set_setting_json("update_check", json!(false));
    platform.set_setting_json("recent", json!(recent));
    let link = Arc::new(NativeLink::new());
    let app = StlApp::new(&egui::Context::default(), Box::new(platform), Box::new(SharedLink { inner: link.clone() }));
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

/// What `settings.json` holds under `recent`, read as a new run would.
fn stored(cfg: &Path) -> Vec<String> {
    let p = NativePlatform::with_config_dir(cfg);
    p.setting_json("recent").and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

fn open_file_menu(h: &mut Harness<'static, StlApp>) {
    h.get_by_label("File").click();
    h.run_steps(4);
}

#[test]
fn open_recent_from_disk() {
    let d = temp("disk");
    let cfg = d.join("cfg");
    let files: Vec<String> = (1..=8).map(|i| make_project(&d, &format!("part{i}.stlx"))).collect();
    let (mut h, link) = app_with_recent(&cfg, &files);

    // the start screen lists the same files by name, "Recent" above them (the list shows six)
    assert_eq!(h.state().shell.recent, files, "the list came from settings.json");
    h.get_by_label("RECENT");
    for i in 1..=6 {
        h.get_by_label(&format!("part{i}.stlx"));
    }
    assert!(h.query_by_label("part7.stlx").is_none() && h.query_by_label("part8.stlx").is_none(), "only the first six are listed");

    // the File menu: New, Open…, then the recent files by name, six of them
    open_file_menu(&mut h);
    // (the ribbon also has Save, Insert Mesh and so on: the menu's own items are the ones with "…")
    for want in ["Open…", "Insert Mesh…", "Save As…"] {
        assert!(h.query_all_by_label(want).count() > 0, "File › {want}");
    }
    assert!(h.query_by_label("part7.stlx").is_none(), "the menu shows the first six of the ten kept");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);

    // a click opens it from disk, as Open does, and moves it to the top of the list
    h.get_all_by_label("part3.stlx").next().unwrap().click();
    wait_for(&mut h, &link, "part3.stlx to open", |h| h.state().bodies.len() == 2);
    assert_eq!(h.state().files.title.as_deref(), Some("part3.stlx"));
    let mut want = files.clone();
    let third = want.remove(2);
    want.insert(0, third);
    assert_eq!(h.state().shell.recent, want, "newest first, no duplicate");
    assert_eq!(stored(&cfg), want, "and it lasts: the next run reads the same list");

    // the menu now offers it first; clicking it again is no different
    open_file_menu(&mut h);
    let item = h.get_all_by_label("part3.stlx").last().unwrap();
    item.click();
    wait_for(&mut h, &link, "part3.stlx to open again", |h| h.state().files.title.as_deref() == Some("part3.stlx"));
    assert_eq!(stored(&cfg), want);
}

#[test]
fn missing_recent_dropped() {
    let d = temp("missing");
    let cfg = d.join("cfg");
    let gone = make_project(&d, "gone.stlx");
    let kept = make_project(&d, "kept.stlx");
    let (mut h, link) = app_with_recent(&cfg, &[gone.clone(), kept.clone()]);
    // the file is moved away after the last run
    std::fs::remove_file(&gone).unwrap();

    h.get_by_label("gone.stlx").click();
    wait_for(&mut h, &link, "the missing-file message", |h| h.query_all_by_label_contains("That file isn\u{2019}t there any more (").count() > 0);
    // plain words, with the path and what happened to the list
    let want = format!("That file isn\u{2019}t there any more ({gone}). It was taken off the Recent list.");
    assert!(h.query_all_by_label_contains(&want).count() > 0, "the message: {want}");
    assert!(h.state().bodies.is_empty(), "nothing was opened");
    // dropped from the list, the screen and the stored file
    h.run_steps(3);
    assert!(h.query_by_label("gone.stlx").is_none(), "no longer listed");
    assert_eq!(h.state().shell.recent, vec![kept.clone()]);
    assert_eq!(stored(&cfg), vec![kept.clone()], "and not offered again after a restart");

    // the file that is there still opens
    h.get_by_label("kept.stlx").click();
    wait_for(&mut h, &link, "kept.stlx to open", |h| h.state().bodies.len() == 2);
    assert_eq!(h.state().files.title.as_deref(), Some("kept.stlx"));
    settle(&mut h, &link);
}
