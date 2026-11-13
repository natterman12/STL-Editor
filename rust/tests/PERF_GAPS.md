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
