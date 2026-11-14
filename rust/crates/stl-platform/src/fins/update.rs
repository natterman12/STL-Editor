//! Support Fins engine updates, matching `stleditor/supportfins.py`.
//!
//! There is no settings type in this crate yet. [`Settings`] is the smallest one
//! `check_update` needs: the two on/off switches, the last-check time, the repository
//! (or a test server URL), the bundled engine folder and the config folder.
//!
//! `check_update` does the download on a background thread and returns at once, so
//! starting the app does not wait on the network. [`finish_update_check`] waits for
//! that thread; the app does not call it at startup. A newer engine is written under
//! `<config dir>/support-fins/` and is picked up only by the next [`choose_engine`].

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::net::{NetError, https_get};

/// 1.x `API`: the integration contract this app understands.
const API: i64 = 1;
/// 1.x checks again after 20 hours (`20 * 3600`), which is what "once a day" means there.
const DAY_SECS: i64 = 20 * 3600;
const DEFAULT_REPO: &str = "natterman12/STL-Editor";

/// 1.x `STATE`: `pending` is a newer engine downloaded this session, used from the next start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FinsState {
    pub pending: Option<String>,
    pub error: Option<String>,
    pub checked: Option<i64>,
}

impl FinsState {
    const EMPTY: Self = Self { pending: None, error: None, checked: None };
}

/// One valid engine folder (`_read_info`).
#[derive(Clone, Debug)]
pub struct EngineInfo {
    pub version: Option<String>,
    pub tag: String,
    pub commit: Option<String>,
    pub source: Option<String>,
    pub path: PathBuf,
    pub folder: PathBuf,
}

/// What [`check_update`] reads. There was no `Settings` type in the crate; this holds
/// only the update-check fields (1.x `settings.get` keys of the same names).
#[derive(Clone, Debug)]
pub struct Settings {
    /// 1.x `update_check`, default on.
    pub update_check: bool,
    /// 1.x `supportfins_update`, default on.
    pub supportfins_update: bool,
    /// 1.x `supportfins_last`, unix seconds. A later value in `<config_dir>/settings.json` wins.
    pub supportfins_last: i64,
    /// `owner/name`, or a full `http://` / `https://` base (the tests point this at
    /// `fins_update_server.py`). `None` is [`DEFAULT_REPO`].
    pub repo: Option<String>,
    /// Shipped engine folder (`VERSION.json` + `engine.iife.js`).
    pub bundled: PathBuf,
    /// App config folder. The download goes in `<config_dir>/support-fins/`.
    pub config_dir: PathBuf,
}

impl Settings {
    pub fn new(bundled: impl Into<PathBuf>, config_dir: impl Into<PathBuf>) -> Self {
        Self {
            update_check: true,
            supportfins_update: true,
            supportfins_last: 0,
            repo: None,
            bundled: bundled.into(),
            config_dir: config_dir.into(),
        }
    }
}

struct Gate {
    state: FinsState,
    inflight: Option<Receiver<()>>,
}

static GATE: Mutex<Gate> = Mutex::new(Gate { state: FinsState::EMPTY, inflight: None });

fn lock() -> MutexGuard<'static, Gate> {
    GATE.lock().unwrap_or_else(|e| e.into_inner())
}

fn snapshot() -> FinsState {
    lock().state.clone()
}

pub(crate) fn pending() -> Option<String> {
    lock().state.pending.clone()
}

/// Drop any in-memory update state. Tests call this between cases.
#[doc(hidden)]
pub fn reset_fins_state() {
    let rx = {
        let mut g = lock();
        g.inflight.take()
    };
    if let Some(rx) = rx {
        let _ = rx.recv();
    }
    lock().state = FinsState::EMPTY;
}

/// Where the web edition publishes its Support Fins engine (1.x `published_url`).
/// An empty string means "no publish URL", which 1.x returns as `None`.
/// A `repo` that is already an `http://` or `https://` URL is returned as the base,
/// so tests can point the check at a local server.
pub fn published_url(repo: Option<&str>) -> String {
    let r = repo.unwrap_or(DEFAULT_REPO).trim().trim_matches('/');
    let scheme = r.to_ascii_lowercase();
    if scheme.starts_with("https://") || scheme.starts_with("http://") {
        let mut u = r.to_string();
        if !u.ends_with('/') {
            u.push('/');
        }
        return u;
    }
    let Some((owner, name)) = r.split_once('/') else {
        return String::new();
    };
    if owner.is_empty() || name.is_empty() {
        return String::new();
    }
    format!("https://{}.github.io/{name}/app/vendor/support-fins/", owner.to_ascii_lowercase())
}

