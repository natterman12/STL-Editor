//! The 2.0 update check in the app (M5 Task 4): once a day across windows, the skipped version,
//! Update now refusing while another 2.0 window is open, and the Help menu texts (1.x's).
//!
//! Network tests use the local fake release server, never GitHub. Every test points
//! `STLEDITOR2_CONFIG_DIR` at a temp folder and keeps settings in a temp folder of its own;
//! tests that change the environment are serialised by `ENV`.
#![cfg(not(target_arch = "wasm32"))]

#[path = "../../stl-platform/tests/support/fake_releases.rs"]
mod fake_releases;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use fake_releases::{FakeReleases, Faults};
use serde_json::{json, Value};
use stl_app::platform::native::NativePlatform;
use stl_app::platform::Platform;
use stl_app::updates::{self, BannerButton, UpdateAction, Updates};
use stl_platform::update::ASSET_NAME;

static ENV: Mutex<()> = Mutex::new(());

const DAY: f64 = 20.0 * 3600.0;

fn env(test_host: bool) -> (MutexGuard<'static, ()>, PathBuf) {
    let g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let d = temp("config");
    // SAFETY: serialised by ENV; no other thread in this binary reads the environment meanwhile.
    unsafe {
        std::env::set_var("STLEDITOR2_CONFIG_DIR", &d);
        if test_host {
            std::env::set_var("STLEDITOR2_UPDATE_TEST_HOST", "127.0.0.1");
        } else {
            std::env::remove_var("STLEDITOR2_UPDATE_TEST_HOST");
        }
    }
    (g, d)
}

fn temp(name: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("stle2-updates-app-{name}-{}-{n}", std::process::id()));
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

/// A port nothing listens on: any check against it fails.
fn dead_api() -> String {
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    format!("http://127.0.0.1:{port}/releases")
}

/// Poll until nothing is running in the background; the actions that came out.
fn settle(u: &mut Updates, p: &dyn Platform, now: f64) -> Vec<UpdateAction> {
    let t = Instant::now();
    let mut out = Vec::new();
    while u.is_busy() {
        assert!(t.elapsed() < Duration::from_secs(20), "the update work never finished");
        out.extend(u.poll(p, now));
        std::thread::sleep(Duration::from_millis(10));
    }
    out.extend(u.poll(p, now));
    out
}

fn toasts(actions: &[UpdateAction]) -> Vec<String> {
    actions.iter().filter_map(|a| if let UpdateAction::Toast { text, .. } = a { Some(text.clone()) } else { None }).collect()
}

fn platform(dir: &Path) -> NativePlatform {
    NativePlatform::with_config_dir(dir)
}

#[test]
fn ignores_older_and_skipped() {
    let (_g, _d) = env(false);
    let cfg = temp("skip");
    let p = platform(&cfg);
    let root = temp("skip-autosave");
    let srv = FakeReleases::start(
        json!([release("v2.0.1", &gh("v2.0.1"), 10), release("v1.30.0", &gh("v1.30.0"), 10)]),
        vec![],
        Faults::default(),
    );
    let api = format!("{}/releases", srv.base_url());
    let now = 1_800_000_000.0;

    // newer and not skipped: the background check shows the banner
    let mut u = Updates::custom(&api, "2.0.0", None, root.clone());
    u.start_background(&p, now);
    settle(&mut u, &p, now);
    let b = u.banner().expect("banner for 2.0.1");
    assert_eq!(b.version, "2.0.1");

    // skipped: the background check stays quiet...
    assert!(u.press(BannerButton::Skip, &p).is_none());
    assert!(u.banner().is_none());
    assert_eq!(p.setting_json("update_skip"), Some(json!("2.0.1")));
    let mut u = Updates::custom(&api, "2.0.0", None, root.clone());
    u.start_background(&p, now + 2.0 * DAY);
    settle(&mut u, &p, now + 2.0 * DAY);
    assert!(u.banner().is_none(), "a skipped version is not offered by the daily check");
    // ...from the cache too
    let mut u = Updates::custom(&api, "2.0.0", None, root.clone());
    u.start_background(&p, now + 2.0 * DAY + 60.0);
    assert!(!u.is_busy());
    assert!(u.banner().is_none());
    // ...but Check for Updates still shows it (1.x: `available && (manual || !skipped)`)
    u.check_now(&p, now + 2.0 * DAY + 120.0);
    assert_eq!(u.busy_note(), Some(updates::CHECKING));
    settle(&mut u, &p, now + 2.0 * DAY + 120.0);
    assert_eq!(u.banner().expect("manual check shows a skipped version").version, "2.0.1");

    // older or the same: nothing, and a manual check says so in 1.x's words; 1.x releases ignored
    for current in ["2.0.1", "2.1.0"] {
        let mut u = Updates::custom(&api, current, None, root.clone());
        u.check_now(&p, now);
        let acts = settle(&mut u, &p, now);
        assert!(u.banner().is_none());
        assert_eq!(toasts(&acts), vec![format!("You have the latest version ({current}).")]);
    }
}

