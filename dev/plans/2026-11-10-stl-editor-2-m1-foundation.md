# STL Editor 2.0 Milestone 1 (Foundation) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Rust workspace containing:

- a kernel that matches 1.x's Manifold results
- the expression engine
- the document model
- the `.stlx` project format
- golden files generated from 1.x, with parity runners
- a CI check that nothing pulls in C or C++

**Architecture:** A Cargo workspace in `rust/` with the seven crates from the spec, plus a test-only `stl-parity` crate and an `xtask` tool. Golden files are produced by Python scripts in `dev/parity/` that run the 1.x engine. The Rust tests read the same case files and compare their results with the matching rules in the spec.

**Tech Stack:**

- Rust 1.99.1 with the stable-x86_64-pc-windows-gnu toolchain and the wasm32-unknown-unknown target
- manifold-rust =0.19.2
- serde and serde_json
- indexmap
- sha2
- zip 9 (`default-features = false, features = ["deflate"]`, which uses zlib-rs)
- Python 3.14 with the 1.x requirements, for the golden files

**Spec:** `dev/specs/2026-11-10-stl-editor-2-rust-design.md`. This plan covers milestone 1 in section 9, using sections 3, 3.2, 4, 5.2–5.5 and 8.3.

## Global Constraints

- No package in the non-dev dependency graph of any shipped target may be `cc`, `cmake`, `bindgen`, `pkg-config` or `vcpkg` (spec 3.2).
  - `-sys` crates that are pure bindings, such as `windows-sys`, are allowed.
  - Targets checked: x86_64-pc-windows-gnu, x86_64-unknown-linux-gnu, aarch64-apple-darwin, wasm32-unknown-unknown.
- `manifold-rust = "=0.19.2"`.
  - Feature `parallel` is on for non-wasm targets only.
  - Only `stl-kernel` may depend on it, and no manifold-rust type may appear in `stl-kernel`'s public API.
- Every crate except `stl-app` and `xtask` builds with `cargo build --target wasm32-unknown-unknown`.
- 1.x code is not modified. The only new 1.x-side files go under `dev/parity/`.
- User-facing messages are identical to 1.x. Every deliberate difference is listed in `rust/tests/EXCEPTIONS.md` with its reason.
- Matching rules (spec 5.3):
  - volume, area and bounding box within 0.1% relative, or 1e-4 mm absolute
  - watertight flag, body count and names exact
  - triangle count exact for exact operations, within 5% for approximate ones
  - messages identical
- Tools on Nathan's PC:
  - `C:\Users\ggnn1\.cargo\bin\cargo.exe`
  - `C:\Program Files\Python314\python.exe`
  - run `cargo` commands from `rust/`
- Git:
  - Before Task 1, Nathan settles the pending uncommitted 1.x work (the feature_modules refactor and the dev/ reports).
  - Then create the branch `v2/foundation`.
  - The commit steps below run only if Nathan has approved committing for this run. Never push without asking.
- Panics: caught per feature on native targets. On wasm32 a panic aborts the worker; that is listed in `EXCEPTIONS.md` and handled in milestone 4.

## Review Focus

1. **`.stlx` from a different 1.x version with unknown keys** (a feature with `"suppressed": true`, extra feature fields, extra `ui` keys). It must round-trip unchanged. Test: Task 7 `extra_keys_round_trip`.
2. **Non-ASCII names** (source file `Teil ö.stl`, body name `Grün`, parameter note `größe`). They must survive open and save. Test: Task 7 `unicode_round_trip`.
3. **Bad meshes into the kernel** (an open box, a NaN coordinate, an empty mesh, a triangle index out of range). The result is `Err`, never a panic. Test: Task 3 `from_mesh_rejects_bad_input`.
4. **Expressions that produce NaN inside a function** (`max(sqrt(-1), 3)`), `0^-1`, and 500 nested parentheses. Each gives a clear error rather than a wrong number or a crash. Tests: Task 5 `nan_inside_function_is_an_error`, `zero_to_negative_power`, `deep_nesting`.
5. **A feature that panics mid-timeline.** That feature shows an error, later features still run on the state before it, and undo still works. Test: Task 6 `panicking_feature_is_contained`.

---

### Task 1: Workspace, skeleton crates and the pure-Rust check

**Files:**
- Create:
  - `rust/Cargo.toml` (workspace: `members = ["crates/*", "xtask"]`, `exclude = ["remesh"]`, `resolver = "3"`, `[workspace.package] edition = "2024"`)
  - `rust/rust-toolchain.toml` (`channel = "1.99.1"`, `targets = ["wasm32-unknown-unknown"]`)
