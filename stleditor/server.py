"""Local HTTP server: serves the UI and a JSON/binary API over the Document."""
from __future__ import annotations

import json
import mimetypes
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import threading
import traceback
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import numpy as np

from . import geometry as G
from .document import Document
from .geometry import GeometryError
from . import settings

WEB = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "web")
PALETTE = ["#9aa7b4", "#c9a66b", "#7fa8c9", "#a3c48a", "#c98a8a", "#a993c9", "#c9b98a",
           "#8ac9c0"]
MESH_FILTER = "Mesh files (*.stl;*.obj;*.3mf;*.ply;*.off;*.glb)"
PROJECT_FILTER = "STL Editor project (*.stlx)"


class App:
    """Holds the open document and the optional GUI window (for file dialogs)."""

    def __init__(self):
        self.doc = Document()
        self.lock = threading.RLock()
        self.window = None  # pywebview window, when running as a desktop app
        self.autosaver = None

    # -- dialogs ----------------------------------------------------------------------------
    def dialog(self, kind: str, title: str, filters: list[str], multiple=False, filename=""):
        if self.window is not None:
            import webview
            FD = getattr(webview, "FileDialog", None)
            if FD is not None:
                mode = {"open": FD.OPEN, "save": FD.SAVE}[kind]
            else:
                mode = {"open": webview.OPEN_DIALOG, "save": webview.SAVE_DIALOG}[kind]
            kw = {"allow_multiple": multiple, "file_types": tuple(filters)} if kind == "open" else \
                {"save_filename": filename, "file_types": tuple(filters)}
            r = self.window.create_file_dialog(mode, directory=settings.get("last_dir", ""), **kw)
            if not r:
                return []
            paths = [r] if isinstance(r, str) else list(r)
        else:
            paths = _tk_dialog(kind, title, filters, multiple, filename)
        if paths:
            settings.set("last_dir", os.path.dirname(paths[0]))
        return paths

    # -- state ------------------------------------------------------------------------------
    def state_json(self):
        d = self.doc
        st = d.state
        res = {r["id"]: r for r in d._results}
        feats = []
        for i, f in enumerate(d.features):
            r = res.get(f["id"], {})
            active = d.rollback is None or i < d.rollback
            feats.append({**f, "error": r.get("error"), "warning": r.get("warning"),
                          "ms": r.get("ms"), "active": active})
        bodies = []
        for i, (bid, b) in enumerate(st.bodies.items()):
            v, f = b.mesh()
            bodies.append({"id": bid, "name": d.body_name(bid), "serial": b.serial,
                           "solid": b.is_solid, "triangles": int(len(f)),
                           "color": d.ui["colors"].get(bid, PALETTE[i % len(PALETTE)]),
                           "hidden": bid in d.ui["hidden"],
                           "bbox": [v.min(0).tolist(), v.max(0).tolist()] if len(v) else None})
        planes = [{"id": k, "name": self._fname(k, p["name"]), "frame": [np.asarray(a).tolist() for a in p["frame"]],
                   "hidden": k in d.ui["hidden"]} for k, p in st.planes.items()]
        sketches = [{"id": k, "name": self._fname(k, s["name"]),
                     "frame": [np.asarray(a).tolist() for a in s["frame"]],
                     "entities": s["entities"], "refs": s.get("refs", []),
                     "hidden": k in d.ui["hidden"]} for k, s in st.sketches.items()]
        return {"features": feats, "rollback": d.rollback, "bodies": bodies, "planes": planes,
                "sketches": sketches,
                "doc": {"path": d.path, "name": os.path.basename(d.path) if d.path else (d.title or "Untitled"),
                        "dirty": d.dirty, "can_undo": bool(d._undo), "can_redo": bool(d._redo),
                "version": __import__("stleditor").__version__,
                "autosaved": self.autosaver.last_time if self.autosaver else None},
                "recent": settings.get("recent", [])}

    def _fname(self, fid, default):
        for f in self.doc.features:
            if f["id"] == fid:
                return f.get("name") or default
        return default


