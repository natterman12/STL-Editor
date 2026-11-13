# STL Editor 2.0 Milestone 4b (Tools and Support Fins) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every ribbon tool works in 2.0 as it does in 1.x:

- all 41 tool panels, with live previews and timeline editing
- the move, rotate and scale gizmo, and the Sculpt brush
- the Support Fins panel, with coverage and Draw Walls

Offset is also made faster, with identical results.

**Architecture:**

- `stl-app` gains `tools/`:
  - `tools/defs/`: the data-driven registry, a port of `TOOLS`
  - `tools/panel.rs`: the floating tool panel
  - `tools/fields/`: one widget per field kind
  - `tools/preview.rs`: the debounced preview flow
  - `tools/gizmo.rs`
  - `tools/sculpt.rs`
- The Support Fins user interface goes in `stl-app/src/supports/`:
  - `panel.rs`, `coverage.rs`, `walls.rs`, `strength.rs`
- The tool panel talks to the engine only through `Command`s and `Update`s, as 4a does.
- Stream C changes only `stl-engine` (and `stl-mesh` or `stl-kernel` if the profile points there).

**Tech Stack:** As 4a. No new dependency without the lead's approval. Every new dependency must pass `xtask pure-rust`.

**Spec:** `dev/specs/2026-11-13-stl-editor-2-m4b-tools-design.md`, under `dev/specs/2026-11-12-stl-editor-2-m4-app-design.md`.

## Global Constraints

- **Every 4a and 4a-fixes Global Constraint still applies,** including:
  - messages word for word
  - `EXCEPTIONS.md` (every 1.x difference "pending Nathan's OK")
  - pure Rust; the wasm32 builds
  - no commits or pushes
  - don't stop for decisions; park them
  - second-worker reviews for ports
  - only `viewport/` uses wgpu
- **SolidCraft (`tangency-labs/solidcraft`) is read-only reference.** Copy nothing from it: no code, no theme values or sizes, no icons, no fonts. Don't depend on it or credit it. A reviewer who finds copied SolidCraft code rejects the task.
- **The 1.x sources of truth:**

  | What | Where |
  |---|---|
  | Tool definitions | `web/js/tools.js` |
  | Tool flow (`openTool`, `renderForm`, `fieldHTML`, `onFormChange`, `onFormClick`, `feedPick`, `schedulePreview`, `runPreview`) | `web/js/app.js` |
  | Support Fins (`supportTools`, `reopenSupports`, `sfStrength`, `quickPaint`, `sfStatusHTML`, coverage) | `web/js/app.js` |
  | Gizmo | `web/js/gizmo.js` |
  | Draw Walls | `web/js/drawwalls.js` |
  | Coverage | `web/js/sfcoverage.js`, `web/js/sfcoverage.worker.js` |
  | Expressions | `web/js/expr.js` |
  | Units | `web/js/units.js` |
  | Panel look | `web/css/*.css` |

  Match them; raise any mismatch egui forces in the report.
- **Support Fins settings are never hard-coded.** The panel is built from the engine's options schema, as 1.x does. Only values that differ from the defaults are saved.
- **Nathan's answers to the 4a parked items (2026-11-13).** Mark these accepted in `EXCEPTIONS.md` in Task 9:
  - a dropped `.stlx` is refused, as in 1.x
  - the section cut face shows the grid, as in 1.x
  - the WebGL2 check on Chrome's software renderer is enough for now
  - the iso fly-in moves to 4c
  - `/app2/` stays private
- **Builds (lessons from 4a):**
  - Each worker sets `CARGO_TARGET_DIR` to `target/4b-t<N>` and runs cargo from `rust/`.
  - Never create `rust/rust/`. Never build in `rust/release` or another worker's folder.
  - Never run two cargo commands at once on the same target directory.
  - The lead verifies in `rust/target/verify` with `-j 6`.
  - Don't delete any target directory.
- **Smoothness:**
  - Opening a tool and its first preview: no frame over 16.7 ms.
  - Gizmo drag on the bench mesh: 60 fps or more.
  - Coverage, preview results and mesh uploads never block the UI thread.
- **Fonts:** every character the new panels show must be in the bundled fonts. Extend `tests/fonts_fallback.rs`.

## Review Focus