- Create: `rust/crates/{stl-kernel,stl-parity,stl-mesh,stl-sketch,stl-engine,stl-io,stl-platform,stl-app}/Cargo.toml` and `src/lib.rs`. Each `lib.rs` is just a `//!` line saying what the crate will hold, taken from the spec 3 table.
- Create: `rust/xtask/Cargo.toml`, `rust/xtask/src/main.rs`, `rust/xtask/src/pure.rs`, `rust/xtask/tests/fixtures/{with_cc.json,windows_sys_only.json}`
- Modify: `.gitignore` (append `rust/target/`)

**Interfaces:**
- Produces: `xtask::pure::forbidden(metadata: &serde_json::Value) -> Vec<String>`
  - Returns the sorted, de-duplicated names of forbidden packages reachable from workspace members through normal or build edges.
  - Dev edges are skipped: a dep whose `dep_kinds` are all `"dev"`.
- Produces: the command `cargo run -p xtask -- pure-rust`.
  - It runs `cargo metadata --format-version 1 --filter-platform <t>` for each of the 4 targets.
  - It prints `pure-rust: ok (4 targets)`, or one line per offender with `pure-rust: <target>: <name>`, then exits 1.

- [ ] **Step 1: Write the failing tests** in `rust/xtask/src/pure.rs`:
  - `forbidden(with_cc) == ["cc"]`
  - `forbidden(windows_sys_only) == []`
  - a dev-only `cc` edge is ignored

  Make the fixtures by trimming real `cargo metadata` output down to `packages`, `workspace_members` and `resolve.nodes`.
- [ ] **Step 2: Run** `cargo test -p xtask`. Expected: FAIL (`forbidden` not defined).
- [ ] **Step 3: Implement** `forbidden` and the `pure-rust` subcommand. Use a breadth-first walk over `resolve.nodes`; the forbidden names are the five in Global Constraints.
- [ ] **Step 4: Verify:**
  - `cargo test -p xtask` passes
  - `cargo run -p xtask -- pure-rust` prints `pure-rust: ok (4 targets)`
  - `cargo build --workspace` succeeds
  - `cargo build --target wasm32-unknown-unknown -p stl-kernel -p stl-parity -p stl-mesh -p stl-sketch -p stl-engine -p stl-io` succeeds
- [ ] **Step 5: Commit** `rust/Cargo.toml rust/rust-toolchain.toml rust/crates rust/xtask .gitignore` with message "2.0: workspace, crate skeletons, pure-Rust check".

### Task 2: `stl-parity` (mesh stats and matching rules)

**Files:**
- Create: `rust/crates/stl-parity/src/lib.rs`, `src/stats.rs`, `src/compare.rs`
- Create: `dev/parity/stats.py` (the Python twin of `mesh_stats`)

**Interfaces:**
- Produces:

```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MeshStats { pub triangles: usize, pub vertices: usize, pub volume: f64, pub area: f64,
                       pub bbox_min: [f64; 3], pub bbox_max: [f64; 3], pub watertight: bool }
pub fn mesh_stats(positions: &[f32], triangles: &[u32]) -> MeshStats;
pub enum Exactness { Exact, Approx }
pub fn close(golden: f64, got: f64) -> bool;  // |d| <= 1e-4 || |d| <= 1e-3 * |golden|
pub fn compare_mesh(golden: &MeshStats, got: &MeshStats, ex: Exactness) -> Vec<String>;
```

- Produces: `dev/parity/stats.py: mesh_stats(verts, faces) -> dict` with the same keys and the same formulas.
  - Volume is the signed tetrahedron sum, accumulated in f64 from f32 positions.
  - Area is the sum of triangle areas.
  - Watertight means every undirected edge is used by exactly 2 triangles.
  - An empty mesh gives zeros and `watertight: false`.
- `compare_mesh` messages are `"<field>: golden <g> got <v>"`. `vertices` is reported but never compared.

- [ ] **Step 1: Write the failing tests** in `stats.rs` and `compare.rs`:
  - `mesh_stats` on a hand-written 10 mm cube (8 vertices, 12 triangles) gives volume 1000, area 600, bbox 0..10, watertight true
  - the same cube with one triangle removed gives watertight false
  - `compare_mesh`:
    - volume 1000 vs 1000.9 passes; 1000 vs 1001.1 fails with `"volume: golden 1000 got 1001.1"`
    - 0 vs 0.00009 passes
    - Approx triangles 100 vs 105 passes; 106 fails
    - Exact triangles 100 vs 101 fails
    - watertight true vs false fails
