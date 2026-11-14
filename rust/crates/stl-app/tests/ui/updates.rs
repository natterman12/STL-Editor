//! Check for Updates, the update banner and Update Settings (M5 Task 8), against the local fake
//! release server, never GitHub.
//!
//! 1.x behaviour matched (`web/js/app.js` `checkUpdates`, `showUpdateBanner`, `updateSettings`;
//! `stleditor/server.py` `update/status`, `update/install`, `update/apply`):
//!
//! - Help menu, desktop: "Check for Updates…", "Update Settings…", "Recover Unsaved Work…".
//! - A newer version: the banner "STL Editor <latest> is available", "You have <current>." and
//!   the install line (1.x's portable build: "Update now downloads it, swaps it in and restarts
//!   STL Editor. Unsaved work is kept for Recover Unsaved Work."). Buttons: "Skip this version",
//!   "Later", "What’s new", "Update now". Skip this version stores the version, and the
//!   background check stops offering it.
//! - Check for Updates while up to date: the toast "You have the latest version (<current>)."
//! - A download that fails shows why in the banner with "Close" and "Download instead", and the
//!   installed exe is left as it was.
//! - Update Settings: 1.x asked for a repository (empty to turn checks off); 2.0 has a fixed
//!   repository and an on/off box (spec 2.4). Its wording is new, pending Nathan's OK.
//!
//! What the app does with a banner action (a toast, the browser, the restart) is in
//! `tests/updates_app.rs` and `tests/recover.rs`; here the banner is drawn and clicked as the
//! window does, by the same `Updates::show`. `StlApp` builds its updater against GitHub inside
//! `enable_autosave`, with no way to point it elsewhere, so the app-level Check for Updates is
//! not run here (see the report).

#[path = "../../../stl-platform/tests/support/fake_releases.rs"]
mod fake_releases;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use fake_releases::{FakeReleases, Faults};
use serde_json::{json, Value};
use stl_app::platform::native::NativePlatform;
use stl_app::platform::Platform;
use stl_app::theme;
use stl_app::updates::{self, UpdateAction, Updates};
use stl_platform::update::ASSET_NAME;

use super::harness::empty_harness;

const NOW: f64 = 1_800_000_000.0;

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("stle2-ui-updates-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn release(tag: &str, url: &str, size: usize) -> Value {
    json!({ "tag_name": tag, "draft": false, "body": format!("What changed in {tag}"),
            "assets": [{ "name": ASSET_NAME, "browser_download_url": url, "size": size }] })
}

fn gh(tag: &str) -> String {
    format!("https://github.com/natterman12/STL-Editor/releases/download/{tag}/{ASSET_NAME}")
}

struct Ui {
    u: Updates,
    p: NativePlatform,
    actions: Vec<UpdateAction>,
}

/// The updater drawn the way the window draws it: `Updates::show` every frame.
fn window(name: &str, api: &str, current: &str, exe: Option<&Path>) -> Harness<'static, Ui> {
    let cfg = temp(&format!("{name}-cfg"));
    let p = NativePlatform::with_config_dir(&cfg);
    let u = Updates::custom(api, current, exe.map(Path::to_path_buf), temp(&format!("{name}-windows")));
    let mut fonts = false;
    Harness::builder().with_size(egui::vec2(1000.0, 700.0)).with_accessibility_check(false).build_ui_state(
        move |ui, st: &mut Ui| {
            if !fonts {
                fonts = true;
                stl_app::fonts::install(ui.ctx());
            }
            let tk = theme::tokens(false);
            theme::apply(ui.ctx(), &tk);
            if let Some(a) = st.u.show(ui.ctx(), &tk, &st.p, NOW) {
                st.actions.push(a);
            }
        },
        Ui { u, p, actions: Vec::new() },
    )
}

/// Steps frames until `ready` (the network work runs on its own threads).
fn until(h: &mut Harness<'static, Ui>, what: &str, ready: impl Fn(&Harness<'static, Ui>) -> bool) {
    let end = Instant::now() + Duration::from_secs(30);
    while !ready(h) {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        h.run_steps(2);
        std::thread::sleep(Duration::from_millis(15));
    }
}

fn check(h: &mut Harness<'static, Ui>, now: f64) {
    let ui = h.state_mut();
    ui.u.check_now(&ui.p, now);
}

