"""Times four operations on dev/bench/test_mesh.stl through the 1.x Document (median of 3).

Prints one JSON object of seconds on stdout: {"import": s, "combine_join": s, "combine_cut": s,
"split": s, "repair_quick": s, "extrude": s, "revolve": s, "sweep": s}. The Rust side (`cargo run --release -p xtask -- bench`) times the
same operations with the same parameters and the same broken copy (every 100th triangle removed).
"""
import json
import math
import os
import statistics
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, ROOT)

from stleditor.document import Document  # noqa: E402

MESH = os.path.join(ROOT, "dev", "bench", "test_mesh.stl")
RUNS = 3


def broken_copy(data: bytes) -> bytes:
    """Binary STL with every 100th triangle (index 99, 199, ...) removed."""
    n = int.from_bytes(data[80:84], "little")
    tris = [data[84 + 50 * i: 134 + 50 * i] for i in range(n) if i % 100 != 99]
    return data[:80] + len(tris).to_bytes(4, "little") + b"".join(tris)


def timed(fn):
    t = time.perf_counter()
    fn()
    return time.perf_counter() - t


def median(setup, op):
    out = []
    for _ in range(RUNS):
        ctx = setup()
        out.append(timed(lambda: op(ctx)))
    return statistics.median(out)


def doc_with_mesh(data, repair):
    d = Document()
    f, _ = d.import_file("test_mesh.stl", data=data, repair=repair, place="keep")
    return d, f["id"]


def with_cube(data):
    d, mesh = doc_with_mesh(data, True)
    cube, _ = d.add_feature("primitive", {"shape": "box", "length": 30, "width": 30, "height": 30,
                                          "anchor": "center", "plane": "XY", "operation": "new"})
    return d, mesh, cube["id"]


def combine(op):
    def go(ctx):
        d, mesh, cube = ctx
        d.add_feature("combine", {"target": mesh, "tools": [cube], "operation": op, "keep_tools": False})
    return go


def ngon(cx, cy, r, n):
    """n points of a regular polygon (same rule as bench.rs)."""
    return [[cx + r * math.cos(2 * math.pi * i / n), cy + r * math.sin(2 * math.pi * i / n)] for i in range(n)]


def with_sketch(entities, plane="XY"):
    d = Document()
    sk, _ = d.add_feature("sketch", {"plane": plane, "entities": entities})
    return d, sk["id"]


def with_sweep_sketches():
    d, prof = with_sketch([{"id": "c", "type": "circle", "cx": 0, "cy": 0, "r": 5}])
    path, _ = d.add_feature("sketch", {"plane": "XZ", "entities": [
        {"id": "p", "type": "polyline", "closed": False,
         "pts": [[5 * math.sin(i * 0.3), 2.0 * i] for i in range(40)]}]})
    return d, prof, path["id"]


def main():
    data = open(MESH, "rb").read()
    broken = broken_copy(data)
    r = {}
    r["import"] = median(lambda: Document(),
                         lambda d: d.import_file("test_mesh.stl", data=data, repair=True, place="keep"))
    r["combine_join"] = median(lambda: with_cube(data), combine("join"))
    r["combine_cut"] = median(lambda: with_cube(data), combine("cut"))
    r["split"] = median(lambda: doc_with_mesh(data, True),
                        lambda c: c[0].add_feature("split", {
                            "body": c[1], "plane": {"origin": [0, 0, 0], "normal": [0, 0, 1]},
                            "keep": "both", "connector": "none"}))
    r["repair_quick"] = median(lambda: doc_with_mesh(broken, False),
                               lambda c: c[0].add_feature("repair", {"body": c[1], "mode": "quick"}))
    ring = [{"id": "r", "type": "polyline", "closed": True, "pts": ngon(30, 0, 10, 200)}]
    r["extrude"] = median(lambda: with_sketch(ring),
                          lambda c: c[0].add_feature("extrude", {"sketch": c[1], "distance": 20}))
    r["revolve"] = median(lambda: with_sketch(ring),
                          lambda c: c[0].add_feature("revolve", {"sketch": c[1], "axis": "y", "angle": 360}))
    r["sweep"] = median(with_sweep_sketches,
                        lambda c: c[0].add_feature("sweep", {"profile": c[1], "path": c[2]}))
    print(json.dumps(r))


if __name__ == "__main__":
    main()
