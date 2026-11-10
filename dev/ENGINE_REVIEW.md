# Engine review: one JS engine for desktop and web?

Status: review, second run. No product code changed. Checks: static reads of both engines, the
test suites run once each, and the same heavy operations timed on both engines (Node for the
JS side, so browser numbers will differ). Every claim below was checked against the code.

## Recommendation

Do not move the desktop app to the JS engine yet. Keep `stleditor/` as the desktop engine and
`web/engine/` as the web engine, and close the parity gaps in the table.

Reasons:

- The JS engine is slower on every timed op: 1.1× to 5× (table below). Boolean cut and sculpt are
  about 4× slower, and remesh about 4.5× (6.8 s vs 1.5 s).
- Desktop-only features with no JS equivalent: To STEP via SolidCraft (true B-rep; the web
  version writes faceted STEP), Send to Slicer, the updater, native file dialogs, system fonts.
- Feature parity is already good: the same 37 evaluator types exist on both sides.

Do the parity fixes first (they are small), then decide with real use data whether the web
engine is fast enough to be the only engine.

## Feature parity

Evaluator keys: 37 on each side (`EVALUATORS`, `stleditor/features.py:1712`,
`web/engine/features.js:1257`). None is on one side only. `DESKTOP_ONLY_TYPES` is `[]` on the JS
side (`web/engine/features.js:1265`). Desktop-only UI tools are `DESKTOP_TOOLS = {to_step, send}`
(`web/js/app.js:18`).

| Area | Python | JS | Same? | Difference |
|---|---|---|---|---|
| import | `features.py:208-213` (`local_repair`, then `quick_repair`) | `features.js:134-138` (`localRepair` only) | No | JS skips `quick_repair`: meshes that local repair can't fix are salvaged less often. Warning text differs (`features.py:228-230` vs `features.js:147`). |
| hollow / shell / lattice pitch | `features.py:958-962` (bbox from `b.mesh()`) | `features.js:825-828` (`solid.boundingBox()`) | No | Walls and struts can differ slightly. |
| voxel size cap | `geometry.py:414` (300e6 voxels) | `voxel.js:8,17` (`MAX_VOXELS` 40e6) | No | Web fails on grids 7.5× smaller. Hollow/voxel on big parts errors out in the browser first. |
| simplify | `geometry.py:599-624` via `features.py:1084-1095` (`fast_simplification`) | `features.js:911-939` + `decimate.js` (port of same quadric idea) | No | Different triangle sets; same target counts (checked: 49,856 both at ratio 0.1). |
| sculpt | `printing.py:264-307` (scipy `cKDTree`, Laplacian) | `meshedit.js:106+` (`PointGrid`) | No | Small numeric mesh differences. |
| remesh | `printing.py:310-326` (refine + simplify) | `meshedit.js:401+` (isotropic split/collapse/flip) | No | Intentionally different algorithms; 26,118 vs 26,394 tris on the test mesh. |
| thin-wall check | `features.py:1665-1670` | `features.js:1228-1230` | Soft | Same scan; number formatting in messages differs. |
| expressions `%`, `^` | `params.py:115-126` (`math.fmod`; rejects complex `^`) | `expr.js:64,68` (JS `%`; no complex rejection) | No | Negative `%` and complex `^` results differ. |
| Shell `^` | `features.py:1003` (CrossSection intersection) | `features.js:856` (`.intersect`) | Yes | — |

### Routes

- Python only (`stleditor/server.py`, `ROUTES` ~809-821): `apps`, `export`, `import`, `open`,
  `open_url`, `quit`, `recovery/list`, `recovery/open`, `recovery/discard`, `restart`, `save`,
  `send_to`, `settings/get`, `settings/set`, `update/status`, `update/install`, `update/apply`.
- JS only (`web/engine/routes.js`): `open_bytes` (157), `save_bytes` (176), `saved` (181),
  `export_bytes` (182), `snapshot` (195).
- Both, registered differently: `state`/`mesh` (Python GET special case, `server.py:209-216`;
  JS `routes.js:65-66`); `upload` (Python POST `server.py:236-240`; JS `routes.js:174`); `fonts`
  (Python `ROUTES:810`; JS `worker.js:55`).
- `to_step`: shared name, different backend (SolidCraft in `server.py:727-757`; faceted
  `step.js` in `routes.js:52-61`).

Feature-API routes (`feature/*`, `preview`, `sketch_op`, `supportfins/*`, `params/set`) match by
name.

## Blockers

None stop the web edition from working today. Items that block a single-engine desktop:

1. **To STEP via SolidCraft** (`server.py:696`, `:727`): Windows-only CLI, true B-rep. The JS
   version writes one planar face per triangle. No JS equivalent of the B-rep result.
