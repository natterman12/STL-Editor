# STL Editor 2.0 Milestone 2, Batch 3 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The eight mesh-tool features work in Rust and match 1.x on golden files:

- Simplify, Remesh, Sculpt, Offset, Hollow, Shell, Lattice and Thin-wall Check
- Repair Rebuild and Combine clearance as well, so parity shows 0 pending

**Architecture:**

- **Library steps.** The ones 1.x gets from scipy, scikit-image and fast_simplification become plain array functions in `stl-mesh`: `voxel/`, `nearest.rs`, `simplify/`.
- **Mesh tools.** They're built on those: `hollow`, `voxel_rebuild`, `sculpt`, `remesh`, `offset_solid` and `thin_scan`.
- **Features.** Each feature is one file in `stl-engine/src/features/`.
- **Golden files.** They come from desktop 1.x, except Remesh, which comes from the web engine run in Node.

**Tech Stack:**

- Everything from batches 1 and 2.
- No new dependency without the lead's approval. The KD-tree, morphology, distance field, smoothing, marching cubes and quadric simplification are our own code.

**Spec:** `dev/specs/2026-11-11-stl-editor-2-m2-batch3-design.md`, under the main 2.0 spec and the batch 1 and 2 sub-specs.

**Already done before the run** (commit `e6ad2b9`): the 1.x `_hollow_pitch` change from spec section 3. `tests/test_engine.py` passes (81 passed). Golden files may be generated from this commit on.

## Global Constraints

- Everything in the batch 1 and batch 2 plans' Global Constraints still applies:
  - one file per feature
  - messages verbatim
  - `EXCEPTIONS.md` and `requires`
  - the tool paths
  - pure Rust and the wasm32 builds
  - no commits or pushes
- **The reference for each library step** is the upstream source at exactly the pinned version:
  - scipy v1.18.1: `scipy/ndimage/src/ni_morphology.c`, `_morphology.py`, `_filters.py`, `src/ni_filters.c`, `src/ni_support.c`
  - scikit-image v0.26.0: `skimage/measure/_marching_cubes_lewiner.py`, `_marching_cubes_lewiner_luts.py`, `_marching_cubes_lewiner_cy.pyx`
  - fast_simplification v0.2.0: `fast_simplification/Simplify.h` and `_simplify.pyx`

  Read these from the projects' GitHub tags. They are a reference only and are not vendored.
- **Licences.** Ported files start with a comment crediting the original project and its licence: BSD-3-Clause for scipy and scikit-image, MIT for Fast Quadric Mesh Simplification. `marching_cubes/tables.rs` keeps scikit-image's BSD notice in full.
- **Grid layout.** `Grid3<T> { n: [usize; 3], data: Vec<T> }`, in C order as numpy: index `(i * n[1] + j) * n[2] + k`, with `i` along x. Value types follow 1.x:
  - occupancy is `bool`
  - the Gaussian input is `f32` (`occ.astype(np.float32)`)
  - the EDT output is `f64`
  - the hollow field goes to `f32` before marching cubes

  Where the referenced source changes type or rounds, the port does the same.
- **Voxel cap.** The 300e6 check (`_check_grid`) runs before any grid is allocated. Every grid allocation goes through `Grid3::try_new`, which uses `try_reserve` and maps failure to `"Resolution too fine for this model size. Use a coarser setting."`
- **Matching (spec section 4).** Volume and area within 0.1 percent for every feature case. A case that misses goes to `EXCEPTIONS.md` with the numbers and waits for Nathan's OK; the rule is not loosened.
- **Golden inputs.** No golden case or golden script reads `dev/bench/test_mesh.stl`. Golden inputs are the committed models in `rust/tests/models/` and primitives built inside the case.
- **Voxel golden size.** All voxel golden files together stay under 5 MB:
  - small grids are stored in full
  - large `bool` grids as `{"sha256": <C-order bytes, 1 byte per cell>, "count": <true cells>}`
  - large float fields as `{"sum", "min", "max", "sample": [every 97th value]}`
- **Rayon.** Code that uses rayon also builds without it on wasm32 (`cfg(target_arch = "wasm32")` runs one thread).
- **Workers.** Fable is not on this team. Worker 1 is used only if Nathan says so.

## Review Focus

