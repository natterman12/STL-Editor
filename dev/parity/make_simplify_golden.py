"""Generate rust/tests/parity/simplify_golden.json from fast_simplification.

Runs fast_simplification.simplify on each welded committed model in
rust/tests/models/*.stl at ratios 0.5, 0.1 and 0.02 (target_reduction =
1 - ratio), before 1.x's weld/repair steps on the result.

Usage:
  py -3.14 dev/parity/make_simplify_golden.py
  py -3.14 dev/parity/make_simplify_golden.py --check
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import fast_simplification
import numpy as np

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "dev" / "parity"))

from stleditor import geometry as G  # noqa: E402

MODELS = ROOT / "rust" / "tests" / "models"
GOLDEN = ROOT / "rust" / "tests" / "parity" / "simplify_golden.json"
OUT_DIR = ROOT / "rust" / "tests" / "parity" / "simplify"

RATIOS = (0.5, 0.1, 0.02)


def _dumps(obj) -> str:
    return json.dumps(obj, indent=1, sort_keys=True, ensure_ascii=False) + "\n"


def _norm_newlines(s: str) -> str:
    return s.replace("\r\n", "\n").replace("\r", "\n")


def _ratio_tag(r: float) -> str:
    s = f"{r:.2f}".replace(".", "_")
    return s


def build() -> tuple[dict, dict[str, bytes]]:
    """Return (records, mesh_bytes_by_name). Does not write files."""
    records: dict[str, dict] = {}
    meshes: dict[str, bytes] = {}

    for path in sorted(MODELS.glob("*.stl")):
        v, f = G.read_stl_bytes(path.read_bytes())
        wv, wf = G.weld(v, f)
        wv32 = np.asarray(wv, np.float32)
        wf64 = np.asarray(wf, np.int64)
        if len(wf64) == 0:
            continue
        for ratio in RATIOS:
            name = f"{path.stem}_r{_ratio_tag(ratio)}"
            nv, nf = fast_simplification.simplify(
                wv32, wf64, target_reduction=1.0 - float(ratio)
            )
            nv = np.asarray(nv, np.float32)
            nf = np.asarray(nf, np.int64)
            meshes[name] = G.stl_bytes(nv, nf)
            records[name] = {
                "model": path.name,
                "ratio": float(ratio),
                "tris": int(len(nf)),
                "mesh": f"simplify/{name}.stl",
            }

    return {k: records[k] for k in sorted(records)}, meshes


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="exit 1 if golden differs")
    args = ap.parse_args()

    golden, meshes = build()
    text = _dumps(golden)

    if args.check:
        existing = GOLDEN.read_text(encoding="utf-8") if GOLDEN.exists() else ""
        if _norm_newlines(existing) != _norm_newlines(text):
            print(f"make_simplify_golden: MISMATCH {GOLDEN.relative_to(ROOT)}", file=sys.stderr)
            return 1
        for name, rec in golden.items():
            mesh_path = ROOT / "rust" / "tests" / "parity" / Path(rec["mesh"])
            if not mesh_path.exists():
                print(
                    f"make_simplify_golden: MISSING mesh {rec['mesh']}",
                    file=sys.stderr,
                )
                return 1
            if mesh_path.read_bytes() != meshes[name]:
                print(
                    f"make_simplify_golden: MISMATCH mesh {rec['mesh']}",
                    file=sys.stderr,
                )
                return 1
        print(f"make_simplify_golden: ok ({len(golden)} cases)")
        return 0

    GOLDEN.parent.mkdir(parents=True, exist_ok=True)
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    for name, data in meshes.items():
        (OUT_DIR / f"{name}.stl").write_bytes(data)
    GOLDEN.write_text(text, encoding="utf-8", newline="\n")
    print(f"wrote {GOLDEN.relative_to(ROOT)} ({len(golden)} cases)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
