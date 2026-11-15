# STL Editor 2.0 Milestone 7 (Test-build fixes, SolidCraft-style navigation and sketching) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Everything Nathan found in the M6 test build works: no crash from Support Fins / Draw Fins, SolidCraft's orbit and Fusion mouse buttons, SolidCraft-style sketching (inline sizes with Tab, tracking guides, coordinates, zoom-following grid snap, constraint icons, clear colours), exact Hole/Thread placement from sketch lines and part edges, Measure's triangle, Section View without red or grid, square Shell corners, highlights that follow and clear, Move snapping with sketches following, planes shown when needed, bodies pickable in the viewport, a reliable tree, a clear Living Hinge, and the cut preview tint.

**Architecture:**

- The crash: a vendored egui-wgpu with the staging-write panic turned into a reported loss; M6's restart path then runs in the real window.
- Navigation in a new `viewport/navigation.rs` (pivot and zoom anchor choices, Fusion bindings) on top of `camera.rs`; settings through the existing `Platform::setting_json`.
- Sketching: inline value boxes in a new `sketch/inline_dims.rs`, guides and grid snap in `sketch/guides.rs`, constraint glyphs in `sketch/glyphs.rs`; the fixed HUD goes.
- Placement: references resolved in the engine (`stl-engine/src/features/common/placement.rs`), picked in a new field `tools/fields/place_refs.rs`.
- Sketch following: the engine transforms a body's sketches when that body is moved (`State::record_transform`).
- The lead wires `app.rs` / `app_wiring.rs` once in Task 1, so workers never share them.