#[test]
fn checks_once_a_day() {
    let (_g, _d) = env(false);
    let cfg = temp("daily");
    // two windows, one settings.json
    let (pa, pb) = (platform(&cfg), platform(&cfg));
    let root = temp("daily-autosave");
    let srv = FakeReleases::start(json!([release("v2.0.1", &gh("v2.0.1"), 10)]), vec![], Faults::default());
    let now = 1_800_000_000.0;

    let mut a = Updates::custom(&format!("{}/releases", srv.base_url()), "2.0.0", None, root.clone());
    a.start_background(&pa, now);
    assert!(a.is_busy(), "the first check of the day goes to the network");
    settle(&mut a, &pa, now);
    assert!(a.banner().is_some());
    assert_eq!(pa.setting_json("update_last").and_then(|v| v.as_f64()), Some(now));
    assert_eq!(pa.setting_json("update_cache").and_then(|c| c.get("version").cloned()), Some(json!("2.0.1")));

    // the other window, an hour later: no network (its server is dead), the cached answer
    let mut b = Updates::custom(&dead_api(), "2.0.0", None, root.clone());
    b.start_background(&pb, now + 3600.0);
    assert!(!b.is_busy(), "checked today already, in another window");
    assert_eq!(b.banner().expect("cached release").version, "2.0.1");

    // a day later it checks again; a failed check is quiet and, as 1.x, leaves `update_last`
    // alone, so the next start asks again
    let later = now + DAY + 3600.0;
    let mut b = Updates::custom(&dead_api(), "2.0.0", None, root.clone());
    b.start_background(&pb, later);
    assert!(b.is_busy());
    let acts = settle(&mut b, &pb, later);
    assert!(acts.is_empty(), "the background check never toasts: {:?}", toasts(&acts));
    assert_eq!(pb.setting_json("update_last").and_then(|v| v.as_f64()), Some(now));
    assert_eq!(pb.setting_json(updates::UPDATE_CLAIM), Some(Value::Null), "the claim is rolled back");
    let mut c = Updates::custom(&dead_api(), "2.0.0", None, root.clone());
    c.start_background(&pa, later + 60.0);
    assert!(c.is_busy(), "retried after a failed check");
    settle(&mut c, &pa, later + 60.0);

    // two windows starting together: the second waits for the first one's answer
    let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let hang = format!("http://{}/releases", silent.local_addr().unwrap());
    let t = later + 120.0;
    let mut first = Updates::custom(&hang, "2.0.0", None, root.clone());
    first.start_background(&pa, t);
    assert!(first.is_busy());
    let mut second = Updates::custom(&dead_api(), "2.0.0", None, root.clone());
    second.start_background(&pb, t + 1.0);
    assert!(!second.is_busy(), "another window is checking right now");
    // a claim left by a window that never finished is stale after a minute
    let mut third = Updates::custom(&dead_api(), "2.0.0", None, root.clone());
    third.start_background(&pb, t + 90.0);
    assert!(third.is_busy());
    settle(&mut third, &pb, t + 90.0);

    // a check that worked writes `update_last`; one "in the future" (the clock was changed) is stale
    let mut e = Updates::custom(&format!("{}/releases", srv.base_url()), "2.0.0", None, root.clone());
    e.start_background(&pa, t + 200.0);
    settle(&mut e, &pa, t + 200.0);
    assert_eq!(pa.setting_json("update_last").and_then(|v| v.as_f64()), Some(t + 200.0));
    pa.set_setting_json("update_last", json!(t + 30.0 * DAY));
    let mut f = Updates::custom(&dead_api(), "2.0.0", None, root.clone());
    f.start_background(&pa, t + 300.0);
    assert!(f.is_busy(), "a last check in the future doesn't stop today's");
    settle(&mut f, &pa, t + 300.0);

    // checks off: nothing at all, ever
    pa.set_setting_json("update_check", json!(false));
    let mut d = Updates::custom(&dead_api(), "2.0.0", None, root.clone());
    d.start_background(&pa, later + 3.0 * DAY);
    assert!(!d.is_busy());
    assert!(d.banner().is_none());
}

