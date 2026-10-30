"""The design document: imported sources + an ordered, editable feature history."""
from __future__ import annotations

import copy
import io
import itertools
import json
import os
import time
import traceback
import zipfile

import numpy as np

from . import geometry as G
from .features import EVALUATORS, LABELS, State
from .geometry import GeometryError

FORMAT_VERSION = 1


class Document:
    def __init__(self):
        self.sources: dict[str, dict] = {}       # sid -> {name, ext, data: bytes}
        self._parsed: dict[str, list] = {}
        self.features: list[dict] = []
        self.rollback: int | None = None          # evaluate features[:rollback]
        self.ui = {"names": {}, "colors": {}, "hidden": []}
        self.path: str | None = None
        self.title: str | None = None              # shown when the document came from a mesh file
        self.dirty = False
        self._ids = itertools.count(1)
        self._cache: list[tuple[str, State]] = []  # (feature hash, state after it)
        self._memo: dict[str, tuple] = {}          # recent results by hash, reused on commit
        self._results: list[dict] = []
        self._undo: list[str] = []
        self._redo: list[str] = []
        self.state = State()

    # -- ids & sources ----------------------------------------------------------------------
    def new_id(self, prefix="f"):
        used = {f["id"] for f in self.features} | set(self.sources)
        while True:
            i = f"{prefix}{next(self._ids)}"
            if i not in used:
                return i

    def add_source(self, name: str, data: bytes) -> str:
        sid = self.new_id("s")
        self.sources[sid] = {"name": name, "ext": os.path.splitext(name)[1].lower(), "data": data}
        return sid

    def load_source(self, sid):
        if sid not in self._parsed:
            src = self.sources.get(sid)
            if src is None:
                raise GeometryError("The imported file is missing from the project.")
            self._parsed[sid] = G.load_mesh_file(src["data"], src["name"])
        return self._parsed[sid]

    # -- undo -------------------------------------------------------------------------------
    def _snapshot(self) -> str:
        return json.dumps({"features": self.features, "rollback": self.rollback, "ui": self.ui})

    def checkpoint(self):
        self._undo.append(self._snapshot())
        if len(self._undo) > 200:
            self._undo.pop(0)
        self._redo.clear()
        self.dirty = True

    def _restore(self, snap: str):
        d = json.loads(snap)
        self.features, self.rollback, self.ui = d["features"], d["rollback"], d["ui"]
        self.evaluate()

    def undo(self):
        if not self._undo:
            return False
        self._redo.append(self._snapshot())
        self._restore(self._undo.pop())
        self.dirty = True
        return True

    def redo(self):
        if not self._redo:
            return False
        self._undo.append(self._snapshot())
        self._restore(self._redo.pop())
        self.dirty = True
        return True

    # -- evaluation -------------------------------------------------------------------------
    def active_features(self):
        end = len(self.features) if self.rollback is None else self.rollback
        return self.features[:end]

    @staticmethod
    def _fhash(f, prev):
        return prev + "|" + json.dumps(f, sort_keys=True)

    def evaluate(self):
        ctx = {"load_source": self.load_source}
        state = State()
        results = []
        new_cache = []
        h = ""
        for i, f in enumerate(self.active_features()):
            h = self._fhash(f, h)
            if i < len(self._cache) and self._cache[i][0] == h and i < len(self._results):
                state = self._cache[i][1]
                results.append(self._results[i])
                new_cache.append(self._cache[i])
                continue
            if h in self._memo:
                state, res = self._memo[h]
            else:
                state, res = self.run_feature(state, f, ctx)
            results.append(res)
            new_cache.append((h, state))
        self._cache = new_cache
        self._results = results
        self.state = state
        return results

    def _remember(self, at: int, f: dict, state, res):
        """Keep a just-computed feature result so the next evaluate() can reuse it."""
        self.evaluate() if at > len(self._cache) else None
        prev = self._cache[at - 1][0] if at > 0 and at - 1 < len(self._cache) else ""
        self._memo[self._fhash(f, prev)] = (state, res)
        while len(self._memo) > 6:
            self._memo.pop(next(iter(self._memo)))

    def run_feature(self, state: State, f: dict, ctx=None):
        ctx = ctx or {"load_source": self.load_source}
        res = {"id": f["id"], "error": None, "warning": None, "created": [], "modified": [],
               "ms": 0.0}
        if f.get("suppressed"):
            res["suppressed"] = True
            return state, res
        t0 = time.perf_counter()
        new = state.clone()
        try:
            EVALUATORS[f["type"]](new, f["id"], f.get("params", {}), res, ctx)
            state = new
        except GeometryError as e:
            res["error"] = str(e)
        except Exception as e:  # keep the app alive; report the failure on the feature
            traceback.print_exc()
            res["error"] = f"{type(e).__name__}: {e}"
        res["ms"] = round((time.perf_counter() - t0) * 1000, 1)
        return state, res

    # -- editing ----------------------------------------------------------------------------
    def add_feature(self, ftype: str, params: dict, name: str | None = None):
        if ftype not in EVALUATORS:
            raise GeometryError(f"Unknown feature {ftype}.")
        f = {"id": self.new_id(), "type": ftype, "name": name or self._auto_name(ftype, params),
             "params": params}
        at = len(self.features) if self.rollback is None else self.rollback
        # try it first; refuse a feature that fails outright
        base = self._state_before(at)
        new_state, res = self.run_feature(base, f)
        if res["error"]:
            raise GeometryError(res["error"])
        self._remember(at, f, new_state, res)
        self.checkpoint()
        self.features.insert(at, f)
        if ftype in ("extrude", "revolve") and params.get("sketch"):
            # a sketch used by a feature is tucked away, as in other CAD tools
            hid = set(self.ui["hidden"]) | {params["sketch"]}
            self.ui["hidden"] = sorted(hid)
        if self.rollback is not None:
            self.rollback += 1
        self.evaluate()
        return f, res

    def preview(self, ftype, params, edit_id=None):
        """Evaluate a feature without committing; returns (state, result)."""
        if edit_id:
            idx = self.index(edit_id)
            base = self._state_before(idx)
            f = dict(self.features[idx], params=params)
            st, res = self.run_feature(base, f)
            if not res["error"]:
                self._remember(idx, f, st, res)
            return st, res
        else:
            at = len(self.features) if self.rollback is None else self.rollback
            base = self._state_before(at)
            f = {"id": "preview", "type": ftype, "name": "preview", "params": params}
        return self.run_feature(base, f)

    def edit_feature(self, fid: str, params: dict | None = None, name: str | None = None,
                     suppressed: bool | None = None, rollback_after="keep"):
        idx = self.index(fid)
        f = copy.deepcopy(self.features[idx])
        if params is not None:
            f["params"] = params
        if name is not None:
            f["name"] = name
        if suppressed is not None:
            f["suppressed"] = bool(suppressed)
        if params is not None:
            new_state, res = self.run_feature(self._state_before(idx), f)
            if res["error"]:
                raise GeometryError(res["error"])
            self._remember(idx, f, new_state, res)
        if rollback_after != "keep":
            # an edit session rolls the timeline back temporarily; undo should land on the
            # timeline as it was before that session, not on the rolled-back view
            self.rollback = rollback_after
        self.checkpoint()
        self.features[idx] = f
        return self.evaluate()

    def delete_feature(self, fid: str):
        idx = self.index(fid)
        self.checkpoint()
        del self.features[idx]
        if self.rollback is not None and idx < self.rollback:
            self.rollback -= 1
        return self.evaluate()

    def move_feature(self, fid: str, new_index: int):
        idx = self.index(fid)
        self.checkpoint()
        f = self.features.pop(idx)
        self.features.insert(max(0, min(new_index, len(self.features))), f)
        return self.evaluate()

    def set_rollback(self, index: int | None):
        if index is not None and index >= len(self.features):
            index = None
        self.rollback = index
        return self.evaluate()

    def index(self, fid):
        for i, f in enumerate(self.features):
            if f["id"] == fid:
                return i
        raise GeometryError("That feature no longer exists.")

    def _state_before(self, idx: int) -> State:
        if idx == 0:
            return State()
        self.evaluate()
        if idx - 1 < len(self._cache):
            return self._cache[idx - 1][1]
        # rolled back before idx: evaluate up to idx on the side
        state = State()
        for f in self.features[:idx]:
            state, _ = self.run_feature(state, f)
        return state

    def _auto_name(self, ftype, params=None):
        if ftype == "primitive":
            shape = (params or {}).get("shape", "box")
            n = sum(1 for f in self.features if f["type"] == ftype and f["params"].get("shape") == shape) + 1
            return f"{shape.capitalize()}{n}"
        n = sum(1 for f in self.features if f["type"] == ftype) + 1
        return f"{LABELS.get(ftype, ftype)}{n}"

    # -- bodies (UI-level properties) -------------------------------------------------------
    def body_name(self, bid):
        return self.ui["names"].get(bid) or self.state.bodies[bid].name

    def set_body_props(self, bid, name=None, color=None, hidden=None):
        self.checkpoint()
        if name is not None:
            self.ui["names"][bid] = name
        if color is not None:
            self.ui["colors"][bid] = color
        if hidden is not None:
            hid = set(self.ui["hidden"])
            (hid.add if hidden else hid.discard)(bid)
            self.ui["hidden"] = sorted(hid)

    # -- files ------------------------------------------------------------------------------
    def import_file(self, path_or_name: str, data: bytes | None = None, repair=True,
                    place="center"):
        if data is None:
            with open(path_or_name, "rb") as fh:
                data = fh.read()
        name = os.path.basename(path_or_name)
        if os.path.splitext(name)[1].lower() not in G.MESH_EXTS:
            raise GeometryError("Unsupported file type. Use STL, OBJ, 3MF, PLY, OFF or GLB.")
        sid = self.add_source(name, data)
        try:
            return self.add_feature("import", {"source": sid, "repair": repair, "place": place},
                                    name=os.path.splitext(name)[0])
        except Exception:
            del self.sources[sid]
            raise

    def save(self, path: str):
        used = {f["params"].get("source") for f in self.features if f["type"] == "import"}
        with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
            z.writestr("document.json", json.dumps({
                "format": FORMAT_VERSION, "app": "STL Editor", "features": self.features,
                "rollback": self.rollback, "ui": self.ui,
                "sources": {k: {"name": v["name"], "ext": v["ext"]} for k, v in self.sources.items()
                            if k in used}}, indent=1))
            for sid, src in self.sources.items():
                if sid in used:
                    z.writestr(f"sources/{sid}{src['ext']}", src["data"])
        self.path = path
        self.dirty = False

    @classmethod
    def open(cls, path: str) -> "Document":
        doc = cls()
        with zipfile.ZipFile(path) as z:
            d = json.loads(z.read("document.json"))
            for sid, meta in d.get("sources", {}).items():
                doc.sources[sid] = {"name": meta["name"], "ext": meta["ext"],
                                    "data": z.read(f"sources/{sid}{meta['ext']}")}
        doc.features = d["features"]
        doc.rollback = d.get("rollback")
        doc.ui = d.get("ui", doc.ui)
        doc.path = path
        nums = [int(x[1:]) for x in [f["id"] for f in doc.features] + list(doc.sources)
                if x[1:].isdigit()]
        doc._ids = itertools.count(max(nums, default=0) + 1)
        doc.evaluate()
        return doc

    def export(self, path: str, body_ids=None):
        ids = body_ids or [b for b in self.state.bodies if b not in self.ui["hidden"]] or \
            list(self.state.bodies)
        bodies = []
        for bid in ids:
            b = self.state.body(bid)
            bodies.append(b.copy(name=self.body_name(bid)))
        if not bodies:
            raise GeometryError("There is nothing to export.")
        G.export_bodies(bodies, path)
        return len(bodies)
