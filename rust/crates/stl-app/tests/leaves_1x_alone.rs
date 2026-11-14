//! M5 review focus 6: 2.0 never touches 1.x's folders.
//!
//! The parent test builds a throwaway profile with 1.x's `STL Editor` folders (sample files and a
//! 1.x `settings.json`), then runs this same test binary again as a child whose environment
//! points the profile variables (`APPDATA`, `LOCALAPPDATA`, `USERPROFILE`, `HOME`, `XDG_*`) at
//! that profile and has no `STLEDITOR2_*` override. So the child uses 2.0's real default
//! folders, below the temp profile and never Nathan's. The child runs settings, recent files,
//! autosave (write, list as recoverable, load, discard), the Recover list and an update check
//! against the fake release server. The parent then checks every 1.x file is byte for byte as it
//! was (and as old), and that no file or folder appeared in 1.x's folders.
//!
//! The same checks for `xtask install` / `uninstall --purge` are in
//! `rust/xtask/tests/leaves_1x_alone.rs`.
#![cfg(not(target_arch = "wasm32"))]

#[path = "../../stl-platform/tests/support/fake_releases.rs"]
mod fake_releases;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime};

use fake_releases::{FakeReleases, Faults};
use serde_json::json;
use stl_app::engine_link::native::NativeLink;
use stl_app::engine_link::{Command as Cmd, EngineLink};
use stl_app::platform::native::NativePlatform;
use stl_app::platform::{Pending, Platform};
use stl_app::recover::{DesktopStore, RecoverStore};
use stl_app::updates::{UpdateAction, Updates};
use stl_platform::update::ASSET_NAME;

const PROFILE_VAR: &str = "STLE2_LEAVES_1X_PROFILE";

/// 1.x's two folders inside `profile`, as `stleditor/paths.py` finds them with the environment
/// [`profile_env`] sets.
fn one_x_dirs(profile: &Path) -> (PathBuf, PathBuf) {
    let (roaming, local) = if cfg!(target_os = "macos") {
        let d = profile.join("Library").join("Application Support");
        (d.clone(), d)
    } else {
        (profile.join("AppData").join("Roaming"), profile.join("AppData").join("Local"))
    };
    (roaming.join("STL Editor"), local.join("STL Editor"))
}

/// The environment of a process that lives in `profile`.
fn profile_env(cmd: &mut Command, profile: &Path) {
    let roaming = profile.join("AppData").join("Roaming");
    let local = profile.join("AppData").join("Local");
    cmd.env("USERPROFILE", profile)
        .env("HOME", profile)
        .env("APPDATA", &roaming)
        .env("LOCALAPPDATA", &local)
        .env("XDG_CONFIG_HOME", &roaming)
        .env("XDG_DATA_HOME", &local)
        .env("XDG_STATE_HOME", profile.join("state"))
        // 2.0 must use its own default folders, not an override
        .env_remove("STLEDITOR2_CONFIG_DIR")
        .env_remove("STLEDITOR2_INSTALL_DIR")
        .env_remove("STLEDITOR2_STARTMENU_DIR")
        .env_remove("STLEDITOR2_UPDATE_TEST_HOST")
        // 1.x's own override points into the same 1.x folders: 2.0 must ignore it
        .env("STLEDITOR_CONFIG_DIR", one_x_dirs(profile).0);
}

type Snapshot = BTreeMap<PathBuf, (Option<Vec<u8>>, Option<SystemTime>)>;

/// Every file and folder below `root`: bytes (files) and modified time.
fn snapshot(root: &Path) -> Snapshot {
    fn walk(dir: &Path, root: &Path, out: &mut Snapshot) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            let rel = p.strip_prefix(root).unwrap().to_path_buf();
            let mtime = e.metadata().ok().and_then(|m| m.modified().ok());
            if p.is_dir() {
                out.insert(rel, (None, None));
                walk(&p, root, out);
            } else {
                out.insert(rel, (Some(std::fs::read(&p).unwrap()), mtime));
            }
        }
    }
    let mut out = Snapshot::new();
    walk(root, root, &mut out);
    out
}