def _tk_dialog(kind, title, filters, multiple, filename):
    import tkinter as tk
    from tkinter import filedialog
    root = tk.Tk()
    root.withdraw()
    root.attributes("-topmost", True)
    ft = []
    for flt in filters:
        label, _, pats = flt.partition("(")
        ft.append((label.strip(), pats.rstrip(")").replace(";", " ")))
    ft.append(("All files", "*.*"))
    try:
        if kind == "open":
            r = filedialog.askopenfilenames(title=title, filetypes=ft, initialdir=settings.get("last_dir", "")) \
                if multiple else filedialog.askopenfilename(title=title, filetypes=ft,
                                                            initialdir=settings.get("last_dir", ""))
            paths = list(r) if multiple else ([r] if r else [])
        else:
            r = filedialog.asksaveasfilename(title=title, filetypes=ft, initialfile=filename,
                                             initialdir=settings.get("last_dir", ""))
            paths = [r] if r else []
    finally:
        root.destroy()
    return [p for p in paths if p]


def pack_meshes(bodies: list[tuple[str, G.Body]], header: dict) -> bytes:
    """[u32 header length][header JSON][for each body: f32 verts, u32 tris] (4-byte aligned)."""
    chunks = []
    meta = []
    off = 0
    for bid, b in bodies:
        v, f = b.mesh()
        vb = np.ascontiguousarray(v, np.float32).tobytes()
        fb = np.ascontiguousarray(f, np.uint32).tobytes()
        meta.append({"id": bid, "nv": int(len(v)), "nf": int(len(f)), "offset": off,
                     "serial": b.serial, "solid": b.is_solid})
        chunks += [vb, fb]
        off += len(vb) + len(fb)
    header = dict(header, meshes=meta)
    hj = json.dumps(header).encode()
    hj += b" " * ((4 - (len(hj) + 4) % 4) % 4)
    return struct.pack("<I", len(hj)) + hj + b"".join(chunks)


def make_handler(app: App):
    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, fmt, *args):
            pass

        # -- plumbing -----------------------------------------------------------------------
        def _send(self, code, body: bytes, ctype="application/json"):
            self.send_response(code)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(body)

        def _json(self, obj, code=200):
            self._send(code, json.dumps(obj, default=_jdefault).encode())

        def _body(self) -> bytes:
            n = int(self.headers.get("Content-Length", 0) or 0)
            return self.rfile.read(n) if n else b""

        def do_GET(self):
            u = urllib.parse.urlparse(self.path)
            q = urllib.parse.parse_qs(u.query)
            try:
                if u.path == "/api/state":
                    with app.lock:
                        return self._json(app.state_json())
                if u.path == "/api/mesh":
                    with app.lock:
                        bid = q["id"][0]
                        b = app.doc.state.body(bid)
                        return self._send(200, pack_meshes([(bid, b)], {}), "application/octet-stream")
                path = u.path.lstrip("/") or "index.html"
                full = os.path.normpath(os.path.join(WEB, path))
                if not full.startswith(WEB) or not os.path.isfile(full):
                    return self._send(404, b"not found", "text/plain")
                ctype = mimetypes.guess_type(full)[0] or "application/octet-stream"
                if full.endswith(".js"):
                    ctype = "text/javascript"
                with open(full, "rb") as fh:
                    self._send(200, fh.read(), ctype)
            except GeometryError as e:
                self._json({"error": str(e)}, 400)
            except Exception as e:
                traceback.print_exc()
                self._json({"error": f"{type(e).__name__}: {e}"}, 500)

        def do_POST(self):
            u = urllib.parse.urlparse(self.path)
            q = urllib.parse.parse_qs(u.query)
            route = u.path[len("/api/"):] if u.path.startswith("/api/") else None
            try:
                raw = self._body()
                if route == "upload":
                    name = q.get("name", ["upload.stl"])[0]
                    with app.lock:
                        app.doc.import_file(name, data=raw, place=q.get("place", ["center"])[0])
                        return self._json({"ok": True, "state": app.state_json()})
                data = json.loads(raw or b"{}")
                fn = ROUTES.get(route)
                if fn is None:
                    return self._json({"error": f"unknown route {route}"}, 404)
                with app.lock:
                    out = fn(app, data)
                if isinstance(out, bytes):
                    return self._send(200, out, "application/octet-stream")
                return self._json(out)
            except GeometryError as e:
                self._json({"error": str(e)}, 400)
            except Exception as e:
                traceback.print_exc()
                self._json({"error": f"{type(e).__name__}: {e}"}, 500)

    return Handler


