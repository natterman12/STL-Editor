//! Where STL Editor 2 keeps its files. Port of `stleditor/paths.py`, with its own "STL Editor 2"
//! folders so 1.x's are never touched. Read at call time, so tests can change the environment.
//!
//! Windows: `%APPDATA%\STL Editor 2` (settings), `%LOCALAPPDATA%\STL Editor 2` (autosave, log).
//! macOS: `~/Library/Application Support/STL Editor 2` for both.
//! Linux: `$XDG_CONFIG_HOME` / `$XDG_DATA_HOME` (defaults `~/.config`, `~/.local/share`).
//!
//! `STLEDITOR2_CONFIG_DIR=<d>` makes [`config_dir`] `<d>` and [`data_dir`] `<d>/data`;
//! `STLEDITOR2_INSTALL_DIR=<d>` makes [`install_dir`] `<d>`.

use std::path::PathBuf;

pub const APP_DIR: &str = "STL Editor 2";

fn env(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn home() -> PathBuf {
    env(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .or_else(|| env("HOME"))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// An environment variable's folder, or `home/<fallback...>`.
fn var_or(name: &str, fallback: &[&str]) -> PathBuf {
    env(name).unwrap_or_else(|| fallback.iter().fold(home(), |p, s| p.join(s)))
}

/// Settings and downloaded engines.
pub fn config_dir() -> PathBuf {
    if let Some(d) = env("STLEDITOR2_CONFIG_DIR") {
        return d;
    }
    if cfg!(windows) {
        var_or("APPDATA", &["AppData", "Roaming"]).join(APP_DIR)
    } else if cfg!(target_os = "macos") {
        home().join("Library").join("Application Support").join(APP_DIR)
    } else {
        var_or("XDG_CONFIG_HOME", &[".config"]).join(APP_DIR)
    }
}

/// Autosave, the web view's data and the log.
pub fn data_dir() -> PathBuf {
    if let Some(d) = env("STLEDITOR2_CONFIG_DIR") {
        return d.join("data");
    }
    if cfg!(windows) {
        var_or("LOCALAPPDATA", &["AppData", "Local"]).join(APP_DIR)
    } else if cfg!(target_os = "macos") {
        home().join("Library").join("Application Support").join(APP_DIR)
    } else {
        var_or("XDG_DATA_HOME", &[".local", "share"]).join(APP_DIR)
    }
}

/// Where `xtask install` puts the exe.
pub fn install_dir() -> PathBuf {
    if let Some(d) = env("STLEDITOR2_INSTALL_DIR") {
        return d;
    }
    if cfg!(windows) {
        var_or("LOCALAPPDATA", &["AppData", "Local"]).join("Programs").join(APP_DIR)
    } else if cfg!(target_os = "macos") {
        home().join("Applications").join(APP_DIR)
    } else {
        var_or("XDG_DATA_HOME", &[".local", "share"]).join(APP_DIR)
    }
}
