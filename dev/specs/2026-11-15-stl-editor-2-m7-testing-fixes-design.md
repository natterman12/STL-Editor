# STL Editor 2.0 Milestone 7: what the M6 test build showed, and SolidCraft-style navigation and sketching (design)

Status: approved by Nathan, 2026-11-15. Follows `dev/specs/2026-11-14-stl-editor-2-m6-finish-design.md`. The rules of `dev/specs/2026-11-12-stl-editor-2-m4-app-design.md` section 2 still apply (smoothness, tests, both editions). SolidCraft is a reference for how things behave only: no code, theme values, icons or fonts from it, no dependency, no credit. Where this spec changes 1.x behaviour on purpose, the change is listed in `rust/tests/EXCEPTIONS.md` as "Accepted by Nathan (2026-11-15)".

## 1. Intent

Nathan tested the M6 build (`stl-editor-2.exe` from ff2c567) on 2026-11-15 and reported, in his words:

- The browser tree does not always let him select features and sketches.
- Orbit still doesn't work properly. He wants SolidCraft's orbit experience copied.
- Measure should draw dotted lines between the two points in the X, Y and Z directions, making a triangle.
- Section View: get rid of the red face once it's cut, and stop the grid lines showing through the part.
- Shell doesn't always make square cuts; the edges show as chamfered.
- Selected faces still show after a feature is done.
- Moving a part doesn't snap to the grid or the build plate. The sketch doesn't follow the moved part, and the blue selected face stays in the old place.
- Support Fins and Draw Fins crash the app.
- Mirror doesn't show the X/Y/Z planes, and the planes never show when picking for a tool that needs one.
- Living Hinge doesn't make clear what it does or what to pick.
- Snap Fit (and similar tools) must let him pick any part in the viewport; today he has to use the tree, which doesn't always work.
- Hole and Thread: clicking a free point is good, but he also needs exact placement: on a sketch line or construction line, and by measuring from a point to where it must go.
- Sketch lines blend into the colour of the selected plane or face; make them like SolidCraft's. Use SolidCraft's sketch measurements: the total size next to the cursor, the size along each X/Y edge, and Tab to type a size.

His decisions:

- **Orbit and mouse buttons as SolidCraft has them set on his PC** (`%APPDATA%\SolidCraft\data\app.ron`): Orbit around = View centre, turntable (up stays up), the **Fusion** mouse buttons, zoom toward the cursor. These become 2.0's defaults.
- **Hole and Thread placement: approach A** (1a). He measures from **both** part edges and sketch lines.
- **Also borrow from SolidCraft:** sketch tracking guides, cursor coordinates, grid snap that follows the visible grid, constraint icons, the cut preview tint, and keeping the view when a sketch is finished.
- **Not borrowed:** SolidCraft's AI assistant and automation link, the other mouse presets (SolidWorks, Blender…), assemblies, render settings.

Goals: every item above works, each covered by a test that would have caught it; the app never crashes on a graphics problem; drawing and placing things exactly feels like SolidCraft.

Non-goals: publishing, a version bump or tag, 1.x changes, fillet/chamfer, spline/ellipse, colour 3MF.

Anti-goals:

- Any crash, or a frozen window, from Support Fins, Draw Fins or a graphics driver hiccup.
- The view jumping or flying off the part when orbiting or zooming.
- A placement tool that looks precise but stores a different point than the one shown.
- Projects from 1.x or earlier 2.0 builds opening differently.

## 1a. Approaches

| Part | Approaches | Picked |
|---|---|---|
| Hole/Thread placement | **A.** Click a spot; the click snaps to sketch geometry; then pick one or two part edges or sketch lines and type the distance from each. **B.** Holes only on points placed and dimensioned in a sketch (the SOLIDWORKS Hole Wizard way): precise, but an extra sketch for every hole. | **A, by Nathan** |
| Orbit and buttons | SolidCraft's View centre plus Fusion buttons as the default, versus keeping 1.x's left-drag orbit with Fusion as an option | **SolidCraft/Fusion default, by Nathan** |
| Sketch sizes | Value boxes on the drawing next to the edges they measure (SolidCraft), replacing the fixed HUD box | One sensible way |
| Shell corners | Voxel hollow kept, inner vertices then snapped onto the inward-moved flat faces (square where the part is flat, unchanged where it is curved), versus a full polyhedral offset (exact, but fails on self-intersecting offsets that print parts often have) | **Snap, builder's call (2.7)** |

