use std::path::PathBuf;
use std::sync::Mutex;
use stl_platform::paths::*;

/// Environment variables are process-wide: tests that touch them take this.
static ENV: Mutex<()> = Mutex::new(());

fn no_1x(p: &PathBuf) {
    assert!(p.components().all(|c| c.as_os_str() != "STL Editor"), "{p:?} has a 1.x folder");
    assert!(p.components().any(|c| c.as_os_str() == "STL Editor 2"), "{p:?} lacks STL Editor 2");
}

#[test]
fn paths_use_stl_editor_2() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    unsafe { std::env::remove_var("STLEDITOR2_CONFIG_DIR") };
    unsafe { std::env::remove_var("STLEDITOR2_INSTALL_DIR") };
    assert_eq!(APP_DIR, "STL Editor 2");
    no_1x(&config_dir());
    no_1x(&data_dir());
    no_1x(&install_dir());
}

#[test]
fn override_env_vars() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let d = std::env::temp_dir().join("stle2-paths-test");
    unsafe { std::env::set_var("STLEDITOR2_CONFIG_DIR", &d) };
    unsafe { std::env::set_var("STLEDITOR2_INSTALL_DIR", d.join("inst")) };
    assert_eq!(config_dir(), d);
    assert_eq!(data_dir(), d.join("data"));
    assert_eq!(install_dir(), d.join("inst"));
    unsafe { std::env::remove_var("STLEDITOR2_CONFIG_DIR") };
    unsafe { std::env::remove_var("STLEDITOR2_INSTALL_DIR") };
}
