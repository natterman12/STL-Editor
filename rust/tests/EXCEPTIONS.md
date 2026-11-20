# Deliberate differences from 1.x

Every place where 2.0 does not match 1.x on purpose, with the reason.

## Expressions (`stl-engine::params`)

- **Non-finite results are errors.** The result of every function call, `^` and `%` is checked
  with `is_finite()`. A non-finite result gives `"{src} doesn't give a number"`. Reason: this is
  what 1.x does on its ValueError/OverflowError path; in Rust `sqrt(-1)` is NaN instead of an
  exception, so without the check `max(sqrt(-1), 3)` could give a wrong number.
- **`0^-1`** gives `"{src} doesn't give a number"`. Reason: in 1.x this raises ZeroDivisionError,
  which escapes the expression engine as a crash instead of a message.
- **Nesting deeper than 200 levels** gives `"{src} is nested too deeply"`. Reason: 1.x hits
  Python's RecursionError (an uncaught crash); Rust would overflow the stack.
- **Long runs of unary minus/plus signs, long `^` chains and long chains of parameters that refer
  to each other** give their result. 1.x crashes with RecursionError: for runs of signs or `^`
  from about 1000 in a row, and for parameter chains from a few hundred levels when the list has
  them in reverse order. Reason: 2.0 evaluates these without recursion, so there is no limit to hit;
  a crash is never the right answer. Brackets and function calls keep the 200-level limit above.
- **Panics in features on wasm32 abort the worker.** Reason: on native targets a panic is caught
  per feature and shown as that feature's error, but wasm32 has no unwinding here. Handled in
  milestone 4.

## Document (`stl-engine::document`)

- **An unexpected failure inside a feature** shows `"Internal error: {panic message}"` on that
  feature; 1.x shows `"{ExceptionType}: {e}"` (for example `ValueError: math domain error`).
  Reason: Rust has no exception types. A bug in a feature is a panic, which carries only a
  message, and "Internal error" says plainly that it is our fault and not something to fix in
  the model. A feature type the build doesn't know (1.x: `KeyError: 'warp'`) shows
  `"Unknown feature warp."`, the message 1.x already uses when adding one.
- **`preview` with a missing edit id** returns an error result `"That feature no longer exists."`
  (with the current state) instead of raising. Reason: the plan's signature isn't a `Result`;
  not visible to the user.
- **`body_name` for a missing body** returns `""` instead of raising. Reason: same, the
  signature isn't a `Result`; not visible to the user.

## Project files (`stl-engine::stlx`)

- **`"This isn't an STL Editor project."`** when the file isn't a zip, has no `document.json`,
  or `document.json` isn't readable as a project. Reason: 1.x lets `BadZipFile` / `KeyError` /
  `JSONDecodeError` escape as crashes.
- **`"This project was made by a newer version of STL Editor."`** when `format` is above 1.
  Reason: 1.x doesn't check the format number and would misread a newer file.
- **`"The imported file is missing from the project."`** when a source listed in `document.json`
  has no entry in the zip. Reason: 1.x raises `KeyError` while opening; this is the message 1.x
  already shows for a missing source during evaluation.
- **Unknown top-level keys in `document.json` are kept and written back on save.** Reason: 1.x
  drops them; a project from a newer or other version keeps its data this way.
- **`"Couldn't open the file: ..."` / `"Couldn't save the project: ..."`** for file system
  errors. Reason: 1.x lets `OSError` escape; Rust needs a message in its `FileError`.

## Messages in features (agreed with Nathan on 2026-11-10)

- **`"Plane size must be a number."`** when a plane's size is text that isn't a number. 1.x
  shows Python's own wording, `could not convert string to float: 'abc'`. Reason: the 1.x text
  is a programming error message that means nothing to someone who prints things.
- **`"Choose a body first."`** when Split, Repair, Separate or Hollow has no body chosen. 1.x
  shows `"Body None no longer exists."`. A body that was chosen but has since been deleted still
  shows `"Body '<id>' no longer exists."`, as in 1.x. Reason: nothing was deleted here, so "no
  longer exists" is misleading. Combine already says `"Choose a target body and at least one tool
  body."` in both versions, and Import has no body (a missing source file keeps its 1.x message),
  so neither changes.

## Reading files (`stl-io`)

- **A glTF (`.gltf`) file that points to other files** gives `"This glTF file points to other
  files. Save it as a .glb, or embed the data, and try again."` 1.x fails with
  `TypeError: 'NoneType' object is not subscriptable`, because no folder is given to find the
  other files in. Reason: say what to do instead of crashing.
- **glTF files with Draco or meshopt compressed meshes** give `"This glTF file uses compressed
  meshes, which can't be read."` 1.x doesn't have the decoder installed, prints a warning and
  loads placeholder zeros as if they were the model. Reason: a made-up shape that looks like a
  real one is worse than an error.
- **A malformed STL file** gives `"Could not read this STL file."` 1.x can crash on some
  broken files. Reason: always show a message, never a crash.
- **A triangle that points at a vertex that doesn't exist** (OBJ and PLY files) gives no solid
  or `"Could not read this OBJ file."` / `"Could not read this PLY file."` 1.x raises
  `IndexError`. Reason: same as above, a message instead of a crash.

## Connectors and repair

- **`"Unknown connector X."`** when Split gets a connector type it doesn't know. 1.x raises
  `KeyError: 'X'`. Reason: a message instead of a crash. The editor never sends one; this only
  matters for hand-edited projects.
- **One path of the local repair returns no result.** When a shell needs reversing and the
  reversed shell isn't a solid, 1.x fails with `TypeError`; 2.0 gives up on the local repair, so
  the Repair feature then tries its next step (or says `"Quick repair wasn't enough. Use
  Rebuild."`). Reason: 1.x's crash is a bug, not a behaviour anyone depends on.
- **The kernel gives up after 100 passes of `dedupe_edges`.** Manifold 3.5.4 repeats until a
  pass finds nothing, which can in theory loop forever. 2.0 stops after 100 passes and then
  reports the mesh as not closed. Reason: an import must never hang. Meshes that import need a
  handful of passes, so the limit isn't reached in practice.

