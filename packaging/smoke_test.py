"""Smoke-test a built STL Editor.exe: start it as a headless server and run every engine tool once.

    python packaging/smoke_test.py "<path to STL Editor.exe>"

The build script runs this after every build, so a library left out of the package by mistake
fails the build instead of reaching users. Uses only the standard library.
"""
import json
import math
import os
import socket
import struct
import subprocess
import sys
import tempfile
import time
import urllib.request


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def sphere_stl(r=15.0, n=24, drop=0):
    """Binary STL of a UV sphere (optionally with a few triangles missing)."""
    tris = []
    for i in range(n):
        t0, t1 = math.pi * i / n, math.pi * (i + 1) / n
        for j in range(2 * n):
            p0, p1 = 2 * math.pi * j / (2 * n), 2 * math.pi * (j + 1) / (2 * n)
            P = lambda t, p: (r * math.sin(t) * math.cos(p), r * math.sin(t) * math.sin(p), r * math.cos(t) + r)
            a, b, c, d = P(t0, p0), P(t1, p0), P(t1, p1), P(t0, p1)
            if i != 0:
                tris.append((a, b, d))
            if i != n - 1:
                tris.append((b, c, d))
    tris = tris[drop:]
    out = bytearray(b"smoke".ljust(80, b" ") + struct.pack("<I", len(tris)))
    for t in tris:
        out += struct.pack("<3f", 0, 0, 0)
        for v in t:
            out += struct.pack("<3f", *v)
        out += b"\0\0"
    return bytes(out)


def main(exe):
    port = free_port()
    env = dict(os.environ, STLEDITOR_PORT=str(port))
    cmd = [sys.executable, exe] if exe.endswith(".py") else [exe]
    proc = subprocess.Popen(cmd + ["--server", "--no-autosave"], env=env)
    base = f"http://127.0.0.1:{port}/api/"
    tmp = tempfile.mkdtemp()

    def call(route, data=None, raw=None, query=""):
        body = raw if raw is not None else (None if data is None else json.dumps(data).encode())
        req = urllib.request.Request(base + route + query, data=body, method="GET" if body is None else "POST")
        with urllib.request.urlopen(req, timeout=180) as r:
            out = json.load(r)
        if isinstance(out, dict) and out.get("error"):
            raise RuntimeError(f"{route}: {out['error']}")
        return out

    def add(ftype, params):
        return call("feature/add", {"type": ftype, "params": params})["feature"]["id"]

    try:
        for _ in range(90):
            try:
                call("state")
                break
            except Exception:
                time.sleep(1)
        else:
            raise RuntimeError("the app never answered")
        steps = []

        def step(name, fn):
            t = time.time()
            fn()
            steps.append(f"{name} ({time.time() - t:.1f}s)")

        st = {}
        step("import", lambda: st.update(s=call("upload", raw=sphere_stl(), query="?name=ball.stl")["state"]["bodies"][0]["id"]))
        S = st["s"]
        step("hollow", lambda: add("hollow", {"body": S, "thickness": 2}))
        step("simplify", lambda: add("simplify", {"body": S, "ratio": 0.5}))
        step("remesh", lambda: add("remesh", {"body": S, "edge": 2}))
        step("sculpt", lambda: add("sculpt", {"body": S, "strokes": [{"mode": "inflate", "radius": 5, "strength": 0.5,
                                                                         "points": [[0, 0, 30], [4, 0, 29]]}]}))
        step("import broken", lambda: st.update(b=call("upload", raw=sphere_stl(10, 16, drop=6), query="?name=broken.stl")["state"]["bodies"][-1]["id"]))
        step("repair rebuild", lambda: add("repair", {"body": st["b"], "mode": "rebuild"}))
        step("box", lambda: st.update(x=add("primitive", {"shape": "box", "length": 300, "width": 40, "height": 30})))
        X = st["x"]
        step("sketch + cut", lambda: add("extrude", {"sketch": add("sketch", {"plane": {"origin": [0, 0, 30], "normal": [0, 0, 1]},
                                                                              "entities": [{"id": "c", "type": "circle", "cx": 0, "cy": 0, "r": 6}]}),
                                                     "distance": 5, "operation": "cut"}))
        step("hole", lambda: add("hole", {"points": [[60, 0, 30]], "normal": [0, 0, 1], "diameter": 5}))
        step("resize hole", lambda: add("resizehole", {"body": X, "point": [62.5, 0, 15], "diameter": 6}))
        step("thread", lambda: add("thread", {"kind": "internal", "diameter": 8, "pitch": 1.25, "length": 10,
                                              "plane": {"origin": [-60, 0, 30], "normal": [0, 0, 1]}, "operation": "cut"}))
        step("offset", lambda: add("offset", {"bodies": [st["b"]], "distance": 0.3}))
        step("lay flat", lambda: add("layflat", {"bodies": [X], "auto": True}))
        step("fit to bed", lambda: add("bedsplit", {"body": X, "printer": "bambu_a1mini", "connector": "pins"}))
        frame = [[0, 0, 15], [1, 0, 0], [0, 1, 0], [0, 0, 1]]
        step("project", lambda: call("sketch_op", {"op": "project", "entities": [], "frame": frame}))
        step("fonts + text", lambda: call("sketch_op", {"op": "text", "entities": [], "text": "Hi 3D",
                                                        "font": call("fonts", {})["default"], "size": 8}))
        for fmt in ("stl", "3mf", "obj"):
            path = os.path.join(tmp, "out." + fmt)
            step("export " + fmt, lambda p=path, f=fmt: call("export", {"format": f, "path": p}))
            step("reopen " + fmt, lambda p=path: call("open", {"path": p, "force": True}))
        proj = os.path.join(tmp, "p.stlx")
        step("save", lambda: call("save", {"path": proj}))
        step("open project", lambda: call("open", {"path": proj, "force": True}))
        step("update status", lambda: call("update/status", {}))
        print("SMOKE OK: " + ", ".join(steps))
        return 0
    finally:
        try:
            call("quit", {})
        except Exception:
            pass
        time.sleep(1)
        if proc.poll() is None:
            proc.kill()


if __name__ == "__main__":
    sys.exit(main(sys.argv[1]))