- [ ] **Step 2: Run** `cargo test -p stl-parity`. Expected: FAIL.
- [ ] **Step 3: Implement** the four items above, then `stats.py` with numpy.
- [ ] **Step 4: Verify:**
  - `cargo test -p stl-parity` passes
  - `python -c "import sys; sys.path.insert(0,'dev/parity'); import stats, numpy as np; print(stats.mesh_stats(<the cube arrays>))"` prints volume 1000.0, area 600.0, watertight True
- [ ] **Step 5: Commit** "2.0: parity stats and matching rules".

### Task 3: `stl-kernel` (our solid interface on manifold-rust)

**Files:**
- Create: `rust/crates/stl-kernel/src/{lib.rs,solid.rs,mesh.rs,shape2d.rs,error.rs}`
- Modify: `rust/crates/stl-kernel/Cargo.toml`:
  - `[target.'cfg(not(target_arch = "wasm32"))'.dependencies] manifold-rust = { version = "=0.19.2", features = ["parallel"] }`
  - `[target.'cfg(target_arch = "wasm32")'.dependencies] manifold-rust = "=0.19.2"`

**Interfaces:**
- Consumes: manifold-rust 0.19.2.
  - Facts checked on 2026-11-10:
    - `manifold_rust::manifold::Manifold` and `manifold_rust::linalg::Vec3::new(x, y, z)`
    - `manifold_rust::types::{MeshGL, Error, OpType, Polygons}`, where `MeshGL { num_prop: u32, vert_properties: Vec<f32>, tri_verts: Vec<u32>, ..Default::default() }`
    - `get_mesh_gl(-1)` and `from_mesh_gl(&MeshGL)`
    - `status() == Error::NoError`
    - `bounding_box()` returns `.min` and `.max` as `Vec3`
  - Method names: `cube(size, center)`, `sphere(r, segs)`, `cylinder_centered(h, r_lo, r_hi, segs, center)`, `union`, `difference`, `intersection`, `batch_boolean`, `translate`, `rotate(x, y, z)` (degrees), `scale`, `mirror`, `transform(&Mat3x4)`, `refine_to_length`, `split_by_plane`, `trim_by_plane`, `convex_hull`, `hull(&[Vec3])`, `decompose`, `compose`, `volume`, `surface_area`, `num_tri`, `num_vert`, `genus`, `is_empty`, `slice(h) -> CrossSection`, `extrude(&Polygons, h, n_div, twist, scale_top)`, `revolve(&Polygons, segs, deg)`.
- Produces:

```rust
pub struct MeshArrays { pub positions: Vec<f32>, pub triangles: Vec<u32> }   // flat xyz / ijk
pub struct Aabb { pub min: [f64; 3], pub max: [f64; 3] }
pub enum KernelError { NotClosed, Invalid(String) }
#[derive(Clone)] pub struct Solid(/* private */);
impl Solid {
  pub fn cube(size: [f64; 3], center: bool) -> Solid;
  pub fn sphere(radius: f64, segments: u32) -> Solid;
  pub fn cylinder(height: f64, r_low: f64, r_high: f64, segments: u32, center: bool) -> Solid;
  pub fn from_mesh(m: &MeshArrays) -> Result<Solid, KernelError>;   // like G.manifold_from_arrays
  pub fn to_mesh(&self) -> MeshArrays;                               // positions only
  pub fn union(&self, o: &Solid) -> Solid; pub fn difference(&self, o: &Solid) -> Solid;
  pub fn intersection(&self, o: &Solid) -> Solid; pub fn union_all(parts: &[Solid]) -> Solid;
  pub fn translate(&self, v: [f64; 3]) -> Solid; pub fn rotate_deg(&self, xyz: [f64; 3]) -> Solid;
  pub fn scale(&self, v: [f64; 3]) -> Solid; pub fn mirror(&self, normal: [f64; 3]) -> Solid;
  pub fn transform(&self, m: [[f64; 4]; 3]) -> Solid;               // row-major 3x4
  pub fn refine_to_length(&self, len: f64) -> Solid;
  pub fn split_by_plane(&self, normal: [f64; 3], offset: f64) -> (Solid, Solid);
  pub fn trim_by_plane(&self, normal: [f64; 3], offset: f64) -> Solid;
  pub fn hull(&self) -> Solid; pub fn hull_points(points: &[[f64; 3]]) -> Solid;
  pub fn decompose(&self) -> Vec<Solid>;   // sorted by volume, largest first
  pub fn compose(parts: &[Solid]) -> Solid;
  pub fn volume(&self) -> f64; pub fn surface_area(&self) -> f64; pub fn bounding_box(&self) -> Aabb;
  pub fn num_tri(&self) -> usize; pub fn num_vert(&self) -> usize; pub fn genus(&self) -> i32;
  pub fn is_empty(&self) -> bool;
  pub fn slice(&self, z: f64) -> Shape2d;
}
#[derive(Clone)] pub struct Shape2d { pub contours: Vec<Vec<[f64; 2]>> }
impl Shape2d { pub fn extrude(&self, height: f64) -> Solid; pub fn revolve(&self, segments: u32, degrees: f64) -> Solid; }
```