## 2. What milestone 7 builds

Priority order for the run: 2.1 first (crash), then the rest in any order the plan sets.

### 2.1 No crash from Support Fins, Draw Fins or the graphics driver

What the log shows (`%LOCALAPPDATA%\STL Editor 2\log.txt`, Windows Application log, 2026-11-15 07:51 and 07:53): egui-wgpu panics in `Renderer::update_buffers` ("Failed to create staging buffer for index data", `queue.write_buffer_with` returned `None`) on Nathan's real GPU during Support Fins / Draw Fins. The panic happens inside winit's window procedure, which Windows cannot unwind through: the process ends with 0xc000041d / 0xc0000005. So M6's `StlApp::run_window` restart never runs in the real window; it only works in the test's stand-in loop.

Two fixes, both required:

1. **Never panic on a refused upload.** Vendor egui-wgpu 0.37.0 under `rust/third_party/egui-wgpu` (via `[patch.crates-io]`, next to `manifold-rust`, with a `VENDORED.md` note like its one) with one change: when a staging write is refused, skip painting egui for that frame and record a device loss the app can read, instead of `panic!`. The app's `device_frame` then sees the loss on the next frame and takes M6's restart path (autosave, message, restart). Keep the patch minimal and noted in its `VENDORED.md` (what changed and why), so it can be dropped when egui-wgpu fixes it. The refused-upload probe from 158a544/ae30899 (`render_state_refuses_uploads`) stays and is how the app reads the loss.
2. **Find and fix why fins makes the device refuse uploads.** Reproduce with Nathan's steps (open a part, Support Fins, generate; and Draw Fins, draw a fin). Log the device-lost reason and message to `log.txt`. Suspects to check: the Support Fins engine runs in a WebView2 (`stl-platform` fins host, `wry`) whose GPU process shares the adapter; a very large preview upload; a GPU timeout (TDR) from a long draw. Fix the cause. If the cause is outside the app (a driver resetting), the restart path from fix 1 is the answer and the message says so.

**Where it can be reproduced (my call):** this PC is Nathan's, with his GPU; the lead reproduces the crash here with the installed build before and after the fix. CI only has WARP, so the GPU fins check is a local verify step, not a CI test.

Tests: the window test drives a refused staging write through the **real** paint path without `catch_unwind` around it (no panic, loss reported, restart marker written); a Support Fins and Draw Fins run on the GPU with a large part (no loss, no panic).

### 2.2 Orbit and mouse buttons like SolidCraft

**Mouse buttons (Fusion), the new default in Model mode:**

| Input | Does |
|---|---|
| Middle drag | Pan |
| Shift + middle drag | Orbit |
| Right drag | Orbit |
| Right click (no drag) | The menu, as now (body menu on a body) |
| Wheel | Zoom toward the cursor |
| Left click | Select, as now |
| Left drag | Nothing (no orbit); box select if 2.0 has it |
| Trackpad two-finger scroll | Pan; with Shift, orbit |

Sketch and Sculpt modes already use middle-pan / right-orbit and stay as they are, plus Shift + middle orbit. The help table (`app.rs` help text) and the user guide's mouse section change to match. **My call:** Fusion is the only mapping; there is no setting to bring back 1.x's left-drag orbit (Nathan excluded other presets). EXCEPTIONS.md: "1.x orbits with the left button."

**Orbit around (Settings › Navigation):** View centre (default), Origin, Under the cursor, as SolidCraft offers:

