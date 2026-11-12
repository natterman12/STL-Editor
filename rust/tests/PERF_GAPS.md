# Where 2.0 is slower than 1.x

**No gaps.** Every row of `cargo run --release -p xtask -- bench` is at or under 1.0 against 1.x.

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

## Pattern circular x12 (open, pending Nathan's OK)

Bench (2026-11-11, quiet, 504k-triangle bench mesh): 1.x 6.933 s, 2.0 7.747 s, ratio 1.12. After
one speed pass, 2.0 moves the 11 copies in parallel (0.37 s down to about 0.1 s). Almost all the
rest, about 8 s, is the union of the 12 copies (`union_plain` in `stl-kernel`, which follows
Manifold 3.5.4 step for step). Neighbouring copies overlap, so the booleans can't be skipped, and
changing the union order would change the triangles. Pattern already works as 1.x does (one batch
union). Closing this needs a faster boolean in the vendored manifold-rust on parts this size.

## Draw Walls, one line (open, pending Nathan's OK)

Bench (2026-11-11, quiet): 1.x 0.987 s, 2.0 1.363 s, ratio 1.38 (1.66 before the speed pass).
The speed pass turned off WebView2's throttling of the hidden window, which made the engine itself
about 1.7 times slower, and cut the argument handling. Support Fins on the same host is now 0.92.
Draw Walls is a short engine call, so a fixed cost of about 0.3 s per call is left: handing the
48 MB triangle soup of the bench mesh to the webview through the `sf://` protocol (about 0.3 s,
inside WebView2), plus building the soup and the cache key. 1.x passes the same string to V8 in
the same process. Closing this would mean keeping the mesh in the page between calls (sending it
once per body, not once per wall).

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
