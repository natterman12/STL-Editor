# Where 2.0 is slower than 1.x

**These must be fixed before 2.0 ships.** Combine join, Combine cut and Split are slower in 2.0
than in 1.x. Import and Repair are already faster.

## Cause

The boolean library we use (manifold-rust 0.19.2) tidies the whole mesh after every boolean
operation: it triangulates, simplifies and sorts all of the triangles again. On the 500,000
triangle test mesh that costs more than the operation itself, even when the second body is a
small cube. 1.x uses the C++ original, which only touches the part that changed.

A separate kernel-performance task will patch a fork of manifold-rust so that this clean-up is
skipped for the parts of the mesh that did not change.

## Bench results

Measured on 2026-11-10 with `cargo run --release -p xtask -- bench`, using
`dev/bench/test_mesh.stl`.

| Operation | 1.x s | 2.0 s | Ratio |
|---|---|---|---|
| Import (repair on) | 1.545 | 1.015 | 0.66 |
| Combine join, 30 mm cube | 0.135 | 0.389 | 2.88 |
| Combine cut, 30 mm cube | 0.160 | 0.312 | 1.95 |
| Split at z = 0 | 0.131 | 0.290 | 2.21 |
| Repair quick, 1% triangles missing | 4.146 | 1.628 | 0.39 |

A ratio above 1 means 2.0 is slower. The bench is strict on purpose: it prints SLOWER for
those rows and exits with an error, and it stays that way until the gap is closed.
