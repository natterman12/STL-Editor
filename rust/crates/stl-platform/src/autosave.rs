//! Autosave and Recover. Port of `stleditor/autosave.py`.
//!
//! Each running window owns one folder `<root>/<yyyymmdd-hhmmss-xxxxxx>` holding `document.json`
//! (1.x's autosave fields), the imported meshes under `sources/`, and a `lock` file that stays
//! locked (an operating-system file lock) while the window runs. A folder whose lock can be
//! taken belongs to a window that didn't close cleanly, so its work can be recovered.
//!
//! The caller decides what goes in `document.json` and when (the engine worker writes it, off the
//! UI thread); this module only keeps the folder.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

/// One folder that can be recovered, for the Recover Unsaved Work dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionInfo {
    pub id: String,
    pub title: String,
    pub path: Option<String>,
    /// Seconds since 1970, when the copy was written.
    pub saved: f64,
    pub steps: usize,
}

/// An imported mesh kept beside `document.json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub ext: String,
    pub data: Vec<u8>,
}

/// An empty folder younger than this is never removed: its window may not have taken the lock yet.
const FRESH_SECS: f64 = 30.0;

pub struct Autosaver {
    dir: PathBuf,
    id: String,
    /// Held for as long as the window runs. Dropping it (or the process ending) frees the lock.
    lock: Option<File>,
}

