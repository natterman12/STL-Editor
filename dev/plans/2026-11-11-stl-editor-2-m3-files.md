# STL Editor 2.0 Milestone 3 (File Export) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 2.0 writes STL, 3MF, OBJ and faceted STEP files that match 1.x. `Document` exports the right bodies under the right names. Pattern and Draw Walls come in at or below 1.0 on the bench.

**Architecture:**

- **Writers:** in a new `stl-io/src/write/` module, one file per format, behind `export_bodies`. Python's and JavaScript's number formatting each get a small helper file.
- **`Document::export`, `export_bytes` and `export_base_name`:** in `stl-engine/src/document/export.rs`.
- **Pattern:** a speed pass in the vendored manifold-rust.
- **Draw Walls:** the mesh stays in the Support Fins webview page between calls.

**Tech Stack:**

- Everything from milestone 2.
- `stl-io` keeps `zip 9` (deflate) and `quick-xml 0.38`.
- No new dependency in any crate. UUIDs come from a seeded splitmix64, not a random-number crate.

**Spec:** `dev/specs/2026-11-11-stl-editor-2-m3-files-design.md`, under the main 2.0 spec.

## Global Constraints

- **Earlier plans still apply.** Every Global Constraint of the milestone 2 plans carries over:
  - messages word for word
  - `EXCEPTIONS.md`
  - pure Rust, and the wasm32 builds of `stl-io` and `stl-engine`
  - volume and area within 0.1 percent
  - no golden input from `dev/bench/test_mesh.stl`
  - no commits or pushes
  - the tool paths
- **Run rules:** don't stop for decisions; park them and report them at the end. Every 1.x difference goes in `EXCEPTIONS.md` as "pending Nathan's OK". Read-only reviews by a second worker are for Rust code ports only.
- **References:**
  - STL and OBJ: the bytes of 1.x desktop `geometry.export_bodies` with trimesh 5.2.0, using `C:\Program Files\Python314\python.exe`
  - 3MF: the XML of 1.x desktop `geometry.export_bodies` with trimesh 5.2.0
  - STEP: `web/engine/step.js` `stepBytes` in Node
- **Number formatting, measured on trimesh 5.2.0:**
  - **OBJ** writes `"%.8f"` of the f64 value of each f32 coordinate, and keeps the sign of `-0.0` (`-0.00000000`). It writes no `vn` lines.
  - **3MF** writes Python `repr(float)` of the same f64: `0.10000000149011612`, `123456792.0`, `-0.0`, and `9.999999747378752e-06` (exponent form with at least two exponent digits when the exponent is below -4 or at least 16).
  - **STEP** writes JavaScript `String(number)` through `step.js`'s `real()`.
- **3MF UUIDs:** trimesh writes a random version 4 `p:UUID` on each `<object>`, on `<build>` and on each `<item>`. 2.0 writes v4 UUIDs from `ExportOptions::uuid_seed`. Golden comparison replaces every UUID value with `UUID` first.
- **New user-facing text, pending Nathan's OK:** `"Can't export .{ext} files."` for a path whose extension isn't one of the four formats. In 1.x, trimesh would try to write it. List this in `EXCEPTIONS.md`.

## Review Focus

1. **Body names that need escaping** (`A & "b" <c> café 'd'`):
   - 3MF writes them as ElementTree does (`&amp;`, `&lt;`, `&gt;`, `&quot;`, UTF-8 kept).
   - STEP writes `?` for each non-ASCII character and doubles the apostrophe.
   - Golden case `names_escape`, owned by Task 2, is checked in Tasks 4 and 5.
2. **A zero-area triangle:**
   - STL writes a `0 0 0` normal.
   - STEP gives `step.js`'s "has faces with no area" error, word for word.
   - Golden case `sliver`.
3. **Coordinates of `-0.0`, `1e-5` and `123456789`:** each format writes exactly the text in Global Constraints. Golden case `numbers`.
4. **A path with no extension, or an uppercase one:**
   - `out/part` with format 3MF writes `out/part.3mf`.
   - `PART.STL` writes STL.
   - Task 6 tests `export_adds_extension` and `export_ext_case`.
