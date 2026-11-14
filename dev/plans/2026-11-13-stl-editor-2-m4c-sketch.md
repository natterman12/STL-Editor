# STL Editor 2.0 Milestone 4c (Sketch Mode, Last UI Tests, Look Pass) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 2.0's interface does everything 1.x's does:

- sketch mode as in 1.x: the plane prompt, drawing and modify tools, typed sizes, snapping, relations, Smart Dimension, Project, Use Edge, text, SVG/DXF import, Finish and Finish & Extrude, editing a sketch from the timeline
- the 9 remaining 1.x UI tests as `egui_kittest` counterparts
- the 320 ms iso fly-in, the look drift fixed, the web preview pause measured (and fixed if over about 0.3 s)

**Architecture:**

- `stl-app` gains `sketch/`:
  - `sketch/mod.rs`: the shared types and the `SketchTool` trait (Task 1, fixed)
  - `sketch/geom.rs`: a port of `sketchgeom.js` (Task 1)
  - `sketch/session.rs`, `panel.rs`, `view.rs`, `keys.rs`: sketch mode (Task 2)
  - `sketch/draw.rs`, `modify.rs`, `snap.rs`, `typed.rs`: drawing and editing (Task 3)
  - `sketch/text.rs`, `touch.rs`: the text dialog and drawing by taps (Task 4)
  - `sketch/relations.rs`, `dims.rs`, `solve.rs`: constraints and dimensions (Task 5)
  - `sketch/import.rs`, `project.rs`: import SVG/DXF, Project, Use Edge (Task 6)
- Sketch maths comes from `stl-sketch` (solver, ops, import2d, text) and `stl-mesh::refs`; 4c adds no geometry.
- The engine gets a `sketch_op` route behind `Command::SketchOp` (Task 6), as 1.x's `routes.js` lines 100–142.
- Only Task 2 edits `app.rs`, `app_wiring.rs` and `ui/ribbon.rs`. Other tasks expose functions Task 2 calls. Only Task 7 edits `viewport/`.

**Tech Stack:** As 4b. No new dependency without the lead's approval; every new dependency must pass `xtask pure-rust`.

**Spec:** `dev/specs/2026-11-13-stl-editor-2-m4c-sketch-design.md`, under `dev/specs/2026-11-12-stl-editor-2-m4-app-design.md`.

## Global Constraints

- **Every 4a and 4b Global Constraint still applies:** messages word for word; `EXCEPTIONS.md` (every 1.x difference "pending Nathan's OK"); pure Rust; the wasm32 builds; no commits or pushes; don't stop for decisions, park them; second-worker reviews for ports; only `viewport/` uses wgpu.
- **SolidCraft is read-only reference.** Copy nothing from it: no code, theme values, sizes, icons or fonts. A reviewer who finds copied SolidCraft code rejects the task.
- **The 1.x sources of truth:**

  | What | Where |
  |---|---|
  | Sketcher (tools, picks, drag, typed sizes, HUD, panel text, dimension drawing) | `web/js/sketcher.js` |
  | Sketch mode in the app (`beginSketch`, `enterSketch`, `openSketchPanel`, `renderSketchPanel`, `finishSketch`, `cancelSketch`, `sketchImport`, the sketch ribbon in `ribbonDef`) | `web/js/app.js` (lines 104–118, 1380, 1490–1760) |
  | Relations and dimension inference | `web/js/constraints.js` (ported in `stl-sketch/src/solver/`) |
  | Dimension layout | `web/js/dims.js` |
  | Entity geometry and labels | `web/js/sketchgeom.js` |
  | `sketch_op` | `web/engine/routes.js` lines 100–142, `stleditor/server.py` |
  | Sketch drawing in the view | `web/js/viewer.js` (`syncSketches`, `sketchLines`, `lookAtPlane`) |
  | Panel and HUD look | `web/css/*.css` |

