//! The desktop platform: file dialogs (rfd) and the clock.
//!
//! Each dialog runs on its own short-lived thread, so the window keeps drawing while it is open;
//! the answer lands in a [`Pending`] that the UI polls.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{Pending, PickedFile, Platform, SaveOutcome};

pub struct NativePlatform {
    /// Called when a dialog has answered, to wake the UI (`Context::request_repaint`).
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
    settings: Settings,
}

/// The small remembered settings: read from the file once, written back whole through a temp file
/// and a rename, so a crash can't leave half a file behind.
struct Settings {
    path: Option<PathBuf>,
    cache: Mutex<Option<serde_json::Map<String, serde_json::Value>>>,
}

/// Makes every temp file name different (together with the process id).
static TMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl Settings {
    fn new(path: Option<PathBuf>) -> Self {
        Settings { path, cache: Mutex::new(None) }
    }

    fn with<R>(&self, f: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>) -> R) -> Option<R> {
        let path = self.path.as_ref()?;
        let mut g = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let map = g.get_or_insert_with(|| read_settings(path));
        Some(f(map))
    }

    fn get(&self, key: &str) -> Option<String> {
        self.with(|m| m.get(key).and_then(|v| v.as_str()).map(str::to_owned)).flatten()
    }

    /// Read, change, write and rename under one lock, so two writers can't put an older copy back.
    fn set(&self, key: &str, value: &str) {
        let Some(path) = self.path.clone() else { return };
        let mut g = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let map = g.get_or_insert_with(|| read_settings(&path));
        map.insert(key.to_owned(), serde_json::Value::String(value.to_owned()));
        let text = serde_json::Value::Object(map.clone()).to_string();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let n = TMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let tmp = path.with_extension(format!("json.{}.{n}.tmp", std::process::id()));
        if std::fs::write(&tmp, text).is_ok() && std::fs::rename(&tmp, &path).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
    }
}

/// The settings file as an object. A file that exists but can't be read as one is copied to
/// `.bak` before anything is written over it.
fn read_settings(path: &std::path::Path) -> serde_json::Map<String, serde_json::Value> {
    let Ok(bytes) = std::fs::read(path) else { return serde_json::Map::new() };
    match serde_json::from_slice::<serde_json::Value>(&bytes) {
        Ok(serde_json::Value::Object(m)) => m,
        _ => {
            let _ = std::fs::write(path.with_extension("json.bak"), &bytes);
            serde_json::Map::new()
        }
    }
}

impl NativePlatform {
    pub fn new() -> Self {
        NativePlatform { wake: None, settings: Settings::new(settings_file()) }
    }

    /// `f` runs on the dialog's thread after the answer is stored.
    pub fn with_waker(f: impl Fn() + Send + Sync + 'static) -> Self {
        NativePlatform { wake: Some(Arc::new(f)), settings: Settings::new(settings_file()) }
    }

    /// Keep the remembered settings in `path` (tests; the app uses the default place).
    pub fn with_settings_file(path: PathBuf) -> Self {
        NativePlatform { wake: None, settings: Settings::new(Some(path)) }
    }

    fn wake_handle(&self) -> Option<Arc<dyn Fn() + Send + Sync>> {
        self.wake.clone()
    }
}

impl Default for NativePlatform {
    fn default() -> Self {
        Self::new()
    }
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `YYYY-MM-DDTHH:MM:SS` in UTC, as the web platform returns it.
pub fn iso_from_unix(secs: i64) -> String {
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let t = secs.rem_euclid(86_400);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}", t / 3600, t % 3600 / 60, t % 60)
}

/// A project path always ends in `.stlx` (1.x adds it when the name was typed bare).
pub fn with_project_ext(path: std::path::PathBuf) -> std::path::PathBuf {
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("stlx")) {
        return path;
    }
    let mut s = path.into_os_string();
    s.push(".stlx");
    s.into()
}

fn write_file(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes)
        .map_err(|e| format!("Couldn't write {}: {e}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()))
}

impl Platform for NativePlatform {
    fn open_file(&self, filters: &[(&str, &[&str])]) -> Pending<Option<PickedFile>> {
        self.open_file_path(filters).map(|p| p.map(|(f, _)| f))
    }

    fn open_file_path(&self, filters: &[(&str, &[&str])]) -> Pending<Option<(PickedFile, Option<String>)>> {
        let pending = Pending::new();
        let out = pending.clone();
        let filters: Vec<(String, Vec<String>)> =
            filters.iter().map(|(n, e)| (n.to_string(), e.iter().map(|s| s.to_string()).collect())).collect();
        let wake = self.wake_handle();
        std::thread::spawn(move || {
            let mut dialog = rfd::FileDialog::new();
            for (name, exts) in &filters {
                dialog = dialog.add_filter(name.as_str(), exts);
            }
            let picked = dialog.pick_file().and_then(|path| {
                let bytes = std::fs::read(&path).ok()?;
                let name = path.file_name()?.to_string_lossy().into_owned();
                Some((PickedFile { name, bytes }, Some(path.to_string_lossy().into_owned())))
            });
            out.set(picked);
            if let Some(w) = wake {
                w();
            }
        });
        pending
    }

    fn save_file(&self, suggested: &str, bytes: Vec<u8>) -> Pending<Result<bool, String>> {
        self.save_file_path(suggested, bytes).map(|r| r.map(|o| o != SaveOutcome::Cancelled))
    }

    fn save_to(&self, path: &str, bytes: Vec<u8>) -> Pending<Result<bool, String>> {
        let pending = Pending::new();
        let out = pending.clone();
        let path = std::path::PathBuf::from(path);
        let wake = self.wake_handle();
        std::thread::spawn(move || {
            out.set(write_file(&path, &bytes).map(|_| true));
            if let Some(w) = wake {
                w();
            }
        });
        pending
    }

    fn save_file_path(&self, suggested: &str, bytes: Vec<u8>) -> Pending<Result<SaveOutcome, String>> {
        let pending = Pending::new();
        let out = pending.clone();
        let suggested = suggested.to_string();
        let wake = self.wake_handle();
        std::thread::spawn(move || {
            let mut dialog = rfd::FileDialog::new().set_file_name(suggested.as_str());
            if let Some(ext) = std::path::Path::new(&suggested).extension().and_then(|e| e.to_str()) {
                dialog = dialog.add_filter(ext.to_uppercase(), &[ext]);
            }
            let result = match dialog.save_file() {
                None => Ok(SaveOutcome::Cancelled),
                Some(path) => {
                    let path = if suggested.to_ascii_lowercase().ends_with(".stlx") { with_project_ext(path) } else { path };
                    write_file(&path, &bytes).map(|_| SaveOutcome::Done(Some(path.to_string_lossy().into_owned())))
                }
            };
            out.set(result);
            if let Some(w) = wake {
                w();
            }
        });
        pending
    }

    fn now_iso_utc(&self) -> String {
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        iso_from_unix(secs)
    }

    fn is_web(&self) -> bool {
        false
    }

    fn setting(&self, key: &str) -> Option<String> {
        self.settings.get(key)
    }

    fn set_setting(&self, key: &str, value: &str) {
        self.settings.set(key, value);
    }
}

/// Where the small remembered settings live (beside, not inside, 1.x's `settings.json`).
fn settings_file() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("APPDATA").or_else(|| std::env::var_os("XDG_CONFIG_HOME")).map(std::path::PathBuf::from).or_else(|| {
        std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
    })?;
    Some(base.join("STL Editor").join("ui-settings.json"))
}
