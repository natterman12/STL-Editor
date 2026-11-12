# STL Editor 2.0 Milestone 4a (Shell and Viewport) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 2.0 is a running app on Windows and the web:

- 1.x's window layout and look
- a smooth wgpu viewport
- the engine in the background
- the browser tree, timeline, parameters and files

A real 1.x project opens and looks the same.

**Architecture:**

- `stl-app` gains `theme.rs` and four modules:
  - `ui/` (one file per region)
  - `viewport/` (the only wgpu code)
  - `engine_link/` (the `Document` on a worker thread, or a Web Worker on wasm)
  - `platform/` (native and web file access)
- The UI sends `Command`s and draws from the `Update`s it gets back. It never touches `Document` directly.

**Tech Stack:**

- eframe and egui 0.37, egui-wgpu, wgpu
- `rfd` (XDG portal on Linux)
- `resvg` or `tiny-skia` for the icons
- `web-time`, `egui_kittest`
- for the web, `wasm-bindgen`, `web-sys` and `js-sys`

No other new dependency without the lead's approval. Every one must pass `xtask pure-rust`.

**Spec:** `dev/specs/2026-11-12-stl-editor-2-m4-app-design.md`, under the main 2.0 spec.

## Global Constraints

- **Milestone 3's constraints still apply:**
  - messages word for word
  - `EXCEPTIONS.md` (every 1.x difference "pending Nathan's OK")
  - pure Rust
  - the wasm32 builds of the engine crates
  - no golden input from `dev/bench/test_mesh.stl`
  - no commits or pushes
  - the tool paths
  - don't stop for decisions; park them
  - second-worker reviews for ports
- **SolidCraft (`tangency-labs/solidcraft`) is read-only reference.** You may read it to learn techniques. Copy nothing from it: no code, no theme values or sizes, no icons, no fonts. Don't depend on it or credit it. A reviewer who finds copied SolidCraft code rejects the task.
- **The 1.x sources of truth:**
  - layout: `web/index.html`
  - tokens: `web/css/*.css` (dark, and `html[data-theme=light]`)
  - icons: `web/js/icons.js`
  - viewer behaviour: `web/js/viewer.js`
  - app behaviour and shortcuts: `web/js/app.js`
  - client calls: `web/js/api.js`

  Match them; raise any mismatch egui forces in the report.
- **Crate boundaries:**
  - Only `stl-app/src/viewport/` uses wgpu.
  - Only `stl-app` uses egui and eframe.
  - Crates below `stl-app` gain no UI dependency.
  - `stl-engine` may gain the cancel and progress hooks in Task 2, and nothing else.
- **New user-facing text, pending Nathan's OK:**
  - `"The engine stopped unexpectedly. Your last saved project is safe."`
  - the button `"Restart"`
- **Smoothness targets (spec 2.3)**, checked with `xtask uibench`:
  - orbit at the refresh rate, at least 60 fps
  - 99th-percentile frame under 16.7 ms on the bench mesh
  - zero frames while idle
  - no frame over 16.7 ms during a rebuild
  - only changed bodies uploaded
  - window within 1 s

## Review Focus

1. **A command arrives while a rebuild is running** (an edit during a slow Hollow). The new command waits, or Cancel stops the running one; the UI never blocks; the final state matches doing the two commands in order. Test: Task 2 `command_during_rebuild`.
2. **Cancel in the middle of a long rebuild.** The document is exactly as it was before that command; undo history is unchanged. Test: Task 2 `cancel_restores_document`.
3. **The engine worker panics** (a feature bug). The app shows the engine-lost message with Restart, and Restart reopens the last saved state. Test: Task 2 `engine_panic_shows_restart`, and the same on web in Task 3.
4. **A project whose bodies are all hidden, or that has no bodies.** The viewport shows the grid and planes, fit-to-view doesn't divide by zero, and picking returns nothing. Tests: Task 4 `empty_scene_fit`, Task 5 `pick_empty_scene`.
5. **The window is resized to tiny or minimised, or the GPU surface is lost.** No panic; the renderer recreates the surface and keeps its uploaded buffers. Test: Task 4 `surface_lost_recovers`, using wgpu's surface-error path.

---

### Task 0: Test harness, UI parity map and CI