/// SHA-256 of a bundle, ignoring line endings (1.x `_digest`, copied: `\r\n` becomes `\n`).
pub fn digest(data: &[u8]) -> String {
    let mut n = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if data[i] == b'\r' && i + 1 < data.len() && data[i + 1] == b'\n' {
            n.push(b'\n');
            i += 2;
        } else {
            n.push(data[i]);
            i += 1;
        }
    }
    to_hex(&sha256(&n))
}

/// The newest valid engine in `bundled` or `user_dir` (1.x `_choose`).
/// Neither valid is the 1.x error "The Support Fins engine is missing from this installation."
pub fn choose_engine(bundled: &Path, user_dir: &Path) -> Result<EngineInfo, String> {
    let mut found = Vec::new();
    if let Some(i) = read_info(bundled) {
        found.push(i);
    }
    if let Some(i) = read_info(user_dir) {
        found.push(i);
    }
    found.into_iter().reduce(|best, i| if semver(&i.tag) > semver(&best.tag) { i } else { best }).ok_or_else(|| {
        "The Support Fins engine is missing from this installation.".to_string()
    })
}

/// Start an update check if one is due. Returns the current [`FinsState`] without waiting:
/// the download runs on a background thread and never blocks startup. `force` ignores the
/// once-a-day limit and the settings switches, as 1.x `check_update(force=True)` does.
pub fn check_update(settings: &Settings, force: bool) -> FinsState {
    if !should_check(settings, force) {
        return snapshot();
    }
    let mut g = lock();
    reap(&mut g);
    if g.inflight.is_some() {
        return g.state.clone();
    }
    let settings = settings.clone();
    let (tx, rx) = channel();
    let spawned = std::thread::Builder::new()
        .name("support-fins-update".into())
        .spawn(move || {
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_check(&settings))).is_err() {
                set_error("the update check failed");
            }
            let _ = tx.send(());
        })
        .is_ok();
    if spawned {
        g.inflight = Some(rx);
    }
    g.state.clone()
}

/// Wait for the check started by [`check_update`]. Startup does not call this.
pub fn finish_update_check() -> FinsState {
    let rx = lock().inflight.take();
    if let Some(rx) = rx {
        let _ = rx.recv();
    }
    snapshot()
}

fn should_check(settings: &Settings, force: bool) -> bool {
    if published_url(settings.repo.as_deref()).is_empty() {
        return false;
    }
    if force {
        return true;
    }
    if !(settings.update_check && settings.supportfins_update) {
        return false;
    }
    stored_last(settings) <= now_secs() - DAY_SECS
}

fn reap(g: &mut Gate) {
    let done = match g.inflight.as_ref() {
        Some(rx) => matches!(rx.try_recv(), Ok(()) | Err(TryRecvError::Disconnected)),
        None => false,
    };
    if done {
        g.inflight = None;
    }
}

fn run_check(settings: &Settings) {
    let base = published_url(settings.repo.as_deref());
    if base.is_empty() {
        return;
    }
    let user_dir = settings.config_dir.join("support-fins");
    let body = match https_get(&format!("{base}VERSION.json"), Duration::from_secs(15)) {
        Ok(b) => b,
        Err(e) => {
            // A failed check is recorded and retried the next day (spec section 2).
            fail(settings, &format!("URLError: {e}"));
            return;
        }
    };
    let info: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            fail(settings, &format!("ValueError: {e}"));
            return;
        }
    };
    // 1.x sets supportfins_last once VERSION.json has been read, before the api check.
    mark_checked(settings);
    if info.get("api").and_then(Value::as_i64) != Some(API) {
        set_error("ValueError: different api");
        return;
    }
    let remote = semver(info.get("tag").and_then(Value::as_str).unwrap_or(""));
    let have = [&settings.bundled, user_dir.as_path()].into_iter().filter_map(read_info).reduce(|a, b| {
        if semver(&b.tag) > semver(&a.tag) { b } else { a }
    });
    if have.is_some_and(|h| remote <= semver(&h.tag)) {
        return;
    }
    let engine = match https_get(&format!("{base}engine.iife.js"), Duration::from_secs(60)) {
        Ok(b) => b,
        Err(e) => {
            set_error(&net_error("URLError", &e));
            return;
        }
    };
    let want = info.get("sha256").and_then(|s| s.get("engine.iife.js")).and_then(Value::as_str).unwrap_or("");
    if digest(&engine) != want {
        set_error("ValueError: checksum mismatch");
        return;
    }
    if let Err(e) = install(&user_dir, &engine, &body) {
        set_error(&format!("OSError: {e}"));
        return;
    }
    let tag = info.get("tag").and_then(Value::as_str).map(str::to_string);
    let mut g = lock();
    g.state.pending = tag;
    g.state.error = None;
    g.state.checked = Some(now_secs());
}