- `from_mesh` returns `Err(NotClosed)` when the status isn't `NoError` or the result is empty. It returns `Err(Invalid(..))`, before calling manifold-rust, for:
  - a positions length that isn't divisible by 3
  - a non-finite coordinate
  - a triangles length that isn't divisible by 3
  - an index ≥ vertex count

- [ ] **Step 1: Write the failing tests** in `rust/crates/stl-kernel/tests/solid.rs`:
  - `cube_minus_sphere_matches_cpp`: `cube([10.0;3], true).difference(&sphere(6.0, 64))` gives volume within 1e-9 relative of 204.79137941438853, area within 1e-9 of 622.7683431285991, `num_tri() == 1640`. These values came from 1.x manifold3d.
  - `round_trip_mesh`: the `to_mesh` → `from_mesh` round trip keeps `num_tri`.
  - `from_mesh_rejects_bad_input`: an open box (cube minus its top two triangles) gives `NotClosed`; a NaN coordinate, an empty mesh, and index 99 with 8 vertices each give `Invalid`. No panic.
  - `split_volumes_add_up`: the two halves of a split at offset 0 sum to the original volume.
  - `decompose_sorted`: two disjoint spheres (r 5 and r 3) give 2 parts, the larger first.
- [ ] **Step 2: Run** `cargo test -p stl-kernel`. Expected: FAIL.
- [ ] **Step 3: Implement** the interface above. `Shape2d` is converted to and from `Polygons`, and to and from `CrossSection`, inside `shape2d.rs` only.
- [ ] **Step 4: Verify:**
  - `cargo test -p stl-kernel` passes
  - `cargo build --target wasm32-unknown-unknown -p stl-kernel` succeeds
  - `cargo run -p xtask -- pure-rust` passes
- [ ] **Step 5: Commit** "2.0: stl-kernel on manifold-rust".

### Task 4: Kernel corpus (golden files from 1.x Manifold)

**Files:**
- Create:
  - `dev/parity/make_models.py`, which writes `rust/tests/models/{holed_block.stl,open_box.stl,two_blobs.stl}` with manifold3d:
    - holed_block: a 30×20×10 box minus two r 3 through-holes
    - open_box: a 20 mm cube with the top 2 triangles removed
    - two_blobs: two disjoint spheres
  - `rust/tests/parity/kernel_cases.json`
  - `dev/parity/make_kernel_golden.py`, which writes `rust/tests/parity/kernel_golden.json`
  - `rust/crates/stl-kernel/tests/corpus.rs` (dev-dependency `stl-parity`)

**Interfaces:**
- Consumes: Task 2 `MeshStats`, `mesh_stats`, `compare_mesh`; Task 3 `Solid`.
- Case format, a stack machine read by both sides: `{"name": str, "exactness": "exact"|"approx" (default "exact"), "ops": [op...]}`. Ops:
  - Push: `cube{size,center}`, `sphere{r,segments}`, `cylinder{h,r1,r2,segments,center}`, `load{model}`
    - `load` welds vertices with identical f32 bits, in first-seen order.
  - Pop two and push one: `union`, `difference`, `intersection`
    - It pops b, then a, and pushes a∘b.
  - Unary, on the top of the stack: `translate{v}`, `rotate{deg}`, `scale{v}`, `mirror{normal}`, `refine_to_length{len}`, `hull`, `trim_by_plane{normal,offset}`, `slice_extrude{z,height}`
  - Replace the top with several solids:
    - `split_by_plane{normal,offset}` pushes the first, then the second
    - `decompose` pushes the parts, largest volume first
- Golden format: `{"<case name>": [MeshStats for each stack entry, bottom to top]}`, using `stats.mesh_stats` on each solid's `to_mesh()` arrays.
- `make_kernel_golden.py --check` regenerates in memory and exits 1 if the result differs from the committed file.

- [ ] **Step 1: Write** `make_models.py` and run it; the three STL files exist. Write `kernel_cases.json` with at least 25 cases:
  - every op at least twice
  - `load` of each model
  - a `load open_box` case: Python records `[]` when `manifold_from_arrays` returns None, and Rust must give `Err(NotClosed)` and an empty stack
