"""STL Editor launcher: starts the local engine and opens the desktop window."""
import os
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

if sys.stdout is None or sys.stderr is None:  # pythonw: keep a log instead of a console
    _logdir = os.path.join(os.environ.get("LOCALAPPDATA", os.path.expanduser("~")), "STL Editor")
    os.makedirs(_logdir, exist_ok=True)
    sys.stdout = sys.stderr = open(os.path.join(_logdir, "log.txt"), "a", buffering=1, encoding="utf-8")

from stleditor import server  # noqa: E402

APP_NAME = "STL Editor"


def _chrome_like():
    cands = [
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        os.path.expandvars(r"%LOCALAPPDATA%\Google\Chrome\Application\chrome.exe"),
    ]
    return next((c for c in cands if os.path.isfile(c)), None)


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
    port = int(os.environ.get("STLEDITOR_PORT", "0"))
    httpd, port = server.serve(app, port)
    url = f"http://127.0.0.1:{port}/"
    print(f"{APP_NAME} running at {url}", flush=True)

    if "--server" in sys.argv:
        while True:
            time.sleep(3600)

    if "--browser" not in sys.argv:
        try:
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
            storage = os.path.join(os.environ.get("LOCALAPPDATA", os.path.expanduser("~")),
                                   "STL Editor", "webview")
            os.makedirs(storage, exist_ok=True)
            # edgechromium only: never fall back to the old IE engine, which shows a blank page
            webview.start(gui="edgechromium" if sys.platform == "win32" else None,
                          debug="--debug" in sys.argv, private_mode=False, storage_path=storage)
            os._exit(0)
        except Exception as e:  # no WebView2 / pywebview: fall back to an app-mode browser window
            print("desktop window unavailable, using browser window:", e, flush=True)
            app.window = None

    exe = _chrome_like()
    if exe:
        import subprocess
        profile = os.path.join(os.environ.get("LOCALAPPDATA", os.path.expanduser("~")),
                               "STL Editor", "browser")
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
