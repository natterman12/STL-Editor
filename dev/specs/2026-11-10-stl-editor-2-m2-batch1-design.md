# STL Editor 2.0 Milestone 2: engine batch 1 and file readers (design)

Status: approved in chat on 2026-11-10. This is a sub-spec of `dev/specs/2026-11-10-stl-editor-2-rust-design.md`; everything there still applies.

## 1. Scope

What Nathan decided:

- The file readers move forward from milestone 3 into this milestone, so Import works fully.
- Repair matches desktop 1.x: its trimesh-based steps are ported step by step and checked against desktop golden files.
- Every feature lives in its own file.
- Cursor takes more jobs, including harder ones.

In scope:

- The batch 1 features: Import, Primitive, Plane, Move, Scale, Mirror, Combine, Split (with connector pins), Remove (`delete`), Separate, Repair (modes `auto` and `quick`).
- The 1.x helpers they use:
  - in `features.py`: `apply_tool`, `_affine`, `_place`, `_rotation`, `_float`, `_transform_body`, `_pivot`, `_mirror_matrix`, `_repeat_features`, and `State.frame`
  - in `geometry.py`: `BASE_PLANES`, `plane_frame`, `unit`, `weld`, `merge_overlapping_shells`, `local_repair`, `quick_repair`, `_cap_holes`, `_touching_shells`, `_is_flat`
  - in `printing.py`: `split_with_connectors`
- `State` gains `planes` and `tools`, as in 1.x.
- Mesh readers: STL (binary and ASCII), OBJ, 3MF, PLY (ASCII and binary), OFF, GLB and glTF. They follow `geometry.load_mesh_file`: one part per object or scene geometry, welded, part names as in 1.x, and GLB/glTF parts sorted by position.

Out of scope, with the waiting cases marked:

- **Repair `rebuild`, and `auto` falling back to rebuild.** These need `voxel_rebuild`, which comes in batch 3 (milestone 2's later batches). Until then, the evaluator returns `FeatureError("Rebuild needs the voxel tools, which aren't ported yet.")`. It is a temporary message, removed in batch 3.
- **Combine `cut` with `clearance > 0`.** This needs `offset_solid`, also batch 3.
- File export, SVG, DXF, fonts and STEP stay in milestone 3.

A golden case that needs one of these declares `"requires": ["voxel"]` or `"requires": ["offset"]`. The runner counts such cases as pending until the requirement is registered.

## 2. Structure

| Crate | Added |
|---|---|
| `stl-mesh` | `weld.rs`. A `repair/` module with one file per step: `winding.rs` (trimesh `fix_winding`), `inversion.rs` (`fix_inversion`, multibody), `fill_holes.rs` (trimesh `fill_holes`), `process.rs` (trimesh `process=True, validate=True` cleanup, `nondegenerate_faces`, `unique_faces`, `remove_unreferenced_vertices`, `split`), `cap_holes.rs`, `shells.rs` (`_touching_shells`, `merge_overlapping_shells`, `_is_flat`), `local.rs` (`local_repair`) and `quick.rs` (`quick_repair`) |
| `stl-io` | `read/` with `stl.rs`, `obj.rs`, `threemf.rs`, `ply.rs`, `off.rs`, `gltf.rs`, and `load.rs` (`load_mesh_file`). `stl-io` now depends on `stl-mesh` for `weld`. This is a change to the crate table in the main spec. |
| `stl-engine` | `features/` with one file per feature: `import.rs`, `primitive.rs`, `plane.rs`, `move_.rs`, `scale.rs`, `mirror.rs`, `combine.rs`, `split.rs`, `delete.rs`, `separate.rs`, `repair.rs`. Each file has a `register(Registry) -> Registry` function. `features/common/` holds `frames.rs` (planes, `plane_frame`, `BASE_PLANES`, `State::frame`), `place.rs` (`_rotation`, `_affine`, `_place`, `_pivot`, `_transform_body`, `_mirror_matrix`), `tool.rs` (`apply_tool`, the `tools` record, `_repeat_features`) and `params.rs` (`_float` and friends). `Registry::builtin()` calls every feature's `register`. |

Rules:

- A feature file may use `features/common/` and the lower crates, but never another feature's file.
- A helper used by two or more features goes in `common/`.

## 3. Reference behaviour

- The reference is desktop 1.x on Nathan's PC: Python 3.14, trimesh 5.2.0, manifold3d.
- For readers and repair, the trimesh source installed there is the specification: `trimesh/repair.py`, `trimesh/base.py` (`process`, `validate`), `trimesh/graph.py`, and `trimesh/exchange/{obj,ply,threemf,gltf,off}.py`. Where trimesh's behaviour is decided by a numpy or scipy detail (sort order, tie-breaking, tolerances), the port copies that detail.
- `web/engine/io.js` and the web `localRepair` are allowed as starting points, but the golden files come from desktop 1.x.
- Any place the port cannot match is listed in `rust/tests/EXCEPTIONS.md` with Nathan's agreement, as in milestone 1.

## 4. Golden additions

- **Reader cases:**
  - Inputs: `rust/tests/models/readers/`, made by `dev/parity/make_reader_models.py` with trimesh exports and hand-written files:
    - OBJ with several objects, groups, quads, n-gons and negative indices
    - ASCII and binary STL
    - PLY ASCII and binary little-endian, with extra vertex properties
    - OFF
    - 3MF with two objects and build-item transforms
    - GLB with nested node transforms and two meshes
    - a file with no triangles (error `"No triangles found in this file."`)
  - Golden: `dev/parity/make_reader_golden.py` → `rust/tests/parity/readers_golden.json`. Per file: the list of parts with name and `MeshStats` after weld, or the error message.
- **Feature cases:**
  - Import: at least one case per format, plus `place` `center` and `keep`, and the place-next-to-existing-body rule.
  - Repair: broken models made by `make_models.py`:
    - open box
    - doubled faces
    - two touching cubes sharing a wall
    - an inside-out shell
    - a non-manifold edge
    - overlapping shells, which triggers `merge_overlapping_shells`
  - At least 4 cases for every other batch 1 feature, including every `operation` and `mode` value and at least one 1.x error message.
- `make_golden.py` and the parity runner learn the `requires` field.

## 5. Done when

1. The parity runner shows 0 failed. Every batch 1 case passes except those with `requires`, and the pending count equals the number of `requires` cases.
2. Reader golden: every file matches (parts, names, stats per the 5.3 rules; error text identical).
3. Benchmarks on the 504k-triangle bench mesh (`dev/bench/test_mesh.stl`, made by `dev/bench/make_test_mesh.mjs`):
   - Import with repair on, Combine (join and cut), Split, and Repair `quick` on a broken copy
   - each no slower than 1.x desktop on the same PC
   - run locally by `cargo run -p xtask -- bench`, which also runs `dev/parity/bench_1x.py` and prints both columns
4. Everything from milestone 1 still passes, including the pure-Rust check and the wasm32 build.

## 6. Team

| Who | Jobs |
|---|---|
| Opus 5.5 | Leads and reviews |
| Fable 5.2 | The whole `stl-mesh/repair/` port and `features/repair.rs` |
| Sonnet A | `features/common/`, Split with connectors, the STL reader, `weld`, the bench task |
| Sonnet B | Import, Combine, the 3MF and glTF readers, `load_mesh_file` |
| Cursor | Primitive, Plane, Move, Scale, Mirror (including mirror of features), Remove, Separate; the OBJ, PLY and OFF readers; all golden scripts and case files |

Every job gets a read-only review by a worker other than its author.
