# STL Editor 2.0 Milestone 2: engine batches 4 and 5 (printing tools and Support Fins) (design)

Status: approved in chat on 2026-11-11. This is a sub-spec of `dev/specs/2026-11-10-stl-editor-2-rust-design.md`. The batch 1 sub-spec's structure rules (one file per feature, `features/common/`) and batch 3's matching rules still apply. With this batch, all 37 feature types in 1.x `EVALUATORS` are ported and milestone 2 is complete.

## 1. Scope

What Nathan decided:

- **One run.** Batches 4 and 5 are planned and run together.
- **Support Fins must be fast, reliable and update itself**, on desktop and web. Support Fins is a separate MIT project (`gittrahan/support-fins`), shipped as one JavaScript engine file, so 2.0 runs that same file instead of porting it (section 2).
- **Speed rules for the run:**
  - The lead doesn't stop for decisions. It parks the item, keeps going, and lists every question at the end. Where 2.0 differs from 1.x only because 1.x has a bug, it adds an `EXCEPTIONS.md` entry marked "pending Nathan's OK".
  - Read-only reviews by a second worker are for code ports only. The lead checks golden scripts, case files, docs, CI and bench rows itself.
- **Matching:** volume and area within 0.1 percent everywhere, as in batch 3.

In scope:

**Batch 4 features:** Fit to Bed (`bedsplit`), Lay Flat, Plate Layout, Pattern, Thread, Resize Hole, Fit Test, Snap-fit and Living hinge. The Hole cases from batch 2 that carry `"requires": ["thread"]` must now pass.

**Batch 5 features:** Support Fins (`supportfins`) and Draw Walls (`supportwall`).

**The 1.x code they use:**

- `features.py`:
  - `_pattern_transforms`, `f_pattern`
  - `f_bedsplit`, `f_layflat`, `_mm`, `f_plate`
  - `f_thread`, `f_resizehole`
  - `SUPPORT_COLOR`, `_add_supports`, `f_supportfins`, `drawn_walls`, `f_supportwall`
- `feature_modules/`: `fit_test.py`, `snap_fit.py` and `living_hinge.py`.
- `printing.py`:
  - `PRINTERS` and the connector table
  - `_axis_section_area`, `plan_cuts`, `split_to_bed`
  - `rotation_to`, `best_rest_normal`, `lay_flat_matrix`
  - `_thread_section`, `_wall`, `_helical_thread`, `threaded_rod`, `thread_tool`
  - `section_shape` and `connector_points` came in batch 1.
- `meshtools.py`: `_tri_normals`, `_adjacency`, `nearest_triangle`, `fit_cylinder`, `_inside`, `_tube`, `resize_round`.
- `supportfins.py`: everything that isn't specific to V8:
  - the engine choice: `_semver`, `_digest`, `_read_info`, `_choose`
  - the calls: `info`, `features`, `version`, `_options`, `_soup`
  - the result cache: `_cached`, `_mesh_key`, `_CACHE_MAX`
  - pieces and walls: `_where`, `match_removed`, `_solids`, `fins`, `arc_path`, `wall_sections`, `walls`
  - the orient tools: `suggest`, `strength`, `wall`, `nearest_face`
  - updates: `published_url`, `check_update`, `STATE`
- The trimesh steps those use: `triangles.closest_point` (for `nearest_face`), and `Trimesh.split(only_watertight=False)` (for `_solids`), which reuses batch 1's `split` port.

Out of scope:

- The interface (milestone 4). That includes the Support Fins panel, the live preview display and the orient rings. The engine calls they need are in scope here, so milestone 4 only draws.
- The app updater (milestone 5). The secure download client it will use is built here for Support Fins, and the updater reuses it.

## 2. Support Fins in 2.0

**One engine file everywhere.** 2.0 runs Support Fins' own `engine.iife.js` through the same calls 1.x makes (`computeFinsB64`, `optionsFromDialogJson`, `reportLine` and the extras listed in its `VERSION.json`).

Measured on Nathan's PC on 2026-11-11, on the same input (`computeFinsB64`):

| Part | 1.x (V8 in mini-racer) | WebView2 | Boa (pure-Rust interpreter) |
|---|---|---|---|
| 50k triangles | 1.08–1.18 s | 0.88–0.96 s | over 230 s (stopped) |
| 504k triangles | 7.4–8.0 s | 6.3–7.3 s | not run |

The WebView2 output matched 1.x: the same number of pieces and the same output size.

**Where the engine runs:**

| Target | Engine host | Notes |
|---|---|---|
| Windows | A hidden WebView2 window through `wry`, which uses the Chromium engine (V8) that ships with Windows 10 and 11 | `wry` and `webview2-com` build no C code |
| macOS | A hidden WKWebView through `wry` (JavaScriptCore) | WKWebView must be created on the main thread. Milestone 4's app provides that hook. Until then, macOS uses the fallback. |
| Linux | A hidden WebKitGTK view through `wry` | WebKitGTK is a system C library found with `pkg-config` at build time. This is the one listed exception to the pure-Rust rule (section 4). |
| Web | The browser itself, through `wasm-bindgen` | The same engine file, loaded the way 1.x web loads it |
| Fallback | Boa, a pure-Rust JavaScript interpreter | Used only when the system web engine is missing or fails to start. It is correct but slow. |

