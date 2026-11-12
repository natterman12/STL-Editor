# STL Editor 2.0 Milestone 4: the app (design)

Status: approved in chat on 2026-11-12. This is a sub-spec of `dev/specs/2026-11-10-stl-editor-2-rust-design.md` (main spec sections 3.1, 6 and 9). It sets the rules for all of milestone 4 and gives batch 4a in detail. Batches 4b and 4c get their own specs.

## 1. Intent

What Nathan decided:

- **2.0's interface is 1.x's interface,** with the same layout, workflows, shortcuts, mouse controls and look, rebuilt in egui with a wgpu viewport. It runs on desktop and web from one codebase.
- **Milestone 4 is three batches,** each with its own spec, plan and PandaCode run:

  | Batch | Contents |
  |---|---|
  | 4a | The app shell and viewport (section 3) |
  | 4b | The ribbon tools: all 37 tool panels with live previews, the move/rotate gizmo, the Support Fins panel (coverage and Draw Walls) |
  | 4c | Sketch mode (sketcher, constraints, dimensions, 2D import), the remaining UI test counterparts, and a final look pass |

- **SolidCraft (`tangency-labs/solidcraft`) is a reference, not a source.** 1.x was designed to resemble it, and Nathan likes its look, design and smoothness. Workers may read it to learn how it does things, such as how it keeps egui and wgpu smooth. Nothing is copied from it:
  - none of its code
  - none of its theme values, design tokens or sizes
  - none of its icons or fonts

  SolidCraft is not a dependency and nothing credits it. Its UI kit, look and theme are under its own brand licence, which needs written permission to use (`LICENSE-BRAND` in that repository); its other code is MIT or Apache, and copying it would need credit. Writing our own avoids both.
- **Smoothness is a requirement, not a nice-to-have** (section 2.3).

## 2. Rules for all of milestone 4

### 2.1 Look

- **Colours:** 1.x's theme tokens from `web/css/*.css`, dark and light (`html[data-theme=light]`), are ported with the same values into `stl-app/src/theme.rs`. Examples: `--chrome #17191e`, `--ribbon #1d2026`, `--bg #2a2e37`, `--panel #22252c`, `--accent #4c8df6`, `--sketch #f08a4b`. Code uses tokens only, as 1.x's CSS does.
- **Layout** follows `web/index.html`:
  - the top bar (save, undo, redo, insert, export, title, search)
  - the ribbon (tabs and body)
  - the browser tree on the left
  - the viewport, with the view cube, the section button, the HUD, the hint line, the start page and the drop overlay
  - the floating tool panel
  - the timeline bar
  - the status line
  - the context menu, toasts, the busy overlay and dialogs
- **Icons:** 1.x's own icons from `web/js/icons.js` (24×24 line icons with accent fills), drawn from the same SVG.
- **Font:** 1.x uses the system UI font.
  - Desktop loads the system UI font at run time (Segoe UI on Windows), and never ships it.
  - The web build and any machine without that font use a bundled open-licence font with close metrics, chosen in 4a and shown to Nathan.
- **Mismatches:** any detail egui can't match is raised with Nathan in the batch report, not worked around quietly.

### 2.2 Structure

| Crate or module | Holds |
|---|---|
| `stl-app/src/theme.rs` | Tokens and the egui style built from them |
| `stl-app/src/ui/` | One file per region (top bar, ribbon, browser, timeline, parameters, status, toasts, busy, dialogs, start page) |
| `stl-app/src/viewport/` | Camera, renderer, picking, section view, view cube. Only this module uses wgpu. |
| `stl-app/src/engine_link/` | The engine on a background worker, commands in, results out (section 3.2) |
| `stl-app/src/platform/` | The `Platform` trait with native and web implementations (section 3.4) |

`stl-app` is still the only crate that knows about the interface. Every crate below it keeps building and testing without a window.

### 2.3 Smoothness targets

Measured on Nathan's PC by a new `cargo run --release -p xtask -- uibench`, recorded in `rust/tests/UI_BENCH.md`:

- **Frame rate:** orbit, pan and zoom on the 504k-triangle bench mesh run at the display's refresh rate, at least 60 frames a second. The 99th-percentile frame time is under 16.7 ms.
- **Idle:** no frames are drawn and CPU use is near zero while nothing changes or moves.
- **The UI thread never waits on the engine.** A rebuild on the bench mesh never stalls a frame for more than 16.7 ms. Long operations show progress and can be cancelled.
- **Uploads:** only bodies whose mesh changed are uploaded to the GPU again.
- **Start-up:** the window shows within 1 second of launch on Windows.

### 2.4 Tests

- **UI tests:** `egui_kittest`.
  - Each of the 26 1.x UI test files in `tests/ui/` gets a 2.0 counterpart that tests the same thing.
  - `rust/tests/UI_PARITY.md` maps each file to 4a, 4b or 4c and records its status.
- **Snapshot tests:** they guard 2.0 against regressions.
- **Side-by-side screenshots:** the same window in 1.x and 2.0, dark and light, go to Nathan for review in each batch's report.
- **What still applies:**
  - parity, every golden check, the pure-Rust check, the wasm32 builds and the 1.x tests still pass
  - the run rules from milestone 3: the lead doesn't stop for decisions, and second-worker reviews are for ports

## 3. Batch 4a: the shell and the viewport

### 3.1 The app

- **Desktop:** one Windows `.exe` with no runtime. Linux and Mac build from the same code; Mac packaging is milestone 5.
- **Web:** the same app built for wasm32, loaded by a small static page.
  - It is built in CI and can be served locally.
  - Publishing it to the `/app2/` preview on GitHub Pages waits for Nathan's yes at the time.
- **Crates:** eframe and egui 0.37, egui-wgpu, and wgpu.
  - `rfd` for native file dialogs, with the XDG portal on Linux, not GTK.
  - `resvg` or `tiny-skia` to draw the SVG icons.
  - `web-time`.
  - `egui_kittest` for tests.

  All must pass `xtask pure-rust`. A crate that fails it is replaced, not allowed.
- **Support Fins:** the fins host from milestone 2 (a hidden `wry` webview) must keep working inside the app. A test opens the app window and runs a Support Fins call.

### 3.2 The engine in the background

- **Commands** follow main spec section 3.1:
  - open
  - import
  - add, edit, move and delete a feature
  - set parameters
  - rollback
  - undo and redo
  - body properties
  - export
- **Results:** per-body meshes as `Arc<MeshArrays>`, plus messages, progress and the timeline state.
- **Desktop:** one worker thread owns the `Document`.
- **Web:** a Web Worker owns the `Document`. Meshes cross to the page as transferable buffers, never as JSON.
- **Cancel:** stops the running evaluation at the next feature boundary. The document is left as it was before the command.
- **One message for a lost worker:** if the engine thread or Web Worker dies, the app shows "The engine stopped unexpectedly. Your last saved project is safe." and offers Restart, which opens the last saved state. This is new text, pending Nathan's OK.

### 3.3 The viewport

Our own wgpu renderer, matching `web/js/viewer.js`:

- **Bodies:** shading as 1.x shades them, and feature edges (on or off) with 1.x's angle rule.
- **Per-body state:** colour, hidden, dimmed, selection and hover highlights.
- **Scene:** base planes, the grid, the origin and the view cube.
- **Section view:** the `btn-section` toggle, with a cap colour on the cut.
- **Camera:**
  - In model mode, left-drag rotates, middle-drag pans and right-drag pans.
  - In sketch and sculpt mode, left is free for tools, middle pans and right rotates.
  - Zoom follows the cursor and pans in screen space, with no damping.
  - Fit to view works as `fit(box)` does.
- **Picking:**
  - body, face and plane picks, as `pick(ev, {bodies, planes})`, `rayToPlane` and `worldToScreen` do
  - a CPU ray test over a bounding-volume tree built once per body mesh, fast enough to feel instant on the bench mesh (under 2 ms a pick)