def _jdefault(o):
    if isinstance(o, np.ndarray):
        return o.tolist()
    if isinstance(o, (np.floating, np.integer)):
        return o.item()
    raise TypeError(str(type(o)))


# ------------------------------------------------------------------------------------------
# Routes
# ------------------------------------------------------------------------------------------
def r_feature_add(app, d):
    f, res = app.doc.add_feature(d["type"], d.get("params", {}), d.get("name"))
    return {"ok": True, "feature": f, "result": res, "state": app.state_json()}


def r_feature_edit(app, d):
    app.doc.edit_feature(d["id"], d.get("params"), d.get("name"), d.get("suppressed"),
                         d["rollback_after"] if "rollback_after" in d else "keep")
    return {"ok": True, "state": app.state_json()}


def r_feature_delete(app, d):
    app.doc.delete_feature(d["id"])
    return {"ok": True, "state": app.state_json()}


def r_feature_move(app, d):
    app.doc.move_feature(d["id"], int(d["index"]))
    return {"ok": True, "state": app.state_json()}


def r_rollback(app, d):
    app.doc.set_rollback(d.get("index"))
    return {"ok": True, "state": app.state_json()}


def r_preview(app, d):
    state, res = app.doc.preview(d["type"], d.get("params", {}), d.get("edit_id"))
    ids = list(dict.fromkeys(res.get("created", []) + res.get("modified", [])))
    bodies = [(i, state.bodies[i]) for i in ids if i in state.bodies]
    return pack_meshes(bodies, {"result": res, "removed": [i for i in res.get("removed", [])]})


def r_face(app, d):
    b = app.doc.state.body(d["body"])
    v, f = b.mesh()
    region, n, c = G.face_region(v, f, int(d["tri"]), float(d.get("angle_tol", 1.0)))
    hit = np.asarray(d.get("point", c), float)
    axis_aligned = np.max(np.abs(n)) > 0.99999
    if axis_aligned:
        # snap to the exact axis and put the sketch origin over the world origin, so sketch
        # coordinates match world coordinates on flat, square-on faces
        k = int(np.argmax(np.abs(n)))
        sign = 1.0 if n[k] > 0 else -1.0
        n = np.zeros(3)
        n[k] = sign
        origin = np.zeros(3)
        origin[k] = c[k]
    else:
        origin = c
    o, x, y, nn = G.plane_frame(origin, n)
    bd = G.region_boundary(f, region)
    if len(bd) > 20000:
        bd = bd[:20000]
    seg3 = v[bd].astype(float)
    # boundary in sketch coordinates, for reference geometry in the sketcher
    seg2 = np.stack([(seg3 - o) @ x, (seg3 - o) @ y], axis=-1)
    tris = region if len(region) <= 200000 else region[:200000]
    contours = []
    if d.get("outline") and len(region) <= 400000:
        from . import printing as PR
        contours = PR.face_outline(v, f, region, (o, x, y, nn))
    return {"contours": contours,"body": d["body"], "origin": o.tolist(), "normal": nn.tolist(), "xdir": x.tolist(),
            "ydir": y.tolist(), "centroid": c.tolist(), "hit": hit.tolist(),
            "tris": tris.tolist(), "boundary3d": seg3.round(5).tolist(),
            "boundary2d": seg2.round(5).tolist(), "planar": len(region) > 1}


