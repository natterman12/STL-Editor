"""Publishes the web edition: copies web/ to docs/app/, which GitHub Pages serves at
https://natterman12.github.io/STL-Editor/app/

    python packaging/build_web.py

The web edition is the same page as the desktop app; it runs the JavaScript engine in
web/engine (manifold-3d WebAssembly) instead of the Python one. Run this after changing
anything in web/ — tests/test_web_engine.py fails while docs/app is out of date.
"""
import os
import shutil
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
SRC = os.path.join(ROOT, "web")
DST = os.path.join(ROOT, "docs", "app")
SKIP_DIRS = {"__pycache__"}
SKIP_FILES = {".DS_Store", "Thumbs.db"}


def main():
    if os.path.isdir(DST):
        shutil.rmtree(DST)
    n = size = 0
    for d, dirs, files in os.walk(SRC):
        dirs[:] = [x for x in dirs if x not in SKIP_DIRS]
        rel = os.path.relpath(d, SRC)
        out = os.path.normpath(os.path.join(DST, rel))
        os.makedirs(out, exist_ok=True)
        for f in files:
            if f in SKIP_FILES:
                continue
            shutil.copy2(os.path.join(d, f), os.path.join(out, f))
            n += 1
            size += os.path.getsize(os.path.join(d, f))
    # GitHub Pages: serve files as they are (no Jekyll processing of folders like _shared)
    open(os.path.join(ROOT, "docs", ".nojekyll"), "w").close()
    print(f"Published {n} files ({size / 1e6:.1f} MB) to docs/app")


if __name__ == "__main__":
    sys.exit(main())
