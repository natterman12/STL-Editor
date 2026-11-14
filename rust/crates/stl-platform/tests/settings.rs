//! `settings.json` (Task 2): every key survives a restart, two windows merge, a corrupt file is
//! kept aside, and the log and downloaded engines live in 2.0's own folders.
//! Every test that reads the environment sets `STLEDITOR2_CONFIG_DIR` to a temp folder.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde_json::{json, Value};
use stl_platform::paths;
use stl_platform::settings::{keys, log_file, Settings};

static ENV: Mutex<()> = Mutex::new(());

fn env_lock() -> MutexGuard<'static, ()> {
    ENV.lock().unwrap_or_else(|e| e.into_inner())
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("stle2-settings-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The folder's files, without `settings.lock` (the lock file stays beside the settings).
fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n != "settings.lock")
        .collect();
    v.sort();
    v
}

#[test]
fn fins_engines_and_log_in_own_folders() {
    let _g = env_lock();
    let d = temp("folders");
    // SAFETY: serialised by ENV; no other thread reads the environment in this test binary.
    unsafe { std::env::set_var("STLEDITOR2_CONFIG_DIR", &d) };
    assert_eq!(stl_platform::fins::engines_dir(), d.join("support-fins"));
    assert_eq!(log_file(), d.join("data").join("log.txt"));
    // the engine check writes its timestamp through the same merged settings.json
    Settings::open(&paths::config_dir()).set(keys::THEME, json!("dark")).unwrap();
    let files = names(&d);
    assert_eq!(files, vec!["settings.json".to_string()]);
    unsafe { std::env::remove_var("STLEDITOR2_CONFIG_DIR") };
    // and without the override: still 2.0's folders, never 1.x's
    for p in [stl_platform::fins::engines_dir(), log_file()] {
        assert!(p.components().any(|c| c.as_os_str() == "STL Editor 2"), "{}", p.display());
        assert!(!p.components().any(|c| c.as_os_str() == "STL Editor"), "{}", p.display());
    }
}

/// One value of the right type for every key in spec 2.2.
fn every_key() -> Vec<(&'static str, Value)> {
    vec![
        (keys::RECENT, json!(["C:\\a\\one.stlx", "C:\\a\\two.stl"])),
        (keys::LAST_DIR, json!("C:\\a")),
        (keys::THEME, json!("light")),
        (keys::UNITS, json!("in")),
        (keys::RIBBON_TAB, json!("print")),
        (keys::PANEL_SIZES, json!({"browser": 240.5, "timeline": 120})),
        (keys::SECTIONS, json!("{\"a\":true}")),
        (keys::SF_MATERIAL, json!("PETG")),
        (keys::SF_REACH, json!("3.5")),
        (keys::SF_DRAW_SHAPE, json!("rect")),
        (keys::UPDATE_CHECK, json!(false)),
        (keys::UPDATE_LAST, json!(1_700_000_000.5)),
        (keys::UPDATE_CACHE, json!({"tag": "v2.0.1", "size": 12})),
        (keys::UPDATE_SKIP, json!("v2.0.1")),
        (keys::SUPPORTFINS_UPDATE, json!(true)),
        (keys::SUPPORTFINS_LAST, json!(1_700_000_001)),
    ]
}

#[test]
fn round_trip_every_key() {
    let d = temp("roundtrip");
    let s = Settings::open(&d);
    for (k, v) in every_key() {
        assert_eq!(s.get(k), None, "{k} starts unset");
        s.set(k, v).unwrap();
    }
    let again = Settings::open(&d);
    for (k, v) in every_key() {
        assert_eq!(again.get(k), Some(v), "{k} survives a restart");
    }
    assert_eq!(names(&d), vec!["settings.json".to_string()], "no temp file is left");
}

#[test]
fn two_writers_merge() {
    let d = temp("merge");
    let a = Settings::open(&d);
    let b = Settings::open(&d);
    a.set("theme", json!("dark")).unwrap();
    b.set("units", json!("in")).unwrap(); // b never saw `theme`
    a.set("last_dir", json!("C:\\x")).unwrap(); // a never saw `units`
    for s in [&a, &b, &Settings::open(&d)] {
        assert_eq!(s.get("theme"), Some(json!("dark")));
        assert_eq!(s.get("units"), Some(json!("in")));
        assert_eq!(s.get("last_dir"), Some(json!("C:\\x")));
    }
}

