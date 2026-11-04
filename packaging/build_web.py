"""Publishes the web edition: copies web/ to docs/app/, which GitHub Pages serves at
https://natterman12.github.io/STL-Editor/app/

    python packaging/build_web.py

The web edition is the same page as the desktop app; it runs the JavaScript engine in
web/engine (manifold-3d WebAssembly) instead of the Python one. Run this after changing
anything in web/ — tests/test_web_engine.py fails while docs/app is out of date.
"""
import hashlib
import json
import os
import re
import shutil
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
SRC = os.path.join(ROOT, "web")
DST = os.path.join(ROOT, "docs", "app")
SKIP_DIRS = {"__pycache__"}
SKIP_FILES = {".DS_Store", "Thumbs.db"}


# not cached for offline use: the service worker itself, and files only the desktop build uses
SW_SKIP = {"sw.js"}


def app_files():
    """Every file of the web app, as URLs relative to web/ (what the service worker caches)."""
    out = []
    for d, dirs, files in os.walk(SRC):
        dirs[:] = sorted(x for x in dirs if x not in SKIP_DIRS)
        for f in sorted(files):
            rel = os.path.relpath(os.path.join(d, f), SRC).replace(os.sep, "/")
            if f not in SKIP_FILES and rel not in SW_SKIP:
                out.append(rel)
    return out


def sw_build_hash():
    h = hashlib.sha256()
    for rel in app_files():
        h.update(rel.encode())
        with open(os.path.join(SRC, rel), "rb") as fh:
            h.update(fh.read().replace(b"\r\n", b"\n"))   # same on Windows checkouts
    return h.hexdigest()[:12]


def update_service_worker():
    """Write the file list and a hash of the app's contents into web/sw.js, so every change
    to the app makes browsers fetch the new version."""
    p = os.path.join(SRC, "sw.js")
    with open(p, encoding="utf-8") as fh:
        s = fh.read()
    files = json.dumps(app_files(), indent=0).replace("\n", "")
    s2 = re.sub(r"const BUILD = '[^']*';", f"const BUILD = '{sw_build_hash()}';", s, count=1)
    s2 = re.sub(r"const FILES = \[.*?\];", lambda m: f"const FILES = {files};", s2, count=1, flags=re.S)
    if s2 != s:
        with open(p, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(s2)


def main():
    update_service_worker()
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