#[test]
fn check_runs_off_ui_thread() {
    let (_g, _d) = env(false);
    let cfg = temp("thread");
    let p = platform(&cfg);
    // a server that accepts the connection and never answers
    let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let api = format!("http://{}/releases", silent.local_addr().unwrap());
    let mut u = Updates::custom(&api, "2.0.0", None, temp("thread-autosave"));
    let t = Instant::now();
    u.start_background(&p, 1_800_000_000.0);
    assert!(u.poll(&p, 1_800_000_000.0).is_none());
    assert!(t.elapsed() < Duration::from_millis(100), "start and poll returned at once ({:?})", t.elapsed());
    assert!(u.is_busy());
}

#[test]
fn refuses_with_other_window_open() {
    let (_g, _d) = env(true);
    let cfg = temp("refuse");
    let p = platform(&cfg);
    let root = temp("refuse-autosave");
    let install = temp("refuse-install");
    let running = install.join("stl-editor-2.exe");
    std::fs::write(&running, b"the installed exe").unwrap();
    let new_exe: Vec<u8> = (0..50_000u32).map(|i| (i % 241) as u8).collect();

    let assets = FakeReleases::start(json!([]), vec![(ASSET_NAME.into(), new_exe.clone())], Faults::default());
    let asset = format!("{}/assets/{ASSET_NAME}", assets.base_url());
    let srv = FakeReleases::start(json!([release("v2.0.1", &asset, new_exe.len())]), vec![], Faults::default());

    let api = format!("{}/releases", srv.base_url());
    // a window that closed without cleaning up (its lock file is left, nobody holds it)
    std::fs::write(root.join("99999-0.lock"), b"").unwrap();
    // this window, and another 2.0 window (each holds its own lock from start-up)
    let mut u = Updates::custom(&api, "2.0.0", Some(running.clone()), root.clone());
    let other = Updates::custom(&api, "2.0.0", None, root.clone());
    u.check_now(&p, 1_800_000_000.0);
    settle(&mut u, &p, 1_800_000_000.0);
    let b = u.banner().expect("banner");
    assert_eq!(b.buttons, vec!["Skip this version", "Later", "What’s new", "Update now"]);

    // another 2.0 window is open: refused in plain words, nothing downloaded or changed
    assert!(u.press(BannerButton::UpdateNow, &p).is_none());
    assert!(!u.is_busy());
    assert_eq!(u.banner().unwrap().note.as_deref(), Some(updates::OTHER_WINDOWS));
    assert_eq!(std::fs::read(&running).unwrap(), b"the installed exe");
    assert_eq!(std::fs::read_dir(&install).unwrap().count(), 1);

    // the other window closes; this window's own session doesn't count
    drop(other);
    assert!(u.press(BannerButton::UpdateNow, &p).is_none());
    assert_eq!(u.banner().unwrap().note.as_deref(), Some(updates::DOWNLOADING));
    assert_eq!(u.banner().unwrap().buttons, vec!["Cancel"]);
    let acts = settle(&mut u, &p, 1_800_000_000.0);
    let exe = acts.iter().find_map(|a| if let UpdateAction::Restart { exe } = a { Some(exe.clone()) } else { None });
    assert_eq!(exe.as_deref(), Some(running.as_path()), "restart the swapped-in exe: {acts:?}");
    assert_eq!(u.banner().unwrap().note.as_deref(), Some(updates::RESTARTING));
    assert_eq!(std::fs::read(&running).unwrap(), new_exe);
    assert_eq!(std::fs::read(install.join("stl-editor-2.old.exe")).unwrap(), b"the installed exe");
}

