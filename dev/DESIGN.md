# How the bigger features work

Notes for anyone changing them (people or Claude Code). `CONTRIBUTING.md` has the code map.

## The timeline and previews

- `stleditor/document.py` / `web/engine/document.js`: an ordered list of features, each
  evaluated by `f_<type>(state, fid, params, res, ctx)` on the state the previous ones left.
  Results are cached by a hash of the params and the previous state (plus parameter values when
  the params use expressions). Rollback, undo / redo (snapshots), `.stlx` files (a zip with
  `document.json` and the imported meshes).
- `preview` runs a feature without adding it. `ctx.preview` lets a feature return preview-only
  bodies (`res.previewOnly`) that are never kept, colours (`res.colors`), see-through ghosts
  (`res.ghosts`) and pick info (`res.pieces`). Support Fins uses all of these.
- Moves are applied on release: in Move / Rotate and Scale, letting go of a handle
  (`web/js/gizmo.js`, three.js TransformControls) adds the step at once, then the tool keeps
  editing that same step (`app.autoAccept`). A rotated part lands on the bed (`bed` param).
- `app.addFeatureSmart` puts a Move / Scale / Lay Flat of a part that has supports *before* the
  supports step, so the fins are rebuilt for the new position instead of tipping with it.

## The web edition

- `web/js/api.js` exports `WEB` (true unless served from 127.0.0.1 / localhost, or with
  `?web`); in web mode requests go to the Worker (`web/engine/worker.js`), one at a time, and
  unused manifold objects are freed after each (`sweep` in `wasm.js`).
- Files: `showOpenFilePicker` / `showSaveFilePicker` where the browser has them (Save writes back
  in place), downloads otherwise. Recent files, autosave copies and added fonts live in
  IndexedDB (`web/js/webstore.js`). **File handles are kept in their own store and never read
  while listing**: Chromium's private windows crash when a FileSystemHandle is read back.
- Offline / install: `web/sw.js` precaches every file (`build_web.py` writes the list and a
  content hash); an update bar offers "Reload now". Not registered on localhost unless `?sw`.

## Parameters

- `doc.params = [{name, expr, note}]`; values can use other parameters (cycles are errors).
  Saved in `document.json`. Route `params/set`.
- Any feature param can be `"=expr"`; `runFeature` resolves them with `resolveDeep`. Errors read
  `Parameter: there's no parameter "h" (in =h)`.
- UI: Edit › Parameters…, MODEL › Reference › Parameters, a Parameters folder in the browser.
  `exprText()` decides when typed text is an expression (it names something or starts with
  `=`, but `25mm`, `1 3/8` and `2"` are numbers). Expression fields are purple.
- Sketch dimensions can be expressions too (see below); after Parameters changes,
  `app.resolveSketchParams()` re-solves those sketches and edits their features.

## Sketches and dimensions

- The solver runs in the UI (`web/js/constraints.js`): minimum-norm Gauss-Newton with a
  Levenberg-style fallback. Entities: line, arc, circle, rect (x, y, w, h), polygon, slot,
  polyline (text and projected outlines), point. Constraints refer to entities `{e}` or points
  on them `{e, k}`.
- **Smart Dimension** (`c_dimension`, key D): `inferDimension(picks, entities, cursor, fixed)`
  works out the dimension from what was picked (line → length, circle → diameter, arc →
  radius, two points / point + line → distance, two lines → angle or, when parallel, distance)
  and from where the pointer is (horizontal / vertical / aligned; which sector of an angle:
  `ra` / `rb` rays). Placing it opens the Modify box (number, or a parameter expression).
- How a dimension is drawn is `web/js/dims.js` (extension lines, arrows, radial leaders, angle
  arcs) as an SVG overlay over the canvas; placement is stored on the constraint (`off`,
  `slide`, `ang`, `out`).