1. **Simplify with `ratio: 0`.** 1.x treats 0 as "not given" and keeps half the triangles (`float(ratio) if ratio else None`, then the default 0.5); it isn't an error or an empty mesh. Test: a Task 1 case `simplify_ratio_zero`.
2. **A Hollow or Shell grid over the voxel cap**, for example a 3000 mm cube at `fine` with a 0.5 mm wall. It gives the 1.x cap message, quickly, without trying to allocate. Tests: a Task 1 case `hollow_too_fine`, and the Task 3 unit test `check_grid_rejects_before_alloc`.
3. **A Sculpt stroke that misses the surface**, because every sample point is farther than the radius from every vertex. The body is unchanged, and there's no error. Test: a Task 1 case `sculpt_stroke_misses`.
4. **Offset on a body with more than 12000 triangles.** It goes through 1.x's simplify-first path, so grow still adds the slack back and the result is at least `distance` everywhere. Test: a Task 1 case `offset_detailed_body`, using a sphere primitive with enough segments to exceed 12000 triangles.
5. **Hollow on a body made of two separate solids** (two cubes apart in one body). Each gets its own cavity, matching 1.x. Test: a Task 1 case `hollow_two_shells`.

---

### Task 1: Golden harnesses, case files and CI

**Owner:** Cursor. **Review:** Worker 7.

**Files:**
- Create:
  - `dev/parity/make_voxel_golden.py` → `rust/tests/parity/voxel_golden.json`
  - `dev/parity/make_simplify_golden.py` → `rust/tests/parity/simplify_golden.json`, plus meshes in `rust/tests/parity/simplify/`
  - `dev/parity/js/remesh_golden.mjs` → `rust/tests/parity/remesh_golden.json`, plus meshes in `rust/tests/parity/remesh/`
- Create: `rust/tests/parity/features/{simplify,remesh,sculpt,offset,hollow,shell,lattice,thincheck}.json`
- Modify:
  - `rust/tests/parity/features/{repair,combine}.json` (the new rebuild cases)
  - `dev/parity/make_golden.py`
  - `.github/workflows/rust.yml`: the golden job runs the two new Python `--check`s and `node dev/parity/js/remesh_golden.mjs --check`

**Interfaces:**
- **Produces the voxel golden format:** `{case: {"op": "solid_occupancy" | "mesh_occupancy" | "edt" | "gaussian" | "dilate" | "erode" | "fill_holes" | "marching_cubes" | "field_to_manifold", "input": {...}, "output": {...}}}`. Model inputs are `{"model": <path>, "pitch": f}`; grid inputs are inline or in the size-limit form from the Global Constraints. `marching_cubes` output is `{"verts": [[x, y, z]...], "faces": [[a, b, c]...]}` exactly as skimage returns them.
- **Produces the simplify golden format:** `{case: {"model": str, "ratio": f, "tris": n, "mesh": "simplify/<case>.stl"}}`, from `fast_simplification.simplify` with `target_reduction = 1 - ratio` on the welded model, before 1.x's weld and repair steps.
- **Produces the remesh golden format:** `{case: {"stats": MeshStats, "mesh": "remesh/<case>.stl"}}`, from `web/engine/meshedit.js` `remesh(solid, edge)` on manifold-3d WASM in Node.
- **Rule for `make_golden.py`:** a case with `"reference": "web"` runs on desktop first. If desktop raises, that error record is kept. Otherwise the record's stats come from `remesh_golden.json`.

- [ ] **Step 1:** Write every case listed in spec section 4, including the five Review Focus cases. Each feature has at least 4 cases and at least one 1.x error.
- [ ] **Step 2:** Write the three scripts and run them. Then run each with `--check` (exit 0). Confirm the voxel golden files total under 5 MB.
- [ ] **Step 3:** Run `make_golden.py`, then `make_golden.py --check`. Run `cargo test -p stl-engine --test parity -- --nocapture`. Expected: the new cases are pending, 0 failed.

### Task 2: `stl-kernel` wrappers

**Owner:** Worker 7. **Review:** Sonnet B.

**Files:**
- Modify: `rust/crates/stl-kernel/src/{solid.rs,shape2d.rs}`
- Test: `rust/crates/stl-kernel/tests/wrappers.rs`

**Interfaces:**
- Produces:

```rust
impl Solid {
    pub fn simplify(&self, tolerance: f64) -> Solid;            // manifold3d Manifold.simplify
    pub fn minkowski_sum(&self, o: &Solid) -> Solid;
    pub fn minkowski_difference(&self, o: &Solid) -> Solid;
    pub fn sphere(radius: f64, segments: u32) -> Solid;         // Manifold.sphere(r, segs)
    pub fn batch_union(parts: &[Solid]) -> Solid;               // batch_boolean(.., Add); reuse union_all if identical
}
impl Shape2d {
    pub fn offset_miter(&self, delta: f64, miter_limit: f64) -> Shape2d;  // offset(delta, Miter, limit, 0)
    pub fn area(&self) -> f64;
    pub fn to_polygons(&self) -> Vec<Vec<[f64; 2]>>;
    pub fn from_polygons_evenodd(polys: &[Vec<[f64; 2]>]) -> Shape2d;   // CrossSection(contours, FillRule.EvenOdd), for Shell
    pub fn intersection(&self, o: &Shape2d) -> Shape2d;                  // `^`, for Shell
}
```

- Add only the methods that `stl-kernel` doesn't have yet. If one already exists under another name, use that name and note it in the review.

- [ ] **Step 1:** Write the failing test `wrappers_match_manifold3d`. Add five cases to the kernel corpus script (`make_kernel_golden.py`): sphere r=1 segs=8 (exact triangle count and volume), Minkowski sum and difference of a cube with that sphere, simplify of a sphere at a tolerance of 0.01, and a miter offset of an L-shape by -0.4 with a limit of 2.0. Regenerate it with `--check`.
- [ ] **Step 2:** Run `cargo test -p stl-kernel --test wrappers`. Expected: FAIL.
- [ ] **Step 3:** Implement each method as a thin call into manifold-rust.
- [ ] **Step 4:** Run the same command and the kernel corpus test. Expected: PASS.

### Task 3: Voxel grid, surface points and smoothing

**Owner:** Worker 7. **Review:** Sonnet A.

**Files:**
- Create: `rust/crates/stl-mesh/src/voxel/{mod.rs,grid.rs,surface.rs,gaussian.rs}`
- Test: `rust/crates/stl-mesh/tests/voxel_basic.rs`

**Interfaces:**
- Produces:

```rust
pub struct Grid3<T> { pub n: [usize; 3], pub data: Vec<T> }
impl<T: Clone> Grid3<T> { pub fn try_new(n: [usize; 3], fill: T) -> Result<Self, MeshError>; pub fn idx(&self, i: usize, j: usize, k: usize) -> usize; }
pub fn grid_for(lo: [f64; 3], hi: [f64; 3], pitch: f64, pad: usize) -> ([f64; 3], [usize; 3]);   // _grid_for
pub fn check_grid(n: [usize; 3]) -> Result<(), MeshError>;                                       // _check_grid, 300e6
pub fn raster_evenodd(polys: &[Vec<[f64; 2]>], ox: f64, oy: f64, pitch: f64, nx: usize, ny: usize) -> Vec<bool>;  // _raster_evenodd, row-major (nx, ny)
pub fn solid_occupancy(s: &Solid, pitch: f64) -> Result<(Grid3<bool>, [f64; 3]), MeshError>;
pub fn surface_points(verts: &[[f64; 3]], faces: &[[u32; 3]], spacing: f64) -> Vec<[f64; 3]>;   // _surface_points, same order as 1.x
pub fn gaussian_filter(field: &mut Grid3<f32>, sigma: f64);   // scipy defaults: truncate 4.0, mode "reflect"
```

- `MeshError::Message(String)` carries user-facing text, as in batch 1.

- [ ] **Step 1:** Write the failing tests:
  - `solid_occupancy_matches_golden` and `gaussian_matches_golden` (within 1e-6), from `voxel_golden.json`
  - `surface_points_match_1x`: the same points in the same order, within 1e-12, on the `holed_block` model
  - `check_grid_rejects_before_alloc`: `check_grid([1000, 1000, 301])` returns the cap message
- [ ] **Step 2:** Run `cargo test -p stl-mesh --test voxel_basic`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the same command, plus the wasm32 build of `stl-mesh`. Expected: PASS.

### Task 4: Distance field, marching cubes and field to solid

**Owner:** Worker 6. **Review:** Sonnet A reviews `edt.rs`; Worker 7 reviews `marching_cubes.rs` and `field.rs`.

**Files:**
- Create: `rust/crates/stl-mesh/src/voxel/{edt.rs,marching_cubes.rs,marching_cubes/tables.rs,field.rs}`
- Test: `rust/crates/stl-mesh/tests/voxel_surface.rs`

