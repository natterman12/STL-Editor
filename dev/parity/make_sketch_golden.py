"""Generate rust/tests/parity/sketch_golden.json from 1.x sketch.py.

Reads every *.json case under rust/tests/parity/sketch/, runs profile (and the
requested ops), and records faces / op results with coordinates rounded to 1e-9.

Also records `polygonize`: the raw shapely faces() rings (exterior then holes,
coords as shapely returns them, including the closing vertex) before any
Clipper / profile step.

Usage:
  py -3.14 dev/parity/make_sketch_golden.py          # write golden
  py -3.14 dev/parity/make_sketch_golden.py --check  # exit 1 if golden differs
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

from stleditor import sketch as S  # noqa: E402
from stleditor.geometry import GeometryError  # noqa: E402

CASES = ROOT / "rust" / "tests" / "parity" / "sketch"
GOLDEN = ROOT / "rust" / "tests" / "parity" / "sketch_golden.json"


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


def _faces_from_cs(cs) -> list:
    return [_r([list(map(float, p)) for p in poly]) for poly in cs.to_polygons()]


def _polygonize(entities: list, only) -> list:
    """Raw shapely faces() rings before Clipper/profile: per face, exterior then holes."""
    out = []
    for f in S.faces(entities, only):
        rings = [_r([list(map(float, p)) for p in f.exterior.coords])]
        for hole in f.interiors:
            rings.append(_r([list(map(float, p)) for p in hole.coords]))
        out.append(rings)
    return out


def _new_id_factory():
    n = 0

    def nid():
        nonlocal n
        n += 1
        return f"n{n}"

    return nid


def _run_offset(ents: list, spec: dict):
    try:
        return _r(S.offset(ents, list(spec["ids"]), float(spec["d"]), _new_id_factory()))
    except (GeometryError, ValueError) as e:
        return {"error": str(e)}


def _run_ops(entities: list, ops: dict) -> dict:
    out: dict = {}
    ents = [dict(e) for e in entities]
    for name in sorted(ops):
        spec = ops[name]
        try:
            if name == "explode":
                eid = spec if isinstance(spec, str) else spec["id"]
                target = next(e for e in ents if e.get("id") == eid)
                out[name] = _r(S.explode(target))
            elif name == "trim":
                out[name] = _r(S.trim(ents, spec["id"], spec["point"]))
            elif name == "offset":
                if isinstance(spec, list):
                    out[name] = [_run_offset(ents, sp) for sp in spec]
                else:
                    out[name] = _run_offset(ents, spec)
            elif name == "mirror":
                out[name] = _r(S.mirror(ents, list(spec["ids"]), spec["axis"], _new_id_factory()))
            elif name == "find_axis":
                axis_id = spec if isinstance(spec, str) else spec["id"]
                p0, d = S.find_axis(ents, axis_id)
                out[name] = _r({"origin": p0.tolist(), "direction": d.tolist()})
            elif name == "snap_points":
                out[name] = _r(S.snap_points(ents))
            else:
                raise ValueError(f"unknown op {name!r}")
        except (GeometryError, ValueError) as e:
            out[name] = {"error": str(e)}
    return out


def _run_case(case: dict) -> dict:
    entities = list(case.get("entities") or [])
    # Feature params name the id filter `entities`; case files use `only` (entities is the sketch).
    only = case.get("only")
    regions = case.get("regions")
    polygonize = _polygonize(entities, only)
    rec: dict = {"polygonize": polygonize}
    try:
        cs = S.profile(entities, only, regions)
        rec["faces"] = _faces_from_cs(cs)
    except GeometryError as e:
        rec["error"] = str(e)
    if case.get("ops"):
        rec["ops"] = _run_ops(entities, case["ops"])
    return rec


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
        print(f"make_sketch_golden: missing {CASES.relative_to(ROOT)}", file=sys.stderr)
        return 1

    golden = build()
    text = _dumps(golden)

    if args.check:
        existing = GOLDEN.read_text(encoding="utf-8") if GOLDEN.exists() else ""
        if _norm_newlines(existing) != _norm_newlines(text):
            print(f"make_sketch_golden: MISMATCH {GOLDEN.relative_to(ROOT)}", file=sys.stderr)
            return 1
        print(f"make_sketch_golden: ok ({len(golden)} cases)")
        return 0

    GOLDEN.parent.mkdir(parents=True, exist_ok=True)
    GOLDEN.write_text(text, encoding="utf-8", newline="\n")
    print(f"wrote {GOLDEN.relative_to(ROOT)} ({len(golden)} cases)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
