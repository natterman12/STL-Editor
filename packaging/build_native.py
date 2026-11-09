"""Builds the desktop STL Editor for Linux or for Apple-silicon Macs.

    py -3.14 packaging/build_native.py --target linux        (run on Linux)
    py -3.14 packaging/build_native.py --target mac-arm64    (run on an Apple-silicon Mac)

PyInstaller cannot cross-compile, so each target must be built on its own system.
Windows is built by packaging/build_portable.ps1 instead (this script refuses Windows).

Output (in dist/, the version comes from stleditor.__version__):
  linux      -> STL-Editor-Linux-x86_64.tar.gz   (top folder "STL Editor/")
  mac-arm64  -> STL-Editor-macOS-arm64.zip      (contains "STL Editor.app")

The Linux build uses the Qt back-end of pywebview, so PyQt6 and PyQt6-WebEngine must be
installed in the build environment (they are not in requirements.txt).
"""
import argparse
import importlib.util
import os
import platform
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
APP = "STL Editor"
BUNDLE_ID = "com.natterman12.stleditor"

TARGETS = {
    # target: (label used in the output name, system the build must run on)
    "linux": ("Linux-x86_64", "linux"),
    "mac-arm64": ("macOS-arm64", "darwin"),
}

# what goes into the build from the repository (same list as build_portable.ps1)
COPY_ITEMS = ["main.py", "stleditor", "web", "stl_editor.ico", "USER_GUIDE.md", "LICENSE", "packaging"]
# the web edition's JavaScript engine, fonts and libraries are only used in the browser
WEB_DROP = [
    "engine", "fonts", os.path.join("vendor", "manifold"),
    os.path.join("vendor", "fflate.module.js"), os.path.join("vendor", "FFLATE-LICENSE.txt"),
    os.path.join("vendor", "opentype.module.js"), os.path.join("vendor", "OPENTYPE-LICENSE.txt"),
    "sw.js", "manifest.webmanifest", "icon-192.png", "icon-512.png", "icon-maskable.png",
    os.path.join("vendor", "support-fins", "engine.esm.js"), os.path.join("vendor", "support-fins", "version.js"),
]
# never used at run time (the same list as build_portable.ps1, minus the Windows-only clr hidden import)
EXCLUDES = [
    "matplotlib", "IPython", "pandas", "PyQt5", "PyQt6", "PySide2", "PySide6", "torch", "cv2", "pytest", "notebook",
    "tkinter", "_tkinter", "PIL.ImageTk", "PIL._imagingtk", "PIL._avif", "PIL.AvifImagePlugin",
    "cryptography", "bcrypt", "zstandard",
    "scipy.stats", "scipy.signal", "scipy.interpolate", "scipy.integrate", "scipy.io", "scipy.odr",
    "scipy.cluster", "scipy.datasets", "scipy.differentiate", "scipy.fftpack", "scipy.misc",
    "skimage.data", "skimage.feature", "skimage.filters", "skimage.segmentation", "skimage.restoration",
    "skimage.registration", "skimage.io", "skimage.color", "skimage.draw", "skimage.exposure", "skimage.future",
    "skimage.graph", "skimage.metrics", "skimage.transform", "skimage.morphology", "skimage.util",
]
DOCS = [("READ ME FIRST.txt", None), ("USER_GUIDE.md", "User guide.md"), ("LICENSE", "LICENSE.txt")]


class BuildError(Exception):
    """A problem the user can fix; the message is printed as-is."""


def read_version(root=ROOT):
    with open(os.path.join(root, "stleditor", "__init__.py"), encoding="utf-8") as f:
        m = re.search(r'__version__ = "(.+?)"', f.read())
    if not m:
        raise BuildError("Could not find __version__ in stleditor/__init__.py")
    return m.group(1)


def asset_name(target, version):
    """The file this script writes to dist/ for a target."""
    label, _ = TARGETS[target]
    ext = "tar.gz" if target == "linux" else "zip"
    return f"STL-Editor-{label}.{ext}"