**Interfaces:**
- Consumes: `Grid3`, `MeshError` (Task 3); `Solid::from_mesh` (kernel); `weld` (batch 1).
- Produces:

```rust
pub fn distance_transform_edt(occ: &Grid3<bool>) -> Grid3<f64>;    // in voxels: distance from each true cell to the nearest false cell
pub fn marching_cubes(field: &Grid3<f32>, level: f32, spacing: f64) -> (Vec<[f64; 3]>, Vec<[u32; 3]>);  // skimage defaults (lewiner)
pub fn field_to_solid(field: &Grid3<f32>, origin: [f64; 3], pitch: f64, level: f32) -> Result<Solid, MeshError>;  // _field_to_manifold
```

- `field_to_solid` copies `_field_to_manifold` step by step:
  - pad by 1 with `min(field.min(), level) - 1`
  - shift by `origin - pitch`
  - reverse each face
  - `weld` with `tol = pitch * 1e-4`
  - the two winding retries, and the volume-sign retry
  - the message `"Voxel rebuild produced an invalid mesh."`

- [ ] **Step 1:** Write the failing tests:
  - `edt_matches_golden` (within 1e-6)
  - `marching_cubes_matches_golden`: the same triangle count, and the same vertex set within 1e-5 after sorting
  - `field_to_solid_matches_golden`: stats per spec 5.3
- [ ] **Step 2:** Run `cargo test -p stl-mesh --test voxel_surface`. Expected: FAIL.
- [ ] **Step 3:** Implement the EDT as the exact two-pass separable transform scipy uses. Port the Lewiner case tables and the cube walk, including its ambiguity tests, from the referenced sources.
- [ ] **Step 4:** Run the same command. Expected: PASS.

### Task 5: Nearest-point search, morphology and mesh occupancy

**Owner:** Sonnet A. **Review:** Worker 7.

**Files:**
- Create: `rust/crates/stl-mesh/src/{nearest.rs,voxel/morphology.rs,voxel/occupancy.rs}`
- Test: `rust/crates/stl-mesh/tests/voxel_occupancy.rs`

**Interfaces:**
- Produces:

```rust
pub struct PointTree { /* static KD-tree over [f64; 3] */ }
impl PointTree {
    pub fn new(points: &[[f64; 3]]) -> Self;
    pub fn nearest_dist(&self, q: [f64; 3]) -> f64;               // cKDTree.query distance
    pub fn within(&self, q: [f64; 3], r: f64) -> Vec<usize>;      // query_ball_point (r inclusive, as scipy)
}
pub fn binary_dilation(g: &Grid3<bool>, iterations: usize) -> Grid3<bool>;   // 6-neighbour cross structure
pub fn binary_erosion(g: &Grid3<bool>, iterations: usize) -> Grid3<bool>;    // border_value = 0
pub fn binary_fill_holes(g: &Grid3<bool>) -> Grid3<bool>;
pub fn mesh_occupancy(verts: &[[f64; 3]], faces: &[[u32; 3]], pitch: f64) -> Result<(Grid3<bool>, [f64; 3]), MeshError>;
```

- `mesh_occupancy` keeps 1.x's pad of 12, the half-pitch surface spacing, `np.rint` rounding (round half to even), the iteration list `(0, 1, 2, 3, 5, 8, 11)` and the `0.2 * shell_count` test.

- [ ] **Step 1:** Write the failing tests:
  - `morphology_matches_golden` and `mesh_occupancy_matches_golden` (identical grids)
  - `point_tree_matches_brute_force`: 10,000 random queries against a linear scan, for both methods
- [ ] **Step 2:** Run `cargo test -p stl-mesh --test voxel_occupancy`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run it. Expected: PASS.

### Task 6: `hollow` and `voxel_rebuild`

**Owner:** Sonnet A. Start after Tasks 3–5. **Review:** Worker 6.

**Files:**
- Create: `rust/crates/stl-mesh/src/voxel/{hollow.rs,rebuild.rs}`
- Test: `rust/crates/stl-mesh/tests/voxel_tools.rs`

**Interfaces:**

