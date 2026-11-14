# STL Editor 2.0 Milestone 5 (Platform and a Local Test Build) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** a 2.0 test build Nathan installs on his PC and uses day to day: its own folders, settings and recent files that last, autosave with Recover, the updater (tested, nothing to find yet), `xtask install`, and the three parked items.

**Architecture:**

- `stl-platform` (desktop only) gains `paths.rs`, `settings.rs`, `autosave.rs` and `update.rs`, ports of `stleditor/paths.py`, `settings.py`, `autosave.py` and `updates.py`, on the existing `net.rs` HTTPS client and the `fins/update.rs` check-download-swap pattern.
- `stl-app/src/platform/` exposes them through the `Platform` trait (native) and `localStorage` / IndexedDB (web).
- The engine worker writes autosaves (it holds the document), when the app asks every 60 s.
- `xtask install` / `uninstall` put the release exe in `%LOCALAPPDATA%\Programs\STL Editor 2` with a Start Menu shortcut.

**Tech Stack:** As milestone 4. No new dependency without the lead's approval; every new dependency must pass `xtask pure-rust`. Windows shortcuts are written as a `.lnk` from Rust (a small pure-Rust writer, or the `mslnk` crate if it passes the pure-Rust check), never by shelling out to PowerShell.

**Spec:** `dev/specs/2026-11-14-stl-editor-2-m5-platform-design.md`, under `dev/specs/2026-11-10-stl-editor-2-rust-design.md` section 9.

## Global Constraints

- **Every milestone 4 Global Constraint still applies:** messages word for word with 1.x; `EXCEPTIONS.md` (every 1.x difference "pending Nathan's OK"); pure Rust; the wasm32 builds; no commits or pushes; don't stop for decisions, park them; second-worker reviews; only `viewport/` uses wgpu.
- **SolidCraft is read-only reference.** Copy nothing from it.
- **Never touch 1.x's folders:** `%APPDATA%\STL Editor`, `%LOCALAPPDATA%\STL Editor`, the `STLEDITOR_CONFIG_DIR` folder, or the 1.x web edition's `localStorage` keys (`stle_*`, `stle.*`) and IndexedDB databases. 2.0 uses `STL Editor 2` folders, `stle2_` keys and the `stle2` database.
- **Tests never touch Nathan's real folders:** every test that reads or writes settings, autosave or the install folder sets `STLEDITOR2_CONFIG_DIR` (and, for install, `STLEDITOR2_INSTALL_DIR`) to a temp folder.
- **Publish nothing:** no tag, release, `/app2/`, website or version bump. The updater's network tests use a fake server, never GitHub.
- **The 1.x sources of truth:** `stleditor/paths.py`, `settings.py`, `autosave.py`, `updates.py`, `server.py` (recent, `last_dir`, the Recover and update routes), and `web/js/app.js` (the Recover dialog, the update banner and settings dialog, `setupDrawer`, the text font list).
- **Builds:** each worker sets `CARGO_TARGET_DIR` to `target/m5-t<N>` and runs cargo from `rust/`; never `rust/rust/` or `rust/crates/target`; the lead verifies in `rust/target/verify` with `-j 6`.
- **Smoothness:** autosave and the update check run off the UI thread; no frame over 16.7 ms while they run.

## Review Focus

1. **Two 2.0 windows at once.** Settings written by both are merged key by key, and Recover never offers the other window's live session. Tests: Task 2 `two_writers_merge`, Task 3 `live_session_not_offered`.
2. **A corrupt or half-written autosave** (bad JSON, a missing source file). It is skipped with a log line, never crashes start-up, and the other sessions are still offered. Test: Task 3 `corrupt_autosave_skipped`.
3. **A recent file that was moved or deleted.** Open Recent says so in 1.x's words and drops it from the list. Test: Task 2 `missing_recent_dropped`.
4. **An update download that fails or comes back the wrong size or digest.** Nothing is swapped and the installed exe is byte-for-byte unchanged. Test: Task 4 `failed_download_leaves_exe`.
5. **An unreadable `settings.json`.** 2.0 starts with defaults, keeps the bad file as `settings.json.bad` and doesn't overwrite it until the next change. Test: Task 2 `corrupt_settings_kept_aside`.
6. **1.x's folders and storage.** Nothing in them changes while 2.0 runs every feature in this plan. Test: Task 8 `leaves_1x_alone`.

---

### Task 0: Fake release server, UI test skeletons

**Owner:** Cursor. **Review:** Hermes (Haiku if Hermes is out). Runs from the start.

