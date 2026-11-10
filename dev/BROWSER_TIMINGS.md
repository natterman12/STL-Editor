# Browser engine timings

These numbers come from running `dev/bench/browser_bench.mjs` in headless Chrome against the local `web/` tree (`index.html?web`). The mesh is `dev/bench/test_mesh.stl` (about 504k triangles; this run imported 503,804). Each operation was timed with `performance.now()` around the app’s own `upload` or `feature/add` call, three independent runs from a fresh import, and the median is reported.

Node and Python columns are the medians from the table in `dev/ENGINE_REVIEW.md` (same kinds of ops; that review used a similar ~500k sphere mesh in Node and in `stleditor/`). Remesh is a different algorithm on the JS side than on Python, so that row is not a pure like-for-like engine comparison.

| operation | browser median (s) | Node median (s) | Python median (s) |
|---|---|---|---|
| Import (weld/repair) | 1.805 | 1.51 | 1.26 |
| Cut: cube | 0.494 | 0.34 | 0.09 |
| Cut: sphere | 1.288 | 0.56 | 0.11 |
| Hollow | 3.943 | 2.11 | 2.27 |
| Simplify (ratio 0.1) | 0.909 | 0.98 | 0.51 |
| Remesh (edge 1.0) | 16.655 | 6.82 | 1.50 |
| Sculpt (one stroke) | 0.921 | 0.97 | 0.29 |

On this machine the browser was slower than Node on cut, hollow, and especially remesh (about 16.7 s median for remesh on this mesh). Simplify and sculpt were close to the Node medians. Python stayed fastest on every timed op except hollow, where Node had been slightly ahead in the earlier review.