#[test]
fn cancel_hides_banner_and_removes_download() {
    let (_g, _d) = env(true);
    let cfg = temp("cancel");
    let p = platform(&cfg);
    let install = temp("cancel-install");
    let running = install.join("stl-editor-2.exe");
    std::fs::write(&running, b"installed").unwrap();
    let data = vec![3u8; 300_000];
    let assets = FakeReleases::start(json!([]), vec![(ASSET_NAME.into(), data.clone())], Faults::default());
    let srv = FakeReleases::start(json!([release("v2.0.1", &format!("{}/assets/{ASSET_NAME}", assets.base_url()), data.len())]), vec![], Faults::default());
    let mut u = Updates::custom(&format!("{}/releases", srv.base_url()), "2.0.0", Some(running.clone()), temp("cancel-windows"));
    u.check_now(&p, 1_800_000_000.0);
    settle(&mut u, &p, 1_800_000_000.0);
    u.press(BannerButton::UpdateNow, &p);
    assert!(u.press(BannerButton::Cancel, &p).is_none());
    assert!(u.banner().is_none(), "Cancel hides the banner at once (1.x)");
    let acts = settle(&mut u, &p, 1_800_000_000.0);
    assert!(acts.is_empty());
    assert_eq!(std::fs::read(&running).unwrap(), b"installed");
    let left: Vec<_> = std::fs::read_dir(&install).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(left.len(), 1, "the cancelled download is removed: {left:?}");
}

