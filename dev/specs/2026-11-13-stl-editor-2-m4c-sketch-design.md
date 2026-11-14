# STL Editor 2.0 Milestone 4c: sketch mode, the last UI tests and the look pass (design)

Status: draft for Nathan's approval, 2026-11-13. This is a sub-spec of `dev/specs/2026-11-12-stl-editor-2-m4-app-design.md`, whose rules in section 2 apply here unchanged: 1.x's look, the structure of `stl-app`, the smoothness targets, the tests, and SolidCraft as read-only reference only. 4c is the last batch of milestone 4.

## 1. Intent

What Nathan decided (2026-11-13):

- **4c brings 1.x's sketch mode to 2.0**, finishes the 1.x UI test counterparts, and does a look pass. After 4c, 2.0's interface does everything 1.x's does.
- **The look pass fixes drift only.** The egui limits Nathan accepted in 4b stay as they are: egui's drop-down list, solid dashed boxes, the flat focus ring, the icon art, and the wgpu-drawn gizmo, rings and brush. No custom drawing to close those gaps.
- **One PandaCode run in parallel streams, like 4b** (section 5). The plan fixes the sketch module's types and calls first, so streams start at once on separate files.
- **Carried over from 4b:**
  - the 320 ms fly-in to the iso view at start-up;
  - measure the web edition's large-preview pause in a real browser, and fix it if it is over about 0.3 s (`EXCEPTIONS.md`, 4b section).
- **Unchanged:** `/app2/` stays private; no version bump; SolidCraft is reference only.

Goals: sketch mode that works and feels like 1.x on desktop and web, with every 1.x sketch test passing as a 2.0 test.

Non-goals:

- New sketch features 1.x doesn't have: sketch fillet, spline, ellipse (1.x's "not built yet" list).
- Closing the accepted egui look gaps (above).
- Milestone 5 work: autosave and Recover, recent files that survive a restart, the update check, the browser drawer on phones.

Anti-goals (a failure even if it runs):

- A sketch that solves, dimensions or snaps differently from 1.x. The solver is already Rust and checked against 1.x; the interface must feed it the same picks and constraints 1.x does.
- Dragging in a sketch, or solving, that stalls the screen.
- A sketch saved by 2.0 that 1.x opens differently, or the reverse (`.stlx` round trip).

## 1a. Approaches

| Approach | Trade-off | |
|---|---|---|
| **A. One run, four parallel streams** split by file, with the sketch module's types and calls fixed first | Fastest (about 4b's size); `sketcher.js` is one tangled file, so the fixed interfaces carry the risk, as they did for 4b's tools | **Picked by Nathan** |
| B. Two runs: sketch mode, then the tests and look pass | Cleaner, the tests target finished code; about twice as long, two plans to approve | |
| C. One run, one strong worker ports `sketcher.js` in order, others on tests and look | Fewest integration bugs; the sketcher becomes a long single-worker bottleneck | |

## 2. What 4c builds

### 2.1 What already exists

The engine side of sketching was ported and checked in milestone 3. 4c builds the interface on it and adds no geometry:

| 1.x | 2.0, already there |
|---|---|
| `constraints.js` (solve, analyze, infer, make constraints) | `stl-sketch/src/solver/` (golden: `solver_golden.mjs`) |
| `import2d.js` (SVG, DXF) | `stl-sketch/src/import2d/` (golden: `import2d_golden.mjs`) |
| `sketch_op` trim, offset, mirror, explode | `stl-sketch/src/ops/` |
| `sketch_op` faces | `stl-sketch::profile::faces` |
| `sketch_op` project, refs (`MT.outline`, `MT.refGeometry`) | `stl-mesh/src/refs.rs` |
| `sketch_op` text (`MT.textContours`) | `stl-sketch/src/text/` |
| `viewer.js` `syncSketches`, `sketchLines`, `addLine`, `addPolyline` | `Viewport::sync_sketches`, `overlays::sketch_lines`, `add_line`, `add_polyline` (4a) |