1. **A preview that arrives after a newer one, or after the tool is closed.** The newer one wins; a late preview after Cancel or OK draws nothing. Test: Task 2 `stale_preview_dropped`.
2. **Editing an earlier step from the timeline.** The tool opens with that step's params through `fromParams`, the timeline rolls back to it while the panel is open, and OK or Cancel restores the previous rollback, as `openTool` does. Test: Task 3 `edit_rolls_back_and_restores`.
3. **An expression in a number field** (`=width/2`), and an invalid one. The field takes the `--expr` colour and the expression reaches the params as the string; an invalid one shows 1.x's error text and blocks OK. Test: Task 2 `expression_fields`.
4. **The gizmo dragged while a preview is still running.** The part follows the mouse at once; the late preview doesn't snap it back (as 1.x checks `gizmo.tc.dragging`). Test: Task 4 `drag_during_preview`.
5. **Coverage on the bench mesh.** It runs off the UI thread, can be cancelled by a new change, and gives the same bare spots as 1.x on the fixture models. Test: Task 6 `coverage_matches_1x`.
6. **Offset after the speed pass.** Bit-identical meshes on every parity case, golden check and kernel corpus case. Test: Task 7 `offset_unchanged`.

---

### Task 0: Tool golden, screenshot harness and UI test skeletons

**Owner:** Cursor. **Review:** Hermes (Haiku if Hermes is out of requests). Runs from the start; Tasks 1 and 8 depend on it.

**Files:**
- **Create:**
  - `dev/parity/js/tools_golden.mjs` → `rust/tests/parity/tools/<tool>.json`
  - `dev/m4b_shots/capture_m4b_shots.mjs` (1.x side)
  - `rust/crates/stl-app/tests/m4b_shots.rs` (2.0 side)
  - `rust/crates/stl-app/tests/ui/` skeletons for the 12 4b files, marked `#[ignore]` until their task lands
- **Modify:**
  - `.github/workflows/rust.yml`: run `node dev/parity/js/tools_golden.mjs --check` and the 2.0 tools golden test

**Interfaces:**
- **`tools_golden.mjs`:**
  - Imports `web/js/tools.js` in Node (it loads with no DOM).
  - Builds a fixed `ctx` from a fixture project in `tests/ui/fixtures`: `state` (bodies, planes, sketches, features), `selection`, `lastSketch`, `sketchOnBody`.
  - For each of the 41 tools it writes:
    - `defaults(ctx)`
    - `toParams(defaults)`
    - one case per `select` option and per `bool`
    - one case per `onChange` trigger (hole presets, thread sizes, printers, connectors, resize sizes)
    - `fromParams(toParams(v))` round trips
    - which fields `show` for each case
  - Also writes `supportfins` and `supportwall` built as `supportTools` builds them, with the schema read from the bundled engine through `web/engine` in Node. If that can't run in Node, park it and Task 5's test reads the schema from the engine instead.
  - `--check` regenerates the files in memory and fails on any difference.
- **Screenshots:**
  - `capture_m4b_shots.mjs` and `m4b_shots.rs` reach the same states on the same fixture, dark and light, and save them to `dev/out/m4b-shots/{1x,2x}/<theme>/<state>.png`:
    - each tool's panel open on its default selection
    - Move with the gizmo in move and in rotate
    - Sculpt with the brush over the part
    - Support Fins in Rotate, Add fins and Remove fins, with coverage on
    - Draw Walls with a line and a curve drawn
  - Both sides apply `states_differ`: no two states identical within one theme and edition. Wait for each state to finish drawing before capturing.
  - 2.0 states not built yet are skipped and listed, not faked.

- [ ] **Step 1:** Write `tools_golden.mjs` and generate the golden files. Run it twice. Expected: identical output.
- [ ] **Step 2:** Write both capture paths and capture the 1.x side now.
- [ ] **Step 3:** Write the 12 test skeletons from `UI_PARITY.md`, each with the 1.x file's steps as comments. Add the CI steps.

### Task 1: The tool registry

**Owner:** Worker 7 (Composer). **Review:** Haiku. Depends on Task 0's golden files; Tasks 2, 3 and 5 build on its types, which are fixed here so they can start at once.

