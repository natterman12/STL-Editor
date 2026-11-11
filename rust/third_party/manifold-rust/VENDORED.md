# Vendored manifold-rust

A copy of [manifold-rust](https://github.com/larsbrubaker/manifold-rust) 0.19.2 (Apache-2.0, by Lars Brubaker),
from upstream commit 1e4564ca101674ecdab1dac67a6f6ede18442cd1, used through `[patch.crates-io]` in
`rust/Cargo.toml` so the 2.0 team can fix performance problems (see `rust/tests/PERF_GAPS.md`).

Only `src/`, `Cargo.toml` (as published on crates.io), `LICENSE` and `README.md` are copied.
Changes are kept as separate commits so they can be offered upstream through the fork
https://github.com/natterman12/manifold-rust.

## Changes from upstream

All of these are speed changes to the boolean pipeline (2.0 milestone 2, batch 2, task 7). None
changes a result: every mesh comes out with the same vertices and triangles in the same order.
Nothing here is specific to STL Editor.

### What the profile showed

Measured with `MANIFOLD_TIMING=1` on `dev/bench/test_mesh.stl` (about 504,000 triangles) against a
30 mm cube and against a half-space, 20 threads. Before any change one boolean took 0.13 to 0.33 s:

| Stage | Join with the cube | What it does |
|---|---|---|
| Intersections | 0.033 s | finds where the two meshes cross |
| Assembly | 0.027 s | builds the faces of the result |
| Triangulation | 0.055 to 0.15 s | turns those faces into triangles |
| Simplification | 0.048 s | removes pinched points and needless edges |
| Sorting | 0.067 s | sorts the result and builds its search tree |
| Copies | 0.06 s | three copies of whole meshes (join only) |

The library is a faithful port of the C++ original, which also passes over the whole mesh in every
stage. It was slower for four reasons:

1. **Fresh memory.** One boolean allocated a few hundred megabytes in buffers of megabytes each.
   The system allocator gives such a buffer back to Windows when it is freed and asks for fresh
   memory for the next one, and every 4 KB of fresh memory costs a page fault the first time it is
   touched. That was about a fifth of the time. Passes that run on many threads did not help: the
   page faults queue up behind each other.
2. **Passes on one thread** that the C++ runs on many: building the search tree (0.03 s), sorting,
   reordering halfedges, counting the sides of faces, filling arrays.
3. **The cutter's faces.** Each face of the cube is crossed by about 1,500 edges of the mesh and
   takes 15 to 28 ms to triangulate. The twelve faces sit next to each other in the face list and
   were often handed to the same thread, one after another.
4. **Lists per element.** A list was made for each of 1.5 million halfedges (edge queries) and for
   each of 250,000 vertices (duplicate edges), nearly all of them empty.

Two costs belong to the algorithm and are the same in the C++: ear clipping a face of n points
looks at about n²/4 points, and collapsing edges around a point with hundreds of neighbours walks
all of them for each edge (0.037 s in the cut).

### The changes

| File | Change | Why |
|---|---|---|
| `scratch.rs` (new), `lib.rs` | A pool that keeps large buffers (256 KB and up) when their owner is done with them and hands them to the next array of the same type. Only capacity is reused; a buffer comes back empty. It keeps at most 512 MB, or 64 MB on wasm32, where memory that was once taken is never given back (`scratch::set_limit`; 0 turns it off). `scratch::clear()` frees every pooled buffer and `scratch::held_bytes()` reads how many bytes are pooled, so a caller can empty the pool when a top-level operation is done. | Cuts the page faults of reason 1. |
| `impl_mesh.rs`, `collider.rs`, `disjoint_sets.rs` | A mesh, its search tree and the union-find give their arrays to the pool when dropped, and a copy takes its arrays from it. | So the arrays of the result are recycled too, not only work arrays. |
| `par.rs` | `maybe_par_map` and `maybe_par_map_unzip` take their vector from the pool. New helpers: `maybe_par_sort_distinct`, `maybe_par_map_each_ct`, `maybe_par_filled`, `maybe_par_for_range`, `maybe_par_for_each_chunk_mut`, `maybe_par_for_each_indexed_mut`, `maybe_par_join`. | Used by the changes below. Each gives the result of the sequential loop. |
| `collider.rs` | The radix tree's nodes are worked out in parallel. The boxes are refitted in parallel, one subtree per task, each in its own run of the array (the tree is checked to be a proper radix tree first; if not, the old loop runs). `Collider::from_leaf_fn` writes the leaf boxes straight into the tree. | The tree took 0.03 s of the 0.067 s of sorting. |
| `sort.rs` | Vertices and faces are sorted as packed `(code, index)` keys instead of indices that look their code up. `sort_geometry` no longer builds a list of face boxes, moves it into sorted order and copies it into the tree: the boxes are made in place. | Less memory moved; the same order. |
| `face_op_triangulate.rs` | Only faces with more than four edges go to the general triangulator, one task each. Output arrays are filled in parallel. Boundary halfedges are paired in parallel. New `face2tri_face_refs` takes the reference of a face from a function. | Reason 3; and see `boolean_result.rs`. |
| `polygon_earclip.rs`, `polygon.rs` | The ear cost of each point in the search box is worked out from copies the tree keeps (the point, its two neighbours, whether it is clipped) instead of following the ring's pointers; whether the point counts is asked only if it would change the result. `vert_cost` stays as the reference and is checked against in debug builds. | The slowest face of the join went from 28 ms to about 17 ms. |
| `face_op.rs` | `reorder_halfedges` runs in parallel. `calculate_vert_normals` finds each vertex's first halfedge in parallel and no longer fills an array it then replaces. | Reason 2. |
| `boolean3_kernels.rs` | Edge queries collect their hits in one list per run of 1,024 edges; a small mesh's edges against a large mesh get a task each; an edge whose box misses the other mesh's box is skipped. `winding03` flags broken edges up front and finds the roots without a hash set. The unions still run in index order, because the roots depend on it. | Reason 4. Intersections went from 0.033 s to 0.015 s. |
| `boolean_result.rs`, `boolean_result_assemble.rs` | Side counts and face normals in parallel. Whole edges are appended face by face in parallel: where an edge lands in its face depends only on that face. No reference per halfedge any more: all halfedges of a face have the same one, so it is worked out per face. | Reason 2, and 20 MB less to write and read. |
| `edge_op.rs`, `edge_op_orbits.rs`, `edge_op_tests.rs` | `split_pinched_verts` and `dedupe_edges` share one search for orbit owners when both would find the same ones. Duplicate edges are searched in runs with one work list. | Reason 4. |
| `csg_tree.rs` | A leaf without a pending transform lends its mesh to the boolean instead of copying it, and the final result is moved out instead of copied. | Two of the three copies in a join. |
| `boolean3.rs`, `manifold_boolean.rs`, `impl_mesh.rs` | `Manifold::split` finds the intersections once and builds both results from them, side by side, as the C++ `Manifold::Split` does. Mesh IDs are set aside first so both results get the IDs they would have had. | Half the intersection work of a split, and the two halves at once. |
| `progress.rs` | A type bound that the pooled `maybe_par_map` needs. | |
| `boolean3.rs`, `boolean_result_assemble.rs` | A debug-build check that a split result has no more mesh IDs than were set aside for it. | A guard only. |
| `par.rs`, `boolean3.rs` | New `par::with_big_job_pool`: a boolean (`boolean_with_token_and_stage` after its fast paths) and `Boolean3`'s `split` run in a pool of at most 10 threads (`BIG_JOB_THREADS`), made once; when the current pool is that size or smaller they run directly. `with_small_job_pool` and this share one helper. Only the thread count changes. | Reason 2, and the cost of waking and sharing threads. The passes of a boolean on a 504,000-triangle mesh are short and bound by memory; on a 10-core, 20-thread CPU a union with a 30 mm cube took 125 ms on 20 threads, 117 on 6, 111 on 8, 107 on 10, 109 on 12 and 118 on 16 (kernel call, best of 15). Checked byte for byte: every vertex and triangle of the join on the bench mesh (four layouts, and the cut), and of every corpus case, its stack, and unions of its parts two and three at a time, are the same before and after (cmp, 48 MB). Other entry points (`hull`, `slice`) could use it. |
| several | Finer `MANIFOLD_TIMING` lines inside each stage. | To see where the time goes. |
| `constructors.rs`, `par.rs`, `impl_mesh.rs`, `face_op.rs` | `Manifold::revolve` (51,000 triangles for a 200-point ring at 128 segments: 20 ms before, 10 ms after). Halfedges are made in parallel and sorted as `(key, index)` pairs (the same order as the stable sort); the look for opposed triangles runs in parallel first and the pairing loop that only acts on them is skipped when there are none; face normals and areas are worked out in parallel and the faces sorted by `(area, index)` pairs (the old comparison sort stays for a NaN area); the first halfedge of each vertex is found in one pass. New `par::with_small_job_pool`: `revolve` runs in a pool of at most 8 threads (`SMALL_JOB_THREADS`), because on a 10-core, 20-thread CPU the short passes of a mesh this size took 16 ms on 20 threads, 11 ms on 8 and 23 ms on 1. Only the thread count changes. | Reason 2, and the cost of waking and sharing threads. Checked byte for byte: vertices, halfedges, both normals, triangle references, epsilon, bounding box and the search tree of 8 cases (bench ring, a profile touching the axis, one crossing it, two rings, partial angles, automatic segments) are the same before and after. Other constructors (`extrude`) could use the same pool. |
| `cross_section.rs` | New `CrossSection::from_polygons_with_fill_rule(polygons, fill_rule)`: several contours with a chosen fill rule (same codes as `from_polygon_with_fill_rule`). Adds only; no existing function changes. | Multi-contour even-odd input, so overlapping contours cut holes the way shapely does; `new` is fixed at Positive. Mirrors the C++ `CrossSection(const Polygons&, FillRule)` constructor. Can go upstream. |

### Bench

`cargo run --release -p xtask -- bench`, 2026-11-11, three runs, ratio 2.0 / 1.x. See
`rust/tests/PERF_GAPS.md` (same figures).

| Operation | Ratio (2.0 / 1.x) |
|---|---|
| Combine join, 30 mm cube | 0.82–0.85 |
| Combine cut, 30 mm cube | 0.80 |
| Split at z = 0 | 0.72–0.76 |
| Revolve | 0.67–0.74 |
| Import (repair on) | 0.40–0.41 |
| Repair quick | 0.27–0.29 |
| Extrude | 0.26–0.29 |
| Sweep | 0.29–0.31 |

### Public signatures that changed

The pooled vectors are stored by element type, so these functions of `par.rs` now also require
`T: 'static` (`A: 'static` and `B: 'static` for the unzip): `maybe_par_map`,
`maybe_par_map_unzip`, `maybe_par_map_ct` and, among the new helpers, `maybe_par_map_each_ct` and
`maybe_par_filled`. A caller that maps to a type holding a borrow no longer compiles; nothing in
this crate or in STL Editor does. Worth saying when the change is offered upstream.

### How "same results" was checked

- A dump of every vertex and triangle of the bench operations (import, join, cut, intersect, split,
  repair) and of every mesh of the kernel corpus, before and after: byte for byte the same. Also
  with every parallel threshold set to almost nothing, so the new parallel code ran on the small
  corpus meshes too.
- This crate's own tests (not run by the workspace): 924 pass before, 927 after (three new ones for
  the pool), with and without assertions and with the tiny thresholds; 925 without the `parallel`
  feature. The same 33 fail before and after: they need test files that the published crate leaves
  out.
