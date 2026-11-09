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
            "checking": STATE.get("checking"), "install_mode": install_mode(), "install": dict(INSTALL)}


# ------------------------------------------------------------------------------------------
# One-click install. The portable app downloads the new release zip, checks it, and swaps its
# own folder after it has closed (a small PowerShell script does the copy, then starts the new
# version). A copy that runs from a git checkout (python main.py) pulls instead.
# ------------------------------------------------------------------------------------------
import hashlib  # noqa: E402
import os  # noqa: E402
import platform  # noqa: E402
import shutil  # noqa: E402
import subprocess  # noqa: E402
import sys  # noqa: E402
import tarfile  # noqa: E402
import tempfile  # noqa: E402
import zipfile  # noqa: E402

ASSET = "STL-Editor-Windows-portable.zip"
EXE = "STL Editor.exe"
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
INSTALL = {"state": "idle", "progress": 0.0, "error": None, "src": None, "version": None, "mode": None}
WORK = os.path.join(tempfile.gettempdir(), "STL Editor update")


def install_mode():
    """'portable' (the built app), 'git' (a git checkout run with Python) or None (can't update itself)."""
    if getattr(sys, "frozen", False):
        return "portable"
    if os.path.isdir(os.path.join(ROOT, ".git")) and shutil.which("git"):
        return "git"
    return None


def asset_for(plat=None, machine=None):
    """The release archive for this system, or None when no build is published for it yet."""
    plat = sys.platform if plat is None else plat
    m = (platform.machine() if machine is None else machine).lower()
    if plat == "win32":
        return ASSET
    if plat == "darwin":
        if m in ("arm64", "aarch64"):
            return "STL-Editor-macOS-arm64.zip"
        if m in ("x86_64", "amd64"):
            return "STL-Editor-macOS-x86_64.zip"
        return None
    if plat.startswith("linux") and m in ("x86_64", "amd64"):
        return "STL-Editor-Linux-x86_64.tar.gz"
    return None


def find_payload(dest, plat=None):
    """The new app inside an unpacked archive: the folder holding STL Editor.exe on Windows,
    else the 'STL Editor.app' bundle (macOS) or the 'STL Editor' folder (Linux)."""
    plat = sys.platform if plat is None else plat
    if plat == "win32":
        return next((d for d, _, files in os.walk(dest) if EXE in files), None)
    if plat == "darwin":
        p = os.path.join(dest, "STL Editor.app")
        return p if os.path.isfile(os.path.join(p, "Contents", "MacOS", "STL Editor")) else None
    p = os.path.join(dest, "STL Editor")
    return p if os.path.isfile(os.path.join(p, "STL Editor")) else None


def _executable_in(payload):
    """The program file inside a payload, for the Unix execute bit (zip files lose it)."""
    if sys.platform == "darwin":
        return os.path.join(payload, "Contents", "MacOS", "STL Editor")
    return os.path.join(payload, "STL Editor")


def _extract(archive, dest):
    if archive.endswith(".tar.gz"):
        with tarfile.open(archive) as t:
            t.extractall(dest, filter="data")
    else:
        with zipfile.ZipFile(archive) as z:
            z.extractall(dest)


def app_dir():
    """The folder the running app lives in (the .app bundle on macOS, the folder on Linux)."""
    if not getattr(sys, "frozen", False):
        return ROOT
    folder = os.path.dirname(sys.executable)
    if sys.platform == "darwin":
        p = folder
        while os.path.dirname(p) != p and not p.endswith(".app"):
            p = os.path.dirname(p)
        if p.endswith(".app"):
            return p
    return folder


def _writable(folder):
    try:
        probe = os.path.join(folder, ".stle-update-probe")
        with open(probe, "w") as fh:
            fh.write("x")
        os.remove(probe)
        return True
    except OSError:
        return False


def _release():
    r = repo()
    req = urllib.request.Request(f"https://api.github.com/repos/{r}/releases/latest",
                                 headers={"Accept": "application/vnd.github+json", "User-Agent": f"STL-Editor/{__version__}"})
    with urllib.request.urlopen(req, timeout=15) as resp:
        return json.loads(resp.read().decode("utf-8"))


