"""packaging/build_native.py: asset names, PyInstaller command lines and refusals. Runs no PyInstaller."""
import importlib.util
import os

import pytest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("build_native", os.path.join(ROOT, "packaging", "build_native.py"))
B = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(B)


def test_asset_names_per_target():
    assert B.asset_name("linux", "1.16.0") == "STL-Editor-Linux-x86_64.tar.gz"
    assert B.asset_name("mac-arm64", "1.16.0") == "STL-Editor-macOS-arm64.zip"


def test_version_is_read_from_package():
    assert B.read_version() == "1.16.0"


def test_linux_command_line():
    args = B.pyinstaller_args("linux", "/s/src", "/w")
    assert args[:2] == ["-m", "PyInstaller"]
    assert args[args.index("--name") + 1] == "STL Editor"
    assert args[-1] == "main.py"
    assert "--windowed" not in args
    assert "--argv-emulation" not in args
    assert "--icon" not in args
    assert "--osx-bundle-identifier" not in args
    assert args[args.index("--add-data") + 1] == os.path.join("/s/src", "web") + os.pathsep + "web"
    # the Qt back-end is kept: PyQt6 is imported, not excluded
    assert "PyQt6.QtWebEngineWidgets" in args
    assert "webview.platforms.qt" in args
    excluded = [args[i + 1] for i, a in enumerate(args) if a == "--exclude-module"]
    assert "PyQt6" not in excluded
    assert "PyQt5" in excluded and "matplotlib" in excluded
    assert "clr" not in args
    assert args[args.index("--distpath") + 1] == os.path.join("/w", "dist")
    assert args[args.index("--specpath") + 1] == "/w"


def test_mac_command_line_without_icon():
    args = B.pyinstaller_args("mac-arm64", "/s/src", "/w", icns=None)
    assert "--windowed" in args
    assert args[args.index("--osx-bundle-identifier") + 1] == "com.natterman12.stleditor"
    assert "--argv-emulation" in args
    assert "--icon" not in args
    assert "PyQt6.QtWebEngineWidgets" not in args
    excluded = [args[i + 1] for i, a in enumerate(args) if a == "--exclude-module"]
    assert "PyQt6" in excluded and "PyQt5" in excluded
    assert args[-1] == "main.py"


def test_mac_command_line_with_icon():
    args = B.pyinstaller_args("mac-arm64", "/s/src", "/w", icns="/w/stl_editor.icns")
    assert args[args.index("--icon") + 1] == "/w/stl_editor.icns"


def test_windows_is_refused():
    with pytest.raises(B.BuildError, match="build_portable.ps1"):
        B.check_target("linux", plat="win32", machine="AMD64")
    with pytest.raises(B.BuildError, match="build_portable.ps1"):
        B.check_target("mac-arm64", plat="win32", machine="AMD64")


def test_target_must_match_system():
    with pytest.raises(B.BuildError, match="cannot cross-compile"):
        B.check_target("linux", plat="darwin", machine="arm64")
    with pytest.raises(B.BuildError, match="must run on macOS"):
        B.check_target("mac-arm64", plat="linux", machine="x86_64")
    with pytest.raises(B.BuildError, match="Apple-silicon"):
        B.check_target("mac-arm64", plat="darwin", machine="x86_64")


def test_matching_systems_accepted():
    B.check_target("linux", plat="linux", machine="x86_64")
    B.check_target("mac-arm64", plat="darwin", machine="arm64")


def test_unknown_target_refused():
    with pytest.raises(B.BuildError, match="Unknown target"):
        B.check_target("freebsd", plat="linux", machine="x86_64")


def test_help_exits_cleanly(capsys):
    with pytest.raises(SystemExit) as e:
        B.main(["--help"])
    assert e.value.code == 0
    assert "--target" in capsys.readouterr().out
