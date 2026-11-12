"""Generate rust/tests/parity/fins_golden.json from the 1.x Support Fins engine.

For each Support Fins test model (t_bracket, lipped_shelf, bridge_ring) and three
option sets (defaults plus two variants), calls the bundled engine via
SF._call("computeFinsB64", ...) and records stats, piece count, report, and a
SHA-256 of the decoded little-endian float32 triangle bytes.

Update checks are off and USER_DIR is empty so only the bundled
web/vendor/support-fins/engine.iife.js (v1.0.5) is used.

Usage:
  python dev/parity/make_fins_golden.py          # write golden
  python dev/parity/make_fins_golden.py --check  # exit 1 if golden differs
"""
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import sys
import tempfile
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))

from stleditor import geometry as G  # noqa: E402
from stleditor import supportfins as SF  # noqa: E402

MODELS = ROOT / "rust" / "tests" / "models"
GOLDEN = ROOT / "rust" / "tests" / "parity" / "fins_golden.json"

# Support Fins overhang models from make_models.py (never dev/bench/test_mesh.stl).
SF_MODELS = ["t_bracket.stl", "lipped_shelf.stl", "bridge_ring.stl"]

# Default options plus two non-default sets used by host parity tests.
# dense_tines: from optionsSchemaJson; vertical + full density changes t_bracket vs default.
OPTION_SETS = [
    ("default", {}),
    ("full", {"mode": "full"}),
    ("dense_tines", {"tineDir": "vertical", "tineDensity": 1.0}),
]


def _dumps(obj) -> str:
    return json.dumps(obj, indent=1, sort_keys=True, ensure_ascii=False) + "\n"


def _norm_newlines(s: str) -> str:
    return s.replace("\r\n", "\n").replace("\r", "\n")


def _force_bundled_engine() -> None:
    """Golden files must use the bundled v1.0.5 engine, never a downloaded update."""
    SF.USER_DIR = tempfile.mkdtemp(prefix="sf-fins-golden-")
    SF._ctx = None
    SF._active = None
    SF.check_update = lambda force=False: None  # type: ignore[assignment]
    info = SF._read_info(SF.BUNDLED)
    if not info:
        raise RuntimeError(f"bundled Support Fins engine missing under {SF.BUNDLED}")
    SF._choose = lambda: info  # type: ignore[assignment]


def _run_case(model: str, options: dict) -> dict:
    path = MODELS / model
    verts, faces = G.read_stl_bytes(path.read_bytes())
    verts, faces = G.weld(verts, faces)
    opts = SF._options(options)
    raw = json.loads(SF._call("computeFinsB64", SF._soup(verts, faces), opts))
    report = SF._call("reportLine", raw["stats"])
    tri_bytes = base64.b64decode(raw["triangles"])
    # Sanity: little-endian float32 triangle soup.
    np.frombuffer(tri_bytes, dtype="<f4")
    return {
        "model": model,
        "options": dict(options),
        "stats": raw["stats"],
        "pieces": len(raw["pieces"]),
        "report": report,
        "triangles_sha256": hashlib.sha256(tri_bytes).hexdigest(),
        "over": len(raw["overFaces"]),
        "small": len(raw["smallFaces"]),
    }


def build() -> dict:
    _force_bundled_engine()
    out: dict[str, dict] = {}
    for model in SF_MODELS:
        stem = Path(model).stem
        for label, options in OPTION_SETS:
            name = f"{stem}_{label}"
            out[name] = _run_case(model, options)
    return {k: out[k] for k in sorted(out)}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="exit 1 if golden differs")
    args = ap.parse_args()

    data = build()
    text = _dumps(data)

    if args.check:
        existing = GOLDEN.read_text(encoding="utf-8") if GOLDEN.exists() else ""
        if _norm_newlines(existing) != _norm_newlines(text):
            print(f"make_fins_golden: MISMATCH {GOLDEN.relative_to(ROOT)}", file=sys.stderr)
            import difflib

            diff = difflib.unified_diff(
                _norm_newlines(existing).splitlines(),
                _norm_newlines(text).splitlines(),
                "committed",
                "generated",
                n=2,
                lineterm="",
            )
            for line in list(diff)[:60]:
                print("  " + line, file=sys.stderr)
            return 1
        print(f"make_fins_golden: ok ({len(data)} cases)")
        return 0

    GOLDEN.parent.mkdir(parents=True, exist_ok=True)
    GOLDEN.write_text(text, encoding="utf-8", newline="\n")
    print(f"wrote {GOLDEN.relative_to(ROOT)} ({len(data)} cases)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
