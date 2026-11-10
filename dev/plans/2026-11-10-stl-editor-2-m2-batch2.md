# STL Editor 2.0 Milestone 2, Batch 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Sketching works end to end in Rust, matching 1.x on golden files:

- the sketch geometry, text, the constraint solver and SVG/DXF import
- the seven sketch-based features
- the kernel's Combine and Split are no slower than 1.x

**Architecture:**

- New modules in `stl-sketch` for `sketch.py`, the fonts, `constraints.js` and `import2d.js`
- `refs.rs` in `stl-mesh`
- One feature file each in `stl-engine/src/features/`
- Speed work only inside `rust/third_party/manifold-rust` (vendored 0.19.2)
- Golden files from desktop 1.x for the Python parts, and from 1.x's own JS running in Node or headless Chrome for the solver and import2d

**Tech Stack:**

- Everything from batch 1
- Pure-Rust crates allowed if the `pure-rust` check passes:
  - `ttf-parser` for fonts
  - `usvg` with `default-features = false` (no text, no fonts) for SVG
- No other new dependency without the lead's approval.

**Spec:** `dev/specs/2026-11-10-stl-editor-2-m2-batch2-design.md`, under the main 2.0 spec and the batch 1 sub-spec.

## Global Constraints

- Everything in the batch 1 plan's Global Constraints still applies, including one file per feature, verbatim messages, `EXCEPTIONS.md`, `requires`, the tool paths, and no commits or pushes.
- Python golden scripts run with `C:\Program Files\Python314\python.exe` and the versions in `dev/parity/requirements-golden.txt`.
- JS golden scripts live in `dev/parity/js/`:
  - Node scripts import `web/js/constraints.js` directly
  - the import2d harness drives headless Chrome with puppeteer-core (installed globally; Chrome path in `CHROME_PATH`) on a page that loads `web/js/import2d.js`
  - every script has `--check`
- `third_party/manifold-rust` is the only place kernel speed work happens. Every change there is listed in `VENDORED.md`, with what it changes and why, and stays upstreamable: no STL Editor-specific code there.
- The kernel corpus, reader, repair and parity results must not change because of kernel speed work. Any change is a failure.

## Review Focus

1. **A sketch where two shapes touch at one point or share an edge.** It gives the same faces as 1.x shapely `polygonize`, not merged or dropped loops. Test: a Task 2 sketch golden case `touching_shapes`.
2. **Text with a character the font lacks** (for example `"Teil ö ✓"` in DejaVuSansMono). The result matches 1.x, whether that's a missing glyph or the `.notdef` box. Test: a Task 3 text case `missing_glyph`.
3. **An over-constrained sketch where the solver can't converge.** It returns the same unsatisfied list as 1.x, never NaN coordinates. Test: a Task 4 solver scene `over_constrained_conflict`.
4. **A Sweep path that crosses itself.** It gives 1.x's self-crossing message. Test: a Task 6 golden case `sweep_self_cross`.
5. **SVG with nested transforms and a percentage width.** Units resolve as in 1.x, with no panic on `width="100%"`. Test: a Task 5 import2d case `svg_percent_width`.

---

### Task 1: Golden harnesses and case files

**Owner:** Cursor. **Review:** Sonnet B.

**Files:**
- Create:
  - `dev/parity/make_sketch_golden.py` → `rust/tests/parity/sketch_golden.json`
  - `dev/parity/make_text_golden.py` → `rust/tests/parity/text_golden.json`
  - `dev/parity/make_refs_golden.py` → `rust/tests/parity/refs_golden.json`
  - `dev/parity/js/solver_golden.mjs` → `rust/tests/parity/solver_golden.json`
  - `dev/parity/js/import2d_golden.mjs` → `rust/tests/parity/import2d_golden.json`
- Create the inputs under `rust/tests/parity/{sketch,text,solver,import2d,refs}/` and new `rust/tests/parity/features/{sketch,extrude,revolve,loft,sweep,hole,pushpull}.json`, then regenerate `features_golden` with `make_golden.py`.
- Modify: `.github/workflows/rust.yml`. The golden job also runs the three new Python `--check`s, plus the two JS `--check`s with Node and Chrome on the runner.

**Interfaces:**
- Sketch golden: `{case: {"faces": [[[x, y]...]...], "ops": {...}} | {"error": str}}`, as `sketch.profile` and the op functions return them, with coordinates rounded to 1e-9.
- Text golden: `{case: {"contours": [[[x, y]...]...], "extent": [w, h]}}`.
- Solver golden: `{scene: {"solved": {entityId: {...}}, "analyze": {...}, "unsatisfied": [...], "infer": [...]}}`.
- import2d golden: `{file: {"entities": [...]} | {"error": str}}`.
- Refs golden: `{case: {"entities": [...]}}`.

