"""Generate rust/tests/parity/features_golden/<type>.json from 1.x Document.

Reads each rust/tests/parity/features/<type>.json case file, builds a 1.x
Document (sources + params + features f1,f2,…), evaluates, and records
results plus body mesh_stats.

Usage:
  python dev/parity/make_golden.py          # write golden files
  python dev/parity/make_golden.py --check  # exit 1 if any golden differs
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "dev" / "parity"))

from stleditor.document import Document  # noqa: E402
import stats as parity_stats  # noqa: E402

FEATURES_DIR = ROOT / "rust" / "tests" / "parity" / "features"
GOLDEN_DIR = ROOT / "rust" / "tests" / "parity" / "features_golden"
MODELS = ROOT / "rust" / "tests" / "models"

BATCH1 = [
    "import",
    "primitive",
    "plane",
    "move",
    "scale",
    "mirror",
    "combine",
    "split",
    "delete",
    "separate",
    "repair",
]

BATCH2 = [
    "sketch",
    "extrude",
    "revolve",
    "loft",
    "sweep",
    "hole",
    "pushpull",
]

BATCH3 = [
    "simplify",
    "remesh",
    "sculpt",
    "offset",
    "hollow",
    "shell",
    "lattice",
    "thincheck",
]

REMESH_GOLDEN = ROOT / "rust" / "tests" / "parity" / "remesh_golden.json"


def _dumps(obj) -> str:
    return json.dumps(obj, indent=1, sort_keys=True, ensure_ascii=False) + "\n"


def _norm_newlines(s: str) -> str:
    return s.replace("\r\n", "\n").replace("\r", "\n")


def _remap_source(params: dict, mapping: dict[str, str]) -> dict:
    """Copy params; rewrite import `source` through the case-key → real-id map."""
    out = dict(params)
    if "source" in out and isinstance(out["source"], str) and out["source"] in mapping:
        out["source"] = mapping[out["source"]]
    return out


def _body_record(bid: str, body) -> dict:
    v, f = body.mesh()
    return {
        "id": bid,
        "name": body.name,
        "solid": bool(body.is_solid),
        "stats": parity_stats.mesh_stats(v, f),
    }


def _run_case(case: dict) -> dict:
    doc = Document()
    mapping: dict[str, str] = {}
    sources = case.get("sources") or {}
    for key in sorted(sources):
        path = MODELS / sources[key]
        data = path.read_bytes()
        sid = doc.add_source(sources[key], data)
        mapping[key] = sid

    doc.params = [
        {
            "name": str(q.get("name") or ""),
            "expr": str(q.get("expr") if q.get("expr") is not None else ""),
            "note": str(q.get("note") or ""),
        }
        for q in (case.get("params") or [])
    ]

    features = []
    for i, raw in enumerate(case.get("features") or [], 1):
        fid = f"f{i}"
        features.append(
            {
                "id": fid,
                "type": raw["type"],
                "params": _remap_source(dict(raw.get("params") or {}), mapping),
                **({"name": raw["name"]} if raw.get("name") else {}),
            }
        )
    doc.features = features

    results_raw = doc.evaluate()
    results = [
        {"id": r["id"], "error": r.get("error"), "warning": r.get("warning"),
         **({"info": r["info"]} if isinstance(r.get("info"), str) else {}),
         **({"thin": r["thin"]} if r.get("thin") else {})}
        for r in results_raw
    ]
    bodies = [_body_record(bid, b) for bid, b in doc.state.bodies.items()]
    return {"results": results, "bodies": bodies}


def _load_remesh_golden() -> dict:
    if not REMESH_GOLDEN.exists():
        return {}
    return json.loads(REMESH_GOLDEN.read_text(encoding="utf-8"))


def build_type(type_name: str, remesh_golden: dict | None = None) -> dict:
    path = FEATURES_DIR / f"{type_name}.json"
    cases = json.loads(path.read_text(encoding="utf-8"))
    remesh_golden = remesh_golden if remesh_golden is not None else {}
    out: dict[str, dict] = {}
    for case in cases:
        rec = _run_case(case)
        # Optional capability gates for the Rust parity runner (e.g. voxel, offset).
        if case.get("requires"):
            rec["requires"] = list(case["requires"])
        # Remesh: desktop first for errors; otherwise stats come from the web golden.
        if case.get("reference") == "web":
            rec["reference"] = "web"
            has_error = any(r.get("error") for r in rec.get("results") or [])
            if not has_error:
                web = remesh_golden.get(case["name"])
                if web is None:
                    raise RuntimeError(
                        f"make_golden: remesh case {case['name']!r} has no entry in "
                        f"{REMESH_GOLDEN.relative_to(ROOT)}; run remesh_golden.mjs first"
                    )
                stats = web["stats"]
                for body in rec.get("bodies") or []:
                    body["stats"] = stats
        out[case["name"]] = rec
    return {k: out[k] for k in sorted(out)}


def build_all() -> dict[str, dict]:
    remesh_golden = _load_remesh_golden()
    types = [*BATCH1, *BATCH2, *BATCH3]
    return {t: build_type(t, remesh_golden) for t in types}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="exit 1 if any golden differs")
    args = ap.parse_args()

    golden = build_all()
    mismatch = False
    types = [*BATCH1, *BATCH2, *BATCH3]
    for type_name, data in golden.items():
        text = _dumps(data)
        dest = GOLDEN_DIR / f"{type_name}.json"
        if args.check:
            existing = dest.read_text(encoding="utf-8") if dest.exists() else ""
            if _norm_newlines(existing) != _norm_newlines(text):
                print(f"make_golden: MISMATCH {dest.relative_to(ROOT)}", file=sys.stderr)
                mismatch = True
            continue
        GOLDEN_DIR.mkdir(parents=True, exist_ok=True)
        dest.write_text(text, encoding="utf-8", newline="\n")
        print(f"wrote {dest.relative_to(ROOT)} ({len(data)} cases)")

    if args.check:
        if mismatch:
            return 1
        print(f"make_golden: ok ({len(types)} types)")
        return 0
    return 0


if __name__ == "__main__":
    sys.exit(main())
