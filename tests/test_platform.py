"""Send to Slicer / To STEP discovery on Windows, macOS and Linux, without the apps installed."""
import os

from stleditor import server as S


def fake(paths=(), cmds=(), flatpaks=()):
    """exists(path), which(cmd) and flatpak(app_id) backed by sets."""
    paths, cmds, flatpaks = set(paths), set(cmds), set(flatpaks)
    return dict(exists=lambda p: p in paths, which=lambda c: f"/usr/bin/{c}" if c in cmds else None,
                flatpak=lambda a: a in flatpaks)


def test_windows_finds_exe():
    exe = r"C:\Program Files\Bambu Studio\bambu-studio.exe"
    assert S.find_slicer("bambu", "win32", **fake([exe])) == [exe]
    assert S.find_slicer("orca", "win32", **fake([exe])) is None


def test_windows_ignores_mac_and_linux_installs():
    f = fake(["/Applications/BambuStudio.app"], ["bambu-studio"], ["com.bambulab.BambuStudio"])
    assert S.find_slicer("bambu", "win32", **f) is None


def test_mac_applications_folder():
    f = fake(["/Applications/BambuStudio.app"])
    assert S.find_slicer("bambu", "darwin", **f) == ["open", "-a", "/Applications/BambuStudio.app"]
    assert S.find_slicer("orca", "darwin", **f) is None


def test_mac_user_applications_folder():
    app = os.path.join(os.path.expanduser("~"), "Applications", "PrusaSlicer.app")
    assert S.find_slicer("prusa", "darwin", **fake([app])) == ["open", "-a", app]
    orca = "/Applications/OrcaSlicer.app"
    assert S.find_slicer("orca", "darwin", **fake([orca])) == ["open", "-a", orca]


def test_linux_path_commands():
    assert S.find_slicer("bambu", "linux", **fake(cmds=["bambu-studio"])) == ["/usr/bin/bambu-studio"]
    assert S.find_slicer("orca", "linux", **fake(cmds=["orcaslicer"])) == ["/usr/bin/orcaslicer"]
    assert S.find_slicer("orca", "linux", **fake(cmds=["orca-slicer"])) == ["/usr/bin/orca-slicer"]
    assert S.find_slicer("prusa", "linux", **fake(cmds=["prusa-slicer"])) == ["/usr/bin/prusa-slicer"]
    assert S.find_slicer("bambu", "linux", **fake()) is None


def test_linux_flatpaks():
    f = fake(flatpaks=["com.bambulab.BambuStudio", "io.github.softfever.OrcaSlicer", "com.prusa3d.PrusaSlicer"])
    assert S.find_slicer("bambu", "linux", **f) == ["flatpak", "run", "com.bambulab.BambuStudio"]
    assert S.find_slicer("orca", "linux", **f) == ["flatpak", "run", "io.github.softfever.OrcaSlicer"]
    assert S.find_slicer("prusa", "linux", **f) == ["flatpak", "run", "com.prusa3d.PrusaSlicer"]


def test_linux_prefers_path_over_flatpak():
    f = fake(cmds=["prusa-slicer"], flatpaks=["com.prusa3d.PrusaSlicer"])
    assert S.find_slicer("prusa", "linux", **f) == ["/usr/bin/prusa-slicer"]


def test_unknown_key():
    assert S.find_slicer("nope", "linux", **fake(cmds=["nope"])) is None


def test_to_step_only_on_windows():
    cli = os.path.join(S.SOLIDCRAFT, "solidcraft-cli.exe")
    assert S.step_available("win32", lambda p: p == cli) is True
    assert S.step_available("win32", lambda p: False) is False
    assert S.step_available("darwin", lambda p: True) is False
    assert S.step_available("linux", lambda p: True) is False


def test_r_apps_shape():
    r = S.r_apps(None, {})
    assert r["ok"] and set(r["apps"]) >= {"bambu", "orca", "prusa", "solidcraft"}
    assert isinstance(r["solidcraft_cli"], bool) and isinstance(r["step_supported"], bool)
    assert r["platform"]
