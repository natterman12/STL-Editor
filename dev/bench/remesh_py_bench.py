"""Desktop (Python) remesh timing on the same mesh as remesh_rs_bench.mjs and browser_bench.mjs.

Run: python dev/bench/remesh_py_bench.py [edge] [runs]
Loads dev/bench/test_mesh.stl, welds vertices with identical float32 bits (same rule as the
JS bench), builds a Manifold and times stleditor.printing.remesh(solid, edge), the function the
desktop app's Remesh feature calls. Also times its two stages (refine_to_length, then
fast_simplification) and reports triangles, edge min/mean/max and watertightness.
"""
import json, sys, time
from pathlib import Path
import numpy as np

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from stleditor import geometry as G, printing as PR  # noqa: E402

EDGE = float(sys.argv[1]) if len(sys.argv) > 1 else 1.0
RUNS = int(sys.argv[2]) if len(sys.argv) > 2 else 3

tri, _ = G.read_stl_bytes((Path(__file__).parent / "test_mesh.stl").read_bytes())
pts = tri.astype(np.float32)
uniq, inv = np.unique(pts.view(np.uint32).reshape(-1, 3), axis=0, return_inverse=True)
verts = uniq.view(np.float32).reshape(-1, 3)
faces = inv.reshape(-1, 3)
solid = G.manifold_from_arrays(verts, faces)
if solid is None:
    sys.exit("test_mesh.stl is not a closed solid")


def stats(m):
    mm = m.to_mesh()
    v = np.asarray(mm.vert_properties, np.float64)[:, :3]
    f = np.asarray(mm.tri_verts, np.int64)
    e = np.sort(np.concatenate([f[:, [0, 1]], f[:, [1, 2]], f[:, [2, 0]]]), axis=1)
    keys, counts = np.unique(e, axis=0, return_counts=True)
    L = np.linalg.norm(v[keys[:, 0]] - v[keys[:, 1]], axis=1)
    r = lambda x: round(float(x), 4)
    return {"triangles": int(len(f)), "vertices": int(len(v)), "edgeMin": r(L.min()),
            "edgeMean": r(L.mean()), "edgeMax": r(L.max()), "watertight": bool((counts == 2).all())}


total, refine = [], []
out = None
for i in range(RUNS):
    t0 = time.perf_counter()
    fine = solid.refine_to_length(EDGE)
    t1 = time.perf_counter()
    refine.append(t1 - t0)
    t0 = time.perf_counter()
    out = PR.remesh(solid, EDGE)
    total.append(time.perf_counter() - t0)
    print(f"  run {i + 1}: remesh {total[-1] * 1000:.0f} ms (refine alone {refine[-1] * 1000:.0f} ms)", file=sys.stderr)

med = lambda a: sorted(a)[len(a) // 2]
res = {"mesh": "dev/bench/test_mesh.stl", "edge": EDGE, "runs": RUNS,
       "inputTriangles": int(len(faces)), "refinedTriangles": int(len(fine.to_mesh().tri_verts)),
       "medianMs": round(med(total) * 1000, 1), "timesMs": [round(t * 1000, 1) for t in total],
       "refineMedianMs": round(med(refine) * 1000, 1), **stats(out)}
print(json.dumps(res, indent=2))
