# Where 2.0 is slower than 1.x

**One gap: Offset 0.2 mm on the full bench mesh** (below). Every row of
`cargo run --release -p xtask -- bench` is at or under 1.0 against 1.x (three runs on
2026-11-12). Offset 0.5 mm on the bench's work mesh is at 1.0 and is marked "watch" below.

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

## Offset grow 0.2 mm on the full bench mesh (open, 2026-11-13)

This is the Offset that uibench tried on `dev/bench/test_mesh.stl` (503,804 triangles) before it
switched to Hollow for its rebuild row.

| | Time | Result |
|---|---|---|
| 1.x `meshtools.offset_solid`, grow 0.2 mm | 12.5 s and 25.3 s (two runs) | 23,162 triangles |
| 2.0 `stl-engine` Offset, grow 0.2 mm | about 200 to 360 s (earlier full uibench runs) | not recorded |

The 1.x times come from `py -3.14 dev/out/offset_timing/offset_timing.py` (log in
`dev/out/offset_timing/offset_timing.log`). The two 1.x runs differ because the second ran
while other jobs were compiling. The 2.0 time was not measured again in this run.

2.0 is 8 to 29 times slower here. 1.x's result has 23,162 triangles, so 1.x works on a much
smaller mesh before its Minkowski sum. In 2.0, a 0.2 mm grow only simplifies within 0.08 mm,
so the Minkowski sum still sees most of the 504k triangles. Next step: compare the two
simplify steps on this mesh (target size and tolerance) before any speed work, because a
different simplify would also change the result.

## Offset grow 0.5 mm (watch)

1.02 to 1.03 in the first bench of 2026-11-11 night, 1.00, 0.99 and 0.99 on 2026-11-12. The
Pattern change is not involved: that path never runs on this row (the work mesh is 10,226
triangles), and timing it with every boolean back on the 10-thread pool gave the same medians
(6.10 to 6.17 s). Most of the time is Minkowski's 485 small booleans. The row sits at 1.0 within
run-to-run spread; if it goes above 1.0 again it needs its own speed pass.

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
