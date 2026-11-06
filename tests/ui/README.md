# UI tests

Playwright scripts that drive the real UI: the web edition, and the desktop app's engine
through the same page. They click the toolbar, draw in sketches, drag handles and read back
the project's state, so they catch what the engine tests can't.

## Setup (once)

```powershell
cd tests/ui
npm install
npx playwright install chromium
```

Needs Node 20 or newer and the Python requirements (the desktop tests start `main.py`).

## Running

```powershell
node tests/ui/run.js            # everything (about 15 minutes)
node tests/ui/run.js web        # the web edition only
node tests/ui/run.js desktop    # the desktop engine only
node tests/ui/run.js skdim sfcov   # just these
```

`run.js` starts what it needs and stops it afterwards: `web/` served on port 8790 and `docs/`
on 8791 (the published copy, for the offline / update test), both by `serve.js`, and
`python main.py --server` on 8765. If one of them is already running, it's used as it is. Set
`PYTHON` to the Python that has the app's packages if `python` (Windows) / `python3` isn't it. `WEB_URL` / `DESKTOP_URL` point the tests elsewhere, e.g.
`WEB_URL=https://natterman12.github.io/STL-Editor/app/?web` for the live site.

A test fails when it crashes, times out or the page reports errors. Each one also prints what it
saw (sizes, volumes, timeline steps, messages); the log and screenshots are in `tests/ui/out/`.
When a test fails, read its log, and look at the screenshots to see the state of the page.

The tests list and what each covers is at the top of `run.js`. Run a single file directly with
`node tests/ui/<file>.js` (`BASE=http://127.0.0.1:8765/` runs a web test against the desktop
engine; the server has to be running then).

## Writing one

- `require('./common')` gives `FIX` (the test parts in `fixtures/`), `OUT` (screenshots and saved
  files: never write into `fixtures/`), `WEB` and `DESKTOP` (the two URLs).
- Start from an empty project (`post('new', { force: true })` through `/js/api.js`) so a test
  doesn't depend on the one before it.
- Click through the UI the way a person would (the tab, then the tool) where that's what's being
  tested; call `window.app` directly for setup.
- Print the values that show it worked, and `ERRORS` plus the messages if the page logged any
  (see any test for the pattern). Add it to the list in `run.js`.
- Headless Chromium uses SwiftShader (software WebGL), so allow time for renders.
