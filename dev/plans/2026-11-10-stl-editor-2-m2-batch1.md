# STL Editor 2.0 Milestone 2, Batch 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The 11 batch 1 features, the six mesh readers and desktop-matching Repair, all passing their 1.x golden files.

**Architecture:**

- Readers go in `stl-io/src/read/`, with `weld` in `stl-mesh`.
- trimesh's repair steps are ported one file each into `stl-mesh/src/repair/`.
- Each feature is one file in `stl-engine/src/features/`, using shared helpers in `features/common/`.
- New golden files come from desktop 1.x; the milestone 1 parity runner checks them.

**Tech Stack:**

- Everything from milestone 1
- 1.x reference: Python 3.14, trimesh 5.2.0, numpy 2.4.6, manifold3d
- New pure-Rust crates allowed when the `pure-rust` check still passes: `zip` (already used), `quick-xml` (3MF) and `serde_json` (glTF). No other new dependency without the lead's approval.

**Spec:** `dev/specs/2026-11-10-stl-editor-2-m2-batch1-design.md`, which sits under `dev/specs/2026-11-10-stl-editor-2-rust-design.md`.

## Global Constraints

- Everything in the milestone 1 plan's Global Constraints still applies: the pure-Rust rule, manifold-rust only inside `stl-kernel`, wasm32 builds, the matching rules, `EXCEPTIONS.md`, the tool paths, and no commits or pushes.
- One file per feature in `rust/crates/stl-engine/src/features/`. A feature file imports only `features/common/` and lower crates.
- Behaviour is ported from the 1.x function the spec names, line for line where practical. User-facing messages are copied verbatim.
- For readers and repair, the trimesh 5.2.0 source in `C:\Program Files\Python314\Lib\site-packages\trimesh\` is the reference, including its tie-breaking and tolerances.
- The temporary Rebuild message is exactly `"Rebuild needs the voxel tools, which aren't ported yet."`.
- Golden cases that need later work carry `"requires": ["voxel"]` or `"requires": ["offset"]`.

## Review Focus

1. **A part with zero triangles among others in a 3MF or GLB** (an empty object). It is skipped as in 1.x, and the file still imports. Test: Task 2 reader golden case `threemf_empty_object`.
2. **Import `place: "center"` when the project already has a raw, non-solid body.** The new part goes 10 mm to the right of every existing mesh, whether solid or raw. Test: a Task 6 golden case `import_next_to_raw`.
3. **Repair on a mesh where every step fails.** It returns the temporary Rebuild message in `auto` mode and `"Quick repair wasn't enough. Use Rebuild."` in `quick` mode, never a panic. Test: Task 4 `repair_gives_up_cleanly`.
4. **Split by a plane that only touches the body** (tangent to a face). It gives `"The plane doesn't cut through this body."` for `keep: both`. Test: a Task 7 golden case `split_tangent_plane`.
5. **Mirror of features that reference a deleted feature id.** The 1.x message comes through, with no panic. Test: a Task 5 golden case `mirror_features_missing`.

---

### Task 1: Golden extensions (models, reader golden, `requires`, more cases)

**Owner:** Cursor. **Review:** Sonnet A.

**Files:**
- Create: `dev/parity/make_reader_models.py` → `rust/tests/models/readers/*`
- Create: `dev/parity/make_reader_golden.py` → `rust/tests/parity/readers_golden.json`
- Modify: `dev/parity/make_models.py`, adding the broken models: `doubled_faces.stl`, `touching_cubes.stl`, `inside_out.stl`, `nonmanifold_edge.stl`, `overlapping_shells.stl`
- Modify: `dev/parity/make_golden.py` (the `requires` field, copied into the golden record) and `rust/tests/parity/features/*.json` (new cases)
- Modify: `rust/crates/stl-engine/tests/parity.rs`: a case is pending if a required type is unregistered or `requires` names a capability not in `Registry::capabilities()`

**Interfaces:**
- Produces: the reader golden format `{"<file>": {"parts": [{"name": str, "stats": MeshStats}]} | {"error": str}}`, made with 1.x `G.load_mesh_file`, with `stats.mesh_stats` applied to each part.
- Produces: `Registry::capabilities(&self) -> BTreeSet<&'static str>`, empty for now. Batch 3 adds `"voxel"` and `"offset"`.

- [ ] **Step 1:** Write the model scripts and run them. They create every file listed in spec section 4, including `threemf_empty_object.3mf` and `empty.obj`.
- [ ] **Step 2:** Write `make_reader_golden.py` with `--check`. Run it, then run `--check` (exit 0).
- [ ] **Step 3:** Add feature cases: at least 4 per feature, Import once per format, the 6 Repair models, and the Review Focus cases `import_next_to_raw`, `split_tangent_plane` and `mirror_features_missing`. Mark the Rebuild and clearance cases with `requires`. Regenerate with `make_golden.py`, then run `--check`.
- [ ] **Step 4:** Update `parity.rs` and add `capabilities()`. Verify that `cargo test -p stl-engine --test parity -- --nocapture` prints `0 passed, <N> pending, 0 failed`, with N equal to the new total.

### Task 2: Mesh readers and `weld`

**Owners:**
- Sonnet A: `weld.rs`, `stl.rs`, `load.rs`
- Cursor: `obj.rs`, `ply.rs`, `off.rs`
- Sonnet B: `threemf.rs`, `gltf.rs`

The three owners write separate files and run in parallel once `weld` and the `Part` type exist. **Review:** a different worker per file.

**Files:**
- Create: `rust/crates/stl-mesh/src/weld.rs`
- Create: `rust/crates/stl-io/src/read/{mod.rs,stl.rs,obj.rs,threemf.rs,ply.rs,off.rs,gltf.rs,load.rs}`
- Test: `rust/crates/stl-io/tests/readers.rs`

**Interfaces:**
- Produces:

```rust
// stl-mesh
pub fn weld(positions: &[f64], triangles: &[u32], tol: f64) -> (Vec<f64>, Vec<u32>);  // geometry.weld, tol 1e-5
// stl-io
pub struct Part { pub name: String, pub positions: Vec<f64>, pub triangles: Vec<u32> }
pub struct ReadError(pub String);
pub fn load_mesh_file(data: &[u8], filename: &str) -> Result<Vec<Part>, ReadError>;
pub const MESH_EXTS: [&str; 7];   // ".stl",".obj",".3mf",".ply",".off",".glb",".gltf"
// each format: pub fn read(data: &[u8], base: &str) -> Result<Vec<Part>, ReadError> (unwelded; load.rs welds and names)
```

- `weld` matches `geometry.weld` exactly:
  - quantising with `round(v / tol)` uses round-half-to-even, as numpy does
  - output vertices are in `np.unique` (lexicographic) order of the quantised keys
  - the duplicate and opposite-face cancellation rules are the same
- `load_mesh_file` is a port of `geometry.load_mesh_file`. The non-STL formats follow trimesh 5.2.0 `load(..., process=False)` and `Scene.dump()` semantics: node transforms are applied, and part order and names are as 1.x produces them.

- [ ] **Step 1:** Write the failing test `readers_match_golden`: for every file in `readers_golden.json`, compare parts, names and stats with `compare_mesh(Exact)`, and error text exactly. Also write `weld_matches_numpy_rounding`: a vertex at exactly 0.5e-5 rounds half to even.
- [ ] **Step 2:** Run `cargo test -p stl-io --test readers`. Expected: FAIL.
- [ ] **Step 3:** Implement `weld`, `Part`, `load.rs` and `stl.rs` first (Sonnet A), then the other formats in parallel.
- [ ] **Step 4:** Verify: `cargo test -p stl-io --test readers` passes, and the wasm32 build of `stl-io` and `stl-mesh` succeeds.

### Task 3: State, frames and shared helpers (`features/common/`)

**Owner:** Sonnet A. **Review:** Sonnet B.

**Files:**
- Modify: `rust/crates/stl-engine/src/state.rs` (add `planes: IndexMap<String, PlaneDef>` and `tools: IndexMap<String, ToolRecord>`, with clone semantics as in 1.x)
- Create: `rust/crates/stl-engine/src/features/{mod.rs,common/mod.rs,common/frames.rs,common/place.rs,common/tool.rs,common/params.rs}`
- Test: `rust/crates/stl-engine/tests/common.rs`

**Interfaces:**
- Produces:

```rust
pub type Vec3 = [f64; 3];
pub struct Frame { pub o: Vec3, pub x: Vec3, pub y: Vec3, pub n: Vec3 }
pub struct PlaneDef { pub frame: Frame, pub extra: serde_json::Map<String, Value> }
pub struct ToolRecord { pub solid: Solid, pub operation: String, pub targets: Vec<String> }   // fields as 1.x State.tools entries
pub fn base_planes() -> [(&'static str, Vec3, Vec3, Vec3); 3];   // geometry.BASE_PLANES: (name, origin, normal, xdir)
pub fn plane_frame(o: Vec3, n: Vec3, xdir: Option<Vec3>) -> Result<Frame, FeatureError>;
impl State { pub fn frame(&self, r: Option<&Value>) -> Result<Frame, FeatureError>; }   // State.frame, all four branches
pub fn rotation(rx: f64, ry: f64, rz: f64) -> [[f64; 3]; 3];       // _rotation
pub fn affine(r: [[f64; 3]; 3], t: Vec3) -> [[f64; 4]; 3];          // _affine
pub fn place(s: &Solid, f: &Frame, offset: Vec3) -> Solid;          // _place
pub fn pivot(b: &Body, pivot: &Value) -> Result<Vec3, FeatureError>; // _pivot
pub fn transform_body(b: &Body, m: [[f64; 4]; 3]) -> Body;          // _transform_body (raw meshes too)
pub fn mirror_matrix(f: &Frame) -> [[f64; 4]; 3];                   // _mirror_matrix
pub fn apply_tool(state: &mut State, fid: &str, tool: Solid, operation: &str, targets: Option<&Value>, name: &str, res: &mut FeatureResult) -> Result<(), FeatureError>;
pub fn repeat_features(state: &mut State, fid: &str, ids: &[String], transforms: &[[[f64; 4]; 3]], res: &mut FeatureResult, verb: &str) -> Result<(), FeatureError>;
pub fn float(p: &Map<String, Value>, k: &str, default: Option<f64>) -> Result<f64, FeatureError>;  // _float, its message verbatim
```

- [ ] **Step 1: Write the failing tests:**
  - `base_planes_match_1x`
  - `plane_frame_orthonormal` (including the default x-direction rule)
  - `frame_resolves_all_ref_kinds`: a name, `{"plane"}`, `{"origin","normal","xdir"}`, and `flip`
  - `missing_plane_message`: `"Plane 'p9' no longer exists."`
  - `rotation_matches_1x`: compare with values printed by 1.x `_rotation(30,45,60)`, pasted into the test
  - `float_message`: the 1.x `_float` error text
  - `apply_tool_new_join_cut_intersect`: four operations on cubes, with volumes checked
- [ ] **Step 2:** Run `cargo test -p stl-engine --test common`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the same command. Expected: PASS.

### Task 4: Repair port (`stl-mesh/src/repair/`) and the Repair feature

**Owner:** Fable. **Review:** Sonnet B, then Sonnet A on the feature file.

**Files:**
- Create: `rust/crates/stl-mesh/src/repair/{mod.rs,process.rs,winding.rs,inversion.rs,fill_holes.rs,cap_holes.rs,shells.rs,local.rs,quick.rs}`
- Create: `rust/crates/stl-engine/src/features/repair.rs`
- Test: `rust/crates/stl-mesh/tests/repair.rs`
- Create: `dev/parity/make_repair_golden.py` → `rust/tests/parity/repair_golden.json`
  - Per broken model and per step (`weld`, `process`, `fix_winding`, `fix_inversion`, `fill_holes`, `cap_holes`, `touching_shells`, `local_repair`, `quick_repair`, `merge_overlapping_shells`), it records `MeshStats` or `null`.
  - This lets a mismatch be traced to one step.

**Interfaces:**
- Consumes: Task 2 `weld`; `stl_kernel::{Solid, MeshArrays}`.
- Produces:

```rust
pub fn process(v: &[f64], f: &[u32]) -> (Vec<f64>, Vec<u32>);                 // Trimesh(process=True, validate=True)
pub fn fix_winding(v: &[f64], f: &mut Vec<u32>);                               // trimesh.repair.fix_winding
pub fn fix_inversion(v: &[f64], f: &mut Vec<u32>, multibody: bool);
pub fn fill_holes(v: &mut Vec<f64>, f: &mut Vec<u32>);                         // trimesh.repair.fill_holes
pub fn cap_holes(v: &[f64], f: &[u32]) -> (Vec<f64>, Vec<u32>, usize);         // geometry._cap_holes
pub fn touching_shells(v: &[f64], f: &[u32]) -> Option<Solid>;                 // _touching_shells
pub fn merge_overlapping_shells(s: &Solid) -> (Solid, usize);
pub fn local_repair(v: &[f64], f: &[u32]) -> Option<Solid>;
pub fn quick_repair(v: &[f64], f: &[u32]) -> (Vec<f64>, Vec<u32>);
```

- `features/repair.rs` is a port of `f_repair` for modes `auto` and `quick`. Where 1.x would call `voxel_rebuild`, it returns the temporary Rebuild message from Global Constraints.

- [ ] **Step 1:** Write `make_repair_golden.py` and run it, then run `--check`.
- [ ] **Step 2: Write the failing tests:**
  - `steps_match_golden`: every step on every broken model, stats with `compare_mesh(Exact)`
  - `repair_gives_up_cleanly`: a mesh of one stray triangle
- [ ] **Step 3:** Run `cargo test -p stl-mesh --test repair`. Expected: FAIL.
- [ ] **Step 4:** Implement the steps in the order listed in Files.
- [ ] **Step 5:** Verify: the same command passes, and the Repair feature cases without `requires` pass in `parity.rs`.

### Task 5: Simple features (Cursor)

**Owner:** Cursor. **Review:** Sonnet A.

**Files:**
- Create: `rust/crates/stl-engine/src/features/{primitive.rs,plane.rs,move_.rs,scale.rs,mirror.rs,delete.rs,separate.rs}`

**Interfaces:**
- Consumes: Task 3 `common`; `stl_kernel::Solid`.
- Produces: one `pub fn register(r: Registry) -> Registry` per file, each wired into `Registry::builtin()`.

- [ ] **Step 1:** Port `f_primitive`, `f_plane`, `f_move`, `f_scale`, `f_mirror` (including `kind: "features"` through `repeat_features`), `f_delete` and `f_separate`.
  - The torus uses `Shape2d` circle and revolve. If `stl-kernel` lacks a circle constructor, add `Shape2d::circle(r: f64, segments: u32) -> Shape2d` to `stl-kernel` and say so in the report.
- [ ] **Step 2:** Verify that `cargo test -p stl-engine --test parity -- --nocapture` shows these features' cases passing and 0 failed.

### Task 6: Import and Combine

**Owner:** Sonnet B. **Review:** Cursor.

**Files:**
- Create: `rust/crates/stl-engine/src/features/{import.rs,combine.rs}`

**Interfaces:**
- Consumes: Task 2 `load_mesh_file`; Task 4 `local_repair`, `quick_repair` and `merge_overlapping_shells`; Task 3 `common`.

- [ ] **Step 1:** Port `f_import`, including every warning text, `place` `center` and `keep`, and the next-to-existing rule over solid and raw bodies. Port `f_combine`: `join`, `cut` and `intersect`, `keep_tools`, and its messages. `cut` with `clearance > 0` returns `FeatureError` until the `offset` capability exists, and its cases are pending.
- [ ] **Step 2:** Verify that the Import and Combine cases pass (0 failed), including `import_next_to_raw`.

### Task 7: Split with connectors

**Owner:** Sonnet A. **Review:** Fable.

**Files:**
- Create: `rust/crates/stl-engine/src/features/split.rs`
- Create: `rust/crates/stl-engine/src/features/common/connectors.rs`, a port of `printing.split_with_connectors` and its helpers

- [ ] **Step 1:** Port `f_split` and `split_with_connectors`: pin diameter, length and clearance defaults, the skipped-connector warning, `keep` modes, and all three messages.
- [ ] **Step 2:** Verify that the Split cases pass, including `split_tangent_plane` and the connector cases.

### Task 8: Benchmarks

**Owner:** Sonnet A. **Review:** Sonnet B.

**Files:**
- Create: `dev/parity/bench_1x.py`, which times on `dev/bench/test_mesh.stl`, median of 3, on the 1.x Document:
  - Import with repair on
  - Combine join and cut against a 30 mm cube
  - Split at z = 0
  - Repair `quick` on a copy with 1% of its triangles removed
- Create: `rust/xtask/src/bench.rs`, the `cargo run --release -p xtask -- bench` subcommand. It:
  - generates the mesh with `node dev/bench/make_test_mesh.mjs` if it is missing
  - times the same four operations through the Rust `Document`
  - runs `bench_1x.py`
  - prints a table with 1.x seconds, 2.0 seconds and the ratio
  - exits 1 if any ratio is above 1.0

- [ ] **Step 1:** Write both and run them. Paste the table into the report. Any ratio above 1.0 goes back to the owner of that feature.

### Task 9: Close-out

**Owner:** the lead.

- [ ] **Step 1:** Run:
  - `cargo test --workspace --release`
  - `cargo run -p xtask -- pure-rust`
  - the wasm32 build of the 6 crates
  - `cargo test -p stl-engine --test parity -- --nocapture` (0 failed; pending equals the `requires` count)
  - every `--check` script
  - `open_check.py`
  - `cargo run --release -p xtask -- bench`
  - the 1.x `pytest`
- [ ] **Step 2:** Update `rust/tests/EXCEPTIONS.md` for anything new, with Nathan's agreement already recorded in the report or flagged as a question.
- [ ] **Step 3:** Report the result lines, the bench table and open questions. Do not commit.