## Turning a mesh into a solid, and the repair steps it changes

Nathan agreed to all of these on 2026-11-10. Each case below is listed in `EXCEPTIONS` in
`crates/stl-mesh/tests/repair.rs` with 2.0's numbers (closed or not, triangle count, area and
volume). That test fails if 2.0's result changes, and also if the case starts to match 1.x
again. No feature parity case changes because of this, so `tests/parity/exceptions.json` has no
entry for it.

- **Edges shared by more than two pairs of triangles are separated without moving any corner.**
  1.x uses Manifold 3.5.4, whose `DedupeEdges` step has a bug: it lists the doubled edges once,
  then repairs every one on the list, even those that an earlier repair has already fixed.
  Repairing an edge that is no longer doubled moves triangle corners onto a copy of another
  vertex, so the solid changes shape (often a thin spike or a dented face). 2.0 checks each
  edge again just before it repairs it and skips the ones that are already fixed, so the shape
  stays as it was. Reason: a repair should not change the shape of the model. The other steps
  of the 3.5.4 import are kept as they are. This only matters for meshes that have such edges,
  which in the tests are meshes with many triangles left out, after their holes were closed.
- **Nine results of the local repair change because of it.** In each, the mesh had every 4th or
  7th triangle left out, the holes were closed, and the closed mesh was turned into a solid.
  Every result is a closed solid in both versions.

  | Case | 1.x triangles | 1.x area | 1.x volume | 2.0 triangles | 2.0 area | 2.0 volume |
  |---|---|---|---|---|---|---|
  | `holed_block.stl@drop4` | 352 | 3860.626 | 5115.261 | 362 | 3555.988 | 5119.270 |
  | `holed_block.stl@nan_drop4` | 392 | 3746.201 | 5111.969 | 398 | 3684.319 | 5120.083 |
  | `holed_block.stl@nan_drop7` | 368 | 3045.069 | 4770.766 | 370 | 3039.111 | 4772.938 |
  | `holed_block.stl@nan_flip3_drop7` | 358 | 3040.301 | 4740.257 | 356 | 3076.217 | 4754.637 |
  | `touching_cubes.stl@drop4` | 26 | 1178.042 | 1448.718 | 28 | 1194.640 | 1397.436 |
  | `touching_cubes.stl@nan_drop4` | 32 | 1181.763 | 1421.267 | 28 | 1194.640 | 1397.436 |
  | `two_blobs.stl@drop4` | 1406 | 507.236 | 611.353 | 1436 | 529.756 | 610.978 |
  | `two_blobs.stl@flip3_drop7` | 1288 | 436.509 | 399.845 | 1294 | 438.308 | 399.989 |
  | `two_blobs.stl@x5_drop7` | 6460 | 2147.994 | 3108.606 | 6464 | 2148.897 | 3108.545 |

- **Three more results of the local repair differ, on two spheres with a stray triangle of
  not-a-number coordinates.** These did not match 1.x even while 2.0 still copied the 3.5.4
  bug, and the exact step where they part was not found. Closing the holes of these meshes
  leaves edges that four, six or eight triangles use, and the two kernels separate those edges
  in a different way. Every result is a closed solid in both versions, and the volumes agree to
  within 0.02%. Reason: the results are as good as those of 1.x (for `nan_drop4` 2.0's surface
  is much smoother: area 534.7 against 607.2 for the same volume), and such meshes are far from
  anything a real file contains.

  | Case | 1.x triangles | 1.x area | 1.x volume | 2.0 triangles | 2.0 area | 2.0 volume |
  |---|---|---|---|---|---|---|
  | `two_blobs.stl@nan_drop4` | 1548 | 607.240 | 611.125 | 1540 | 534.716 | 611.026 |
  | `two_blobs.stl@nan_drop7` | 1314 | 428.826 | 621.731 | 1318 | 429.003 | 621.758 |
  | `two_blobs.stl@nan_flip3_drop7` | 1332 | 447.923 | 620.858 | 1336 | 448.100 | 620.884 |

## Differences found by the parity tests

Each of these is also an entry in `tests/parity/exceptions.json` (the case, the reason and
2.0's values), so the parity run passes by exception instead of hiding the difference.
Add new ones below, with the 1.x behaviour, the 2.0 behaviour and the reason.

## Mesh tools (batch 3)

- **Offset on a detailed body** (`offset/offset_detailed_body`) differs by 0.117 percent in area:
  1.x gives 1399.6306720744435 mm² with 5946 triangles; 2.0 gives 1397.9951396150825 mm² with
  5938 triangles. Reason: Manifold C++ 3.5.4's convex hull uses a float test that leaves an
  inside-out pocket of 8 triangles in 1.x's Minkowski sum (about 0.819 mm² counted twice, so
  about 1.64 mm² of false area). The vendored manifold-rust deliberately fixes that bug with an
  exact orientation test (see the comment above `is_above` in
  `rust/third_party/manifold-rust/src/quickhull.rs`), so 2.0's surface is clean. This is a 1.x bug
  that 2.0 doesn't copy (agreed with Nathan on 2026-11-11).
- **1.x desktop `_hollow_pitch` now takes the diagonal from the solid's bounding box at full
  precision**, as the web engine does (commit e6ad2b9). Before, it used float32 points, a
  difference of about 1e-6 mm on the test models. Reason: a 1.x fix so Hollow, Shell and Lattice
  share the web rule when the golden files are made.
- **`"The sculpt strokes are not valid."`** when the sculpt strokes JSON is malformed. 1.x would
  crash with a Python error. Reason: a message instead of a crash; the editor never sends
  malformed strokes.


## Printing tools and Support Fins (batches 4 and 5)