- [ ] **Step 2: Write** `make_kernel_golden.py` and run it. Then run it with `--check`; it exits 0.
- [ ] **Step 3: Write the failing test** `corpus_matches_golden` in `corpus.rs`. For every case it runs the ops and checks:
  - the stack length matches the golden list
  - `compare_mesh(golden[i], mesh_stats(top_i), exactness)` returns nothing

  On failure the message is `"<case>: entry <i>: <mismatch>"`.
- [ ] **Step 4: Run** `cargo test -p stl-kernel --test corpus`. Expected: PASS. A failure is a kernel bug or a manifold-rust divergence: record it in `EXCEPTIONS.md` only with Nathan's agreement.
- [ ] **Step 5: Commit** "2.0: kernel corpus against 1.x Manifold".

### Task 5: Expressions (`stl-engine::params`)

**Files:**
- Create: `rust/crates/stl-engine/src/params.rs`
- Create: `rust/tests/parity/expr_cases.json`, `dev/parity/make_expr_golden.py` → `rust/tests/parity/expr_golden.json`
- Create: `rust/crates/stl-engine/tests/expr.rs`
- Create: `rust/tests/EXCEPTIONS.md` (first entries, below)

**Interfaces:**
- Produces:

```rust
pub struct ExprError(pub String);                       // Display = the message
pub struct Param { pub name: String, pub expr: String, pub note: String }  // serde, 1.x keys
pub struct Resolved { pub values: BTreeMap<String, f64>, pub errors: BTreeMap<String, String> }
pub fn is_expr(v: &serde_json::Value) -> bool;
pub fn evaluate(text: &str, vars: &BTreeMap<String, f64>) -> Result<f64, ExprError>;
pub fn names_in(text: &str) -> BTreeSet<String>;
pub fn resolve_params(list: &[Param]) -> Resolved;
pub fn resolve_deep(v: &serde_json::Value, values: &BTreeMap<String, f64>) -> Result<serde_json::Value, ExprError>;
pub fn uses_expr(v: &serde_json::Value) -> bool;
pub fn is_valid_name(s: &str) -> bool;  pub const RESERVED: [&str; 14];
```

- Behaviour is a port of `stleditor/params.py`, keeping the same:
  - grammar and precedence; `^` is right-associative, and unary minus binds tighter than `^` (so `-2^2` is 4), as in `params.py`
  - token regexes
  - function table: `round` is `floor(x+0.5)`; trig functions take degrees, inverse trig functions return degrees
  - arity errors
  - message text: every f-string in `params.py`, with `src` being the trimmed text with leading `=` removed
- Additions (rules where Python crashes or diverges), each listed in `EXCEPTIONS.md`:
  - The result of every function call, `^` and `%` is checked with `is_finite()`. A non-finite result gives `"{src} doesn't give a number"`. This matches Python's ValueError path, and makes `max(sqrt(-1), 3)` an error.
  - `0^-1` gives `"{src} doesn't give a number"`. In Python this raises ZeroDivisionError, which escapes.
  - Nesting deeper than 200 levels gives `"{src} is nested too deeply"`. Python hits RecursionError.
  - Panics in features on wasm32 abort the worker. This is the native/wasm note from Global Constraints.

- [ ] **Step 1: Write** `expr_cases.json` with at least 60 cases:
  - every operator, function and constant
  - precedence (`2^3^2`, `-2^2`, `2*-3`)
  - every error message in `params.py`
  - `resolve_params` lists with cycles, duplicates, reserved names, unknown names and a bad name
  - the three addition cases

  Write `make_expr_golden.py`. It records `{"value": x}` or `{"error": msg}`, or `{"python_exception": "ZeroDivisionError"}` for crashes, and supports `--check`. Run it.
- [ ] **Step 2: Write the failing tests** in `expr.rs`:
  - `matches_golden`: values within 1e-12 relative and messages identical. A `python_exception` case must equal the `EXCEPTIONS.md` replacement message.
  - `nan_inside_function_is_an_error`
  - `zero_to_negative_power`
  - `deep_nesting`: 500 nested parentheses give the nesting message, with no stack overflow
- [ ] **Step 3: Run** `cargo test -p stl-engine --test expr`. Expected: FAIL.
- [ ] **Step 4: Implement** `params.rs`.
- [ ] **Step 5: Run** the same command. Expected: PASS.
- [ ] **Step 6: Commit** "2.0: expressions with 1.x golden tests".

### Task 6: Document model (`stl-engine`)

**Files:**
- Create: `rust/crates/stl-engine/src/{state.rs,feature.rs,registry.rs,document.rs,error.rs,labels.rs}`
- Test: `rust/crates/stl-engine/tests/document.rs`