```rust
pub fn hollow(s: &Solid, thickness: f64, pitch: Option<f64>, cavity_only: bool) -> Result<Solid, MeshError>;  // geometry.hollow
pub fn voxel_rebuild(verts: &[[f64; 3]], faces: &[[u32; 3]], pitch: f64, smooth: f64) -> Result<Solid, MeshError>;
pub fn auto_pitch(verts: &[[f32; 3]], target: f64) -> f64;   // 1.x passes float32 mesh points here; keep that
```

- `hollow` copies 1.x:
  - the size check, which uses the float32 mesh points from `to_mesh`
  - the default pitch
  - `edt * pitch - pitch * 0.5`
  - the `2.5 * pitch` band, refined with `PointTree` over `surface_points(.., pitch / 3.0)`
  - `field[!occ] = -pitch`, then the empty-cavity check
  - `simplify(pitch * 0.1)`
  - both messages verbatim

- [ ] **Step 1:** Write the failing test `hollow_and_rebuild_match_1x`. Run `hollow` at each 1.x quality's pitch and `voxel_rebuild` on the broken models; compare with the Task 1 feature golden records per spec 5.3.
- [ ] **Step 2:** Run `cargo test -p stl-mesh --test voxel_tools`. Expected: FAIL.
- [ ] **Step 3:** Implement. Use rayon for the band queries, as 1.x uses `workers=-1`.
- [ ] **Step 4:** Run it. Expected: PASS.

### Task 7: Simplify port

**Owner:** Worker 6. **Review:** Sonnet B.

**Files:**
- Create: `rust/crates/stl-mesh/src/simplify/{mod.rs,fqms.rs}`
- Test: `rust/crates/stl-mesh/tests/simplify.rs`

**Interfaces:**

```rust
pub fn fqms_simplify(verts: &[[f32; 3]], faces: &[[u32; 3]], target_reduction: f64) -> (Vec<[f64; 3]>, Vec<[u32; 3]>);  // fast_simplification.simplify defaults
pub enum Simplified { Solid(Solid), Raw(Vec<[f64; 3]>, Vec<[u32; 3]>) }
pub fn simplify(v: &[[f32; 3]], f: &[[u32; 3]], solid: Option<&Solid>, target_ratio: Option<f64>, tolerance: Option<f64>) -> Simplified;  // geometry.simplify
```

- `simplify` copies `geometry.simplify`:
  - the tolerance-only path
  - the ratio clamp `[0.001, 1.0]`
  - weld, then `quick_repair` (batch 1)
  - the 12-step Manifold-simplify fallback (`tol *= 1.8`, the `1.15` limit)
  - the raw-mesh return

- [ ] **Step 1:** Write the failing test `fqms_matches_golden`: the same triangle count, and a symmetric Hausdorff distance under 0.01 mm against `simplify/<case>.stl`.
- [ ] **Step 2:** Run `cargo test -p stl-mesh --test simplify`. Expected: FAIL.
- [ ] **Step 3:** Port `Simplify.h`: the same quadrics, error formula, threshold schedule (`0.000000001 * (iteration + 3)^agg`), update order and compaction.
- [ ] **Step 4:** Run it. Expected: PASS.

### Task 8: Sculpt, Remesh, Offset and the thin-wall scan

**Owner:** Sonnet B. **Review:** Worker 7.

**Files:**
- Create: `rust/crates/stl-mesh/src/{sculpt.rs,remesh.rs,offset.rs,thin.rs}`. `remesh.rs` is the `rust/remesh` trial's `remesh_core`, without wasm-bindgen.
- Test: `rust/crates/stl-mesh/tests/mesh_tools.rs`

**Interfaces:**

```rust
pub struct Stroke { pub points: Vec<[f64; 3]>, pub radius: f64, pub strength: f64, pub mode: String }   // serde, 1.x stroke JSON, same defaults (5, 0.5, "smooth")
pub fn sculpt(s: &Solid, strokes: &[Stroke]) -> Result<Solid, MeshError>;       // printing.sculpt
pub fn remesh(s: &Solid, edge: f64, iterations: u32) -> Result<Solid, MeshError>;   // meshedit.js remesh, iterations 5
pub fn offset_solid(s: &Solid, distance: f64, max_tris: usize) -> Result<Solid, MeshError>;   // meshtools.offset_solid, max_tris 12000
pub struct ThinScan { pub min: Option<f64>, pub count: usize, pub spots: Vec<[f64; 3]> }
pub fn thin_scan(s: &Solid, t: f64) -> ThinScan;                                // thin_wall.thin_scan
```

