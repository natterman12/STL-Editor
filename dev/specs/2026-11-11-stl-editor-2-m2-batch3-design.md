# STL Editor 2.0 Milestone 2: engine batch 3 (mesh tools) (design)

Status: approved in chat on 2026-11-11. This is a sub-spec of `dev/specs/2026-11-10-stl-editor-2-rust-design.md`. The batch 1 sub-spec's structure rules (one file per feature, `features/common/`) still apply.

## 1. Scope

What Nathan decided:

- **Voxel size (Hollow, Shell, Lattice):** the web rule. The size comes from the solid's bounding box at full precision.
  - 1.x desktop `_hollow_pitch` is changed to match before the golden files are generated (section 3).
  - The old desktop rule rounded the points to float32 first. On the test models the two differ by about 1e-6 mm.
- **Voxel surface:** a pure-Rust port of desktop 1.x's scikit-image marching cubes, not the web's level-set mesher.
- **Simplify:** a pure-Rust port of the algorithm desktop 1.x uses (`fast_simplification` 0.2.0, which wraps Fast Quadric Mesh Simplification, MIT licence), not a port of `decimate.js`. This replaces the `decimate.js` line in the main spec's library table.
- **Matching rules:** unchanged. Volume and area must be within 0.1 percent for every feature, approximate ones included. A case that can't meet this is listed in `rust/tests/EXCEPTIONS.md` for Nathan's OK.
- **The run:** one batch, like batch 2.

In scope:

**Features:**

- Simplify, Remesh, Sculpt, Offset, Hollow, Shell, Lattice and Thin-wall Check.
- Repair `rebuild` mode, and `auto` falling back to rebuild. The temporary message "Rebuild needs the voxel tools, which aren't ported yet." is removed.
- Combine `cut` with `clearance > 0`.

**The 1.x code they use:**

- `features.py`: `_hollow_pitch`, `f_hollow`, `f_shell`, `f_simplify`, `f_sculpt`, `f_remesh`, `f_offset`, the rebuild branch of `f_repair`, and the clearance branch of `f_combine`
- `feature_modules/lattice.py` (`f_lattice`) and `feature_modules/thin_wall.py` (`_thin_residual`, `_thin_parts`, `thin_scan`, `f_thincheck`)
- `geometry.py`: `_grid_for`, `_check_grid`, `_raster_evenodd`, `solid_occupancy`, `_surface_points`, `mesh_occupancy`, `_field_to_manifold`, `voxel_rebuild`, `auto_pitch`, `hollow`, `simplify`
- `meshtools.py`: `offset_solid`
- `printing.py`: `_adjacency`, `_vertex_normals`, `_resample`, `sculpt`
- Remesh follows the web engine, `web/engine/meshedit.js` `remesh` (main spec 5.2), not `printing.py` `remesh`.

**The library steps 1.x calls, ported to pure Rust:**

- `scipy.ndimage`:
  - `distance_transform_edt` (exact Euclidean)
  - `gaussian_filter` (`sigma` as given, scipy defaults `truncate=4.0`, `mode="reflect"`)
  - `binary_dilation` and `binary_erosion` with `generate_binary_structure(3, 1)`, `iterations` and `border_value=0`
  - `binary_fill_holes`
- `scipy.spatial.cKDTree`: `query` (nearest distance) and `query_ball_point`
- `scipy.sparse` neighbour sums, as `sculpt` uses them
- `numpy.linalg.svd`: the last right-singular vector, for the Flatten brush
- `skimage.measure.marching_cubes` with its defaults (method `lewiner`)
- `fast_simplification.simplify` (`target_reduction`, default settings)

**Kernel calls the features need from `stl-kernel`:**

- `Solid::simplify(tolerance)`, `refine_to_length`
- `minkowski_sum` and `minkowski_difference`
- `batch_boolean` and `slice`
- `Shape2d` offset with the miter join (`JoinType.Miter`, miter limit 2.0)
- `decompose`, `area` and `bounds`

All of these exist in the vendored manifold-rust. `stl-kernel` wraps the ones it doesn't expose yet.