2. **Send to Slicer** (`server.py:714`): launches a local program. A browser can't do this.
3. **Updater** (`stleditor/updates.py`, 359 lines): zip swap or `git pull`. Desktop only.
4. **Import repair gap** (`features.js:136`): no `quick_repair` fallback, so some broken meshes
   import on desktop but fail on web.
5. **Voxel cap** (`voxel.js:8`, 40e6 vs `geometry.py:414`, 300e6): hollow and voxel repair on
   large parts fail in the browser.

## Measured timings

Machine: Intel Core i9-10850K, 31.9 GB RAM, Windows 11. A `llama-server` process (9 GB) and
several Edge processes were running; system CPU ~11% before timing. Another worker ran test
suites at the same time, so numbers are noisy.

Method: fresh `Document`, import with `repair=true`, then one timed `add_feature` /
`addFeature`. 3 runs each, median. Python calls `stleditor/` directly; JS runs `web/engine` in
Node (manifold WASM).

Input: trimesh UV sphere, radius 30, 498,576 triangles, binary STL. Both sides: 498,576 tris,
249,290 vertices, bbox ±29.9995 (x, y), ±30 (z).

| Op | Python median s | JS (Node) median s | JS/Py | Params | Notes |
|---|---|---|---|---|---|
| Import (weld/repair) | 1.26 | 1.51 | 1.2× | repair on | |
| Cut: cube | 0.09 | 0.34 | 3.8× | combine cut, 20 mm cube | 487,168 tris both |
| Cut: sphere | 0.11 | 0.56 | 5.1× | combine cut, same-size sphere | 503,908 tris both |
| Hollow | 2.27 | 2.11 | 0.93× | thickness 2, normal resolution | 507,660 vs 508,444 tris |
| Simplify | 0.51 | 0.98 | 1.9× | ratio 0.1 | 49,856 tris both |
| Remesh | 1.50 | 6.82 | 4.5× | edge 1.0 | Different algorithms; not a pure engine comparison |
| Sculpt | 0.29 | 0.97 | 3.3× | one stroke, radius 6, strength 0.8, 15 points | same tri count and bbox |

Not measured: browser Web Worker timings (single-threaded WASM; expect the browser to differ from
Node), and any op not listed. Hollow is about the same on both: the voxel work dominates.

Source of timing scripts: `C:\Users\ggnn1\AppData\Local\Temp\engine-bench` (outside repo).

## Test results

| Suite | Result | Time |
|---|---|---|
| `py -3.14 -m pytest -q tests` | 105 passed, 0 failed | 29.9 s |
| `node --test tests/web_engine.test.mjs` | 54 pass, 0 fail | 12.0 s |

Coverage by word match (a name counted as covered if it appears anywhere in the tests):

- Python evaluators with no test reference: `revolve`, `scale`, `simplify`, `delete`, `pushpull`,
  `sculpt`, `remesh`. Checked: `simplify`, `sculpt`, `remesh` do not appear in `tests/test_engine.py`.
- JS evaluators with no test reference: `revolve`, `delete`, `pushpull`. `scale`, `simplify`,
  `sculpt`, `remesh` exist in JS (`features.js:670`, `:897`, `:905`, `:911`) and have only
  incidental mentions in tests.

## Options

A. **Keep two engines, close the gaps (recommended).** Fix `quick_repair` in JS import, align the
   voxel cap, add tests for untested evaluators on the side that lacks them. Keep SolidCraft,
   slicer and updater on desktop.
B. **JS engine as desktop default, Python as fast path.** Only after a browser benchmark shows the
   JS engine is acceptable on 500k+ meshes. Needs the remesh speed-up first.
C. **Python engine for web via a server.** Breaks the web edition's offline use. Not recommended.

## Effort (rough)

- A: 1–2 days (import fallback, voxel cap, 6–8 tests, doc notes).
- B: 1–2 weeks (remesh port or speed-up, browser benchmark, desktop switch, fallback path).
- Keeping SolidCraft STEP in the browser: not feasible without a B-rep kernel in JS. Expect ongoing
  faceted-STEP limitation.

## Risks

- Remesh on the web takes 6.8 s on a 500k mesh (Node). A browser worker may be slower; users see
  a frozen-looking UI without progress.
- Numeric differences (sculpt, simplify, remesh) mean the same .stlx project can give slightly
  different meshes per engine. Keep parity tests on volume/bbox, not on exact triangles.
- Voxel cap on the web can block a model that works on desktop.
- Frozen build assumption: desktop serves `web/` from the PyInstaller bundle. Inferred from
  `packaging/build_portable.ps1:40`, not verified by a build.

## Verified vs not

- Verified: evaluator key parity, the import/voxel/expression/route differences above, both test
  suite results, all timings in the table.
- Not verified: browser-side timings, exact numeric equality of sculpt/simplify/remesh outputs,
  frozen desktop build.
- Corrected from the earlier worker report: JS does have `simplify`, `sculpt`, `remesh` and
  `scale`; the parity table has no one-sided features.
