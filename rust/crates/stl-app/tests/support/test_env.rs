//! Keeps the tests out of Nathan's real 2.0 folders.
//!
//! The engine worker reads `paths::config_dir()` (downloaded Support Fins engines) and the app
//! platform writes `settings.json` there, so a test that starts either would touch
//! `%APPDATA%\STL Editor 2`. This points `STLEDITOR2_CONFIG_DIR` at a folder of its own, once per
//! test process, before `main` runs (so before any test thread or the engine thread starts).
//! A test that wants another folder sets the variable itself, as the settings tests do.
//!
//! Use from a test file with `#[path = "support/test_env.rs"] mod test_env;` (the gpu and
//! wgpu_renderer helpers already include it). Nothing has to be called.
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Once;

/// The folder this process's tests use for 2.0's settings (`<temp>/stle2-test-config-<pid>`).
pub fn config_dir() -> PathBuf {
    std::env::temp_dir().join(format!("stle2-test-config-{}", std::process::id()))
}

/// Points `STLEDITOR2_CONFIG_DIR` at [`config_dir`] unless a caller already chose a folder.
/// Runs by itself before `main`; calling it again is harmless.
pub fn isolate() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        if std::env::var_os("STLEDITOR2_CONFIG_DIR").is_none_or(|v| v.is_empty()) {
            // SAFETY: runs before main (see `INIT`), when no other thread exists; a later call
            // only gets here if that failed, and then it is `Once`-guarded.
            unsafe { std::env::set_var("STLEDITOR2_CONFIG_DIR", config_dir()) };
        }
    });
}

extern "C" fn init() {
    isolate();
}

/// A constructor the loader runs before `main` (the `.init_array` / `.CRT$XCU` /
/// `__mod_init_func` section of each platform), so no helper has to be called.
#[used]
#[cfg_attr(windows, unsafe(link_section = ".CRT$XCU"))]
#[cfg_attr(all(unix, not(target_vendor = "apple")), unsafe(link_section = ".init_array"))]
#[cfg_attr(target_vendor = "apple", unsafe(link_section = "__DATA,__mod_init_func"))]
static INIT: extern "C" fn() = init;
