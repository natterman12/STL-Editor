"""STL Editor launcher: starts the local engine and opens the desktop window."""
import os
import shutil
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from stleditor import paths  # noqa: E402  (stdlib only, so it is safe before the stdout check)

if sys.stdout is None or sys.stderr is None:  # pythonw: keep a log instead of a console
    _logdir = paths.log_dir()
    os.makedirs(_logdir, exist_ok=True)
    sys.stdout = sys.stderr = open(os.path.join(_logdir, "log.txt"), "a", buffering=1, encoding="utf-8")

from stleditor import server  # noqa: E402

APP_NAME = "STL Editor"


def gui_candidates(plat):
    """pywebview back-ends to try, in order. Empty: no desktop window, use a browser window."""
    if plat == "win32":
        return ["edgechromium"]   # never the old IE engine, which shows a blank page
    if plat == "darwin":
        return ["cocoa"]
    if plat.startswith("linux"):
        return ["qt", "gtk"]
    return []


_MAC_BROWSERS = [
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
]
_LINUX_BROWSERS = ["google-chrome", "chromium", "chromium-browser", "microsoft-edge"]
_WIN_BROWSERS = [
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    os.path.expandvars(r"%LOCALAPPDATA%\Google\Chrome\Application\chrome.exe"),
]


def browser_exe(plat=None, exists=os.path.isfile, which=shutil.which):
    """A Chromium-based browser to open the app in its own window, or None."""
    plat = sys.platform if plat is None else plat
    if plat == "win32":
        return next((c for c in _WIN_BROWSERS if exists(c)), None)
    if plat == "darwin":
        return next((c for c in _MAC_BROWSERS if exists(c)), None)
    for name in _LINUX_BROWSERS:
        found = which(name)
        if found:
            return found
    return None


def _open_window(app, url, gui, storage):
    import webview

    def on_closing():
        if app.doc.dirty:
            ok = app.window.create_confirmation_dialog(
                APP_NAME, "You have unsaved changes. Close anyway?")
            if not ok:
                return False
        server.shutdown(app)
        return True

    app.window = webview.create_window(APP_NAME, url, width=1500, height=940,
                                       min_size=(900, 600), text_select=False)
    app.window.events.closing += on_closing
    webview.start(gui=gui, debug="--debug" in sys.argv, private_mode=False, storage_path=storage)


def main():
    app = server.App()
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    if args and os.path.isfile(args[0]):
        try:
            server.r_open(app, {"path": os.path.abspath(args[0]), "force": True})
        except Exception as e:  # show the app anyway
            print("could not open", args[0], e)
    if "--no-autosave" not in sys.argv:
        from stleditor.autosave import Autosaver
        from stleditor import settings
        try:
            app.autosaver = Autosaver(app, float(settings.get("autosave_interval") or 60))
        except Exception as e:
            print("autosave unavailable:", e, flush=True)
    if "--server" not in sys.argv:
        from stleditor import updates
        updates.check_in_background()
        from stleditor import supportfins
        supportfins.check_in_background()   # a newer Support Fins engine, used from the next start
    port = int(os.environ.get("STLEDITOR_PORT", "0"))
    httpd, port = server.serve(app, port)
    url = f"http://127.0.0.1:{port}/"
    print(f"{APP_NAME} running at {url}", flush=True)

    if "--server" in sys.argv:
        while True:
            time.sleep(3600)

    if "--browser" not in sys.argv:
        storage = os.path.join(paths.data_dir(), "webview")
        os.makedirs(storage, exist_ok=True)
        for gui in gui_candidates(sys.platform) or [None]:
            try:
                _open_window(app, url, gui, storage)
                os._exit(0)
            except Exception as e:  # this window back-end isn't usable: try the next one
                print(f"desktop window ({gui or 'default'}) unavailable:", e, flush=True)
                app.window = None
        print("desktop window unavailable, using browser window", flush=True)

    exe = browser_exe()
    if exe:
        import subprocess
        profile = os.path.join(paths.data_dir(), "browser")
        subprocess.Popen([exe, f"--app={url}", f"--user-data-dir={profile}",
                          "--window-size=1500,940", "--no-first-run"])
    else:
        import webbrowser
        webbrowser.open(url)
    # browser mode: quit when the page stops sending heartbeats
    server.HEARTBEAT = time.time() + 60
    while True:
        time.sleep(2)
        if time.time() - getattr(server, "HEARTBEAT", time.time()) > 15:
            app.discard_on_exit = False  # browser closed without asking: keep work recoverable
            server.shutdown(app)
            os._exit(0)


if __name__ == "__main__":
    main()
