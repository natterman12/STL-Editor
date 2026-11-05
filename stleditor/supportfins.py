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
import collections
import hashlib
import math
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


def _digest(data):
    """SHA-256 of a bundle, ignoring line endings (a Windows git checkout may turn LF into CRLF)."""
    return hashlib.sha256(data.replace(b"\r\n", b"\n")).hexdigest()


def _read_info(folder):
    try:
        with open(os.path.join(folder, "VERSION.json"), encoding="utf-8") as fh:
            info = json.load(fh)
        path = os.path.join(folder, "engine.iife.js")
        with open(path, "rb") as fh:
            data = fh.read()
        if info.get("api") != API or _digest(data) != info.get("sha256", {}).get("engine.iife.js"):
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
                import py_mini_racer
                from py_mini_racer import MiniRacer
            except ImportError as e:
                raise GeometryError("Support Fins needs the mini-racer package (pip install mini-racer).") from e
            # V8 with its background threads (mini-racer's default is single-threaded, which
            # runs Support Fins about 1.7 times slower)
            try:
                py_mini_racer.init_mini_racer(flags=(), ignore_duplicate_init=True)
            except Exception:
                pass
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
            "source": a.get("source"), "pending": STATE.get("pending"), "curves": _has("drawWallThroughB64"),
            "features": features()}


def features():
    """The extras this engine has (see packaging/support_fins_extras): a release that broke one leaves it out."""
    return {"curves": _has("drawWallThroughB64"), "gussets": _has("gussetWallB64"),
            "anchors": _has("anchorFinB64"), "orient": _has("suggestPosesJson")}


def nearest_face(verts, faces, p, n):
    """The triangle nearest `p` whose normal is close to `n` (how a hand-placed anchor finds its face again)."""
    v = np.asarray(verts, np.float64)
    f = np.asarray(faces, np.int64)
    a, b, c = v[f[:, 0]], v[f[:, 1]], v[f[:, 2]]
    nrm = np.cross(b - a, c - a)
    ln = np.linalg.norm(nrm, axis=1)
    nn = np.asarray(n, np.float64)
    nn = nn / (np.linalg.norm(nn) or 1)
    ok = (ln > 0) & ((nrm @ nn) / np.where(ln > 0, ln, 1) >= 0.85)
    if not ok.any():
        return -1
    idx = np.nonzero(ok)[0]
    import trimesh
    tri = np.stack([a[idx], b[idx], c[idx]], axis=1)
    closest = trimesh.triangles.closest_point(tri, np.repeat(np.asarray(p, np.float64)[None], len(idx), axis=0))
    d = np.linalg.norm(closest - np.asarray(p, np.float64), axis=1)
    k = int(np.argmin(d))
    return int(idx[k]) if d[k] < 1.0 else -1


def version():
    _engine()
    return _active.get("version")


def _has(name):
    ctx = _engine()
    with _lock:
        return bool(ctx.eval(f"typeof SupportFinsEngine.{name} === 'function'"))


def _options(values):
    return _call("optionsFromDialogJson", json.dumps(values or {}))


def _soup(verts, faces):
    v = np.asarray(verts, np.float64)
    f = np.asarray(faces, np.int64)
    return base64.b64encode(np.ascontiguousarray(v[f].reshape(-1), dtype="<f8").tobytes()).decode("ascii")


# The engine takes a second or more on a big part, and the same build is asked for more than
# once (the live preview, then OK; each new wall while drawing), so its raw output is kept
# for the last few inputs.
_CACHE = {"fins": collections.OrderedDict(), "wall": collections.OrderedDict()}
_CACHE_MAX = {"fins": 6, "wall": 64}


def _cached(kind, key, fn):
    c = _CACHE[kind]
    with _lock:
        if key in c:
            c.move_to_end(key)
            return c[key]
    r = fn()
    with _lock:
        c[key] = r
        while len(c) > _CACHE_MAX[kind]:
            c.popitem(last=False)
    return r


def _mesh_key(verts, faces):
    h = hashlib.blake2b(digest_size=16)
    h.update(np.ascontiguousarray(verts, dtype=np.float32).tobytes())
    h.update(np.ascontiguousarray(faces, dtype=np.int64).tobytes())
    return h.hexdigest()


def _where(v):
    """Where a piece is: the middle of its box and its size (how a removed fin is found again)."""
    lo, hi = v.min(0), v.max(0)
    return {"c": [round(float(x), 3) for x in (lo + hi) / 2], "size": [round(float(x), 3) for x in hi - lo]}