- **Accepted egui limits are not drift** (Nathan, 2026-11-13): egui's drop-down list, solid dashed boxes, the flat focus ring, the icon art, the wgpu gizmo, rings and brush. Don't custom-draw them.
- **Out of scope:** sketch fillet, spline, ellipse; milestone 5 (autosave, persistent recent files, update check, browser drawer); `/app2/` publishing; version bumps.
- **Builds:** each worker sets `CARGO_TARGET_DIR` to `target/4c-t<N>` and runs cargo from `rust/`. Never create `rust/rust/` or `rust/crates/target`. Never run two cargo commands at once on one target folder. The lead verifies in `rust/target/verify` with `-j 6`. Don't delete any target folder.
- **Tests that pick** wait for the viewport first (`Viewport::flush`, or the harness's `settle`), or they race on slow machines.
- **Smoothness:** dragging a point in a 200-entity, 100-constraint sketch keeps every frame under 16.7 ms; entering and leaving a sketch on the bench mesh too; solving never stalls the UI thread for more than one frame.
- **Fonts:** every character the sketch interface shows must be in the bundled fonts. Extend `tests/fonts_fallback.rs`.

## Review Focus

1. **Editing a sketch from the timeline, then Cancel.** The timeline rolls back to the sketch while it is open; Cancel puts the previous rollback back and leaves the step's entities and constraints unchanged. Test: Task 2 `edit_then_cancel_restores`.
2. **Ctrl+Z inside a sketch** undoes the last sketch edit (`undoLocal`), never the document's last step; after Finish, Ctrl+Z undoes the sketch step. Test: Task 2 `ctrl_z_in_sketch_is_local`.
3. **A part edge the sketch measured from has changed.** It shows red, the "N part edges … have changed" toast appears word for word, and its dimensions hold to where it was. Test: Task 5 `missing_ref_warned`.
4. **A conflicting dimension** (over-defined). The definition line says over-defined in 1.x's words and colour, and the conflicting dimension is marked as 1.x marks it. Test: Task 5 `over_defined_reported`.
5. **Typed sizes in inch mode** ("1/2", "1 3/8") become dimensions in millimetres (12.7, 34.925) and show in inches. Test: Task 3 `typed_size_inch`.
6. **Finish on an empty sketch** says "The sketch is empty." and adds the step, as 1.x; Finish & Extrude with no closed profile adds the sketch but doesn't open Extrude. Test: Task 2 `finish_empty_and_open_profile`.

---

### Task 0: Golden, screenshot harness and UI test skeletons

**Owner:** Cursor. **Review:** Hermes (Haiku if Hermes is out). Runs from the start; Tasks 6 and 8 depend on it.

**Files:**
- **Create:**
  - `dev/parity/js/sketchop_golden.mjs` → `rust/tests/parity/sketchop/<case>.json`
  - `dev/m4c_shots/capture_m4c_shots.mjs` (1.x side), `rust/crates/stl-app/tests/m4c_shots.rs` (2.0 side)
  - `rust/crates/stl-app/tests/ui/` skeletons for `desktop_flow`, `desktop_new`, `desktop_sketch2`, `desktop_v11`, `skdim`, `skdim2`, `tap`, `web2`, marked `#[ignore]` until their task lands
- **Modify:** `.github/workflows/rust.yml`: run `node dev/parity/js/sketchop_golden.mjs --check` and the 2.0 `sketchop_golden` test

**Interfaces:**
- **`sketchop_golden.mjs`:** runs 1.x's `sketch_op` route in Node (`web/engine/routes.js`, with the fixture project loaded as `app.doc`) for a fixed case list. At least: `trim` (line crossing two lines, arc), `offset` (closed and open chains, negative distance), `mirror` (about a line, about an axis), `explode` (rect, polygon, slot), `project` (`section` and `silhouette`, construction on/off) and `refs` on the fixture box and a tipped part, `text` (each `align` and `anchor`, `angle` 30), `faces` (nested rectangles). Every case uses `seed: "g"` so ids are stable. Each file holds `{ "op", "data", "result" }`. `--check` regenerates in memory and fails on any difference.
- **Screenshots:** both sides reach the same states on the same fixture, dark and light, saved to `dev/out/m4c-shots/{1x,2x}/<theme>/<state>.png`: `plane_prompt`, `sketch_empty`, `draw_line`, `draw_arc`, `draw_rect`, `draw_circle`, `draw_polygon`, `draw_slot`, `sketch_under`, `sketch_full`, `sketch_over`, `modify_dialog`, `text_dialog`, `useedge_refs`, `missing_ref`, `inch_dims`, `finish_extrude`. `states_differ` applies. 2.0 states not built yet are skipped and listed, not faked.

- [ ] **Step 1:** Write `sketchop_golden.mjs` and generate the files. Run it twice. Expected: identical output.
- [ ] **Step 2:** Write both capture paths and capture the 1.x side now.
- [ ] **Step 3:** Write the 8 test skeletons from `UI_PARITY.md`, each with the 1.x file's steps as comments, and un-ignore nothing yet. Add the CI steps.

### Task 1: The fixed sketch interfaces

**Owner:** Sonnet A. **Review:** the lead. Short; Tasks 2–6 start as soon as it lands, against these names.

**Files:**
- **Create:** `rust/crates/stl-app/src/sketch/mod.rs`, `sketch/geom.rs`; stub files with the `tool_for` stub below in `sketch/{draw.rs,relations.rs,text.rs}`; empty modules `sketch/{session.rs,panel.rs,view.rs,keys.rs,modify.rs,snap.rs,typed.rs,touch.rs,dims.rs,solve.rs,import.rs,project.rs}`
- **Modify:** `rust/crates/stl-app/src/lib.rs` (`pub mod sketch;`), `engine_link/mod.rs` (the two variants below; the worker and web handling are Task 6)
- **Test:** `rust/crates/stl-app/tests/sketch_core.rs`

**Interfaces** (fixed; changes go through the lead):

```rust
// engine_link/mod.rs
Command::SketchOp { seq: u64, op: String, data: serde_json::Value }      // 1.x post('sketch_op', {op, ...data})
Update::SketchOp { seq: u64, result: Result<serde_json::Value, String> } // the route's JSON, or its error text

// sketch/mod.rs
pub use stl_sketch::entities::Entity;                 // 1.x JSON shape; unknown keys kept in `extra`
pub use stl_sketch::solver::{Constraint, Ref};
pub struct RefEnt { pub ent: Entity, pub missing: bool }   // a part edge (sketch_op refs), red when missing

pub struct SketchSession {
    pub frame: crate::viewport::PlaneFrame,
    pub plane_ref: serde_json::Value,            // the step's `plane`, as 1.x
    pub feature_id: Option<String>, pub name: Option<String>,
    pub entities: Vec<Entity>, pub constraints: Vec<Constraint>,
    pub refs: Vec<RefEnt>, pub ref_kind: Option<String>,
    pub sel: std::collections::BTreeSet<String>, pub sel_c: Option<usize>,
    pub tool: String,                             // 1.x tool keys: "select", "line", "arc3", "c_dimension", ...
    pub history: Vec<(Vec<Entity>, Vec<Constraint>)>,
    pub seed: String,                             // base-36 time, as Date.now().toString(36)
    next_id: u32,
}
impl SketchSession {
    pub fn new(frame: PlaneFrame, plane_ref: Value, feature_id: Option<String>, name: Option<String>,
               entities: Vec<Entity>, constraints: Vec<Constraint>, refs: Vec<RefEnt>, ref_kind: Option<String>) -> Self;
    pub fn new_id(&mut self) -> String;           // "e<seed><n>", skipping ids in use (routes.js newId)
    pub fn push(&mut self);                       // history before an edit
    pub fn undo_local(&mut self) -> bool;
    pub fn to_uv(&self, p: [f64; 3]) -> [f64; 2]; pub fn to_3(&self, uv: [f64; 2]) -> [f64; 3];
    pub fn used_refs(&self) -> Vec<serde_json::Value>;
    pub fn step_params(&self) -> serde_json::Value;   // {plane, entities, refs: used_refs, constraints}
}

pub enum SketchInput {
    Down { uv: [f64; 2], pos: egui::Pos2, mods: egui::Modifiers },
    Move { uv: [f64; 2], pos: egui::Pos2, mods: egui::Modifiers },   // hover and drag
    Up { uv: [f64; 2], pos: egui::Pos2 },
    DoubleClick { uv: [f64; 2], pos: egui::Pos2 },
    Key { key: egui::Key, mods: egui::Modifiers },
    Typed(String),                                 // typed size characters while drawing
}
pub enum SketchDialog { Modify(String), Text { group: Option<String> }, Offset, DimValue(usize) }
pub enum SketchEffect {
    Hint(String), Hud(Option<String>), Toast(crate::engine_link::Level, String),
    SketchOp { op: String, data: serde_json::Value },   // sent by Task 2 as Command::SketchOp
    Changed,                                            // entities/constraints changed: Task 2 re-solves and redraws
    Dialog(SketchDialog), Finish { extrude: bool }, Cancel,
}
pub struct ToolCx<'a> {
    pub s: &'a mut SketchSession, pub vp: &'a crate::viewport::Viewport,
    pub inch: bool, pub params: &'a std::collections::BTreeMap<String, f64>,
    pub out: &'a mut Vec<SketchEffect>,
}
pub trait SketchTool {
    fn key(&self) -> &'static str;
    fn hint(&self, s: &SketchSession) -> String;               // toolHint
    fn input(&mut self, cx: &mut ToolCx, ev: &SketchInput) -> bool;  // true = handled
    fn cancel(&mut self, cx: &mut ToolCx);                      // Escape, tool change
    fn picks_geometry(&self) -> bool;                           // pickingTool(): dimension labels let clicks through
    fn preview(&self, s: &SketchSession) -> Vec<crate::viewport::overlays::SketchOutline>;  // rubber band
    fn sketch_op_result(&mut self, _cx: &mut ToolCx, _op: &str, _result: &serde_json::Value) {}
}
/// One registry: draw.rs, relations.rs and text.rs each `pub fn tool_for(key: &str) -> Option<Box<dyn SketchTool>>`.
pub fn tool_for(key: &str) -> Option<Box<dyn SketchTool>>;

// sketch/geom.rs: sketchgeom.js, function for function
pub fn entity_outline(e: &Entity) -> Vec<[f64; 2]>;  pub fn arc_sweep(e: &Entity) -> f64;
pub fn arc_pts(e: &Entity, n: usize) -> Vec<[f64; 2]>; pub fn arc_from3(p1: [f64; 2], p2: [f64; 2], p3: [f64; 2]) -> Option<Entity>;
pub fn key_points(e: &Entity) -> Vec<(String, [f64; 2])>; pub fn dist_to_entity(e: &Entity, p: [f64; 2]) -> f64;
pub fn describe(e: &Entity) -> String;               // ENTITY_LABEL + describe()
pub fn dimensions(e: &Entity) -> Vec<(String, f64)>; pub fn apply_dimension(e: &mut Entity, key: &str, v: f64);
```

- [ ] **Step 1: Write the failing tests** in `sketch_core.rs`: `new_ids_follow_1x` (ids `e<seed>1`, `e<seed>2`, skipping one in use), `undo_local_restores`, `uv_round_trip` (a tilted frame, `to_3(to_uv(p)) == p` within 1e-9), `step_params_shape` (keys and order as `finishSketch` sends), `geom_matches_1x` (each `geom.rs` function against values computed with `sketchgeom.js` in Node for one entity of each type, recorded in the test).
- [ ] **Step 2:** Implement until they pass.
- [ ] **Step 3: Measure the solver.** Time `stl_sketch::solver::solve` on a generated sketch of 200 entities and 100 constraints (a grid of constrained rectangles), release build, 100 runs. Record p99 in the report. If p99 is over 8 ms (half a frame), tell the lead before Tasks 3 and 5 start: solving then moves off the UI thread (spec 2.7, decision 1) and the lead adds `Command::Solve` / `Update::Solved` here.

### Task 2: Sketch mode

**Owner:** Sonnet A. **Review:** Composer B.

**Files:**
- **Create:** `sketch/session.rs` (begin, enter, finish, cancel, edit from the timeline, the event dispatcher), `sketch/panel.rs` (`renderSketchPanel`, `openModify`), `sketch/view.rs` (plane grid, sketch drawing, rubber band, HUD), `sketch/keys.rs`
- **Modify:** `app.rs`, `app_wiring.rs` (sketch wiring, `sketcher_active()` returns the real state), `ui/ribbon.rs` (the sketch tab), `ui/timeline.rs` (edit opens sketch mode)
- **Test:** `rust/crates/stl-app/tests/sketch_mode.rs`

**Interfaces:**
- **Consumes:** Task 1; `relations::paint(ui, s, vp, inch) -> Vec<SketchEffect>` and `dims::paint(ui, s, vp, inch, params) -> Vec<SketchEffect>` from Task 5; `touch::input(...)` from Task 4.
- **Produces:** `StlApp::begin_sketch(plane: Option<Value>)`, `StlApp::enter_sketch(plane: Option<Value>, feature: Option<&TimelineItem>)`, `StlApp::finish_sketch(extrude: bool)`, `StlApp::cancel_sketch()`, `StlApp::sketch() -> Option<&SketchSession>`, `StlApp::sketch_set_tool(key: &str)` (tests and the ribbon use these).

**Behaviour** (as `app.js` 1490–1760 and `sketcher.js` `start`, `stop`, `setTool`, `onKey`, `redraw`, `showHud`):
- `begin_sketch(None)` shows the "Create Sketch" plane prompt in the tool panel (Start button, the info line word for word, base planes shown). A picked plane enters the sketch.
- `enter_sketch` sends `SketchOp refs`, flies the camera to face the plane (`lookAtPlane`), shows the missing-edges toast, hides the gizmo, swaps the ribbon to the sketch tab, opens the sketch panel (title "Sketch" or "Edit Sketch", button "Finish Sketch"), sets the hint. Editing rolls back to the step and remembers the previous rollback.
- The dispatcher turns viewport pointer events into `SketchInput` through `Viewport::ray_to_plane` and `to_uv`, and gives them to the active `SketchTool`; Select is the default tool. Orbit and pan stay with the viewport.
- After every `SketchEffect::Changed`: solve (Task 5's `solve::solve_all`), redraw, update the panel and the definition line.
- `finish_sketch` sends `AddFeature` (kind `sketch`) or `EditFeature` with `rollback_after` the remembered rollback; "The sketch is empty." toast on an empty one; Finish & Extrude opens Extrude with `sketch` set when there is a closed profile. `cancel_sketch` restores the rollback when editing.
- Keys as `sketcher.js` `onKey` and `app.js`: Escape (finish the line chain, cancel a draw, or leave the tool), Enter (finish a line chain), Delete, Ctrl+Z (`undoLocal`).
- The tool search (Ctrl+K) lists the sketch tools while a sketch is open (`app.js` line 165); Look At faces the plane again; the other sketches show as 1.x shows them (`syncSketches` with the active one).

- [ ] **Step 1: Write the failing tests:** `search_lists_sketch_tools`, `begin_prompts_for_a_plane`, `enter_from_face_and_origin_plane`, `finish_adds_sketch_step` (the step JSON equals `step_params`), `edit_then_cancel_restores`, `ctrl_z_in_sketch_is_local`, `finish_empty_and_open_profile`, `ribbon_is_sketch_tab` (the groups and labels of `ribbonDef` lines 105–118), `gizmo_hidden_in_sketch`.
- [ ] **Step 2:** Implement until they pass.

### Task 3: Drawing and modify tools

**Owner:** Opus (Claude). **Review:** Composer B.

**Files:**
- **Create/fill:** `sketch/draw.rs` (line, arc3, rect, rect_center, circle, polygon, slot), `sketch/modify.rs` (select, box select, drag, trim, offset, mirror, round corner, delete, construction), `sketch/snap.rs`, `sketch/typed.rs`
- **Test:** `rust/crates/stl-app/tests/sketch_draw.rs`

**Interfaces:**
- **Consumes:** Task 1 (`SketchTool`, `ToolCx`, `geom.rs`); `stl_sketch::solver::infer` for `autoConstrain`.
- **Produces:** `draw::tool_for` for every draw and modify key in the sketch ribbon; `snap::snap(s: &SketchSession, vp: &Viewport, uv: [f64; 2]) -> Snap { uv, kind, on: Option<Ref> }`.

**Behaviour** (`sketcher.js` `onClick`, `updateDraw`, `commit`, `addLinePoint`, `finishLine`, `readTyped`, `commitTyped`, `addTypedDims`, `autoConstrain`, `dragStart/Move/End`, `roundCorner`, `toggleConstruction`, `deleteSelected`):
- Clicks, rubber bands and finishing as 1.x for each tool; typed sizes become dimensions (`addTypedDims`) in display units (inch mode through `units`); `autoConstrain` adds the relations 1.x adds while drawing.
- Trim, offset and mirror go out as `SketchEffect::SketchOp` (`trim`, `offset`, `mirror`) and take the result in `sketch_op_result`; offset asks its distance through `SketchDialog::Offset`.
- Snapping: endpoints, midpoints, centres, part edges (`refs`), the grid, at 1.x's pixel radii.

- [ ] **Step 1: Write the failing tests:** one per draw tool (clicks in plane coordinates → the entity JSON 1.x makes for the same clicks, recorded from `sketcher.js` in Node or by hand from its code), `typed_size_mm`, `typed_size_inch`, `line_chain_finish` (double-click, Enter, Escape), `auto_constrain_horizontal_and_coincident`, `drag_point_resolves`, `trim_offset_mirror_send_ops`, `round_corner`, `construction_toggle`, `snap_priorities`.
- [ ] **Step 2:** Implement until they pass.

### Task 4: Text dialog and drawing by taps

**Owner:** Gemini (Cursor, limited). If Gemini runs out, the lead hands it to Opus (Claude) with what's done. **Review:** Haiku.

**Files:** fill `sketch/text.rs`, `sketch/touch.rs`. **Test:** `rust/crates/stl-app/tests/sketch_text_touch.rs`.

**Interfaces:** **Produces:** `text::tool_for("text")`; `text::dialog(ui, tk, s, state) -> Option<SketchEffect>` (the text panel: text, font, size, bold, italic, spacing, align, angle, anchor, as 1.x); `touch::input(ev: &egui::Event, ...) -> Option<SketchInput>` (tap = click, double-tap = double-click, double-tap a sketch step's profile in model mode = edit that sketch).

- [ ] **Step 1: Write the failing tests:** `text_sends_sketch_op_text` (the `data` 1.x sends), `text_edit_replaces_group`, `tap_draws_line`, `double_tap_enters_sketch`.
- [ ] **Step 2:** Implement until they pass.

### Task 5: Relations, dimensions and solving

**Owner:** Sonnet B. **Review:** Hermes (Haiku if Hermes is out).

**Files:** fill `sketch/relations.rs`, `sketch/dims.rs` (a port of `dims.js` plus `buildDims`, `placeDims`, `dimText`, `dimClass`, `editDimension`), `sketch/solve.rs`. **Test:** `rust/crates/stl-app/tests/sketch_dims.rs`.

**Interfaces:**
- **Consumes:** Task 1; `stl_sketch::solver::{solve, analyze, is_redundant, unsatisfied, make_constraint, infer_dimension}`.
- **Produces:** `relations::tool_for` for every `c_*` key (`CTOOLS`); `relations::paint(ui: &egui::Ui, s: &mut SketchSession, vp: &Viewport, inch: bool) -> Vec<SketchEffect>` (the glyphs, hover, click to select or delete); `dims::paint(ui: &egui::Ui, s: &mut SketchSession, vp: &Viewport, inch: bool, params: &BTreeMap<String, f64>) -> Vec<SketchEffect>` (labels, arrows, extension lines; drag to place; double-click to edit); `solve::solve_all(s: &mut SketchSession) -> SolveState { def: Def, msg: Option<String>, conflicts: Vec<usize> }` with `Def::{Under(u32), Full, Over}` and `solve::def_text(&SolveState) -> String` (`defText` word for word).

**Behaviour** (`sketcher.js` `constraintClick`, `dimClick`, `updatePending`, `prepDim`, `placeDimension`, `addConstraint`, `useEdge` refs, `solveAll`, `deleteConstraint`, `setDimValue`, `setDimRef`, `parseDim`, `updateDefinition`):
- Each relation tool's picks, hint ("… · N picked") and messages as 1.x.
- Smart Dimension on one or two picks, reference dimensions to part edges, `=expr` values through the project's parameters, inch display.
- Missing references in red with their dimensions held.

- [ ] **Step 1: Write the failing tests:** one per `CTOOLS` relation (picks → the constraint JSON 1.x makes), `smart_dimension_linear_radial_angle`, `dimension_to_part_edge`, `dimension_with_parameter`, `dimension_text_inch`, `dims_layout_matches_1x` (`dims.js` functions against Node values), `under_full_over`, `over_defined_reported`, `missing_ref_warned`, `delete_relation`.
- [ ] **Step 2:** Implement until they pass.

### Task 6: The `sketch_op` route, Project, Use Edge and import

**Owner:** Grok B. **Review:** Opus (Claude).

**Files:**
- **Create:** `rust/crates/stl-engine/src/sketch_op.rs` (`Document::sketch_op(op: &str, data: &Value) -> Result<Value, GeometryError>`), `rust/crates/stl-app/tests/sketchop_golden.rs`; fill `sketch/import.rs`, `sketch/project.rs`
- **Modify:** `engine_link/worker.rs` and `engine_link/web.rs` (handle `Command::SketchOp`), the web worker entry; `stleditor`-side nothing (1.x is the reference only)

**Interfaces:**
- **Produces:** `Document::sketch_op` for `trim`, `offset`, `mirror`, `explode`, `project`, `refs`, `text`, `faces`, answering the JSON `routes.js` answers; `import::import_file(name: &str, bytes: &[u8]) -> Result<Vec<Entity>, String>` (SVG or DXF by extension; "No lines or curves found in that file." word for word); `project::tool_for("sk_project")` and `("sk_useedge")` (`pickEdge`, refs).

- [ ] **Step 1: Write the failing test** `sketchop_golden`: every Task 0 case through `Document::sketch_op` gives identical JSON.
- [ ] **Step 2:** Implement until it passes, desktop and web (the wasm32 build answers through the worker).
- [ ] **Step 3: Write and pass** `import_svg_dxf_into_sketch` (the 1.x import2d fixtures land as the same entities), `project_section_and_silhouette`, `use_edge_adds_ref`, `regions_from_new_sketch` (Extrude's region field picks the faces `sketch_op faces` gives for a sketch made in 4c, as `app.js` line 1380).

### Task 7: Fly-in, look drift and the web preview pause

**Owner:** Worker 6 (Grok). **Review:** Opus (Claude). Only this task edits `viewport/`.

**Files:** `viewport/camera.rs`, `viewport/mod.rs` (fly-in; web preview slices), `ui/*.rs` and `tools/*.rs` for drift fixes (not `sketch/`); `rust/tests/EXCEPTIONS.md`.

- [ ] **Step 1:** The start-up fly-in: the camera flies to the iso view over 320 ms with 1.x's easing (`viewer.js`), and stops at once on user input. Test `start_flies_to_iso` (positions at 0, 160 and 320 ms).
- [ ] **Step 2:** Compare every 4a, 4b and 4c screenshot pair. List each drift (spacing, sizes, colours, text, sketch colours) in `dev/out/m4c-shots/drift.md` with the 1.x CSS value, then fix it. Accepted egui limits are not drift.
- [ ] **Step 3:** Measure the web large-preview pause: a 500,000-triangle preview in Chrome through `xtask web` (add a `--measure-preview` mode that reports the longest main-thread task). Record it in `EXCEPTIONS.md`. If it is over 300 ms, prepare the preview in slices across frames on wasm32, as the red overhangs are, and measure again.

### Task 8: The 9 UI tests, screenshots, smoothness and round trip

**Owner:** Cursor. **Review:** Hermes (Haiku if Hermes is out).

- [ ] **Step 1:** Fill in the 8 skeletons from Task 0 and un-park the sketch-and-Extrude edit in `desktop_tools`, as each task lands, asserting what the 1.x file asserts (step JSON, solved coordinates, panel and toast text, timeline). Canvas clicks go through `world_to_screen` of plane points; buttons and labels are found by AccessKit name. Set each row in `UI_PARITY.md`; none stays "planned".
- [ ] **Step 2:** The uibench rows "sketch drag" (a point in a 200-entity, 100-constraint sketch) and "enter sketch" (on the bench mesh). Extend `fonts_fallback.rs` to every sketch label, hint, toast and panel text.
- [ ] **Step 3:** The round trip check `sketch_round_trip`: a sketch drawn, constrained and dimensioned in 2.0 is saved to `.stlx`, loaded by 1.x's engine in Node, and rebuilds to the same profile (area and vertex count within 1e-6); the 1.x fixture projects with sketches load in 2.0 with identical step JSON.
- [ ] **Step 4:** Capture the 2.0 screenshots after Tasks 2–6 land. Write `dev/out/m4c-shots/index.md` with each pair and what still differs.

### Task 9: Close-out

**Owner:** the lead, in `rust/target/verify`.

- [ ] **Step 1:** Run:
  - `cargo test --workspace --release -j 6`, including the new tests and every kittest
  - the parity runner (0 failed, 0 pending), `sketchop_golden.mjs --check`, `tools_golden.mjs --check` and every other `--check` script
  - `xtask pure-rust`; the wasm32 builds; `xtask web --check` and `--check --webgl2`
  - the snapshot tests under WARP, as CI runs them
  - uibench three times with nothing else running
  - the 1.x `pytest`
- [ ] **Step 2:** Update `EXCEPTIONS.md` (every new 1.x difference "pending Nathan's OK"; the web preview measurement) and `UI_PARITY.md`.
- [ ] **Step 3:** Report:
  - the result lines and the uibench table (with the two sketch rows)
  - the solver timing from Task 1 and where solving runs
  - the web preview pause, before and after
  - the screenshot folder, with what still differs per state, and the drift list from Task 7
  - anything parked

  Do not commit.
