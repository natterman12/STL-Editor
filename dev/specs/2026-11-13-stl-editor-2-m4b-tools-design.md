# STL Editor 2.0 Milestone 4b: tools and Support Fins (design)

Status: approved in chat on 2026-11-13. This is a sub-spec of `dev/specs/2026-11-12-stl-editor-2-m4-app-design.md`, whose rules in section 2 apply here unchanged: 1.x's look, the structure of `stl-app`, the smoothness targets, the tests, and SolidCraft as read-only reference only.

## 1. Intent

What Nathan decided:

- **4b is one PandaCode run** in three parallel streams that touch different files:

  | Stream | Contents |
  |---|---|
  | A | The tool framework, all 41 tools in `TOOLS` (`web/js/tools.js`), the gizmo, the Sculpt brush |
  | B | The Support Fins panel |
  | C | An Offset speed pass in the engine |

- **Offset gets a speed pass** in the same run. Results must not change.
- **`/app2/` stays private** until Nathan says otherwise.
- **Answers to the 4a parked items** (2026-11-13):
  - A dropped `.stlx` is refused, as in 1.x. Accepted.
  - The section cut face shows the grid, as in 1.x. Accepted.
  - The WebGL2 check on Chrome's software renderer is enough for now.
  - The 320 ms fly-in to the iso view moves to 4c's look pass.
  - The empty `rust/rust/` folder and the per-task build folders under `rust/target` are deleted between runs, by Nathan's assistant.

## 2. What 4b builds

### 2.1 Tool framework (stream A)

**The tool registry:**

- `stl-app/src/tools/defs/` holds a Rust port of `TOOLS` in `web/js/tools.js`, with the same keys, labels, icons, field lists, defaults, `show` rules, `toParams`, `fromParams` and `onChange`.
- It also ports the tables beside it, value for value:
  - `HOLE_PRESETS`, `THREAD_SIZES`, `RESIZE_SIZES`
  - `PRINTER_OPTS`, `CONNECTOR_OPTS`, `CONNECTOR_DEF`
  - `OPS`, `AXES`
  - `REPEATABLE`
  - the `primitive(...)` helper's tools
- The Support Fins and Support Wall definitions that `app.js` builds from the engine's schema (`supportTools`, `app.js` lines 355–460) are ported in stream B.

**The tool panel:**

- **Layout:** `#tool-panel` with its icon, title, close button, form, message line, credit, Cancel and OK, laid out as in 1.x's CSS.
- **Rendering:** `fieldHTML`, `renderForm`, `onFormChange`, `onFormClick`, `editParams`, `params()` and `feedPick` behave as in `app.js`. That covers `show` rules, `onChange` patches, grouped selects, units, expressions in number fields (the `--expr` colour, as `expr.js` does), and the "used" and error text.

**Field kinds** (every kind `tools.js` uses):

- **Plain fields:** `number`, `select` (including grouped), `bool`, `vec3`, `text`.
- **Picked in the viewport:** `bodies`, `plane`, `sketch`, `regions`, `features`, `facepoints`, `pface`, `cylface`, `points3`, `axisline`.
- Picking uses 4a's `Viewport::pick` and its overlays (`highlight_face`, `add_marker`, `add_arrow`, `add_plane_ghost`, `add_regions` and the rest).
- `placeOnFace` tools work as in `app.js` (around line 1413): clicking a body face fills the plane field.

**Live preview:**

- `schedulePreview` (280 ms after the last change) and `runPreview` work as in `app.js`. The preview goes through a new `Command::Preview { kind, params, edit_id }` to `Document::preview`, and comes back as an `Update::Preview`. Only the newest preview may land; an older one arriving later is dropped, as `_previewSeq` does.
- Tools with `noLivePreview` show the Preview button.
- Preview meshes are drawn with `Viewport::show_preview`, with 1.x's hide list.

**Committing and editing:**

- OK commits with `AddFeature` or `EditFeature`. Cancel discards.
- Double-clicking a timeline item, or choosing Edit from its menu, reopens the tool through `fromParams`, as `openTool(key, { editFeature })` does (`app.js` line 588), including rolling back to that step while it is open.

**Opening tools:**

- Ribbon buttons, tool search (`#tool-search`) and the keyboard shortcuts that open tools.
- **Rotate** opens Move with the gizmo in rotate mode (`app.js` line 346).

**The gizmo:** `gizmo.js`, ported:

- move arrows, rotate rings in 5° steps (Shift turns freely), scale boxes
- Land on bed
- the part moves on screen at once, and letting go fills the tool's numbers so the engine preview follows
- `quatOf` and `rotOf` keep 1.x's Euler order, ZYX

**Sculpt:** the brush cursor (`setBrush`), strokes (`drawStrokes`) and mouse mode `Sculpt`, as in 1.x.

### 2.2 Support Fins panel (stream B)

A port of 1.x's Support Fins user interface:

- **Panel and modes:**
  - the panel is built from the engine's options schema (`optionsSchemaJson` through `FinsHost`), as web 1.x builds it
  - mode buttons: Rotate, Add fins, Remove fins
  - the Held card and the status HTML (`sfStatusHTML`)
  - folding sections that are remembered, and the footer credit
