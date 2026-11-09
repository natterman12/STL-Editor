"""Where the app keeps its files on each system.

Windows: %APPDATA%\\STL Editor (settings), %LOCALAPPDATA%\\STL Editor (autosave, web view data,
browser profile, log.txt) -- unchanged from earlier versions.
macOS: ~/Library/Application Support/STL Editor, and ~/Library/Logs/STL Editor for the log.
Linux: $XDG_CONFIG_HOME, $XDG_DATA_HOME and $XDG_STATE_HOME (defaults ~/.config,
~/.local/share, ~/.local/state), each with an "STL Editor" folder inside.

Read at call time, so tests can change the platform and the environment.
"""
import os
import sys

APP = "STL Editor"


def _home():
    return os.path.expanduser("~")


def _xdg(name, default):
    return os.environ.get(name) or os.path.join(_home(), *default)


def _windows(var, fallback):
    return os.environ.get(var) or os.path.join(_home(), *fallback)


def config_dir():
    """Settings and downloaded engines."""
    if sys.platform == "win32":
        return os.path.join(_windows("APPDATA", ("AppData", "Roaming")), APP)
    if sys.platform == "darwin":
        return os.path.join(_home(), "Library", "Application Support", APP)
    return os.path.join(_xdg("XDG_CONFIG_HOME", (".config",)), APP)


def data_dir():
    """Autosave, the web view's data and the browser profile."""
    if sys.platform == "win32":
        return os.path.join(_windows("LOCALAPPDATA", ("AppData", "Local")), APP)
    if sys.platform == "darwin":
        return os.path.join(_home(), "Library", "Application Support", APP)
    return os.path.join(_xdg("XDG_DATA_HOME", (".local", "share")), APP)


def log_dir():
    """log.txt (the app's output when started without a console)."""
    if sys.platform == "win32":
        return data_dir()
    if sys.platform == "darwin":
        return os.path.join(_home(), "Library", "Logs", APP)
    return os.path.join(_xdg("XDG_STATE_HOME", (".local", "state")), APP)