def check_target(target, plat=None, machine=None):
    """Raise BuildError unless this system can build the target. Arguments default to this machine."""
    plat = sys.platform if plat is None else plat
    machine = platform.machine() if machine is None else machine
    if plat == "win32":
        raise BuildError("Windows is not built by this script. Use packaging/build_portable.ps1 on Windows "
                         "(this script builds Linux and macOS only).")
    if target not in TARGETS:
        raise BuildError(f"Unknown target {target!r}; use one of: {', '.join(TARGETS)}")
    _, need = TARGETS[target]
    if not plat.startswith(need):
        raise BuildError(f"The {target} build must run on {'Linux' if need == 'linux' else 'macOS'} "
                         f"(this system is {plat}). PyInstaller cannot cross-compile.")
    if target == "mac-arm64" and machine.lower() not in ("arm64", "aarch64"):
        raise BuildError(f"The mac-arm64 build needs an Apple-silicon Mac (this one reports {machine}).")


def missing_qt_module():
    """Name of the module the Linux build needs but cannot find, or None."""
    for mod in ("webview", "PyQt6.QtWebEngineWidgets"):
        try:
            if importlib.util.find_spec(mod) is None:
                return mod
        except ImportError:
            return mod
    return None


def pyinstaller_args(target, src, work, icns=None):
    """The PyInstaller command line (after the python executable) for a target. Pure: runs nothing.

    src: the staged clean copy of the repository (PyInstaller runs from here).
    work: the build folder; output goes to work/dist, work/build, and the spec file to work.
    icns: path to a .icns icon for the Mac bundle, or None.
    """
    sep = os.pathsep  # ':' on Linux and macOS, ';' on Windows (only used for --add-data)
    args = [
        "-m", "PyInstaller", "--noconfirm", "--clean", "--name", APP,
        "--add-data", f"{os.path.join(src, 'web')}{sep}web",
        "--collect-data", "trimesh",
        "--collect-all", "manifold3d",
        "--collect-all", "fast_simplification",
        "--collect-all", "py_mini_racer",
        "--collect-submodules", "skimage.measure",
        "--collect-submodules", "fontTools.ttLib.tables",
        "--collect-all", "webview",
    ]
    excludes = EXCLUDES
    if target == "mac-arm64":
        args += ["--windowed", "--osx-bundle-identifier", BUNDLE_ID, "--argv-emulation"]
        if icns:
            args += ["--icon", icns]
    else:
        # the Qt back-end of pywebview; PyQt6 is needed here, so it is not excluded
        args += ["--hidden-import", "webview.platforms.qt",
                 "--hidden-import", "PyQt6.QtWebEngineCore",
                 "--hidden-import", "PyQt6.QtWebEngineWidgets"]
        excludes = [e for e in EXCLUDES if e != "PyQt6"]
    for mod in excludes:
        args += ["--exclude-module", mod]
    args += ["--distpath", os.path.join(work, "dist"), "--workpath", os.path.join(work, "build"),
             "--specpath", work, "main.py"]
    return args


def stage(root, work, repo=""):
    """Copy the repository into work/src without caches, user files or the web-only engine."""
    src = os.path.join(work, "src")
    os.makedirs(src)
    ignore = shutil.ignore_patterns("__pycache__")
    for item in COPY_ITEMS:
        s = os.path.join(root, item)
        d = os.path.join(src, item)
        if os.path.isdir(s):
            shutil.copytree(s, d, ignore=ignore)
        else:
            shutil.copy2(s, d)
    for item in WEB_DROP:
        p = os.path.join(src, "web", item)
        if os.path.isdir(p):
            shutil.rmtree(p)
        elif os.path.exists(p):
            os.remove(p)
    if repo:
        # bake the GitHub repository for update checks into this build
        init = os.path.join(src, "stleditor", "__init__.py")
        with open(init, encoding="utf-8") as f:
            text = f.read()
        text = re.sub(r'UPDATE_REPO = ".*"', f'UPDATE_REPO = "{repo}"', text)
        with open(init, "w", encoding="utf-8") as f:
            f.write(text)
        print(f"Update checks: github.com/{repo}")
    return src


