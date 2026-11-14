//! `settings.json`: small remembered values, merged key by key so two windows don't clobber
//! each other. Port of `stleditor/settings.py` (one JSON file, written to a `.tmp` and renamed).
//!
//! * Every write re-reads the file, changes only its own key and writes the whole thing back, so
//!   a second window's keys survive. The read-merge-write holds a process lock and an
//!   operating-system lock on `settings.lock` (beside the file), so two windows (processes)
//!   never interleave. If the lock file can't be made or locked the write goes ahead without it.
//! * A file that can't be read as a JSON object gives the defaults, is copied to
//!   `settings.json.bad`, and is left alone until the next change (which writes a fresh file).
//!   The first copy is kept: later reads of the same bad file don't touch it, and it is only
//!   replaced when settings.json has been rewritten (and gone bad again) since.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde_json::{Map, Value};

/// The keys 2.0 remembers (1.x `settings.py` names where 1.x had one).
pub mod keys {
    /// Recent files: an array of paths, newest first, at most 10 (1.x `recent`).
    pub const RECENT: &str = "recent";
    /// The folder the open and save dialogs start in (1.x `last_dir`).
    pub const LAST_DIR: &str = "last_dir";
    pub const THEME: &str = "theme";
    pub const UNITS: &str = "units";
    pub const RIBBON_TAB: &str = "ribbon_tab";
    pub const PANEL_SIZES: &str = "panel_sizes";
    pub const SECTIONS: &str = "stle_sections";
    pub const SF_MATERIAL: &str = "stle_sf_material";
    pub const SF_REACH: &str = "stle_sf_reach";
    pub const SF_DRAW_SHAPE: &str = "stle_draw_shape";
    pub const UPDATE_CHECK: &str = "update_check";
    pub const UPDATE_LAST: &str = "update_last";
    pub const UPDATE_CACHE: &str = "update_cache";
    pub const UPDATE_SKIP: &str = "update_skip";
    pub const SUPPORTFINS_UPDATE: &str = "supportfins_update";
    pub const SUPPORTFINS_LAST: &str = "supportfins_last";
}

/// One process-wide lock for the read-merge-write, shared by every [`Settings`].
static WRITE_LOCK: Mutex<()> = Mutex::new(());
/// Makes every temp file name different (together with the process id).
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub struct Settings {
    path: PathBuf,
}

impl Settings {
    /// `settings.json` in `dir`.
    pub fn open(dir: &Path) -> Settings {
        Settings { path: dir.join("settings.json") }
    }

    /// The file this reads and writes.
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn get(&self, key: &str) -> Option<Value> {
        // no lock needed: the file is only ever replaced by a rename
        self.read().ok()?.remove(key)
    }

    /// The operating-system lock that keeps two windows' writes apart; held until dropped.
    /// `None` when the lock file can't be made or locked (then the process lock still holds).
    fn lock_file(&self) -> Option<std::fs::File> {
        let dir = self.path.parent()?;
        std::fs::create_dir_all(dir).ok()?;
        let f = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(dir.join("settings.lock")).ok()?;
        f.lock().ok()?; // waits for the other window's write, which takes microseconds
        Some(f)
    }

    /// Re-read, merge this key, write `.tmp` and rename.
    pub fn set(&self, key: &str, value: Value) -> std::io::Result<()> {
        let _g = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _file_lock = self.lock_file();
        let mut map = self.read()?;
        map.insert(key.to_owned(), value);
        self.write(&map)
    }

    /// The file as an object. A missing or empty file is `{}`; one that isn't a JSON object is
    /// copied to `.bad` and also reads as `{}`. A file that can't be read at all (another window
    /// is renaming over it, no permission) is an error, so a write never replaces good settings
    /// with defaults.
    fn read(&self) -> std::io::Result<Map<String, Value>> {
        let mut last = None;
        for attempt in 0..4 {
            match std::fs::read(&self.path) {
                Ok(bytes) => return Ok(self.parse(&bytes)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
                Err(e) => {
                    last = Some(e);
                    if attempt < 3 {
                        std::thread::sleep(std::time::Duration::from_millis(5 * (attempt + 1)));
                    }
                }
            }
        }
        Err(last.expect("the loop ran"))
    }

    fn parse(&self, bytes: &[u8]) -> Map<String, Value> {
        if bytes.iter().all(u8::is_ascii_whitespace) {
            return Map::new();
        }
        match serde_json::from_slice::<Value>(bytes) {
            Ok(Value::Object(m)) => m,
            _ => {
                self.keep_aside(bytes);
                Map::new()
            }
        }
    }

    /// Copy an unreadable settings.json to `settings.json.bad`, unless a copy of it (or of an
    /// earlier bad file that nothing has replaced since) is already there.
    fn keep_aside(&self, bytes: &[u8]) {
        let mut bad = self.path.clone().into_os_string();
        bad.push(".bad");
        let bad = PathBuf::from(bad);
        if let Ok(old) = std::fs::metadata(&bad) {
            let rewritten = std::fs::metadata(&self.path)
                .and_then(|m| m.modified())
                .and_then(|new| old.modified().map(|o| new > o))
                .unwrap_or(false);
            if !rewritten {
                return;
            }
        }
        let _ = std::fs::write(&bad, bytes);
    }

    fn write(&self, map: &Map<String, Value>) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(map).map_err(std::io::Error::other)?;
        let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut tmp = self.path.clone().into_os_string();
        tmp.push(format!(".{}.{n}.tmp", std::process::id()));
        let tmp = PathBuf::from(tmp);
        std::fs::write(&tmp, text)
            .and_then(|()| std::fs::rename(&tmp, &self.path))
            .inspect_err(|_| {
                let _ = std::fs::remove_file(&tmp);
            })
    }
}

/// The log file (1.x `log.txt`): `data_dir()/log.txt`.
pub fn log_file() -> PathBuf {
    crate::paths::data_dir().join("log.txt")
}