**The voxel cap:** 300e6 voxels (`_check_grid`), with 1.x's message "Resolution too fine for this model size. Use a coarser setting." On wasm32, grid allocations use `try_reserve`, and an allocation that fails gives the same message instead of aborting.

Out of scope:

- Batch 4 (printing) and batch 5 (Support Fins, Draw Walls).
- File export, STEP and the interface stay in milestones 3 and 4.
- The web 1.x engine gets no changes: it already uses the chosen voxel-size rule.

## 2. Structure

| Crate | Added |
|---|---|
| `stl-kernel` | The wrappers listed in section 1 that are still missing |
| `stl-mesh` | `voxel/` with one file per step: `grid.rs` (`_grid_for`, `_check_grid`, `_raster_evenodd`, `solid_occupancy`), `surface.rs` (`_surface_points`), `morphology.rs` (dilation, erosion, `binary_fill_holes`), `occupancy.rs` (`mesh_occupancy`), `edt.rs` (`distance_transform_edt`), `gaussian.rs` (`gaussian_filter`), `marching_cubes.rs` (with the Lewiner tables in `marching_cubes/tables.rs`), `field.rs` (`_field_to_manifold`), `rebuild.rs` (`voxel_rebuild`, `auto_pitch`), `hollow.rs` (`hollow`). Also `nearest.rs` (the KD-tree replacement), `simplify/` (`fqms.rs` for the Fast Quadric port and `mod.rs` for `geometry.simplify`), `remesh.rs` (moved from the `rust/remesh` trial crate, which is then deleted), `sculpt.rs`, `offset.rs` (`offset_solid`) and `thin.rs` (`thin_scan`) |
| `stl-engine` | `features/{simplify,remesh,sculpt,offset,hollow,shell,lattice,thincheck}.rs`; `features/common/pitch.rs` (`_hollow_pitch`, used by Hollow, Shell and Lattice); the rebuild path in `features/repair.rs`; the clearance path in `features/combine.rs`; the runner registers the `voxel` and `offset` requirements |

Rules:

- Parallel code (rayon) falls back to one thread on wasm32. The wasm32 builds must still pass.
- Each library port is a plain function on arrays, with no engine types, so it can be tested on its own.

## 3. Reference behaviour

- **Desktop 1.x is the reference for everything except Remesh.** It runs on Nathan's PC, pinned by `dev/parity/requirements-golden.txt` (scipy 1.18.1, scikit-image 0.26.0, fast_simplification 0.2.0, manifold3d 3.5.4). Where a result depends on a library detail (sort order, tie-breaking, border handling, float32 against float64), the port copies that detail.
- **The one 1.x change.** In `stleditor/features.py`, `_hollow_pitch` computes the diagonal from `b.require_solid().bounding_box()` instead of from `b.mesh()`:
  - It is committed on its own, before any golden file is generated.
  - `tests/test_engine.py` must still pass.
  - It is noted in `EXCEPTIONS.md` as a 1.x fix.
  - `hollow`'s own "too large" check and Rebuild's `auto_pitch` are left as they are.
- **Remesh.** The reference is `web/engine/meshedit.js` `remesh(solid, edge, iterations = 5)`, run in Node on manifold-3d WASM by a new harness, `dev/parity/js/remesh_golden.mjs`:
  - Remesh feature cases carry `"reference": "web"`, and their expected stats come from that harness.
  - Their error messages still come from desktop 1.x.
- **Rebuild warning.** The text is "Rebuilt as a watertight solid on a {pitch:.3g} mm grid.", with Python's `.3g` formatting copied exactly.

## 4. Golden additions