**Tech Stack:** As milestone 6. The only new code from outside is the vendored egui-wgpu 0.37.0 (same licence as today's dependency). Every new dependency must pass `xtask pure-rust`.

**Spec:** `dev/specs/2026-11-15-stl-editor-2-m7-testing-fixes-design.md`.

## Global Constraints

- **Every milestone 4–6 Global Constraint still applies:** pure Rust; the wasm32 builds; no commits or pushes; don't stop for decisions, park them; second-worker reviews; only `viewport/` uses wgpu; tests never touch Nathan's real folders (`STLEDITOR2_CONFIG_DIR`); new `Update`/`Command` variants get arms in `crates/stl-app/tests/web_worker.rs`.
- **SolidCraft is a behaviour reference only.** No code, theme values, icons or fonts from it, no dependency, no credit.
- **1.x differences** go in `rust/tests/EXCEPTIONS.md` under "Milestone 7" as "Accepted by Nathan (2026-11-15)" when the spec decides them, else "pending Nathan's OK".
- **Older projects open and run unchanged** (golden round trip); every new stored field is optional.
- **Both editions:** everything works in the web build (WebGL2 and WebGPU) except the Support Fins host.
- **Builds:** each worker sets `CARGO_TARGET_DIR` to `rust/target/m7-t<N>` and runs cargo from `rust/` with `-j 6`; the lead verifies in `rust/target/verify`. Disk is tight: delete your `m7-t<N>` folder when your task is approved.
- **Publish nothing:** no tag, release, `/app2/`, website or version bump.

## Review Focus

1. **A zoom toward empty space, then an orbit.** The part stays in view and the turn centre is never behind the camera or far off. Test: Task 3 `zoom_into_empty_space_then_orbit_keeps_part`.
2. **Two candidate edges after a rebuild** (a plate whose opposite edges are both parallel and equally near the stored line). The reference counts as missing; the hole keeps its point and the panel says so; nothing moves silently. Test: Task 5 `ambiguous_edge_is_missing`.
3. **Typing a size, then moving the mouse.** A locked box keeps its typed value while the cursor moves; only unlocked boxes follow. Test: Task 4 `locked_box_ignores_cursor`.
4. **A body moved, then its sketch reopened and edited.** Edits stay on the moved face, and the extrude after the move lands on it. Test: Task 8 `edit_followed_sketch_after_move`.
5. **A tool closed with Escape mid-pick.** Every highlight it made is gone and the origin planes return to how they were. Test: Task 7 `escape_clears_highlights`, Task 9 `escape_restores_planes`.

---

### Task 1: Fixed interfaces and the app wiring

**Owner:** the lead. Short; Tasks 2–10 start as soon as it lands.

**Files:**
- **Create:** `stl-app/src/viewport/navigation.rs`, `stl-app/src/sketch/inline_dims.rs`, `stl-app/src/sketch/guides.rs`, `stl-app/src/sketch/glyphs.rs`, `stl-app/src/tools/fields/place_refs.rs`, `stl-engine/src/features/common/placement.rs`, `stl-mesh/src/voxel/square.rs` (bodies `todo!()` or empty where marked)
- **Modify:** `stl-app/src/app.rs`, `stl-app/src/app_wiring.rs`, `stl-app/src/viewport/scene.rs` (colour constants), `stl-app/src/ui/mod.rs` (`UiEvent`), `stl-engine/src/state.rs` (or wherever `State` lives), `stl-engine/src/features/common/mod.rs`, `tests/web_worker.rs`

**Interfaces** (fixed; changes go through the lead):

```rust
// stl-app/src/viewport/navigation.rs (Task 3)
pub enum OrbitAround { ViewCentre, Origin, Cursor }            // setting "orbit_around": "view_centre" | "origin" | "cursor"
pub enum NavAction { Orbit, Pan }
pub fn nav_action(mode: MouseMode, button: egui::PointerButton, shift: bool) -> Option<NavAction>; // replaces camera::drag_action
pub fn orbit_pivot(cam: &Camera, around: OrbitAround, scene: Option<Aabb>, under_cursor: Option<V3>, kept: Option<V3>) -> V3;
pub fn zoom_anchor(cam: &Camera, cursor_ray: (V3, V3), surface_hit: Option<V3>) -> V3; // never behind the eye or far off
// camera.rs gains: pub fn orbit_about(&mut self, pivot: V3, dx: f64, dy: f64); pub fn zoom_at(&mut self, s: f64, anchor: V3);

// stl-app/src/viewport (Task 7)
impl Viewport {
    pub fn set_face_highlight(&mut self, tag: &str, body: &str, tris: Vec<u32>, color: u32); // drawn from the body's current mesh and transform
    pub fn clear_tool_highlights(&mut self);                                                // every highlight a tool made
    pub fn set_cut_tint(&mut self, id: &str, mesh: Option<std::sync::Arc<MeshData>>);      // translucent red cutter shape
    pub fn set_measure(&mut self, m: Option<(V3, V3)>);                                     // line, X/Y/Z dotted legs, labels
}
// scene.rs (lead, values per spec 2.3): SKETCH_LINE_DARK = 0xf2f2f2, SKETCH_LINE_LIGHT = 0x1a1a1a,
// SKETCH_CONSTRUCTION = 0x8a8a8a, SKETCH_CONSTRAINED_DARK = 0xb8d4ff, SKETCH_CONSTRAINED_LIGHT = 0x1f3f7a,
// PLANE_SKETCH_ALPHA = 0.18; CUT_TINT = 0xe03131 at alpha 0.35.

// stl-app/src/sketch/inline_dims.rs (Task 4)
pub struct DimBox { pub label: &'static str, pub value: String, pub locked: bool, pub anchor: P2, pub along: P2 }
impl DrawTool { pub fn dim_boxes(&self) -> Vec<DimBox>; pub fn total_label(&self) -> Option<String>; } // replaces hud()/set_hud()

// stl-app/src/sketch/guides.rs (Task 4)
pub struct Guide { pub from: P2, pub to: P2 }
pub fn track(cursor: P2, points: &[P2], tol: f64) -> (P2, Vec<Guide>);
pub fn visible_grid_step(px_per_mm: f64) -> f64;   // the spacing the grid draws at this zoom (1-2-5 steps)

// stl-engine/src/features/common/placement.rs (Task 5)
pub enum PlaceRef {
    SketchLine { sketch: String, entity: String, along: Option<f64>, dist: Option<f64> },
    Edge { a: Vec3, b: Vec3, face_normal: Vec3, dist: f64 },
}
pub fn parse_refs(v: Option<&Value>) -> Vec<Vec<PlaceRef>>;               // param "point_refs", one list per point; absent = none
pub fn resolve_point(state: &State, body: &str, stored: Vec3, normal: Vec3, refs: &[PlaceRef]) -> Result<(Vec3, Option<String>), FeatureError>; // (point, warning)
pub fn find_edge(mesh: &Mesh, a: Vec3, b: Vec3, face_normal: Vec3) -> Option<(Vec3, Vec3)>; // spec 2.4 matching rule; None when missing or ambiguous
// Hole stores `point_refs` beside `points`; Thread stores `position_refs` (one list) beside `position`.

// stl-engine State (Task 8)
impl State { pub fn record_transform(&mut self, body: &str, m: Mat34); } // Move/Rotate/Mirror call it; moves the frames of SketchRec with that body
// SketchRec gains `pub body: Option<String>` (from the plane value's "body" key).

// stl-mesh/src/voxel/square.rs (Task 6)
pub fn square_inner(inner: &mut Mesh, outer: &Mesh, t: f64, pitch: f64);  // snaps inner vertices onto inward-moved flat regions

// stl-app/src/ui/mod.rs (Task 10)
UiEvent::SelectSketch(String), UiEvent::SelectFeature(String)
```

- [ ] **Step 1:** Write the stubs above. In `app.rs`/`app_wiring.rs`: `handle_input` calls `navigation::nav_action` and the pivot/anchor functions; every tool close (OK, Cancel, Escape, another tool) and project open calls `viewport.clear_tool_highlights()` and `self.sync_tool_planes()` (new, Task 9 fills it: show origin planes while a plane field is active, restore the eye state after); finishing or cancelling a sketch no longer moves the camera; the sketch HUD Area at `ws.min + (256, 8)` is replaced by drawing `dim_boxes()` and `total_label()` at their anchors; `UiEvent::SelectSketch` / `SelectFeature` handled (select and highlight; double-click edits as today); Measure calls `viewport.set_measure`; tools that cut pass their preview's cutter to `set_cut_tint` when the setting `cut_preview_tint` (default true) is on.
- [ ] **Step 2:** `cargo check --workspace --tests` and the wasm32 build are clean; every existing test still passes.

### Task 2: No crash from fins or the graphics driver (spec 2.1)

**Owner:** Opus (Claude). **Review:** Nemotron (Haiku if the free models are out). Starts first.

**Files:** `rust/third_party/egui-wgpu/**` (with `VENDORED.md`), `rust/Cargo.toml` (`[patch.crates-io]`), `stl-app/src/app_device.rs`, `stl-app/src/viewport/mod.rs` (`device_lost` only), `stl-app/src/viewport/renderer.rs` (loss logging only), the fins host in `stl-platform/src/fins/` if the cause is there; tests `stl-app/tests/device_lost_window.rs`, new `stl-app/tests/fins_gpu.rs`.

- [ ] **Step 1:** Vendor egui-wgpu 0.37.0 unchanged, patch it in, build: all tests pass.
- [ ] **Step 2:** Test `refused_staging_write_does_not_panic` in `device_lost_window.rs`: destroy the device, then paint through the real `Renderer::update_buffers` path with no `catch_unwind` around it; assert no panic, `viewport.device_lost(false)` is true, and the restart marker is written. Run: fails (panic).
- [ ] **Step 3:** The patch: where `write_buffer_with` returns `None`, skip the egui paint for that frame and set a flag the app reads (the probe `render_state_refuses_uploads` stays). Document it in `VENDORED.md`. Step 2's test passes.
- [ ] **Step 4:** Log the wgpu device-lost reason and message to `log.txt`.
- [ ] **Step 5:** Reproduce on this PC's GPU with the installed build (Support Fins generate; Draw Fins draw one fin, on `tests/fixtures/projects/basic.stlx` and a 200k-triangle part); find the cause (WebView2 GPU process, preview upload size, a long draw) and fix it. Test `fins_gpu.rs` (`#[ignore]` unless `STLEDITOR2_GPU_TESTS=1`, run in the lead's local verify): both flows on the large part, no loss, no panic. Report the cause found.

