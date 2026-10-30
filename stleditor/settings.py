"""Tiny persistent settings store (recent files, last folder, UI prefs)."""
import json
import os
import threading

_lock = threading.Lock()
_dir = os.path.join(os.environ.get("APPDATA") or os.path.expanduser("~/.config"), "STL Editor")
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
