"""Support Fins (https://github.com/gittrahan/support-fins, MIT) in the desktop app.

Support Fins adds the breakaway support fins that let a part print tipped over, in its
strong orientation. This runs Support Fins' own engine -- the same bundle the web edition
uses, web/vendor/support-fins/engine.iife.js, built from their release by
packaging/update_support_fins.py -- inside an embedded V8 (mini-racer), the way Support
Fins' Cura, FreeCAD and Blender plugins do.

Updates: at start-up (at most once a day, if update checks are on) the app reads the
VERSION.json published with the web edition. When it names a newer Support Fins release
with the same integration API, the engine is downloaded, checked against its SHA-256 and
kept in %APPDATA%\\STL Editor\\support-fins; it is used from the next start, so a session
never changes engine half-way.
"""
from __future__ import annotations

import atexit
import base64
import hashlib
import json
import os
import re
import threading
import time
import urllib.request

import numpy as np

from . import __version__, settings
from .geometry import GeometryError, manifold_from_arrays, weld

API = 1   # the integration contract (see packaging/update_support_fins.py)
BUNDLED = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "web", "vendor", "support-fins")
USER_DIR = os.path.join(os.environ.get("APPDATA") or os.path.expanduser("~/.config"), "STL Editor", "support-fins")

_lock = threading.RLock()
_ctx = None
_active = None
STATE = {"pending": None, "error": None, "checked": None}


def _semver(tag):
    m = re.fullmatch(r"v?(\d+)\.(\d+)\.(\d+)", str(tag or ""))
    return tuple(int(x) for x in m.groups()) if m else (0, 0, 0)


def _read_info(folder):
    try:
        with open(os.path.join(folder, "VERSION.json"), encoding="utf-8") as fh:
            info = json.load(fh)
        path = os.path.join(folder, "engine.iife.js")
        with open(path, "rb") as fh:
            data = fh.read()
        if info.get("api") != API or hashlib.sha256(data).hexdigest() != info.get("sha256", {}).get("engine.iife.js"):
            return None
        return {**info, "path": path, "folder": folder}
    except (OSError, ValueError):
        return None


def _choose():
    """The newest valid engine: the one shipped with the app, or a downloaded update."""
    found = [i for i in (_read_info(BUNDLED), _read_info(USER_DIR)) if i]
    if not found:
        raise GeometryError("The Support Fins engine is missing from this installation.")
    return max(found, key=lambda i: _semver(i.get("tag")))


def _engine():
    global _ctx, _active
    with _lock:
        if _ctx is None:
            try:
                from py_mini_racer import MiniRacer
            except ImportError as e:
                raise GeometryError("Support Fins needs the mini-racer package (pip install mini-racer).") from e
            info = _choose()
            ctx = MiniRacer()
            with open(info["path"], encoding="utf-8") as fh:
                ctx.eval(fh.read())
            atexit.register(ctx.close)
            _ctx, _active = ctx, info
        return _ctx


def _call(name, *args):
    from py_mini_racer import JSEvalException
    ctx = _engine()
    with _lock:
        try:
            return ctx.call(f"SupportFinsEngine.{name}", *args)
        except JSEvalException as e:
            msg = str(e).strip().splitlines()
            raise GeometryError(f"Support Fins: {msg[0] if msg else 'error'}") from None


def info():
    schema = json.loads(_call("optionsSchemaJson"))
    a = _active
    return {"schema": schema, "version": a.get("version"), "tag": a.get("tag"), "commit": a.get("commit"),
            "source": a.get("source"), "pending": STATE.get("pending")}


def version():
    _engine()
    return _active.get("version")


def _options(values):
    return _call("optionsFromDialogJson", json.dumps(values or {}))


def _soup(verts, faces):
    v = np.asarray(verts, np.float64)
    f = np.asarray(faces, np.int64)
    return base64.b64encode(np.ascontiguousarray(v[f].reshape(-1), dtype="<f8").tobytes()).decode("ascii")


