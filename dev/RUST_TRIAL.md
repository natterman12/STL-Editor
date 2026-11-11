# Rust remesh trial

Experiment only. Nothing is wired into the app, nothing is committed, and no app behaviour changed.

## Question

1. What are the real browser timings of the web engine?
2. Does a Rust/WebAssembly remesh close the gap?

## Test mesh

`dev/bench/make_test_mesh.mjs` builds `dev/bench/test_mesh.stl` with the web engine: a sphere of radius 20 (1130 segments) with a cube hole and a slab cut. It has 503,804 triangles. Every benchmark below uses this file.

## 1. Browser timings

`dev/bench/browser_bench.mjs` serves `web/` locally and runs `index.html?web` in headless Chrome (puppeteer-core, `CHROME_PATH`). Each operation was timed through the app's own call, three runs from a fresh import, median reported. The full table is in `dev/BROWSER_TIMINGS.md`.

| Operation | Browser median (s) |
|---|---|
| Import | 1.805 |
| Cut: cube | 0.494 |
| Cut: sphere | 1.288 |
| Hollow | 3.943 |
| Simplify (ratio 0.1) | 0.909 |
| Remesh (edge 1.0) | 16.655 |
| Sculpt (one stroke) | 0.921 |

A second full run gave the same picture: remesh 16.6 to 17.4 s, the others within about 10 percent.

Caveat on the Node and Python columns in `ENGINE_REVIEW.md`: they were measured on a different input (a 498,576-triangle UV sphere, radius 30). Only browser versus Node on the same mesh is a fair comparison. The Node remesh run on `test_mesh.stl` (below) took about 16.0 s, so the browser is roughly the same speed as Node on this mesh, not slower by 2.4 times as the old table suggested.

## 2. JS versus Rust remesh

The trial crate `rust/remesh` (since moved into `rust/crates/stl-mesh/src/remesh.rs` as `remesh_core`, and deleted in 2.0 batch 3) ported the isotropic remesh from `web/engine/meshedit.js` (split, collapse, flip, smooth) to Rust with wasm-bindgen. The trial's bench script (since deleted) refined the test mesh to edge 1.0 with the kernel, then ran both remeshes on the same arrays, three runs each.

| | JS (Node) | Rust (wasm) |
|---|---|---|
| Median time | 15.96 s | 2.31 s |
| Median time, with kernel steps | 15.96 s | 2.36 s |
| Output triangles | 17,876 | 17,876 |
| Edge min / mean / max (mm) | 0.0006 / 0.9961 / 1.7332 | 0.0006 / 0.9961 / 1.7332 |
| Watertight | yes | yes |
| Accepted by the Manifold kernel | n/a | yes |

Speed-up: 6.9 times. The quality gate passed: triangle counts are identical (0 percent difference, limit 5 percent), and the Rust output is watertight.

Differences from the JS code:
- Positions go in and out as Float32, the kernel's format. Maths is 64-bit.
- `sqrt` replaces `Math.hypot`, so last-bit rounding can differ on other meshes.
- The "more than 3 million triangles" guard is not ported. The caller must keep it.
- The Rust side rejects a face that points at a missing vertex.
- The JS remesh is called with a stand-in solid whose `refineToLength()` returns the refined mesh, so the JS timing excludes the refine step on both sides.

Build (the trial crate, now deleted): `wasm-pack build --target web` in `rust/remesh`. The `.wasm` is 50,039 bytes. Rust unit tests: 2 passed (`cargo test --release`).

## 3. Checks

- `python -m pytest -q tests` (Python 3.14): 105 passed. Nothing in the app changed.

## Recommendation

- Remesh is the one operation where JS logic dominates. Porting it to Rust gives about 7 times speed-up and identical output on this mesh. That is worth it if slow remesh is a user complaint.
- The other operations are mostly kernel time (cuts, simplify) or voxel work (hollow, 3.9 s). Porting simplify, sculpt or the voxel tools to Rust would only help the JS parts. Profile them first; do not port blind.
- Cost of a Rust remesh in the product: a prebuilt `.wasm` artifact committed or built in CI, a build step (conflicts with the "no build step" rule for `web/js`), a second implementation to keep in parity with the Python `remesh`, and parity tests for both.
- Suggested next step, if Nathan wants it: profile hollow, simplify and sculpt on `test_mesh.stl`. If remesh stays the only real pain point, port remesh alone and keep the JS version as fallback.

## 4. Desktop (Python) remesh on the same mesh

Added later. `dev/bench/remesh_py_bench.py` times `stleditor.printing.remesh(solid, 1.0)`, the function the desktop Remesh feature calls, on `test_mesh.stl` (3 runs, median).

| | Desktop (Python) | Web JS (browser) | Web JS (Node) | Rust (wasm, Node) |
|---|---|---|---|---|
| Median time | 9.28 s | 16.58 s | 15.72 s | 2.27 s |
| Output triangles | 243,912 | 17,876 | 17,876 | 17,876 |
| Edge min / mean / max (mm) | 0.0 / 0.68 / 22.24 | 0.0006 / 0.996 / 1.73 | same | same |
| Watertight | yes | yes | yes | yes |

The desktop remesh does not reach the requested edge length on this mesh. It refines to 755,558 triangles (1.0 s), then asks `fast_simplification` for 17,236 triangles (surface area 7,463 mm² / 0.433), but the simplifier stops at 243,912 after 8.2 s. The result has slivers up to 22 mm and near-zero edges. On the UV-sphere input used in `ENGINE_REVIEW.md` the two engines agreed (26,118 vs 26,394), so the problem depends on the mesh. Why the simplifier stops early is not yet known.

So on this mesh the Rust port is about 4x faster than the desktop app and gives the expected result, while the desktop gives a different, poorer mesh. A shared Rust remesh (wasm for the web, a Python binding for desktop) would fix speed and parity together.