### Task 3: Orbit and mouse buttons like SolidCraft (spec 2.2)

**Owner:** Sonnet A. **Review:** Hermes (Haiku if out).

**Files:** `viewport/navigation.rs`, `viewport/camera.rs`, the help table text in `app.rs` (one string; coordinate with the lead), a Settings › Navigation entry (`ui/settings*.rs`); tests `stl-app/tests/navigation.rs`.

- [ ] **Step 1:** Tests: `fusion_bindings` (every row of spec 2.2's table, Model mode, through `nav_action`; Sketch/Sculpt: middle pans, right and Shift+middle orbit, left None); `view_centre_keeps_framing_centre` (iso view of a 40×30×20 box at (50,50,0), right-drag 180 px: the box centre's screen point moves < 2 px); `origin_keeps_origin`; `cursor_pivot_far_part_stays_in_view` (SolidCraft #2616 case: box at (50000,50000,0) plus a 10 mm box at the origin, drag from an empty corner: the far box centre moves < 2 px); `zoom_into_empty_space_then_orbit_keeps_part` (ten wheel zooms toward a view corner, then an orbit: the part's box stays on screen); `turntable_up_stays_up`; `trackpad_scroll_pans_shift_orbits`.
- [ ] **Step 2:** Implement per spec 2.2. View centre: the Fit framing centre at the target's depth, kept while it stays where it was on screen. Zoom: toward the surface hit under the cursor, else the ray point at the target's depth, clamped in front of the eye.
- [ ] **Step 3:** Settings › Navigation: "Orbit around" (View centre, Origin, Under the cursor). Help table updated. `EXCEPTIONS.md`: 1.x orbits with the left button and zooms along the cursor ray.

### Task 4: Sketching like SolidCraft (spec 2.3)

**Owner:** Sonnet B. **Review:** Particle (Haiku if out).

**Files:** `sketch/draw.rs`, `sketch/inline_dims.rs`, `sketch/guides.rs`, `sketch/glyphs.rs`, `sketch/panel.rs`, `sketch/mod.rs`; Settings › Sketch entry ("Cursor coordinates", on); tests `stl-app/tests/sketch_inline.rs`, a WARP snapshot per theme.

- [ ] **Step 1:** Tests: `line_boxes` (length and angle boxes beside the segment; typing "25" Tab "30" Enter places a 25 mm line at 30°, and adds length and angle dimensions); `rect_boxes_along_edges` (width box beside the X edge, height beside the Y edge; total label "W × H"); `circle_box_diameter`; `locked_box_ignores_cursor`; `shift_tab_goes_back`; `expression_value` (`=width/2`); `coordinates_follow_cursor_mm_and_inch`; `guide_snaps_to_aligned_point` (cursor within tolerance of a point's X: snapped, one guide drawn); `grid_snap_follows_zoom` (`visible_grid_step` gives 1, 2, 5, 10… at the zooms the grid draws them, and the snap uses it); `constraint_glyph_per_kind` (one glyph per constraint kind; clicking it selects the constraint; Delete removes it); `finish_keeps_camera`.
- [ ] **Step 2:** Implement per spec 2.3 with the Task 1 colour constants: lines 2 px, construction dashed, selected in the selection blue, fully constrained in the constrained shade, plane fill at `PLANE_SKETCH_ALPHA`.
- [ ] **Step 3:** Snapshots of a rectangle sketch on a box's top face, dark and light themes: the lines differ from the fill by a contrast ratio of at least 3:1.

### Task 5: Hole and Thread placed exactly (spec 2.4)

**Owner:** Opus (Claude), after Task 2. **Review:** Nemotron (Haiku if out).

**Files:** `stl-engine/src/features/common/placement.rs`, `stl-engine/src/features/hole.rs`, `stl-engine/src/features/thread.rs`, `stl-app/src/tools/fields/place_refs.rs`, `stl-app/src/tools/fields/facepoints.rs`, `stl-app/src/tools/defs/modify.rs` (Hole and Thread definitions only); tests `stl-engine/tests/placement.rs`, `stl-app/tests/ui/hole_place.rs`.

- [ ] **Step 1:** Engine tests: `edge_distances_exact` (40×30×5 plate, hole 10 mm from the left edge and 8 mm from the front edge: centre (10, 8, 5) to 0.001); `plate_longer_moves_hole` (plate width parameter 40 → 60 with the hole measured from the right edge: the hole moves 20 mm); `sketch_line_along` (point 12 mm along a sketch line); `ambiguous_edge_is_missing` (two edges tie: point kept, warning returned); `removed_edge_is_missing`; `no_refs_unchanged` (projects without `point_refs` rebuild byte for byte).
- [ ] **Step 2:** Implement `placement.rs` per spec 2.4's matching rule (1° tolerances, nearest to the stored line, ties by length then lowest end point, a tie within 0.01 mm counts as missing). Hole and Thread call `resolve_point` and pass the warning to the step's message.
- [ ] **Step 3:** UI: snapping to sketch points, ends, midpoints, centres and onto lines (a marker shows the snap), projected onto the face along its normal; the value box along a snapped line; the panel's "From" row picking one or two edges or sketch lines, each with a distance box and a dimension drawn in the viewport. UI test `hole_place`: click near a sketch line end snaps to it; two edge distances typed give the exact centre; reopening shows both references.

### Task 6: Shell and Hollow make square corners (spec 2.7)

**Owner:** Sonnet B, after Task 4. **Review:** Hermes (Haiku if out).

**Files:** `stl-mesh/src/voxel/square.rs`, `stl-mesh/src/voxel/hollow.rs` (one call); tests `stl-mesh/tests/square_corners.rs`, goldens updated where Shell/Hollow results change.

- [ ] **Step 1:** Tests: `cube_shell_open_top` (20 mm cube, 2 mm, top open: cavity 16 × 16 × 18, volume within 0.1 %, every cavity corner within 0.01 mm of the ideal point); `l_part_inner_corner_square`; `sphere_unchanged` (volume within 0.1 % of today's); `still_watertight`; `deterministic` (two runs, same bytes).
- [ ] **Step 2:** Implement `square_inner` per spec 2.7: flat regions are coplanar groups covering at least `(3 × pitch)²`; snap to one plane, two planes' line, or three planes' point when within one pitch; leave curved areas.
- [ ] **Step 3:** `EXCEPTIONS.md`: 1.x bevels Shell and Hollow corners.

### Task 7: Highlights, Measure, Section looks and the cut tint (spec 2.5, 2.6, 2.8, 2.14)

**Owner:** Sonnet A, after Task 3, and after Task 2 has finished with `viewport/mod.rs`. Its `hole.rs`/`thread.rs` cutter edits wait until Task 5 is in. **Review:** Particle (Haiku if out).

**Files:** `viewport/overlays.rs`, `viewport/section.rs`, `viewport/renderer.rs`, `viewport/mod.rs`, `viewport/shaders/*`, `stl-app/src/app_measure.rs`, `stl-app/src/tools/fields/bodies.rs` (`apply_face` only), cutter output in the cutting features (`stl-engine/src/features/{extrude,revolve,hole,thread,hinge,split}.rs`: preview-only `res.cutter`); tests `stl-app/tests/ui/highlights.rs`, `stl-app/tests/ui/measure_triangle.rs`, WARP snapshots.

- [ ] **Step 1:** Tests: `ok_clears_highlights`, `cancel_clears_highlights`, `escape_clears_highlights`, `highlight_follows_moved_body`; `measure_triangle_legs` (points (0,0,0) and (10,20,5): legs end at (10,0,0), (10,20,0), (10,20,5); labels 10, 20, 5 and 22.9 mm); `measure_one_leg` (differs only in X: one leg); snapshots `section_hatch_on/off` (no pixel with red dominant over the body colour by more than 40; no grid line inside the cut face); `cut_tint_hole_preview` and with the setting off.
- [ ] **Step 2:** Implement per the spec sections; the grid is depth-tested against bodies and caps and drawn before them; remove the on-cap grid "hatch" (`renderer.rs` ~940); the cap uses the body colour darkened 25 %, hatch lines kept.

### Task 8: Move snaps; sketches follow their body (spec 2.9)

**Owner:** Opus (Claude), after Task 5. **Review:** Hermes (Haiku if out).

**Files:** `stl-app/src/tools/gizmo.rs`, the Move panel definition, `stl-engine/src/features/move_.rs`, `stl-engine/src/features/mirror.rs`, `stl-engine/src/features/sketch.rs`, `State`; tests `stl-engine/tests/sketch_follow.rs`, `stl-app/tests/gizmo_snap.rs`.

- [ ] **Step 1:** Tests: `translate_snaps_to_grid` (step from `visible_grid_step` at the current zoom); `shift_is_free`; `z_snaps_to_plate` (within 6 px); `drop_to_plate_button`; `sketch_follows_move`, `sketch_follows_rotate`, `mirror_keep_original_sketch_stays`; `edit_followed_sketch_after_move`; `origin_plane_sketch_does_not_move`; `old_project_sketch_on_body_follows` (listed in EXCEPTIONS).
- [ ] **Step 2:** Implement: `SketchRec.body` from the plane value's `body`; `State::record_transform` moves those sketches' frames; Move/Rotate/Mirror (not keep-original copies) call it. Gizmo snapping per spec 2.9.

### Task 9: Planes when needed, bodies pickable, a clear Living Hinge (spec 2.10, 2.11, 2.13)

**Owner:** Haiku (Claude), after Task 7 (it shares `bodies.rs` and needs `set_cut_tint`). **Review:** Particle (Sonnet B if out).

**Files:** `stl-app/src/tools/fields/plane.rs`, `stl-app/src/tools/fields/bodies.rs` (not `apply_face`), `stl-app/src/tools/defs/model.rs` (`hinge`, `snapfit`), `stl-app/src/app_tools_planes.rs` (new, `sync_tool_planes`); tests `stl-app/tests/ui/tool_picking.rs`.

- [ ] **Step 1:** Tests: `mirror_shows_planes`; `pick_xz_fills_and_restores`; `escape_restores_planes`; `snapfit_click_two_bodies` (pick On, click two bodies: both listed); `tree_row_fills_bodies`; `bodies_activates_after_on`; `hinge_panel_text` (the two numbered lines of spec 2.13 word for word); `hinge_preview_after_plane`.
- [ ] **Step 2:** Implement: `sync_tool_planes` uses `wants_planes`; `advance_field` no longer skips Bodies; a body click while Bodies is active adds it (Ctrl/Shift toggles); the hinge's preview draws the slot through `set_cut_tint` plus the hinge line and flex arrow.

### Task 10: The tree and timeline select reliably (spec 2.12)

**Owner:** Haiku (Claude), after Task 9. **Review:** Hermes (Sonnet B if out).

**Files:** `stl-app/src/ui/browser.rs`, `stl-app/src/ui/timeline.rs`; tests `stl-app/tests/ui/tree_select.rs`.

- [ ] **Step 1:** Tests: `click_row_empty_side_selects`; `sketch_row_selects_and_highlights`; `sketch_row_double_click_edits`; `feature_click_selects`; `click_while_busy_registers`.
- [ ] **Step 2:** Implement: the whole row is the click target; sketch rows send `SelectSketch`; timeline items send `SelectFeature` on click (double-click edits as now).

### Task 11: Close-out

**Owner:** the lead.

- [ ] **Step 1:** Verify in `rust/target/verify`: workspace tests (debug, `--no-fail-fast`) and release; 0 warnings native and wasm; web `--check` on WebGL2 and WebGPU; WARP snapshots; `xtask pure-rust`; `uibench` (no row slower than M6); `STLEDITOR2_GPU_TESTS=1` for `fins_gpu.rs` on this PC.
- [ ] **Step 2:** `rust/tests/EXCEPTIONS.md` milestone 7 section complete; the help table matches the Fusion buttons.
- [ ] **Step 3:** `cargo run -p xtask -- install` (with `CARGO_TARGET_DIR=target/verify`) with the STL Editor 2 window closed; start the installed build and run Support Fins and Draw Fins once by hand.
- [ ] **Step 4:** Report: result lines, the bench table, the fins crash cause Task 2 found, and every "pending Nathan's OK" line.