def _git(*args, timeout=120):
    flags = 0x08000000 if os.name == "nt" else 0   # CREATE_NO_WINDOW
    return subprocess.run(["git", "-C", ROOT, *args], capture_output=True, text=True, timeout=timeout, creationflags=flags)


def _install():
    INSTALL.update(state="downloading", progress=0.0, error=None, src=None)
    try:
        mode = install_mode()
        INSTALL["mode"] = mode
        if mode == "git":
            st = _git("status", "--porcelain", "--untracked-files=no")
            if st.returncode:
                raise RuntimeError(st.stderr.strip() or "git status failed")
            if st.stdout.strip():
                raise RuntimeError("This copy has changes of its own (git status isn't clean), so it won't pull over them. "
                                   "Commit or stash them, then try again.")
            r = _git("pull", "--ff-only")
            if r.returncode:
                raise RuntimeError((r.stderr or r.stdout).strip().splitlines()[-1] if (r.stderr or r.stdout).strip() else "git pull failed")
            INSTALL.update(state="ready", progress=1.0, version=STATE.get("latest"))
            return
        if mode != "portable":
            raise RuntimeError("This copy can't update itself. Download the new zip instead." if sys.platform == "win32"
                               else "This copy can't update itself. Download the new version from the website instead.")
        name = asset_for()
        if not name:
            raise RuntimeError("Updates for this computer come from the website for now. Download the new version there.")
        if not _writable(app_dir()):
            where = f"({app_dir()})"
            if sys.platform == "win32":
                raise RuntimeError(f"Windows won't let STL Editor change its own folder {where}. Move the STL Editor "
                                   "folder somewhere of yours, like Documents or the Desktop, or download the new zip.")
            raise RuntimeError(f"The system won't let STL Editor change its own folder {where}. Move the app "
                               "somewhere you own, like your home folder, or download the new version from the website.")
        rel = _release()
        latest = (rel.get("tag_name") or "").lstrip("vV")
        asset = next((a for a in rel.get("assets", []) if a.get("name") == name), None)
        if not asset:
            raise RuntimeError(f"The {latest} release has no {name}.")
        want = (asset.get("digest") or "").removeprefix("sha256:").lower()
        if not want:
            import re as _re
            m = _re.search(r"SHA-256 of " + _re.escape(name) + r":\s*`?([0-9a-f]{64})", rel.get("body") or "")
            want = m.group(1) if m else ""
        shutil.rmtree(WORK, ignore_errors=True)
        os.makedirs(WORK, exist_ok=True)
        zpath = os.path.join(WORK, name)
        req = urllib.request.Request(asset["browser_download_url"], headers={"User-Agent": f"STL-Editor/{__version__}"})
        h = hashlib.sha256()
        with urllib.request.urlopen(req, timeout=60) as resp, open(zpath, "wb") as out:
            total = int(resp.headers.get("Content-Length") or asset.get("size") or 0)
            got = 0
            while True:
                chunk = resp.read(1 << 16)
                if not chunk:
                    break
                out.write(chunk)
                h.update(chunk)
                got += len(chunk)
                if total:
                    INSTALL["progress"] = min(0.95, got / total * 0.95)
        if want and h.hexdigest() != want:
            raise RuntimeError("The download didn't match its checksum; nothing was changed. Try again later.")
        INSTALL["state"] = "unpacking"
        dest = os.path.join(WORK, "new")
        _extract(zpath, dest)
        src = find_payload(dest)
        if not src:
            raise RuntimeError(f"{EXE if sys.platform == 'win32' else 'The app'} isn't in the downloaded {name}.")
        if sys.platform != "win32":
            os.chmod(_executable_in(src), 0o755)   # zip and tar bring the execute bit back only this way
        INSTALL.update(state="ready", progress=1.0, src=src, version=latest)
    except Exception as e:
        INSTALL.update(state="error", error=str(e) or type(e).__name__)


def install_start():
    if INSTALL["state"] in ("downloading", "unpacking"):
        return INSTALL
    threading.Thread(target=_install, daemon=True).start()
    time.sleep(0.05)
    return INSTALL