- [ ] **Step 1:** Write every input set listed in spec section 4, including the five Review Focus cases.
- [ ] **Step 2:** Write the five scripts and run them, then run each with `--check` (exit 0).
- [ ] **Step 3:** Add the feature case files, then run `make_golden.py` and `make_golden.py --check`. Verify the parity line shows the new cases as pending and 0 failed.

### Task 2: `sketch.py` port (`stl-sketch`: entities, contours, profile, ops, polygon)

**Owner:** Sonnet B. **Review:** Fable.

**Files:**
- Create: `rust/crates/stl-sketch/src/{entities.rs,contours.rs,profile.rs,polygon.rs,ops/mod.rs,ops/explode.rs,ops/trim.rs,ops/offset.rs,ops/mirror.rs,ops/axis.rs,ops/snap.rs}`
- Test: `rust/crates/stl-sketch/tests/sketch.rs`

**Interfaces:**
- Produces:

```rust
pub type P2 = [f64; 2];
pub enum Entity { Line{..}, Arc{..}, Circle{..}, Rect{..}, Polygon{..}, Slot{..}, Polyline{..}, Point{..} }  // serde, 1.x JSON shape
pub fn entity_contours(e: &Entity) -> Vec<Vec<P2>>;      // sketch.entity_contours
pub fn entity_path(e: &Entity) -> Vec<P2>;               // sketch.entity_path
pub fn faces(entities: &[Entity]) -> Result<Vec<Vec<Vec<P2>>>, SketchError>;  // sketch.faces (outer + holes)
pub fn profile(entities: &[Entity], pick: Option<&serde_json::Value>) -> Result<Shape2d, SketchError>;  // sketch.profile
pub fn explode(..); pub fn trim(..); pub fn offset(..); pub fn mirror(..); pub fn find_axis(..); pub fn snap_points(..);  // same params and returns as sketch.py
```

- `polygon.rs` provides the shapely operations sketch.py uses (`polygonize`, `unary_union`, point-in-polygon, `LineString` noding), with shapely's results on the golden cases.

- [ ] **Step 1: Write the failing test** `sketch_matches_golden`. Coordinates within 1e-9, or 1e-6 for arcs; messages exact.
- [ ] **Step 2:** Run `cargo test -p stl-sketch --test sketch`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the same command, plus the wasm32 build of `stl-sketch`. Expected: PASS.

### Task 3: Text and fonts

**Owner:** Cursor. **Review:** Sonnet A.

**Files:**
- Create: `rust/crates/stl-sketch/src/text/{mod.rs,fonts.rs,glyphs.rs}`
- Test: `rust/crates/stl-sketch/tests/text.rs`

**Interfaces:**

```rust
pub fn font_dirs() -> Vec<PathBuf>; pub fn list_fonts() -> Vec<FontInfo>; pub fn default_font() -> Option<PathBuf>;  // native only
pub fn text_contours(font: &[u8], text: &str, size: f64, opts: &TextOpts) -> Result<Vec<Vec<P2>>, SketchError>;  // meshtools.text_contours
pub fn text_extent(font: &[u8], text: &str, size: f64, opts: &TextOpts) -> Result<[f64; 2], SketchError>;
```

- Glyph flattening matches the fontTools pen in `meshtools.py`: the same curve subdivision count and rule, kerning or advance handling, and line spacing.
- `font_dirs` and `list_fonts` are native-only (`cfg(not(target_arch = "wasm32"))`).

- [ ] **Step 1:** Write the failing test `text_matches_golden`, within 1e-9.
- [ ] **Step 2:** Run it. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run it. Expected: PASS. Then run the wasm32 build of `stl-sketch`.

### Task 4: Constraint solver (`constraints.js` port)

**Owner:** Sonnet A. **Review:** Fable.

**Files:**
- Create: `rust/crates/stl-sketch/src/solver/{mod.rs,constraints.rs,solve.rs,analyze.rs,infer.rs}`
- Test: `rust/crates/stl-sketch/tests/solver.rs`

**Interfaces:**

```rust
pub struct Constraint { /* serde, same JSON as 1.x constraints */ }
pub fn solve(entities: &mut Vec<Entity>, constraints: &[Constraint], locks: &[String], fixed: &[String]) -> SolveReport;
pub fn analyze(entities: &[Entity], constraints: &[Constraint], fixed: &[String]) -> Analysis;   // rank, DOF, moving set
pub fn is_redundant(..) -> bool; pub fn unsatisfied(..) -> Vec<String>; pub fn measure(c: &Constraint, e: &[Entity]) -> f64;
pub fn make_constraint(tool: &str, picks: &[Pick], e: &[Entity]) -> Option<Constraint>;
pub fn infer_dimension(picks: &[Pick], e: &[Entity], cursor: Option<P2>, fixed: &BTreeSet<String>) -> Option<Constraint>;
```

- The algorithm is the same as `constraints.js`: the minimum-norm Gauss-Newton step, the Levenberg fallback, its tolerances and iteration caps, and the rank test in `analyze`.