**Owner:** Cursor. **Review:** Hermes (Haiku if Hermes is out of requests). It runs from the start.

**Files:**
- **Create:**
  - `rust/tests/UI_PARITY.md`
  - `rust/crates/stl-app/tests/ui/{mod.rs,harness.rs}`
  - `rust/xtask/src/uibench.rs`
  - `rust/tests/UI_BENCH.md`
- **Modify:**
  - `rust/xtask/src/main.rs` (the `uibench` command)
  - `.github/workflows/rust.yml` (build the app on Windows and Linux; build the web app; run the kittest tests; run a headless-Chrome load test of the web build)

**Interfaces:**
- `UI_PARITY.md` has one row per file in `tests/ui/*.js`, except `common.js`, `run.js` and `serve.js`: `| 1.x file | what it tests | batch (4a/4b/4c) | 2.0 test | status |`.
- `harness.rs` gives `fn app_harness(project: Option<&Path>) -> Harness<'static, StlApp>`: it starts the app with a fake `Platform`, loads the project, and waits until the engine has no work left.
- `uibench` opens the bench mesh in the real app window and drives each item with synthetic input:
  - 10 s of orbit
  - 5 s of pan
  - 5 s of zoom
  - 5 s idle
  - one rebuild (Fillet on the bench mesh)
  - launch to first frame

  It reports the frame count, frame times (median and 99th percentile), the longest frame, the idle frames and the upload count. It appends a dated table to `UI_BENCH.md`.

- [ ] **Step 1:** Write `UI_PARITY.md`. Read each 1.x test file and decide its batch. The 4a ones are those testing only the shell, viewport, files, browser, timeline or parameters.
- [ ] **Step 2:** Write the harness and `uibench` against the Task 1 skeleton. Run `cargo test -p stl-app --test ui`. Expected: compiles; the placeholder test passes.
- [ ] **Step 3:** Add the CI jobs. Check them locally where possible (`cargo build -p stl-app --release`, and the web build command from Task 3).

### Task 1: App skeleton, theme, icons and font

**Owner:** Sonnet A. **Review:** Composer B.

Do Step 1 first and tell the lead, because every other task builds on it.

**Files:**
- **Create:**
  - `rust/crates/stl-app/src/{main.rs,app.rs,theme.rs,icons.rs,fonts.rs}`
  - `rust/crates/stl-app/src/{ui,viewport,engine_link,platform}/mod.rs`, with the types below
- **Modify:** `rust/crates/stl-app/Cargo.toml`, `rust/crates/stl-app/src/lib.rs`

**Interfaces (produced, used by every task):**

```rust
pub struct StlApp { /* theme, platform: Box<dyn Platform>, link: Box<dyn EngineLink>, viewport: Viewport, ui state */ }
// theme.rs
pub struct Tokens { pub chrome: Color32, pub ribbon: Color32, pub bg: Color32, pub panel: Color32, pub accent: Color32, /* every --token in web/css, same names in snake_case */ }
pub fn tokens(dark: bool) -> Tokens;
pub fn apply(ctx: &egui::Context, t: &Tokens);
// icons.rs
pub fn icon(name: &str, size: f32, color: Color32) -> egui::ImageSource<'static>; // names as ICONS keys in icons.js
// engine_link/mod.rs
pub enum Command { Open { name: String, bytes: Vec<u8> }, Import { name: String, bytes: Vec<u8> }, AddFeature { kind: String, params: serde_json::Value, index: Option<usize> }, EditFeature { id: String, params: serde_json::Value }, DeleteFeature { id: String }, MoveFeature { id: String, index: usize }, SetParams(Vec<Param>), Rollback(Option<usize>), Undo, Redo, BodyProps { id: String, name: Option<String>, color: Option<String>, hidden: Option<bool> }, Save, Export { format: ExportFormat, ids: Option<Vec<String>>, date: String }, Cancel }
pub enum Update { Bodies(Vec<BodyView>), Timeline(TimelineView), Params(Vec<ParamView>), Planes(Vec<PlaneView>), Sketches(Vec<SketchView>), Message { level: Level, text: String }, Progress { done: usize, total: usize, label: String }, Busy(bool), Saved { bytes: Vec<u8> }, Exported { name: String, bytes: Vec<u8>, count: usize }, Error(String), EngineLost }
pub struct BodyView { pub id: String, pub name: String, pub color: Option<String>, pub hidden: bool, pub mesh: Arc<MeshArrays>, pub revision: u64 }
pub trait EngineLink { fn send(&self, c: Command); fn poll(&self) -> Vec<Update>; fn restart(&self); }
// platform/mod.rs
pub struct PickedFile { pub name: String, pub bytes: Vec<u8> }
pub struct Pending<T>(Arc<Mutex<Option<T>>>); // poll with .take()
pub trait Platform { fn open_file(&self, filters: &[(&str, &[&str])]) -> Pending<Option<PickedFile>>; fn save_file(&self, suggested: &str, bytes: Vec<u8>) -> Pending<Result<bool, String>>; fn now_iso_utc(&self) -> String; fn is_web(&self) -> bool; }
// viewport/mod.rs: pub struct Viewport; see Task 4 for its methods
pub struct PlaneFrame { pub origin: [f64; 3], pub x: [f64; 3], pub y: [f64; 3], pub normal: [f64; 3] }
// ui/mod.rs
pub enum UiEvent { OpenTool(String), Select(Vec<String>), Hover(Option<String>) }
```

