# STL Editor 2.0: full Rust rewrite (design)

Status: design approved in chat on 2026-11-10; this file is for review before the implementation plan.
Scope: the whole 2.0 programme. Each milestone (section 9) gets its own spec, plan and build.

## 1. Intent

What Nathan asked for:

- 2.0 is written entirely in Rust: engine, file handling, platform code and the interface.
- Goals, all four: speed; one codebase for desktop and web; a small, easy install; Rust as a long-term base (GPU, plugins, multithreading).
- Release bar: full parity with 1.x on desktop and web. Until then 1.x stays the main app and gets bug fixes only.
- Interface: the same layout and workflows as 1.x, rebuilt in a Rust GUI toolkit.
- Pure Rust: no C or C++ in our code or in our geometry, file or network dependencies.
- Approach A: build 2.0 alongside 1.x in the same repository, foundation and parity tests first.

Assumptions (stated in chat, not contradicted):

- 2.0 opens 1.x `.stlx` projects and autosaves.
- Platforms stay the same: Windows, Mac, Linux and the web edition.
- 2.0 keeps handing off to SolidCraft and slicers. They are separate programs, not part of STL Editor.

Out of scope for 2.0: new features, a redesign, plugins and GPU compute. These are later 2.x work and the architecture must not block them.

### Success criteria

1. Every 1.x feature (section 5.1) passes its parity tests against the golden files.
2. Every 1.x UI test has a 2.0 counterpart that passes.
3. Every benchmark (section 5.6) is at least as fast as 1.x on the same machine.
4. Real 1.x projects open in 2.0 and rebuild to matching results.
5. Windows ships as one `.exe` with no Python or other runtime. Mac, Linux and the web build from the same code.
6. `cargo tree` for the shipped targets contains no crate that compiles C or C++ (checked in CI, section 8.3).

## 2. Current system (1.x, for reference)

- Desktop engine: Python, about 6,500 lines in `stleditor/`. It uses manifold3d, numpy, scipy, scikit-image, trimesh, fast-simplification, shapely and fontTools, and runs behind a local JSON API (`server.py`) in a pywebview window.
- Web engine: JavaScript, about 5,700 lines in `web/engine/`, on manifold-3d WASM in a Web Worker.
- Interface: about 7,900 lines of HTML, JS and CSS in `web/js`, `web/css`, shared by both editions.
- Tests: `tests/test_engine.py` (Python engine), `tests/web_engine.test.mjs` (JS engine), Playwright UI tests in `tests/ui/`.
- Known 1.x inconsistencies, measured 2026-11-10 (`dev/ENGINE_REVIEW.md`, `dev/RUST_TRIAL.md`):
  - the lattice voxel pitch differs between the engines
  - the desktop remesh misses the requested edge length on some meshes
  - the web voxel cap is 40e6 against 300e6 on desktop
  - the expression `%` and `^` operators behave differently in the two engines

## 3. Architecture

A Cargo workspace in `rust/` in the STL-Editor repository. 1.x code is not modified by 2.0 work, except that the golden-file generator is added (section 5.2).

| Crate | Responsibility | Depends on |
|---|---|---|
| `stl-kernel` | Our own solid and 2D-shape interface: booleans, refine, hull, slice, cross-sections, transforms, mesh import and export as arrays. The only crate that uses manifold-rust. | manifold-rust |
| `stl-mesh` | Mesh tools: remesh (from `rust/remesh`), simplify (QEM, ported from `decimate.js`), voxelise and marching cubes, repair, sculpt, nearest-point search | stl-kernel |
| `stl-sketch` | 2D sketches and dimensions, text and fonts, SVG and DXF import, 2D polygon operations | stl-kernel |
| `stl-engine` | Document: feature timeline, rollback, parameters and expressions, evaluation and per-feature cache, undo/redo, `.stlx` format. One module per feature with a registry, as in the 1.x feature_modules pattern. | kernel, mesh, sketch |
| `stl-io` | STL (binary and ASCII), 3MF, OBJ read and write; PLY, OFF and GLB import (as 1.x); faceted STEP export | stl-kernel |
| `stl-platform` | Desktop only: updater, settings, autosave, slicer and SolidCraft hand-off. Compiled out of the web target. | stl-engine |
| `stl-app` | egui/eframe interface and wgpu viewport; native and wasm32 targets | all of the above |

Rules:

- Dependencies point downward only. `stl-app` is the only crate that knows about the interface.
- `stl-kernel` exposes our own types (`Solid`, `Shape2d`, `MeshArrays`). No manifold-rust type appears in its public interface, so the kernel can be replaced or forked without changes elsewhere.
- Every crate below `stl-app` builds and tests without a window, on native and on `wasm32-unknown-unknown`.

### 3.1 Data flow

1. The interface sends commands to the engine: open, import, add, edit, move or delete a feature, set a parameter, rollback, undo, redo, export.
2. The engine evaluates on a background worker: a thread on native, a Web Worker on wasm.
3. The engine sends back per-body meshes, messages and progress. Long operations can be cancelled.
4. The viewport uploads the meshes to the GPU.