- **Overhangs:** instant red overhangs (`overhangFaces`) and the fins-only preview.
- **Coverage:**
  - a port of `sfcoverage.js` and `sfcoverage.worker.js`
  - finds bare spots, and handles "Full coverage" and "Bare beyond"
  - runs on a background thread on desktop and in a Web Worker on web, never on the UI thread
- **Draw Walls:** a port of `drawwalls.js`, with all six shapes (line, multi-line, curve, gusset, anchor, drop fin), its hints, colours and remembered shape.
- **Other actions:**
  - suggest orientations and strength (`sfStrength`)
  - edit the existing Support Fins feature instead of stacking a new one (`reopenSupports`)
  - paint mode (`quickPaint`)
- **The SUPPORTS ribbon tab** and its tools: Support Fins, Support Wall and the rest that `supportTools` defines.

### 2.3 Offset speed pass (stream C)

- `PERF_GAPS.md` has 1.x at 12.5–25.3 s for Offset 0.2 mm on the bench mesh, and 2.0 at 200–360 s.
- Profile it, find where 2.0 spends the time, and speed it up. Every parity case, golden check and kernel corpus case must stay unchanged.
- Target: at or below 1.0 against 1.x. If it can't get there, `PERF_GAPS.md` records the new numbers and the profile.

## 3. Checks

- **Tool golden file** (`dev/parity/js/tools_golden.mjs` → `rust/tests/parity/tools/`):
  - For every tool in `TOOLS`, Node imports 1.x's `tools.js` (it loads in Node with no DOM) with a fixed fixture context (`ctx`) built from `tests/ui/fixtures`.
  - It records `defaults(ctx)`, the `toParams` output for the defaults, and for scripted edits (each `select` option, and each `onChange` trigger such as hole presets and thread sizes).
  - 2.0's registry must give identical JSON. It runs in CI.
- **UI tests:** `egui_kittest` counterparts for the 12 `UI_PARITY.md` rows marked 4b:
  - `desktop_mirror`, `desktop_place`, `desktop_tools`
  - `gizmo`
  - `sf11`, `sf9`, `sfcov`, `sfpanel`
  - `touch`
  - `v110`
  - `web7`, `web8`
- **Support Fins definitions:** a test builds `supportfins` and `supportwall` from the bundled engine's schema and checks their fields, defaults and `fromParams` against the same tools built by 1.x (recorded by `tools_golden.mjs` with the schema read from the bundled engine).
- **Screenshots, built at the start of the run, not the end:**
  - 1.x and 2.0 side by side, dark and light, in `dev/out/m4b-shots/`
  - each tool's panel open on the fixture project, the gizmo in move and rotate, Sculpt, and the Support Fins panel in each mode, with coverage on and Draw Walls
  - the `states_differ` check from 4a applies
- **Smoothness:**
  - Opening any tool, and its first preview, keep every frame under 16.7 ms.
  - A new uibench row drags the gizmo on the bench mesh at 60 fps or more.
  - Coverage never runs on the UI thread.
- **Fonts:** every character the panels show is in the bundled fonts (4a's `fonts_fallback` test, extended to the new text).
- **Lessons from 4a, as rules:**
  - each worker builds only in its own `rust/target/<task>` folder, with paths relative to `rust/`
  - the lead runs the CI steps locally, including the WARP snapshot run and `xtask web --check --webgl2`
  - no `rust/rust/`
- **Unchanged:** parity, every golden check, pure-rust, the wasm32 builds and the 1.x tests still pass.

## 4. Done when

1. Every tool golden case matches.
2. The 12 4b UI test counterparts pass, and `UI_PARITY.md` shows them.
3. The screenshot pairs exist for every state, and every difference left is listed in the report.
4. The smoothness checks pass; the Offset row is at or below 1.0, or in `PERF_GAPS.md` with the reason.
5. Every 4a and milestone 3 check passes, and CI would pass.
6. `EXCEPTIONS.md` lists every 1.x difference, pending Nathan's OK, with Nathan's 4a answers marked accepted.

## 5. Team

The "STL 2.0 build" profile as in 4a.

| Who | Jobs |
|---|---|
| Opus 5.5 (lead) | Hands out work, checks results, runs the benchmarks and CI steps, closes out |
| Sonnet A | Tool panel, plain field widgets, presets and `onChange`, preview flow |
| Sonnet B | Pick fields, `placeOnFace`, timeline edit, ribbon, search and shortcuts |
| Worker 7 (Composer) | Porting the 41 tool definitions and their tables |
| Opus (Claude) | Gizmo, Sculpt brush, overhang overlay |
| Grok B | Support Fins panel, modes, Held card, suggest and strength, SUPPORTS tab |
| Gemini (Cursor, limited) | Coverage and Draw Walls. If Gemini runs out, Grok B takes them. |
| Worker 6 (Grok) | Offset speed pass |
| Cursor (Auto) | `tools_golden.mjs`, screenshots, the 12 UI tests, uibench row, CI |
| Composer B, Haiku, Hermes | Read-only reviews. If Hermes runs out, Haiku. |

Worker 1 is not used.
