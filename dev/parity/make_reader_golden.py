"""Generate rust/tests/parity/readers_golden.json from 1.x load_mesh_file.

For every file in rust/tests/models/readers/, records either
  {"parts": [{"name": str, "stats": MeshStats}, ...]}
or
  {"error": str}
using geometry.load_mesh_file then stats.mesh_stats per part.

Usage:
  python dev/parity/make_reader_golden.py          # write golden
  python dev/parity/make_reader_golden.py --check  # exit 1 if golden differs
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "dev" / "parity"))

from stleditor import geometry as G  # noqa: E402
from stleditor.geometry import GeometryError  # noqa: E402
import stats as parity_stats  # noqa: E402

MODELS = ROOT / "rust" / "tests" / "models" / "readers"
GOLDEN = ROOT / "rust" / "tests" / "parity" / "readers_golden.json"


def _dumps(obj) -> str:
    return json.dumps(obj, indent=1, sort_keys=True, ensure_ascii=False) + "\n"


def _norm_newlines(s: str) -> str:
    return s.replace("\r\n", "\n").replace("\r", "\n")


def _file_record(path: Path) -> dict:
    data = path.read_bytes()
    try:
        parts = G.load_mesh_file(data, path.name)
    except GeometryError as e:
        return {"error": str(e)}
    out = []
    for name, verts, faces in parts:
        out.append({"name": name, "stats": parity_stats.mesh_stats(verts, faces)})
    return {"parts": out}


def build() -> dict:
    records: dict[str, dict] = {}
    for path in sorted(MODELS.iterdir()):
        if not path.is_file():
            continue
        records[path.name] = _file_record(path)
    return {k: records[k] for k in sorted(records)}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="exit 1 if golden differs")
    args = ap.parse_args()

    if not MODELS.is_dir():
        print(f"make_reader_golden: missing {MODELS.relative_to(ROOT)}", file=sys.stderr)
        return 1

    golden = build()
    text = _dumps(golden)

    if args.check:
        existing = GOLDEN.read_text(encoding="utf-8") if GOLDEN.exists() else ""
        if _norm_newlines(existing) != _norm_newlines(text):
            print(f"make_reader_golden: MISMATCH {GOLDEN.relative_to(ROOT)}", file=sys.stderr)
            return 1
        print(f"make_reader_golden: ok ({len(golden)} files)")
        return 0

    GOLDEN.parent.mkdir(parents=True, exist_ok=True)
    GOLDEN.write_text(text, encoding="utf-8", newline="\n")
    print(f"wrote {GOLDEN.relative_to(ROOT)} ({len(golden)} files)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
