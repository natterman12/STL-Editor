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

Nathan accepted four of these on 2026-11-12; they are marked "Accepted". The rest are still
pending Nathan's OK.

- **Accepted (2026-11-12): `x11-dl` is on the Linux pure-Rust allow-list.** It is winit's X11
  backend. Its build script asks `pkg-config` for the X11 library folders, and no feature drops
  that build dependency. It loads the X11 libraries at run time (`xtask/src/pure.rs`).

- **Accepted (2026-11-12). New text: "The engine stopped unexpectedly. Your last saved project is safe."** with a
  **Restart** button, shown when the engine thread or Web Worker dies. 1.x has no engine
  that can die on its own. Restart reopens the last opened or saved project.
- **The busy overlay has a Cancel button.** 1.x's busy overlay (`index.html` `#busy`) has
  none. Cancel stops the running rebuild at the next step and leaves the project exactly as
  it was before the command (undo history included); the status line then says "Cancelled."
- **Opening a `.stlx` can't be cancelled,** and its progress shows only once the file is
  loaded, because the project loader rebuilds every step inside the load. Opening a mesh file
  can be cancelled.
- **On the web, Cancel needs JS Promise Integration (JSPI)** to stop a rebuild that is already
  running (Chrome and Edge 137+, Firefox 153+, Safari 27+). On a browser without JSPI the app
  still works, but a running rebuild finishes first and Cancel takes effect between commands.
- **The rollback marker can be dragged.** 1.x shows a resize cursor on the marker but has no
  drag; you roll back from the menu or the step buttons. 2.0 also rolls back to where the
  marker is dropped.
- **Adding a step in the middle of the history takes two undos,** because it is added at the
  end and then moved. 1.x adds it in place, in one undo step.
- **Accepted (2026-11-12): Open Recent without a path until milestone 5. Recent files last only for this session,** and come back from the bytes in memory, so a
  Save after Open Recent asks where to save. Recent files that survive a restart, and the
  settings, are milestone 5.
- **The Help menu has no Check for Updates or Update Settings yet.** They need the update
  check, which is milestone 5. About Support Fins arrived in milestone 4b.
- **Accepted (2026-11-12): plain-text Help and About layout.** The version comes from the 2.0
  build, the Ko-fi link sits below the text instead of inside it, and the heart isn't
  coloured.
- **About credits list what 2.0 uses** (egui and eframe, wgpu, the manifold-rust port, the
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
- **Ten constraint icons** (`c_coincident` to `c_symmetric`) are text glyphs in 1.x's
  `icons.js`. The icon renderer has no text support, so 2.0 draws them as paths that look
  close but not identical.
- **If the graphics device is lost** (a driver reset), the view goes blank until the app is
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
- **On the web, a large tool preview is prepared on the page's own thread** (the web build has
  no threads for it). On the desktop it is prepared in the background. For a 500,000-triangle
  preview this is estimated at 0.3 to 1 s, during which the page doesn't respond. Not measured
  in a browser yet. **Nathan (2026-11-13): measure it in a real browser in 4c; if it is over
  about 0.3 s, fix it (for example in slices, like the red overhangs).**
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
  `desktop_tools` (needs sketch mode; milestone 4c).
