# STL Editor 2.0 Milestone 6 (Finish 2.0 against 1.x) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 2.0 does everything 1.x does: Section View with a drag-the-plane control and hatched cuts, Split that moves its cut, Measure, Properties, Send to Slicer, To STEP, recovery after a graphics driver reset in the real window, 1.x's update check skipping 2.x, and no bench row slower than 1.x. A test proves no button does nothing.

**Architecture:**

- One drag-the-plane control: its maths in `viewport/plane_drag.rs` (plain functions, no wgpu), its drawing and pointer handling in `viewport/`. The app feeds it from Section's state or from any tool whose definition names a plane, a distance and a body field (Split).
- Each new 1.x feature lives in its own new file (`impl StlApp` blocks in `src/app_*.rs`, panels in `src/ui/`); the lead wires `on_ribbon` once in Task 1, so workers never share `app.rs`.
- Engine: Split gains `offset`; a new body-info command answers Properties, on desktop and web.
- Desktop hand-off (slicers, SolidCraft's STEP CLI) in a new `stl-platform::handoff`, compiled out of the web build.

**Tech Stack:** As milestone 5. No new dependency without the lead's approval; every new dependency must pass `xtask pure-rust`.

**Spec:** `dev/specs/2026-11-14-stl-editor-2-m6-finish-design.md`, under `dev/specs/2026-11-10-stl-editor-2-rust-design.md`.

## Global Constraints

- **Every milestone 4 and 5 Global Constraint still applies:** messages word for word with 1.x; every 1.x difference in `rust/tests/EXCEPTIONS.md` "pending Nathan's OK"; pure Rust; the wasm32 builds; no commits or pushes; don't stop for decisions, park them; second-worker reviews; only `viewport/` uses wgpu; tests never touch Nathan's real folders (`STLEDITOR2_CONFIG_DIR`).
- **SolidCraft is a behaviour reference only.** No code, theme values, icons or fonts from it, no dependency, no credit. Running its separate CLI for To STEP is the 1.x hand-off, not a dependency.
- **The 1.x sources of truth:** `web/js/app.js` (`onRibbon`, `startSection`, `renderSection`, `applyCut`, `toggleCut`, `clearCut`, `startMeasure`, `measureClick`, `startInfo`, `showInfo`, `sendMenu`, `slicerItems`, `toStep`), `web/js/viewer.js` (`setSection`, `_applySection`), `stleditor/server.py` (`find_slicer`, `step_available`, `r_to_step`, `r_info`, the `export` route's `slicer`), `stleditor/updates.py`.
- **Publish nothing:** no tag, release, `/app2/`, website or version bump. The 1.x change in Task 7 is committed with the rest, not released.
- **New `Update` or `Command` variants get arms in `crates/stl-app/tests/web_worker.rs`** (wasm-only; CI caught this twice).
- **Builds:** each worker sets `CARGO_TARGET_DIR` to `rust/target/m6-t<N>` and runs cargo from `rust/` with `-j 6`; the lead verifies in `rust/target/verify`.
- **Not in this run:** the thin dashed outlines of hidden edges (spec 2.3, pending Nathan), and the orbit point (already done).

## Review Focus

1. **A plane seen edge-on or face-on.** Dragging the arrow when the normal points at the camera still moves the plane smoothly (the vertical-drag fallback), never jumps or produces NaN. Test: Task 2 `drag_face_on_falls_back`.
2. **A face exactly on the body's boundary.** A fresh pick of the top face starts halfway; a plane already strictly inside keeps 0; reopening a moved split keeps its distance. Tests: Task 2 `start_distance_boundary_is_halfway`, Task 3 `reopen_keeps_offset`.
3. **Old projects.** A split saved by 1.x or an earlier 2.0 build (no `offset`) rebuilds byte for byte as before. Test: Task 3 `split_without_offset_unchanged` plus the existing goldens.
4. **A slicer or SolidCraft that isn't installed, or fails.** The menu lists only what is found; a launch that fails says so in plain words; To STEP is disabled without the CLI. Tests: Task 6 `find_slicer_rules`, `launch_failure_message`.
5. **Properties on a body that isn't watertight.** Shows "No — run Repair" and leaves out Volume and the weights, as 1.x. Test: Task 5 `info_open_mesh`.
6. **A button that still does nothing.** Task 8's walk fails on it.

---

### Task 1: Fixed interfaces and the ribbon wiring

**Owner:** the lead. Short; Tasks 2–7 start as soon as it lands.

**Files:**
- **Create:** `rust/crates/stl-app/src/viewport/plane_drag.rs`, `src/app_section.rs`, `src/app_measure.rs`, `src/app_handoff.rs`, `src/ui/section_panel.rs`, `src/ui/measure_panel.rs`, `src/ui/info_panel.rs`, `rust/crates/stl-engine/src/info.rs`, `rust/crates/stl-platform/src/handoff.rs` (bodies `todo!()` or empty where marked)
- **Modify:** `src/app.rs` (`on_ribbon`: `"section"`, `"measure"`, `"info"`, `"send"`, `"to_step"`; the per-frame drag-plane feed), `src/engine_link/mod.rs`, `src/tools/mod.rs` (`ToolFlags`), `src/platform/mod.rs`, `tests/web_worker.rs`

**Interfaces** (fixed; changes go through the lead):

```rust
// stl-app/src/viewport/plane_drag.rs: pure maths, used by the viewport, the app and tests
pub struct DragPlane { pub origin: V3, pub normal: V3, pub distance: f64, pub span: Aabb }
pub fn start_distance(origin: V3, normal: V3, span: &Aabb) -> f64;          // 0 if strictly inside, else to the middle
pub fn drag_distance(cam: &Camera, p: &DragPlane, from: [f64; 2], to: [f64; 2], d0: f64) -> f64;
pub fn handle_at(cam: &Camera, p: &DragPlane) -> [f64; 2];                   // the arrow's tip on screen
pub fn hits_handle(cam: &Camera, p: &DragPlane, ptr: [f64; 2]) -> bool;
pub fn plane_eq(p: &DragPlane, flip: bool) -> [f64; 4];                       // the kept half, as section.rs takes it

// stl-app/src/viewport/mod.rs (Task 2 implements)
impl Viewport {
    pub fn set_drag_plane(&mut self, p: Option<DragPlane>);
    pub fn take_drag_distance(&mut self) -> Option<(f64, bool)>;  // new distance; true when the drag ended
    pub fn set_section_hatch(&mut self, on: bool);
}

// stl-app/src/tools/mod.rs
pub struct DragSpec { pub plane: &'static str, pub distance: &'static str, pub body: &'static str }
// ToolFlags gains `pub drag: Option<DragSpec>`; Split sets Some(DragSpec { plane: "plane", distance: "offset", body: "body" }).

// stl-app/src/engine_link/mod.rs
Command::BodyInfo { seq: u64, id: String }
Update::BodyInfo { seq: u64, result: Result<serde_json::Value, String> }

// stl-engine/src/info.rs: the 1.x `info` route's object
pub fn body_info(state: &State, id: &str) -> Result<serde_json::Value, FeatureError>;
// {"name", "solid": bool, "size": [x,y,z], "triangles", "area", "volume": f64|null, "genus": i64 (-1 unknown)}

// stl-platform/src/handoff.rs (desktop only)
pub const SLICERS: [(&str, &str); 4];                  // ("bambu","Bambu Studio"), ("orca","OrcaSlicer"), ("prusa","PrusaSlicer"), ("solidcraft","SolidCraft")
pub fn find_slicer(key: &str) -> Option<Vec<String>>;  // 1.x find_slicer, per platform
pub fn find_slicer_with(key: &str, os: &str, exists: &dyn Fn(&str) -> bool, which: &dyn Fn(&str) -> Option<String>, flatpak: &dyn Fn(&str) -> bool, listdir: &dyn Fn(&str) -> Vec<String>) -> Option<Vec<String>>;
pub fn open_in(cmd: &[String], path: &Path) -> Result<(), String>;
pub fn step_available() -> bool;
pub fn to_step(stl: &Path, step: &Path) -> Result<(), String>;

// stl-app/src/platform/mod.rs (defaults: nothing found, desktop-only message)
fn slicers(&self) -> Vec<(String, String)> { Vec::new() }
fn send_to_slicer(&self, key: &str, name: &str, bytes: Vec<u8>) -> Pending<Result<(), String>>;
fn step_available(&self) -> bool { false }
fn to_step(&self, suggested: &str, stl: Vec<u8>) -> Pending<Result<SaveOutcome, String>>;
fn launch_log(&self) -> Vec<String> { Vec::new() }   // test platforms record launches and dialogs here
```

- [ ] **Step 1:** Write the stubs above; `on_ribbon` sends `"section"` to `self.open_section()`, `"measure"` to `self.open_measure()`, `"info"` to `self.open_info(None)`, `"send"` to `self.open_send_menu(anchor)`, `"to_step"` to `self.to_step()`, each in its new file. The app, each frame, builds a `DragPlane` from the open tool's `ToolFlags::drag` (the plane field's frame, the distance field, the body's box) or from the open Section panel, passes it to `set_drag_plane`, and feeds `take_drag_distance` back with `tool_set_value` (one undo step per finished drag).
- [ ] **Step 2:** `cargo check --workspace --tests` and the wasm32 build are clean; every existing test still passes.

### Task 2: The drag control and Section View with hatch

**Owner:** Opus (Claude). **Review:** Nemotron (Haiku if the free models are out). Only this task edits `viewport/`.

**Files:** `viewport/plane_drag.rs`, `viewport/mod.rs`, `viewport/section.rs`, `viewport/renderer.rs`, `viewport/shaders/*`, `src/app_section.rs`, `src/ui/section_panel.rs`; tests `tests/plane_drag.rs`, `tests/ui/section_view.rs`, a WARP snapshot.

- [ ] **Step 1:** Tests in `tests/plane_drag.rs`: `start_distance_boundary_is_halfway` (20 mm cube, top face z = 20 normal +Z gives -10; a plane at z = 5 gives 0), `drag_follows_pointer_projection` (side view, dragging 100 px along the arrow moves the plane by 100 × pixel size within 1 %), `drag_face_on_falls_back` (normal toward the camera: finite, monotonic in vertical movement), `plane_eq_flip`.
- [ ] **Step 2:** Implement `plane_drag.rs`; draw the translucent plane, its outline, the arrow and the live distance box (current units); hover and drag colours as the Move gizmo.
- [ ] **Step 3:** The Section View panel per spec 2.3 (X/Y/Z, a picked plane or flat face, Distance, Flip, Hatch, the 1.x note, Remove cut, Done); X and the view cube open it when there is no cut (`toggleCut`). Hatch in the cap shader: diagonal lines in screen space, darker than the cap colour.
- [ ] **Step 4:** UI test `section_view`: the ribbon's Section button opens the panel and the cut is on; picking a side face and dragging moves the cut; Flip swaps the kept half; X turns it off and on; Remove cut clears it. A WARP snapshot of a hatched cut differs from Hatch off.
- [ ] **Step 5:** `EXCEPTIONS.md` lines: the plane can be a face (1.x: axis only), Hatch.

### Task 3: Split moves its cut

**Owner:** Sonnet A. **Review:** Hermes (Haiku if out).

**Files:** `rust/crates/stl-engine/src/features/split.rs`, `src/tools/defs/modify.rs` (Split's Distance field, `ToolFlags::drag`, the fresh-pick start in `on_change_split`), `tests/tools_golden.rs`, `tests/tool_picks.rs`; tests in `stl-engine/tests/` and `tests/ui/split_face.rs`.

- [ ] **Step 1:** Engine tests: `split_offset_by_hand` (20 mm cube, top face, offset -5: volumes 6000 and 2000 mm³), `split_without_offset_unchanged` (existing goldens), `split_offset_expression` (`"=h/2"`).
- [ ] **Step 2:** `split` reads `offset` (default 0) and moves the plane's origin along its normal. Stored only when not 0. The new message: "The plane doesn't cut through this body. Drag the arrow or change Distance to move it into the part."
- [ ] **Step 3:** The panel: Distance after Split plane; a fresh plane pick sets it with `plane_drag::start_distance`; editing from the timeline keeps it (`reopen_keeps_offset`).
- [ ] **Step 4:** UI test `split_face`: select a box, open Split, pick its side face, OK: two bodies. Update the two definition tests on purpose; `EXCEPTIONS.md` line.

### Task 4: Split and Pattern speed

**Owner:** Sonnet A, after Task 3. **Review:** Opus (Claude).

- [ ] **Step 1:** Quiet A/B bench runs as in `rust/tests/PERF_GAPS.md` (one at a time, nothing else running); profile Split and Pattern circular x12 with `MANIFOLD_TIMING=1`.
- [ ] **Step 2:** Fix what is slow without changing results (byte-for-byte checks as the join fix did; parity unchanged). Record before and after in `PERF_GAPS.md`; any row still above 1.0 is explained there and listed in `EXCEPTIONS.md`.

### Task 5: Measure and Properties

**Owner:** Sonnet B. **Review:** Particle (Haiku if out).

**Files:** `src/app_measure.rs`, `src/ui/measure_panel.rs`, `src/ui/info_panel.rs`, `rust/crates/stl-engine/src/info.rs`, the body-info command in the engine worker (desktop and web); tests `stl-engine/tests/info.rs`, `tests/ui/measure_info.rs`.

- [ ] **Step 1:** Engine tests: `info_cube` (20 mm cube: size 20×20×20, area 2400, volume 8000, genus 0, solid), `info_ring` (a ring: genus 1), `info_open_mesh` (not solid, volume null).
- [ ] **Step 2:** Implement `body_info` and the command; Properties panel with 1.x's rows, units and weights (PLA 1.24, PETG 1.27 g/cm³); clicking a body while it is open shows that body.
- [ ] **Step 3:** Measure: two picks, blue markers and a line, Distance and ΔX/ΔY/ΔZ; a third pick starts again.
- [ ] **Step 4:** UI test `measure_info`: the ribbon's Measure and Properties open their panels; two picks on the basic project's box give the expected distance; Properties of the box shows its volume in cm³ and in in³ in inch mode.

### Task 6: Send to Slicer and To STEP

**Owner:** Haiku. **Review:** Sonnet B.

**Files:** `rust/crates/stl-platform/src/handoff.rs`, `src/app_handoff.rs`, `src/platform/native.rs`, `src/platform/web.rs`; tests `stl-platform/tests/handoff.rs`, `tests/ui/handoff.rs`.

- [ ] **Step 1:** `find_slicer_rules`: 1.x's cases through `find_slicer_with` (Windows paths and folder search, Mac `.app`, Linux command and flatpak), copied from 1.x's tests in `tests/`.
- [ ] **Step 2:** Implement; the menu lists only found slicers (also File › Send to Slicer); a launch failure shows "Couldn't open <Slicer>: <reason>" (1.x's words if 1.x has them). To STEP as 1.x `r_to_step`, disabled without `solidcraft-cli.exe`. Web: 1.x's desktop-only message.
- [ ] **Step 3:** UI test with a recording platform: Send to Slicer › Bambu Studio records one launch with a 3MF path; To STEP with no CLI is disabled.

### Task 7: 1.x's update check skips 2.x

**Owner:** Haiku, after Task 6. **Review:** Hermes.

**Files:** `stleditor/updates.py`, `tests/test_updates.py` (or the existing update tests).

- [ ] **Step 1:** Tests with recorded release lists: `v2.0.0` newer than the installed 1.x is not offered; a newer `v1.14.0` still is; a list with only 2.x releases offers nothing.
- [ ] **Step 2:** List releases (not `releases/latest`) and take the newest `v1.*`. `python -m pytest -q tests` passes (with `C:\Program Files\Python314\python.exe`).

### Task 8: No silent buttons

**Owner:** Sonnet B, after Task 5 (it needs Tasks 2, 3, 5 and 6 in). **Review:** Opus (Claude).

**Files:** `tests/ui/no_silent_buttons.rs`, a recording test platform in `tests/ui/harness.rs`, and fixes where it finds gaps.

- [ ] **Step 1:** Walk every ribbon item on every tab, every tool-search command and every item of the body right-click menu, with the basic project open and a body selected; after each, check for a tool panel, menu, dialog, mode, toast, view change, or a recorded dialog or launch; then close it. Desktop-only items are skipped on the web build.
- [ ] **Step 2:** Fix anything it finds (or list it in `EXCEPTIONS.md` for Nathan with the reason).

### Task 9: Graphics driver reset in the real window

**Owner:** Opus (Claude), after Task 2.

- [ ] **Step 1:** Make the real window recover after a device loss (a new device and surface, the scene uploaded again). If eframe 0.37 can't, build the fallback in spec 2.6 (autosave at once, the message, Restart recovers without asking) and say why in `EXCEPTIONS.md`.
- [ ] **Step 2:** A test forces a device loss through the real window's code path.

### Task 10: Close-out

**Owner:** the lead, in `rust/target/verify`.

- [ ] Every earlier check (workspace tests, parity, goldens, `xtask pure-rust`, wasm32 builds and `xtask web --check --webgl2`, the snapshot tests under WARP, uibench, `python -m pytest -q tests`), and the new tests. Bench table in `PERF_GAPS.md`.
- [ ] `cargo run -p xtask -- install` with the window closed, so Nathan can try it. Report: result lines, bench table, what Task 8 found and fixed, which device-reset path Task 9 built. Don't commit or push.
