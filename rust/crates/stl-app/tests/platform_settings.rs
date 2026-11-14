//! The desktop platform's remembered settings (`<config dir>/settings.json`, through
//! `stl_platform::settings`): merged key by key, written through a temp file and a rename, and
//! the reach read as JavaScript's `parseFloat` does.
#![cfg(not(target_arch = "wasm32"))]

#[path = "support/test_env.rs"]
mod test_env;

use stl_app::platform::native::NativePlatform;
use stl_app::platform::Platform;

fn dir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("stle-settings-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn settings_survive_a_restart_and_leave_no_temp_file() {
    let d = dir("restart");
    let p = NativePlatform::with_config_dir(&d);
    assert_eq!(p.setting("stle_sf_reach"), None);
    p.set_setting("stle_sf_reach", "3");
    p.set_setting("stle_sections", "{}");
    let names: Vec<_> = std::fs::read_dir(&d).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).filter(|n| n != "settings.lock").collect();
    assert_eq!(names, vec!["settings.json".to_string()], "the temp file was renamed away");
    let again = NativePlatform::with_config_dir(&d);
    assert_eq!(again.setting("stle_sf_reach").as_deref(), Some("3"));
    assert_eq!(again.setting("stle_sections").as_deref(), Some("{}"));
}

#[test]
fn a_broken_file_reads_as_empty_and_every_write_merges_with_the_file() {
    let d = dir("cache");
    let file = d.join("settings.json");
    std::fs::write(&file, "{ not json").unwrap();
    let p = NativePlatform::with_config_dir(&d);
    assert_eq!(p.setting("a"), None, "a broken file reads as empty");
    p.set_setting("a", "1");
    // another window adds a key behind this one's back: the next write keeps it
    let mut v: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    v["other"] = serde_json::json!("x");
    std::fs::write(&file, v.to_string()).unwrap();
    p.set_setting("b", "2");
    assert_eq!(p.setting("other").as_deref(), Some("x"));
    assert_eq!(p.setting("a").as_deref(), Some("1"));
    assert_eq!(p.setting("b").as_deref(), Some("2"));
}

#[test]
fn json_values_and_strings_share_the_file() {
    let d = dir("json");
    let p = NativePlatform::with_config_dir(&d);
    p.set_setting_json("recent", serde_json::json!(["a", "b"]));
    p.set_setting("name", "text");
    assert_eq!(p.setting_json("recent"), Some(serde_json::json!(["a", "b"])));
    assert_eq!(p.setting_json("name"), Some(serde_json::json!("text")), "a string setting is a JSON string");
    assert_eq!(p.setting("recent"), None, "not a string");
}

#[test]
fn reach_reads_like_parse_float() {
    use stl_app::supports::panel::parse_float_js;
    assert_eq!(parse_float_js("3mm"), Some(3.0));
    assert_eq!(parse_float_js(" 4.5 "), Some(4.5));
    assert_eq!(parse_float_js("abc"), None);
}

#[test]
fn an_unreadable_file_is_kept_as_a_bad_copy_and_replaced_by_the_next_change() {
    let d = dir("bak");
    let file = d.join("settings.json");
    std::fs::write(&file, "{ not json, but the user's").unwrap();
    let p = NativePlatform::with_config_dir(&d);
    p.set_setting("a", "1");
    assert_eq!(std::fs::read_to_string(d.join("settings.json.bad")).unwrap(), "{ not json, but the user's");
    assert!(std::fs::read_to_string(&file).unwrap().contains("\"a\""));
}

#[test]
fn writers_on_many_threads_lose_nothing() {
    let d = dir("threads");
    let file = d.join("settings.json");
    let p = std::sync::Arc::new(NativePlatform::with_config_dir(&d));
    let handles: Vec<_> = (0..16)
        .map(|i| {
            let p = p.clone();
            std::thread::spawn(move || {
                for j in 0..10 {
                    p.set_setting(&format!("k{i}_{j}"), "v");
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(v.as_object().unwrap().len(), 160, "every write is in the file");
    let leftovers = std::fs::read_dir(&d).unwrap().filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().ends_with(".tmp")).count();
    assert_eq!(leftovers, 0, "no temp files left");
}

#[test]
fn the_test_process_never_uses_the_real_config_folder() {
    // support/test_env.rs points STLEDITOR2_CONFIG_DIR at a temp folder before main
    let dir = std::env::var_os("STLEDITOR2_CONFIG_DIR").expect("set before main");
    assert_eq!(std::path::PathBuf::from(dir), test_env::config_dir());
    assert!(stl_platform::paths::config_dir().starts_with(std::env::temp_dir()));
}