- `Manifold::split` against `intersection` and `difference` on the bench mesh: the same meshes.

### Open items

These are for Nathan to decide. None is applied.

1. **Split: call `Manifold::split` from the kernel (done).** `Solid::split_by_plane` in
   `rust/crates/stl-kernel/src/solid.rs` calls `self.0.split(&cutter)`, so both halves come from
   one intersection pass built side by side.
2. **Join: no copy of the parts in the kernel (done).** `Solid::union_all` uses `union_plain` on
   borrowed meshes, so overlapping parts are not copied into the tree. Join is now about 0.82–0.85
   (see `rust/tests/PERF_GAPS.md`).
3. **The buffer pool is kept while work continues and emptied when idle (decided by Nathan, 2026-11-10).** `scratch.rs` reuses
   freed buffers (up to 512 MB, 64 MB on wasm32: `scratch::DEFAULT_LIMIT`, `scratch::set_limit(bytes)`, `0` turns the pool
   off) and keeps them between operations. `stl_kernel::scratch` decides when to empty it (`scratch::clear()`): after
   5 seconds with no kernel operation, at the end of a rebuild that evaluated features, and when a `Document` is dropped. Each
   feature evaluation marks itself with `ScratchGuard::begin()`; the pool is never emptied while one is alive. Native: one
   background thread (std only, started lazily) does the idle release. wasm32: no threads and no `Instant`, so the next
   operation empties an idle pool when the host-supplied clock (`stl_kernel::set_clock`, milliseconds) says more than the
   idle time passed; with no clock set, the pool is never emptied on idle. `set_idle_timeout` changes the 5 seconds (tests).
   Results do not depend on the pool (see `rust/tests/PERF_GAPS.md`).
4. **An allocator that keeps freed memory** (mimalloc, say) in the app would recycle everything
   the pool does not cover. It is a new dependency.
5. **Parallel unions in `winding03`**: see below.

### Not done, because it could change results

`winding03` unites the edges on one thread, in index order (about 7 ms on the bench mesh). The C++
unites them on all threads in no fixed order. That picks other representative vertices, and for
meshes that touch exactly the result could differ, so it was left as it is.