def r_sketch_op(app, d):
    from . import sketch as S
    import itertools
    ents = d.get("entities", [])
    used = {e.get("id") for e in ents}
    cnt = itertools.count(1)

    def new_id():
        while True:
            i = f"e{d.get('seed', 'x')}{next(cnt)}"
            if i not in used:
                used.add(i)
                return i
    op = d.get("op")
    if op == "trim":
        out = S.trim(ents, d["id"], d["point"])
    elif op == "offset":
        out = S.offset(ents, d.get("ids", []), float(d.get("distance", 1)), new_id)
    elif op == "mirror":
        out = S.mirror(ents, d.get("ids", []), d["axis"], new_id)
    elif op == "explode":
        out = []
        for e in ents:
            out.extend(S.explode(e) if e.get("id") in d.get("ids", []) else [e])
    elif op == "project":
        from . import meshtools as MT
        st = app.doc.state
        ids = d.get("bodies") or [k for k, b in st.bodies.items()
                                  if k not in app.doc.ui["hidden"] and b.is_solid]
        solids = [st.body(i).require_solid() for i in ids if i in st.bodies]
        if not solids:
            raise GeometryError("There are no solid bodies to project.")
        polys = MT.outline(solids, d["frame"], d.get("mode", "section"),
                           float(d.get("tolerance", 0.02)), float(d.get("offset", 0)))
        add = MT.outline_entities(polys, new_id, bool(d.get("construction")))
        return {"ok": True, "entities": ents + add, "added": [e["id"] for e in add]}
    elif op == "text":
        from . import meshtools as MT
        import math as _m
        cont = MT.text_contours(d.get("text", ""), d.get("font"), float(d.get("size", 10)),
                                bool(d.get("bold")), bool(d.get("italic")), float(d.get("spacing", 0)),
                                d.get("align", "left"))
        if not cont:
            raise GeometryError("Type some text first.")
        lo, hi = MT.text_extent(cont)
        anchor = d.get("anchor", "baseline")
        ox = {"left": 0.0, "center": (lo[0] + hi[0]) / 2, "right": hi[0]}.get(d.get("align", "left"), 0.0) \
            if anchor != "center" else (lo[0] + hi[0]) / 2
        oy = (lo[1] + hi[1]) / 2 if anchor == "center" else 0.0
        a = _m.radians(float(d.get("angle", 0)))
        ca, sa = _m.cos(a), _m.sin(a)
        x0, y0 = float(d.get("x", 0)), float(d.get("y", 0))
        group = d.get("group") or f"t{d.get('seed', 'x')}"
        meta = {k: d.get(k) for k in ("text", "font", "size", "bold", "italic", "spacing", "align", "angle", "x", "y", "anchor")}
        keep = [e for e in ents if e.get("group") != group]
        add = []
        for c in cont:
            px, py = c[:, 0] - ox, c[:, 1] - oy
            pts = [[round(x0 + ca * u - sa * w, 4), round(y0 + sa * u + ca * w, 4)] for u, w in zip(px, py)]
            add.append({"id": new_id(), "type": "polyline", "closed": True, "pts": pts, "group": group, "text": meta})
        return {"ok": True, "entities": keep + add, "added": [e["id"] for e in add], "group": group}
    elif op == "faces":
        return {"ok": True, "faces": [[list(map(list, f.exterior.coords))] + [list(map(list, h.coords)) for h in f.interiors]
                                      for f in S.faces(ents)]}
    else:
        raise GeometryError(f"Unknown sketch operation {op}.")
    return {"ok": True, "entities": out}


def r_cylinder(app, d):
    from . import meshtools as MT
    b = app.doc.state.body(d["body"])
    v, f = b.mesh()
    tri = int(d["tri"]) if d.get("tri") is not None else MT.nearest_triangle(v, f, d["point"])
    cyl = MT.fit_cylinder(v, f, tri)
    return {"ok": True, "kind": cyl["kind"], "diameter": round(2 * cyl["radius"], 3),
            "length": round(cyl["z1"] - cyl["z0"], 3), "axis": cyl["axis"], "tris": cyl["tris"][:200000]}