- **View centre:** the turn is about the point in the middle of the view at the depth of the model: where Home/Fit centre the model, projected to the camera target's depth. Panning moves it with the view. A turn that keeps that point where it was on screen keeps using it; after a pan or zoom in the same drag it is chosen again.
- **Origin:** about (0, 0, 0).
- **Under the cursor:** about the model point under the cursor when the drag starts; over empty space, about the middle of the part in view (never a far-off point: SolidCraft #2616).
- Turntable: up (+Z) stays up; the turn never rolls the view.

**Zoom toward the cursor:** toward the model's surface under the cursor, else toward the point on the cursor's ray at the target's depth, never behind the eye or far off. This replaces today's `zoom_to_cursor`, which moves the target along the cursor ray and so drags the orbit centre off the part: the likely cause of "orbit not working properly".

M6's `look_at_plane` target rule (orbit about the part after a tool looks at a face) stays, now as part of View centre.

Tests: orbit about View centre keeps the model's framing centre fixed on screen; ten wheel zooms toward a corner of the view then an orbit keep the part in view; Origin keeps (0,0,0) fixed; Under the cursor keeps the picked point fixed and keeps a far-off part in view (the SolidCraft #2616 case); each binding row above, through egui events.

### 2.3 Sketching like SolidCraft

- **Colours.** Sketch lines get a colour that stands out against the sketch plane fill and the selected-face blue: today both lines (`SKETCH_COL` 0xd9480f) and the plane fill (`PLANE_COL` 0xe8763a) are orange. **My call:** lines near-white in the dark theme and near-black in the light theme, 2 px; construction lines dashed in a mid grey; selected lines in the selection blue; fully constrained lines a slightly different shade, as SolidCraft does. The plane fill while sketching becomes fainter. Values are 2.0's own, not SolidCraft's.
- **Value boxes on the drawing** (replacing the fixed HUD box at the viewport's top left): once a shape's first point is down, a small editable box sits next to each edge it measures and follows the cursor: line length and angle; rectangle width and height (each beside its own edge, along X and Y); circle diameter; polygon radius and sides; arc radius and angle. Typing goes straight into the first box and locks that value; Tab moves to the next box, Shift+Tab back; Enter places the shape with the typed values. Values accept expressions and parameters as today (`typed.rs`, `=width/2`). **Typed values become sketch dimensions** (2.0 already has sketch dimensions, `sketch/dims.rs`).
- **The total size next to the cursor:** for a line, its length; for a rectangle, "W × H"; for a circle, its diameter.
- **Cursor coordinates:** the cursor's X, Y on the sketch plane, small, next to the cursor (Settings › Sketch, on by default, as on Nathan's SolidCraft).
- **Tracking guides:** when the cursor lines up horizontally or vertically with an existing point, line end, midpoint or centre, a dotted guide line from that point, and the cursor snaps to the alignment. The points it tracks are the ones on the sketch, plus any the cursor rested on briefly (SolidCraft's tracking points).
- **Grid snap that follows zoom:** the grid's spacing changes with zoom, and snapping uses the spacing shown.
- **Constraint icons:** small glyphs beside constrained lines (horizontal, vertical, equal, parallel, perpendicular, tangent, coincident, fixed), hidden when lines are dense; clicking one selects that constraint so Delete removes it.
- **Keep the view when a sketch is finished** (Nathan's SolidCraft: "keep"): finishing or cancelling a sketch doesn't fly the camera back.

Tests: each value box per tool (placement next to the right edge, Tab order, Enter places, typed value becomes a dimension); guides appear and snap; coordinates follow the cursor in mm and inches; constraint icons per constraint type; finishing a sketch leaves the camera unchanged; snapshots of a sketch on a face in both themes (lines visible against the fill).

### 2.4 Hole and Thread placed exactly (approach A)

Applies to Hole's Positions and Thread's On/Position, and to any later tool that places points on a face.

- **Snap to sketch geometry:** while placing, the click snaps (with a marker) to visible sketch points, line ends, midpoints, circle and arc centres, and to the nearest point on a sketch line or construction line. The snapped point is projected onto the face along its normal.
- **Distance along a sketch line:** after snapping onto a line, a value box beside it gives the distance from the line's start; typing moves the point along the line.
- **Distance from edges:** after placing a point, a "From" row in the panel (and on the point in the viewport): pick one or two straight part edges or sketch lines; a distance box appears for each, drawn as a dimension in the viewport. Typing a distance moves the point perpendicular to that edge, in the face's plane. With two edges, both distances hold.
- **What is stored:** the resulting point as today (so the feature runs the same and older projects are unchanged), plus, for each point, the references and distances used, so reopening the step shows them and a parameter change re-places the point. A reference that can no longer be found (the edge was removed) keeps the last point and says so in the panel; no silent move.
- **Part edges are found again after a change (Nathan, 2026-11-15: "yes", best effort).** The mesh has no lasting edge IDs, so a part-edge reference stores the edge's two end points, its direction and the plane of the face the hole sits on. When the project runs again, the edge is the straight feature edge on the body that lies in a face parallel to that plane (within 1°), runs parallel to the stored direction (within 1°), and is nearest the stored line; ties go to the longest edge, then the lowest end point. If no edge passes, or two pass and are both within 0.01 mm of each other's distance, the reference counts as missing (below). Sketch lines have IDs and always re-place.
- Deterministic: the point is worked out from the stored references and values only.

Tests: snap to each sketch geometry kind; distance along a line; one and two edge distances give the exact point (to 0.001 mm) on a 40 × 30 plate; reopening shows the references; a parameter change on the plate re-places the hole; a missing reference keeps the point and shows the message; projects without references open unchanged.

### 2.5 Measure draws the triangle

Between the two picked points: the straight line solid with its distance labelled at its middle, plus the X, Y and Z legs as dotted lines from the first point (X, then Y, then Z) to the second, each in its axis colour (X red, Y green, Z blue) with its length labelled. Legs of zero length are left out. Markers and labels draw on top of the part, as now.

Tests: the legs' end points and labels for a known pair of points; a pair that differs only in X draws one leg.

### 2.6 Section View without the red face or the grid

- The cut face is drawn in the body's own colour, a little darker, with the diagonal hatch lines kept (Nathan liked SolidCraft's). No red. With Hatch off, plain darker body colour.
- The grid never shows through a body or on the cut face: it is drawn under the bodies and caps, depth-tested against both. Today it is drawn on the cap on purpose (a 1.x "hatch", `renderer.rs` 940-942); that goes.

Tests: snapshots (WARP) of a cut box with Hatch on and off: no red pixels, no grid lines inside the cut face or through the part.

### 2.7 Shell makes square corners on flat parts

**Shell and Hollow both get it (my call):** they share the code, and a hollow with bevelled corners would be the same complaint. Today Shell and Hollow use the voxel distance field (`stl-mesh` `voxel::hollow`, also 1.x's method), so inside corners come out bevelled. **Picked approach:** keep the voxel hollow, then snap the inner surface onto the flat faces moved inward: find the body's flat regions (coplanar triangle groups larger than a set size); for each inner-wall vertex, find the moved planes it lies within one voxel of; project onto one plane, onto the intersection line of two, or onto the corner point of three. Curved areas are unchanged. The result stays watertight and deterministic. EXCEPTIONS.md: 1.x bevels Shell corners.

Tests: a 20 mm cube shelled 2 mm with the top opened has a cavity of exactly 16 × 16 × 18 (volume within 0.1%, corners square to 0.01 mm); an L-shaped part's inner corner is square; a sphere's shell volume is unchanged from today's result.

### 2.8 Highlights follow the part and clear when done

- A face highlight is stored as (body, face triangles) and drawn from the body's current mesh and transform, so it moves with the body and is correct after any rebuild. Today it is a copy of world-space triangles taken at pick time.
- Every face and edge highlight a tool made is cleared when the tool closes (OK, Cancel, Escape, another tool) and when a project opens.

Tests: OK clears; Cancel clears; a highlighted face moved by the gizmo is drawn at its new place.

### 2.9 Move snaps, and sketches move with their part

- **Snap:** translation snaps to the grid step shown (Shift for free, like rotation's 5°). Z also snaps to the build plate (Z of the lowest point = 0) within a few pixels. A **Drop to plate** button in the Move panel.
- **Sketches follow:** a sketch made on a body's face (its plane picked on that body) belongs to that body. A later Move, Rotate or Mirror of that body moves the sketch with it: the viewport draws it moved, and any later feature that uses the sketch uses the moved plane. Sketches on origin or user planes don't move. **My calls:** following is worked out when the project runs (the sketch's plane, with its `body` key, transformed by later Move/Rotate/Mirror steps of that body); the stored plane is never rewritten. With Mirror keeping the original, the sketch stays with the original. Holes and threads placed with references (2.4) re-place from them; holes placed by a plain click keep their stored point, as today and in 1.x. Older projects keep their behaviour unless the sketch's plane was picked on a body (then it follows, as the user would expect; listed in EXCEPTIONS.md).

Tests: snapping to grid and plate; Shift is free; a sketch on a face follows a move, and an extrude of it after the move lands on the moved face.

### 2.10 Planes show whenever a tool needs one

When a tool field that takes a plane is active (Mirror, Split, Section, Living Hinge, Snap Fit's On, Pattern's mirror plane, a new sketch), the X/Y/Z origin planes (and user planes) are shown and pickable, then go back to how they were when the field is filled or the tool closes. `wants_planes()` (`tools/fields/plane.rs`) exists but is never called; wire it.

Tests: opening Mirror shows the planes; picking XZ fills the field and the planes go back; the browser's Origin eye state is unchanged afterwards.

### 2.11 Pick bodies in the viewport for any tool

- A tool with a Bodies field: clicking a body in the viewport while that field is active adds it (Ctrl/Shift toggles). After the last pick field above it is filled, Bodies becomes the active field (today `advance_field` skips fields with a placeholder, so Bodies never activates).
- When the active field takes faces, a body click fills that field as now; clicking the Bodies box (or Tab) makes the next body click a body pick.
- The tree's body rows fill the active Bodies field the same way.

Tests: Snap Fit: pick On, then click two bodies in the viewport: both listed; the tree does the same.

### 2.12 The tree and timeline select reliably

- The whole row is clickable (today only the name text).
- Sketch rows are selectable: a click selects the sketch and highlights it in the viewport; double-click edits it.
- Timeline feature items: a click selects the feature and highlights what it made; double-click edits (as now).
- Clicks register even while the engine is busy; nothing is ignored silently.

Tests: clicks at the row's empty right side select; sketch and feature selection through egui events.

### 2.13 Living Hinge says what it does

- The panel starts with two numbered lines: "1. Pick the face or plane the hinge runs across." "2. Set where it sits and how thin it is."
- A live preview in the viewport: the slot drawn in the cut tint (2.14) and the hinge line, with an arrow showing which way the part will flex.
- The hint names the result in plain words.

Tests: preview appears once a plane is picked; panel text as above.

### 2.14 Cut preview tint

While a tool that removes material is open (Extrude/Revolve cut, Hole, Thread cut, Living Hinge, Split's removed side when one side is discarded), the cutting shape (the hole, the slot, the cut extrusion) is drawn in a translucent red tint where it will cut, before OK. **My call:** the cutter shape itself, not a worked-out boolean of the removed material, so previews stay instant on big meshes and the web. Settings › Display, on by default.

Tests: snapshot of a hole preview with the tint; the setting off removes it.

## 3. Constraints

- Both editions: everything here works in the web edition too (wasm build, WebGL2 and WebGPU), except what touches the Support Fins host (desktop only).
- New stored fields are additive; older projects open and run unchanged (golden round trip).
- Every 1.x difference goes in `rust/tests/EXCEPTIONS.md`.
- No new warnings (native and wasm), WARP snapshots updated where looks change, `uibench` rows no slower.
- Plain-English text in the app.

## 4. Left to the builder

- The plan's task split and the PandaCode team (Opus, Sonnet A, Sonnet B, Haiku; free reviewers), as in M6.
- Exact sizes, colours and spacings within the calls above.
- How the vendored egui-wgpu patch reports the loss to the app.