def make_icns(ico, out):
    """Write a .icns from the .ico with Pillow. Returns False (and says why) if that is not possible."""
    try:
        from PIL import Image
    except ImportError:
        print("Icon: Pillow is not installed, so the .app is built without an icon.")
        return False
    try:
        im = Image.open(ico)
        biggest = max(im.size)
        sizes = [s for s in [(16, 16), (32, 32), (64, 64), (128, 128), (256, 256), (512, 512), (1024, 1024)]
                 if s[0] <= biggest]
        im.save(out, format="ICNS", sizes=sizes)
        return True
    except Exception as e:  # noqa: BLE001 - an icon is optional; report and go on
        print(f"Icon: could not convert stl_editor.ico to .icns ({e}); the .app is built without an icon.")
        return False


def run(cmd, cwd=None):
    print("+ " + " ".join(f'"{c}"' if " " in c else c for c in cmd), flush=True)
    subprocess.run(cmd, cwd=cwd, check=True)


def build(target, repo="", out_dir=None):
    check_target(target)
    version = read_version()
    if target == "linux":
        missing = missing_qt_module()
        if missing:
            raise BuildError("The Linux build needs PyQt6 and PyQt6-WebEngine in this Python environment "
                             f"(pip install PyQt6 PyQt6-WebEngine). Missing: {missing}.")
    out_dir = out_dir or os.path.join(ROOT, "dist")
    work = os.path.join(tempfile.gettempdir(), "stle-build-native")
    if os.path.isdir(work):
        shutil.rmtree(work)
    os.makedirs(work)
    print(f"Building STL Editor {version} for {target}")
    src = stage(ROOT, work, repo)

    icns = None
    if target == "mac-arm64":
        icns_path = os.path.join(work, "stl_editor.icns")
        icns = icns_path if make_icns(os.path.join(src, "stl_editor.ico"), icns_path) else None

    run([sys.executable, *pyinstaller_args(target, src, work, icns)], cwd=src)
    dist = os.path.join(work, "dist")
    if target == "linux":
        app_dir = os.path.join(dist, APP)
        exe = os.path.join(app_dir, APP)
        docs_dir = app_dir
    else:
        app_dir = os.path.join(dist, APP + ".app")
        exe = os.path.join(app_dir, "Contents", "MacOS", APP)
        docs_dir = os.path.join(app_dir, "Contents", "Resources")
    if not os.path.exists(exe):
        raise BuildError(f"PyInstaller did not produce {exe}")

    # licences and the readme, as in the Windows build
    run([sys.executable, os.path.join(src, "packaging", "collect_licenses.py"), os.path.join(docs_dir, "LICENSES")])
    for name, dest in DOCS:
        shutil.copy2(os.path.join(src, "packaging", "READ ME FIRST.txt") if name.startswith("READ")
                     else os.path.join(src, name), os.path.join(docs_dir, dest or name))

    # run every engine tool once in the built app; a missing library fails the build here
    run([sys.executable, os.path.join(src, "packaging", "smoke_test.py"), exe])

    os.makedirs(out_dir, exist_ok=True)
    out = os.path.join(out_dir, asset_name(target, version))
    if os.path.exists(out):
        os.remove(out)
    if target == "linux":
        with tarfile.open(out, "w:gz") as tf:  # symlinks are stored as links
            tf.add(app_dir, arcname=APP)
    else:
        # ditto keeps symlinks, resource forks and code signatures; zipfile/tar do not
        run(["ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", app_dir, out])
    mb = os.path.getsize(out) / 1048576
    print(f"BUILD OK: {out} ({mb:.1f} MB)")
    return out


def main(argv=None):
    p = argparse.ArgumentParser(description="Build the STL Editor for Linux or macOS (Apple silicon) with PyInstaller. "
                                            "Windows: use packaging/build_portable.ps1.")
    p.add_argument("--target", required=True, choices=sorted(TARGETS),
                   help="linux (run on Linux) or mac-arm64 (run on an Apple-silicon Mac)")
    p.add_argument("--repo", default="", help="GitHub owner/name to bake in for update checks, e.g. natterman12/STL-Editor")
    p.add_argument("--out", default="", help="output folder (default: dist/ in the repository)")
    a = p.parse_args(argv)
    try:
        build(a.target, a.repo, a.out or None)
    except BuildError as e:
        print(f"ERROR: {e}", file=sys.stderr)
        return 2
    except subprocess.CalledProcessError as e:
        print(f"ERROR: a build step failed (exit {e.returncode}): {' '.join(map(str, e.cmd))}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
