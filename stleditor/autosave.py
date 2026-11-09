"""Autosave and crash recovery.

Each running window owns one folder under %LOCALAPPDATA%\\STL Editor\\autosave\\<session>, holding
`document.json`, the imported source meshes and a `lock` file kept locked while the session
runs. A folder whose lock can be taken at start-up belongs to a session that didn't close
cleanly, so its work can be recovered."""
from __future__ import annotations

import json
import os
import shutil
import threading
import time
import uuid

from . import paths

ROOT = os.path.join(paths.data_dir(), "autosave")


def _lock(fh):
    try:
        if os.name == "nt":
            import msvcrt
            fh.seek(0)
            msvcrt.locking(fh.fileno(), msvcrt.LK_NBLCK, 1)
        else:
            import fcntl
            fcntl.flock(fh.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        return True
    except OSError:
        return False


class Autosaver:
    def __init__(self, app, interval=60.0):
        self.app = app
        self.interval = interval
        self.session = time.strftime("%Y%m%d-%H%M%S-") + uuid.uuid4().hex[:6]
        self.dir = os.path.join(ROOT, self.session)
        os.makedirs(self.dir, exist_ok=True)
        self._lockfh = open(os.path.join(self.dir, "lock"), "a+")
        self._lockfh.write(" ")
        self._lockfh.flush()
        _lock(self._lockfh)
        self._last = None
        self.last_time = None
        self._stop = threading.Event()
        threading.Thread(target=self._loop, daemon=True).start()

    # -- saving -----------------------------------------------------------------------------
    def _loop(self):
        while not self._stop.wait(self.interval):
            try:
                self.save_now()
            except Exception as e:  # never let autosave take the app down
                print("autosave failed:", e, flush=True)

    def save_now(self, force=False):
        with self.app.lock:
            doc = self.app.doc
            if not doc.features or (not doc.dirty and not force):
                return False
            snap = json.dumps({"features": doc.features, "rollback": doc.rollback, "ui": doc.ui,
                               "params": doc.params})
            if snap == self._last and not force:
                return False
            used = {f["params"].get("source") for f in doc.features if f["type"] == "import"}
            srcdir = os.path.join(self.dir, "sources")
            os.makedirs(srcdir, exist_ok=True)
            meta = {}
            for sid, src in doc.sources.items():
                if sid not in used:
                    continue
                fn = os.path.join(srcdir, sid + src["ext"])
                if not os.path.exists(fn):
                    with open(fn + ".tmp", "wb") as fh:
                        fh.write(src["data"])
                    os.replace(fn + ".tmp", fn)
                meta[sid] = {"name": src["name"], "ext": src["ext"]}
            data = {"format": 1, "app": "STL Editor", "features": doc.features, "rollback": doc.rollback,
                    "ui": doc.ui, "params": doc.params, "sources": meta, "path": doc.path, "title": doc.title or (
                        os.path.basename(doc.path) if doc.path else None),
                    "saved": time.time(), "count": len(doc.features)}
            tmp = os.path.join(self.dir, "document.json.tmp")
            with open(tmp, "w", encoding="utf-8") as fh:
                json.dump(data, fh)
            os.replace(tmp, os.path.join(self.dir, "document.json"))
            self._last = snap
            self.last_time = time.time()
            return True

    def clear(self):
        """The user saved or discarded: nothing to recover from this session any more."""
        try:
            os.remove(os.path.join(self.dir, "document.json"))
        except OSError:
            pass
        shutil.rmtree(os.path.join(self.dir, "sources"), ignore_errors=True)
        self._last = None

    def shutdown(self):
        """Clean exit: remove this session's folder."""
        self._stop.set()
        try:
            self._lockfh.close()
        except OSError:
            pass
        shutil.rmtree(self.dir, ignore_errors=True)


# ------------------------------------------------------------------------------------------
# Recovery
# ------------------------------------------------------------------------------------------
def find_recoverable(exclude: str | None = None):
    out = []
    if not os.path.isdir(ROOT):
        return out
    for name in sorted(os.listdir(ROOT)):
        d = os.path.join(ROOT, name)
        if name == exclude or not os.path.isdir(d):
            continue
        lockf = os.path.join(d, "lock")
        if os.path.exists(lockf):
            try:
                fh = open(lockf, "a+")
            except OSError:
                continue
            free = _lock(fh)
            fh.close()
            if not free:
                continue  # another window is still running this session
        docf = os.path.join(d, "document.json")
        if not os.path.exists(docf):
            shutil.rmtree(d, ignore_errors=True)  # stale empty session
            continue
        try:
            with open(docf, encoding="utf-8") as fh:
                meta = json.load(fh)
        except Exception:
            continue
        out.append({"id": name, "title": meta.get("title") or "Untitled", "path": meta.get("path"),
                    "saved": meta.get("saved"), "steps": meta.get("count", len(meta.get("features", [])))})
    return out


def load(session_id: str):
    """Rebuild a Document from an autosave folder."""
    from .document import Document
    import itertools
    d = os.path.join(ROOT, session_id)
    with open(os.path.join(d, "document.json"), encoding="utf-8") as fh:
        data = json.load(fh)
    doc = Document()
    for sid, meta in data.get("sources", {}).items():
        with open(os.path.join(d, "sources", sid + meta["ext"]), "rb") as fh:
            doc.sources[sid] = {"name": meta["name"], "ext": meta["ext"], "data": fh.read()}
    doc.features = data["features"]
    doc.rollback = data.get("rollback")
    doc.ui = data.get("ui", doc.ui)
    doc.params = data.get("params", [])
    doc.path = data.get("path")
    doc.title = data.get("title")
    nums = [int(x[1:]) for x in [f["id"] for f in doc.features] + list(doc.sources) if x[1:].isdigit()]
    doc._ids = itertools.count(max(nums, default=0) + 1)
    doc.evaluate()
    doc.dirty = True
    return doc


def discard(session_id: str):
    d = os.path.join(ROOT, session_id)
    if os.path.dirname(os.path.abspath(d)) == os.path.abspath(ROOT):
        shutil.rmtree(d, ignore_errors=True)