There is no HTTP server and no JSON API.

### 3.2 Dependency policy (pure Rust)

- No crate in the shipped dependency tree may compile C or C++: no `cc` or `cmake` build scripts, no `-sys` crates that build native code.
- System graphics and windowing APIs (DirectX, Metal, Vulkan, WebGPU/WebGL, OS file dialogs) are called through Rust crates and are allowed. They are operating-system interfaces, not bundled C code.
- TLS for the updater uses rustls with a pure-Rust crypto provider. ring and aws-lc are not allowed because they compile C and assembly.
- Planned replacements for 1.x libraries:

  | 1.x library | 2.0 replacement |
  |---|---|
  | manifold3d | manifold-rust behind `stl-kernel` |
  | fast-simplification | QEM port of `decimate.js` |
  | scikit-image marching cubes | port of `voxel.js` |
  | scipy KD-tree | a pure-Rust KD-tree crate, or our own grid |
  | shapely | kernel cross-sections, or a pure-Rust polygon crate |
  | fontTools | a pure-Rust font parser |
  | lxml / SVG | a pure-Rust SVG parser |
  | DXF | a pure-Rust DXF crate or our own reader |
  | 3MF | zip plus XML crates |
  | trimesh, networkx | our own code, or petgraph |

  Exact crates are chosen in the milestone specs and must pass the CI check in section 8.3.

## 4. Kernel risk

manifold-rust (Apache-2.0) reports a complete port of Manifold 3.5 with 730 passing tests, validated against the C++ version, and a WebAssembly build. It is maintained by one person in spare time. Mitigations:

- It is hidden behind `stl-kernel`, as described in section 3.
- Our own kernel test corpus (the models and golden files in section 5.2, compared with the rules in section 5.3) runs in CI on every manifold-rust version we adopt.
- The version is pinned. Updates are deliberate and pass the corpus.
- We fork it into our organisation if it stops being maintained or a blocking bug is not fixed upstream.

## 5. Engine and parity

### 5.1 Port order

Each batch is done when its golden tests and benchmarks pass.

1. Core: Import, Primitive, Plane, Move, Scale, Mirror, Combine, Split, Remove, Separate, Repair.
2. Sketching: Sketch (with dimensions, text, SVG/DXF), Extrude, Revolve, Loft, Sweep, Hole, Push/Pull.
3. Mesh tools: Simplify, Remesh, Sculpt, Offset, Hollow, Shell, Lattice, Thin-wall Check.
4. Printing: Fit to Bed, Lay Flat, Plate Layout, Pattern, Thread, Resize Hole, Fit Test, Snap-fit, Living hinge.
5. Support Fins and Draw Walls.

This covers all 37 feature types in `stleditor/features.py` `EVALUATORS`.

### 5.2 Golden files

- A generator script in 1.x, `dev/parity/make_golden.py`, runs every feature with a set of parameter cases on a fixed set of input models through the Python engine.
- For each case it writes a JSON record:
  - triangle and vertex counts
  - volume and surface area
  - bounding box
  - watertight flag
  - body count and body names
  - messages and info text
- It also writes a mesh file for cases that need a geometric comparison.
- The input models are committed in `rust/tests/models/`: small generated solids, the 504k-triangle bench mesh, and real meshes with cut features.
- The reference is the Python engine, with these documented exceptions:
  - Remesh: the reference is the web engine's result, because of the desktop edge-length bug.
  - Lattice: one voxel-pitch rule, chosen by Nathan in the batch 3 spec. 1.x is changed to match before its golden files are generated.
  - Expressions: the Python rules for `%` (`math.fmod`) and for rejecting complex `^`.
  - Voxel cap: 300e6 on desktop. On the web it is limited by memory and fails with a clear message rather than a fixed lower cap.
- Any further case where 1.x is wrong is listed in `rust/tests/EXCEPTIONS.md` with the reason and Nathan's decision. Nothing is silently copied.
- Golden files are regenerated whenever a 1.x bug fix changes results.

### 5.3 Matching rules

- Volume, surface area and bounding box: within 0.1 percent relative, or 1e-4 mm absolute for values near zero.
- Watertight flag, body count and body names: exact.
- Triangle count: exact for exact operations (booleans, transforms, mirror, split); within 5 percent for approximate ones (remesh, simplify, voxel tools, sculpt, offset).
- Messages shown to the user: identical text.
- Mesh comparison where recorded: symmetric Hausdorff distance under 0.01 mm, or 1 percent of the bounding-box diagonal for approximate operations.

### 5.4 Errors

- Every feature returns `Result<Output, FeatureError>`. A `FeatureError` carries the user-facing message, and the 1.x wording is kept.
- Panics are bugs. The engine catches any panic in a feature, reports it as an internal error on that feature, and keeps the document usable.
- Invalid input (open meshes, bad parameters, too-large voxel grids) returns an error and never crashes.

