# Where 2.0 is slower than 1.x

Close-out bench (2026-11-13, quiet, median of 3): Offset grow 0.5 mm is 1.x 6.512 s,
2.0 3.120 s, ratio 0.48. The same quiet run also printed Combine join 1.08, Combine cut 1.03,
Split 1.22, Support Fins 1.02, Fit to Bed 1.04 and Pattern circular x12 1.14; those paths were
not changed for Offset.

A/B check of those rows (2026-11-13): the same `xtask bench` built from a clean HEAD (dd965a6, a
separate worktree and target folder) and from the 4b working tree, run in turn A, B, A, B, A, B,
each run started only when no other cargo, rustc or xtask was running. 2.0 seconds, median of the
three runs (each run is itself a median of 3):

| Row | HEAD | 4b | Verdict |
|---|---|---|---|
| Combine join | 0.151 | 0.147 | noise |
| Combine cut | 0.176 | 0.180 | noise |
| Split at z = 0 | 0.149 | 0.155 | noise |
| Support Fins | 7.47 | 7.77 | noise (runs overlap: 7.43 to 7.65 against 7.58 to 7.99) |
| Fit to Bed | 2.86 | 2.86 | noise |
| Pattern circular x12 | 7.95 | 7.98 | noise |
| Offset grow 0.5 mm | 6.49 | 2.94 | faster (the 4b Minkowski batches) |

4b did not slow these rows down: HEAD, without any 4b change, is just as slow. The gap against
the earlier closed numbers (join 0.103 s on 2026-11-11) is the machine, not the code: the 1.x
side slowed by about as much (join 0.125 s then, 0.154 s in the close-out run), so the ratios move
with whatever else the PC is doing. One 4b run of Pattern took 28.7 s (something else woke up);
the median ignores it. Cargo.lock only gains the `rayon` line under stl-kernel, at the version
manifold already used, so the thread pools are unchanged.


## Milestone 6 close-out bench (2026-11-14)

Quiet run, nothing else building: `cargo run --release -p xtask -- bench`, median of 3, after the milestone 6 Split and
Pattern changes. No row is slower than 1.x; the highest are Combine join 0.98, Pattern circular x12 0.93, Fit to Bed 0.93
and Split 0.92.

| Row | 1.x s | 2.0 s | Ratio |
|---|---|---|---|
| Import (repair on) | 1.642 | 0.639 | 0.39 |
| Combine join, 30 mm cube | 0.155 | 0.152 | 0.98 |
| Combine cut, 30 mm cube | 0.186 | 0.162 | 0.87 |
| Split at z = 0 | 0.147 | 0.136 | 0.92 |
| Repair quick, 1% triangles missing | 4.427 | 1.294 | 0.29 |
| Extrude 200-segment sketch, 20 mm | 0.002 | 0.001 | 0.25 |
| Revolve 200-segment sketch, 360 deg | 0.024 | 0.018 | 0.73 |
| Sweep 10 mm circle, 40-point path | 0.011 | 0.003 | 0.25 |
| Simplify ratio 0.1 | 0.556 | 0.437 | 0.79 |
| Remesh edge 1.0 | 10.070 | 2.432 | 0.24 |
| Hollow 2 mm normal | 57.277 | 1.388 | 0.02 |
| Shell 2 mm, no faces | 56.754 | 1.387 | 0.02 |
| Lattice cell 6, strut 1, wall 2 | 56.930 | 1.494 | 0.03 |
| Offset grow 0.5 mm | 6.609 | 2.924 | 0.44 |
| Thin-wall Check 0.8 mm | 1.650 | 1.027 | 0.62 |
| Repair rebuild normal, broken | 11.286 | 1.329 | 0.12 |
| Sculpt Smooth, 20 pts | 0.355 | 0.226 | 0.64 |
| Support Fins, default options | 8.309 | 7.242 | 0.87 |
| Fit to Bed, A1 mini (part too big) | 2.865 | 2.678 | 0.93 |
| Lay Flat auto | 1.381 | 0.964 | 0.70 |
| Plate Layout, 6 copies | 0.254 | 0.135 | 0.53 |
| Pattern circular x12 | 7.675 | 7.136 | 0.93 |
| Thread M8 x 30 external | 0.030 | 0.022 | 0.74 |
| Resize Hole (hole_and_pin) | 0.011 | 0.006 | 0.55 |
| Draw Walls, one line | 1.056 | 0.963 | 0.91 |
| Export STL | 0.114 | 0.035 | 0.30 |
| Export 3MF | 1.527 | 0.851 | 0.56 |
| Export OBJ | 0.952 | 0.332 | 0.35 |
| Export STEP | 8.228 | 3.232 | 0.39 |