/// 1.x's folders as a 1.x install leaves them: a settings file, recent projects, autosave copies.
fn make_1x(profile: &Path) -> (PathBuf, PathBuf) {
    let (config, data) = one_x_dirs(profile);
    let w = |p: PathBuf, body: &[u8]| {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    };
    w(
        config.join("settings.json"),
        br#"{"recent": ["C:\\parts\\old.stlx"], "last_dir": "C:\\parts", "update_repo": "natterman12/STL-Editor", "update_skip": "1.12.0", "update_check": true}"#,
    );
    w(config.join("support-fins").join("engine.js"), b"// 1.x's downloaded Support Fins engine\n");
    w(data.join("log.txt"), b"1.x started\n1.x closed normally\n");
    w(data.join("autosave").join("19700101-aaaa").join("document.json"), br#"{"title":"1.x work","count":3}"#);
    w(data.join("autosave").join("19700101-aaaa").join("sources").join("s1.stl"), b"solid 1.x\nendsolid\n");
    w(data.join("webview").join("Default").join("Local State"), &[0, 1, 2, 3, 255, 254]);
    (config, data)
}

#[test]
fn leaves_1x_alone() {
    let profile = std::env::temp_dir().join(format!("stle2-leaves-1x-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&profile);
    std::fs::create_dir_all(&profile).unwrap();
    let (config, data) = make_1x(&profile);
    // old enough that a rewrite shows in the modified time
    let before = (snapshot(&config), snapshot(&data));
    assert!(before.0.len() >= 3 && before.1.len() >= 6, "the fixtures are there");
    std::thread::sleep(Duration::from_millis(30));

    let mut child = Command::new(std::env::current_exe().unwrap());
    child.args(["--exact", "child_runs_every_platform_feature", "--nocapture", "--test-threads=1"]).env(PROFILE_VAR, &profile);
    profile_env(&mut child, &profile);
    let out = child.output().unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "the child run failed:\n{text}");
    assert!(text.contains("1 passed"), "the child test did not run:\n{text}");

    // 2.0 wrote to its own folders (so the child really ran in this profile) ...
    let roaming = config.parent().unwrap();
    assert!(roaming.join("STL Editor 2").join("settings.json").is_file(), "2.0's settings are in its own folder: {text}");
    // ... and 1.x's are exactly as they were
    let after = (snapshot(&config), snapshot(&data));
    assert_eq!(before.0.keys().collect::<Vec<_>>(), after.0.keys().collect::<Vec<_>>(), "no file or folder appeared or went in 1.x's settings folder");
    assert_eq!(before.1.keys().collect::<Vec<_>>(), after.1.keys().collect::<Vec<_>>(), "no file or folder appeared or went in 1.x's data folder");
    assert_eq!(before.0, after.0, "1.x's settings folder is byte for byte as it was");
    assert_eq!(before.1, after.1, "1.x's data folder is byte for byte as it was");
    let _ = std::fs::remove_dir_all(&profile);
}

fn poll<T>(p: &Pending<T>) -> T {
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(v) = p.take() {
            return v;
        }
        assert!(Instant::now() < end, "no answer");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// The work: runs only as the parent's child, in the temp profile.
#[test]
fn child_runs_every_platform_feature() {
    let Some(profile) = std::env::var_os(PROFILE_VAR).map(PathBuf::from) else { return };
    let (config_1x, data_1x) = one_x_dirs(&profile);
    // 2.0's defaults must be 2.0's own folders, below the temp profile
    let config = stl_platform::paths::config_dir();
    let data = stl_platform::paths::data_dir();
    assert!(config.starts_with(&profile) && data.starts_with(&profile), "2.0's folders are in the temp profile: {config:?} {data:?}");
    assert_eq!(config.file_name().unwrap(), "STL Editor 2");
    assert_ne!(config, config_1x);
    assert_ne!(data, data_1x);
    assert!(!data.starts_with(&data_1x) && !config.starts_with(&config_1x));

    // settings and recent files (2.0's `settings.json`, merged key by key)
    let platform = NativePlatform::new();
    let part = profile.join("part.stlx");
    std::fs::write(&part, b"PROJECT").unwrap();
    platform.set_setting_json("recent", json!([part.to_string_lossy()]));
    platform.set_setting_json("update_check", json!(true));
    platform.set_setting_json("update_skip", json!("2.0.9"));
    platform.set_setting("stle_sf_material", "PETG");
    assert_eq!(platform.setting_json("recent"), Some(json!([part.to_string_lossy()])));
    assert_eq!(poll(&platform.read_path(&part.to_string_lossy())).unwrap().bytes, b"PROJECT");
    assert!(poll(&platform.read_path(&profile.join("gone.stlx").to_string_lossy())).is_err());

    // autosave: write, list as recoverable, load, discard (the default root: data_dir()/autosave)
    let link = NativeLink::new();
    let bytes = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/projects/basic.stlx")).unwrap();
    link.send(Cmd::Open { name: "basic.stlx".into(), bytes });
    link.send(Cmd::AddFeature { kind: "primitive".into(), params: json!({"shape": "box", "size": [4.0, 4.0, 4.0]}), index: None, name: None });
    link.send(Cmd::Autosave { path: None, title: Some("two".into()) });
    let end = Instant::now() + Duration::from_secs(120);
    while link.is_busy() {
        assert!(Instant::now() < end, "the engine never went idle");
        std::thread::sleep(Duration::from_millis(5));
    }
    drop(link); // closing with unsaved work keeps the copy
    let root = data.join("autosave");
    let found = stl_platform::autosave::recoverable(&root, None);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(root.starts_with(&profile) && !root.starts_with(&data_1x));
    let id = found[0].id.clone();
    assert!(stl_platform::autosave::load(&root, &id).is_ok());
    // the Recover dialog's own store sees it, and loading discards it
    let store = DesktopStore { root: root.clone() };
    let list = poll(&store.list());
    assert_eq!(list.len(), 1);
    assert!(poll(&store.load(&id)).is_ok());
    // a second copy, discarded by the user
    let link = NativeLink::new();
    link.send(Cmd::Import { name: "x.stl".into(), bytes: b"solid x\nendsolid x\n".to_vec() });
    link.send(Cmd::Autosave { path: None, title: None });
    while link.is_busy() {
        std::thread::sleep(Duration::from_millis(5));
    }
    drop(link);
    for s in stl_platform::autosave::recoverable(&root, None) {
        stl_platform::autosave::discard(&root, &s.id);
    }

    // an update check against the fake server: a banner, Skip, and the cached answer in 2.0's settings
    let rel = json!([{ "tag_name": "v2.0.1", "draft": false, "body": "What changed",
        "assets": [{ "name": ASSET_NAME, "browser_download_url": "https://github.com/natterman12/STL-Editor/releases/download/v2.0.1/x", "size": 10 }] }]);
    let srv = FakeReleases::start(rel, vec![], Faults::default());
    let mut u = Updates::custom(&format!("{}/releases", srv.base_url()), "2.0.0", None, data.join("windows"));
    u.check_now(&platform, 1_800_000_000.0);
    let end = Instant::now() + Duration::from_secs(20);
    let mut actions: Vec<UpdateAction> = Vec::new();
    while u.is_busy() {
        assert!(Instant::now() < end, "the check never finished");
        actions.extend(u.poll(&platform, 1_800_000_000.0));
        std::thread::sleep(Duration::from_millis(10));
    }
    actions.extend(u.poll(&platform, 1_800_000_000.0));
    assert_eq!(u.banner().map(|b| b.title), Some("STL Editor 2.0.1 is available".to_string()), "{actions:?}");
    assert!(u.press(stl_app::updates::BannerButton::Skip, &platform).is_none());
    assert_eq!(platform.setting_json("update_skip"), Some(json!("2.0.1")));
}