### 2.2 The engine route (stream 3)

- A new `Command::SketchOp { seq, op, data }` and `Update::SketchOp { seq, result }` through `engine_link`, on desktop (engine thread) and web (Web Worker), as 1.x's `sketch_op` route (`web/engine/routes.js` lines 100–142, `stleditor/server.py`).
- The ops and their JSON are 1.x's: `trim`, `offset`, `mirror`, `explode`, `project`, `refs`, `text`, `faces`. New entity ids follow 1.x's `e<seed><n>` rule.
- `project` and `refs` read the document's solid, visible bodies, as 1.x does.
- A golden check records 1.x's `sketch_op` answers for a fixed set of cases (Node, `routes.js`) and 2.0's route must give identical JSON.

### 2.3 Sketch mode (stream 1)

A port of the sketch parts of `app.js` and the session parts of `sketcher.js`, in a new `stl-app/src/sketch/` module:

- **Starting a sketch:** `beginSketch` (the plane prompt in the tool panel: a flat face, a construction plane or an origin plane), `enterSketch` (frame from the plane, the part-edge refs from `sketch_op refs`, the "N part edges have changed" warning), and editing a sketch step from the timeline (double-click or Edit), which rolls back to it while it is open.
- **The sketch ribbon:** the `ribbonDef()` sketch tab (`app.js` lines 105–118): Draw, Modify, Dimension, Relations (the constraint grid, 5 columns), Finish. The tool search lists the sketch tools while a sketch is open.
- **The sketch panel:** `openSketchPanel` and `renderSketchPanel`: title Sketch or Edit Sketch, the entity list, the selected entity's fields (`updateEntity`), the definition line (`defText`, `defState`: under-, fully or over-defined), the Modify dialog (`openModify`), Finish Sketch.
- **Finishing:** `finishSketch` (add or edit the `sketch` step with `plane`, `entities`, `refs`, `constraints`; an empty sketch says so), Finish & Extrude (opens Extrude on the new sketch when it has a closed profile), `cancelSketch` (puts the rollback back).
- **In the viewport:** the sketch plane grid (`_buildPlaneGrid`), the sketch drawn with 1.x's colours (`--sketch` and the construction, selected, reference and missing colours), Look At, and the other sketches shown as 1.x shows them while one is open.
- **The HUD and hint line:** `showHud`, `setHud`, `toolHint` and the per-tool hints.
- **Keys:** Escape, Enter, Delete, Ctrl+Z inside the sketch (`undoLocal`), and the shortcuts 1.x has in sketch mode.
- **Leaving 4b's stub:** `sketcher_active()` (`app_wiring.rs`) returns the real state, so the gizmo hides while a sketch is open.

### 2.4 Drawing and modify tools (stream 2)

A port of `sketcher.js`'s drawing and editing, with `sketchgeom.js`:

- **Draw:** line (chains, double-click or Enter to finish), arc (3-point), rectangle, centre rectangle, circle, polygon, slot, text (the text dialog, through `sketch_op text`).
- **Modify:** select (click, box), drag points and entities (`dragStart`, `dragMove`, `dragEnd`), trim, offset, mirror, round corner (`roundCorner`), delete, construction toggle.
- **Typed sizes:** `readTyped`, `commitTyped`, `addTypedDims`: sizes typed while drawing become dimensions, as 1.x.
- **Snapping and inference:** points, midpoints, centres, part edges, the grid, and `autoConstrain` (the constraints 1.x adds as you draw).
- **Project, Use Edge, Import SVG/DXF:** through `sketch_op project`, `pickEdge` and `stl-sketch` import2d (the file dialog through `Platform`).
- **Region picking for Extrude and the other tools** (`app.js` line 1380, `sketch_op faces`) keeps working with the new sketches.
- **Touch:** draw by taps and double-tap into a sketch, as `tap.js` tests.

### 2.5 Constraints and dimensions (stream 3)

A port of the constraint and dimension interface:

- **Relations:** the `CTOOLS` picks (`constraintClick`, `makeConstraint`), their glyphs on the sketch (`glyphSpots`, the ten constraint icons), deleting a relation, and the messages 1.x shows (for example "Part edges and the origin are fixed already.").
- **Smart Dimension:** `dimClick`, `inferDimension`, `prepDim`, `placeDimension`, reference dimensions to part edges, parameters (`=width/2`) in a dimension, `setDimValue`, `setDimRef`, double-click to edit (`editDimension`).
- **Dimension drawing:** a port of `dims.js` (`linearBase`, `roundBase`, `defaultPlacement`, `placeAt`, `layout`) and `buildDims` / `placeDims`: the dimension labels, arrows and extension lines, in the display unit (inch mode as 1.x).
- **Solving:** `solveAll` on every change through `stl-sketch`'s solver, with `analyze` for the definition state and `isRedundant` / `unsatisfied` for the warnings.

### 2.6 The last UI tests, look drift and the web preview pause (stream 4)

- **UI tests:** `egui_kittest` counterparts for the 8 `UI_PARITY.md` rows marked 4c, and the parked sketch-and-Extrude edit in `desktop_tools`:
  - `desktop_flow`, `desktop_new`, `desktop_sketch2`, `desktop_v11`
  - `skdim`, `skdim2`
  - `tap`
  - `web2`
- **The fly-in:** the camera flies to the iso view over 320 ms at start-up, as 1.x's `viewer.js` does during load.
- **Look drift:** compare every 4a, 4b and 4c screenshot pair, list what drifted from 1.x (spacing, sizes, colours, text, sketch colours), and fix it. The accepted egui limits are not drift.
- **The web preview pause:** measure the large-preview preparation in Chrome (a 500,000-triangle preview, the time the page doesn't respond). If it is over about 0.3 s, prepare it in slices over several frames, as the red overhangs are. `EXCEPTIONS.md` records the measured time either way.

### 2.7 Decisions for the builder (Claude's calls, from 1.x and the project; Nathan can overrule)

1. **Solving is synchronous on the UI thread, in both editions, as in 1.x** (`constraints.js` runs in the page). `stl-app` already links `stl-sketch`, which builds for wasm32. Plan Task 1 measures a solve on the 200-entity, 100-constraint case first. Only if it can't hold the 16.7 ms frame does solving move off the UI thread (a background thread on desktop, the engine's Web Worker on web, newest result wins), and then the lead decides it once for all streams.
2. **The sketch model is `stl-sketch`'s typed entities and constraints,** which keep unknown keys in `extra`, so a 1.x sketch round-trips losslessly. The step JSON (`plane`, `entities`, `refs`, `constraints`) is 1.x's. New ids use 1.x's `e<seed><n>` rule in one shared helper.
3. **One event dispatcher, owned by stream 1.** Task 1 fixes a `SketchTool` trait (pointer down, move, up, double-click, key, hover, cancel) and the active-tool state. Stream 2's draw and modify tools and stream 3's relation and dimension tools implement it; none of them reads viewport events directly. Orbit, pan and the gizmo stay with the viewport, as in 1.x's `setMouseMode('sketch')`.
4. **The view is not locked.** Entering a sketch flies to face the plane (`lookAtPlane`, as `sketcher.start` does), and the user can still orbit, as in 1.x. Picks go through `Viewport::ray_to_plane` into plane coordinates (`toUV`), so they work from any angle.
5. **UI test counterparts assert the same observable outcomes** as the 1.x files: the step JSON, solved coordinates, the panel and toast text, the timeline. They don't copy DOM clicks. Clicks on the canvas go through `world_to_screen` of plane points. Ribbon buttons, panel fields, dimension labels and the definition line carry AccessKit labels, so tests find them by name. Streams 1 to 3 add those labels as they build.
6. **Keys follow 1.x exactly:** the full set in `sketcher.js` `onKey` and the sketch branch of `app.js`. In a sketch, Ctrl+Z undoes the last sketch edit (`undoLocal`), as in 1.x, not the document's last step.