**Structure:**

- **The calls.** `stl-engine` defines `trait FinsHost { fn call(&self, name: &str, args: &[&str]) -> Result<String, String>; fn has(&self, name: &str) -> bool; fn host_kind(&self) -> HostKind; }`. The `supportfins.py` port, the two features and the orient tools use only this trait.
- **The desktop hosts** live in `stl-platform::fins`, which is desktop-only and never in the web build:
  - `webview.rs`: runs a hidden `wry` webview on its own thread (`with_any_thread` on Windows and Linux) and serves the engine file through a custom protocol, as the measurement did.
  - `boa.rs`: the fallback.
  - `update.rs`: the update check.
  - `mod.rs`: picks the host.
- **The web host** lives in `stl-app`'s web target.
- **Errors.** An engine error becomes `FeatureError("Support Fins: {first line}")`, as in 1.x.

**Reliable:**

- If the webview host can't start, 2.0 uses Boa and keeps working. Examples: no WebView2 runtime, no WebKitGTK at run time, or a crash on start.
  - `info()` reports `"host": "webview" | "browser" | "boa"`.
  - In Boa mode it adds the notice "Support Fins is running in slow mode because this computer's built-in web engine isn't available." On Windows the notice continues: "Install the Microsoft Edge WebView2 Runtime to make it fast."
  - Milestone 4 shows this notice in the panel.
- A call that takes longer than 120 s on the webview host is cancelled with `"Support Fins: the engine stopped responding."`, and the host is restarted for the next call. 1.x has no such limit; this is listed in `EXCEPTIONS.md` as an improvement.
- The result cache matches 1.x: the last 6 fin builds and the last 64 walls, keyed by the mesh and the options.

**Updates automatically, as 1.x does:**

- When update checks are on in settings, the check runs at start-up, at most once a day:
  - It reads `VERSION.json` from `published_url()`.
  - When that names a newer release with the same `api`, it downloads `engine.iife.js` and checks its SHA-256, with line endings ignored as in `_digest`.
  - It stores the file in the app's config folder, under `support-fins`, and uses it from the next start, so a session never changes engine half-way.
- The bundled engine (`web/vendor/support-fins/`) ships inside the app. `_choose` picks the newest valid one.
- **The download client:**
  - HTTPS through `rustls` with a pure-Rust crypto provider, as the main spec requires for the updater.
  - It lives in `stl-platform::net`, so the milestone 5 updater can reuse it.
  - It runs in the background and never blocks start-up. A failed check is recorded in `STATE["error"]` and retried the next day.
- The web edition loads the published engine as 1.x web does.

## 3. Structure

| Crate | Added |
|---|---|
| `stl-kernel` | Any wrapper the ports need that isn't there yet |
| `stl-mesh` | `nearest.rs` gains `closest_point_on_triangles` (trimesh `triangles.closest_point`); `fit.rs` (`_tri_normals`, `_adjacency`, `nearest_triangle`, `fit_cylinder`); `thread.rs` (`_thread_section`, `_wall`, `_helical_thread`, `threaded_rod`, `thread_tool`) |
| `stl-engine` | `features/{bedsplit,layflat,plate,pattern,thread,resizehole,fittest,snapfit,hinge,supportfins,supportwall}.rs`. `features/common/`: `printers.rs` (`PRINTERS`, connector table), `bed.rs` (`plan_cuts`, `split_to_bed`, `_axis_section_area`), `pose.rs` (`rotation_to`, `best_rest_normal`, `lay_flat_matrix`), `round.rs` (`_inside`, `_tube`, `resize_round`), `fins/` (the `supportfins.py` port on `FinsHost`, plus `_add_supports` and `drawn_walls`) |
| `stl-platform` | `fins/{mod,webview,boa,update}.rs`, `net.rs` |
| `stl-app` | The web `FinsHost`, a thin `wasm-bindgen` bridge |

Rules:

- Nothing outside `stl-platform::fins` and the web bridge touches `wry`, `tao` or Boa.
- `stl-engine` still builds for wasm32 with no host compiled in.

## 4. Pure-Rust rule

`cargo run -p xtask -- pure-rust` gets one allow-list entry: on the Linux target only, `webkit2gtk-sys` and the GTK and WebKit `-sys` crates that `wry`'s Linux backend brings with it may use `pkg-config` or `system-deps`. The plan lists them by exact name from `cargo tree`. The reason ("system WebKitGTK for Support Fins") is printed in the check's output and listed in `EXCEPTIONS.md`.

Every other target and crate keeps the full rule. The check must still fail if anything else adds `cc`, `cmake`, `bindgen`, `pkg-config` or `vcpkg`.

## 5. Reference behaviour