The view structs (`TimelineView`, `ParamView`, `PlaneView`, `SketchView`) are declared here with the fields `views.rs` fills; Task 2 may add fields.

- [ ] **Step 1:** Make the skeleton compile with these types, an empty window, and stub implementations: an `EngineLink` that does nothing and a `Platform` that returns `None`. Run `cargo run -p stl-app` and `cargo build -p stl-app --target wasm32-unknown-unknown`. Expected: both build and the native window opens.
- [ ] **Step 2: Write the failing tests:**
  - `tokens_match_css`: parses `web/css/*.css` and checks every `--token` in both themes equals `tokens(dark)`.
  - `icons_cover_icons_js`: every key in `ICONS` draws without error.
- [ ] **Step 3:** Implement the tokens, the egui style (rounding, spacing and text sizes measured from the 1.x CSS), the icons from `icons.js`'s SVG, and the font rule from spec 2.1. Write down the bundled font chosen and why.
- [ ] **Step 4:** Run `cargo test -p stl-app`. Expected: PASS.

### Task 2: Desktop engine link, with cancel and progress

**Owner:** Sonnet B, after Task 1 Step 1. **Review:** Composer B.

**Files:**
- **Create:** `stl-app/src/engine_link/{native.rs,worker.rs,views.rs}`
- **Modify:** `rust/crates/stl-engine/src/document.rs` (cancel and progress hooks only)
- **Test:** `stl-app/tests/engine_link.rs`

**Interfaces:**
- `Document::set_cancel(&mut self, flag: Arc<AtomicBool>)`, checked between features. A cancelled evaluate restores the state from before the command and returns `FeatureError` `"Cancelled."`, which is shown only as a status message.
- `Document::set_progress(&mut self, f: Box<dyn FnMut(usize, usize, &str) + Send>)`: done, total, feature label.
- `views.rs` builds `BodyView`, `TimelineView`, `ParamView`, `PlaneView` and `SketchView` from `Document`.
  - Each body's `revision` changes only when its mesh changes, so Task 6 can upload only changed bodies.
  - `worker.rs` holds the logic both links share.
- A panic in the worker is caught (`catch_unwind` around each command) and sends `Update::EngineLost`. `restart()` reopens the last `Saved` bytes, or a new document if there are none.

- [ ] **Step 1: Write the failing tests:**
  - `open_project_sends_bodies`, on a 1.x fixture from `make_project_fixtures.py`
  - `command_during_rebuild`
  - `cancel_restores_document`
  - `progress_reports_each_feature`
  - `unchanged_body_keeps_revision`
  - `engine_panic_shows_restart`, using a test-only feature kind that panics
- [ ] **Step 2:** Run `cargo test -p stl-app --test engine_link`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the tests, `cargo test -p stl-engine --test parity` (unchanged) and the engine wasm32 build. Expected: PASS.

### Task 3: Web edition (engine Web Worker, wasm app, web platform)

**Owner:** Grok B, after Task 1 Step 1. **Review:** Worker 6.

