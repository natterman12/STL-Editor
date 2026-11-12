"""Times operations on dev/bench/test_mesh.stl through the 1.x Document (median of 3).

Prints one JSON object of seconds on stdout. The Rust side (`cargo run --release -p xtask -- bench`)
times the same operations with the same parameters and the same broken copy (every 100th triangle
removed). Remesh uses desktop `stleditor/printing.py` remesh via the remesh feature.

Resize Hole uses rust/tests/models/hole_and_pin.stl: the bench mesh's through-cut is square, not a
round wall. Support Fins uses the bundled engine with update checks off (same as make_golden.py).
"""
import json
import math
import os
import statistics
import sys
import tempfile
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, ROOT)

from stleditor.document import Document  # noqa: E402

MESH = os.path.join(ROOT, "dev", "bench", "test_mesh.stl")
HOLE = os.path.join(ROOT, "rust", "tests", "models", "hole_and_pin.stl")
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


def with_sculpt(data):
    """Imported mesh plus a 20-point Smooth stroke across the bbox (centre y, top z)."""
    d, mesh = doc_with_mesh(data, True)
    bb = d.state.bodies[mesh].require_solid().bounding_box()
    lo, hi = bb[:3], bb[3:]
    y = 0.5 * (lo[1] + hi[1])
    z = hi[2]
    pts = [[lo[0] + i * (hi[0] - lo[0]) / 19.0, y, z] for i in range(20)]
    return d, mesh, pts


def force_bundled_fins():
    """Bundled engine only; update checks off (same rule as make_golden.py)."""
    from stleditor import supportfins as SF

    SF.USER_DIR = tempfile.mkdtemp(prefix="sf-bench-")
    SF._ctx = None
    SF._active = None
    SF.check_update = lambda force=False: None  # type: ignore[assignment]
    info = SF._read_info(SF.BUNDLED)
    if info:
        SF._choose = lambda: info  # type: ignore[assignment]


def clear_sf_cache():
    """Drop the fins/wall LRU so median-of-3 isn't a cache hit (same mesh+options each run)."""
    from stleditor import supportfins as SF

    SF._CACHE["fins"].clear()
    SF._CACHE["wall"].clear()


def with_oversized_for_bed(data):
    """Bench mesh is ~40 mm; scale ×10 so it needs cuts on the A1 mini (180 mm)."""
    d, mesh = doc_with_mesh(data, True)
    d.add_feature("scale", {"bodies": [mesh], "factor": 10, "pivot": "center"})
    return d, mesh


def with_six_copies(data):
    d, mesh = doc_with_mesh(data, True)
    pat, _ = d.add_feature("pattern", {
        "bodies": [mesh], "kind": "linear", "direction": [1, 0, 0],
        "spacing": 50, "count": 6, "join": False})
    bodies = [mesh] + [f"{pat['id']}.{mesh}.{i}" for i in range(2, 7)]
    return d, bodies


def with_pattern_source(data):
    """Offset from the origin so circular ×12 does real work (a centred near-sphere is almost identity)."""
    d, mesh = doc_with_mesh(data, True)
    d.add_feature("move", {"bodies": [mesh], "translate": [40, 0, 0], "rotate": [0, 0, 0], "pivot": "origin"})
    return d, mesh


def with_export_body(data):
    """Imported bench mesh as the single body Document.export / geometry.export_bodies would write."""
    from stleditor import geometry as G

    d, mesh = doc_with_mesh(data, True)
    body = d.state.body(mesh).copy(name=d.body_name(mesh))
    return G, [body]


def export_op(ext):
    """Write the Document's body to a temp file via geometry.export_bodies (timed)."""
    def go(ctx):
        G, bodies = ctx
        fd, path = tempfile.mkstemp(suffix=ext)
        os.close(fd)
        try:
            G.export_bodies(bodies, path)
        finally:
            try:
                os.unlink(path)
            except OSError:
                pass
    return go


def time_export_step_node():
    """Median STEP time from stepBytes in Node (dev/parity/js/bench_step.mjs)."""
    import subprocess

    script = os.path.join(ROOT, "dev", "parity", "js", "bench_step.mjs")
    # Prefer the default heap; retry with 8 GB if Node runs out of memory.
    attempts = [
        ["node", script],
        ["node", "--max-old-space-size=8192", script],
    ]
    last_err = ""
    for cmd in attempts:
        p = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
        if p.returncode == 0:
            line = next((l for l in reversed(p.stdout.splitlines()) if l.strip().startswith("{")), "")
            return json.loads(line)["export_step"]
        last_err = (p.stderr or p.stdout or "").strip()
        if "heap" not in last_err.lower() and "out of memory" not in last_err.lower():
            break
    raise RuntimeError(f"bench_step.mjs failed: {last_err}")