**Files:**
- **Create:** `rust/crates/stl-platform/tests/support/fake_releases.rs` (a local HTTP server on `127.0.0.1:0` serving a releases JSON and asset bytes, with knobs for wrong size, wrong digest, dropped connection); `rust/crates/stl-app/tests/ui/` skeletons `recover.rs`, `open_recent.rs`, `updates.rs`, marked `#[ignore]` until their task lands.
- **Modify:** `.github/workflows/rust.yml`: an "artefacts" job on push that builds the release Windows exe, the Mac `.app` (zipped), the Linux AppImage and the web build, and uploads them with `actions/upload-artifact`; nothing attached to a release.

- [ ] **Step 1:** The fake server: `FakeReleases::start(releases: serde_json::Value, assets: Vec<(String, Vec<u8>)>, faults: Faults) -> FakeReleases` with `base_url(&self) -> String`. Self-test `fake_server_serves_json_and_assets`.
- [ ] **Step 2:** The three skeletons, each with the matching 1.x behaviour as comments. The CI artefacts job, checked with `actionlint` (it runs for real on the next push, after close-out).

### Task 1: The fixed platform interfaces

**Owner:** Sonnet A. **Review:** the lead. Short; Tasks 2–6 start as soon as it lands.

**Files:**
- **Create:** `rust/crates/stl-platform/src/{paths.rs,settings.rs,autosave.rs,update.rs}` with the signatures below (bodies `todo!()` except `paths.rs`, written here in full); `rust/crates/stl-platform/tests/paths.rs`
- **Modify:** `stl-platform/src/lib.rs`; `stl-app/src/platform/mod.rs` (the trait additions); `stl-app/src/engine_link/mod.rs` (the two variants)

**Interfaces** (fixed; changes go through the lead):

```rust
// stl-platform/src/paths.rs (desktop)
pub const APP_DIR: &str = "STL Editor 2";
pub fn config_dir() -> PathBuf;   // %APPDATA%\STL Editor 2 · ~/.config/STL Editor 2 · ~/Library/Application Support/STL Editor 2
pub fn data_dir() -> PathBuf;     // %LOCALAPPDATA%\STL Editor 2 · ~/.local/share/STL Editor 2 · same as config on Mac
pub fn install_dir() -> PathBuf;  // %LOCALAPPDATA%\Programs\STL Editor 2
// STLEDITOR2_CONFIG_DIR=<d> makes config_dir() = <d>, data_dir() = <d>/data;
// STLEDITOR2_INSTALL_DIR=<d> makes install_dir() = <d>.

// stl-platform/src/settings.rs
pub struct Settings { /* path */ }
impl Settings {
    pub fn open(dir: &Path) -> Settings;                       // settings.json in dir
    pub fn get(&self, key: &str) -> Option<serde_json::Value>;
    pub fn set(&self, key: &str, value: serde_json::Value) -> std::io::Result<()>; // re-read, merge this key, .tmp + rename
}

// stl-platform/src/autosave.rs
pub struct SessionInfo { pub id: String, pub title: String, pub path: Option<String>, pub saved: f64, pub steps: usize }
pub struct Source { pub id: String, pub name: String, pub ext: String, pub data: Vec<u8> }
pub struct Autosaver { /* dir, lock */ }
impl Autosaver {
    pub fn begin(root: &Path) -> std::io::Result<Autosaver>;   // new <yyyymmdd-hhmmss-xxxxxx> folder, lock taken
    pub fn id(&self) -> &str;
    pub fn write(&self, doc: &serde_json::Value, sources: &[Source]) -> std::io::Result<()>;
    pub fn clear(&self);                                        // saved or discarded
    pub fn end(self);                                           // clean exit: folder removed
}
pub fn recoverable(root: &Path, exclude: Option<&str>) -> Vec<SessionInfo>;
pub fn load(root: &Path, id: &str) -> Result<(serde_json::Value, Vec<Source>), String>;
pub fn discard(root: &Path, id: &str);

// stl-platform/src/update.rs
pub const RELEASES_API: &str = "https://api.github.com/repos/natterman12/STL-Editor/releases";
pub const ASSET_NAME: &str = "STL-Editor-2-Windows.exe";
pub struct Release { pub tag: String, pub version: String, pub url: String, pub size: u64, pub digest: Option<String>, pub notes: String }
pub fn newest_2x(api: &str, current: &str) -> Result<Option<Release>, String>;   // v2.* tags with ASSET_NAME, newer than current
pub fn download(rel: &Release, to: &Path) -> Result<(), String>;                   // host, size and digest checks
pub fn swap(new_exe: &Path, running_exe: &Path) -> Result<(), String>;              // running -> stl-editor-2.old.exe, new -> running
pub fn finish_swap(running_exe: &Path);                                             // first frame drawn: delete the .old.exe

// stl-app/src/platform/mod.rs: Platform gains (defaults: nothing remembered)
fn setting_json(&self, key: &str) -> Option<serde_json::Value> { None }
fn set_setting_json(&self, key: &str, value: serde_json::Value) {}
fn read_path(&self, path: &str) -> Pending<Result<PickedFile, String>>;  // Open Recent from disk; web: Err
// the existing string setting()/set_setting() keep working, stored as JSON strings

// stl-app/src/engine_link/mod.rs
Command::Autosave                                   // the worker writes if the document changed since the last one
Update::Autosaved { ok: bool, error: Option<String> }
```