UPDATER = r"""param([int]$ProcId, [string]$Src, [string]$Dst, [string]$Exe, [string]$ExeArgs, [string]$Log)
function Say($m) { Add-Content -Path $Log -Value ("{0:u} {1}" -f (Get-Date), $m) }
Say "waiting for STL Editor ($ProcId) to close"
try { Wait-Process -Id $ProcId -Timeout 90 -ErrorAction Stop } catch { }
Start-Sleep -Milliseconds 700
if ($Src) {
  foreach ($d in Get-ChildItem -LiteralPath $Src -Directory) {
    robocopy $d.FullName (Join-Path $Dst $d.Name) /MIR /R:5 /W:1 /NFL /NDL /NJH /NJS /NP | Out-Null
    Say "copied $($d.Name) ($LASTEXITCODE)"
  }
  foreach ($f in Get-ChildItem -LiteralPath $Src -File) { Copy-Item -LiteralPath $f.FullName -Destination $Dst -Force; Say "copied $($f.Name)" }
}
Say "starting $Exe $ExeArgs"
if ($ExeArgs) { Start-Process -FilePath $Exe -ArgumentList $ExeArgs -WorkingDirectory $Dst } else { Start-Process -FilePath $Exe -WorkingDirectory $Dst }
"""


POSIX_UPDATER = """#!/bin/sh
# Args: pid, new copy (or empty), folder to replace, log file, program, then its arguments.
pid="$1"; src="$2"; dst="$3"; log="$4"; exe="$5"
shift 5
say() { printf '%s %s\\n' "$(date)" "$*" >> "$log"; }
say "waiting for STL Editor ($pid) to close"
i=0
while kill -0 "$pid" 2>/dev/null && [ $i -lt 450 ]; do sleep 0.2; i=$((i + 1)); done
sleep 0.7
if [ -n "$src" ] && [ -e "$src" ]; then
  rm -rf "$dst.old"
  if mv "$dst" "$dst.old" && cp -R "$src" "$dst"; then
    rm -rf "$dst.old"; say "copied the new version"
  else
    say "copy failed, putting the old version back"
    rm -rf "$dst"; mv "$dst.old" "$dst"
  fi
fi
say "starting $exe"
"$exe" "$@" >/dev/null 2>&1 &
"""


def _restart_posix(app, copy_from=None):
    """macOS and Linux: a shell helper waits for this process, swaps the app, starts it again."""
    os.makedirs(WORK, exist_ok=True)
    script = os.path.join(WORK, "update.sh")
    with open(script, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(POSIX_UPDATER)
    if getattr(sys, "frozen", False):
        exe, extra = sys.executable, []
    else:
        exe, extra = sys.executable, [os.path.join(ROOT, "main.py")]
    subprocess.Popen(["sh", script, str(os.getpid()), copy_from or "", app_dir(), os.path.join(WORK, "update.log"),
                      exe, *extra], start_new_session=True, close_fds=True,
                     stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    _quit(app)


def _quit(app):
    # unsaved work is kept for Recover Unsaved Work, then the app closes
    app.discard_on_exit = False
    from . import server
    server.shutdown(app)
    threading.Timer(0.3, lambda: os._exit(0)).start()


def restart(app, copy_from=None):
    """Close the app and start it again (after copying a new version over it, if `copy_from`)."""
    if sys.platform != "win32":
        return _restart_posix(app, copy_from)
    os.makedirs(WORK, exist_ok=True)
    script = os.path.join(WORK, "update.ps1")
    with open(script, "w", encoding="utf-8-sig") as fh:
        fh.write(UPDATER)
    if getattr(sys, "frozen", False):
        exe, args = os.path.join(app_dir(), EXE), ""
    else:
        py = sys.executable
        pyw = os.path.join(os.path.dirname(py), "pythonw.exe")
        exe, args = (pyw if os.path.isfile(pyw) else py), f'"{os.path.join(ROOT, "main.py")}"'
    flags = 0x00000008 | 0x00000200 | 0x08000000   # DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW
    subprocess.Popen(["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-File", script,
                      "-ProcId", str(os.getpid()), "-Src", copy_from or "", "-Dst", app_dir(), "-Exe", exe,
                      "-ExeArgs", args, "-Log", os.path.join(WORK, "update.log")],
                     creationflags=flags, close_fds=True)
    _quit(app)


def install_apply(app):
    if INSTALL["state"] != "ready":
        raise RuntimeError("The update isn't downloaded yet.")
    restart(app, INSTALL["src"] if INSTALL.get("mode") == "portable" else None)