- **Overlays for 4b and 4c:** the viewer's overlay calls (`highlightFace`, `addMarker`, `addArrow`, `addLine`, `addPolyline`, `showPreview`, `sketchLines` and the others) get Rust equivalents in 4a, with tests. 4b and 4c only use them.

### 3.4 Panels and files

- **Browser tree:** bodies, planes and sketches with counts; rename, colour and hide, through `set_body_props`.
- **Timeline bar:**
  - each feature
  - rollback by dragging the marker
  - start, previous, next and end
  - the context menu with edit, delete and move
- **Parameters panel:** 1.x's list, editing, expressions and errors, through `set_params`.
- **The rest of the window:** the top bar, the ribbon (1.x's tabs and buttons; tool buttons open their panels in 4b), the status line, toasts, the busy overlay with Cancel, dialogs, and the start page (Open, Box and recent files).
- **Drag and drop:** files dropped on the window open or import as in 1.x.
- **Files, through the `Platform` trait:**
  - open and save `.stlx`
  - import meshes
  - export, with milestone 3's `Document::export`, `export_bytes` and `export_base_name`
  - desktop uses the OS dialogs; the web edition uploads and downloads
  - the web build passes the page's clock for the STEP date (closing the milestone 3 exception)
  - autosave, settings and recent-file storage beyond the session belong to milestone 5
- **Keyboard and theme:**
  - undo and redo
  - 1.x's keyboard shortcuts from `App.onKey` in `web/js/app.js`, with the same keys and the same behaviour
  - the dark/light switch

### 3.5 Done when

1. A real 1.x project (`.stlx`) opens and shows the same bodies, names, colours, planes, sketches and timeline as 1.x. Save writes a file that 1.x opens.
2. The section 2.3 targets are met in `UI_BENCH.md`.
3. `UI_PARITY.md` maps all 26 1.x UI test files, and the 4a counterparts pass.
4. Snapshot tests pass, and side-by-side screenshots (1.x and 2.0, dark and light) are in the report.
5. The Windows `.exe` and the web build are built in CI. The web build loads and opens a project in headless Chrome.
6. Every milestone 3 check still passes. `EXCEPTIONS.md` lists every 1.x difference, pending Nathan's OK.

## 4. Team for 4a

The "STL 2.0 build" profile, with two workers added: Grok B (`grok-4.7[context=256k,reasoning_effort=high,fast=true]`) and Composer B (`composer-2.5[fast=true]`).

| Who | Jobs |
|---|---|
| Opus 5.5 (lead) | Hands out work, checks results, runs the benchmarks, closes out |
| Opus (Claude) | The viewport renderer: shading, edges, colours, highlights, grid, planes, origin, view cube, frame rate |
| Gemini (Cursor, limited) | Picking. If it reaches its limit, the lead hands picking to Grok B with what's done. |
| Worker 6 (Grok) | Section view; the smoothness work (draw only on change, upload only changed bodies) |
| Grok B | The web edition: the engine Web Worker, the wasm app build, upload and download; Gemini's fallback |
| Sonnet A | Top bar, ribbon, theme tokens, icons, font, start page, dialogs |
| Sonnet B | The desktop engine thread with progress and Cancel; native file dialogs; open, save, import and export; undo and redo; shortcuts; drag and drop |
| Worker 7 (Composer) | Browser tree, timeline bar, parameters panel, status line, toasts, busy overlay |
| Cursor (Auto) | `UI_PARITY.md`; the `egui_kittest` tests; `xtask uibench`; CI for the app and web builds; the side-by-side screenshots |
| Composer B, Haiku (Claude) | Read-only reviews. Composer's code is reviewed by Haiku. |
| Hermes | Read-only reviews of Cursor's `UI_PARITY.md` mapping and tests, and of the theme port against 1.x's CSS. If it reaches its daily limit, Haiku takes them. |

Worker 1 and Fable are not used.