## Combine join (closed 2026-11-11)

Join was 1.01 to 1.03 (earlier 1.02 and 1.16). A profile with `MANIFOLD_TIMING=1` showed join and cut
share the same stages (intersections 14 ms, assembly 17, triangulation 29 to 45, simplification 31,
sorting 28), with no stage that join alone pays for: join was the cheaper of the two. The cost was
in how the stages ran. On a 10-core, 20-thread CPU the passes of a boolean on 504,000 triangles are
short and bound by memory, and 20 threads spent more on waking and sharing than they saved. A union
with the 30 mm cube (best of 15, kernel only):

| Threads | 6 | 8 | 10 | 12 | 16 | 20 |
|---|---|---|---|---|---|---|
| Join, ms | 117 | 111 | 107 | 109 | 118 | 126 |

A boolean (and the one-pass split) now runs in a pool of at most 10 threads (`par::with_big_job_pool`
in `rust/third_party/manifold-rust`, listed in its `VENDORED.md`). Only the thread count changes.
Results are the same byte for byte (every vertex and triangle of the join on the bench mesh in four
layouts, the cut, every kernel corpus case and unions of their parts, 48 MB, `cmp` before and after),
and the parity run is unchanged (147 passed, 4 pending, 0 failed). No allocator dependency, no
parallel unions.

Bench (2026-11-11, three runs, ratio 2.0 / 1.x): join 0.82, 0.83, 0.85 (0.103, 0.104 and 0.105 s
against 0.125, 0.125 and 0.124 s); cut 0.80, 0.80, 0.80; Split 0.72, 0.72, 0.76; Import 0.41, 0.41,
0.40; Repair 0.29, 0.27, 0.28; Extrude 0.26 to 0.29; Revolve 0.67 to 0.74; Sweep 0.29 to 0.31.

## Pattern circular x12 (closed 2026-11-12)

Was 1.12 (1.x 6.933 s, 2.0 7.747 s). Profiling with `MANIFOLD_TIMING=1` showed most of the time in
the union of the 12 overlapping copies. When both meshes of a boolean have at least 50,000
triangles, the `Boolean3` build (both `Intersect12` directions and both `Winding03` floods) now runs
on the caller's thread pool instead of the 10-thread pool, and four small kernels are inlined
(`rust/third_party/manifold-rust`, listed in its `VENDORED.md`). The union order is unchanged and no
boolean is skipped. The result on the bench mesh is the same byte for byte (sha256 of vertices and
triangles), the kernel corpus is unchanged, and the parity run is unchanged (276 passed, 0 pending,
0 failed).

Bench (2026-11-12, three runs): 0.93, 0.91, 0.96 (6.60, 6.58 and 6.68 s against 7.07, 7.22 and
6.96 s).

## Draw Walls, one line (closed 2026-11-12)

Was 1.38. The Support Fins webview page now keeps any argument over 1 MB under a key (up to 6,
oldest dropped first; a reload or a new host empties it), so the 48 MB triangle soup of a part
crosses `sf://` once instead of on every wall. The soup and its key are kept for the last 2 bodies
(1 on the web build), the soup is encoded on up to 8 threads, and the key is a tree SHA-256 used
only as an in-process cache key. What is left per call is the engine itself (about 0.65 to 0.7 s,
the same in Node) and the join of the walls to the part (about 0.11 s). Results are unchanged on
both hosts (parity 276 passed, 0 pending, 0 failed, with the replay host and with
`STL_FINS_HOST=webview`).

