# STL Editor 2.0 Milestone 5: platform, and a test build Nathan can use (design)

Status: draft for Nathan's approval, 2026-11-14. Milestone 5 of `dev/specs/2026-11-10-stl-editor-2-rust-design.md` (section 9: "Platform: updater, settings, autosave, installers and release workflow"). The rules of `dev/specs/2026-11-12-stl-editor-2-m4-app-design.md` section 2 still apply (1.x's look and behaviour, smoothness, tests, SolidCraft reference only).

## 1. Intent

What Nathan decided (2026-11-14):

- **The goal is a 2.0 test build Nathan installs on his PC and uses day to day.** Everything that makes daily use safe comes first: settings that last, autosave and Recover, and 2.0 living beside 1.x without touching it.
- **Local install only (option B).** The test build is built on Nathan's PC and installed into a folder with a Start Menu shortcut. Nothing is published: no GitHub release or tag, no `/app2/`. New test builds replace the installed one the same way.
- **The updater is built and tested, but has nothing to find yet.** It is 1.x's updater for 2.0, tested against a fake release server. Until a real 2.0 release exists, a check finds nothing.
- **One PandaCode run in five parallel streams (approach A),** split by file as in 4b and 4c.
- **The items parked for milestone 5** are in: the phone browser drawer, recovering after a graphics driver reset, system fonts and font files for sketch text, and Check for Updates / Update Settings in the Help menu.

Goals: 2.0 can be used for real work on Nathan's PC, with nothing lost to a crash or a restart, and 1.x keeps working exactly as before.

Non-goals:

- Publishing anything (GitHub releases, tags, `/app2/`, the website). Milestone 6 does that after the beta.
- Code signing, Mac or Linux installers anyone uses. CI builds the Mac `.app` and Linux AppImage as artefacts so they don't rot; nobody installs them yet.
- Migrating 1.x's settings or recent files into 2.0 (the beta starts clean; milestone 6 decides).

Anti-goals (a failure even if it works):

- 2.0 reading, writing or locking anything in 1.x's folders (`%APPDATA%\STL Editor`, `%LOCALAPPDATA%\STL Editor`), or 1.x's update check offering a 2.0 build.
- Losing work: a crash, a forced close or a power cut that leaves nothing to recover, or a recover prompt that loses imported meshes.
- Autosave or the update check stalling the screen.

## 1a. Approaches

| Approach | Trade-off | |
|---|---|---|
| **A. One run, five parallel streams** split by file | About 4c's size; the streams share little (settings are the only shared piece, fixed first) | **Picked by Nathan** |
| B. Two runs: settings, autosave and updater first; builds, install and parked items second | A usable core sooner, but two plans and twice the waiting | |

Delivery: **local install (B)** picked over GitHub pre-releases (A), which would have tested the updater for real but put each beta on the releases page.

## 2. What milestone 5 builds

### 2.1 2.0's own folders (stream 1, first)