- **Linux builds use the system WebKitGTK for Support Fins.** On the Linux target only, the
  pure-Rust check allows these crates to use `pkg-config` or `system-deps` at build time:
  `atk-sys`, `cairo-sys-rs`, `gdk-pixbuf-sys`, `gdk-sys`, `gdkwayland-sys`, `gio-sys`,
  `glib-sys`, `gobject-sys`, `gtk`, `gtk-sys`, `javascriptcore-rs-sys`, `pango-sys`,
  `soup3-sys` and `webkit2gtk-sys`. The check prints the reason ("system WebKitGTK for Support
  Fins") and still fails for any other C build tool. Reason: spec section 4. `wry` is built
  with x11 off, so `gdkx11-sys` and `x11-dl` stay out.
- **macOS runs Support Fins in Boa (slow mode) until milestone 4.** WKWebView must be made on
  the main thread, and the app that provides that hook comes in milestone 4. `wry`'s macOS
  backend also builds C code (`objc2-exception-helper` uses `cc`), so `wry` and `tao` are
  only built for Windows and Linux for now. Milestone 4 has to solve both.
- **A Support Fins call on the webview host stops after 120 seconds** with "Support Fins: the
  engine stopped responding.", and the next call starts a fresh host. 1.x has no limit and
  could hang. On Windows a crashed WebView2 process is noticed within about 0.2 s; on Linux
  only the 120 s limit catches it, and because GTK can't be started on a second thread, a dead
  Linux webview stays dead until the next start, which then falls back to Boa. Boa has no
  limit. Reason: an improvement (spec section 2).
- **Engine error text** (accepted by Nathan on 2026-11-11). When the Support Fins engine throws, 1.x shows
  the first line of mini-racer's message, for example `Support Fins: <anonymous>:1: Error:
  boom`. 2.0 shows `Support Fins: boom`. Reason: `<anonymous>:1: Error:` is noise from 1.x's
  JavaScript runtime, not part of the engine's message. No golden case has an engine error.
- **An update for a different engine `api` is recorded.** 1.x ignores it silently; 2.0 also
  keeps the current engine but records `different api` in the update state's `error`.
  Reason: Review Focus 3 of the batch plan.

## Messages in features (batches 4 and 5)

These replace a 1.x crash or Python error text. The editor never sends such values; they only
come from a broken or hand-edited project file. Reason for each: a plain message instead of a
programming error, the same practice as batch 1's `"Plane size must be a number."`.

- **`"point must be a number."`** (Resize Hole) when the click point isn't three numbers. 1.x
  raises numpy's `TypeError` or `ValueError` in `nearest_triangle`.
- **`"Click the curved wall of a round hole or pin."`** (Resize Hole) when the body has no
  triangles. 1.x gives this message only when the picked region is under 6 triangles; with no
  triangles it fails first in numpy (`attempt to get argmin of an empty sequence`).
- **`"count must be a whole number."`** (Fit Test) when `count` isn't a whole number. 1.x raises
  Python's `int()` error (`invalid literal for int() with base 10: ...`). Python's `int()` also
  takes text such as `"6_0"` (60); 2.0 rejects it. JSON numbers still round toward zero, as in 1.x.
- **`"normal must be three numbers."`** (Lay Flat) when a given `normal` isn't three numbers.
  2.0 checks this before doing anything. 1.x uses `p["normal"]` as it is and fails later in
  the geometry with a numpy error.
- **`"offset must be a number."`** (Thread) when the plane offset isn't a number. 1.x raises
  numpy's `ValueError` from `np.asarray(..., float)`.
- **`"Diameter, pitch and length must be positive."`** (Thread) also when a size is not a number
  (NaN). 1.x only checks `<= 0`, so NaN gets past the check and fails later or builds nothing
  useful.
- **`"Could not build this thread. Try a different size or pitch."`** (Thread) also when the
  thread would need more than a million turns or more vertices than a mesh can hold. 1.x has
  no limit and can run out of memory instead.
- **An empty `margin` in Fit to Bed** uses the default 5 mm. 1.x raises Python's
  `could not convert string to float: ''`.
- **`"Body ... no longer exists."` for a Draw Walls body that is neither text nor empty** (for
  example a number) shows the value as JSON (`Body 5 no longer exists.`). 1.x shows Python's
  `repr`, which is the same for numbers but differs for lists and other values.

## Bench (accepted by Nathan on 2026-11-11)

- **The Resize Hole bench row uses `rust/tests/models/hole_and_pin.stl`**, not the bench mesh,
  on both sides. Reason: the bench mesh's through-cut is square, so it has no round wall for
  Resize Hole to pick.

## File export (milestone 3)

- **Faceted STEP export on desktop** (accepted by Nathan on 2026-11-11). In 1.x only the web
  edition writes faceted STEP; desktop 1.x only has the SolidCraft "To STEP" hand-off. 2.0
  offers faceted STEP in both editions, with the same bytes as the web edition's `step.js`.
- **`"Can't export .{ext} files."`** (accepted by Nathan on 2026-11-12) when an export path ends in an
  extension that isn't `.stl`, `.3mf`, `.obj` or `.step`. 1.x hands the path to trimesh, which
  raises its own error or writes a format the editor doesn't offer.
- **3MF `p:UUID` values come from a seeded generator** (accepted by Nathan on 2026-11-12). trimesh writes a
  random version 4 UUID on each object, on `<build>` and on each item, so no two 1.x exports
  are the same. 2.0 writes version 4 UUIDs in the same places, seeded from the clock at
  export time. The zip's compression settings may also differ from trimesh's
  (`compresslevel=5`); the XML inside is the same.
- **Closed in milestone 4a: the STEP date on the web build.** It was `1970-01-01T00:00:00`
  (accepted by Nathan on 2026-11-12). The app now passes the page's clock (`Date`, UTC) with
  every export, so the web build writes the real time, as `step.js` does.
- **The export base name follows the web edition's rule** (accepted by Nathan on 2026-11-12). 1.x has two
  rules. The web edition (`stepResult` and the export routes) uses the body's name when the
  bodies being exported come to one body, and otherwise the project path, then the title, then
  `model`, without `.stlx`. Desktop 1.x `r_export` looks at the ids it was given, so with no ids
  and one visible body it suggests the project name or `model` instead of the body's name, and it
  never uses the title. 2.0 uses the web rule in both editions. Golden case `hidden_body`
  (base `keep`) shows the difference: desktop 1.x would suggest `model`.

## The app: shell and viewport (milestone 4a)

Nathan answered all of these (2026-11-12, 2026-11-13 and 2026-11-14).

- **Accepted (2026-11-12): `x11-dl` is on the Linux pure-Rust allow-list.** It is winit's X11
  backend. Its build script asks `pkg-config` for the X11 library folders, and no feature drops
  that build dependency. It loads the X11 libraries at run time (`xtask/src/pure.rs`).

- **Accepted (2026-11-12). New text: "The engine stopped unexpectedly. Your last saved project is safe."** with a
  **Restart** button, shown when the engine thread or Web Worker dies. 1.x has no engine
  that can die on its own. Restart reopens the last opened or saved project.
- **Accepted (2026-11-14): the busy overlay has a Cancel button.** 1.x's busy overlay (`index.html` `#busy`) has
  none. Cancel stops the running rebuild at the next step and leaves the project exactly as
  it was before the command (undo history included); the status line then says "Cancelled."
- **Accepted (2026-11-14): opening a `.stlx` can't be cancelled,** and its progress shows only once the file is
  loaded, because the project loader rebuilds every step inside the load. Opening a mesh file
  can be cancelled.
- **Accepted (2026-11-14): on the web, Cancel needs JS Promise Integration (JSPI)** to stop a rebuild that is already
  running (Chrome and Edge 137+, Firefox 153+, Safari 27+). On a browser without JSPI the app
  still works, but a running rebuild finishes first and Cancel takes effect between commands.
- **Accepted (2026-11-14): the rollback marker can be dragged.** 1.x shows a resize cursor on the marker but has no
  drag; you roll back from the menu or the step buttons. 2.0 also rolls back to where the
  marker is dropped.
- **Fixed (2026-11-14): adding a step in the middle of the history is one undo step, as 1.x.** One Ctrl+Z took it out but left the marker rolled back to where it went in; `Document::add_feature_at` now records the marker as it was (test `a_step_added_mid_history_is_one_undo`).
- **Accepted (2026-11-12): Open Recent without a path until milestone 5. Recent files last only for this session,** and come back from the bytes in memory, so a
  Save after Open Recent asks where to save. Recent files that survive a restart, and the
  settings, are milestone 5.
- **Done in milestone 5: the Help menu has Check for Updates… and Update Settings… (desktop only).** Earlier milestones had neither; the update
  check arrived in milestone 5. About Support Fins arrived in milestone 4b.
- **Accepted (2026-11-12): plain-text Help and About layout.** The version comes from the 2.0
  build, the Ko-fi link sits below the text instead of inside it, and the heart isn't
  coloured.
- **Accepted (2026-11-14): About credits list what 2.0 uses** (egui and eframe, wgpu, the manifold-rust port, the
  Selawik font and the other Rust crates, each with its licence) instead of 1.x's Python
  libraries. 1.x's thanks and Ko-fi text are kept. Nathan asked for this on 2026-11-12.
- **Accepted (2026-11-13): a dropped `.stlx` is refused** with "Use File › Open for projects.", as in 1.x. The fixes
  plan asked for a dropped project to open and set the title; 2.0 keeps 1.x's behaviour.
- **Accepted (2026-11-13): the section cut face shows the floor grid** where the inside of the part is on or below
  the bed, as 1.x does (`viewer.js` draws the see-through grid after the solid cut face). The
  fixes plan asked for the cut face to hide the grid; 2.0 follows 1.x.
- **Accepted (2026-11-13): 2.0 opens straight in the start view for now.** 1.x flies the camera there over 320 ms during
  load. The fly-in moves to milestone 4c's look pass.
- **Accepted (2026-11-13): the web check for WebGL2 runs on SwiftShader** (Chrome's software WebGL2), because
  `--disable-features=WebGPU` alone leaves WebGPU on in Chrome 157. Hardware WebGL2 is not
  covered by the check. Nathan: enough for now.
- **Accepted (2026-11-14): ten constraint icons** (`c_coincident` to `c_symmetric`) are text glyphs in 1.x's
  `icons.js`. The icon renderer has no text support, so 2.0 draws them as paths that look
  close but not identical.
- **Superseded in milestone 6 (accepted by Nathan 2026-11-16): if the graphics device is lost** (a driver reset), the view goes blank until the app is
  restarted; nothing crashes and the project is kept. eframe 0.37 doesn't create a new
  device on its own. A browser tab in 1.x reloads its WebGL context instead.

## Tools and Support Fins (milestone 4b)

Nathan answered all of these on 2026-11-13: six accepted, one to measure in 4c, one to fix.

- **Accepted (2026-11-13): Restart now (About Support Fins) asks first when there are unsaved changes:** "Restart
  without saving them?". 1.x keeps an autosave copy for Recover Unsaved Work and restarts
  without asking; 2.0 has no autosave until milestone 5. Like 1.x, the new copy waits for the
  old one to close and reopens nothing.
- **Accepted (2026-11-13): on the web, the instant red overhangs are worked out in 4 ms slices per frame,** so on a
  very large part they can appear over a few frames instead of at once. The faces drawn are
  the same. On the desktop they are worked out on another thread.
- **Accepted (2026-11-13): Scale's starting size is worked out from the mesh's single-precision positions.** 1.x uses
  double precision, so on an exact tie in the third decimal the starting size can differ by
  0.001 mm.
- **Fixed (2026-11-13, measured again 2026-11-14): on the web, a large tool preview is prepared in 4 ms slices per frame,**
  the same way as the red overhangs. Measured in headless Chrome (`cargo run -p xtask -- web --measure-preview`,
  a 500,000-triangle mesh). On 2026-11-13 one shot on the main thread was 483 ms (the call itself 482.5 ms),
  and after slicing the longest stretch was 246 ms over 41 slices. Remeasured 2026-11-14. Prep only
  (creased normals and the picking tree, the mesh built inside the page, no receive and no upload):
  one shot 269.2 ms (long task 269 ms); sliced, the longest stretch was 134.2 ms over 22 slices
  (long task 134 ms). The same run then timed the app path: copying in the engine's bytes, preparing
  the preview and uploading it to the GPU. One shot was 529.9 ms (long task 538 ms, 18 chunks of 2 MB).
  Sliced, the longest frame was 134.5 ms over 37 frames (long task 134 ms, 18 chunks). That remaining
  stretch is `Bvh::build`, which stays one call (`stl-mesh` is out of scope). 134.5 ms is under the
  0.3 s bar, with about 165 ms of margin, so the tree was left whole. On the desktop the preview is
  still prepared on a worker thread.
- **Fixed (2026-11-13, 487b865): the Diameter readout of a picked cylinder face** was always
  in millimetres, also in inch mode. It now follows the unit setting like 1.x (`U.str`).
- **Accepted (2026-11-13): the outline of a picked flat face** (Lay Flat, Push/Pull) is chained from the face's
  boundary edges. 1.x's `faceOutline` also simplifies and orders the rings, so the drawn
  outline can have more points; the picked face and its params are the same.
- **Accepted (2026-11-13): look differences egui forces:** a select is egui's drop-down, not the system one (grouped
  options keep 1.x's group headings); dashed selection boxes are solid; the busy note uses
  egui's spinner; the focus glow is two strokes.
- **Accepted (2026-11-13): two UI checks wait for later milestones:** the browser drawer of `touch` (2.0's browser
  panel floats and has no drawer; milestone 5) and the sketch and Extrude timeline edit of
  `desktop_tools` (done in milestone 4c).

## Sketch mode (milestone 4c)

- **Accepted (2026-11-14): solving runs off the screen's thread when it is slow.** 1.x solves a
  sketch in the page on every change. In 2.0 the solver took 44.2 ms (p50) and 47.0 ms (p99) on a
  200-shape, 100-relation sketch (release, 100 runs), which is too long for a 16.7 ms frame. So 2.0
  solves at once while the last solve took under 4 ms, and otherwise on the engine thread (the Web
  Worker on the web), keeping only the newest answer. Small sketches behave exactly as in 1.x.
- **Fixed (2026-11-14): dragging a shape that is not fully defined could end a little differently.** 2.0 now solves each drag step from the drag-start copy, as 1.x (`SketchSession::solve_from`; test `drag_solves_from_where_it_began`).
  1.x solves each drag step from the copy taken when the drag began; 2.0 solves from the last
  solved position, so where an under-defined shape ends up can depend on the path of the pointer.
  Fully defined shapes end in the same place.
- **Accepted (2026-11-14): open-chain Offset follows 1.x desktop, not 1.x web.** 1.x web gives
  1.4999999999999998 for a point that 1.x desktop (GEOS) and 2.0 put at exactly 1.5 (golden case
  `offset_open`). The golden test allows this one case to differ by at most 1e-12.
- **Accepted (2026-11-14): Trim on an arc or circle cuts where the true curve meets the line, as 1.x
  web does.** 1.x desktop cut the 50-segment polygon of the arc, so the cut angles came out
  59.9853954° and 120.0146046° instead of 60° and 120° (about 2.5 µm off at radius 10). Construction
  shapes no longer cut a trim, also as 1.x web.
- **Accepted (2026-11-14): profile points are no longer nudged by rounding when they already sit on
  the 0.00001 mm grid.** 1.x desktop gave 13.000000000000002 where 1.x web and 2.0 give 13
  (`faces_nested`).
- **Accepted by Nathan (2026-11-16) (milestone 5): the desktop text font list starts with the bundled fonts and any added fonts; "More: fonts on this computer…" adds the system fonts.** 1.x desktop listed every system font at once and had neither entry (both were web-only). Font files are .ttf or .otf only (2.0 can't read .woff), so the bad-file message says "(use .ttf or .otf)" where 1.x says "(use .ttf, .otf or .woff)".
- **Accepted (2026-11-14): bundled symbol glyphs.** The bundled fonts had no ⌀ and some relation
  symbols; 2.0 adds them from Noto (SIL Open Font License) so labels show the same symbols as 1.x.

## Platform: settings, autosave, updates, install (milestone 5)

Nathan answered these on 2026-11-14: accepted unless marked otherwise.

- **Accepted (2026-11-14): 2.0's own folders and storage.** Settings in `%APPDATA%\STL Editor 2\settings.json`, autosave and the log in
  `%LOCALAPPDATA%\STL Editor 2`, web settings under `stle2_` keys and autosave in IndexedDB `stle2`. 1.x's folders,
  `ui-settings.json` and `stle_*` keys are never read or written. 1.x's settings and recent files are not migrated.
- **Accepted (2026-11-14): A corrupt `settings.json` is kept as `settings.json.bad`.** 1.x just started from defaults.
- **Accepted (2026-11-14): Settings are merged key by key under a lock file (`settings.lock`)**, so two 2.0 windows don't undo each other.
- **Fixed (2026-11-14): a recent file that was moved or deleted** is dropped from the list and 2.0 says "That file isn't there any more (<path>). It was taken off the Recent list." (1.x showed Python's `FileNotFoundError: [Errno 2] No such file or directory: '<path>'` and kept it listed). Recent files on the web edition last for the session only.
- **Accepted (2026-11-14): The log** (`log.txt`) records panics only; 1.x's log was a copy of the console.
- **Accepted (2026-11-14): Autosave messages 1.x didn't have** (1.x only printed failures to the console): "Couldn't save an autosave copy: …",
  "Couldn't save an autosave copy: the project can't be written.", "Couldn't keep an autosave copy in this browser: …",
  "Couldn't recover that copy: …". The Recover dialog shows the time as `autosaved YYYY-MM-DD HH:MM` (1.x used the
  browser's locale format) and the file path as a tooltip.
- **Accepted (2026-11-14): Closing a window with unsaved changes keeps its autosave copy for Recover** (as 1.x); Restart with autosave on no
  longer asks first: it writes the copy and the new window offers it.
- **Accepted (2026-11-14): The updater looks only at `v2.*` releases with the asset `STL-Editor-2-Windows.exe`**, from the full release list
  (not `/latest`), comparing versions by semver (pre-releases count). Every download redirect must stay on GitHub's
  hosts, the size must match and a listed `sha256:` digest must match; there is no fallback to a checksum line in the
  release notes. A new setting `update_checking` stops two windows checking at the same moment.
- **Accepted (2026-11-14): Update now** is offered only for the installed Windows exe (other copies get "Download"), refuses while another 2.0
  window is open ("Close the other STL Editor 2.0 windows first, then press Update now again."), shows "Downloading
  update…" with no percentage, and keeps `stl-editor-2.old.exe` until the new build draws its first frame. New messages:
  the wrong-size, not-from-GitHub, download-error and rollback messages, and "Download the new version and use it instead
  of this copy."
- **Accepted (2026-11-14): Update Settings is an on/off switch** ("Check for new versions once a day"). 1.x asked for a repository name
  (`update_repo`), which 2.0 doesn't need.
- **Fixed in milestone 6 (2026-11-14): 1.x's update check never offers a 2.x release.** `stleditor/updates.py` lists the releases
  and takes the newest `v1.*` (drafts and prereleases skipped), for the check and for one-click install; `tests/test_updates.py`.
- **Accepted (2026-11-14): The title and About say "STL Editor 2.0 (test build <date> <commit>)"**; the date is the UTC build date.
- **Accepted (2026-11-14): The phone drawer's button sits above the tool panel** (1.x puts it below), because 2.0's tool panel isn't yet a
  bottom sheet on phones.
- **Changed in milestone 6 (accepted by Nathan 2026-11-16): after a graphics driver reset the desktop window restarts itself.** eframe 0.37 can't
  give the window a new device (egui-wgpu's `Painter` makes its render state once and keeps it private), and egui-wgpu panics on the
  next paint after a loss, so 2.0 can't keep drawing in the same window. Instead STL Editor saves an autosave copy at once, says "The
  graphics driver restarted. STL Editor needs to restart to keep drawing. Your work is saved." and starts again, and the new window
  opens the saved copy (or the project's file when nothing was unsaved) without asking. 1.x's browser tab reloads its WebGL context
  instead. Differences accepted by Nathan (2026-11-16): the message is a Windows message box, because nothing can be drawn on the lost device; its
  button reads "OK", not "Restart" (rfd 0.18 on Windows uses MessageBoxW); closing it with X also restarts; if the copy couldn't be
  written the message leaves out "Your work is saved." and the new window offers Recover Unsaved Work as usual; while the engine
  finishes a running job the window is gone for up to 20 s before the message shows. The test renderer still recovers in place
  (milestone 5). Only tried on Windows.
- **Fixed (2026-11-14): system fonts and added font files draw in their own font.** The text request carries the font's bytes (1.x web `fontData`) and the engine uses them, on desktop and web. On the web, added font files are kept in IndexedDB `stle2` (store `blobs`), not `localStorage`; ones saved the old way still load.

## Finishing 2.0 (milestone 6)

- **Asked for by Nathan (2026-11-14): after Sketch or Look At, orbiting turns around the middle of the parts.** 1.x `lookAtPlane`
  sets the orbit target to the point on the plane, so orbiting swings around the face. 2.0 puts the camera exactly where 1.x does
  and only moves the target back along the same line to the parts' middle (the view itself is unchanged). On a close-up where the
  middle would be behind the camera, the point on the plane is kept.
- **Accepted by Nathan (2026-11-16): Section View can cut on a picked construction plane or flat face.** 1.x offers X, Y and Z only. A picked
  face that only touches the body starts halfway through it.
- **Accepted by Nathan (2026-11-16): Section View has a Distance box and a drag arrow instead of 1.x's Position slider.** The box takes `=`
  expressions and inches; a small box shows the distance while dragging. The hint reads "Drag the arrow or change Distance to look
  inside · only the view changes · the cut stays when you close this; X turns it off and on" (1.x: "Drag the slider to look
  inside…"). New text with no 1.x original: "Or click a flat face or a plane." and "That face isn't flat. Click a flat face or a
  plane." A Distance expression that fails shows the engine's expression error, as the tool panel does. The panel has Cancel,
  which puts the cut back as it was.
- **Accepted by Nathan (2026-11-16): Hatch (on by default) draws darker diagonal lines on the cut faces** (45°, 12 points apart in screen space (6 until Nathan asked for wider spacing, 2026-11-16), at
  0.53 × the cap colour). With Hatch off the cut face is 1.x's flat #d06060.
- **Asked by Nathan (2026-11-16): nothing behind a cut face shows through it** (edges, or slivers of faces where a pocket
  or wall meets the material above it). The M7 build drew the cut face from the body's back faces, at their depth. Where
  the GPU has a depth buffer with a stencil (`DEPTH32FLOAT_STENCIL8`), the cut face is now drawn at the cut plane: a
  stencil count of the surfaces beyond the plane marks where the cut passes through material, and the cut face hides
  everything behind it there; a hole the cut passes through still shows (`tests/m4a_fix_t5.rs`
  `cut_face_hides_what_is_behind_it_*`). Known limit: where shells overlap inside one STL the count is even, so that
  part of the cut shows no cut face. Without the stencil feature, edges whose two faces both point away from the camera
  are hidden while the cut is on, which covers simple parts.
- **Accepted by Nathan (2026-11-16): Split has a Distance field (`offset`, mm) after Split plane, with the drag arrow.** 1.x has no such field.
  It is saved only when it isn't 0, so 1.x projects and the 1.x goldens are unchanged; `tests/tools_golden.rs` leaves it out
  before comparing with the 1.x definition. A fresh plane pick sets it: 0 when the plane passes through the body, else halfway.
- **Accepted by Nathan (2026-11-14): Split's message for a plane that misses the body is always "The plane doesn't cut through this
  body. Drag the arrow or change Distance to move it into the part."**, at Distance 0 and for 1.x steps too (1.x: "The plane
  doesn't cut through this body."). The parity cases `split/split_plane_misses` and `split/split_tangent_plane` pass by exception
  (`tests/parity/exceptions.json`).
- **Accepted by Nathan (2026-11-16): fins made as a body of their own follow their part.** Support Fins / Draw Walls with
  output "separate" record the part they hold up (`State::attached`). A Split shares the fins out, each fin going with the
  half it overlaps most (the first half's stay in the supports body, renamed "<half> supports"; the other half's go in a
  new body `<split id>.2s`; fins under a half thrown away by Keep go with it), and a Move of the part moves its fins too
  (the Move handles carry them while dragging). 1.x leaves the fins where they were built. A Move of a part with supports
  goes before the supports only when nothing but support steps follow them; otherwise at the end, with a note (1.x
  `insertBeforeSupports` always puts it before, so a later Split or Thread ran on the moved part at its old place).
- **Accepted by Nathan (2026-11-17): Support Fins rebuild for split and moved parts.** A Move, Scale, Lay Flat or Split
  added at the end of the History for a part with Support Fins (or a half a later Split made of it) puts that Support
  Fins step last again, on the parts as they are now (`body`/`bodies` set to the halves still there), in the same Undo
  as the add (`Document::keep_supports_last`, called by the app's worker). Walls drawn by hand in it are taken out, with
  a note. It stays where it is (fins carried, as above) for Draw Walls, or when a later step uses its fins body. 1.x
  adds the step at the end and never moves the supports.
- **Accepted by Nathan (2026-11-16): Send to Slicer writes a 3MF.** 1.x's `r_send_to` writes an STL (the 1.x UI doesn't pass `slicer` to the
  export route).
- **Accepted by Nathan (2026-11-16): the Send to Slicer menu lists only the slicers found.** 1.x shows Bambu Studio and OrcaSlicer greyed out
  when they are missing.
- **Accepted by Nathan (2026-11-16): a failed launch says "Couldn't open <Slicer>: it wasn't found on this computer".** 1.x: "Couldn't find
  <key> on this computer."
- **Accepted by Nathan (2026-11-16): To STEP, when greyed out, has the tooltip "needs SolidCraft, which isn't installed"** (1.x has none), and a
  conversion that takes more than 10 minutes stops with a plain message (1.x showed Python's exception text).
- **Accepted by Nathan (2026-11-16): the body menu's Send to Slicer replaces the menu with the slicer list** instead of opening a submenu beside
  the row, as 1.x does; 2.0's menu has no submenus. The rows and their words are 1.x's.
- **Accepted by Nathan (2026-11-16): the tool search leaves out Send to Slicer and To STEP where the ribbon hides them** (off Windows without a
  slicer found; To STEP off Windows). 1.x lists them anyway, and running them there only shows "No slicer found…" or an error.
- **Fixed (2026-11-16): the colour picker's copy button (Colour… in the body menu) shows 📋.** egui draws U+1F4CB
  there; `symbols.ttf` now has it, from Noto Sans Symbols 2 (`tests/fonts_fallback.rs` checks it).

## Test-build fixes, navigation and sketching (milestone 7)

- **Accepted by Nathan (2026-11-15): the mouse buttons are Fusion's.** 1.x orbits with the left button and zooms along the cursor
  ray. 2.0: middle-drag pans; Shift+middle-drag and right-drag orbit; the left button never moves the view with a mouse (one finger
  on a touch screen still orbits); the wheel zooms toward the surface under the cursor, else the point on the cursor's ray at the
  target's depth. Two-finger scroll pans, with Shift it orbits. Orbit around (View menu): View Centre (default), Origin, Under the
  Cursor.
- **Accepted by Nathan (2026-11-16): the orbit and sketch settings are in the View menu** (Orbit Around, Tint Cuts in Previews, Sketch Cursor
  Coordinates). The spec says Settings › Navigation / Sketch / Display; 2.0 has no Settings window. 1.x has none of these.
- **Accepted by Nathan (2026-11-15): sketch colours and fill changed** (spec 2.3): near-white (dark theme) or near-black (light) lines
  2 px wide, a pale or dark blue for fully defined shapes, grey dashed construction lines, the profile fill at alpha 0.18 (1.x 0.38
  and orange lines).
- **Accepted by Nathan (2026-11-15): sketch sizes are typed into value boxes beside the edges** (spec 2.3), replacing 1.x's fixed
  HUD. Typing a size no longer freezes the preview: unlocked boxes keep following the pointer (1.x froze everything).
- **Accepted by Nathan (2026-11-15): grid snap defaults to "Auto"** and follows the grid spacing shown at the zoom (1.x: a fixed
  1 mm, or 1/16 in).
- **Accepted by Nathan (2026-11-16): a typed line angle becomes an angle dimension** (1.x added only the length dimension).
- **Accepted by Nathan (2026-11-15): Hole and Thread re-place their points from optional `point_refs` / `position_refs`** (part
  edges and sketch lines, spec 2.4). 1.x has no such keys. Projects without them build byte for byte as before.
- **Accepted by Nathan (2026-11-16): how a part edge is found again** (spec 2.4 leaves these open): a candidate must be at least 25% of the
  stored edge's length (else a hole's rim could match), an edge counts only where its faces meet at more than 20°, touching pieces
  of one straight line are joined, and when the found edge moved along the face normal (a thicker plate) the point moves with it.
- **Accepted by Nathan (2026-11-16): 1.x bevels Shell and Hollow corners; 2.0 makes them square** (spec 2.7 picked the method). A 30×30×20 box
  shelled 2 mm: 1.x cavity volume 10811.0 / area 2986.4 (100 triangles), Hollow body 7188.98–7206.10 depending on resolution; 2.0
  gives the exact 26×26×16 cavity (10816 / 3016) and a 7184 / 7216 body. Lattice fills the same cavity, so its walls follow: about
  −0.28% volume and +0.7% area on the two parity cases (`tests/parity/exceptions.json`). At an inside (reflex) corner the cavity is
  now a square mitre, not the voxel result's rounded arc, so the wall is a little thicker there.
- **Accepted by Nathan (2026-11-15): Section View's cut face is the body colour 25% darker with the hatch kept.** 1.x painted it
  red.
- **Accepted by Nathan (2026-11-15): the grid never shows on the cut face or through a body.** 1.x drew it on the cap; it also
  showed over a part that dips below the bed, which 2.0 no longer does (accepted by Nathan 2026-11-16 for that case).
- **Accepted by Nathan (2026-11-15): a translucent red cut tint** shows the cutter shape while a tool that removes material is open
  (1.x has none).
- **Accepted by Nathan (2026-11-15): Measure draws the X, Y and Z legs** as dotted lines in the axis colours with their lengths
  (1.x draws only the straight line).
- **Accepted by Nathan (2026-11-15): a sketch whose plane was picked on a body's face now follows later Move, Rotate and Mirror
  (Replace) steps of that body** (spec 2.9), in older projects too. 1.x left the sketch where it was. Move with Copy and Mirror Join
  or New Body leave it in place.
- **Accepted by Nathan (2026-11-16): Move snaps to the grid shown at the zoom and to the plate within 6 px; Shift turns both snaps off** (spec
  2.9 names the snaps, not the key). 1.x moved freely.
- **Accepted by Nathan (2026-11-16): the Move panel's "Drop to bed" is now "Drop to plate" and counts a pending turn**, so a turned part lands
  with its lowest point at Z = 0. 1.x used the unturned box, so a turned part could land too high or sink into the bed.
- **Accepted by Nathan (2026-11-15): a Bodies field becomes active once the picks above it are filled** (spec 2.11), even when it
  is optional ("Auto"). 1.x skipped optional Bodies fields. A tree row click fills the active Bodies field. A plain click on a
  picked body still removes it; Ctrl and Shift are not used yet (accepted by Nathan 2026-11-16).
- **Accepted by Nathan (2026-11-15): Living Hinge's hint is rewritten** (spec 2.13): two numbered steps (pick the face or plane,
  then set where it sits and how thin it is), then what the slot does and what the orange line and arrow mean. The tools
  golden keeps 1.x's hint; `tools_golden.rs` compares the rest with 1.x and checks the new hint on its own.
- **Accepted by Nathan (2026-11-15): the origin planes show while a tool waits for a plane** and go back as they were afterwards
  (spec 2.10). 1.x left them as they were.
- **Accepted by Nathan (2026-11-15): clicks in the tree and timeline register while the engine is busy** (spec 2.12). The busy
  screen no longer blocks them; its Cancel button still works. A click on a sketch row or a
  timeline step selects it; a double-click on a sketch row opens it and also selects it (accepted by Nathan 2026-11-16).
- **Asked for by Nathan (2026-11-16): sketching looks and works like SolidCraft's** (design
  `dev/specs/2026-11-16-stl-editor-2-sketch-look-design.md`). Origin planes are one square from the origin in the +
  directions, a quarter of the part's longest side (at least 10 mm, 25 mm with no part), not 1.x's centred squares
  (`viewport_cube_*` snapshots, the `pick_empty_scene` click point). No X,Y box next to the pointer and no View menu item for it
  (`shell_view_menu_*`). Value boxes are one line with the unit. Finished sketch lines are 1.5 px on a 3 px casing (were 2 on
  4), with small squares at their points; the shape being drawn is green (`sketch_inline_*`). Snaps show a marker and a
  label, and a line can snap tangent to a circle or arc. The Rectangle button's icon starts the plain rectangle; its label row
  opens the list. The sketch panel has tool options, Show and Snapping switches, and a folded "More". Feature panels put the
  main rows first and fold rarely used fields (start offset, taper, clearance, resolution, thread tolerance...) under "More
  options"; the fields and what they send are unchanged.
- **Asked for by Nathan (2026-11-16): clicked points (Hole, Thread, Measure and other picks) are small dots of a fixed size on
  screen** (8 real pixels with a thin ring), not 1.x's spheres `max(extent * 0.006, 0.3)` across, which covered the point when
  zoomed in. Sketch lines are drawn as SolidCraft draws them (Nathan, 2026-11-16): widths in real screen pixels, 2 px for lines
  and 1.2 px for construction lines, no casing (`sketch_inline_*`); sketch points are 7 px squares. The dark theme's pale lines
  are faint on light faces, so the 3:1 readability check covers the light theme only. Dark-theme sketch colours are
  SolidCraft's (Nathan, "stronger blue"): free lines blue 0x5c96ff, fully defined near-white 0xe8eaee, selected amber
  0xffc94a (the selection blue would look like a free line). While a sketch is open the bed grid is hidden and only the sketch
  plane's grid shows (Nathan: two grids at once were distracting). Closed shapes are filled blue, SolidCraft's dark profile
  fill 0x7896c8 at alpha 40, in both themes, not 1.x's peach 0xf6c7a8 (Nathan: "make it blue like solidcraft"). The sketch plane's grid is the same
  grey-blue (0x7896c8 at 0.16 / 0.32) on a cool sheet 0xe8eef8, not 1.x's rust 0xc58a68 on 0xfff1e8. The Recover screen has "Discard
  all", which asks once before deleting every copy.
- **Asked for by Nathan (2026-11-19): Thread has a Screw head choice for tapped holes**, under Size: Flush (no pocket, the
  1.x tapped hole and the default), Socket head cap (counterbore) and Countersunk. A pocket for the head is cut into the
  surface and the thread starts at its bottom, so Length is still all thread. New params, stored only with a pocket:
  `head` (`"counterbore"` / `"countersink"`), `head_diameter`, and `head_depth` (counterbore) or `head_angle` (countersink,
  whose depth follows from the diameter and angle). Picking a size fills them from the standard head sizes with the Hole
  tool's socket head allowances (head + 0.5 mm + print clearance across, head height + 0.3 mm deep); countersunk is 90°
  metric and 82° inch. Screws (`external`) have no head. 1.x has none of this; a 2.0 project with a pocket opened in 1.x
  cuts the plain tapped hole. `tools_golden` compares everything else and `thread_head_pocket_is_the_one_difference` checks
  the choice; volumes are in `stl-engine/tests/thread_head.rs`.