Bench (2026-11-12, three runs): 0.80, 0.91, 0.93 (0.82, 0.91 and 0.91 s against 1.02, 1.00 and
0.98 s). The bench shares one host across its runs, so only the first run sends the soup.

## Export (milestone 3, 2026-11-12)

All four rows are under 1.0. STEP is compared with `web/engine/step.js` in Node (default heap).
STEP was 2.1 to 2.4 and 3MF 1.08 to 1.21 before one speed pass each (one output buffer, no
per-number allocation, a hand-written edge table, and the CPython / V8 digit rule computed on
the stack).

| Row | Run 1 | Run 2 | Run 3 |
|---|---|---|---|
| Export STL | 0.31 | 0.33 | 0.31 |
| Export 3MF | 0.58 | 0.56 | 0.55 |
| Export OBJ | 0.35 | 0.34 | 0.35 |
| Export STEP | 0.43 | 0.44 | 0.45 |

## Offset grow 0.2 mm on the full bench mesh (closed 2026-11-13)

The 200 to 360 s figure was stale. On `dev/bench/test_mesh.stl` (503,804 triangles), a 0.2 mm
grow simplifies at 0.02, 0.04 and 0.06 mm (21,972, then 12,262, then 8,766 triangles, about
0.17 to 0.18 s each) and stops at 0.06 mm, under the 12,000 cap. It does not try 0.08 mm and
it does not leave the 504k triangles for Minkowski. The work mesh is 8,766 triangles against a
32-triangle sphere. The result is 23,146 triangles (1.x was 23,162).

After simplify, the time is the Minkowski sum: 9 batches of 1,000 triangle hulls, then a union
with the original. Stock, one batch after another (release, kernel `offset_solid`, no
`MANIFOLD_TIMING`): minkowski 5.486 s, union with the original 0.103 s, the whole offset 6.011 s.
The same batches on a desktop pool of at most 8 threads (the web build still runs them one by
one): minkowski 2.347 s, of which the closing merge is about 0.26 s, then the union with the
original 0.115 s. The stages together are 2.976 s. Against the 1.x times of 12.5 s and 25.3 s
that is well under 1.0. The pool is smaller than manifold's 10-thread boolean pool, so each
batch boolean stays on it and the batches overlap. Memory keeps the speedup near 2x, not 9x.
Parts under 4,000 triangles stay on the stock path, which is faster for a handful of batches.

The union order is unchanged: hull point order, batch size 1000, closing list
`[solid, batch0, batch1, ...]`, same Add or Subtract. The bench mesh matches the stock mesh
bit for bit at 0.2 mm (23,146 triangles) and at 0.5 mm (27,338). `offset_unchanged` hashes
every Offset parity case, the combine cut that uses a clearance, and the two kernel Minkowski
cases, recorded before this change.

Code: `rust/crates/stl-kernel/src/minkowski.rs`, called from `Solid::minkowski_sum` and
`minkowski_difference`. `rayon` is a native-only dependency of `stl-kernel`. manifold-rust
is unchanged.

## Offset grow 0.5 mm (bench row, 2026-11-13)

Was 1.02 to 1.03 on 2026-11-11 night, then 1.00, 0.99 and 0.99 on 2026-11-12. The work mesh
still simplifies at the first step, 0.05 mm, to 10,226 triangles (11 batches). The same batch
change applies.

Quiet close-out `xtask bench` on 2026-11-13 (median of 3): 1.x 6.512 s, 2.0 3.120 s,
ratio 0.48. An earlier same-day run with other builds in the checkout was 1.x 6.507 s,
2.0 2.831 s, ratio 0.44. A direct `offset_solid` on the same mesh: stock minkowski 6.308 s
(whole offset 6.726 s), batches together minkowski 2.959 s (stages 3.222 s). Both results
are 27,338 triangles and match bit for bit. The quiet close-out run also printed Combine
join 1.08, Combine cut 1.03, Split 1.22, Support Fins 1.02, Fit to Bed 1.04 and Pattern
circular x12 1.14 (8.294 s against 7.303 s). Those paths were not changed for Offset.

## Split and Pattern circular x12 (milestone 6, 2026-11-14)