impl Autosaver {
    /// A new `<yyyymmdd-hhmmss-xxxxxx>` folder under `root`, lock taken.
    pub fn begin(root: &Path) -> std::io::Result<Autosaver> {
        std::fs::create_dir_all(root)?;
        let mut last = None;
        // Another window clearing stale folders can remove ours between creating it and locking
        // it (it takes the lock first, so we wait our turn); try again with a new name.
        for _ in 0..4 {
            let id = new_id();
            let dir = root.join(&id);
            let attempt = (|| {
                std::fs::create_dir(&dir)?;
                let lock = open_lock(&dir)?;
                lock.lock()?; // waits while a scan of the folder holds it for a moment
                Ok::<_, std::io::Error>(lock)
            })();
            match attempt {
                Ok(lock) => {
                    // The folder was removed under us after we opened the lock: start over.
                    if !dir.is_dir() {
                        continue;
                    }
                    return Ok(Autosaver { dir, id, lock: Some(lock) });
                }
                Err(e) => last = Some(e),
            }
        }
        Err(last.unwrap_or_else(|| std::io::Error::other("couldn't start an autosave folder")))
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// `document.json` (written to a `.tmp` and renamed) and the sources given. A source whose
    /// file is already there with the same size is not written again; sources not given are left
    /// alone, so the caller can pass only the new ones.
    pub fn write(&self, doc: &Value, sources: &[Source]) -> std::io::Result<()> {
        if !sources.is_empty() {
            let dir = self.dir.join("sources");
            std::fs::create_dir_all(&dir)?;
            for s in sources {
                let file = dir.join(format!("{}{}", s.id, s.ext));
                if std::fs::metadata(&file).is_ok_and(|m| m.len() == s.data.len() as u64) {
                    continue;
                }
                write_atomic(&file, &s.data)?;
            }
        }
        let text = serde_json::to_vec(doc).map_err(std::io::Error::other)?;
        write_atomic(&self.dir.join("document.json"), &text)
    }

    /// Saved or discarded: nothing to recover from this window any more.
    pub fn clear(&self) {
        let _ = std::fs::remove_file(self.dir.join("document.json"));
        let _ = std::fs::remove_file(self.dir.join("document.json.tmp"));
        let _ = std::fs::remove_dir_all(self.dir.join("sources"));
    }

    /// Clean exit: the folder is removed.
    pub fn end(mut self) {
        self.lock = None;
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Folders of windows that didn't close cleanly, oldest first. Skips `exclude`, folders another
/// running window holds the lock of, and folders that can't be read (logged). Empty stale folders
/// are removed.
pub fn recoverable(root: &Path, exclude: Option<&str>) -> Vec<SessionInfo> {
    let mut names: Vec<String> = match std::fs::read_dir(root) {
        Ok(rd) => rd.flatten().filter(|e| e.path().is_dir()).filter_map(|e| e.file_name().into_string().ok()).collect(),
        Err(_) => return Vec::new(),
    };
    names.sort();
    let mut out = Vec::new();
    for name in names {
        if exclude == Some(name.as_str()) {
            continue;
        }
        let dir = root.join(&name);
        // Keep the lock while we look, so a window starting up can't be mistaken for a stale one
        // and the folder can't be taken over meanwhile.
        let guard = match open_lock(&dir) {
            Ok(f) => match f.try_lock() {
                Ok(()) => Some(f),
                Err(TryLockError::WouldBlock) => continue, // another window is still running this session
                Err(TryLockError::Error(e)) => {
                    eprintln!("autosave: couldn't check {name}: {e}");
                    continue;
                }
            },
            Err(_) => None,
        };
        let doc_path = dir.join("document.json");
        if !doc_path.exists() {
            // only a folder whose name says it is old enough: a new one may belong to a window
            // that hasn't taken its lock yet
            if age_secs(&name).is_some_and(|a| a > FRESH_SECS) {
                drop(guard);
                let _ = std::fs::remove_dir_all(&dir); // stale empty session
            }
            continue;
        }
        match read_session(&dir, &name) {
            Ok(info) => out.push(info),
            Err(e) => eprintln!("autosave: skipping {name}: {e}"),
        }
    }
    out
}

/// The `document.json` content of a session and its sources, for rebuilding the project.
pub fn load(root: &Path, id: &str) -> Result<(Value, Vec<Source>), String> {
    let dir = session_dir(root, id).ok_or("That autosave copy is gone.")?;
    let doc = read_doc(&dir.join("document.json"))?;
    let sources = read_sources(&dir, &doc)?;
    Ok((doc, sources))
}

/// Remove a session's folder (if no window is running it).
pub fn discard(root: &Path, id: &str) {
    let Some(dir) = session_dir(root, id) else { return };
    if let Ok(f) = open_lock(&dir) {
        if matches!(f.try_lock(), Err(TryLockError::WouldBlock)) {
            return; // a running window's own session
        }
        drop(f);
    }
    let _ = std::fs::remove_dir_all(dir);
}

// ------------------------------------------------------------------------------ helpers

/// `root/id`, only for a plain folder name inside `root`.
fn session_dir(root: &Path, id: &str) -> Option<PathBuf> {
    let plain = !id.is_empty() && !id.contains(['/', '\\']) && id != "." && id != "..";
    let dir = root.join(id);
    (plain && dir.is_dir()).then_some(dir)
}

fn open_lock(dir: &Path) -> std::io::Result<File> {
    OpenOptions::new().read(true).write(true).create(true).truncate(false).open(dir.join("lock"))
}

fn write_atomic(file: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut tmp = file.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let mut f = File::create(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?; // on disk before the rename, so a power cut leaves the old file or the new, never half
    drop(f);
    std::fs::rename(&tmp, file)
}

fn read_doc(file: &Path) -> Result<Value, String> {
    let text = std::fs::read(file).map_err(|e| format!("can't read {}: {e}", file.display()))?;
    match serde_json::from_slice::<Value>(&text) {
        Ok(v @ Value::Object(_)) => Ok(v),
        Ok(_) => Err("document.json is not an object".into()),
        Err(e) => Err(format!("document.json is damaged: {e}")),
    }
}

/// Every source `document.json` lists, from `sources/`.
fn read_sources(dir: &Path, doc: &Value) -> Result<Vec<Source>, String> {
    let mut out = Vec::new();
    if let Some(Value::Object(meta)) = doc.get("sources") {
        for (id, m) in meta {
            let text = |k: &str| m.get(k).and_then(Value::as_str).map(str::to_string);
            let (Some(name), Some(ext)) = (text("name"), text("ext")) else {
                return Err(format!("source {id} has no name"));
            };
            let file = dir.join("sources").join(format!("{id}{ext}"));
            let data = std::fs::read(&file).map_err(|e| format!("source {id} is missing: {e}"))?;
            out.push(Source { id: id.clone(), name, ext, data });
        }
    }
    Ok(out)
}

fn read_session(dir: &Path, name: &str) -> Result<SessionInfo, String> {
    let doc = read_doc(&dir.join("document.json"))?;
    // a half-written copy (a source that didn't make it) can't be recovered: check without keeping the bytes
    if let Some(Value::Object(meta)) = doc.get("sources") {
        for (id, m) in meta {
            let ext = m.get("ext").and_then(Value::as_str).ok_or_else(|| format!("source {id} has no name"))?;
            let file = dir.join("sources").join(format!("{id}{ext}"));
            if !file.is_file() {
                return Err(format!("source {id} is missing"));
            }
        }
    }
    let features = doc.get("features").and_then(Value::as_array).ok_or("document.json has no steps")?;
    let steps = doc.get("count").and_then(Value::as_u64).map_or(features.len(), |n| n as usize);
    let title = doc.get("title").and_then(Value::as_str).filter(|t| !t.is_empty()).unwrap_or("Untitled").to_string();
    let path = doc.get("path").and_then(Value::as_str).map(str::to_string);
    let saved = doc.get("saved").and_then(Value::as_f64).unwrap_or(0.0);
    Ok(SessionInfo { id: name.to_string(), title, path, saved, steps })
}

/// Seconds since the time in a folder name (`yyyymmdd-hhmmss-xxxxxx`, UTC); `None` if it isn't one.
fn age_secs(name: &str) -> Option<f64> {
    let b = name.as_bytes();
    if b.len() < 15 || b[8] != b'-' || !b[..8].iter().chain(&b[9..15]).all(u8::is_ascii_digit) {
        return None;
    }
    let n = |r: std::ops::Range<usize>| name[r].parse::<i64>().ok();
    let days = days_from_civil(n(0..4)?, n(4..6)?, n(6..8)?);
    let t = days * 86400 + n(9..11)? * 3600 + n(11..13)? * 60 + n(13..15)?;
    Some(now_secs() - t as f64)
}

fn now_secs() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

fn new_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let secs = now_secs() as i64;
    let (y, m, d) = civil_from_days(secs.div_euclid(86400));
    let rem = secs.rem_euclid(86400);
    // six hex digits from the time, the process, a counter and the OS's per-process random keys
    use std::hash::{BuildHasher, Hasher, RandomState};
    let mut h = RandomState::new().build_hasher();
    h.write_u64(SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64));
    h.write_u32(std::process::id());
    h.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}-{:06x}", rem / 3600, rem % 3600 / 60, rem % 60, h.finish() & 0xff_ffff)
}

/// Days since 1970-01-01 for a civil date (proleptic Gregorian).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}