**Files:**
- **Create:**
  - `stl-app/src/engine_link/web.rs`
  - `stl-app/src/platform/web.rs`
  - `rust/crates/stl-app/web/{index.html,worker.js}`
  - `rust/xtask/src/web.rs` (`cargo run -p xtask -- web [--serve]`)
- **Modify:** `.github/workflows/rust.yml`, with Cursor in Task 0

**Interfaces:**
- **The Web Worker** runs a second entry point of the same wasm module and owns the `Document`. Commands go as structured-clone messages; meshes come back as `ArrayBuffer`s, transferred, not copied. `web.rs` implements `EngineLink` with the same behaviour as Task 2, including `EngineLost` when the worker errors.
- **The web `Platform`:**
  - opens files with a file input element and saves with a download link
  - `now_iso_utc()` comes from `Date`, which closes the milestone 3 STEP-date exception
- **The Support Fins browser host** (`stl-app/src/web/fins.rs`) keeps working.
- **`xtask web`** builds into `rust/target/web/` and `--serve` serves it on localhost.

- [ ] **Step 1: Write the failing tests** (`wasm-bindgen-test`, headless Chrome):
  - `worker_opens_project`
  - `worker_mesh_is_transferred`: the buffer is detached on the worker side after sending
  - `worker_error_sends_engine_lost`
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run them, then `xtask web`, then a headless-Chrome check that opens the page, loads a fixture project and sees its body count in the browser tree. Expected: PASS.

### Task 4: Viewport renderer

**Owner:** Opus (Claude), after Task 1 Step 1. **Review:** Worker 6.

**Files:**
- **Create:** `stl-app/src/viewport/{mod.rs,camera.rs,renderer.rs,shaders/*.wgsl,scene.rs,overlays.rs,viewcube.rs}`
- **Test:** `stl-app/tests/viewport.rs`

**Interfaces (produced):**

```rust
impl Viewport {
    pub fn show(&mut self, ui: &mut egui::Ui) -> ViewportResponse;          // egui-wgpu paint callback
    pub fn sync_bodies(&mut self, bodies: &[BodyView]);                    // upload when revision changed
    pub fn set_selection(&mut self, ids: &[String]); pub fn set_hover(&mut self, id: Option<&str>);
    pub fn set_dimmed(&mut self, ids: &[String], dim: bool); pub fn set_edges_visible(&mut self, v: bool);
    pub fn set_grid_visible(&mut self, v: bool); pub fn set_base_planes_visible(&mut self, v: bool);
    pub fn set_mouse_mode(&mut self, m: MouseMode); pub fn set_theme(&mut self, dark: bool);
    pub fn sync_planes(&mut self, planes: &[PlaneView]); pub fn sync_sketches(&mut self, s: &[SketchView], active: Option<&str>);
    pub fn fit(&mut self, b: Aabb); pub fn scene_box(&self) -> Option<Aabb>;
    pub fn world_to_screen(&self, p: [f64; 3]) -> Pos2; pub fn pixel_size(&self, at: [f64; 3]) -> f64;
    // overlays, same names and arguments as viewer.js, tag-addressed:
    pub fn highlight_face(&mut self, body: &str, tris: &[u32], tag: &str, color: Color32);
    pub fn add_marker(..); pub fn add_arrow(..); pub fn add_line(..); pub fn add_polyline(..); pub fn add_plane_ghost(..);
    pub fn set_band(..); pub fn add_regions(..); pub fn show_preview(..); pub fn clear_preview(&mut self); pub fn clear_overlay(&mut self, tag: &str);
}
pub enum MouseMode { Model, Sketch, Sculpt }
```

- **Shading, the edge angle, colours, the grid, the base planes, the origin and the view cube** all follow `viewer.js`. Read its constants; don't guess them.
- **Mouse mapping:**
  - `Model`: left rotates, middle and right pan.
  - `Sketch` and `Sculpt`: left does nothing, middle pans, right rotates.
  - Zoom goes to the cursor, panning is in screen space, and there's no damping.

- [ ] **Step 1: Write the failing tests:**
  - `camera_fit_matches_viewer_js`: the same box gives the same camera distance and target as 1.x `fit`
  - `empty_scene_fit`
  - `mouse_modes_map_buttons`
  - `sync_skips_unchanged_revision`
  - `surface_lost_recovers`
  - snapshot tests of the cube fixture: shaded, edges, selected, hidden, dark and light