- **Feature cases in `rust/tests/parity/features/`:**
  - Each feature gets at least 4 cases, including at least one 1.x error message.
  - **Simplify:** by `ratio`, by `tolerance`, the non-watertight warning path, and the Manifold-simplify fallback.
  - **Remesh:** at least three edge lengths, and edge ≤ 0.
  - **Sculpt:** each brush (`smooth`, `flatten`, `inflate`, `deflate`) on fixed strokes; an unknown brush; no strokes; the folded-surface error.
  - **Offset:** `grow` and `shrink`, more than one body, zero distance, and a shrink that leaves nothing.
  - **Hollow:** each `resolution` (`draft`, `normal`, `fine`), and a wall that is too thick.
  - **Shell:**
    - with no faces, one face, and two faces that meet at an edge
    - the errors for a non-flat face, no hollow space behind the face, and no body
    - a wall ≤ 0
  - **Lattice:** at least two cell and strut sizes, plus the struts-as-wide-as-cells, too-many-struts and zero-size errors.
  - **Thin-wall Check:** a model with no thin spots, one with several, and a minimum ≤ 0. The `thin` result (`min`, `count`, `spots`) must match exactly.
  - **Repair:** the 3 pending rebuild cases now pass, plus `rebuild` with an explicit `voxel`, and each `resolution`.
  - **Combine:** the pending clearance case now passes.
- **Voxel step golden, `rust/tests/parity/voxel/`:** made by `dev/parity/make_voxel_golden.py` from fixed inputs (the test models at coarse voxel sizes, plus hand-made grids).
  - **`solid_occupancy` and `mesh_occupancy`:** identical grids.
  - **`distance_transform_edt` and `gaussian_filter`:** within 1e-6.
  - **`marching_cubes`:** the same triangle count, and the same vertices within 1e-5 mm after sorting.
- **Simplify golden, `rust/tests/parity/simplify/`:** made by `dev/parity/make_simplify_golden.py`. It runs `fast_simplification.simplify` on the test models and the bench mesh at ratios 0.5, 0.1 and 0.02. The port must give the same triangle count, and a symmetric Hausdorff distance under 0.01 mm.
- **Remesh golden:** made by `remesh_golden.mjs`. It writes stats and, for the comparison, a mesh file per case.
- The CI golden job runs the new `--check` scripts, and the Node harness, like the batch 2 ones.

## 5. Done when

1. **Parity:** 0 failed and 0 pending. Every case from batches 1 and 2 still passes.
2. **Golden checks:** the voxel, simplify and remesh checks pass under the section 4 rules. Every earlier golden check still passes.
3. **Matching:** every feature case meets the main spec's 5.3 rules, with volume and area within 0.1 percent. Any exception is listed in `EXCEPTIONS.md` with Nathan's OK.
4. **Bench** on the 504k-triangle bench mesh, run by `cargo run --release -p xtask -- bench`:
   - every existing row stays at or below 1.0
   - new rows: Simplify ratio 0.1, Remesh edge 1.0, Hollow 2 mm `normal`, Shell 2 mm with no faces, Lattice cell 6 mm, Offset grow 0.5 mm, Thin-wall Check 0.8 mm, Repair `rebuild` on the broken copy, and a 20-point Smooth sculpt stroke
   - every new row at or below 1.0, or listed in `PERF_GAPS.md` with the reason and Nathan's OK
   - the Remesh row's 1.x column is desktop 1.x's own remesh
5. **Builds:** the pure-Rust check, the wasm32 builds, `tests/test_engine.py` and CI all pass.

## 6. Team

| Who | Jobs |
|---|---|
| Opus 5.5 | Leads and reviews |
| Worker 6 (Opus 5.5 via the Kilo slot) | `marching_cubes.rs` and its tables; `edt.rs`; `field.rs`; the Fast Quadric Simplify port (`simplify/`) |
| Sonnet A | `nearest.rs`; `morphology.rs` and `occupancy.rs`; `hollow.rs` and `rebuild.rs`; Hollow, Shell, Lattice and Repair rebuild |
| Sonnet B | `sculpt.rs`, `remesh.rs` (moving the trial crate), `offset.rs`, `thin.rs`; Simplify, Remesh, Sculpt, Offset, Thin-wall Check and Combine clearance |
| Worker 7 (Gemini 4 Carbon via the Kilo slot) | `gaussian.rs`, `grid.rs`, `surface.rs`; the `stl-kernel` wrappers; many of the read-only reviews |
| Cursor | The 1.x `_hollow_pitch` change; all golden scripts and case files, including `remesh_golden.mjs`; the bench rows; CI steps; docs |

- Every job gets a read-only review by a worker other than its author.
- Worker 1 (Antigravity) is used only if Nathan says so. Fable is not on the team.