### 5.5 Projects

- 2.0 reads and writes `.stlx` and the autosave layout with the same feature types, parameter names, expressions and stored sources as 1.x.
- A test opens real 1.x projects from `rust/tests/projects/` and checks that they rebuild to the golden results.
- Projects saved by 2.0 open in 1.x unless they use 2.0-only features. There are none in 2.0.

### 5.6 Performance

- Native builds use all cores for parallel operations. Wasm runs single-threaded.
- Each batch adds benchmarks on the 504k-triangle bench mesh and smaller cases, comparing against 1.x desktop and web timings.
- An operation slower than 1.x is a bug that blocks the batch.

## 6. Interface

- Toolkit: egui/eframe, with a wgpu viewport. The web uses WebGPU where available and falls back to WebGL2.
- Layout matches 1.x:
  - ribbon toolbar
  - feature timeline with rollback
  - bodies list
  - parameters panel
  - tool panels, including Support Fins
  - sketch mode with dimensions
  - move/rotate gizmo
  - status and messages area
- Keyboard shortcuts and mouse controls are unchanged. Colours and spacing follow the 1.x theme as closely as egui allows. Any detail that cannot be matched is raised with Nathan, not worked around quietly.
- Viewport module:
  - shading, edges, per-body colour and visibility
  - face and body picking for push/pull, holes and sketch planes
  - section view and the gizmo
  - nothing outside the module touches wgpu
- Platform layer, a small trait with native and web implementations:
  - file open and save: OS dialogs on native, upload and download on web
  - autosave storage: the app data folder on native, IndexedDB on web
  - the slicer, SolidCraft and updater entries are hidden on web, as in 1.x
- The engine runs in the background. Long operations show progress and can be cancelled, and the interface stays responsive.
- Tests: egui_kittest interaction and screenshot tests. Each 1.x Playwright test in `tests/ui/` gets a 2.0 counterpart, tracked in `rust/tests/UI_PARITY.md`.

## 7. Platform

- Updater: checks GitHub Releases as 1.x does, over HTTPS with rustls and a pure-Rust crypto provider. Download and replace are the same flow as 1.x.
- Settings and autosave: same behaviour as 1.x, including crash recovery and the lock file.
- Slicer and SolidCraft hand-off: same as 1.x, Windows-only where it is today.

## 8. Build, release and CI

### 8.1 Artefacts

| Platform | Artefact |
|---|---|
| Windows | One `.exe`, expected around 15 to 30 MB |
| Mac | `.app` |
| Linux | AppImage |
| Web | wasm plus static files, previewed at `/app2/` on GitHub Pages until release, then replacing `/app/` |

### 8.2 1.x during the build

- 1.x stays on its current code and gets bug fixes only, so the parity target does not move.
- The 1.x CI keeps running.

### 8.3 CI

On every push, GitHub Actions runs:

- `cargo test` for the workspace
- parity tests against the golden files
- benchmarks, as a report with a regression check
- egui_kittest UI tests
- a wasm32 build
- the pure-Rust dependency check (no crate with `cc` or `cmake` build dependencies, no C-building `-sys` crates)
- the existing 1.x tests

## 9. Milestones

Each milestone gets its own spec, implementation plan and PandaCode tasks.

1. Foundation:
   - workspace and crate skeletons
   - `stl-kernel` on manifold-rust and the kernel test corpus
   - the golden generator and the first golden files
   - parity test runner
   - document model, `.stlx` reader and writer, parameters and expressions
   - CI with the pure-Rust check
2. Engine batches 1 to 5 (section 5.1), one at a time.
3. Files: STL, 3MF, OBJ; PLY, OFF and GLB import; SVG, DXF and fonts; STEP.
4. App: interface, viewport, platform layer, desktop and web builds, UI test counterparts.
5. Platform: updater, settings, autosave, installers and release workflow.
6. Parity sign-off: every success criterion in section 1 passes, plus a beta period with real 1.x projects. Then 2.0 is released.

Team use (PandaCode NEV-Lite):

- Haiku and Cursor port features against golden tests.
- Strong Claude (Sonnet) takes the harder ports.
- Fable is reserved for the kernel interface, Support Fins and jobs Sonnet has failed.
- Work is split by file so workers run in parallel.

## 10. Risks

| Risk | Mitigation |
|---|---|
| manifold-rust maintenance or bugs | Kernel interface, own corpus, pinned version, fork if needed (section 4) |
| egui cannot match a layout detail | Raised with Nathan case by case |
| Scale and duration | Independent milestones, batch-by-batch done criteria, parity tests as the definition of done |
| A pure-Rust replacement for a 1.x library is missing or weak | Port our existing JS code, as done for remesh, or write our own. Decided per milestone spec. |
| 1.x changes during the build | Bug fixes only; golden files regenerated on result changes |
| Web memory limits with large voxel grids | Clear error message; measured in the batch 3 benchmarks |