The close-out rows were Split 1.22 and Pattern 1.14 (8.294 s against 7.303 s). On a quiet machine
today HEAD, before any change here, measured Split 0.90 to 0.94 and Pattern 0.95 to 1.04 (the
1.x side of Split ran 0.145 to 0.150 s today against 0.122 s in the close-out run, so the ratio
moves with the machine). Both are faster now, and no `xtask bench` row is above 1.0.

**Split, where the time goes** (`MANIFOLD_TIMING=1`, one split of the bench mesh, about 147 ms
through `add_feature`): the cut is all boolean stages, no other cost. Intersections 26 ms,
then assembly 25, triangulation 30, simplification 35 and sorting 38 ms, the two halves built side
by side so each stage shows the slower of the two. Within the intersections, `Intersect12` P to Q
9.5 ms, Q to P 6.5 and the winding flood of P 8.7 ms ran one after another. The pool size is not
the lever: split (kernel only, 15 runs, best) took 117 ms on 8 threads, 122 on 10, 121 on 12,
132 on 16, 139 with the caller's pool of 20 and 146 with a pool of 20 made fresh; building the two
halves one after the other took 129 ms on 10 threads and 167 to 171 ms on 20. The cap of 10 stays.

**Change 1, the two directions side by side** (`Boolean3::new_with_token_and_stage` in
`rust/third_party/manifold-rust/src/boolean3.rs`). The P to Q intersections and the winding of P
need nothing from the Q to P intersections and the winding of Q, so when there is no cancel token
and no stage sink the two chains run side by side (`par::maybe_par_join`). Each chain gives the
numbers it gave in the old order. With a token or a sink the old order runs, so the stage marks
come in the order they did. For Split this hides the 8.7 ms single-thread flood behind the other
direction. For the two large meshes of Pattern it makes no difference (both `Intersect12` calls
already fill the threads: 0.75 + 0.81 s in turn, 1.47 + 1.48 s side by side).

**Pattern, where the time goes**: of the 7.1 s, the two real booleans (the union of two groups of
four copies, 2.0 million triangles each, then that result with the last group) take 2.4 s and 2.0 s,
and `transform` of the 11 copies 0.42 s. The other 2.1 s is invisible to the stage timers: the
non-overlapping copies are put together without a boolean (`compose_meshes`), three groups of four
copies of 504,000 triangles, 0.64 s each, one after another. Inside one compose (2.0 million
triangles) the pieces on one thread are joining the vertices and triangles (about 50 ms), the
triangle references (26 ms) and the flood that finds coplanar faces (168 ms, the order depends on the
areas, so it stays in order); the rest is already parallel.

**Change 2, the groups composed side by side** (`union_plain` in
`rust/crates/stl-kernel/src/solid.rs`, `compose_meshes_from` and `compose_id_count` in
`boolean3.rs`). The groups do not depend on each other, so when at least two groups have more than
one part (and none is a soup) they are composed with `par::maybe_par_map`. Mesh IDs are the one
thing that would depend on timing, so they are set aside first, in group order, which is the order
the loop reserved them in; each compose then uses its own range (`increment_mesh_ids_from`). The
three composes took 1.9 s one after another and 1.7 s side by side: the passes that were already
parallel share the cores, so the gain is the single-thread flood and copies overlapping. A
sequential build (wasm32) runs the groups one after another, as before.

**Same results.** Before and after, on the bench mesh through `add_feature`: Split halves (the
original body and `.2`) sha256 `f141f1306fddbed50de84ccd9ef076b68d2b0efd19304b2503c4fc4ddeee991e`,
Pattern body sha256 `0174129d804ac6e0cf306007b4bef98e3d215a1ed8c50fca618a746619357dd6`, each over
every vertex and triangle, the same three times in a row. The parity run is unchanged (276 passed,
1 of them by exception, 0 pending, 0 failed), as are `cargo test -p stl-kernel -p stl-engine` (134
passed, 0 failed, 1 ignored, release) and the kernel corpus. New test
`rust/crates/stl-kernel/tests/union_groups.rs`: unions of a row of 9 and a ring of 12 parts (two
and three groups) must give the hashes recorded from the old one-after-another loop (a clean HEAD build, with
the same test), and repeat. The hash covers positions, indices and, per triangle, the rank of its
mesh ID among the solid's IDs (`Solid::mesh_id_ranks`, hidden). Reserving the IDs of the groups in
reverse order does not change that hash: the union gives P's and Q's IDs separate ranges, so the
order between groups never shows; shifting the ranges does, in the ring. `cargo build --target wasm32-unknown-unknown -p stl-app` is
clean.