- [ ] **Step 1:** Write the failing test `solver_matches_golden` (solver rule from spec section 3), plus `over_constrained_conflict`.
- [ ] **Step 2:** Run it. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run it. Expected: PASS.

### Task 5: SVG/DXF import and part-edge refs

**Owner:** Cursor. **Review:** Sonnet B.

**Files:**
- Create: `rust/crates/stl-sketch/src/import2d/{mod.rs,svg.rs,dxf.rs}`, `rust/crates/stl-mesh/src/refs.rs`
- Test: `rust/crates/stl-sketch/tests/import2d.rs`, `rust/crates/stl-mesh/tests/refs.rs`

**Interfaces:**

```rust
pub fn import_svg(text: &str) -> Result<Vec<Entity>, SketchError>;   // import2d.js SVG branch
pub fn import_dxf(text: &str) -> Result<Vec<Entity>, SketchError>;   // import2d.js DXF branch, bulgePts included
// stl-mesh
pub fn ref_geometry(bodies: &[(&str, &Solid)], frame: &Frame) -> Vec<RefEntity>;   // meshtools.ref_geometry
```

- `Frame` moves from `stl-engine` `common/frames.rs` to `stl-kernel`, so that `stl-mesh` can use it; `stl-engine` re-exports it.
- SVG uses `usvg` to parse and flatten. If usvg's flattening misses the 0.01 mm rule on a case, write our own flattener for that path type rather than loosening the rule.

- [ ] **Step 1:** Write the failing tests `import2d_matches_golden` (SVG point rule, DXF within 1e-9) and `refs_match_golden`.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run them. Expected: PASS.

### Task 6: The seven features

**Owners:**
- Sonnet B: `sketch.rs`, `extrude.rs`, `revolve.rs` and `common/profiles.rs`
- Sonnet A: `loft.rs`, `sweep.rs`
- Cursor: `hole.rs`, `pushpull.rs`

Start after Tasks 2 and 5, with `profiles.rs` first. **Review:** a different worker per file.

**Files:**
- Create: `rust/crates/stl-engine/src/features/{sketch,extrude,revolve,loft,sweep,hole,pushpull}.rs`, `features/common/profiles.rs` (the spec section 1 helpers), and `pushpull_tool` in `features/common/` (port of `printing.pushpull_tool`)

- [ ] **Step 1:** Port each `f_*` and its helpers, with messages verbatim. Hole thread options needing batch 4 return `FeatureError`, and their cases carry `requires: ["thread"]`.
- [ ] **Step 2:** Verify that `cargo test -p stl-engine --test parity -- --nocapture` shows 0 failed and all new cases passing except `requires`. Include `sweep_self_cross`.

### Task 7: Kernel speed fix (vendored manifold-rust)

**Owner:** Fable. **Review:** Sonnet A. This runs alongside Tasks 1–6; it only touches `rust/third_party/manifold-rust/` and `VENDORED.md`.

**Files:**
- Modify: `rust/third_party/manifold-rust/src/**`, `rust/third_party/manifold-rust/VENDORED.md`, `rust/tests/PERF_GAPS.md`

- [ ] **Step 1:** Profile Combine join, Combine cut and Split on `dev/bench/test_mesh.stl`. Use `cargo run --release -p xtask -- bench`, plus a profiler or timing spans. Write what dominates into `VENDORED.md`.
- [ ] **Step 2:** Change the boolean pipeline so the cost follows the region the operation touches, the way the C++ original behaves, keeping results identical.
- [ ] **Step 3:** After each change, verify:
  - `cargo test --workspace --release` passes
  - the kernel corpus, readers, repair steps and parity are unchanged
  - the bench is rerun
- [ ] **Step 4:** Stop when Combine join, Combine cut and Split are at or below 1.0. Empty those rows from `PERF_GAPS.md` and record the final table in `VENDORED.md`.

### Task 8: Bench additions

**Owner:** Sonnet B. **Review:** Cursor.

**Files:**
- Modify: `rust/xtask/src/bench.rs`, `dev/parity/bench_1x.py`

- [ ] **Step 1:** Add rows for Extrude of a 200-segment closed sketch by 20 mm, Revolve of the same profile by 360°, and a Sweep of a 10 mm circle along a 40-point path.
- [ ] **Step 2:** Run the bench. Any new row above 1.0 goes back to that feature's owner.

### Task 9: Close-out

**Owner:** the lead.

- [ ] **Step 1:** Run:
  - `cargo test --workspace --release`
  - the pure-Rust check
  - the wasm32 build of the 6 crates
  - the parity runner (0 failed)
  - every `--check` script, Python and JS
  - `open_check.py`
  - the bench (every row ≤ 1.0)
  - the 1.x `pytest`
- [ ] **Step 2:** Update `EXCEPTIONS.md` and `PERF_GAPS.md`. List open questions for Nathan.
- [ ] **Step 3:** Report result lines, the bench table and questions. Do not commit.