- **How defined**: `analyze()` takes the rank of the constraints' Jacobian per connected group;
  the nullspace says which shapes can still move (blue) and how many degrees of freedom are left.
  Fully defined shapes are black; constraints that don't hold are red. A dimension that adds no
  rank (`isRedundant`) is offered as a reference dimension (`ref: true`, grey, in brackets,
  ignored by the solver).
- **Part edges**: route `sketch_op {op: 'refs', frame}` (`ref_geometry` / `refGeometry` in
  meshtools) returns the outline of faces lying on the sketch plane, else where the plane cuts
  the parts, else their shadow; circles are fitted. The sketcher adds them as fixed entities
  (`refEnts`: origin `_o`, edges `r<n>`, axes `_h` / `_v`) that constraints can use. The edges a
  sketch uses are saved in `params.refs`; on edit they're matched to the new edges (same edge,
  else the nearest parallel one) and missing ones are drawn red. *Use Edge* copies an edge into
  the sketch, tied to it.
- Sizes typed while drawing become driving dimensions (`addTypedDims`).

## Support Fins

[Support Fins](https://github.com/gittrahan/support-fins) (MIT, by gittrahan) adds breakaway
support walls and a bed pad to a part printed tipped over.

- `packaging/update_support_fins.py [vX.Y.Z | --check | --force]` clones the newest release,
  bundles its plugin entry (`plugins/shared/engine/bridge.js`) with esbuild into
  `engine.esm.js` (web) and `engine.iife.js` (desktop, run in mini-racer), probes the bundle on
  a test part and writes `VERSION.json` (tag, commit, SHA-256s, `features`, `extras`). `API = 1`
  is the integration contract; a release that changes the options schema version is refused.
- **Extras** (`packaging/support_fins_extras/`): things the site has that the plugin entry
  doesn't export yet: curved walls, gussets, anchor fins, Suggest orientations / Strength.
  Each is built and probed on its own; a broken one is left out and the app hides or
  approximates that tool.
- The desktop downloads newer engines from the Pages copy of `VERSION.json` (checked by
  SHA-256) and uses them from the next start (`stleditor/supportfins.py`). Hashes normalise
  CRLF.
- Features `supportfins` {body, output: join | separate, options, walls (drawn by hand),
  removed [{kind, c, size}]} and `supportwall` (Draw Walls). Removed fins are matched by their
  bounding-box middle and size, so they survive small option changes. Raw results are cached by
  a mesh digest plus the options.
- UI: panels built from the engine's options schema (`supportTools()` in app.js, field keys
  `sf:<option>`); modes Rotate (rings on the part; one "Rotate" Move step per session) / Add
  fins / Remove fins; overhangs painted red at once (`viewer.overhangFaces`), then the
  engine's exact faces.
- **Coverage check** (`web/js/sfcoverage.js`, run in `sfcoverage.worker.js` after each
  preview): an overhang point is held when the top of a fin, or a wall of the part going down
  from the overhang's edge, is within *Bare beyond* (default 5 mm, localStorage
  `stle_sf_reach`). Overhangs are split into small pieces (longest-edge bisection), each
  corner gets its distance to the nearest support, and the overlay draws pale / bright red
  with a clean edge (`viewer.addDistTris`). The panel's Held line gives the share held, the
  bare spots and the worst distance.

## Updates

- `stleditor/updates.py`: the update check reads the latest GitHub release. `install_mode()` is
  `portable` (frozen build), `git` (a git checkout) or none. Portable: download the asset,
  check its SHA-256 (the asset `digest`), unzip, then a PowerShell script waits for the app to
  close, robocopies the new files over and starts it again. Git: `git pull --ff-only` if the
  working tree is clean, then restart.

## Threads and hardware presets

- Thread sizes and hole presets are in `web/js/tools.js` (`THREAD_SIZES`, `HOLE_PRESETS`,
  `RESIZE_SIZES`): ISO metric M1.6–M30, UNC #1-64 to 3/4"-10, common UNF; heat-set inserts,
  clearance, counterbores, nut traps, self-tapping holes and magnet pockets, metric and inch,
  shown in grouped selects (`groups: true`).