#[test]
fn writers_on_many_threads_lose_nothing() {
    let d = temp("threads");
    let handles: Vec<_> = (0..8)
        .map(|i| {
            let d = d.clone();
            std::thread::spawn(move || {
                let s = Settings::open(&d);
                for j in 0..10 {
                    s.set(&format!("k{i}_{j}"), json!(j)).unwrap();
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let v: Value = serde_json::from_slice(&std::fs::read(d.join("settings.json")).unwrap()).unwrap();
    assert_eq!(v.as_object().unwrap().len(), 80);
    assert_eq!(names(&d), vec!["settings.json".to_string()]);
}

#[test]
fn corrupt_settings_kept_aside() {
    let d = temp("corrupt");
    let file = d.join("settings.json");
    std::fs::write(&file, "{ not json, but the user's").unwrap();
    let s = Settings::open(&d);
    // defaults, and the bad file is kept aside but not touched
    assert_eq!(s.get("theme"), None);
    assert_eq!(std::fs::read_to_string(d.join("settings.json.bad")).unwrap(), "{ not json, but the user's");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "{ not json, but the user's", "not overwritten by a read");
    // the next change writes a fresh file; the .bad copy stays
    s.set("theme", json!("dark")).unwrap();
    assert_eq!(Settings::open(&d).get("theme"), Some(json!("dark")));
    assert_eq!(std::fs::read_to_string(d.join("settings.json.bad")).unwrap(), "{ not json, but the user's");
    // an array is not a settings object either
    std::fs::write(&file, "[1,2]").unwrap();
    assert_eq!(s.get("theme"), None);
    assert_eq!(std::fs::read_to_string(d.join("settings.json.bad")).unwrap(), "[1,2]");
}

#[test]
fn first_bad_copy_is_kept_until_the_file_is_rewritten() {
    let d = temp("bad-first");
    let file = d.join("settings.json");
    std::fs::write(&file, "first {").unwrap();
    let s = Settings::open(&d);
    for _ in 0..3 {
        assert_eq!(s.get("a"), None);
    }
    assert_eq!(std::fs::read_to_string(d.join("settings.json.bad")).unwrap(), "first {");
    // a later read of the same bad file, even through a set that fails to change it, keeps it
    std::fs::write(d.join("settings.json.bad"), "kept copy").unwrap();
    assert_eq!(s.get("a"), None);
    assert_eq!(std::fs::read_to_string(d.join("settings.json.bad")).unwrap(), "kept copy", "not overwritten by a later read");
    // once settings.json has been replaced and goes bad again, the new bad file is kept
    s.set("a", json!(1)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));
    std::fs::write(&file, "second {").unwrap();
    assert_eq!(s.get("a"), None);
    assert_eq!(std::fs::read_to_string(d.join("settings.json.bad")).unwrap(), "second {");
}

/// Re-run by `two_processes_merge` with the folder and an id in the environment: writes many
/// keys of its own. Does nothing in a normal run.
#[test]
fn child_writer() {
    let (Some(dir), Some(id)) = (std::env::var_os("STLE2_SETTINGS_CHILD_DIR"), std::env::var_os("STLE2_SETTINGS_CHILD_ID")) else {
        return;
    };
    let s = Settings::open(Path::new(&dir));
    for j in 0..60 {
        s.set(&format!("p{}_{j}", id.to_string_lossy()), json!(j)).unwrap();
    }
}

#[test]
fn two_processes_merge() {
    let d = temp("processes");
    let exe = std::env::current_exe().unwrap();
    let children: Vec<_> = (0..3)
        .map(|id| {
            std::process::Command::new(&exe)
                .args(["child_writer", "--exact", "--nocapture", "--test-threads=1"])
                .env("STLE2_SETTINGS_CHILD_DIR", &d)
                .env("STLE2_SETTINGS_CHILD_ID", id.to_string())
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap()
        })
        .collect();
    for mut c in children {
        assert!(c.wait().unwrap().success(), "a writer process failed");
    }
    let s = Settings::open(&d);
    let v: Value = serde_json::from_slice(&std::fs::read(d.join("settings.json")).unwrap()).unwrap();
    assert_eq!(v.as_object().unwrap().len(), 180, "every key of every process survived");
    for id in 0..3 {
        for j in 0..60 {
            assert_eq!(s.get(&format!("p{id}_{j}")), Some(json!(j)));
        }
    }
    assert_eq!(names(&d), vec!["settings.json".to_string()]);
}

#[test]
fn empty_or_missing_file_is_just_defaults() {
    let d = temp("empty");
    let s = Settings::open(&d);
    assert_eq!(s.get("a"), None);
    std::fs::write(d.join("settings.json"), "").unwrap();
    assert_eq!(s.get("a"), None);
    assert!(!d.join("settings.json.bad").exists());
    s.set("a", json!(1)).unwrap();
    assert_eq!(s.get("a"), Some(json!(1)));
}