- **`sculpt`:**
  - rebuilds `PointTree` per stroke, as 1.x does
  - sorts the indices
  - the Flatten brush uses the last right-singular vector of the weighted 3×n matrix; its sign doesn't matter, because it is applied as `dist * nrm`
  - messages verbatim, including `"Unknown brush {mode}."`
- **`thin_scan`:** copies the slice steps, the binary search of 8 halvings, the link distance and the union-find grouping. Spots are sorted by `(-size, coords)` and rounded as 1.x rounds them.

- [ ] **Step 1:** Write the failing tests:
  - `sculpt_modes_match_1x`
  - `remesh_matches_web_golden`
  - `offset_grow_and_shrink_match_1x`
  - `thin_scan_matches_1x`: `count` exact; spots within 0.01; `min` within 0.001
- [ ] **Step 2:** Run `cargo test -p stl-mesh --test mesh_tools`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run it. Expected: PASS.

### Task 9: The features

**Owners:**
- Sonnet A: `common/pitch.rs`, `hollow.rs`, `shell.rs`, `lattice.rs`, and the rebuild path in `repair.rs`
- Sonnet B: `simplify.rs`, `remesh.rs`, `sculpt.rs`, `offset.rs`, `thincheck.rs`, the clearance path in `combine.rs`, and the registry capabilities

Start after Tasks 2 and 6–8. **Review:** Sonnet A's files by Worker 6; Sonnet B's files by Worker 7.

**Files:**
- Create: `rust/crates/stl-engine/src/features/{simplify,remesh,sculpt,offset,hollow,shell,lattice,thincheck}.rs`, `features/common/pitch.rs`
- Modify:
  - `features/{repair,combine}.rs`: drop the temporary Rebuild message
  - `registry.rs`: `capabilities()` returns `{"voxel", "offset"}`
  - `features/mod.rs`: registration

**Interfaces:**
- `hollow_pitch(s: &Solid, t: f64, q: Option<&str>) -> f64` in `common/pitch.rs`. It computes the diagonal from `s.bounding_box()`, with divisors draft 80, normal 150, fine 260, and a default of 150.
- Thin-wall Check writes `res["thin"] = {"min", "count", "spots"}` and the 1.x `info` or `warning` text, with Python `:g` formatting copied exactly.
- The Repair warning uses Python `.3g` formatting, copied exactly.

- [ ] **Step 1:** Port each `f_*`, with messages verbatim.
- [ ] **Step 2:** Run `cargo test -p stl-engine --test parity -- --nocapture`. Expected: 0 failed, 0 pending, and all five Review Focus cases pass.

### Task 10: Bench rows

**Owner:** Cursor. **Review:** Sonnet B.

**Files:**
- Modify: `rust/xtask/src/bench.rs`, `dev/parity/bench_1x.py`

- [ ] **Step 1:** Add these rows on `dev/bench/test_mesh.stl`, in both columns:
  - Simplify ratio 0.1
  - Remesh edge 1.0 (the 1.x column uses desktop `printing.remesh`)
  - Hollow 2 mm `normal`
  - Shell 2 mm with no faces
  - Lattice cell 6 mm, strut 1 mm, wall 2 mm
  - Offset grow 0.5 mm
  - Thin-wall Check 0.8 mm
  - Repair `rebuild` `normal` on the existing broken copy
  - Sculpt: one Smooth stroke, radius 5, strength 0.5, 20 points evenly spaced along x across the bounding box at its centre y and top z
- [ ] **Step 2:** Run `cargo run --release -p xtask -- bench`. Any new row above 1.0 goes back to the owner of that tool's task, with the profile.

### Task 11: Close-out

**Owner:** the lead.

- [ ] **Step 1:** Run:
  - `cargo test --workspace --release`
  - the pure-Rust check
  - the wasm32 build of the 6 crates
  - the parity runner (0 failed, 0 pending)
  - every `--check` script, Python and JS
  - `open_check.py`
  - the bench (every row ≤ 1.0, or listed)
  - the 1.x `pytest`
- [ ] **Step 2:** Delete `rust/remesh/` (approved in the spec). Update the reference in `dev/RUST_TRIAL.md` to point at `stl-mesh/src/remesh.rs`.
- [ ] **Step 3:** Update `EXCEPTIONS.md` and `PERF_GAPS.md`. List open questions for Nathan.
- [ ] **Step 4:** Report the result lines, the bench table and the questions. Do not commit.