- **Desktop 1.x is the reference**, pinned by `dev/parity/requirements-golden.txt` plus `mini-racer` at the installed version.
- **Support Fins golden files** use the bundled engine, v1.0.5. Tests never use a downloaded update, and the golden scripts set the update check off.
- **Engine output across hosts:**
  - Fin builds must give the same `stats` as 1.x: integers exact, floats within 1e-6 relative.
  - They must give the same number of pieces and the same `report` text.
  - The fin solids must meet the main spec's 5.3 rules. JavaScript engines may differ in the last bit of `Math` functions, hence the float tolerance.
- **Messages and info text** match 1.x word for word, including Support Fins' report lines.
- **Preview cases.** A golden case may set `"ctx": {"preview": true}`, which the parity runner and `make_golden.py` learn. It covers the preview-only paths in `_add_supports` and `f_supportfins`.

## 6. Golden additions

- **Test models.** `make_models.py` adds small models with overhangs for Support Fins (a T-bracket, a shelf with a lip and a ring with a bridge, each under 5,000 triangles), plus a block with a round hole and a pin for Resize Hole.
- **Feature cases**, at least 4 per feature, each feature with at least one 1.x error:
  - **Fit to Bed:** each connector mode (`none`, `pins`, `dowels`, `filament`, `magnets`); two printers; a body that already fits; the too-thin-for-connectors warning.
  - **Lay Flat:** `auto`, and a given normal.
  - **Plate Layout:** with and without `rotate`, and each of its three errors.
  - **Pattern:** linear; circular at 360° and below 360°; `join: false`; features as the source.
  - **Thread:** internal and external, left hand, chamfer off, a non-zero tolerance, and a placed plane.
  - **Resize Hole:** resize and fill, on a hole and on a pin.
  - **Fit Test, Snap-fit, Living hinge:** every error message.
  - **Support Fins:**
    - default options, and two option sets
    - removed fins, including a removed fin that's no longer there
    - hand walls of each shape (line, curve, anchor)
    - the no-overhangs message, a non-solid body
    - `output: "separate"`, and preview mode
  - **Draw Walls:** one wall, several walls, and a wall that can't be placed.
- **The engine golden**, `dev/parity/make_fins_golden.py` → `rust/tests/parity/fins_golden.json`. It records the raw engine results for each Support Fins test model and option set from 1.x: stats, pieces, report and a hash of the triangles. Host tests compare against it.
- **CI:**
  - The Windows jobs run every Support Fins case on the WebView2 host. GitHub's Windows runners include the WebView2 runtime.
  - The Linux job installs WebKitGTK to build. It runs one fallback (Boa) case on the T-bracket and one webview case under `xvfb`.

## 7. Done when

1. **Parity:** 0 failed and 0 pending across all 37 feature types. Every case from batches 1 to 3 still passes.
2. **Golden checks:** the engine golden matches under section 5's rules on the webview host. The Boa fallback matches it on the T-bracket.
3. **Bench** on the 504k-triangle bench mesh. Every row is at or below 1.0, or listed in `PERF_GAPS.md` with the reason and Nathan's OK. New rows:
   - Support Fins (default options), webview host against 1.x mini-racer
   - Fit to Bed on a printer smaller than the part
   - Lay Flat `auto`
   - Plate Layout of 6 copies
   - Pattern circular ×12
   - Thread M8 × 30 external
   - Resize Hole
   - one Draw Walls line wall
4. **Updates:** an update test serves a newer `VERSION.json` and engine from a local test server:
   - a newer valid engine is used from the next start
   - a wrong checksum or a different `api` is refused
   - an offline check fails quietly
5. **Builds:**
   - The pure-Rust check passes with only the section 4 entry, and still fails when a test adds `cc` anywhere else.
   - The wasm32 builds of the engine crates pass.
   - `tests/test_engine.py` passes, and CI passes.

## 8. Team

| Who | Jobs |
|---|---|
| Opus 5.5 | Leads; checks golden scripts, cases, docs, CI and bench rows itself |
| Worker 6 (Opus 5.5 via the Kilo slot) | `stl-platform::fins` (webview host, Boa fallback, host choice, update check) and `net.rs`; `thread.rs` and Thread |
| Sonnet A | `common/{printers,bed,pose}.rs`; Fit to Bed, Lay Flat, Plate Layout, Pattern |
| Sonnet B | `fit.rs`, `common/round.rs`; Resize Hole, Fit Test, Snap-fit, Living hinge; `common/fins/`, Support Fins and Draw Walls on `FinsHost` |
| Worker 7 (via the Kilo slot) | The web `FinsHost` bridge; the pure-Rust check entry; reviews |
| Cursor | Test models, all golden scripts and case files, the update test server, bench rows, CI |
| Hermes (Inkling 2 Small, OpenRouter free) | Short read-only reviews of small diffs, within its daily limit |

- Code ports get a read-only review by a worker other than their author.
- Worker 1 is used only if Nathan says so. Fable is not on the team.