def r_fonts(app, d):
    from . import meshtools as MT
    fams = MT.list_fonts()
    return {"ok": True, "fonts": list(fams), "default": MT.default_font()}


def r_recovery_list(app, d):
    from . import autosave
    sess = app.autosaver.session if app.autosaver else None
    return {"ok": True, "items": autosave.find_recoverable(exclude=sess)}


def r_recovery_open(app, d):
    from . import autosave
    app.doc = autosave.load(d["id"])
    autosave.discard(d["id"])
    if app.autosaver:
        app.autosaver.save_now(force=True)
    return {"ok": True, "state": app.state_json()}


def r_recovery_discard(app, d):
    from . import autosave
    for i in d.get("ids") or [d.get("id")]:
        if i:
            autosave.discard(i)
    return {"ok": True}


SETTING_KEYS = {"units", "theme", "update_check", "update_repo", "update_skip", "autosave_interval",
                "snap_step"}


def r_settings_get(app, d):
    return {"ok": True, "settings": {k: settings.get(k) for k in SETTING_KEYS}}


def r_settings_set(app, d):
    k = d.get("key")
    if k not in SETTING_KEYS:
        raise GeometryError("Unknown setting.")
    settings.set(k, d.get("value"))
    if k == "autosave_interval" and app.autosaver:
        app.autosaver.interval = max(15.0, float(d.get("value") or 60))
    return {"ok": True}


def r_update_status(app, d):
    from . import updates
    if d.get("check"):
        updates.check(force=True)
    return {"ok": True, **updates.status()}


def r_open_url(app, d):
    import webbrowser
    url = str(d.get("url", ""))
    if not url.startswith("https://"):
        raise GeometryError("Only https links can be opened.")
    webbrowser.open(url)
    return {"ok": True}


def r_printers(app, d):
    from . import printing as PR
    return {"ok": True, "printers": {k: {"name": v[0], "bed": v[1]} for k, v in PR.PRINTERS.items()}}


def r_body(app, d):
    app.doc.set_body_props(d["id"], d.get("name"), d.get("color"), d.get("hidden"))
    return {"ok": True, "state": app.state_json()}


def r_info(app, d):
    b = app.doc.state.body(d["id"])
    return {"ok": True, "info": dict(b.info(), name=app.doc.body_name(d["id"]))}


def r_undo(app, d):
    app.doc.undo()
    return {"ok": True, "state": app.state_json()}


def r_redo(app, d):
    app.doc.redo()
    return {"ok": True, "state": app.state_json()}


def _remember(path):
    rec = [p for p in settings.get("recent", []) if p != path]
    settings.set("recent", ([path] + rec)[:10])


def _clear_autosave(app):
    if app.autosaver:
        app.autosaver.clear()


def r_new(app, d):
    if app.doc.dirty and not d.get("force"):
        return {"ok": False, "confirm": "Discard unsaved changes?"}
    app.doc = Document()
    _clear_autosave(app)
    return {"ok": True, "state": app.state_json()}


def r_open(app, d):
    if app.doc.dirty and not d.get("force"):
        return {"ok": False, "confirm": "Discard unsaved changes?"}
    paths = [d["path"]] if d.get("path") else app.dialog("open", "Open", [PROJECT_FILTER, MESH_FILTER])
    if not paths:
        return {"ok": False, "cancelled": True}
    p = paths[0]
    _clear_autosave(app)
    if p.lower().endswith(".stlx"):
        app.doc = Document.open(p)
    else:
        app.doc = Document()
        app.doc.import_file(p)
        app.doc.title = os.path.splitext(os.path.basename(p))[0]
        app.doc.dirty = False
        app.doc._undo.clear()
    _remember(p)
    return {"ok": True, "state": app.state_json()}


