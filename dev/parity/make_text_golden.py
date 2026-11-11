"""Generate rust/tests/parity/text_golden.json from 1.x meshtools text_*.

Reads every *.json case under rust/tests/parity/text/, runs text_contours and
text_extent against the bundled fonts in web/fonts/, rounds to 1e-9.

Usage:
  py -3.14 dev/parity/make_text_golden.py
  py -3.14 dev/parity/make_text_golden.py --check
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

from stleditor import meshtools as MT  # noqa: E402
from stleditor.geometry import GeometryError  # noqa: E402

CASES = ROOT / "rust" / "tests" / "parity" / "text"
GOLDEN = ROOT / "rust" / "tests" / "parity" / "text_golden.json"
FONTS = ROOT / "web" / "fonts"


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
    if isinstance(v, np.ndarray):
        return _r(v.tolist(), nd)
    return v


def _use_bundled_fonts() -> None:
    """Restrict font discovery to the repo's DejaVu files so goldens are portable."""
    MT.font_dirs = lambda: [str(FONTS)]  # type: ignore[assignment]
    MT.list_fonts.cache_clear()
    MT._load_font.cache_clear()
    MT._font_path  # noqa: B018 — ensure attribute exists
    if hasattr(MT, "default_font") and hasattr(MT.default_font, "cache_clear"):
        MT.default_font.cache_clear()


def _run_case(case: dict) -> dict:
    try:
        contours = MT.text_contours(
            case.get("text", ""),
            family=case.get("family"),
            size=float(case.get("size", 10)),
            bold=bool(case.get("bold", False)),
            italic=bool(case.get("italic", False)),
            spacing=float(case.get("spacing", 0)),
            align=case.get("align", "left"),
            line_gap=float(case.get("line_gap", 1.2)),
        )
    except GeometryError as e:
        return {"error": str(e)}
    lo, hi = MT.text_extent(contours)
    if len(contours) == 0:
        extent = [0.0, 0.0]
    else:
        extent = [float(hi[0] - lo[0]), float(hi[1] - lo[1])]
    return {
        "contours": _r([[list(map(float, p)) for p in c] for c in contours]),
        "extent": _r(extent),
    }


def build() -> dict:
    _use_bundled_fonts()
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
        print(f"make_text_golden: missing {CASES.relative_to(ROOT)}", file=sys.stderr)
        return 1
    if not FONTS.is_dir():
        print(f"make_text_golden: missing {FONTS.relative_to(ROOT)}", file=sys.stderr)
        return 1

    golden = build()
    text = _dumps(golden)

    if args.check:
        existing = GOLDEN.read_text(encoding="utf-8") if GOLDEN.exists() else ""
        if _norm_newlines(existing) != _norm_newlines(text):
            print(f"make_text_golden: MISMATCH {GOLDEN.relative_to(ROOT)}", file=sys.stderr)
            return 1
        print(f"make_text_golden: ok ({len(golden)} cases)")
        return 0

    GOLDEN.parent.mkdir(parents=True, exist_ok=True)
    GOLDEN.write_text(text, encoding="utf-8", newline="\n")
    print(f"wrote {GOLDEN.relative_to(ROOT)} ({len(golden)} cases)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