**Interfaces:**
- Consumes: Task 3 `Solid` and `MeshArrays`; Task 5 `Param`, `resolve_params`, `resolve_deep` and `uses_expr`.
- Produces:

```rust
pub struct FeatureError(pub String);
pub struct Body { pub name: String, pub solid: Option<Solid>, pub mesh: Option<MeshArrays> }
impl Body { pub fn require_solid(&self) -> Result<&Solid, FeatureError>; }
  // '"{name}" is not a watertight solid. Run Repair on it first.'
#[derive(Clone, Default)] pub struct State { pub bodies: IndexMap<String, Body> }
impl State { pub fn body(&self, bid: &str) -> Result<&Body, FeatureError>; } // "Body '{bid}' no longer exists."
#[derive(Serialize, Deserialize, Clone)] pub struct Feature {
  pub id: String, #[serde(rename = "type")] pub kind: String,
  #[serde(default, skip_serializing_if = "Option::is_none")] pub name: Option<String>,
  #[serde(default)] pub params: serde_json::Map<String, Value>,
  #[serde(default, skip_serializing_if = "std::ops::Not::not")] pub suppressed: bool,
  #[serde(flatten)] pub extra: serde_json::Map<String, Value> }
pub struct Source { pub name: String, pub ext: String, pub data: Vec<u8> }
#[derive(Default)] pub struct FeatureResult { pub id: String, pub error: Option<String>, pub warning: Option<String>,
  pub created: Vec<String>, pub modified: Vec<String>, pub ms: f64, pub suppressed: bool, pub preview_only: bool }
pub struct Ctx<'a> { pub sources: &'a BTreeMap<String, Source>, pub preview: bool }
impl Ctx<'_> { pub fn source(&self, sid: &str) -> Result<&Source, FeatureError>; } // "The imported file is missing from the project."
pub type Evaluator = fn(&mut State, &str, &serde_json::Map<String, Value>, &mut FeatureResult, &Ctx) -> Result<(), FeatureError>;
#[derive(Clone, Default)] pub struct Registry { /* type -> Evaluator */ }
impl Registry { pub fn new() -> Registry; pub fn builtin() -> Registry; pub fn with(self, kind: &'static str, f: Evaluator) -> Registry; pub fn has(&self, kind: &str) -> bool; }
pub const LABELS: [(&str, &str); 37];   // copied from stleditor/features.py LABELS, same order
pub struct Document { pub features: Vec<Feature>, pub rollback: Option<usize>, pub ui: serde_json::Map<String, Value>,
  pub params: Vec<Param>, pub sources: BTreeMap<String, Source>, pub state: State, /* cache, undo, registry: private */ }
impl Document {
  pub fn new(registry: Registry) -> Document;            // ui = {"names":{}, "colors":{}, "hidden":[]}
  pub fn new_id(&mut self, prefix: &str) -> String;     pub fn add_source(&mut self, name: &str, data: Vec<u8>) -> String;
  pub fn evaluate(&mut self) -> Vec<FeatureResult>;     pub fn results(&self) -> &[FeatureResult];
  pub fn add_feature(&mut self, kind: &str, params: Map<String, Value>, name: Option<String>) -> Result<String, FeatureError>;
  pub fn preview(&mut self, kind: &str, params: Map<String, Value>, edit_id: Option<&str>) -> (State, FeatureResult);
  pub fn edit_feature(&mut self, fid: &str, params: Option<Map<String, Value>>, name: Option<String>, suppressed: Option<bool>, rollback_after: RollbackAfter) -> Result<(), FeatureError>;
  pub fn delete_feature(&mut self, fid: &str) -> Result<(), FeatureError>;
  pub fn move_feature(&mut self, fid: &str, new_index: usize) -> Result<(), FeatureError>;
  pub fn set_rollback(&mut self, index: Option<usize>);
  pub fn set_params(&mut self, list: Vec<Param>) -> Result<(), FeatureError>;
  pub fn param_values(&mut self) -> &Resolved;
  pub fn undo(&mut self) -> bool; pub fn redo(&mut self) -> bool;
  pub fn body_name(&self, bid: &str) -> String;
  pub fn set_body_props(&mut self, bid: &str, name: Option<String>, color: Option<String>, hidden: Option<bool>);
}
pub enum RollbackAfter { Keep, Set(Option<usize>) }
```

