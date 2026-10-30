# Contributing to STL Editor

Thanks for helping. Bug reports, model files that break things, ideas and pull requests are all welcome.

## Reporting a problem

Use **[New issue › Bug report](https://github.com/natterman12/STL-Editor/issues/new/choose)**. The form asks for the steps, the version and, most usefully, the model file. Mesh bugs are almost always about one particular file, so a report with the STL usually gets fixed fastest.

## Setting up

You need Windows 10/11 and Python 3.12 or newer.

```powershell
git clone https://github.com/natterman12/STL-Editor.git
cd STL-Editor
pip install -r requirements.txt pytest
python main.py            # desktop window
python main.py --browser  # the same UI in your browser, handy with dev tools
python -m pytest -q tests
```

## Where things live

| Path | What it is |
|---|---|
| `stleditor/features.py` | Every tool, as a function `f_<name>(state, fid, params, result, ctx)`, plus the `EVALUATORS` table |
| `stleditor/document.py` | The timeline: adding / editing / reordering features, cached re-evaluation, undo, `.stlx` files |
| `stleditor/geometry.py` | Mesh I/O, repair, voxel rebuild, hollow, plane frames |
| `stleditor/printing.py` | Fit to bed, connectors, threads, lay flat, sculpt, remesh |
| `stleditor/sketch.py` | Sketch entities → 2D regions |
| `stleditor/server.py` | The local JSON API the UI talks to (`/api/...`) |
| `web/js/tools.js` | The panel for each tool: fields, defaults, hints |
| `web/js/app.js` | Toolbar, browser tree, timeline, menus |
| `web/js/sketcher.js`, `constraints.js` | Sketch mode and the constraint solver |
| `web/js/viewer.js` | three.js viewport |

**Adding a tool** usually means:

1. Write an `f_<name>` evaluator and register it in `EVALUATORS` and `LABELS`.
2. Add a panel definition in `web/js/tools.js`.
3. Add a button in `ribbonDef()` in `web/js/app.js`. Tool search picks it up automatically.
4. Add a test in `tests/test_engine.py` that checks a volume or a bounding box you can work out by hand.

## Guidelines

- The engine always works in **millimetres**. Inch mode only changes what the UI shows.
- Feature evaluators must be **deterministic**. They're re-run whenever an earlier step changes.
- Raise `GeometryError("plain-English message")` for anything the user can fix. The message is shown in the app as-is.
- Keep the UI free of build steps: plain ES modules, no bundler.
- Run `python -m pytest -q tests` before opening a pull request. GitHub runs the tests on every pull request.

## Releasing (maintainer)

1. Bump `__version__` in `stleditor/__init__.py`. Optionally add `.github/release-notes/v<version>.md`.
2. Commit, then tag and push: `git tag v1.2.1 && git push && git push --tags`.
3. The **Release** workflow tests, builds and smoke-tests the portable zip and publishes it as `STL-Editor-Windows-portable.zip`. Installed copies see the update within a day.

## Licence

STL Editor is GPL-3.0. By contributing, you agree your changes are released under the same licence.
