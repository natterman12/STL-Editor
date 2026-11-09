"""Tiny persistent settings store (recent files, last folder, UI prefs).

Kept in %APPDATA%\\STL Editor\\settings.json. STLEDITOR_CONFIG_DIR points it at another folder:
the tests set it to a throwaway one so their files don't show up in the user's Recent list.
"""
import json
import os
import threading

from . import paths

_lock = threading.Lock()
_dir = os.environ.get("STLEDITOR_CONFIG_DIR") or paths.config_dir()
_path = os.path.join(_dir, "settings.json")
_data = None


def _load():
    global _data
    if _data is None:
        try:
            with open(_path, encoding="utf-8") as fh:
                _data = json.load(fh)
        except Exception:
            _data = {}
    return _data


def get(key, default=None):
    with _lock:
        return _load().get(key, default)


def set(key, value):  # noqa: A001 - mirrors dict-style API
    with _lock:
        _load()[key] = value
        try:
            os.makedirs(_dir, exist_ok=True)
            tmp = _path + ".tmp"
            with open(tmp, "w", encoding="utf-8") as fh:
                json.dump(_data, fh, indent=1)
            os.replace(tmp, _path)
        except OSError:
            pass