def r_import(app, d):
    paths = d.get("paths") or app.dialog("open", "Insert mesh", [MESH_FILTER], multiple=True)
    if not paths:
        return {"ok": False, "cancelled": True}
    for p in paths:
        app.doc.import_file(p, repair=d.get("repair", True))
    return {"ok": True, "state": app.state_json()}


def r_save(app, d):
    path = d.get("path") or (None if d.get("save_as") else app.doc.path)
    if not path:
        name = (os.path.splitext(os.path.basename(app.doc.path))[0] if app.doc.path else (app.doc.title or "Untitled")) + ".stlx"
        paths = app.dialog("save", "Save project", [PROJECT_FILTER], filename=name)
        if not paths:
            return {"ok": False, "cancelled": True}
        path = paths[0]
    if not path.lower().endswith(".stlx"):
        path += ".stlx"
    app.doc.save(path)
    _clear_autosave(app)
    _remember(path)
    return {"ok": True, "path": path, "state": app.state_json()}


def r_export(app, d):
    fmt = d.get("format", "stl")
    path = d.get("path")
    if not path:
        base = "model"
        ids = d.get("bodies")
        if ids and len(ids) == 1:
            base = app.doc.body_name(ids[0])
        elif app.doc.path:
            base = os.path.splitext(os.path.basename(app.doc.path))[0]
        flt = {"stl": "STL (*.stl)", "3mf": "3MF (*.3mf)", "obj": "OBJ (*.obj)"}[fmt]
        paths = app.dialog("save", "Export", [flt], filename=f"{base}.{fmt}")
        if not paths:
            return {"ok": False, "cancelled": True}
        path = paths[0]
    if not os.path.splitext(path)[1]:
        path += "." + fmt
    n = app.doc.export(path, d.get("bodies"))
    if d.get("slicer"):
        _open_in(d["slicer"], path)
    return {"ok": True, "path": path, "count": n}


SOLIDCRAFT = r"C:\Program Files\SolidCraft"
SLICERS = {
    "bambu": [r"C:\Program Files\Bambu Studio\bambu-studio.exe"],
    "orca": [r"C:\Program Files\OrcaSlicer\orca-slicer.exe"],
    "solidcraft": [os.path.join(SOLIDCRAFT, "solidcraft.exe")],
}


def _find_exe(key):
    for p in SLICERS.get(key, []):
        if os.path.isfile(p):
            return p
    # search the install folder for any exe matching the key
    folder = {"bambu": r"C:\Program Files\Bambu Studio", "orca": r"C:\Program Files\OrcaSlicer"}.get(key)
    if folder and os.path.isdir(folder):
        for fn in os.listdir(folder):
            if fn.lower().endswith(".exe") and key in fn.lower():
                return os.path.join(folder, fn)
    return None


def _open_in(key, path):
    exe = _find_exe(key)
    if not exe:
        raise GeometryError(f"Couldn't find {key} on this computer.")
    subprocess.Popen([exe, path], close_fds=True)


def r_apps(app, d):
    return {"ok": True, "apps": {k: bool(_find_exe(k)) for k in SLICERS},
            "solidcraft_cli": os.path.isfile(os.path.join(SOLIDCRAFT, "solidcraft-cli.exe"))}


def r_send_to(app, d):
    """Export the chosen bodies to a temp STL and open it in a slicer or SolidCraft."""
    ids = d.get("bodies")
    folder = os.path.join(tempfile.gettempdir(), "STL Editor")
    os.makedirs(folder, exist_ok=True)
    base = app.doc.body_name(ids[0]) if ids and len(ids) == 1 else "model"
    safe = "".join(ch for ch in base if ch.isalnum() or ch in " -_()") or "model"
    path = os.path.join(folder, f"{safe}.stl")
    app.doc.export(path, ids)
    _open_in(d["app"], path)
    return {"ok": True, "path": path}


