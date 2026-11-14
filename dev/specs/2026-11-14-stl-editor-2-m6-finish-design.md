# STL Editor 2.0 Milestone 6: finish 2.0 against 1.x, and fix what testing found (design)

Status: approved by Nathan, 2026-11-14. The hidden-edge outlines in 2.3 stay unbuilt until he asks. Milestone 6 of `dev/specs/2026-11-10-stl-editor-2-rust-design.md`. The rules of `dev/specs/2026-11-12-stl-editor-2-m4-app-design.md` section 2 still apply (1.x's look and behaviour, smoothness, tests). SolidCraft is a reference for how things behave only: no code, theme values, icons or fonts from it, no dependency, no credit.

## 1. Intent

What Nathan decided (2026-11-14):

- **Finish 2.0 first, release last.** Milestone 6 makes 2.0 complete against 1.x and fixes what Nathan's day-to-day use of the test build turned up. The 2.0 release (GitHub release, `/app2/` to `/app/`, the website) is one final update after Nathan has tested everything; it is not in this milestone.
- **Option A:** the items parked for milestone 6, the speed criterion, and the gaps from testing. New features (fillet and chamfer on mesh edges, sketch spline and ellipse, colour 3MF, sketches following a moved edge at once) are milestone 7.
- **Three gaps from the test build:**
  1. When a tool such as Sketch looks straight at a face, orbiting turns around a point on that face. It must keep turning around the part.
  2. The Section button on the ribbon does nothing.
  3. Split never works on a picked face ("The plane doesn't cut through this body"), and there is no way to move the cut.
- **A sweep of the ribbon against 1.x's `onRibbon` (2026-11-14) found four more buttons that do nothing,** because their 1.x features were never built in 2.0: **Measure, Properties, Send to Slicer and To STEP.** The main spec has them (its section 1 and the `stl-platform` row: the slicer and SolidCraft hand-off; Measure and Properties are 1.x tools). They are built in this milestone (2.5).
- **Section and Split share one "drag the plane" control (approach 1),** shaped like SolidCraft's Section Analysis: pick a plane or a flat face, then drag an arrow along its normal or type a distance, with Flip. The section's cut faces get **diagonal hatch lines**, as SolidCraft draws them.
- **Send the work to a PandaCode team run.**

Goals: everything 1.x does works in 2.0, the three gaps are fixed and covered by tests that would have caught them, and no ribbon button or searchable command silently does nothing.

Non-goals: publishing anything, a version bump or tag, the milestone 7 features, code signing.

Anti-goals:

- A section or split control that only works on axis-aligned faces.
- The camera jumping when a tool opens or closes, or orbiting around empty space.
- Projects from 1.x or earlier 2.0 builds opening differently.

## 1a. Approaches

| Approach | Trade-off | |
|---|---|---|
| **1. One drag-the-plane control for Section and Split.** Split stores a distance with its plane. | One control to build and test; a 2.0 project with a moved split cuts at the plane itself in 1.x | **Picked by Nathan** |
| 2. Split adds an Offset Plane step for you | 1.x's project format unchanged, but every split is two steps and moving the cut means editing the plane | |

## 2. What milestone 6 builds

### 2.1 The orbit point stays on the part

**Done ahead of the run (2026-11-14), with the M5 fixes:** `Camera::look_at_plane` puts the camera exactly where 1.x does; only the orbit target moves, back along the same line to the middle of the scene's box, so the view is unchanged and orbiting turns around the part. On a close-up where the middle would be behind the camera, the point on the plane is kept. Test `tests/orbit_point.rs`; `EXCEPTIONS.md` (1.x orbits around the plane point). No other camera behaviour changes.

### 2.2 The drag-the-plane control (shared)

A viewport overlay and its logic, used by Section (2.3) and Split (2.4):

- **Input:** a plane frame (origin, normal), a distance in mm along the normal, and the body or scene box it should span.
- **Drawn:** the plane as a translucent rectangle a little larger than the box's cross-section, its outline, and an arrow handle at its centre pointing along the normal. A small box next to the handle shows the distance in the current units while dragging, as SolidCraft does.
- **Drag:** dragging the arrow moves the plane along its normal; the distance follows the pointer's projection onto the normal line (not screen-space pixels), so it tracks the pointer at any view angle. When the normal points almost straight at the camera (within about 15°, for example a Z cut seen from Top), that projection breaks down, so the drag falls back to vertical screen movement (up moves the plane toward the camera), scaled by the pixel size at the plane. Hover and drag colour the arrow as the Move gizmo's handles are coloured. A drag is one undoable change of the tool's distance field.
- **Typed:** the panel's Distance field (a normal number field, so `=` expressions and inch mode work) moves the plane the same way.
- **Start position:** on a fresh pick, when the plane does not pass strictly through the box (a face on the box's boundary counts as not passing through), the distance is set so the plane sits halfway through the box along the normal. A plane strictly inside keeps distance 0. Reopening a step from the timeline keeps its stored distance; the halfway start applies only to a fresh pick.
- Works for any plane: construction planes, offset or angled planes, and flat faces at any angle.

### 2.3 Section View

1.x's Section tool (`web/js/app.js` `startSection`, `renderSection`, `applyCut`, `toggleCut`, `clearCut`), with the plane chosen like SolidCraft's:

- The ribbon's Section button and the tool search open a **Section View** panel: **Plane** (X, Y, Z as quick buttons, as 1.x's Axis, or a picked construction plane or flat face), **Distance** with the drag control, **Flip** (which half is kept), **Hatch** (on by default), 1.x's note that the cut only changes the view, and **Remove cut**. The button reads Done.
- The cut shows live while the panel is open, and stays after Done. X and the view cube's Section button turn it off and on, as now; with no cut yet they open the panel, as 1.x's `toggleCut` does.
- Editing the cut again opens the panel with the plane where it was. A new or opened project removes the cut (as now).
- **Hatch:** the cut faces are drawn in the cap colour with darker diagonal lines, evenly spaced in screen space (so they read the same at any zoom), as SolidCraft draws them. With Hatch off they are the flat cap colour 1.x uses.
- **Pending Nathan:** SolidCraft also draws thin outlines of edges that sit behind the remaining walls (`dev/out/solidcraft-section.png`). If that is the "dashed lines" Nathan meant, the cut view also draws the bodies' feature edges hidden behind other faces as thin dashed lines while a cut is on. Built only if he says so.
- The cut stays view-only: export, Properties and Measure use the whole part (1.x).
- Test: clicking the ribbon's Section button opens the panel and turns the cut on; picking a side face and dragging moves the cut; Flip swaps the kept half; the WARP snapshot of a hatched cut differs from an unhatched one.

### 2.4 Split

- **Engine:** `split` takes an optional `offset` (mm, default 0): the cut plane is the picked plane moved `offset` along its normal. A missing `offset` cuts exactly as 1.x does, so every existing project and golden is unchanged. The parameter can be an `=` expression like any other.
- **Panel:** Body, Split plane, **Distance** (the new field, with the drag control), Keep, and the connector fields, as now. Picking a flat face or a plane that only touches the body starts the plane halfway through it (2.2), so a first Split on a face works.
- The live preview shows the two halves while dragging, as it does today when fields change.
- The message for a plane that still misses the body becomes "The plane doesn't cut through this body. Drag the arrow or change Distance to move it into the part."
- Tests: engine tests with volumes worked out by hand (a 20 mm cube split by its top face at distance -5 gives 20×20×15 and 20×20×5); an old project's split rebuilds to the same result; a UI test picks a side face, accepts, and gets two bodies; reopening a moved split from the timeline shows its stored distance.
- The new Distance field changes Split's definition, so `tests/tools_golden.rs` and `tests/tool_picks.rs` (which compare tool definitions with 1.x) are updated on purpose, with an `EXCEPTIONS.md` line.

### 2.5 The missing 1.x features, and no silent buttons

Built as 1.x has them (`web/js/app.js` `onRibbon` and the functions named below; `stleditor/server.py` routes):

- **Measure** (`startMeasure`, `measureClick`): a Measure panel ("Click two points on the model."); the first click shows Point 1, the second draws a line between blue markers and shows Distance, ΔX, ΔY, ΔZ in the current units; a third click starts again. Done closes it.
- **Properties** (`startInfo`, `showInfo`, server route `info`): with no body selected, "Click a body to see its size, volume and triangle count."; for a body: its name, Watertight (or "No — run Repair"), Size (X×Y×Z), Triangles, Surface area (cm² or in²), and for a solid Volume, Weight (PLA) at 1.24 g/cm³, Weight (PETG) at 1.27 g/cm³ and Through-holes (genus), as 1.x shows them. The engine gains a body-info command in `stl-engine` (desktop and web), with tests on a cube and a ring worked out by hand.
- **Send to Slicer** (desktop, `sendMenu`, `slicerItems`, server routes for Bambu Studio, OrcaSlicer, PrusaSlicer and SolidCraft): the menu of the slicers found on the PC (1.x `find_slicer`: the same install paths and search rules on Windows, Mac and Linux), also under File › Send to Slicer; picking one exports the selected bodies (or all visible ones) as 1.x's `slicerItems` and the `export` route with `slicer` do, then opens the file in that program. On the web the button says it's desktop-only, as 1.x.
- **To STEP** (desktop, `toStep`, server `to_step`): with SolidCraft's command-line tool installed, converts the selected body to a true STEP through it, as 1.x does ("Converting with SolidCraft…", "Saved STEP: <name>"); without it the button is disabled, as 1.x. This runs SolidCraft as a separate program, which the main spec keeps (section 1: "2.0 keeps handing off to SolidCraft and slicers"); no SolidCraft code is used.
- **No silent buttons:** a test walks every ribbon item on every tab, every tool-search command and every item of the body right-click menu, with a body selected, and checks that something happens: a tool panel, a menu, a dialog, a mode, a toast or a view change. The app runs on a test platform that records file-dialog and program-launch requests instead of opening them (a real dialog would hang the test). Anything the test finds that does nothing is fixed in this milestone, or listed in `EXCEPTIONS.md` for Nathan with the reason. Desktop-only items are checked on desktop and skipped on the web, as `DESKTOP_TOOLS` is in 1.x.

### 2.6 Graphics driver reset in the real window

- Milestone 5 recovers in the test renderer only, because eframe owns the window's device. Milestone 6 makes the real window recover: when the device is lost, the app keeps the project, makes a new device and surface, uploads the scene again and carries on.
- If eframe 0.37 can't give the app a new device, the fallback is: autosave at once, show "The graphics driver restarted. STL Editor needs to restart to keep drawing. Your work is saved." with a Restart button that restarts the app and recovers the autosave without asking. The builder says which one was built and why.
- Test: a forced device loss in the real window's code path (not only the test renderer).

### 2.7 1.x's update check skips 2.x

- `stleditor/updates.py` ignores any release whose tag starts with `v2.` (it lists releases and takes the newest `v1.*` instead of reading `releases/latest`). The 1.x web edition has no update check, so nothing changes there.
- Tests in `tests/` with recorded release lists: a newer `v2.0.0` is not offered; a newer `v1.x` still is.
- Release order for the final update (recorded here so it isn't lost): this 1.x change goes out as a 1.x release first, and the 2.0 release is published with GitHub's "latest" flag off (`make_latest: false`), so 1.x installs that never update are not offered it either, and the website's `releases/latest/download/STL-Editor-Windows-portable.zip` link keeps pointing at 1.x until the website moves to 2.0.
- Nathan's installed 1.x runs from this checkout, so the change takes effect at his next 1.x start; it must keep 1.x's own update check working (its tests prove a newer `v1.*` is still offered).

### 2.8 As fast as 1.x on every benchmark

- The main spec's criterion: no `xtask bench` row slower than 1.x. Today's quiet runs read Split 1.22 and Pattern circular x12 1.14 (`rust/tests/PERF_GAPS.md`); Join 1.08 and Cut 1.03 are within noise.
- Profile Split and Pattern, fix what is slow, and record the before and after in `PERF_GAPS.md` with A/B runs as in 4b. Results must stay the same byte for byte (the parity and kernel corpus checks).
- If a row can't reach 1.0 after a real attempt, `PERF_GAPS.md` says why, and it is listed in `EXCEPTIONS.md` for Nathan.

### 2.9 Decisions for the builder (Claude's calls; Nathan can overrule)

1. The section state is not saved in the project (1.x doesn't save it).
2. Split's `offset` is stored only when it isn't 0, so a split that wasn't moved saves exactly as 1.x.
3. The hatch spacing is about 6 px at 1x scale, at 45°, in a darker shade of the cap colour; the exact values are the builder's, chosen to look right in light and dark themes.
4. The drag control lives in `viewport/` (only `viewport/` uses wgpu), with its maths in a plain module the tools and tests can call.

## 3. Checks

- The new tests in 2.1–2.7, the bench rows in 2.8.
- Unchanged: every check from milestones 1–5 (parity, goldens, pure-rust, wasm32, web checks, uibench, the 1.x tests).
- The lead installs the test build (`xtask install`) at the end so Nathan can try the three fixes.

## 4. Done when

1. Orbiting after a face view turns around the part (done ahead of the run).
2. Section View opens from the ribbon, cuts on any plane or face, drags, flips, hatches, and X toggles it.
3. Split cuts on a picked face first time, and the cut can be dragged or typed.
4. Measure, Properties, Send to Slicer and To STEP work as in 1.x, and the no-silent-buttons test passes.
5. The real window survives a forced device loss (or restarts and recovers, if that's what was built).
6. 1.x's update check skips 2.x releases, with tests.
7. No bench row slower than 1.x, or each one left explained in `PERF_GAPS.md` and `EXCEPTIONS.md`.
8. Every earlier check passes, CI would pass, and every new 1.x difference is in `EXCEPTIONS.md` pending Nathan's OK.

## 5. Team

The "STL 2.0 build + free" profile. Cursor-based workers (Cursor, Grok B, Composer B, Gemini) are out of credit and get no tasks.

| Who | Jobs |
|---|---|
| Opus 5.5 (lead) | Writes the fixed interfaces (plan Task 1), does the ribbon wiring in `app.rs`, hands out work, checks results, runs the CI steps, installs the test build |
| Opus (Claude) | The drag control and Section View with hatch (2.2, 2.3), then the real window's device recovery (2.6) |
| Sonnet A | Split's `offset` in the engine and panel (2.4), then Split and Pattern speed (2.8) |
| Sonnet B | Measure and Properties, with the engine's body-info command (2.5) |
| Haiku | Send to Slicer and To STEP (2.5), 1.x's update skip (2.7) |
| Sonnet B, last | The no-silent-buttons test and fixing what it finds (2.5) |
| Hermes, Nemotron, Particle (free), Haiku | Read-only reviews. Free models first; Haiku when they run out. |
