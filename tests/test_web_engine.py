"""Runs the web edition's JavaScript engine tests (tests/web_engine.test.mjs) under Node, and
checks the published copy of the web app (docs/app) matches web/."""
import hashlib
import os
import re
import shutil
import subprocess

import pytest

ROOT = os.path.join(os.path.dirname(__file__), "..")


@pytest.mark.skipif(shutil.which("node") is None, reason="Node.js is not installed")
def test_web_engine_in_node():
    r = subprocess.run(["node", "--test", os.path.join("tests", "web_engine.test.mjs")], cwd=ROOT,
                       capture_output=True, text=True, timeout=600)
    assert r.returncode == 0, r.stdout[-4000:] + r.stderr[-2000:]


def test_web_version_matches_app_version():
    init = open(os.path.join(ROOT, "stleditor", "__init__.py"), encoding="utf-8").read()
    routes = open(os.path.join(ROOT, "web", "engine", "routes.js"), encoding="utf-8").read()
    v = re.search(r'__version__ = "(.+?)"', init).group(1)
    w = re.search(r"VERSION = '(.+?)'", routes).group(1)
    assert v == w, f"web/engine/routes.js says {w}, stleditor says {v}"


def _tree(base):
    out = {}
    for d, _, files in os.walk(base):
        for f in files:
            p = os.path.join(d, f)
            out[os.path.relpath(p, base).replace(os.sep, "/")] = hashlib.md5(open(p, "rb").read()).hexdigest()
    return out


@pytest.mark.skipif(not os.path.isdir(os.path.join(ROOT, "docs", "app")), reason="no published web app")
def test_published_web_app_is_current():
    """docs/app is what GitHub Pages serves: run packaging/build_web.py after changing web/."""
    src, dst = _tree(os.path.join(ROOT, "web")), _tree(os.path.join(ROOT, "docs", "app"))
    stale = sorted(k for k in src if dst.get(k) != src[k])
    assert not stale, "docs/app is out of date (run python packaging/build_web.py): " + ", ".join(stale[:10])
