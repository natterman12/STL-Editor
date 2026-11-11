"""Generate rust/tests/parity/kernel_golden.json from 1.x manifold3d.

Reads rust/tests/parity/kernel_cases.json, runs each stack-machine case, and
records mesh_stats for every stack entry (bottom to top).

Usage:
  python dev/parity/make_kernel_golden.py          # write golden
  python dev/parity/make_kernel_golden.py --check  # exit 1 if golden differs
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import manifold3d as m3
import numpy as np

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "dev" / "parity"))

from stleditor import geometry as G  # noqa: E402
import stats as parity_stats  # noqa: E402

CASES = ROOT / "rust" / "tests" / "parity" / "kernel_cases.json"
GOLDEN = ROOT / "rust" / "tests" / "parity" / "kernel_golden.json"
MODELS = ROOT / "rust" / "tests" / "models"


def _weld_f32_bits(verts, faces) -> tuple[np.ndarray, np.ndarray]:
    """Merge vertices with identical float32 bit patterns, first-seen order."""
    v = np.asarray(verts, dtype=np.float32).reshape(-1, 3)
    f = np.asarray(faces, dtype=np.int64).reshape(-1, 3)
    seen: dict[bytes, int] = {}
    out: list[np.ndarray] = []
    remap = np.empty(len(v), dtype=np.int64)
    for i in range(len(v)):
        key = v[i].tobytes()
        j = seen.get(key)
        if j is None:
            j = len(out)
            seen[key] = j
            out.append(v[i])
        remap[i] = j
    nv = np.asarray(out, dtype=np.float32) if out else np.zeros((0, 3), np.float32)
    return nv, remap[f].astype(np.uint32, copy=False)


def _load_model(name: str) -> m3.Manifold | None:
    path = MODELS / name
    data = path.read_bytes()
    verts, faces = G.read_stl_bytes(data)
    # Triangle soup from STL: faces index consecutive triples.
    welded_v, welded_f = _weld_f32_bits(verts, faces)
    return G.manifold_from_arrays(welded_v, welded_f)


def _solid_stats(man: m3.Manifold) -> dict:
    mm = man.to_mesh()
    v = np.asarray(mm.vert_properties, dtype=np.float32)[:, :3]
    f = np.asarray(mm.tri_verts, dtype=np.uint32)
    return parity_stats.mesh_stats(v, f)


def _run_case(case: dict) -> list[dict]:
    stack: list[m3.Manifold] = []
    for op in case["ops"]:
        kind = op["op"]
        if kind == "cube":
            size = op["size"]
            stack.append(m3.Manifold.cube(tuple(size), bool(op["center"])))
        elif kind == "sphere":
            stack.append(m3.Manifold.sphere(float(op["r"]), int(op["segments"])))
        elif kind == "cylinder":
            stack.append(
                m3.Manifold.cylinder(
                    float(op["h"]),
                    float(op["r1"]),
                    float(op["r2"]),
                    int(op["segments"]),
                    bool(op["center"]),
                )
            )
        elif kind == "load":
            man = _load_model(op["model"])
            if man is None:
                return []
            stack.append(man)
        elif kind == "union":
            b = stack.pop()
            a = stack.pop()
            stack.append(a + b)
        elif kind == "difference":
            b = stack.pop()
            a = stack.pop()
            stack.append(a - b)
        elif kind == "intersection":
            b = stack.pop()
            a = stack.pop()
            stack.append(a ^ b)
        elif kind == "translate":
            v = op["v"]
            stack[-1] = stack[-1].translate(tuple(v))
        elif kind == "rotate":
            d = op["deg"]
            stack[-1] = stack[-1].rotate(tuple(d))
        elif kind == "scale":
            v = op["v"]
            stack[-1] = stack[-1].scale(tuple(v))
        elif kind == "mirror":
            n = op["normal"]
            stack[-1] = stack[-1].mirror(tuple(n))
        elif kind == "refine_to_length":
            stack[-1] = stack[-1].refine_to_length(float(op["len"]))
        elif kind == "simplify":
            stack[-1] = stack[-1].simplify(float(op["tolerance"]))
        elif kind == "minkowski_sum":
            b = stack.pop()
            a = stack.pop()
            stack.append(a.minkowski_sum(b))
        elif kind == "minkowski_difference":
            b = stack.pop()
            a = stack.pop()
            stack.append(a.minkowski_difference(b))
        elif kind == "offset_miter_extrude":
            contours = [np.asarray(c, dtype=np.float64) for c in op["contours"]]
            cs = m3.CrossSection(contours, m3.FillRule.EvenOdd)
            cs = cs.offset(float(op["delta"]), m3.JoinType.Miter, float(op["miter_limit"]), 0)
            stack.append(m3.Manifold.extrude(cs, float(op["height"])))
        elif kind == "hull":
            stack[-1] = stack[-1].hull()
        elif kind == "trim_by_plane":
            n = op["normal"]
            stack[-1] = stack[-1].trim_by_plane(tuple(n), float(op["offset"]))
        elif kind == "slice_extrude":
            z = float(op["z"])
            height = float(op["height"])
            cs = stack[-1].slice(z)
            stack[-1] = m3.Manifold.extrude(cs, height)
        elif kind == "split_by_plane":
            n = op["normal"]
            first, second = stack.pop().split_by_plane(tuple(n), float(op["offset"]))
            stack.append(first)
            stack.append(second)
        elif kind == "decompose":
            parts = stack.pop().decompose()
            parts = sorted(parts, key=lambda p: -float(p.volume()))
            stack.extend(parts)
        else:
            raise ValueError(f"unknown op {kind!r} in case {case['name']!r}")
    return [_solid_stats(m) for m in stack]


def build_golden() -> dict:
    cases = json.loads(CASES.read_text(encoding="utf-8"))
    out: dict[str, list] = {}
    for case in cases:
        out[case["name"]] = _run_case(case)
    return {k: out[k] for k in sorted(out)}


def _dumps(obj: dict) -> str:
    return json.dumps(obj, indent=1, ensure_ascii=False) + "\n"


def _norm_newlines(s: str) -> str:
    return s.replace("\r\n", "\n").replace("\r", "\n")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="exit 1 if golden differs")
    args = ap.parse_args()
    golden = build_golden()
    text = _dumps(golden)
    if args.check:
        existing = GOLDEN.read_text(encoding="utf-8") if GOLDEN.exists() else ""
        if _norm_newlines(existing) != _norm_newlines(text):
            print("make_kernel_golden: MISMATCH", file=sys.stderr)
            return 1
        print("make_kernel_golden: ok")
        return 0
    GOLDEN.parent.mkdir(parents=True, exist_ok=True)
    GOLDEN.write_text(text, encoding="utf-8", newline="\n")
    print(f"wrote {GOLDEN.relative_to(ROOT)} ({len(golden)} cases)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