**Files:**
- **Create:**
  - `rust/crates/stl-app/src/tools/mod.rs`
  - `rust/crates/stl-app/src/tools/defs/{mod.rs,tables.rs,model.rs,print.rs,mesh.rs,planes.rs}` (split by ribbon tab; the exact split is up to the worker)
  - `rust/crates/stl-app/tests/tools_golden.rs`

**Interfaces** (fixed; changes go through the lead):

```rust
pub type Values = serde_json::Map<String, serde_json::Value>;

pub struct Ctx<'a> {
    pub bodies: &'a [BodyView], pub planes: &'a [PlaneView], pub sketches: &'a [SketchView],
    pub features: &'a [TimelineItem], pub selection: &'a [String],
    pub last_sketch: Option<&'a str>, pub sketch_on_body: bool,   // as `ctx()` in app.js line 581
}
// `TimelineItem` gains `params: serde_json::Value` (1.x `f.params`) here, because
// `repeatableFeatures` and some `defaults` read it.

pub enum FieldKind {
    Number { unit: Option<&'static str> }, Select { options: Vec<(String, String)>, groups: Vec<(String, Vec<(String, String)>)> },
    Bool, Vec3 { unit: Option<&'static str> }, Text { placeholder: Option<String> },
    Bodies { multi: bool }, Plane { any_face: bool }, Sketch, Regions, Features, FacePoints, PFace, CylFace, Points3, AxisLine,
}

pub struct Field {
    pub key: &'static str, pub label: &'static str, pub kind: FieldKind,
    pub help: Option<&'static str>, pub show: Option<fn(&Values) -> bool>,
}

pub struct ToolFlags { pub place_on_face: bool, pub no_live_preview: bool, pub gizmo: bool,
                       pub plane_tool: bool, pub sculpt: bool, pub paint: bool, pub wall_tool: bool }

pub struct ToolDef {
    pub key: &'static str, pub label: &'static str, pub icon: &'static str,
    pub kind: &'static str,          // the engine feature type (`type` in tools.js)
    pub fields: Vec<Field>, pub hint: Option<&'static str>, pub extra: Option<&'static str>,
    pub flags: ToolFlags,
    pub defaults: fn(&Ctx) -> Values,
    pub to_params: fn(&Values, &Ctx) -> serde_json::Value,
    pub from_params: fn(serde_json::Value) -> Values,
    pub on_change: Option<fn(&str, &Values, &Ctx) -> Option<Values>>,  // returns a patch
}

pub fn tools() -> &'static [ToolDef];
pub fn tool(key: &str) -> Option<&'static ToolDef>;
```

- Tables are ported value for value, including label text and order:
  - `HOLE_PRESETS`, `THREAD_SIZES`, `RESIZE_SIZES`
  - `PRINTER_OPTS`, `CONNECTOR_OPTS`, `CONNECTOR_DEF`
  - `OPS`, `AXES`, `AXV`
  - `REPEATABLE`, `repeatableFeatures`, `selBody`
- If a JS rule depends on JS behaviour (number formatting, `||` on `0`, array or string coercion), match JS's result and comment the line.

- [ ] **Step 1: Write the failing test** `tools_golden`. It runs every case in `rust/tests/parity/tools/` through the registry and compares the JSON exactly, numbers included.
- [ ] **Step 2:** Port the tools until every case passes. Run `cargo test -p stl-app --test tools_golden`. Expected: 41 tools, 0 failed.

### Task 2: Tool panel, plain fields and the preview flow

**Owner:** Sonnet A. **Review:** Composer B.

**Files:**
- **Create:**
  - `rust/crates/stl-app/src/tools/{panel.rs,preview.rs}`
  - `rust/crates/stl-app/src/tools/fields/{mod.rs,number.rs,select.rs,bool.rs,vec3.rs,text.rs}`
  - `rust/crates/stl-app/tests/tool_panel.rs`