#[test]
fn cancelled_download_reused_only_for_same_release() {
    let (_g, _d) = env(true);
    let p = platform(&temp("orphan"));
    let install = temp("orphan-install");
    let running = install.join("stl-editor-2.exe");
    std::fs::write(&running, b"installed").unwrap();
    let data = vec![9u8; 30_000];
    // 2.0.1's download never finishes (the server accepts and says nothing)
    let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let stuck = format!("http://{}/assets/{ASSET_NAME}", silent.local_addr().unwrap());
    let api1 = FakeReleases::start(json!([release("v2.0.1", &stuck, data.len())]), vec![], Faults::default());
    let assets = FakeReleases::start(json!([]), vec![(ASSET_NAME.into(), data.clone())], Faults::default());
    let api2 = FakeReleases::start(json!([release("v2.0.2", &format!("{}/assets/{ASSET_NAME}", assets.base_url()), data.len())]), vec![], Faults::default());
    let now = 1_800_000_000.0;
    let mut u = Updates::custom(&format!("{}/releases", api1.base_url()), "2.0.0", Some(running.clone()), temp("orphan-windows"));
    u.check_now(&p, now);
    settle(&mut u, &p, now);
    u.press(BannerButton::UpdateNow, &p);
    u.press(BannerButton::Cancel, &p);
    assert!(u.banner().is_none());

    // a newer release turns up: Update now starts its own download, not 2.0.1's stuck one
    u.set_api(&format!("{}/releases", api2.base_url()));
    u.check_now(&p, now);
    let t = Instant::now();
    while u.banner().is_none() {
        assert!(t.elapsed() < Duration::from_secs(20));
        u.poll(&p, now);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(u.banner().unwrap().version, "2.0.2");
    u.press(BannerButton::UpdateNow, &p);
    let t = Instant::now();
    let exe = loop {
        assert!(t.elapsed() < Duration::from_secs(20), "the 2.0.2 download never finished");
        if let Some(UpdateAction::Restart { exe }) = u.poll(&p, now) {
            break exe;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(exe, running);
    assert_eq!(std::fs::read(&running).unwrap(), data);
}

#[test]
fn failed_update_shows_why_and_keeps_exe() {
    let (_g, _d) = env(true);
    let cfg = temp("fail");
    let p = platform(&cfg);
    let install = temp("fail-install");
    let running = install.join("stl-editor-2.exe");
    std::fs::write(&running, b"installed").unwrap();
    let data = vec![7u8; 4000];
    let srv = FakeReleases::start(json!([]), vec![(ASSET_NAME.into(), data.clone())], Faults { wrong_size: true, ..Faults::default() });
    let asset = format!("{}/assets/{ASSET_NAME}", srv.base_url());
    let api_srv = FakeReleases::start(json!([release("v2.0.1", &asset, data.len())]), vec![], Faults::default());
    let mut u = Updates::custom(&format!("{}/releases", api_srv.base_url()), "2.0.0", Some(running.clone()), temp("fail-autosave"));
    u.check_now(&p, 1_800_000_000.0);
    settle(&mut u, &p, 1_800_000_000.0);
    u.press(BannerButton::UpdateNow, &p);
    let acts = settle(&mut u, &p, 1_800_000_000.0);
    assert!(acts.iter().all(|a| !matches!(a, UpdateAction::Restart { .. })));
    let b = u.banner().unwrap();
    assert_eq!(b.note.as_deref(), Some(stl_platform::update::WRONG_SIZE));
    assert_eq!(b.buttons, vec!["Close", "Download instead"]);
    assert_eq!(std::fs::read(&running).unwrap(), b"installed");
    assert_eq!(std::fs::read_dir(&install).unwrap().count(), 1);
    assert!(matches!(u.press(BannerButton::DownloadInstead, &p), Some(UpdateAction::OpenUrl(url)) if url == "https://github.com/natterman12/STL-Editor/releases/tag/v2.0.1"));
}

#[test]
fn help_menu_items() {
    let (_g, _d) = env(false);
    assert_eq!(updates::help_menu_items(false), vec![("help.check_updates", "Check for Updates…"), ("help.update_settings", "Update Settings…")]);
    assert!(updates::help_menu_items(true).is_empty(), "hidden on the web edition");

    // 1.x's banner and toasts
    assert_eq!(updates::available_title("2.0.1"), "STL Editor 2.0.1 is available");
    assert_eq!(updates::latest_text("2.0.0"), "You have the latest version (2.0.0).");
    assert_eq!(updates::CHECKING, "Checking for updates…");
    assert_eq!(updates::CHECKS_OFF, "Update checks are off.");
    assert_eq!(updates::DOWNLOADING, "Downloading update…");
    assert_eq!(updates::RESTARTING, "Restarting STL Editor…");

    let cfg = temp("texts");
    let p = platform(&cfg);
    let srv = FakeReleases::start(json!([release("v2.0.1", &gh("v2.0.1"), 10)]), vec![], Faults::default());
    let api = format!("{}/releases", srv.base_url());
    // installed: Update now
    let mut u = Updates::custom(&api, "2.0.0", Some(cfg.join("stl-editor-2.exe")), temp("texts-autosave"));
    u.check_now(&p, 1_800_000_000.0);
    settle(&mut u, &p, 1_800_000_000.0);
    let b = u.banner().unwrap();
    assert_eq!(b.title, "STL Editor 2.0.1 is available");
    assert_eq!(b.line, "You have 2.0.0. Update now downloads it, swaps it in and restarts STL Editor. Unsaved work is kept for Recover Unsaved Work.");
    assert!(matches!(u.press(BannerButton::WhatsNew, &p), Some(UpdateAction::OpenUrl(_))));
    assert!(u.banner().is_some(), "What’s new keeps the banner");
    assert!(u.press(BannerButton::Later, &p).is_none());
    assert!(u.banner().is_none());
    // not installed (a build run from elsewhere): Download
    let mut u = Updates::custom(&api, "2.0.0", None, temp("texts-autosave2"));
    u.check_now(&p, 1_800_000_000.0);
    settle(&mut u, &p, 1_800_000_000.0);
    assert_eq!(u.banner().unwrap().buttons, vec!["Skip this version", "Later", "Download"]);
    assert_eq!(u.banner().unwrap().line, format!("You have 2.0.0. {}", updates::DOWNLOAD_LINE));

    // Update Settings: off says so and stops the checks; on checks straight away (1.x)
    u.open_settings(&p);
    assert_eq!(u.settings_open(), Some(true));
    let a = u.set_checks(&p, false, 1_800_000_000.0);
    assert!(matches!(a, Some(UpdateAction::Toast { ref text, .. }) if text == updates::CHECKS_OFF));
    assert_eq!(p.setting_json("update_check"), Some(json!(false)));
    assert_eq!(u.settings_open(), None);
    u.open_settings(&p);
    assert_eq!(u.settings_open(), Some(false));
    assert!(u.set_checks(&p, true, 1_800_000_000.0).is_none());
    assert_eq!(p.setting_json("update_check"), Some(json!(true)));
    assert!(u.is_busy(), "turning checks on checks now");
    settle(&mut u, &p, 1_800_000_000.0);
}
