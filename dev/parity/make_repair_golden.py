"""Generate rust/tests/parity/repair_golden.json from the 1.x repair steps.

For every broken model in rust/tests/models/*.stl, and for a few variants of it (see VARIANTS),
runs each repair step on its own and records MeshStats, or null when the step gives no solid:

  weld                       geometry.weld
  process                    trimesh.Trimesh(process=True, validate=True) on the welded mesh
  fix_winding                trimesh.repair.fix_winding on the welded mesh
  fix_inversion              trimesh.repair.fix_inversion(multibody=True) on the welded mesh
  fix_inversion_single       the same with multibody=False
  fill_holes                 trimesh.repair.fill_holes on the welded mesh
  cap_holes                  geometry._cap_holes on the welded mesh
  touching_shells            geometry._touching_shells on the welded mesh (a solid or null)
  local_repair               geometry.local_repair on the raw triangles (a solid or null)
  quick_repair               geometry.quick_repair on the raw triangles
  merge_overlapping_shells   on the welded mesh as a solid (null when it is not a solid)

plus "cap_loops" (loops closed by _cap_holes) and "merge_count" (shells merged). A mismatch in
the Rust port can then be traced to one step. The variants are made from the raw triangle list
in a way the Rust test repeats exactly (rust/crates/stl-mesh/tests/repair.rs).

What the newer variants are for:
  x5, lap3   many shells in one solid, apart (x5) or pushed into each other (lap3), for the
             order merge_overlapping_shells takes them in
  drop4      on touching_cubes it leaves a piece that is neither solid nor flat, so
             _touching_shells gives up (_is_flat is False)
  nan...     one triangle whose coordinates are not numbers. weld drops the triangle and keeps
             its vertex, the kernel then refuses the whole mesh three times, and local_repair
             reaches its last try: shell by shell (one shell, several shells unioned, and
             inside-out shells that are reversed)

Usage:
  python dev/parity/make_repair_golden.py          # write golden
  python dev/parity/make_repair_golden.py --check  # exit 1 if golden differs
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "dev" / "parity"))

import trimesh  # noqa: E402

from stleditor import geometry as G  # noqa: E402
import stats as parity_stats  # noqa: E402

MODELS = ROOT / "rust" / "tests" / "models"
GOLDEN = ROOT / "rust" / "tests" / "parity" / "repair_golden.json"

# name -> what it does to the raw triangles (triangle i has vertices 3i, 3i+1, 3i+2)
VARIANTS = [
    "plain", "flip3", "drop7", "flip3_drop7", "x5_flip3", "x5_drop7",
    "x5", "lap3", "drop4",
    "nan", "nan_drop7", "nan_flip3_drop7", "nan_drop4", "x5_nan_flip3",
]


def variant(v, f, name):
    """v (n, 3) float32, f (m, 3) of a raw triangle soup -> the variant's (v, f)."""
    v = np.asarray(v, np.float32)
    f = np.asarray(f, np.int64)
    if name.startswith("x5"):
        # five copies, each 2.5 bounding boxes further along x
        step = np.float32(2.5) * (v[:, 0].max() - v[:, 0].min())
        vs, fs = [], []
        for k in range(5):
            c = v.copy()
            c[:, 0] = c[:, 0] + np.float32(k) * step
            vs.append(c)
            fs.append(f + k * len(v))
        v, f = np.concatenate(vs), np.concatenate(fs)
    if "lap3" in name:
        # three copies, each a quarter of the bounding box further along x, y and z
        step = np.float32(0.25) * (v.max(axis=0) - v.min(axis=0))
        vs, fs = [], []
        for k in range(3):
            vs.append(v + np.float32(k) * step)
            fs.append(f + k * len(v))
        v, f = np.concatenate(vs), np.concatenate(fs)
    if "drop7" in name:
        # leave out every triangle whose index is 3 mod 7
        f = f[np.arange(len(f)) % 7 != 3]
    if "drop4" in name:
        # leave out every triangle whose index is 2 mod 4
        f = f[np.arange(len(f)) % 4 != 2]
    if "flip3" in name:
        # reverse every triangle whose index is 1 mod 3
        f = f.copy()
        m = np.arange(len(f)) % 3 == 1
        f[m] = f[m][:, ::-1]
    if "nan" in name:
        # one more triangle, on three new vertices whose coordinates are all NaN
        n = len(v)
        v = np.concatenate([v, np.full((3, 3), np.nan, np.float32)])
        f = np.concatenate([f, np.array([[n, n + 1, n + 2]], np.int64)])
    return v, f


