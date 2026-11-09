# STL Editor: notes for Claude Code

CAD-style editing for STL meshes (sketch, extrude, cut, holes, threads, split to fit the bed,
breakaway support fins) with an editable timeline. Two editions share one UI:

- **Desktop** (Windows): Python engine in `stleditor/` (manifold3d, trimesh, shapely, scipy...)
  behind a local JSON API (`stleditor/server.py`), shown in pywebview / WebView2. `main.py`
  starts it. Shipped as a portable zip built by PyInstaller.
- **Web edition**: the same UI (`web/`) with a JavaScript port of the engine (`web/engine/`,
  manifold-3d WASM) in a Web Worker. Published from `docs/app/` on GitHub Pages:
  https://natterman12.github.io/STL-Editor/app/

Read `CONTRIBUTING.md` for the code map and how to add a tool. `dev/DESIGN.md` has the
internals of the bigger features (Support Fins, sketch dimensions, parameters, updates, the
web edition's storage); read the relevant section before changing one of them.

## Commands

```powershell
pip install -r requirements.txt pytest       # once
python main.py                               # desktop window (python main.py --browser: in a browser)
python -m pytest -q tests                    # engine tests, both engines (runs the Node tests too)
node --test tests/web_engine.test.mjs        # just the web engine's tests
python packaging/build_web.py                # copy web/ to docs/app and update web/sw.js: after ANY change in web/
python packaging/smoke_test.py main.py       # every engine tool once through the HTTP API
cd tests/ui; npm install; npx playwright install chromium   # once, for the UI tests
node tests/ui/run.js [web|desktop|<name>...]  # UI tests (Playwright); see tests/ui/README.md
node tests/ui/serve.js                       # web edition locally: http://127.0.0.1:8790/?web
```

Before saying a change is done: `python packaging/build_web.py`, `python -m pytest -q tests`,
and the UI tests that cover what you touched (`node tests/ui/run.js <names>`). The pytest run
fails if `docs/app` or `web/sw.js` is out of date. Don't serve `web/` with
`python -m http.server` on Windows: it can send `.js` as text/plain, and the UI's modules then
don't load (`serve.js` sends the right types).

## Rules that are easy to break

- **Both engines.** Every feature exists in `stleditor/features.py` *and*
  `web/engine/features.js`, with the same params, the same route names (`server.py` /
  `web/engine/routes.js`) and the same results. Engine tests check volumes you can work out by
  hand, in both (`tests/test_engine.py`, `tests/web_engine.test.mjs`). Only Send to Slicer and
  To STEP are desktop-only (`DESKTOP_TOOLS` in app.js).
- **Version parity**: `__version__` in `stleditor/__init__.py` must equal `VERSION` in
  `web/engine/routes.js` (a test checks).
- **Never edit `docs/app/`** (generated) or the `FILES` / `BUILD` lines of `web/sw.js`.
- **Never edit `web/vendor/support-fins/`** by hand: it's Support Fins' own engine, bundled
  unmodified by `packaging/update_support_fins.py`. A daily workflow
  (`.github/workflows/support-fins.yml`) updates it and pushes. Its tool panels are built from
  the engine's options schema, so don't hard-code its settings; only values that differ from
  the defaults are saved in projects.
- **Millimetres** everywhere in the engines; inch mode only changes what the UI shows (`U` in
  `web/js/units.js`).
- Feature evaluators must be **deterministic** (they re-run when an earlier step changes).
  Raise `GeometryError("plain-English message")` for anything the user can fix; it is shown
  as-is.
- **Parameters**: any number in a feature's params can be a string `"=width/2"`. It's resolved
  in runFeature (`web/js/expr.js`, `stleditor/params.py`, same rules) and the cache hash
  includes the parameter values. Keep the two evaluators in step.
- **UI**: plain ES modules, no bundler, no build step. CI runs `node --check` on every
  `web/js/*.js`. `web/js` is shared by both editions; `web/engine` is web-only and is removed
  from the desktop build (`packaging/build_portable.ps1`).
- **Toolbar** (`ribbonDef()` in app.js): each tool appears on exactly one tab (MODEL, key
  `solid`; PRINT; SUPPORTS; MESH); Inspect is the pinned group at the right of every tab. The
  tool search (Ctrl+K) finds everything.
- **CRLF**: CI checks out on Windows with autocrlf. Anything that hashes or byte-compares files
  must normalise CRLF; `web/vendor/**` and `docs/app/vendor/**` are `-text` in
  `.gitattributes`.
- Don't bump the version or tag unless asked to release.

## Writing for users

The app's text, the guide, the README, the website (`docs/index.html`) and release notes are
for people who print things, not programmers: plain words, short sentences, say what a
setting does and when you'd change it. When a feature changes, update `USER_GUIDE.md` (and the
README / website lines that mention it). The Ko-fi link should stay noticeable but low-key, never
ad-like.

## Releasing (only when asked)

1. Bump `__version__` and routes.js `VERSION`, run `python packaging/build_web.py`, run the
   tests, and write the notes to `.github/release-notes/v<version>.md` (the Release workflow
   uses that file; plain-English sections like the earlier releases). Update the version and
   size on the two download buttons in `docs/index.html` (hero and Get started), and the Mac and
   Linux box if those builds changed.
2. Commit, then `git tag v<version>`, `git push`, `git push origin v<version>`.
3. The **Release** workflow tests, builds, smoke-tests and publishes
   `STL-Editor-Windows-portable.zip` (keep that asset name: the website links to
   `releases/latest/download/STL-Editor-Windows-portable.zip`). About 3–4 minutes. Pushing
   `docs/app` also redeploys the web edition (pages-build-deployment, 1–2 minutes).
4. Check: `gh run list --limit 3` (Release, Tests and Pages all green),
   `gh release view v<version>`, and that the live site serves the new version
   (`https://natterman12.github.io/STL-Editor/app/engine/routes.js` contains the new `VERSION`).
5. If a Pages deploy hangs ("Getting Pages deployment status... Timeout"), wait for the run to
   end, then `gh api -X POST repos/natterman12/STL-Editor/pages/builds`. A failed Release run
   publishes nothing, so deleting and re-pushing the tag is safe.

Installed copies find the release through the update check; "Update now" downloads and swaps
the portable build, or `git pull`s a git checkout (`stleditor/updates.py`).

## Ideas not built yet

- Support Fins: a "Fill gaps" button that adds walls under bare spots; click a bare spot to drop
  a fin there; a warning before export when overhangs are bare (the coverage check,
  `web/js/sfcoverage.js`, already finds them).
- Sketches tied to part edges follow a moved edge only when the sketch is next opened, not when
  a parameter change moves it.
- Fillet / chamfer on mesh edges; sketch fillet, spline and ellipse; SVG / DXF import; loft,
  sweep; plate layout and colour 3MF; code signing.
- Port the web edition's isotropic remesher to the desktop (too slow in pure Python).