- [ ] **Step 2:** Run `cargo test -p stl-app --test viewport`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the tests, plus `xtask uibench` for the orbit, pan and zoom rows. Expected: PASS, and orbit at least 60 fps with the 99th percentile under 16.7 ms on the bench mesh.

### Task 5: Picking

**Owner:** Gemini (Cursor, limited). If Gemini reaches its limit, the lead hands this task, with its work so far, to Grok B. **Review:** Opus (Claude).

**Files:**
- **Create:** `stl-app/src/viewport/pick.rs`, plus a BVH in `rust/crates/stl-mesh/src/bvh.rs` (pure geometry, no UI)
- **Test:** `stl-app/tests/pick.rs`, `stl-mesh` unit tests

**Interfaces:**

```rust
pub struct Bvh; impl Bvh { pub fn build(m: &MeshArrays) -> Bvh; pub fn ray(&self, m: &MeshArrays, o: [f64; 3], d: [f64; 3]) -> Option<(u32, f64)>; }
pub enum Hit { Body { id: String, face: u32, point: [f64; 3], normal: [f64; 3] }, Plane { id: String, point: [f64; 3] } }
impl Viewport { pub fn pick(&self, pos: Pos2, bodies: bool, planes: bool) -> Option<Hit>; pub fn ray_to_plane(&self, pos: Pos2, frame: &PlaneFrame) -> Option<[f64; 3]>; pub fn pick_preview(&self, pos: Pos2) -> Option<String>; }
```

- A body's BVH is built once per mesh revision, in the background, never on the UI thread.
- Hidden bodies are never hit. Planes are hit only when shown.

- [ ] **Step 1: Write the failing tests:**
  - `pick_matches_brute_force`: 1,000 random rays on three fixtures give the same triangle as testing every triangle
  - `pick_empty_scene`
  - `pick_skips_hidden`
  - `ray_to_plane_matches_viewer_js`
  - `pick_bench_mesh_under_2ms`, run in release
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run them. Expected: PASS.

### Task 6: Section view and smoothness

**Owner:** Worker 6, after Task 4 has a first frame. **Review:** Opus (Claude).

**Files:**
- **Modify:** `stl-app/src/viewport/{renderer.rs,scene.rs}`
- **Create:** `stl-app/src/viewport/section.rs`

**Interfaces:**
- `Viewport::set_section(&mut self, on: bool)`, with `viewer.js`'s plane and cap colour.
- **Repaints happen only when something changes:** input, an `Update`, a running animation or a resize. Never every frame.
- **Uploads:** only bodies whose `revision` changed are uploaded. Large uploads are split across frames, so no frame goes over 16.7 ms.

- [ ] **Step 1: Write the failing tests:**
  - `section_snapshot`, dark and light
  - `idle_draws_no_frames`: 5 s with no input gives 0 repaints
  - `rebuild_never_stalls`: a Fillet rebuild on the bench mesh has its longest frame under 16.7 ms
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run them and all of `xtask uibench`. Expected: every spec 2.3 target met.

### Task 7: Top bar, ribbon, start page and dialogs

**Owner:** Sonnet A, after Task 1. **Review:** Composer B.

**Files:**
- **Create:** `stl-app/src/ui/{topbar.rs,ribbon.rs,start.rs,dialogs.rs,menu.rs}`

**Interfaces:**
- **The ribbon** shows 1.x's tabs, groups and buttons, built from the same data `tools.js` uses. Tool buttons send `UiEvent::OpenTool(key)`; in 4a that does nothing yet.
- **Tool search** (`#tool-search`) uses arrow keys, Enter and Escape, as `app.js` line 289 onward does.
- **`dialogs.rs`** matches `app.js`'s modal (`#modal`): a prompt with a text input (Enter accepts) and confirm dialogs with buttons.
- **The start page** offers Open, Box and recent files for this session.
- **The hint line** (`#hint`) and the HUD (`#hud`) show what `app.js` shows there. The sketch HUD is 4c.
- **The dark/light switch** is where 1.x has it, and calls `theme::apply` and `Viewport::set_theme`.

- [ ] **Step 1: Write the failing tests:** kittest tests for the Task 0 rows of this region, plus snapshots of each region in dark and light.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run them. Expected: PASS.

