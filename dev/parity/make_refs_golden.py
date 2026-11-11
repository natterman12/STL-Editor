"""Generate rust/tests/parity/refs_golden.json from 1.x meshtools.ref_geometry.

Reads every *.json case under rust/tests/parity/refs/, loads the named STL from
rust/tests/models/, builds a manifold, and records the ref entities.

Usage:
  py -3.14 dev/parity/make_refs_golden.py
  py -3.14 dev/parity/make_refs_golden.py --check
"""
from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))

from stleditor import geometry as G  # noqa: E402
from stleditor import meshtools as MT  # noqa: E402
from stleditor.geometry import GeometryError  # noqa: E402

CASES = ROOT / "rust" / "tests" / "parity" / "refs"
GOLDEN = ROOT / "rust" / "tests" / "parity" / "refs_golden.json"
MODELS = ROOT / "rust" / "tests" / "models"


def _dumps(obj) -> str:
    return json.dumps(obj, indent=1, sort_keys=True, ensure_ascii=False) + "\n"


def _norm_newlines(s: str) -> str:
    return s.replace("\r\n", "\n").replace("\r", "\n")


def _r(v, nd=9):
    if isinstance(v, (float, np.floating)):
        x = float(v)
        if not math.isfinite(x):
            return x
        return round(x, nd)
    if isinstance(v, (list, tuple)):
        return [_r(a, nd) for a in v]
    if isinstance(v, dict):
        return {k: _r(v[k], nd) for k in sorted(v)}
    if isinstance(v, np.ndarray):
        return _r(v.tolist(), nd)
    return v


def _load_solid(name: str):
    path = MODELS / name
    verts, faces = G.read_stl_bytes(path.read_bytes())
    wv, wf = G.weld(verts, faces)
    return G.manifold_from_arrays(wv, wf)


def _frame(spec: dict):
    origin = spec.get("origin", [0, 0, 0])
    normal = spec.get("normal", [0, 0, 1])
    xdir = spec.get("xdir")
    return G.plane_frame(origin, normal, xdir)


def _ents_evenodd(polys, tol: float) -> list:
    """Match meshtools.ref_geometry's inner ents(tol) EvenOdd simplify path."""
    import manifold3d as m3

    ps = [np.asarray(p, float) for p in polys]
    if tol:
        cs = m3.CrossSection(
            [np.ascontiguousarray(p) for p in ps], m3.FillRule.EvenOdd
        ).simplify(float(tol))
        ps = [np.asarray(p, float) for p in cs.to_polygons()]
    out = []
    for p in ps:
        if len(p) < 3:
            continue
        c = MT._as_circle(p)
        if c is not None:
            out.append({"type": "circle", "cx": c[0], "cy": c[1], "r": c[2]})
            continue
        for i in range(len(p)):
            q0, q1 = p[i], p[(i + 1) % len(p)]
            if math.hypot(q1[0] - q0[0], q1[1] - q0[1]) > 1e-6:
                out.append(
                    {
                        "type": "line",
                        "p1": [round(float(q0[0]), 5), round(float(q0[1]), 5)],
                        "p2": [round(float(q1[0]), 5), round(float(q1[1]), 5)],
                    }
                )
    return out


def _run_case(case: dict) -> dict:
    op = case.get("op") or "ref_geometry"
    if op == "outline_entities":
        models = case.get("models") or [case["model"]]
        solids = [_load_solid(m) for m in models]
        frame = _frame(case.get("frame") or {})
        mode = case.get("mode") or "section"
        try:
            polys = MT.outline(solids, frame, mode=mode, tolerance=float(case.get("tolerance", 0.02)))
        except GeometryError as e:
            return {"error": str(e)}
        ids = iter(range(1, 10_000))
        ents = MT.outline_entities(polys, lambda: next(ids), construction=bool(case.get("construction")))
        return _r({"entities": ents, "op": "outline_entities"})
    if op == "ents_evenodd":
        polys = [np.asarray(p, float) for p in case["polys"]]
        ents = _ents_evenodd(polys, float(case.get("tol", 0.005)))
        return _r({"entities": ents, "op": "ents_evenodd"})
    models = case.get("models") or [case["model"]]
    solids = [_load_solid(m) for m in models]
    frame = _frame(case.get("frame") or {})
    try:
        result = MT.ref_geometry(solids, frame, max_segs=int(case.get("max_segs", 3000)))
    except GeometryError as e:
        return {"error": str(e)}
    # Interface: {case: {"entities": [...]}}; also keep kind for the Rust port.
    return _r({"entities": result.get("refs") or [], "kind": result.get("kind")})


def build() -> dict:
    records: dict[str, dict] = {}
    for path in sorted(CASES.glob("*.json")):
        case = json.loads(path.read_text(encoding="utf-8"))
        name = case.get("name") or path.stem
        records[name] = _run_case(case)
    return {k: records[k] for k in sorted(records)}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="exit 1 if golden differs")
    args = ap.parse_args()

    if not CASES.is_dir():
        print(f"make_refs_golden: missing {CASES.relative_to(ROOT)}", file=sys.stderr)
        return 1

    golden = build()
    text = _dumps(golden)

    if args.check:
        existing = GOLDEN.read_text(encoding="utf-8") if GOLDEN.exists() else ""
        if _norm_newlines(existing) != _norm_newlines(text):
            print(f"make_refs_golden: MISMATCH {GOLDEN.relative_to(ROOT)}", file=sys.stderr)
            return 1
        print(f"make_refs_golden: ok ({len(golden)} cases)")
        return 0

    GOLDEN.parent.mkdir(parents=True, exist_ok=True)
    GOLDEN.write_text(text, encoding="utf-8", newline="\n")
    print(f"wrote {GOLDEN.relative_to(ROOT)} ({len(golden)} cases)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