- **Modify:**
  - `rust/crates/stl-app/src/engine_link/{mod.rs,worker.rs,native.rs,web.rs}`:
    - add `Command::Preview { seq: u64, kind: String, params: Value, edit_id: Option<String> }` and `Update::Preview { seq: u64, pack: PreviewPack, msg: Option<String>, error: Option<String> }` (4a's `viewport::PreviewPack`; on web the meshes cross as transferable buffers)
    - add `Command::ClearPreview`
    - previews never enter undo history
  - `rust/crates/stl-app/src/app.rs`: an open tool's state

**Behaviour:**
- **Panel:** as `#tool-panel` in 1.x. Icon, title (`Edit ` prefix when editing), close, form, message line, credit, Cancel, OK; position and size from 1.x's CSS.
- **Opening:** `openTool` as `app.js` line 588:
  - values from `defaults(ctx)` or `fromParams`
  - when editing, roll back to that step and remember the previous rollback
  - first preview at once (`schedulePreview(0)`)
- **Plain fields:** `fieldHTML` and `renderForm` for `number`, `select` (grouped included), `bool`, `vec3`, `text`:
  - `show` rules; `help` text
  - units through `units.js`' rules (inch mode changes only what is shown)
  - `onChange` patches, then re-render as 1.x does
- **Expressions:** a number field accepts `=expr`. It is shown in the `--expr` colour, sent as the string, and checked against the project's parameters with `expr.js`' rules and messages.
- **Preview:**
  - 280 ms after the last change
  - only the newest `seq` lands; a preview for a closed tool is dropped
  - the "used" text and errors in the message line, word for word
  - `noLivePreview` tools show the Preview button and preview only on it
  - preview meshes through `Viewport::show_preview(&PreviewPack)`, with its hide, removed, colour and ghost lists
- **OK and Cancel:** OK sends `AddFeature` or `EditFeature` and closes; Cancel clears the preview and restores the rollback. Escape is Cancel, Enter is OK, as `onKey` does.

- [ ] **Step 1: Write the failing tests:**
  - `opens_with_defaults`
  - `show_rules_hide_fields`
  - `on_change_patches_values` (hole preset, thread size)
  - `inch_mode_shows_inches_sends_mm`
  - `expression_fields`
  - `stale_preview_dropped`
  - `no_live_preview_button`
  - `ok_adds_feature`, `cancel_restores`
  - `open_tool_frame_under_budget`
- [ ] **Step 2:** Implement until they pass. Add the panel to the snapshot tests, dark and light.

### Task 3: Pick fields, timeline edit, ribbon, search and shortcuts

**Owner:** Sonnet B. **Review:** Composer B.

**Files:**
- **Create:**
  - `rust/crates/stl-app/src/tools/fields/{bodies.rs,plane.rs,sketch.rs,regions.rs,features.rs,facepoints.rs,pface.rs,cylface.rs,points3.rs,axisline.rs}`
  - `rust/crates/stl-app/tests/tool_picks.rs`
- **Modify:**
  - `rust/crates/stl-app/src/ui/{ribbon.rs,timeline.rs,keys.rs,topbar.rs}` (tool search)

**Behaviour:**
- **Pick fields:** each kind as `fieldHTML`, `onFormClick` and `feedPick` handle it:
  - the active field, its chip list and remove buttons
  - what a click in the viewport adds
  - the overlays 1.x draws for it (face highlight, markers, arrows, plane ghost, regions)
  - All picking goes through 4a's `Viewport::pick` and overlays.
- **`placeOnFace` and `planeTool`:** clicking a body face fills the plane field, as `app.js` around line 1413 does.
- **Timeline edit:** double-click or the menu's Edit opens the tool for that step (Support Fins steps go to Task 5's `reopen_supports`).
- **Opening tools:**
  - ribbon buttons open their tools, through `ribbon.rs`' existing action keys and 1.x's dispatch (`app.js` lines 330–350)
  - Rotate opens Move with `_gizmo: 'rotate'`
  - tool search (Ctrl+K) lists every tool, as `#tool-search` does
  - the keyboard shortcuts that open tools
- **Repeat:** the `REPEATABLE` behaviour that re-runs the last tool, as 1.x.

- [ ] **Step 1: Write the failing tests:**
  - one per pick kind (`pick_bodies`, `pick_plane`, … `pick_axisline`)
  - `place_on_face_fills_plane`
  - `edit_rolls_back_and_restores`
  - `ribbon_opens_every_tool`
  - `search_finds_every_tool`
  - `rotate_opens_move_gizmo`
- [ ] **Step 2:** Implement until they pass.

