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

Rust workspace (`rust/`, the 2.0 rewrite), run from `rust/`: `cargo fmt --all` and
`cargo clippy --workspace --all-targets -- -D warnings` before saying a Rust change is done
(CI's "Format and lints" job fails on either). Settings: `rust/rustfmt.toml` and
`[workspace.lints]` in `rust/Cargo.toml`; `rust/third_party` is vendored and left alone.
Rust tests: `cargo nextest run --workspace --release` (about a minute once built; install once
with `cargo install cargo-nextest --locked`; settings in `rust/.config/nextest.toml`). CI runs
them with `--cargo-profile test-fast` in place of `--release`: optimised but no fat LTO, so the
120 test programs build in minutes (profile in `rust/Cargo.toml`).
`cargo test` still works. After adding or updating a crate: `cargo deny check` from `rust/`
(advisories and licences; CI's "Dependencies" job; install once with
`cargo install cargo-deny --locked`; settings and exceptions in `rust/deny.toml`).
Speed: before a change that could slow a tool, `cargo run --release -p xtask -- bench --skip-1x --save`;
after it, the same with `--compare` (fails on a row over 20% and 30 ms slower than the saved run;
run both with nothing else building). Without `--skip-1x` it also times 1.x and fails where 2.0
is slower. Not on CI: the runners' timings are too noisy. Tests that hold a frame to a time budget run alone under nextest: give
a new one a name the filter in that file matches.

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

### Releasing 2.0 (only when asked)

2.0 has its own workflow, **Release 2.0** (`.github/workflows/release-2.yml`); `release.yml` only
answers `v1.*` tags. Set `version` in `rust/Cargo.toml` (`[workspace.package]`), write
`.github/release-notes/v<version>.md`, commit, then tag `v<version>` and push the commit and the
tag. The tag push starts the Rust workflow too; Release 2.0 waits for that run to pass (an hour
or so) and publishes the files it built: `STL-Editor-2-Windows.exe` (keep that name: the 2.0
updater looks for it), `STL-Editor-2-macOS.app.zip` and `STL-Editor-2-x86_64.AppImage`. A tag
with a suffix (`v2.0.0-beta.1`) is a pre-release. The release is not marked "Latest" while
`MAKE_LATEST` in that file is `"false"`, so the website's download button and 1.x installs stay
on 1.x; change it together with the website. If a flaky test fails the Rust run, re-run its
failed jobs, then the failed Release 2.0 job.

The 2.0 web edition is a preview at https://natterman12.github.io/STL-Editor/app2/, published by
**Web preview (2.0)** (`.github/workflows/web-preview.yml`, the same file on `main` and here). It
commits the web build of a green Rust run to `docs/app2` on `main`: by hand
(`gh workflow run web-preview.yml`, the newest green run of `v2-main`) and on each `v2.*` tag.
About 8 MB of repository per changed build, so not on every push. Never edit `docs/app2` by
hand, and only publish when Nathan asks. The page is marked noindex; `/app2/build.json` names
the commit. `/app/` stays 1.x until the release moves it.

## Ideas not built yet

- Sketches tied to part edges follow a moved edge only when the sketch is next opened, not when
  a parameter change moves it.
- Fillet / chamfer on mesh edges; sketch spline and ellipse; colour 3MF; code signing.
- Port the web edition's isotropic remesher to the desktop (too slow in pure Python).