fn open_settings(h: &mut Harness<'static, Ui>) {
    let ui = h.state_mut();
    ui.u.open_settings(&ui.p);
}

fn banner_up(h: &Harness<'static, Ui>) -> bool {
    h.query_by_label("STL Editor 2.0.1 is available").is_some()
}

const INSTALL_LINE: &str = "You have 2.0.0. Update now downloads it, swaps it in and restarts STL Editor. Unsaved work is kept for Recover Unsaved Work.";

#[test]
fn help_menu_has_the_update_items() {
    let (mut h, _link) = empty_harness();
    h.get_by_label("Help").click();
    h.run_steps(4);
    for item in ["Check for Updates…", "Update Settings…", "Recover Unsaved Work…", "Keyboard Shortcuts", "About STL Editor"] {
        assert!(h.query_by_label(item).is_some(), "Help › {item}");
    }
}

#[test]
fn check_for_updates() {
    let srv = FakeReleases::start(json!([release("v2.0.1", &gh("v2.0.1"), 10)]), vec![], Faults::default());
    let api = format!("{}/releases", srv.base_url());
    let exe_dir = temp("exe");
    let exe = exe_dir.join("stl-editor-2.exe");
    std::fs::write(&exe, b"the installed exe").unwrap();

    // Check for Updates: the busy note while it asks, then the banner with 1.x's words
    let mut h = window("banner", &api, "2.0.0", Some(&exe));
    check(&mut h, NOW);
    assert_eq!(h.state().u.busy_note(), Some("Checking for updates…"), "1.x's note while it asks");
    until(&mut h, "the banner", banner_up);
    h.get_by_label(INSTALL_LINE);
    for button in ["Skip this version", "Later", "What’s new", "Update now"] {
        h.get_by_label(button);
    }
    assert!(h.state().actions.is_empty(), "nothing else happened");

    // What’s new opens the release page and keeps the banner
    h.get_by_label("What’s new").click();
    h.run_steps(3);
    assert_eq!(h.state().actions, [UpdateAction::OpenUrl("https://github.com/natterman12/STL-Editor/releases/tag/v2.0.1".into())]);
    assert!(banner_up(&h));

    // Later puts it away
    h.get_by_label("Later").click();
    h.run_steps(3);
    assert!(!banner_up(&h));

    // Skip this version: stored, and the background check stops offering it (a manual check still does)
    check(&mut h, NOW);
    until(&mut h, "the banner again", banner_up);
    h.get_by_label("Skip this version").click();
    h.run_steps(3);
    assert!(!banner_up(&h));
    assert_eq!(h.state().p.setting_json("update_skip"), Some(json!("2.0.1")));
    let ui = h.state_mut();
    ui.u.start_background(&ui.p, NOW + 60.0);
    h.run_steps(4);
    assert!(!banner_up(&h), "the skipped version is not offered by the background check");
    check(&mut h, NOW + 120.0);
    until(&mut h, "the manual check to offer it anyway", banner_up);
}

#[test]
fn up_to_date_says_so() {
    let srv = FakeReleases::start(json!([release("v2.0.0", &gh("v2.0.0"), 10)]), vec![], Faults::default());
    let api = format!("{}/releases", srv.base_url());
    let mut h = window("latest", &api, "2.0.0", None);
    check(&mut h, NOW);
    until(&mut h, "the answer", |h| !h.state().actions.is_empty());
    assert_eq!(h.state().actions, [UpdateAction::Toast { text: "You have the latest version (2.0.0).".into(), warn: false }]);
    assert!(h.query_by_label("STL Editor 2.0.0 is available").is_none(), "no banner when there is nothing newer");
}

#[test]
fn a_copy_that_cannot_update_itself_offers_the_download() {
    let srv = FakeReleases::start(json!([release("v2.0.1", &gh("v2.0.1"), 10)]), vec![], Faults::default());
    let api = format!("{}/releases", srv.base_url());
    let mut h = window("download", &api, "2.0.0", None);
    check(&mut h, NOW);
    until(&mut h, "the banner", banner_up);
    h.get_by_label(&format!("You have 2.0.0. {}", updates::DOWNLOAD_LINE));
    h.get_by_label("Download");
    assert!(h.query_by_label("Update now").is_none(), "only the installed copy can swap itself");
    h.get_by_label("Download").click();
    h.run_steps(3);
    assert_eq!(h.state().actions, [UpdateAction::OpenUrl("https://github.com/natterman12/STL-Editor/releases/tag/v2.0.1".into())]);
    assert!(!banner_up(&h));
}