- Behaviour is a port of `stleditor/document.py`, lines 23–330:
  - `active_features`, `evaluate` with the per-step hash cache (sha2 over `prev + "|" + serde_json` of the feature, with `"$params"` added when it uses expressions) and the memo of 6
  - `run_feature`: suppressed handling, `"Parameter: {e}."`, results
  - `add_feature`: the trial run that refuses a failing feature, `"Unknown feature {kind}."`, hiding the sketch on extrude/revolve, the rollback shift
  - `_state_before`, `_auto_name` using `LABELS`
  - `index` (`"That feature no longer exists."`)
  - `set_params` validation, with the four messages verbatim
  - undo/redo: snapshots of features, rollback, ui and params, capped at 200
- A panic inside an evaluator is caught with `catch_unwind` on native and becomes `"Internal error: {panic message}"`. This is in `EXCEPTIONS.md`, because 1.x shows `"{ExceptionType}: {e}"`.
- `Registry::builtin()` is empty in this milestone; milestone 2 fills it.

- [ ] **Step 1: Write the failing tests** in `tests/document.rs`, using test-only evaluators registered with `Registry::new().with(..)`:
  - `box`: creates body `"b<fid>"` from `Solid::cube`
  - `grow`: scales a body by param `k`
  - `fail`: returns `FeatureError("nope")`
  - `boom`: panics

  Tests:
  - `add_then_evaluate_builds_bodies`
  - `failing_feature_is_refused`: the message is `"nope"` and the features are unchanged
  - `expressions_rebuild_on_param_change`: `grow` with `k: "=2*s"`; after `set_params` changes `s`, the body volume changes
  - `set_params_messages`: all four 1.x messages
  - `rollback_and_shift`
  - `delete_and_move_adjust_rollback`
  - `undo_redo_round_trip`: undo 3 times and redo 3 times, and the features JSON equals the original
  - `auto_names`: `Box1`, then `Box2` for primitive shape box; `Move1` for move
  - `suppressed_is_skipped`
  - `panicking_feature_is_contained`: with `[box, boom, grow]`, boom's error starts with `"Internal error:"`, grow still runs on box's body, and undo works
  - `cache_reuses_unchanged_prefix`: an evaluator call counter does not increase on a second `evaluate()`
- [ ] **Step 2: Run** `cargo test -p stl-engine --test document`. Expected: FAIL.
- [ ] **Step 3: Implement** the files above.
- [ ] **Step 4: Run** the same command plus `cargo build --target wasm32-unknown-unknown -p stl-engine`. Expected: PASS.
- [ ] **Step 5: Commit** "2.0: document model".

### Task 7: `.stlx` project files (read 1.x, write for 1.x)

**Files:**
- Create: `rust/crates/stl-engine/src/stlx.rs`
- Create: `dev/parity/make_project_fixtures.py` → `rust/tests/projects/{basic.stlx,unicode.stlx,extra_keys.stlx}`
  - Each fixture is made with 1.x `Document`, with small STL sources from `rust/tests/models/`.
- Create: `dev/parity/open_check.py`
- Test: `rust/crates/stl-engine/tests/stlx.rs`

**Interfaces:**
- Consumes: Task 6 `Document`, `Feature` and `Source`.
- Produces:

```rust
pub struct FileError(pub String);
pub fn to_bytes(doc: &Document) -> Result<Vec<u8>, FileError>;
pub fn from_bytes(data: &[u8], registry: Registry) -> Result<Document, FileError>;   // evaluates
pub fn save(doc: &Document, path: &Path) -> Result<(), FileError>;
pub fn open(path: &Path, registry: Registry) -> Result<Document, FileError>;
```

- The layout matches `document.py` `save` and `open`:
  - a zip, deflate-compressed
  - `document.json` with `format: 1`, `app: "STL Editor"`, `features`, `rollback`, `ui` and `params`
  - `sources` containing only the sources used by `import` features, as `{sid: {name, ext}}`
  - `sources/<sid><ext>` holding the bytes
- On open the id counter continues after the highest numeric suffix. Unknown top-level keys are kept and written back.
- New messages (listed in `EXCEPTIONS.md`):
  - not a zip, or no `document.json`: `"This isn't an STL Editor project."`
  - `format > 1`: `"This project was made by a newer version of STL Editor."`
  - a missing source entry: `"The imported file is missing from the project."`

- [ ] **Step 1: Write** `make_project_fixtures.py` and run it:
  - `basic`: two imports, two parameters and a rollback
  - `unicode`: source `Teil ö.stl`, body name `Grün`, note `größe`
  - `extra_keys`: a suppressed feature, an extra feature field `"memo": "x"`, an extra `ui` key `"camera": {...}`