5. **Every body hidden:** exports every body, as 1.x does. With no bodies, the error is `"There is nothing to export."`. Golden cases `all_hidden` and `empty`.

---

### Task 0: Pattern speed pass

**Owner:** Worker 6. **Review:** Sonnet A. It runs from the start, because it touches only `rust/third_party/manifold-rust` and its `VENDORED.md`.

The background is in `rust/tests/PERF_GAPS.md` under "Pattern circular x12". About 8 s of the 7.75 s against 6.93 s goes on the union of 12 overlapping copies (`union_plain`, which follows Manifold 3.5.4 step for step).

- [ ] **Step 1:** Profile the bench Pattern row with `MANIFOLD_TIMING=1` and write down where the time goes, stage by stage.
- [ ] **Step 2:** Make it faster with byte-identical results. Allowed: thread counts, allocation and buffer reuse, data layout, and parallel passes whose output order is fixed. Not allowed: changing the order of the unions, or skipping booleans.
- [ ] **Step 3:** Check that the results are unchanged:
  - `cmp` of the Pattern result's vertices and triangles on the bench mesh, before and after
  - every kernel corpus case
  - `cargo test -p stl-engine --test parity` (same counts as before)
- [ ] **Step 4:** Run the bench with nothing else running, three times. Expected: Pattern at or below 1.0 in every run, and no other row slower. If it can't get there, update the Pattern section of `PERF_GAPS.md` with the new numbers and the profile.

### Task 1: Draw Walls sends the mesh once

**Owner:** Opus (Claude). **Review:** Worker 7. It runs from the start: it touches `rust/crates/stl-platform/src/fins/webview.rs` and `fins/mod.rs`, and `stl-engine/src/features/common/fins/` only if the cache key has to move.

**Interfaces:**
- `FinsHost::call(name, args)` is unchanged.
- Inside the webview host:
  - an argument over 1 MB is kept in the page under its key (a hash of the string), in a store of at most 6 entries with the oldest dropped first
  - later calls with the same argument send only the key
  - the page reloading, or a host restart, empties the store
- Boa and the browser host are unchanged.

- [ ] **Step 1: Write the failing tests** in `webview.rs`:
  - `large_arg_sent_once`: two calls with the same 2 MB argument move it over `sf://` once. Count the protocol requests.
  - `large_arg_after_reload`: after a forced page reload, the next call sends it again and still returns the right result.
- [ ] **Step 2:** Run `cargo test -p stl-platform fins -- --nocapture`. Expected: both FAIL.
- [ ] **Step 3:** Implement the store. Also cache the soup string and its key per body mesh (the `Arc<MeshArrays>` pointer plus the options string), so a second wall on the same body doesn't build or hash the soup again.
- [ ] **Step 4:** Run the platform tests and `cargo test -p stl-engine --test parity -- --nocapture`. Expected: all pass, and every `supportwall` and `supportfins` case is unchanged.
- [ ] **Step 5:** Run the Draw Walls bench row three times. Expected: at or below 1.0. If not, update the Draw Walls section of `PERF_GAPS.md`.

### Task 2: Export golden files, dense tines and CI

**Owner:** Cursor. **Check:** the lead.

**Files:**
- **Create:**
  - `dev/parity/make_export_golden.py` → `rust/tests/parity/export/`
  - `dev/parity/js/step_golden.mjs` → `rust/tests/parity/export/*.step`
  - `rust/tests/parity/export/cases.json`
- **Modify:**
  - `dev/parity/make_models.py`: add `sliver.stl` (closed, with one zero-area triangle) and `numbers.stl` (closed, with vertices at `-0.0`, `1e-5` and `123456789`)
  - `dev/parity/make_fins_golden.py`, `rust/tests/parity/fins_golden.json`: the dense-tines option set
  - `.github/workflows/rust.yml`: the golden job runs both new `--check` scripts

**Interfaces:**
- **`cases.json`:** `{case: {"models": [{"file": str, "name": str}], "hidden": [name...], "body_ids": [name...] | null, "path": str, "title": str | null}}`.
- **Cases:**
  - `one_body`
  - `three_bodies`
  - `names_escape`
  - `hidden_body`
  - `all_hidden`
  - `empty`
  - `sliver`
  - `numbers`
  - `open_body` (STEP error)
