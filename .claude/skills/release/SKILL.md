---
name: release
description: Publish a new STL Editor version (desktop zip on GitHub Releases plus the web edition). Use when asked to release, publish, ship or tag a version.
---

# Release STL Editor

1. Pick the version: minor (1.13.0 → 1.14.0) for new features, patch (1.13.0 → 1.13.1) for
   fixes only. Check `git status` is clean apart from the work being released, and that you're
   on `main`.
2. Set `__version__` in `stleditor/__init__.py` and `VERSION` in `web/engine/routes.js` to it.
3. Write `.github/release-notes/v<version>.md`: plain English for people who print things, a
   `##` heading per change, short bullets saying what they can now do (see the earlier files
   there for the tone). No internal names.
4. Run, and fix anything that fails:
   - `python packaging/build_web.py`
   - `python -m pytest -q tests`
   - `python packaging/smoke_test.py main.py` (must print `SMOKE OK`)
   - `node tests/ui/run.js <the tests covering what changed>`
5. Make sure `USER_GUIDE.md` (and the README / `docs/index.html` lines that mention it) describe
   the change.
6. Commit everything (`STL Editor <version>: <what changed>`), then
   `git tag v<version>`, `git push`, `git push origin v<version>`.
7. Watch `gh run list --limit 3` until Release, Tests and pages-build-deployment are all green
   (about 3–4 minutes). If Pages hangs on "Getting Pages deployment status", wait for it to end,
   then `gh api -X POST repos/natterman12/STL-Editor/pages/builds`. If Release fails, fix it,
   delete the tag locally and on GitHub, and tag again (a failed run publishes nothing).
8. Check it's live: `gh release view v<version>` lists `STL-Editor-Windows-portable.zip`, and
   `https://natterman12.github.io/STL-Editor/app/engine/routes.js` has the new `VERSION`.
9. Tell Nathan in a few lines what's in it and that installed copies will offer the update.