def _solids(tri, offset, ranges):
    """Fin triangles (seated frame) -> one closed solid per piece, in the part's frame."""
    import trimesh
    shift = np.array([offset["x"], offset["y"], offset["z"]], np.float64)
    out, skipped = [], 0
    for rs in ranges:
        t = np.concatenate([tri[a:b] for a, b in rs]) if rs else np.zeros((0, 3, 3))
        if not len(t):
            continue
        v = t.reshape(-1, 3).astype(np.float64) - shift
        f = np.arange(len(v)).reshape(-1, 3)
        v, f = weld(v, f, tol=1e-5)
        m = manifold_from_arrays(v, f)
        if m is None:
            parts = []
            for sh in trimesh.Trimesh(v, f, process=False).split(only_watertight=False):
                pm = manifold_from_arrays(sh.vertices, sh.faces)
                if pm is not None:
                    parts.append(pm)
            if parts:
                import manifold3d as m3
                m = m3.Manifold.batch_boolean(parts, m3.OpType.Add)
        if m is not None and not m.is_empty():
            out.append(m)
        else:
            skipped += 1
    return out, skipped


def fins(verts, faces, values):
    """Support Fins under a posed body (z up, as it will print)."""
    opts = _options(values)
    r = json.loads(_call("computeFinsB64", _soup(verts, faces), opts))
    tri = np.frombuffer(base64.b64decode(r["triangles"]), dtype="<f4").reshape(-1, 3, 3)
    solids, skipped = _solids(tri, r["offset"], [p["ranges"] for p in r["pieces"]])
    return {"solids": solids, "skipped": skipped, "stats": r["stats"], "report": _call("reportLine", r["stats"]),
            "over": r["overFaces"], "small": r["smallFaces"], "version": _active.get("version")}


def wall(verts, faces, a, b, values):
    """One hand-drawn wall between two points on the body (Support Fins' Draw mode)."""
    opts = _options(values)
    r = json.loads(_call("drawWallB64", _soup(verts, faces), json.dumps(list(map(float, a))),
                         json.dumps(list(map(float, b))), opts))
    if not r.get("ok"):
        raise GeometryError(r.get("reason") or "Support Fins couldn't place a wall there.")
    tri = np.frombuffer(base64.b64decode(r["triangles"]), dtype="<f4").reshape(-1, 3, 3)
    solids, _ = _solids(tri, r["offset"], [[[0, len(tri)]]])
    if not solids:
        raise GeometryError("Support Fins made a wall that isn't a closed solid.")
    return {"solids": solids, "stats": r["stats"], "version": _active.get("version")}


# ------------------------------------------------------------------------------------------
# Engine updates
# ------------------------------------------------------------------------------------------
def published_url(repo=None):
    """Where the web edition publishes its Support Fins engine: the repository's Pages site."""
    from . import updates
    r = (repo or updates.repo() or "").strip("/")
    if "/" not in r:
        return None
    owner, name = r.split("/", 1)
    return f"https://{owner.lower()}.github.io/{name}/app/vendor/support-fins/"


def _get(url, timeout=15):
    req = urllib.request.Request(url, headers={"User-Agent": f"STL-Editor/{__version__}"})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return resp.read()


def check_update(force=False):
    """Download a newer engine if one is published. Returns the new tag or None."""
    base = published_url()
    if not base or not (force or (settings.get("update_check", True) and settings.get("supportfins_update", True))):
        return None
    if not force and settings.get("supportfins_last", 0) > time.time() - 20 * 3600:
        return None
    try:
        info = json.loads(_get(base + "VERSION.json").decode("utf-8"))
        settings.set("supportfins_last", time.time())
        STATE["checked"] = time.time()
        if info.get("api") != API:
            return None   # a newer integration than this app understands: wait for an app update
        have = max((i for i in (_read_info(BUNDLED), _read_info(USER_DIR)) if i), key=lambda i: _semver(i.get("tag")),
                   default=None)
        if have and _semver(info.get("tag")) <= _semver(have.get("tag")):
            return None
        data = _get(base + "engine.iife.js", timeout=60)
        if hashlib.sha256(data).hexdigest() != info.get("sha256", {}).get("engine.iife.js"):
            raise ValueError("checksum mismatch")
        os.makedirs(USER_DIR, exist_ok=True)
        tmp = os.path.join(USER_DIR, "engine.iife.js.tmp")
        with open(tmp, "wb") as fh:
            fh.write(data)
        os.replace(tmp, os.path.join(USER_DIR, "engine.iife.js"))
        with open(os.path.join(USER_DIR, "VERSION.json"), "w", encoding="utf-8") as fh:
            json.dump(info, fh, indent=1)
        STATE.update(pending=info.get("tag"), error=None)
        return info.get("tag")
    except Exception as e:  # never let an update check get in the way
        STATE["error"] = f"{type(e).__name__}: {e}"
        return None


def check_in_background():
    threading.Thread(target=check_update, daemon=True).start()