def match_removed(pieces, removed):
    """Which fins a list of removed ones means in this build (see web/engine/supportfins.js)."""
    gone, missing = set(), 0
    for r in removed or []:
        best, bd = -1, float("inf")
        for i, p in enumerate(pieces):
            if not p or i in gone or (r.get("kind") and p["kind"] != r["kind"]):
                continue
            d = math.dist(p["c"], r["c"])
            ds = max(abs(a - b) for a, b in zip(p["size"], r["size"])) if r.get("size") else 0
            if d <= 1.0 and ds <= 2.0 and d < bd:
                bd, best = d, i
        if best >= 0:
            gone.add(best)
        else:
            missing += 1
    return gone, missing


def _solids(tri, offset, ranges, info=None):
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
            if info is not None:
                info.append(_where(v))
        else:
            skipped += 1
            if info is not None:
                info.append(None)
    return out, skipped


def fins(verts, faces, values):
    """Support Fins under a posed body (z up, as it will print)."""
    opts = _options(values)

    def build():
        r = json.loads(_call("computeFinsB64", _soup(verts, faces), opts))
        r["report"] = _call("reportLine", r["stats"])
        return r
    r = _cached("fins", (_mesh_key(verts, faces), opts), build)
    tri = np.frombuffer(base64.b64decode(r["triangles"]), dtype="<f4").reshape(-1, 3, 3)
    info = []
    solids, skipped = _solids(tri, r["offset"], [p["ranges"] for p in r["pieces"]], info)
    pieces = [dict(kind=p["kind"], **w) for p, w in zip(r["pieces"], info) if w]
    return {"solids": solids, "pieces": pieces, "skipped": skipped, "stats": r["stats"], "report": r["report"],
            "over": r["overFaces"], "small": r["smallFaces"], "version": _active.get("version")}


def arc_path(a, b, m, step=4.0):
    """The circle through a, b and m in plan, sampled as a path from a to b (None when in line)."""
    d = 2 * (a[0] * (b[1] - m[1]) + b[0] * (m[1] - a[1]) + m[0] * (a[1] - b[1]))
    if abs(d) < 1e-9:
        return None
    sa, sb, sm = a[0] ** 2 + a[1] ** 2, b[0] ** 2 + b[1] ** 2, m[0] ** 2 + m[1] ** 2
    cx = (sa * (b[1] - m[1]) + sb * (m[1] - a[1]) + sm * (a[1] - b[1])) / d
    cy = (sa * (m[0] - b[0]) + sb * (a[0] - m[0]) + sm * (b[0] - a[0])) / d
    r = math.hypot(a[0] - cx, a[1] - cy)
    if not r < 1e4:
        return None
    tau = 2 * math.pi
    ang = lambda p: math.atan2(p[1] - cy, p[0] - cx)  # noqa: E731
    a0 = ang(a)
    to_b, to_m = (ang(b) - a0) % tau, (ang(m) - a0) % tau
    sweep = to_b if to_m < to_b else to_b - tau
    fm = to_m / sweep if sweep > 0 else (tau - to_m) / -sweep
    n = max(2, math.ceil(abs(sweep) * r / step))
    out = []
    for k in range(n + 1):
        t = k / n
        th = a0 + sweep * t
        z = a[2] + (m[2] - a[2]) * (t / fm if fm > 0 else 1) if t <= fm else m[2] + (b[2] - m[2]) * ((t - fm) / (1 - fm))
        out.append([cx + r * math.cos(th), cy + r * math.sin(th), z])
    return out


def wall_sections(w, curves):
    """The pieces a drawn wall is built from: [(a, b, m or None), ...]."""
    p = [list(map(float, q)) for q in (w.get("points") or [])]
    if w.get("shape") == "curve" and len(p) == 3:
        if curves:
            return [(p[0], p[1], p[2])]
        # an engine without the curve extra: short straight walls round the same arc
        path = arc_path(p[0], p[1], p[2], 5.0)
        if path:
            return [(path[i], path[i + 1], None) for i in range(len(path) - 1)]
        return [(p[0], p[1], None)]
    return [(p[i - 1], p[i], None) for i in range(1, len(p))]