def main(only=None):
    """Time all rows, or only the keys in `only` (for a subset check without the full bench)."""
    want = set(only) if only else None

    def take(key, build):
        if want is not None and key not in want:
            return
        r[key] = build()

    force_bundled_fins()
    data = open(MESH, "rb").read()
    broken = broken_copy(data)
    hole_data = open(HOLE, "rb").read()
    r = {}
    take("import", lambda: median(lambda: Document(),
                         lambda d: d.import_file("test_mesh.stl", data=data, repair=True, place="keep")))
    take("combine_join", lambda: median(lambda: with_cube(data), combine("join")))
    take("combine_cut", lambda: median(lambda: with_cube(data), combine("cut")))
    take("split", lambda: median(lambda: doc_with_mesh(data, True),
                        lambda c: c[0].add_feature("split", {
                            "body": c[1], "plane": {"origin": [0, 0, 0], "normal": [0, 0, 1]},
                            "keep": "both", "connector": "none"})))
    take("repair_quick", lambda: median(lambda: doc_with_mesh(broken, False),
                               lambda c: c[0].add_feature("repair", {"body": c[1], "mode": "quick"})))
    ring = [{"id": "r", "type": "polyline", "closed": True, "pts": ngon(30, 0, 10, 200)}]
    take("extrude", lambda: median(lambda: with_sketch(ring),
                          lambda c: c[0].add_feature("extrude", {"sketch": c[1], "distance": 20})))
    take("revolve", lambda: median(lambda: with_sketch(ring),
                          lambda c: c[0].add_feature("revolve", {"sketch": c[1], "axis": "y", "angle": 360})))
    take("sweep", lambda: median(with_sweep_sketches,
                        lambda c: c[0].add_feature("sweep", {"profile": c[1], "path": c[2]})))
    take("simplify", lambda: median(lambda: doc_with_mesh(data, True),
                           lambda c: c[0].add_feature("simplify", {"body": c[1], "ratio": 0.1})))
    take("remesh", lambda: median(lambda: doc_with_mesh(data, True),
                         lambda c: c[0].add_feature("remesh", {"body": c[1], "edge": 1.0})))
    take("hollow", lambda: median(lambda: doc_with_mesh(data, True),
                         lambda c: c[0].add_feature("hollow", {
                             "body": c[1], "thickness": 2, "resolution": "normal"})))
    take("shell", lambda: median(lambda: doc_with_mesh(data, True),
                        lambda c: c[0].add_feature("shell", {
                            "body": c[1], "thickness": 2, "faces": []})))
    take("lattice", lambda: median(lambda: doc_with_mesh(data, True),
                          lambda c: c[0].add_feature("lattice", {
                              "body": c[1], "cell": 6, "strut": 1, "thickness": 2})))
    take("offset", lambda: median(lambda: doc_with_mesh(data, True),
                         lambda c: c[0].add_feature("offset", {
                             "bodies": [c[1]], "distance": 0.5, "direction": "grow"})))
    take("thincheck", lambda: median(lambda: doc_with_mesh(data, True),
                            lambda c: c[0].add_feature("thincheck", {"body": c[1], "min": 0.8})))
    take("repair_rebuild", lambda: median(lambda: doc_with_mesh(broken, False),
                                 lambda c: c[0].add_feature("repair", {
                                     "body": c[1], "mode": "rebuild", "resolution": "normal"})))
    take("sculpt", lambda: median(lambda: with_sculpt(data),
                         lambda c: c[0].add_feature("sculpt", {
                             "body": c[1],
                             "strokes": [{"points": c[2], "radius": 5, "strength": 0.5, "mode": "smooth"}]})))

    def supportfins_op(c):
        clear_sf_cache()
        c[0].add_feature("supportfins", {"body": c[1], "output": "join", "options": {}})

    take("supportfins", lambda: median(lambda: doc_with_mesh(data, True), supportfins_op))
    take("bedsplit", lambda: median(lambda: with_oversized_for_bed(data),
                           lambda c: c[0].add_feature("bedsplit", {
                               "body": c[1], "printer": "bambu_a1mini", "connector": "none",
                               "margin": 5, "optimize": True})))
    take("layflat", lambda: median(lambda: doc_with_mesh(data, True),
                          lambda c: c[0].add_feature("layflat", {"bodies": [c[1]], "auto": True})))
    take("plate", lambda: median(lambda: with_six_copies(data),
                        lambda c: c[0].add_feature("plate", {
                            "bodies": c[1], "printer": "bambu_x1", "gap": 5, "rotate": False})))
    def pattern_op(c):
        d, mesh = c
        d.add_feature("pattern", {
            "bodies": [mesh], "kind": "circular", "center": [0, 0, 0],
            "axis": [0, 0, 1], "angle": 360, "count": 12, "join": True})
        # Manifold leaves the boolean lazy until volume/mesh; force the real work for the ratio.
        d.state.bodies[mesh].require_solid().volume()

    take("pattern", lambda: median(lambda: with_pattern_source(data), pattern_op))
    take("thread", lambda: median(lambda: Document(),
                         lambda d: d.add_feature("thread", {
                             "kind": "external", "diameter": 8, "pitch": 1.25, "length": 30,
                             "tolerance": 0.2, "hand": "right", "chamfer": True,
                             "plane": "XY", "offset": [0, 0, 0], "operation": "new"})))
    take("resizehole", lambda: median(lambda: doc_with_mesh(hole_data, True),
                             lambda c: c[0].add_feature("resizehole", {
                                 "body": c[1], "point": [16, 15, 7.5], "diameter": 6})))
    def supportwall_op(c):
        clear_sf_cache()
        c[0].add_feature("supportwall", {
            "body": c[1],
            "walls": [{"shape": "line", "points": [[-5, -10, -5], [5, -10, -5]]}],
            "output": "join", "options": {}})

    take("supportwall", lambda: median(lambda: doc_with_mesh(data, True), supportwall_op))

    # --- M3 export rows (temp file each run; STEP baseline is Node stepBytes) ---
    take("export_stl", lambda: median(lambda: with_export_body(data), export_op(".stl")))
    take("export_3mf", lambda: median(lambda: with_export_body(data), export_op(".3mf")))
    take("export_obj", lambda: median(lambda: with_export_body(data), export_op(".obj")))
    take("export_step", time_export_step_node)
    print(json.dumps(r))


if __name__ == "__main__":
    # Optional keys after the script name time only those rows (e.g. export_stl export_3mf).
    main(sys.argv[1:] or None)
