# STL Editor 2.0 Milestone 4a Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the gaps found when milestone 4a was checked. Afterwards, 2.0's shell and viewport look and behave like 1.x in every 4a screenshot state, the web build runs without WebGPU, and every uibench run meets the rebuild target.

**Architecture:** Fixes inside the existing 4a modules. No new module and no new dependency, apart from enabling wgpu's `webgl` feature for the web target.

**Tech Stack:** As 4a.

**Spec:** `dev/specs/2026-11-12-stl-editor-2-m4-app-design.md`. The 4a plan is `dev/plans/2026-11-12-stl-editor-2-m4a-shell-viewport.md`.

## Global Constraints

- **Every 4a Global Constraint still applies.** That includes the SolidCraft rule (read-only reference, copy nothing), matching 1.x's sources of truth, no commits or pushes, and not stopping for decisions.
- **Nathan's answers to the 4a parked questions (2026-11-12):**
  - Accepted, so mark them accepted in `EXCEPTIONS.md` and the pure-rust allow-list notes:
    - `x11-dl` on the Linux pure-rust allow-list
    - the engine-lost text and the Restart button
    - plain-text Help and About layout
    - Open Recent without a path until milestone 5
  - To fix in this run:
    - the 23.33 ms rebuild frame (Task 3)
    - the About credits (Task 1)
    - the Offset 0.2 mm timing (Task 5)
  - Waiting: publishing to `/app2/` waits for Nathan's yes after this run.
- **Builds:**
  - Never run two cargo commands at once on the same target directory.
  - Each worker builds in its own `CARGO_TARGET_DIR` (`rust/target/fix-<task>`).
  - The lead verifies in `rust/target/verify`.
  - Don't delete any target directory; Nathan decides that.

## Review Focus

1. **A project opened from Recent, or by drag and drop, rather than from File > Open.** The title shows the file name and the project is clean in every case. Test: Task 1 `title_after_each_open_path`.
2. **A body with no colour set.** Its swatch shows the default body colour the viewport draws, not grey. Test: Task 1 `swatch_matches_viewport_colour`.
3. **A browser with WebGL2 but no WebGPU,** such as Chrome with WebGPU turned off. The app starts on WebGL2, and the viewport, picking and section view work. Test: Task 4 `web_check_webgl2`.
4. **A screenshot state that fails to load.** The capture script fails loudly instead of saving the previous screen. Test: Task 2 `states_differ`.

---

### Task 1: Title, swatches, parameters list and About credits

**Owner:** Sonnet A. **Review:** Composer B.

- **Title:** after any successful open (File > Open, Recent, drag and drop, a startup argument), the top-bar title is the file name, as `basic.stlx` is in 1.x, and the project is clean (no `•`). A failed open keeps the previous title and dirty state.
- **Swatches:** each body's browser swatch uses the colour the viewport draws: the user's colour, else the feature's, else the default, as 1.x's tree does.
- **Parameters:** the browser's Parameters section lists `name = value` rows, expanded by default, as in 1.x (`dx = 5`, `spare = 1`).
- **About credits:** list what 2.0 actually uses, from the workspace's `Cargo.toml` files: egui and eframe, wgpu, the manifold-rust port, the fonts and so on, each with its licence. Keep 1.x's thanks and Ko-fi text; drop every Python library name.

- [ ] **Step 1: Write the failing tests:**
  - `title_after_each_open_path`
  - `failed_open_keeps_title`
  - `swatch_matches_viewport_colour`
  - `params_listed_expanded`
  - `about_lists_rust_crates_only`: no `numpy`, `trimesh`, `pywebview` or similar
- [ ] **Step 2:** Implement until they pass. Update the snapshots.

### Task 2: Real screenshots for every state

**Owner:** Cursor. **Check:** the lead.

The 4a screenshots compare nothing for four states:

- On the 1.x side, rename, timeline, parameters and section are byte-identical to the project shot in each theme.
- On the 2.0 side, the timeline shot is identical to the project shot.