def walls(verts, faces, wall_list, values):
    """Hand-drawn walls (Support Fins' Draw mode): lines, lines of several sections, curves."""
    opts = _options(values)
    mk = _mesh_key(verts, faces)
    curves = _has("drawWallThroughB64")
    soup = []

    def one(a, b, m):
        def build():
            if not soup:
                soup.append(_soup(verts, faces))
            if m is not None and curves:
                r = _call("drawWallThroughB64", soup[0], json.dumps(a), json.dumps(b), json.dumps(m), opts)
            else:
                r = _call("drawWallB64", soup[0], json.dumps(a), json.dumps(b), opts)
            return json.loads(r)
        return _cached("wall", (mk, json.dumps([a, b, m]), opts), build)

    def hand_piece(w):
        """A gusset (off a side, out to an overhang) or an anchor fin (against a side), by hand."""
        pts = [list(map(float, q)) for q in (w.get("points") or [])]
        n = [float(x) for x in (w.get("normal") or [0, 0, 0])]
        if not soup:
            soup.append(_soup(verts, faces))
        if w.get("shape") == "gusset":
            if not _has("gussetWallB64"):
                return {"ok": False, "reason": "this Support Fins engine has no gussets"}
            if len(pts) < 2:
                return {"ok": False, "reason": "click the side, then the overhang"}
            return _cached("wall", (mk, "g" + json.dumps([pts, n]), opts), lambda: json.loads(
                _call("gussetWallB64", soup[0], json.dumps(pts[0]), json.dumps(pts[1]), json.dumps(n), opts)))
        if not _has("anchorFinB64"):
            return {"ok": False, "reason": "this Support Fins engine has no anchor fins"}
        face = nearest_face(verts, faces, pts[0], n)
        if face < 0:
            return {"ok": False, "reason": "the face it stood on isn’t there any more"}
        try:
            sway = json.loads(_call("optionsFromDialogJson", json.dumps({**(values or {}), "sway.on": True}))).get("sway") or {}
        except GeometryError:
            sway = {}
        return _cached("wall", (mk, "a" + json.dumps([pts[0], face]), opts), lambda: json.loads(
            _call("anchorFinB64", soup[0], json.dumps(pts[0]), face, opts, json.dumps(sway))))

    solids, report, by_wall = [], [], []
    for w in wall_list:
        mine = []
        by_wall.append(mine)
        st = {"ok": True, "length": 0.0, "height": 0.0, "tines": 0, "partAttached": False, "sections": 0,
              "failed": 0, "reason": None}
        hand = w.get("shape") in ("gusset", "anchor")
        for a, b, m in ([(None, None, None)] if hand else wall_sections(w, curves)):
            r = hand_piece(w) if hand else one(a, b, m)
            st["sections"] += 1
            if not r.get("ok"):
                st["failed"] += 1
                st["reason"] = st["reason"] or r.get("reason")
                continue
            tri = np.frombuffer(base64.b64decode(r["triangles"]), dtype="<f4").reshape(-1, 3, 3)
            got, _ = _solids(tri, r["offset"], [[[0, len(tri)]]])
            if not got:
                st["failed"] += 1
                st["reason"] = st["reason"] or "it didn’t come out as a closed solid"
                continue
            solids += got
            mine += got
            s = r["stats"]
            st["length"] += s.get("length", 0)
            st["height"] = max(st["height"], s.get("height", 0))
            st["tines"] += s.get("tines", 0) or 0
            st["partAttached"] = st["partAttached"] or bool(s.get("partAttached"))
        st["ok"] = st["failed"] < st["sections"]
        report.append(st)
    return {"solids": solids, "byWall": by_wall, "walls": report, "version": _active.get("version")}


def suggest(verts, faces, values, top=3):
    """Support Fins' Suggest: up to `top` print orientations, best first (rot: applied to the part as it is)."""
    if not _has("suggestPosesJson"):
        raise GeometryError("This Support Fins engine can’t suggest orientations.")
    opts = _options(values)
    return _cached("fins", (_mesh_key(verts, faces), f"suggest{top}", opts),
                   lambda: json.loads(_call("suggestPosesJson", _soup(verts, faces), opts, int(top))))


def strength(verts, faces, values, load=None):
    """How strong the current pose is, and for a load pointing `load` (as the part sits), if given."""
    if not _has("strengthOfJson"):
        raise GeometryError("This Support Fins engine has no strength check.")
    opts, soup = _options(values), _soup(verts, faces)
    r = json.loads(_call("strengthOfJson", soup, opts, json.dumps(load) if load else ""))
    r["strongest"] = (json.loads(_call("strongestPoseJson", soup, opts, json.dumps(load)))
                      if load and (r.get("load") or {}).get("quality") != "good" else None)
    return r


def wall(verts, faces, a, b, values):
    """One hand-drawn wall between two points on the body (kept for older callers)."""
    r = walls(verts, faces, [{"shape": "line", "points": [a, b]}], values)
    w = r["walls"][0]
    if not w["ok"]:
        raise GeometryError(w["reason"] or "Support Fins couldn't place a wall there.")
    return {"solids": r["solids"], "stats": w, "version": r["version"]}


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
        if _digest(data) != info.get("sha256", {}).get("engine.iife.js"):
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