- **Desktop:** settings in `%APPDATA%\STL Editor 2\settings.json`, autosave in `%LOCALAPPDATA%\STL Editor 2\autosave\`, downloaded Support Fins engines in `%APPDATA%\STL Editor 2\support-fins\`, the log in `%LOCALAPPDATA%\STL Editor 2\log.txt`. Linux and Mac use the matching XDG / Application Support folders with "STL Editor 2".
- 2.0 stops writing `ui-settings.json` into 1.x's `%APPDATA%\STL Editor` (it does today). Its keys move into 2.0's `settings.json`.
- `STLEDITOR2_CONFIG_DIR` points the settings and autosave at a throwaway folder, as `STLEDITOR_CONFIG_DIR` does for 1.x; every test that touches them sets it.
- **Web:** settings in `localStorage` under keys starting `stle2_`, autosave in IndexedDB database `stle2`, so the 1.x web edition's storage on the same site is never touched.

### 2.2 Settings that last (stream 1)

A port of `stleditor/settings.py` (one JSON file, written to a `.tmp` and renamed) behind the existing `Platform::setting` / `set_setting`:

- theme, units, the ribbon tab, panel sizes, remembered sections, the Support Fins keys (material, Bare beyond, draw shape), the update keys (`update_check`, `update_last`, `update_cache`, `update_skip`), `supportfins_update` and `supportfins_last`;
- **recent files with their paths** (1.x `recent`, up to 1.x's count), so Open Recent reopens from disk and Save writes back without asking; a missing file is dropped from the list with 1.x's message;
- **`last_dir`** for the open and save dialogs.

### 2.3 Autosave and Recover (stream 2)

A port of `stleditor/autosave.py`:

- **Desktop:** one folder per window, `autosave\<yyyymmdd-hhmmss-xxxxxx>`, holding `document.json` (1.x's format 1 fields), the imported source meshes under `sources\`, and a `lock` file held locked while the window runs. Saved every 60 s while the project has unsaved changes, on a background thread (never the UI thread); written to `.tmp` and renamed.
- Saving or discarding clears the session's copy; a clean exit removes the folder.
- **At start-up,** folders whose lock can be taken are sessions that didn't close cleanly. The Recover Unsaved Work dialog lists them as 1.x does (title, path, time, step count), and recovering rebuilds the project with its sources and marks it unsaved. Empty stale folders are removed.
- **Web:** the same, in IndexedDB, with 1.x web's rules for which tab owns a session.
- The File menu's Recover Unsaved Work item, as 1.x.

### 2.4 The updater (stream 3)

A port of `stleditor/updates.py` for 2.0, on `stl-platform::net` (pure-Rust HTTPS) and the pattern of `stl-platform::fins::update` (check, download, swap):

- checks `natterman12/STL-Editor` GitHub Releases at most once a day in the background (1.x's `update_check`, `update_last`, `update_cache`), and only considers **2.x** versions, never a 1.x release;
- Update now downloads the new `.exe`, swaps it for the running one (Windows: rename the running exe aside, move the new one in, restart through the existing restart helper), and keeps the old one until the new one starts;
- Skip this version (`update_skip`) and Update Settings (on/off), as 1.x;
- **Help menu:** Check for Updates and Update Settings, with 1.x's texts;
- tests run against a fake release server (a local HTTPS server with a test certificate, or the client's base URL overridden in tests): newer found, none newer, a 1.x release ignored, download and swap, a failed download leaves the installed exe untouched.

### 2.5 Builds and the local install (stream 4)

- **`xtask install`:** builds the release exe and installs it to `%LOCALAPPDATA%\Programs\STL Editor 2\stl-editor-2.exe` with a Start Menu shortcut "STL Editor 2" (and no file associations, so 1.x keeps opening `.stl` and `.stlx` files). Running it again replaces the installed build; the window must be closed first, and it says so if it isn't.
- **`xtask uninstall`** removes the folder and the shortcut, and leaves 2.0's settings and autosave unless `--purge`.
- **CI artefacts:** the Windows exe, the Mac `.app` (zipped), the Linux AppImage and the web build are built on every push and kept as workflow artefacts. Nothing is attached to a release.
- The window title and About say "STL Editor 2.0 (test build)" with the build's date and commit, so Nathan always knows which build he is running.

### 2.6 Parked items (stream 5)

- **The browser drawer on phones** (`setupDrawer`, 1.x's narrow-screen layout): the `touch` UI test's parked drawer check passes.
- **Recovering after a graphics driver reset:** when wgpu reports the device lost, the viewport creates a new device and uploads the scene again, without losing the project; a test forces a device loss.
- **Sketch text fonts:** "More: fonts on this computer…" (system fonts, desktop only) and "Add a font file…" (TTF/OTF, desktop and web), as in 1.x; an added font is remembered in settings.

### 2.7 Decisions for the builder (Claude's calls; Nathan can overrule)

1. **How a 2.x release is told apart:** its tag starts with `v2.` (for example `v2.0.0`, `v2.0.1-beta.1`) and its Windows asset is named `STL-Editor-2-Windows.exe`. The 2.0 updater lists releases (not `releases/latest`) and considers only `v2.*` tags with that asset. Milestone 6 must also stop 1.x's `releases/latest` check from offering a 2.x release; that is recorded in `EXCEPTIONS.md` as a milestone 6 item, not built here.
2. **Several windows at once are supported, as in 1.x** (one autosave folder and lock per window). `settings.json` is re-read just before each write and only the changed keys are merged in, so two windows don't undo each other's settings. Recover never offers a session whose lock is held by a running window. `xtask install` and Update now refuse while another 2.0 window is open, and say so in plain words. One update check per day across all windows, through `update_last`.
3. **Download integrity:** HTTPS to GitHub only (the asset's `browser_download_url` host must be `github.com` or `objects.githubusercontent.com`), the downloaded size must equal the asset's `size`, and when the release lists a `digest` (`sha256:…`) for the asset it must match. Otherwise nothing is swapped and the update says why. The previous exe is kept as `stl-editor-2.old.exe` and deleted only after the new one has drawn its first frame; if the new one never starts, the old one is still there to run by hand. No automatic rollback.
4. **Autosave writes the same project JSON 2.0 writes inside a `.stlx`** (`stl-engine`'s document format, which is 1.x's `document.json`: features, rollback, ui, params), plus 1.x's autosave fields (`format`, `app`, `sources` meta, `path`, `title`, `saved`, `count`), with the imported meshes as files under `sources\`. Recovering loads it through the same code path as opening a `.stlx`.
5. **The update download** runs on a background thread with the busy note "Downloading update…" and no percentage (`net::https_get` returns the whole body), unless adding a progress callback to `net.rs` is small.

## 3. Checks

- **Separation from 1.x:** a test runs 2.0's settings, autosave and updater with 1.x's folders present and checks that nothing in them changes. 1.x's pytest and UI tests still pass.
- **Crash recovery:** a test kills a window's process mid-session (desktop) and a new window offers its work, with its imported meshes; recovering rebuilds the same timeline.
- **Settings round trip:** every key in 2.2 survives a restart.
- **Updater:** the fake-server tests in 2.4.
- **Install:** `xtask install` then `xtask uninstall` on a clean user folder (CI on Windows, with a temp profile).
- **Smoothness:** autosave of the bench mesh project and an update check never put a frame over 16.7 ms (a uibench row "autosave").
- **UI tests:** the parked `touch` drawer check, Recover Unsaved Work, Open Recent from disk, Check for Updates.
- **Unchanged:** every check from milestones 1–4 (parity, goldens, pure-rust, wasm32, web checks, uibench, 1.x tests).
- **Rules from milestone 4:** each worker builds only in `rust/target/m5-t<N>`; tests that pick wait for the viewport; the lead runs the CI steps locally.

## 4. Done when

1. 2.0 uses only its own folders, and the separation test passes.
2. Settings, recent files, autosave and Recover work on desktop and web, with their tests.
3. The updater passes its fake-server tests, and the Help menu has its items.
4. `xtask install` puts a working test build on Nathan's PC with a Start Menu shortcut; CI keeps the four artefacts.
5. The three parked items pass their tests.
6. Every earlier check passes, and CI would pass. `EXCEPTIONS.md` lists every new 1.x difference, pending Nathan's OK.

## 5. Team

The "STL 2.0 build" profile, as in 4c.

| Who | Stream | Jobs |
|---|---|---|
| Opus 5.5 (lead) | | Hands out work, writes the fixed settings interface (plan Task 1), checks results, runs the CI steps, closes out |
| Sonnet A | 1 | 2.0's folders, settings, recent files with paths, `last_dir` |
| Sonnet B | 2 | Autosave, the lock file, Recover Unsaved Work (desktop and web) |
| Grok B | 3 | The updater, the fake release server tests, the Help menu items |
| Cursor (Auto) | 4 | `xtask install` / `uninstall`, CI artefacts, the "test build" title, the UI tests |
| Opus (Claude) | 5 | Graphics device recovery, sketch text fonts |
| Composer B | 5 | The phone browser drawer |
| Haiku, Hermes | | Read-only reviews. If Hermes runs out, Haiku. |