- **Per case, the golden folder `export/{case}/` holds:**
  - `out.stl` and `out.obj` (the 1.x bytes)
  - `3dmodel.model`, `.rels` and `[Content_Types].xml` (from 1.x's 3MF, with UUIDs replaced by `UUID`)
  - `out.step` (from `step.js` with `date: "2026-01-01T00:00:00"` and `title` = the 1.x base name)
  - `golden.json`: `{"count": n, "base": str, "error": str | null, "step_error": str | null, "readback": {"3mf": [{name, stats}], "obj": [...], "stl": [...]}}`, where `readback` comes from 1.x `load_mesh_file` and `stats.py`
- Any file over 1 MB is stored as `{"sha256": hex, "size": n}` in `golden.json` instead.
- `make_export_golden.py` builds each case in a 1.x `Document`, then calls `doc.export(path, body_ids)` and the `r_export` base-name rule.

- [ ] **Step 1:** Add the two models and the cases. Run `make_export_golden.py`, then `make_export_golden.py --check` (exit 0). Do the same for `node dev/parity/js/step_golden.mjs` and its `--check`.
- [ ] **Step 2:** Replace the dense-tines option set in `make_fins_golden.py` with one taken from `optionsSchemaJson` that changes the result on `t_bracket.stl`. Regenerate the file, then run `--check`. Expected: that case's `triangles_sha256` differs from the default case's.
- [ ] **Step 3:** Run `cargo test -p stl-engine --test parity`. Expected: 0 failed and 0 pending. Run the dense-tines case with the real webview host, and expect it to pass.

### Task 3: Writer module and STL

**Owner:** Sonnet B. **Review:** Haiku (Claude). Sonnet B does this first, because Tasks 4 and 5 add to its `mod.rs`.

**Files:**
- **Create:**
  - `rust/crates/stl-io/src/write/{mod.rs,stl.rs}`
  - stub files `threemf.rs`, `obj.rs`, `step.rs`, each returning `Err(ExportError::Unsupported)`
  - `rust/crates/stl-io/tests/export.rs`
- **Modify:** `stl-io/src/lib.rs` (`pub mod write;`)

**Interfaces (produced):**

```rust
pub enum ExportFormat { Stl, ThreeMf, Obj, Step }
impl ExportFormat {
    pub fn from_ext(ext: &str) -> Option<ExportFormat>; // "stl" | "3mf" | "obj" | "step", any case, no dot
    pub fn ext(self) -> &'static str;
}
pub struct ExportBody<'a> { pub name: String, pub mesh: &'a MeshArrays }
pub struct ExportOptions { pub uuid_seed: u64, pub step_title: String, pub step_date: String }
pub enum ExportError { Geometry(String), Unsupported, Io(String) }
pub fn export_bodies(bodies: &[ExportBody], format: ExportFormat, opts: &ExportOptions) -> Result<Vec<u8>, ExportError>;
```

- [ ] **Step 1: Write the failing test** `stl_matches_1x`: for each case in `cases.json` with bodies, `export_bodies(.., Stl, ..)` equals `export/{case}/out.stl` byte for byte (or its sha256). The test loads the bodies with the 2.0 readers, in case order.
- [ ] **Step 2:** Run `cargo test -p stl-io --test export stl`. Expected: FAIL.
- [ ] **Step 3:** Port `geometry.stl_bytes`:
  - the header `b"STL Editor binary STL"` padded with spaces to 80 bytes, then the `<u32>` count
  - each record: the normal (cross product in f32, normalized, `0` when the length is 0), 9 vertex floats, and a `0u16`
  - bodies concatenated in order
- [ ] **Step 4:** Run the test. Expected: PASS, including `sliver` and `numbers`.

### Task 4: 3MF and OBJ writers

**Owner:** Sonnet A, after Task 3's `mod.rs`. **Review:** Worker 7.

**Files:**
- **Create:** `rust/crates/stl-io/src/write/pyfmt.rs`, with `pub fn py_float_repr(x: f64) -> String` and `pub fn c_fixed8(x: f64) -> String`
- **Modify:** `write/threemf.rs` and `write/obj.rs`, replacing the stubs

**Interfaces:**
- **3MF zip:** entries in this order, all deflate:
  1. `3D/3dmodel.model`
  2. `_rels/.rels`
  3. `[Content_Types].xml`
- **3MF XML text:** exactly trimesh 5.2.0's `export_3MF` output for a `Scene` built as 1.x builds it, including:
  - the `<?xml version='1.0' encoding='utf-8'?>` line
  - the six namespace declarations
  - `type="model"` and `p:UUID` on each object
  - the namespace declarations repeated on each `<item>`
  - each item's `transform="1.0 0.0 0.0 0.0 1.0 0.0 0.0 0.0 1.0 0.0 0.0 0.0"` and `partnumber`
- **OBJ:** the header `# https://github.com/mikedh/trimesh`, then `v` lines in `c_fixed8`, then `f` lines with 1-based indices, offset per body as `trimesh.util.concatenate` does. The trailing newlines are as the golden file has them.

- [ ] **Step 1: Write the failing tests:**
  - `py_float_repr_cases`: `0.1f32 as f64` → `"0.10000000149011612"`; `123456789f32 as f64` → `"123456792.0"`; `-0.0` → `"-0.0"`; `1e-5f32 as f64` → `"9.999999747378752e-06"`; `1e16` → `"1e+16"`; `5.0` → `"5.0"`
  - `c_fixed8_cases`: `-0.0` → `"-0.00000000"`; `123456792.0` → `"123456792.00000000"`
  - `obj_matches_1x`: compares bytes with each golden `out.obj`
  - `threemf_matches_1x`: compares the three entries, with UUIDs normalized, to the goldens, and checks the zip entry order
  - `threemf_reads_back`: the 2.0 3MF reader and `stats.py`-equivalent `MeshStats` give `golden.json`'s `readback`
- [ ] **Step 2:** Run `cargo test -p stl-io --test export` and `cargo test -p stl-io write::pyfmt`. Expected: FAIL.
- [ ] **Step 3:** Implement. `py_float_repr` uses Rust's shortest round-trip digits (`{:e}`), laid out as Python's `float_repr_style = 'short'` does. UUIDs are `{8}-{4}-4{3}-{y}{3}-{12}` hex from splitmix64 seeded with `uuid_seed`, where `y` is one of `8`, `9`, `a` and `b`.
- [ ] **Step 4:** Run the tests. Expected: PASS, including `names_escape` and `numbers`. Then run `cargo build -p stl-io --target wasm32-unknown-unknown`.

### Task 5: Faceted STEP writer

**Owner:** Sonnet B, after Task 3. **Review:** Worker 6.

**Files:**
- **Create:** `rust/crates/stl-io/src/write/jsfmt.rs`, with `pub fn js_number_string(x: f64) -> String` (ECMAScript `Number::toString`)
- **Modify:** `write/step.rs`, replacing the stub

**Interfaces:**
- A port of `stepBytes(meshes, {title, date})` that keeps everything `step.js` does:
  - the entity order and numbering
  - `real()` and `str()`
  - the shared vertices and edges
  - the `FILE_NAME` line
  - both `GeometryError` messages, word for word, including `${label}`
- `opts.step_title` and `opts.step_date` take the place of `title` and `date`.
- Errors return `ExportError::Geometry(message)`.

- [ ] **Step 1: Write the failing tests:**
  - `js_number_string_cases`: `0.1f32 as f64` → `"0.10000000149011612"`; `1e21` → `"1e+21"`; `1e-7` → `"1e-7"`; `123456792.0` → `"123456792"`; `-0.0` → `"0"`
  - `step_matches_js`: compares with each golden `out.step`, using the case's title and `2026-01-01T00:00:00`
  - `step_errors_match_js`: `sliver` and `open_body` give `golden.json`'s `step_error`
- [ ] **Step 2:** Run `cargo test -p stl-io --test export step`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the tests and the wasm32 build. Expected: PASS.

### Task 6: `Document` export

**Owner:** Sonnet B, after Tasks 3–5. **Review:** Haiku (Claude).

**Files:**
- **Create:**
  - `rust/crates/stl-engine/src/document/export.rs` (a child module of `document.rs`)
  - `rust/crates/stl-engine/tests/export.rs`

**Interfaces (produced, used in milestone 4):**

```rust
impl Document {
    /// ids given → those; else bodies not hidden; else all bodies. Names from body_name.
    pub fn export_bodies_for(&self, ids: Option<&[String]>) -> Result<Vec<String>, FeatureError>; // "There is nothing to export."
    pub fn export_base_name(&self, ids: &[String], project_path: Option<&str>) -> String;
    pub fn export_bytes(&self, format: ExportFormat, ids: Option<&[String]>, opts: &ExportOptions) -> Result<(Vec<u8>, usize), FeatureError>;
    #[cfg(not(target_arch = "wasm32"))]
    pub fn export(&self, path: &Path, format: ExportFormat, ids: Option<&[String]>, project_path: Option<&str>) -> Result<(PathBuf, usize), FeatureError>;
}
pub fn iso_utc_now() -> String; // "YYYY-MM-DDTHH:MM:SS", UTC, from SystemTime, no new crate
```

- **`export_base_name`:**
  - one body: `body_name`
  - otherwise the stem of `project_path`, else `title`, else `"model"`
  - in every case, a trailing `.stlx` is removed whatever its case
- **`export`:**
  - adds `.{format.ext()}` when the path has no extension
  - otherwise picks the format from the extension (`from_ext`); an unknown extension gives `"Can't export .{ext} files."`
  - the STEP title is `export_base_name`, the date is `iso_utc_now()`, and `uuid_seed` is the nanoseconds since the epoch
  - returns the path it wrote and the body count

- [ ] **Step 1: Write the failing tests:**
  - `export_cases_match_1x`: each golden case gives the same count, base name and error as `golden.json`, and the same bytes as Tasks 3–5 check
  - `export_adds_extension`: `out/part` with `ThreeMf` writes `part.3mf`
  - `export_ext_case`: `PART.STL` writes STL
  - `export_unknown_ext`: `a.ply` gives the new message
  - `iso_utc_now_shape`: the result has 19 characters and parses back as a time
- [ ] **Step 2:** Run `cargo test -p stl-engine --test export`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the tests, the wasm32 build of `stl-engine`, and `cargo test -p stl-engine --test parity`. Expected: PASS, with parity unchanged.

### Task 7: Bench rows

**Owner:** Cursor, after Task 6. **Check:** the lead.

- [ ] **Step 1:** Add rows to `rust/xtask/src/bench.rs` and `dev/parity/bench_1x.py`: Export STL, Export 3MF and Export OBJ on the bench mesh against 1.x `export_bodies`, writing to a temp file. Add Export STEP against `stepBytes` in Node (from `dev/parity/js/`; if Node runs out of memory, use `--max-old-space-size=8192` and note it).
- [ ] **Step 2:** Run `cargo run --release -p xtask -- bench` with nothing else running, three times. Any row above 1.0 goes to that writer's owner for one speed pass. If it's still above 1.0 after that, it goes in `PERF_GAPS.md`.

### Task 8: Close-out

**Owner:** the lead.

- [ ] **Step 1:** Run:
  - `cargo test --workspace --release`
  - the pure-Rust check
  - the wasm32 builds
  - the parity runner (0 failed, 0 pending)
  - every `--check` script
  - `open_check.py`
  - the 1.x `pytest`
  - the bench, three runs
- [ ] **Step 2:** Write one export of each kind from `three_bodies` to `dev/out/m3-check/`. If installed, open the 3MF, OBJ and STL with `prusa-slicer-console.exe --info` and the STEP file with FreeCAD's command line. List what couldn't be checked, so Nathan can open it in Bambu Studio and PrusaSlicer.
- [ ] **Step 3:** Update `EXCEPTIONS.md`:
  - **accepted 2026-11-11:** the faceted STEP addition on desktop, the shorter Support Fins error text (`Support Fins: boom`), and the Resize Hole bench row on `hole_and_pin.stl`
  - **pending Nathan's OK:** `"Can't export .{ext} files."` and anything else parked

  Update `PERF_GAPS.md`.
- [ ] **Step 4:** Report the result lines, the bench table, what was opened in which program, and every parked question. Do not commit.