def _stats(v, f):
    # A vertex that no triangle uses and that is not finite (the "nan" variants) is left out:
    # it would put NaN in the bounding box, which JSON cannot hold.
    v, f = np.asarray(v), np.asarray(f)
    drop = ~np.isfinite(v).all(axis=1)
    drop[np.unique(f)] = False
    if drop.any():
        f = (np.cumsum(~drop) - 1)[f]
        v = v[~drop]
    return parity_stats.mesh_stats(v, f)


def _solid_stats(man):
    if man is None:
        return None
    v, f = G.Body("x", solid=man).mesh()
    return _stats(v, f)


def _tm(v, f):
    return trimesh.Trimesh(vertices=v.copy(), faces=f.copy(), process=False)


def run_steps(v, f) -> dict:
    out = {}
    wv, wf = G.weld(v, f)
    out["weld"] = _stats(wv, wf)

    tm = trimesh.Trimesh(vertices=wv.copy(), faces=wf.copy(), process=True, validate=True)
    out["process"] = _stats(tm.vertices, tm.faces)

    tm = _tm(wv, wf)
    trimesh.repair.fix_winding(tm)
    out["fix_winding"] = _stats(tm.vertices, tm.faces)

    tm = _tm(wv, wf)
    trimesh.repair.fix_inversion(tm, multibody=True)
    out["fix_inversion"] = _stats(tm.vertices, tm.faces)

    tm = _tm(wv, wf)
    trimesh.repair.fix_inversion(tm, multibody=False)
    out["fix_inversion_single"] = _stats(tm.vertices, tm.faces)

    tm = _tm(wv, wf)
    trimesh.repair.fill_holes(tm)
    out["fill_holes"] = _stats(tm.vertices, tm.faces)

    cv, cf, loops = G._cap_holes(wv.copy(), wf.copy())
    out["cap_holes"] = _stats(cv, cf)
    out["cap_loops"] = int(loops)

    out["touching_shells"] = _solid_stats(G._touching_shells(wv.copy(), wf.copy()))
    out["local_repair"] = _solid_stats(G.local_repair(v, f))

    qv, qf = G.quick_repair(v, f)
    out["quick_repair"] = _stats(qv, qf)

    man = G.manifold_from_arrays(wv, wf)
    if man is None:
        out["merge_overlapping_shells"] = None
        out["merge_count"] = 0
    else:
        merged, n = G.merge_overlapping_shells(man)
        out["merge_overlapping_shells"] = _solid_stats(merged)
        out["merge_count"] = int(n)
    return out


def build() -> dict:
    records = {}
    for path in sorted(MODELS.glob("*.stl")):
        v, f = G.read_stl_bytes(path.read_bytes())
        for name in VARIANTS:
            vv, vf = variant(v, f, name)
            records[f"{path.name}@{name}"] = run_steps(vv, vf)
    return {k: records[k] for k in sorted(records)}


def _dumps(obj) -> str:
    return json.dumps(obj, indent=1, sort_keys=True, ensure_ascii=False) + "\n"


def _norm_newlines(s: str) -> str:
    return s.replace("\r\n", "\n").replace("\r", "\n")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="exit 1 if golden differs")
    args = ap.parse_args()

    golden = build()
    text = _dumps(golden)

    if args.check:
        existing = GOLDEN.read_text(encoding="utf-8") if GOLDEN.exists() else ""
        if _norm_newlines(existing) != _norm_newlines(text):
            print(f"make_repair_golden: MISMATCH {GOLDEN.relative_to(ROOT)}", file=sys.stderr)
            return 1
        print(f"make_repair_golden: ok ({len(golden)} cases)")
        return 0

    GOLDEN.parent.mkdir(parents=True, exist_ok=True)
    GOLDEN.write_text(text, encoding="utf-8", newline="\n")
    print(f"wrote {GOLDEN.relative_to(ROOT)} ({len(golden)} cases)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