**A/B**, the same `xtask bench` built from a clean HEAD (63052a8, a separate worktree and target
folder) and from this working tree, run in turn A, B, A, B, A, B (each about 16 minutes). Before
and after each run the CPU load was 0 to 15 % and no cargo or rustc was running at those
moments, but other workers were building in the same checkout in between, so treat the third
digit with care. 2.0 seconds, then ratio 2.0 / 1.x, three runs each, medians:

| Row | HEAD 2.0 s | Now 2.0 s | HEAD ratio | Now ratio |
|---|---|---|---|---|
| Split at z = 0 | 0.141, 0.130, 0.132 (0.132) | 0.125, 0.123, 0.128 (0.125) | 0.94, 0.90, 0.90 | 0.87, 0.86, 0.87 |
| Pattern circular x12 | 7.51, 7.14, 7.15 (7.15) | 6.42, 6.45, 6.70 (6.45) | 1.04, 0.97, 0.95 | 0.90, 0.90, 0.91 |
| Combine join | 0.135, 0.139, 0.148 | 0.133, 0.137, 0.140 | 0.87, 0.97, 1.04 | 0.92, 0.94, 0.92 |
| Combine cut | 0.155, 0.157, 0.161 | 0.154, 0.154, 0.158 | 0.89, 0.91, 0.95 | 0.87, 0.90, 0.90 |
| Support Fins | 6.91, 7.08, 6.97 | 7.07, 7.00, 7.27 | 0.85, 0.93, 0.88 | 0.92, 0.87, 0.92 |
| Fit to Bed | 2.51, 2.59, 2.58 | 2.53, 2.58, 2.79 | 0.91, 0.94, 0.91 | 0.92, 0.93, 0.99 |

Split is about 5 % faster and Pattern about 10 % (0.7 s). Only part of the Pattern gain is
explained: the composes went from 1.9 s to 1.7 s side by side, so about 0.5 s of the 0.7 s is not
accounted for by them and may be machine noise (HEAD's three Pattern runs ranged from 7.14 to
7.51 s). The join, cut, Support Fins and Fit to
Bed rows are noise (the run to run spread is as large as the difference), and so is every other
row of the bench table. No row of the three runs of the working tree is above 1.0 (HEAD's Pattern read 1.04 in its first run): the highest is Fit to
Bed at 0.99 in one run (0.92 and 0.93 in the others). The final quiet numbers are the close-out
run's.

**What is left**: Pattern is 0.90 against 1.x and spends most of its time where 1.x does: the two
unions' `Intersect12` (1.5 s and 1.0 s of the 6.4 s) and the stages after them. The flood in
`set_normals_and_coplanar` and `winding03`'s union-find stay in index order because the result
depends on it. Making the collider of a result lazy (it is 14 ms of each half's 38 ms of sorting
in Split) would change a data structure used in 24 places for about a tenth of Split; not done.

## What is left (not gaps)

- The union-find that works out what is inside what (`winding03`) runs on one thread in index order,
  about 7 ms of a 103 ms join. 1.x does it on all threads. The roots depend on the order, so doing
  it in parallel could change results for parts that touch exactly; left alone on purpose.
- The scratch buffer pool is emptied after about 5 seconds idle, at the end of a rebuild that
  evaluated features, and when a project closes (so the app does not hold up to 512 MB, 64 MB on
  web, while idle). On the web build the idle release happens at the start of the next operation,
  and only if the page gives the kernel a clock (`set_clock`).
- The pool cap of 10 is a number measured on one machine. If a machine with many more cores shows
  a boolean slower than 1.x, raise `BIG_JOB_THREADS` in `par.rs` and measure again.