**Fix:**
- `dev/m4a_shots/capture_m4a_shots.mjs` must really reach each state in 1.x:
  - rename a body in the tree
  - roll the timeline back one step
  - open the Parameters panel
  - turn on section view

  Wait for 1.x to finish drawing before each capture.
- 2.0's `m4a_shots` test must reach the same states.
- **New check, `states_differ`:** within one theme and edition, no two states give identical images. The script exits non-zero if they do.

- [ ] **Step 1:** Fix both capture paths and add `states_differ`.
- [ ] **Step 2:** Regenerate `dev/out/m4a-shots/` after Tasks 1, 3 and 6 land. List any remaining visible difference per state in `index.md`.

### Task 3: Rebuild frames under 16.7 ms in every run

**Owner:** Worker 6. **Review:** Opus (Claude).

In the 4a uibench, run 1 had one 23.33 ms frame out of 1,715 during the Hollow rebuild.

- [ ] **Step 1:** Find what that frame did, using a per-frame timing trace in uibench: upload, BVH swap, engine message handling or something else. Fix it, for example by spreading the mesh upload or the BVH swap over more frames.
- [ ] **Step 2:** Run uibench three times with nothing else running. Expected: no frame over 16.7 ms in any run, and the other rows no worse.

### Task 4: WebGL2 fallback for the web build

**Owner:** Grok B. **Review:** Worker 6.

Main spec section 6 says the web uses WebGPU where available and falls back to WebGL2. The 4a web build enables only `webgpu`.

- [ ] **Step 1:** Enable wgpu's `webgl` feature for wasm32 and choose WebGPU when the browser offers it, else WebGL2. Keep shaders within WebGL2 limits (no storage buffers in the vertex stage, and so on), or give the WebGL2 path its own variants.
- [ ] **Step 2:** Add `xtask web --check --webgl2`, which runs headless Chrome with WebGPU off (`--disable-features=WebGPU` or the equivalent flag). Check: both bodies in the tree, a pick at the block hits it, and section view draws. Run `xtask web --check` too. Expected: both pass.

### Task 5: Grid, axes and section cap

**Owner:** Opus (Claude). **Review:** Worker 6.

- **Grid and axes:** the scene grid and axis lines match `viewer.js`: the grid's extent and fade, and the axis lines visible on the start page. 2.0 currently draws a smaller, square grid on the start page with no axes.
- **Section cap:** the cut face hides the grid behind and below it, as 1.x's cap does (fix the depth or stencil order).

- [ ] **Step 1: Write the failing snapshot tests:** `start_grid_and_axes` and `section_cap_hides_grid`, dark and light.
- [ ] **Step 2:** Implement until they pass.

### Task 6: Offset timing against 1.x, warnings and repo hygiene

**Owner:** Cursor.

- **Offset timing:** time 1.x's Offset 0.2 mm on the bench mesh, the same feature and values 2.0 took 200 to 360 s on. If 2.0 is slower, add an Offset section to `PERF_GAPS.md` with both times. If not, note the result in `UI_BENCH.md`.
- **`.gitignore`:** add `rust/target-*/` and `rust/crates/*/target/`.
- **Warnings:** clear `stl-app`'s compiler warnings (40 at the end of 4a) without changing behaviour.

- [ ] **Step 1:** Do all three. Run `cargo build -p stl-app 2>&1 | findstr warning`. Expected: none.

### Task 7: Close-out

**Owner:** the lead, in `rust/target/verify`.

- [ ] **Step 1:** Run every check from the 4a Task 11 list, plus `xtask web --check --webgl2` and uibench three times.
- [ ] **Step 2:** Update `EXCEPTIONS.md`, marking Nathan's accepted items, and `UI_PARITY.md`.
- [ ] **Step 3:** Report:
  - the result lines
  - the uibench table
  - the screenshot folder, with what still differs per state
  - the Offset timing
  - anything parked

  Do not commit.