### Task 4: Gizmo, Sculpt brush and overhang overlay

**Owner:** Opus (Claude). **Review:** Worker 6.

**Files:**
- **Create:**
  - `rust/crates/stl-app/src/tools/{gizmo.rs,sculpt.rs}`
  - `rust/crates/stl-app/src/viewport/{gizmo_draw.rs,overhang.rs}`
  - `rust/crates/stl-app/tests/gizmo_sculpt.rs`
- **Modify:**
  - `rust/crates/stl-app/src/viewport/{mod.rs,renderer.rs,overlays.rs}`

**Behaviour:**
- **Gizmo,** as `gizmo.js`:
  - move arrows, rotate rings, scale boxes, in the colours and sizes 1.x shows
  - rotate snaps to 5°, and Shift rotates freely
  - Land on bed
  - while dragging, the part moves on screen at once through a per-body transform, with no new mesh upload
  - letting go writes the numbers into the Move or Scale fields, and the preview follows
  - `quatOf` and `rotOf` use ZYX Euler order, as 1.x does
  - a late preview never snaps the part back while dragging
- **Sculpt:** `setBrush` (the brush ring on the surface), `drawStrokes`, and mouse mode `Sculpt` (left for the brush, middle pans, right rotates).
- **Overhang overlay:** draws the faces `overhangFaces` returns in 1.x's red, at once, for Task 5. It's an overlay, not a mesh rebuild.

- [ ] **Step 1: Write the failing tests:**
  - `gizmo_move_writes_fields`
  - `rotate_snaps_5_deg`, `shift_rotates_free`
  - `quat_rot_round_trip_zyx` (against values from `gizmo.js` run in Node)
  - `land_on_bed`
  - `drag_during_preview`
  - `sculpt_brush_follows_surface`
  - `overhang_overlay_draws`
- [ ] **Step 2:** Implement until they pass. Add a uibench row, "gizmo drag": 5 s on the bench mesh. Expected: 60 fps or more, 99th percentile under 16.7 ms.

### Task 5: Support Fins panel

**Owner:** Grok B. **Review:** Composer B.

**Files:**
- **Create:**
  - `rust/crates/stl-app/src/supports/{mod.rs,panel.rs,strength.rs}`
  - `rust/crates/stl-app/tests/supports_panel.rs`
- **Modify:**
  - `rust/crates/stl-app/src/ui/ribbon.rs` (the SUPPORTS tab)
  - `rust/crates/stl-app/src/engine_link/*`: add `Command::SupportFinsInfo` and the update carrying the schema, if 4a's link doesn't give it already

**Behaviour:**
- **Definitions:** `supportTools` (`app.js` lines 355–460), ported:
  - the `supportfins` and `supportwall` definitions, built from the engine's options schema
  - the `sf:` field keys, visibility rules, defaults, `toParams`, `fromParams`
  - only changed values saved
  - It uses Task 1's `ToolDef` type, built at run time.
- **Panel:**
  - the mode buttons: Rotate, Add fins, Remove fins
  - the Held card
  - `sfStatusHTML`'s status text
  - folding sections remembered for the session
  - the footer credit, word for word
- **Actions:**
  - `reopenSupports`: opening Support Fins on a body that already has it edits that step instead of stacking a new one
  - `quickPaint`: instant red overhangs through Task 4's overlay
  - the fins-only preview
  - `sfStrength` and suggested orientations
  - `_mode` and `_load` changes as `onFormChange` handles them (`app.js` lines 825–827)
- **SUPPORTS tab:** every tool and menu 1.x shows there, including About Support Fins.

- [ ] **Step 1: Write the failing tests:**
  - `schema_builds_panel` (and the golden from Task 0 if it was recorded)
  - `only_changed_values_saved`
  - `modes_switch`, `held_card`
  - `sections_remembered`
  - `edits_not_stacks`
  - `instant_overhangs`
  - `strength_and_suggest`
- [ ] **Step 2:** Implement until they pass.

### Task 6: Coverage and Draw Walls

**Owner:** Gemini (Cursor, limited). If Gemini reaches its limit, the lead hands the task to Grok B with what's done. **Review:** Haiku.

