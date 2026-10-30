"""Update notice: asks GitHub Releases for the newest version, at most once a day.

Only the public releases page of the configured repository is fetched; nothing about the user
or their files is sent."""
from __future__ import annotations

import json
import re
import threading
import time
import urllib.request

from . import settings
from . import __version__, UPDATE_REPO

STATE = {"checking": False, "latest": None, "url": None, "notes": None, "error": None, "checked": None}


def repo() -> str:
    return (settings.get("update_repo") or UPDATE_REPO or "").strip().strip("/")


def _ver(s):
    nums = re.findall(r"\d+", s or "")
    return tuple(int(n) for n in nums[:4]) or (0,)


def is_newer(latest, current=__version__):
    return _ver(latest) > _ver(current)


def check(force=False):
    r = repo()
    if not r or not settings.get("update_check", True) and not force:
        STATE.update(error=None if r else "No update source is set.")
        return STATE
    if not force and settings.get("update_last", 0) > time.time() - 20 * 3600:
        cached = settings.get("update_cache") or {}
        STATE.update(latest=cached.get("latest"), url=cached.get("url"), notes=cached.get("notes"),
                     checked=settings.get("update_last"))
        return STATE
    STATE["checking"] = True
    try:
        req = urllib.request.Request(f"https://api.github.com/repos/{r}/releases/latest",
                                     headers={"Accept": "application/vnd.github+json",
                                              "User-Agent": f"STL-Editor/{__version__}"})
        with urllib.request.urlopen(req, timeout=8) as resp:
            data = json.loads(resp.read().decode("utf-8"))
        latest = (data.get("tag_name") or data.get("name") or "").lstrip("vV")
        url = data.get("html_url") or f"https://github.com/{r}/releases/latest"
        notes = (data.get("body") or "")[:1500]
        STATE.update(latest=latest, url=url, notes=notes, error=None, checked=time.time())
        settings.set("update_last", time.time())
        settings.set("update_cache", {"latest": latest, "url": url, "notes": notes})
    except Exception as e:
        STATE.update(error=f"Couldn't reach GitHub ({type(e).__name__}).")
    finally:
        STATE["checking"] = False
    return STATE


def check_in_background():
    threading.Thread(target=check, daemon=True).start()


def status():
    latest = STATE.get("latest")
    return {"current": __version__, "repo": repo(), "enabled": settings.get("update_check", True),
            "latest": latest, "url": STATE.get("url"), "notes": STATE.get("notes"),
            "available": bool(latest and is_newer(latest)),
            "skipped": settings.get("update_skip") == latest, "error": STATE.get("error"),
            "checking": STATE.get("checking")}