- [ ] **Step 1:** `paths.rs` in full, with tests `paths_use_stl_editor_2` (no path contains a component equal to `STL Editor`) and `override_env_vars`.
- [ ] **Step 2:** The rest as signatures. `cargo check --workspace --tests` and the wasm32 build pass.

### Task 2: Settings and recent files (stream 1)

**Owner:** Sonnet A. **Review:** Haiku.

**Files:** fill `stl-platform/src/settings.rs`; `stl-app/src/platform/{native.rs,web.rs}`; `stl-app/src/ui/files.rs` (recent from disk, `last_dir`); the settings call sites in `app.rs`. **Test:** `stl-platform/tests/settings.rs`, `stl-app/tests/settings_app.rs`.

**Behaviour:** `stleditor/settings.py` and the `recent` / `last_dir` code in `server.py`, with spec 2.1–2.2: every remembered value moves to `settings.json` (or `stle2_` keys on web); `ui-settings.json` in 1.x's folder is no longer read or written; the log goes to `data_dir()\log.txt` and downloaded Support Fins engines to `config_dir()\support-fins\` (`stl-platform::fins::update` and the log setup take the new folders; the bundled engine is still the fallback); Open Recent reopens from the path and Save writes back; the File menu lists up to 1.x's number of recent files.

- [ ] **Step 1: Write the failing tests:** `fins_engines_and_log_in_own_folders`, `round_trip_every_key` (each key in spec 2.2 survives a new `Settings::open`), `two_writers_merge` (two `Settings` on one folder set different keys; both survive), `corrupt_settings_kept_aside`, `missing_recent_dropped` (1.x's message word for word), `open_recent_reads_from_disk`, `save_after_open_recent_writes_back`, `last_dir_used_by_dialogs`, `no_ui_settings_in_1x_folder`.
- [ ] **Step 2:** Implement until they pass.

### Task 3: Autosave and Recover (stream 2)

**Owner:** Sonnet B. **Review:** Hermes (Haiku if Hermes is out).

**Files:** fill `stl-platform/src/autosave.rs`; `stl-engine/src/stlx.rs` (`pub fn document_json(doc: &Document) -> serde_json::Value` and `pub fn from_parts(json: &Value, sources: Vec<(String, String, String, Vec<u8>)>, registry: Registry) -> Result<Document, FileError>`, factored out of `to_bytes` / `from_bytes`); `engine_link/worker.rs` (`Command::Autosave`), `engine_link/web.rs` (IndexedDB store); `app.rs` (the 60 s timer, Recover dialog, `help.recover`, the start-up offer). **Test:** `stl-platform/tests/autosave.rs`, `stl-app/tests/recover.rs`.

**Behaviour:** `stleditor/autosave.py` and 1.x's Recover dialog, with spec 2.3 and 2.7 decision 4: the app sends `Command::Autosave` every 60 s; the worker writes only when the document changed; saving or New clears the copy; a clean exit ends the session; at start-up the Recover dialog lists `recoverable(root, Some(own id))` as 1.x does.

- [ ] **Step 1: Write the failing tests:** `writes_only_when_changed`, `sources_kept` (an imported mesh comes back byte for byte), `clean_exit_removes_folder`, `live_session_not_offered` (a second `Autosaver` holds its lock), `crashed_session_offered` (a child process takes a lock and is killed; the session is offered), `corrupt_autosave_skipped`, `recover_rebuilds_timeline` (same step ids, params and rollback), `stale_empty_folders_removed`, `autosave_off_ui_thread` (no frame over 16.7 ms while the bench project autosaves).
- [ ] **Step 2:** Implement until they pass, desktop and web.

### Task 4: The updater (stream 3)

**Owner:** Grok B. **Review:** Opus (Claude).

**Files:** fill `stl-platform/src/update.rs`; `stl-app/src/updates.rs` (new: the daily background check, the update banner, Update now, Skip this version, Update Settings, as 1.x); `app.rs` (Help menu `help.check_updates`, `help.update_settings`, hidden on web). **Test:** `stl-platform/tests/update.rs` (against Task 0's fake server), `stl-app/tests/updates_app.rs`.

**Behaviour:** `stleditor/updates.py` with spec 2.4 and 2.7 decisions 1, 3 and 5. The background check runs at most once a day across windows (`update_last`), off the UI thread. Update now refuses while another 2.0 window is open, in plain words.

- [ ] **Step 1: Write the failing tests:** `finds_newer_2x`, `ignores_1x_releases`, `ignores_older_and_skipped`, `checks_once_a_day`, `download_checks_host_size_digest`, `failed_download_leaves_exe`, `swap_keeps_old_until_first_frame`, `refuses_with_other_window_open`, `help_menu_items` (texts as 1.x).
- [ ] **Step 2:** Implement until they pass.

### Task 5: Install, uninstall and the test-build title (stream 4)

**Owner:** Cursor. **Review:** Haiku.

**Files:** `rust/xtask/src/install.rs` (new), `xtask/src/main.rs` (`install`, `uninstall [--purge]`); `stl-app/build.rs` (build date and short commit); the window title and About. **Test:** `rust/xtask/tests/install.rs`.

**Behaviour:** spec 2.5. `xtask install` builds `stl-app` in release, copies it to `install_dir()\stl-editor-2.exe`, and writes the Start Menu shortcut "STL Editor 2" (`%APPDATA%\Microsoft\Windows\Start Menu\Programs\STL Editor 2.lnk`, overridable for tests); no file associations. It refuses, in plain words, while `stl-editor-2.exe` is running. `uninstall` removes both; `--purge` also removes `config_dir()` and `data_dir()`. The title reads "STL Editor 2.0 (test build <yyyy-mm-dd> <commit>)" and About shows the same.

- [ ] **Step 1: Write the failing tests:** `install_then_uninstall` (temp `STLEDITOR2_INSTALL_DIR` and Start Menu folder), `install_replaces_previous`, `refuses_while_running`, `purge_removes_settings_and_autosave`, `title_says_test_build`.
- [ ] **Step 2:** Implement until they pass.

### Task 6: Graphics device recovery and sketch text fonts (stream 5)

**Owner:** Opus (Claude). **Review:** Composer B. Only this task edits `viewport/`.

**Files:** `viewport/renderer.rs` and `viewport/mod.rs` (device lost); `sketch/text.rs` (font list); `stl-platform/src/fonts.rs` (new, desktop: list system font files). **Test:** `stl-app/tests/device_lost.rs`, `stl-app/tests/sketch_fonts.rs`.

- [ ] **Step 1: Write the failing tests:** `device_lost_recovers` (force a lost device in the test renderer; the next frames draw the scene again, the project is unchanged), `system_fonts_listed` (desktop), `font_file_added_and_remembered` (a TTF from the fixtures; it appears in the list after a restart, through settings), `bad_font_file_says_why`.
- [ ] **Step 2:** Implement until they pass. "More: fonts on this computer…" and "Add a font file…" with 1.x's texts; system fonts are desktop only.

### Task 7: The phone browser drawer (stream 5)

**Owner:** Composer B. **Review:** Haiku.

**Files:** `ui/browser.rs`, `ui/mod.rs` (narrow layout). **Test:** un-park the drawer check in `tests/ui/touch.rs`.

- [ ] **Step 1:** Port `setupDrawer`: on narrow windows the browser panel is a drawer opened by a button over the model, as 1.x. Write the failing check from `touch.js`, then implement until it passes.

### Task 8: UI tests, separation and smoothness

**Owner:** Cursor. **Review:** Hermes (Haiku if Hermes is out).

- [ ] **Step 1:** Fill in the Task 0 skeletons (`recover`, `open_recent`, `updates`) as Tasks 2–4 land, asserting what 1.x shows. Set `UI_PARITY.md`.
- [ ] **Step 2:** `leaves_1x_alone`: create 1.x's folders with sample files and a 1.x `settings.json`, run settings, recent, autosave, Recover, an update check against the fake server and `xtask install` / `uninstall`, then check every 1.x file is byte-for-byte unchanged and no new file appeared there.
- [ ] **Step 3:** The uibench row "autosave" (the bench project autosaving during an orbit): no frame over 16.7 ms. Extend `fonts_fallback.rs` to the new dialogs and messages.

### Task 9: Close-out

**Owner:** the lead, in `rust/target/verify`.

- [ ] **Step 1:** Run every check: `cargo test --workspace --release -j 6`; the parity runner and every `--check`; `xtask pure-rust`; the wasm32 builds; `xtask web --check` and `--webgl2`; the WARP snapshot run; uibench three times; the 1.x `pytest`.
- [ ] **Step 2:** `xtask install` on this PC (the test build Nathan will use), start it from the Start Menu shortcut, and check: the title shows today's build, 1.x still opens `.stl` files, and `%APPDATA%\STL Editor` is unchanged.
- [ ] **Step 3:** Update `EXCEPTIONS.md` (new 1.x differences pending Nathan's OK, and the milestone 6 note on 1.x's update check from spec 2.7) and `UI_PARITY.md`.
- [ ] **Step 4:** Report the result lines, the uibench table with the autosave row, where the test build is installed and how to update it, and anything parked. Do not commit.