**Files:**
- **Create:**
  - `rust/crates/stl-app/src/supports/{coverage.rs,walls.rs}`
  - `rust/crates/stl-app/tests/supports_coverage.rs`
- **Modify:**
  - the web worker entry in `rust/crates/stl-app/src/web/` (coverage in a Web Worker)

**Behaviour:**
- **Coverage:** `sfcoverage.js` and `sfcoverage.worker.js`, ported:
  - held and bare overhang points within reach
  - "Full coverage" and "Bare beyond"
  - the status line
  - Runs on a background thread on desktop and in a Web Worker on web. A newer change cancels the running check.
- **Draw Walls:** `drawwalls.js`, ported:
  - the shapes line, multi-line, curve, gusset, anchor and drop fin
  - click rules, previews, hints and colours
  - the shape remembered between uses
  - the walls written to the tool's `walls` field

- [ ] **Step 1: Write the failing tests:**
  - `coverage_matches_1x`: bare points on the fixture models equal those from `sfcoverage.js` run in Node
  - `coverage_off_ui_thread`
  - `newer_change_cancels`
  - one test per wall shape
  - `wall_shape_remembered`
- [ ] **Step 2:** Implement until they pass.

### Task 7: Offset speed pass

**Owner:** Worker 6 (Grok). **Review:** Opus (Claude).

**Files:** whatever the profile points to in `stl-engine`, `stl-mesh` or `stl-kernel`. Record the result in `rust/tests/PERF_GAPS.md`.

- [ ] **Step 1:** Profile Offset 0.2 mm on the bench mesh, the case `PERF_GAPS.md` records. Write down where the time goes.
- [ ] **Step 2:** Write the test `offset_unchanged` first. It hashes the Offset output on every parity case, golden check and kernel corpus case that uses Offset or the code you'll change, against hashes taken before any change.
- [ ] **Step 3:** Make it faster without changing any result. Prefer algorithmic fixes (spatial index, fewer passes, less allocation) and rayon on desktop; keep the wasm32 build single-threaded and correct.
- [ ] **Step 4:** Run `xtask bench` with nothing else running. Expected: Offset at or below 1.0 against 1.x's 12.5–25.3 s, or the new numbers and the profile in `PERF_GAPS.md`. Run the full parity runner. Expected: 0 failed.

### Task 8: The 12 UI tests, screenshots and smoothness rows

**Owner:** Cursor. **Review:** Hermes (Haiku if Hermes is out of requests).

- [ ] **Step 1:** Fill in the 12 test skeletons from Task 0 as each task lands, testing what the 1.x file tests:
  - `desktop_mirror`, `desktop_place`, `desktop_tools`
  - `gizmo`
  - `sf11`, `sf9`, `sfcov`, `sfpanel`
  - `touch`
  - `v110`
  - `web7`, `web8`

  Set each row's status in `UI_PARITY.md`.
- [ ] **Step 2:** Add the uibench rows "open tool" (open each tool once, first preview) and "gizmo drag" (from Task 4). Extend `tests/fonts_fallback.rs` to every label, help text, hint and message in the tool and Support Fins panels.
- [ ] **Step 3:** Capture the 2.0 screenshots after Tasks 2–6 land. Write `dev/out/m4b-shots/index.md` with each state's pair and what still differs.

### Task 9: Close-out

**Owner:** the lead, in `rust/target/verify`.

- [ ] **Step 1:** Run:
  - `cargo test --workspace --release -j 6`
  - the kittest tests, including the 12 new ones
  - the parity runner (0 failed, 0 pending), `tools_golden.mjs --check` and every other `--check` script
  - `xtask pure-rust`
  - the wasm32 builds, `xtask web --check` and `xtask web --check --webgl2`
  - the snapshot tests under WARP, as CI runs them
  - uibench three times with nothing else running
  - `xtask bench` (the Offset row)
  - the 1.x `pytest`
- [ ] **Step 2:** Update `EXCEPTIONS.md`: mark Nathan's 4a answers accepted and add every new 1.x difference "pending Nathan's OK". Update `UI_PARITY.md`.
- [ ] **Step 3:** Report:
  - the result lines
  - the uibench table
  - the Offset timing
  - the screenshot folder, with what still differs per state
  - every look mismatch egui forced
  - anything parked

  Do not commit.