## 3. Checks

- **UI tests:** the 9 counterparts above pass; `UI_PARITY.md` shows every 1.x file done or parked with a reason.
- **`sketch_op` golden:** identical JSON to 1.x for every recorded case. It runs in CI.
- **Sketch round trip:** a sketch drawn, constrained and dimensioned in 2.0 saves to `.stlx`, and 1.x's engine rebuilds it to the same profile (and the reverse, with the 1.x fixture projects that have sketches).
- **Screenshots, built at the start of the run:** 1.x and 2.0 side by side, dark and light, in `dev/out/m4c-shots/`, for each sketch state: the plane prompt, an empty sketch, each draw tool mid-draw, a constrained and dimensioned sketch (under-, fully and over-defined), the Modify dialog, the text dialog, Use Edge with part edges, a missing reference, inch mode, Finish & Extrude. The `states_differ` check from 4a applies.
- **Smoothness:**
  - a new uibench row drags a point in a sketch with 200 entities and 100 constraints: every frame under 16.7 ms;
  - entering and leaving a sketch on the bench mesh keeps every frame under 16.7 ms;
  - solving never stalls the UI thread for more than one frame. If the solver is too slow on the UI thread, it moves to a background thread with the newest result winning.
- **Fonts:** every character the sketch interface shows is in the bundled fonts (`fonts_fallback`, extended).
- **Rules from 4a and 4b:**
  - each worker builds only in its own `rust/target/4c-t<N>` folder, running cargo from `rust/`; never `rust/rust/` or `rust/crates/target`;
  - test scenes wait for the viewport (`Viewport::flush`) before picking;
  - the lead runs the CI steps locally, including the WARP snapshot run and `xtask web --check --webgl2`.
- **Unchanged:** parity, every golden check, pure-rust, the wasm32 builds and the 1.x tests still pass.

## 4. Done when

1. The 9 UI test counterparts pass, and `UI_PARITY.md` has no row left "planned".
2. The `sketch_op` golden matches, and the round trip check passes.
3. The screenshot pairs exist for every sketch state; every difference left is either an accepted egui limit or listed in the report.
4. The smoothness checks pass, and the web preview pause is measured (and fixed if over about 0.3 s).
5. Every 4a, 4b and milestone 3 check passes, and CI would pass.
6. `EXCEPTIONS.md` lists every 1.x difference, pending Nathan's OK.

## 5. Team

The "STL 2.0 build" profile, as in 4b. Streams split by file; the shared wiring (`app.rs`, `app_wiring.rs`, `ribbon.rs`, `engine_link`) is done once, in order, by the stream that owns it.

| Who | Stream | Jobs |
|---|---|---|
| Opus 5.5 (lead) | | Hands out work, writes the fixed interfaces (plan Task 1), checks results, runs the benchmarks and CI steps, closes out |
| Sonnet A | 1 | Sketch mode: begin, enter, finish, cancel, edit from the timeline, the sketch ribbon, panel, HUD, hint, keys |
| Opus (Claude) | 2 | Drawing tools, typed sizes, snapping, `autoConstrain`, select and drag, the modify tools |
| Sonnet B | 3 | Constraint and dimension interface, the `dims.js` port, solving and the definition state |
| Grok B | 3 | The `SketchOp` command and route (desktop and web), Project, Use Edge, text, import SVG/DXF, the `sketch_op` golden |
| Worker 6 (Grok) | 4 | The fly-in, look drift fixes, the web preview measurement (and fix) |
| Cursor (Auto) | 4 | The 9 UI tests, the screenshots, the uibench rows, the round trip check, CI |
| Gemini (Cursor, limited) | 2 | Text dialog and touch drawing; if Gemini runs out, Opus (Claude) takes them |
| Composer B, Haiku, Hermes | | Read-only reviews. If Hermes runs out, Haiku. |

Worker 1 and Worker 7 are not used (Worker 7 failed twice in 4b).