def r_to_step(app, d):
    """Use SolidCraft's Convert Mesh to turn a prismatic mesh body into an editable STEP solid."""
    cli = os.path.join(SOLIDCRAFT, "solidcraft-cli.exe")
    if not os.path.isfile(cli):
        raise GeometryError("SolidCraft isn't installed.")
    bid = d["body"]
    name = app.doc.body_name(bid)
    paths = [d["path"]] if d.get("path") else app.dialog("save", "Save STEP", ["STEP (*.step;*.stp)"],
                                                         filename=f"{name}.step")
    if not paths:
        return {"ok": False, "cancelled": True}
    out = paths[0]
    work = tempfile.mkdtemp(prefix="stle-")
    try:
        stl = os.path.join(work, "body.stl")
        app.doc.export(stl, [bid])
        script = {"commands": [
            {"command": "file.insert_mesh", "params": {"path": stl}},
            {"command": "mesh.convert", "params": {"body": "body",
                                                   "tolerance": float(d.get("tolerance", 0.05))}},
            {"command": "file.export", "params": {"path": out}}]}
        sp = os.path.join(work, "convert.json")
        with open(sp, "w") as fh:
            json.dump(script, fh)
        r = subprocess.run([cli, "run", sp, "--quiet"], capture_output=True, text=True, timeout=600,
                           creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        if r.returncode != 0:
            msg = (r.stderr or r.stdout).strip().splitlines()
            raise GeometryError("SolidCraft couldn't convert this mesh: " +
                                (msg[-1] if msg else "unknown error").replace("error: ", ""))
    finally:
        shutil.rmtree(work, ignore_errors=True)
    return {"ok": True, "path": out}


def r_quit(app, d):
    if app.doc.dirty and not d.get("force"):
        return {"ok": False, "confirm": "You have unsaved changes. Quit anyway?"}
    shutdown(app)
    threading.Timer(0.2, lambda: os._exit(0)).start()
    return {"ok": True}


def shutdown(app):
    """Clean exit: an unsaved document is kept for recovery only if the user didn't choose to
    discard it, so the autosave folder is removed only when there is nothing unsaved."""
    if app.autosaver:
        if app.doc.dirty and app.doc.features and not getattr(app, "discard_on_exit", True):
            app.autosaver.save_now(force=True)
        else:
            app.autosaver.shutdown()


HEARTBEAT = 0.0


def r_supportfins_info(app, d):
    from . import supportfins
    return {"ok": True, **supportfins.info()}


def r_status(app, d):
    global HEARTBEAT
    import time
    HEARTBEAT = time.time()
    return {"ok": True, "dirty": app.doc.dirty, "desktop": app.window is not None}


ROUTES = {
    "cylinder": r_cylinder, "fonts": r_fonts,
    "feature/add": r_feature_add, "feature/edit": r_feature_edit, "feature/delete": r_feature_delete,
    "feature/move": r_feature_move, "rollback": r_rollback, "preview": r_preview, "face": r_face,
    "body": r_body, "info": r_info, "undo": r_undo, "redo": r_redo, "new": r_new, "open": r_open,
    "import": r_import, "save": r_save, "export": r_export, "apps": r_apps, "send_to": r_send_to,
    "to_step": r_to_step, "quit": r_quit, "status": r_status, "sketch_op": r_sketch_op,
    "printers": r_printers, "recovery/list": r_recovery_list, "recovery/open": r_recovery_open,
    "recovery/discard": r_recovery_discard, "settings/get": r_settings_get, "settings/set": r_settings_set,
    "update/status": r_update_status, "open_url": r_open_url, "supportfins/info": r_supportfins_info,
}


def serve(app: App, port: int = 0):
    httpd = ThreadingHTTPServer(("127.0.0.1", port), make_handler(app))
    httpd.daemon_threads = True
    t = threading.Thread(target=httpd.serve_forever, daemon=True)
    t.start()
    return httpd, httpd.server_address[1]