#[test]
fn a_failed_download_shows_why_and_keeps_the_exe() {
    // The updater only downloads from GitHub, except from this host in tests.
    // SAFETY: only the updater reads it, on its own threads, and it is only ever set to this value.
    unsafe { std::env::set_var("STLEDITOR2_UPDATE_TEST_HOST", "127.0.0.1") };
    let data = vec![7u8; 40_000];
    let assets = FakeReleases::start(json!([]), vec![(ASSET_NAME.into(), data.clone())], Faults { wrong_size: true, ..Default::default() });
    let asset_url = format!("{}/assets/{ASSET_NAME}", assets.base_url());
    let srv = FakeReleases::start(json!([release("v2.0.1", &asset_url, data.len())]), vec![], Faults::default());
    let api = format!("{}/releases", srv.base_url());
    let install = temp("failed-install");
    let exe = install.join("stl-editor-2.exe");
    std::fs::write(&exe, b"the installed exe").unwrap();

    let mut h = window("failed", &api, "2.0.0", Some(&exe));
    check(&mut h, NOW);
    until(&mut h, "the banner", banner_up);
    h.get_by_label("Update now").click();
    until(&mut h, "the failure", |h| h.query_by_label("Download instead").is_some());
    h.get_by_label(stl_platform::update::WRONG_SIZE);
    h.get_by_label("Close");
    assert_eq!(std::fs::read(&exe).unwrap(), b"the installed exe", "the installed exe is as it was");
    let leftovers: Vec<_> = std::fs::read_dir(&install).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".new.exe")).collect();
    assert!(leftovers.is_empty(), "no half download left behind: {leftovers:?}");
    assert!(!install.join(stl_platform::update::OLD_NAME).exists(), "nothing was swapped");
    assert!(!h.state().actions.iter().any(|a| matches!(a, UpdateAction::Restart { .. })), "no restart");

    // Close puts the banner away
    h.get_by_label("Close").click();
    h.run_steps(3);
    assert!(!banner_up(&h));
}

#[test]
fn update_settings_dialog() {
    let srv = FakeReleases::start(json!([release("v2.0.0", &gh("v2.0.0"), 10)]), vec![], Faults::default());
    let api = format!("{}/releases", srv.base_url());
    let mut h = window("settings", &api, "2.0.0", None);

    open_settings(&mut h);
    h.run_steps(4);
    h.get_by_label("Update Settings");
    h.get_by_label("Check for new versions once a day");
    h.get_by_label_contains("STL Editor asks GitHub (github.com/natterman12/STL-Editor) for the newest version.");
    h.get_by_label("OK");
    h.get_by_label("Cancel");

    // Cancel changes nothing
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    assert!(h.query_by_label("Update Settings").is_none());
    assert_eq!(h.state().p.setting_json("update_check"), None);

    // off: the toast says so (1.x), the choice is stored, and the background check never starts
    open_settings(&mut h);
    h.run_steps(4);
    h.get_by_label("Check for new versions once a day").click();
    h.run_steps(2);
    h.get_by_label("OK").click();
    h.run_steps(3);
    assert_eq!(h.state().actions, [UpdateAction::Toast { text: "Update checks are off.".into(), warn: false }]);
    assert_eq!(h.state().p.setting_json("update_check"), Some(json!(false)));
    let ui = h.state_mut();
    ui.u.start_background(&ui.p, NOW + 5.0 * 86_400.0);
    assert!(!h.state().u.is_busy(), "checks are off: nothing asked");

    // on again: 1.x checks straight away (the answer here is "nothing newer")
    h.state_mut().actions.clear();
    open_settings(&mut h);
    h.run_steps(4);
    h.get_by_label("Check for new versions once a day").click();
    h.run_steps(2);
    h.get_by_label("OK").click();
    h.run_steps(3);
    assert_eq!(h.state().p.setting_json("update_check"), Some(json!(true)));
    until(&mut h, "the check that turning checks on starts", |h| !h.state().actions.is_empty());
    assert_eq!(h.state().actions, [UpdateAction::Toast { text: "You have the latest version (2.0.0).".into(), warn: false }]);
}