fn net_error(kind: &str, e: &NetError) -> String {
    format!("{kind}: {e}")
}

fn fail(settings: &Settings, msg: &str) {
    mark_checked(settings);
    set_error(msg);
}

fn mark_checked(settings: &Settings) {
    let t = now_secs();
    write_last(&settings.config_dir, t);
    lock().state.checked = Some(t);
}

fn set_error(msg: &str) {
    lock().state.error = Some(msg.to_string());
}

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn stored_last(settings: &Settings) -> i64 {
    settings.supportfins_last.max(read_last(&settings.config_dir).unwrap_or(0))
}

fn read_last(dir: &Path) -> Option<i64> {
    let n = crate::settings::Settings::open(dir).get("supportfins_last")?;
    n.as_i64().or_else(|| n.as_f64().map(|f| f as i64))
}

/// Merged into `settings.json` key by key, like every other setting (two windows, and the app's
/// own keys, are never overwritten).
fn write_last(dir: &Path, t: i64) {
    let _ = crate::settings::Settings::open(dir).set("supportfins_last", Value::from(t));
}

/// Where downloaded Support Fins engines go and are read from: `config_dir()/support-fins`
/// (2.0's own folder, never 1.x's). Pass it as the `user_dir` of [`choose_engine`]; the bundled
/// engine stays the fallback.
pub fn engines_dir() -> PathBuf {
    crate::paths::config_dir().join("support-fins")
}

fn install(user_dir: &Path, engine: &[u8], version_json: &[u8]) -> std::io::Result<()> {
    fs::create_dir_all(user_dir)?;
    let tmp = user_dir.join("engine.iife.js.tmp");
    fs::write(&tmp, engine)?;
    replace_file(&tmp, &user_dir.join("engine.iife.js"))?;
    let tmp = user_dir.join("VERSION.json.tmp");
    fs::write(&tmp, version_json)?;
    replace_file(&tmp, &user_dir.join("VERSION.json"))
}

/// `fs::rename` replaces an existing file in one step on Windows too (`MOVEFILE_REPLACE_EXISTING`).
fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}

/// 1.x `_read_info`: `api` must be [`API`] and the engine bytes must match `sha256`.
fn read_info(folder: &Path) -> Option<EngineInfo> {
    let info: Value = serde_json::from_slice(&fs::read(folder.join("VERSION.json")).ok()?).ok()?;
    let data = fs::read(folder.join("engine.iife.js")).ok()?;
    let sha = info.get("sha256").and_then(|s| s.get("engine.iife.js")).and_then(Value::as_str)?;
    if info.get("api").and_then(Value::as_i64) != Some(API) || digest(&data) != sha {
        return None;
    }
    Some(EngineInfo {
        version: json_str(&info, "version"),
        tag: json_str(&info, "tag").unwrap_or_default(),
        commit: json_str(&info, "commit"),
        source: json_str(&info, "source"),
        path: folder.join("engine.iife.js"),
        folder: folder.to_path_buf(),
    })
}

fn json_str(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

/// 1.x `_semver`: `v1.2.3` or `1.2.3`. Anything else is `(0, 0, 0)`.
fn semver(tag: &str) -> (u64, u64, u64) {
    let t = tag.strip_prefix('v').unwrap_or(tag);
    let mut parts = t.split('.');
    let (Some(a), Some(b), Some(c)) = (parts.next(), parts.next(), parts.next()) else {
        return (0, 0, 0);
    };
    if parts.next().is_some() {
        return (0, 0, 0);
    }
    match (parse_u64(a), parse_u64(b), parse_u64(c)) {
        (Some(a), Some(b), Some(c)) => (a, b, c),
        _ => (0, 0, 0),
    }
}

fn parse_u64(s: &str) -> Option<u64> {
    if s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s
}

/// SHA-256 (FIPS 180-4). Inlined so the engine hash does not add a dependency.
fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be,
        0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa,
        0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85,
        0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
        0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f,
        0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] =
        [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = Vec::with_capacity(data.len() + 72);
    msg.extend_from_slice(data);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(chunk[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) = (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    let mut out = [0u8; 32];
    for (i, v) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}