- [ ] **Step 2: Write the failing tests** in `tests/stlx.rs`:
  - `opens_1x_fixtures`: `features`, `ui` and `params` equal the fixture's `document.json` as `serde_json::Value`, and source bytes are equal
  - `extra_keys_round_trip`
  - `unicode_round_trip`: open, `to_bytes`, `from_bytes`, and everything is equal
  - `rejects_non_project` and `rejects_newer_format`, with exact messages
  - `writes_for_1x`: saves each fixture into `rust/target/parity-out/`

  Registry: `Registry::builtin()`. Feature errors from unported types are expected and ignored here.
- [ ] **Step 3: Run** `cargo test -p stl-engine --test stlx`. Expected: FAIL.
- [ ] **Step 4: Implement** `stlx.rs`.
- [ ] **Step 5: Verify:**
  - `cargo test -p stl-engine --test stlx` passes
  - `python dev/parity/open_check.py rust/target/parity-out` prints `open_check: 3 ok`
    - The script opens each file with 1.x `Document.open`, and fails if any feature result has an error that the original fixture did not have.
- [ ] **Step 6: Commit** "2.0: .stlx read/write compatible with 1.x".

### Task 8: Feature golden files (batch 1) and the parity runner

**Files:**
- Create: `rust/tests/parity/features/<type>.json` for the batch 1 types: import, primitive, plane, move, scale, mirror, combine, split, delete, separate, repair
- Create: `dev/parity/make_golden.py` → `rust/tests/parity/features_golden/<type>.json`
- Create: `rust/crates/stl-engine/tests/parity.rs`

**Interfaces:**
- Consumes: Task 2 stats and compare functions (Rust and Python); Task 6 `Document`, `Registry::builtin()` and `Ctx`; the models from Task 4.
- Case format: `[{"name": str, "exactness": "exact"|"approx", "sources": {"s1": "<model file>"}, "params": [Param...], "features": [{"type": str, "params": {...}, "name"?: str}]}]`
  - Feature ids are assigned `f1`, `f2` and so on, in order, on both sides.
- Golden format per case:
  - `{"results": [{"id", "error", "warning"}...]`
  - `"bodies": [{"id", "name", "solid": bool, "stats": MeshStats}...]}`, in state order
- `make_golden.py`:
  - builds a 1.x `Document`: `add_source` with the model bytes, sets `doc.params`, sets `doc.features` with the assigned ids, calls `evaluate()`
  - writes the golden file
  - supports `--check`
- Runner rules:
  - A case is **pending** if any feature type is missing from the registry.
  - Otherwise it **fails** on any mismatch: results compared exactly, body ids, names and the solid flag exact, stats through `compare_mesh`.
  - It prints `parity: <passed> passed, <pending> pending, <failed> failed` and asserts `failed == 0`.

- [ ] **Step 1: Write** the 11 case files, at least 3 cases each:
  - one normal case
  - one using an expression parameter
  - one error case with a 1.x message, for example Move on a missing body, Combine with one body, or Repair on a watertight body

  Take parameter names and values from the evaluators in `stleditor/features.py` and their uses in `tests/test_engine.py`.
- [ ] **Step 2: Write** `make_golden.py`; run it, then run it with `--check` (exit 0).
- [ ] **Step 3: Write** `parity.rs` following the runner rules.
- [ ] **Step 4: Run** `cargo test -p stl-engine --test parity -- --nocapture`. Expected: PASS, printing `parity: 0 passed, <N> pending, 0 failed`, where N is the number of cases.
- [ ] **Step 5: Commit** "2.0: batch 1 golden files and parity runner".

### Task 9: CI

**Files:**
- Create: `.github/workflows/rust.yml`

**Interfaces:**
- Consumes: every command above.

- [ ] **Step 1: Write** the workflow, triggered on push and pull_request for paths `rust/**`, `dev/parity/**`, `stleditor/**` and `.github/workflows/rust.yml`.
  - Job `rust` on ubuntu-latest and windows-latest, `working-directory: rust`:
    - install the toolchain from `rust-toolchain.toml`
    - `cargo test --workspace --release`
    - `cargo build --target wasm32-unknown-unknown -p stl-kernel -p stl-parity -p stl-mesh -p stl-sketch -p stl-engine -p stl-io`
    - `cargo run -p xtask -- pure-rust`
  - Job `golden` on windows-latest:
    - set up the same Python version as `tests.yml`
    - `pip install -r requirements.txt`
    - run `make_kernel_golden.py --check`, `make_expr_golden.py --check` and `make_golden.py --check`
    - run `cargo test -p stl-engine --test stlx`, then `python dev/parity/open_check.py rust/target/parity-out`
- [ ] **Step 2: Verify locally:** every command in the workflow passes on Nathan's PC.
- [ ] **Step 3: Commit** "2.0: CI for the Rust workspace". Ask Nathan before pushing; after pushing, the workflow run is green on both jobs.
