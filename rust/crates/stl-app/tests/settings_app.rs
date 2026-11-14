//! Settings and recent files in the app (milestone 5, task 2): Recent lasts across restarts and
//! reopens from disk, Save writes back without asking, a missing file is dropped with 1.x's
//! message, `last_dir` feeds the dialogs, and nothing is written into 1.x's folders.
//!
//! Every test uses a temp folder for settings (`NativePlatform::with_config_dir`, or
//! `STLEDITOR2_CONFIG_DIR` / `APPDATA` pointed at a temp folder, serialised by `ENV`).
#![cfg(not(target_arch = "wasm32"))]

// keeps this test binary out of the real 2.0 folders (STLEDITOR2_CONFIG_DIR)
#[path = "support/test_env.rs"]
mod test_env;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use stl_app::engine_link::{Command, EngineLink, Level, Update};
use stl_app::platform::native::NativePlatform;
use stl_app::platform::Platform;
use stl_app::ui::files::{FileAction, Files};

static ENV: Mutex<()> = Mutex::new(());

fn env_lock() -> MutexGuard<'static, ()> {
    ENV.lock().unwrap_or_else(|e| e.into_inner())
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("stle2-app-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Remembers every command; never answers.
#[derive(Default)]
struct RecLink {
    sent: Mutex<Vec<Command>>,
}

impl EngineLink for RecLink {
    fn send(&self, c: Command) {
        self.sent.lock().unwrap().push(c);
    }
    fn poll(&self) -> Vec<Update> {
        Vec::new()
    }
    fn restart(&self) {}
}

fn accepted() -> Vec<Update> {
    vec![Update::Busy(true), Update::Bodies(Vec::new()), Update::Busy(false)]
}

/// Polls until the file reads (a thread answers) have come back; returns their messages.
fn poll_until_idle(files: &mut Files, platform: &dyn Platform, link: &RecLink, updates: &[Update]) -> Vec<(Level, String)> {
    let end = Instant::now() + Duration::from_secs(10);
    let mut messages = Vec::new();
    let mut first = true;
    loop {
        messages.extend(files.poll(platform, link, if first { updates } else { &[] }).messages);
        first = false;
        if !files.busy() {
            return messages;
        }
        assert!(Instant::now() < end, "the read never answered");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn opened(link: &RecLink) -> Vec<(String, Vec<u8>)> {
    link.sent
        .lock()
        .unwrap()
        .iter()
        .filter_map(|c| if let Command::Open { name, bytes } = c { Some((name.clone(), bytes.clone())) } else { None })
        .collect()
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

#[test]
fn open_recent_reads_from_disk() {
    let d = temp("recent-disk");
    let cfg = d.join("cfg");
    let file = d.join("part.stlx");
    std::fs::write(&file, b"PROJECT BYTES").unwrap();
    let key = path_str(&file);

    // an earlier run left the file in Recent
    let platform = NativePlatform::with_config_dir(&cfg);
    platform.set_setting_json("recent", serde_json::json!([key]));
    let (link, mut files) = (RecLink::default(), Files::new());
    files.load_recent(&platform);
    assert_eq!(files.recent_keys(), vec![key.clone()]);
    assert_eq!(files.recent_names(), vec!["part.stlx".to_string()]);

    // the bytes come from the disk, not from memory
    assert!(files.open_recent(&platform, &link, &key));
    let messages = poll_until_idle(&mut files, &platform, &link, &[]);
    assert!(messages.is_empty(), "{messages:?}");
    assert_eq!(opened(&link), vec![("part.stlx".to_string(), b"PROJECT BYTES".to_vec())]);

    // the engine accepts it: still first in Recent, still stored by path
    files.poll(&platform, &link, &accepted());
    assert_eq!(files.title.as_deref(), Some("part.stlx"));
    assert_eq!(platform.setting_json("recent"), Some(serde_json::json!([key])));
}

#[test]
fn save_after_open_recent_writes_back() {
    let d = temp("recent-save");
    let file = d.join("part.stlx");
    std::fs::write(&file, b"OLD").unwrap();
    let key = path_str(&file);

    let platform = NativePlatform::with_config_dir(&d.join("cfg"));
    platform.set_setting_json("recent", serde_json::json!([key]));
    let (link, mut files) = (RecLink::default(), Files::new());
    files.load_recent(&platform);
    assert!(files.open_recent(&platform, &link, &key));
    poll_until_idle(&mut files, &platform, &link, &[]);
    files.poll(&platform, &link, &accepted());

    // Save goes back to the same path: no dialog (a dialog would hang the test)
    files.start(&platform, &link, FileAction::Save, None);
    assert!(link.sent.lock().unwrap().iter().any(|c| matches!(c, Command::Save)));
    let messages = poll_until_idle(&mut files, &platform, &link, &[Update::Saved { bytes: b"NEW".to_vec() }]);
    assert_eq!(std::fs::read(&file).unwrap(), b"NEW");
    assert!(messages.iter().any(|(l, m)| *l == Level::Success && m.starts_with("Saved")), "{messages:?}");
    assert_eq!(files.recent_keys(), vec![key], "the saved project stays under its path");
}

#[test]
fn missing_recent_dropped() {
    let d = temp("recent-missing");
    let gone = d.join("gone.stlx");
    let kept = d.join("kept.stlx");
    std::fs::write(&kept, b"x").unwrap();
    let (gone_s, kept_s) = (path_str(&gone), path_str(&kept));

    let platform = NativePlatform::with_config_dir(&d.join("cfg"));
    platform.set_setting_json("recent", serde_json::json!([gone_s, kept_s]));
    let (link, mut files) = (RecLink::default(), Files::new());
    files.load_recent(&platform);
    assert!(files.open_recent(&platform, &link, &gone_s));
    let messages = poll_until_idle(&mut files, &platform, &link, &[]);

    // plain words with the path (Nathan, 2026-11-14; 1.x showed Python's FileNotFoundError)
    let want = format!("That file isn\u{2019}t there any more ({gone_s}). It was taken off the Recent list.");
    assert_eq!(messages, vec![(Level::Error, want)]);
    assert!(opened(&link).is_empty(), "nothing was opened");
    // dropped from the list, in memory and in settings.json; the other file stays
    assert_eq!(files.recent_keys(), vec![kept_s.clone()]);
    assert_eq!(platform.setting_json("recent"), Some(serde_json::json!([kept_s])));
}

#[test]
fn recent_keeps_ten_and_lasts_a_restart() {
    let d = temp("recent-ten");
    let platform = NativePlatform::with_config_dir(&d);
    let (link, mut files) = (RecLink::default(), Files::new());
    for i in 0..12 {
        let p = d.join(format!("f{i}.stlx"));
        std::fs::write(&p, b"x").unwrap();
        files.open_bytes(
            &link,
            stl_app::platform::PickedFile { name: format!("f{i}.stlx"), bytes: b"x".to_vec() },
            Some(path_str(&p)),
        );
        files.poll(&platform, &link, &accepted());
    }
    let keys = files.recent_keys();
    assert_eq!(keys.len(), 10, "1.x keeps ten");
    assert!(keys[0].ends_with("f11.stlx") && keys[9].ends_with("f2.stlx"), "{keys:?}");
    // a new run reads them back
    let mut again = Files::new();
    again.load_recent(&NativePlatform::with_config_dir(&d));
    assert_eq!(again.recent_keys(), keys);
}

#[test]
fn last_dir_used_by_dialogs() {
    let d = temp("last-dir");
    let cfg = d.join("cfg");
    let folder = d.join("models");
    std::fs::create_dir_all(&folder).unwrap();

    let platform = NativePlatform::with_config_dir(&cfg);
    assert_eq!(platform.dialog_dir(), None, "nothing remembered yet");
    // what the open and save dialogs do after a pick
    platform.remember_dir(&folder.join("a.stl"));
    assert_eq!(platform.setting("last_dir"), Some(path_str(&folder)));
    assert_eq!(platform.dialog_dir(), Some(folder.clone()));
    // it lasts a restart
    assert_eq!(NativePlatform::with_config_dir(&cfg).dialog_dir(), Some(folder.clone()));
    // a folder that has gone is not offered
    std::fs::remove_dir_all(&folder).unwrap();
    assert_eq!(platform.dialog_dir(), None);
}

#[test]
fn no_ui_settings_in_1x_folder() {
    let _g = env_lock();
    let d = temp("no-1x");
    let vars = ["STLEDITOR2_CONFIG_DIR", "STLEDITOR_CONFIG_DIR", "APPDATA", "LOCALAPPDATA", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "HOME", "USERPROFILE"];
    let saved: Vec<_> = vars.iter().map(|v| std::env::var_os(v)).collect();
    // SAFETY: serialised by ENV; this test binary's other tests don't read these variables.
    unsafe {
        std::env::remove_var("STLEDITOR2_CONFIG_DIR");
        std::env::remove_var("STLEDITOR_CONFIG_DIR");
        for v in &vars[2..] {
            std::env::set_var(v, d.join(v));
        }
    }
    let platform = NativePlatform::new();
    platform.set_setting("stle_sf_material", "PETG");
    platform.set_setting_json("recent", serde_json::json!(["x"]));
    platform.remember_dir(&d.join("a.stl"));
    assert_eq!(NativePlatform::new().setting("stle_sf_material").as_deref(), Some("PETG"));

    let mut found = Vec::new();
    walk(&d, &mut found);
    unsafe {
        for (v, old) in vars.iter().zip(saved) {
            match old {
                Some(o) => std::env::set_var(v, o),
                None => std::env::remove_var(v),
            }
        }
    }
    assert!(!found.iter().any(|p| p.components().any(|c| c.as_os_str() == "STL Editor")), "1.x's folder was touched: {found:?}");
    assert!(!found.iter().any(|p| p.file_name().is_some_and(|n| n == "ui-settings.json")), "{found:?}");
    assert!(
        found.iter().any(|p| p.file_name().is_some_and(|n| n == "settings.json") && p.components().any(|c| c.as_os_str() == "STL Editor 2")),
        "settings.json is in 2.0's folder: {found:?}"
    );
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        out.push(p.clone());
        if p.is_dir() {
            walk(&p, out);
        }
    }
}