### Task 8: Browser tree, timeline, parameters, status, toasts and busy overlay

**Owner:** Worker 7 (Composer), after Task 1 and Task 2's `views.rs`. **Review:** Haiku (Claude).

**Files:**
- **Create:** `stl-app/src/ui/{browser.rs,timeline.rs,params.rs,status.rs,toasts.rs,busy.rs}`

**Interfaces:**
- **The browser tree** shows `#browser-count` and supports rename, colour and hide, each sending `Command::BodyProps`. Selecting a row selects the body in the viewport.
- **The timeline bar:**
  - one item per feature
  - drag the marker to roll back (`Command::Rollback`)
  - `tl-start`, `tl-prev`, `tl-next` and `tl-end`
  - the right-click menu with edit (opens the tool in 4b), delete and move
- **Parameters:** 1.x's list, with editing, expressions in the `--expr` colour, and errors shown as 1.x shows them. Changes send `Command::SetParams`.
- **The busy overlay** shows `Progress` and a Cancel button that sends `Command::Cancel`.
- **Toasts and the status line** show `Message` updates with 1.x's levels and colours.

- [ ] **Step 1: Write the failing tests:** kittest tests for this region's Task 0 rows, including a rollback, a rename, a hide and a parameter edit on a fixture project; plus snapshots.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run them. Expected: PASS.

### Task 9: Files, shortcuts, undo and redo, drag and drop

**Owner:** Sonnet B, after Tasks 1 and 2. **Review:** Composer B.

**Files:**
- **Create:**
  - `stl-app/src/platform/native.rs` (rfd)
  - `stl-app/src/ui/{files.rs,keys.rs}`

**Interfaces:**
- **Open** reads `.stlx` and the mesh formats the 1.x open dialog lists. **Import** adds a body. **Save** writes `.stlx`. **Export** offers STL, 3MF, OBJ and STEP, with `Document::export_base_name` as the suggested name.
- **`keys.rs`** ports `App.onKey` in `web/js/app.js` key by key, including what each key does while a text field has focus.
- **Drag and drop:** dropping a file opens or imports it, as 1.x does. The drop overlay shows while a file is dragged over the window.

- [ ] **Step 1: Write the failing tests:**
  - `open_save_round_trip`: open a 1.x fixture, save it, and check 1.x's `stlx` reader accepts it (a Python check through `dev/parity/open_check.py`)
  - `export_each_format`: through the fake `Platform`
  - `every_onkey_binding`: one kittest per key
  - `drop_opens_file`
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run them. Expected: PASS.

### Task 10: Support Fins in the app, the real-project check, uibench and screenshots

**Owner:** Cursor, after Tasks 2–9. **Check:** the lead.

- [ ] **Step 1:** Add `fins_in_app`: the app window is open and a Support Fins call through the webview host returns `fins_golden.json`'s result.
- [ ] **Step 2:** Open each project in `rust/tests/models/projects/` (from `make_project_fixtures.py`) in the app. Check bodies, names, colours, planes, sketches and timeline against 1.x's `Document` loaded from the same file.
- [ ] **Step 3:** Run `xtask uibench` three times with nothing else running. Append the results to `UI_BENCH.md`.
- [ ] **Step 4:** Save side-by-side screenshots to `dev/out/m4a-shots/`. Each pair is 1.x served by `tests/ui/serve.js` and 2.0 at the same window size: the start page, a project open, the browser with a rename, the timeline rolled back, the parameters panel, and section view, each in dark and light.

### Task 11: Close-out

**Owner:** the lead.

- [ ] **Step 1:** Run:
  - `cargo test --workspace --release`
  - the kittest tests
  - the parity runner (0 failed, 0 pending)
  - every `--check` script
  - `xtask pure-rust`
  - the wasm32 builds and `xtask web`
  - the web load test
  - `open_check.py`
  - the 1.x `pytest`
  - the export bench rows (no regression)
- [ ] **Step 2:** Update `EXCEPTIONS.md`, marking every new 1.x difference "pending Nathan's OK". Update `UI_PARITY.md` statuses.
- [ ] **Step 3:** Report:
  - the result lines
  - the `UI_BENCH.md` table
  - the screenshot folder
  - every look mismatch egui forced
  - which font was bundled and why
  - every parked question

  Do not commit.
