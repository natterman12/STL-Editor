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
from stleditor.features import State  # noqa: E402
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

BATCH4 = [
    "bedsplit",
    "layflat",
    "plate",
    "pattern",
    "thread",
    "resizehole",
    "fittest",
    "snapfit",
    "hinge",
]

BATCH5 = [
    "supportfins",
    "supportwall",
]

ALL_TYPES = [*BATCH1, *BATCH2, *BATCH3, *BATCH4, *BATCH5]

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


def _force_bundled_fins() -> None:
    """Support Fins goldens use the bundled engine only; update checks stay off."""
    import tempfile

    from stleditor import supportfins as SF

    SF.USER_DIR = tempfile.mkdtemp(prefix="sf-make-golden-")
    SF._ctx = None
    SF._active = None
    SF.check_update = lambda force=False: None  # type: ignore[assignment]
    info = SF._read_info(SF.BUNDLED)
    if info:
        SF._choose = lambda: info  # type: ignore[assignment]


def _evaluate_with_ctx(doc: Document, extra_ctx: dict) -> list:
    """Like Document.evaluate, but merges extra_ctx (e.g. preview) into the feature ctx."""
    ctx = {"load_source": doc.load_source, **extra_ctx}
    state = State()
    results = []
    new_cache = []
    h = ""
    for i, f in enumerate(doc.active_features()):
        h = doc._hash(f, h)
        if i < len(doc._cache) and doc._cache[i][0] == h and i < len(doc._results):
            state = doc._cache[i][1]
            results.append(doc._results[i])
            new_cache.append(doc._cache[i])
            continue
        if h in doc._memo:
            state, res = doc._memo[h]
        else:
            state, res = doc.run_feature(state, f, ctx)
        results.append(res)
        new_cache.append((h, state))
    doc._cache = new_cache
    doc._results = results
    doc.state = state
    return results


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

    extra_ctx = case.get("ctx") or {}
    if extra_ctx:
        results_raw = _evaluate_with_ctx(doc, dict(extra_ctx))
    else:
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
    _force_bundled_fins()
    remesh_golden = _load_remesh_golden()
    return {t: build_type(t, remesh_golden) for t in ALL_TYPES}


_REL = 1e-2      # volume, area and bounding box: 1 percent here (freshness check only; parity keeps 0.1 percent).
                 # A tilted round pin can get one more or one less side on another machine.
_ABS = 1e-4      # or 1e-4 mm near zero
_COUNT = 0.10    # triangle and vertex counts: up to 10 percent between machines


def _close(a, b):
    return abs(a - b) <= max(_ABS, _REL * max(abs(a), abs(b)))


def _compare_records(old: dict, new: dict):
    """Compare two golden files for one type. Returns (problems, machine_differences)."""
    problems, machine = [], []
    if sorted(old) != sorted(new):
        return [f"case names differ: {sorted(set(old) ^ set(new))}"], machine
    for name in sorted(new):
        o, n = old[name], new[name]
        if {k: v for k, v in o.items() if k != "bodies"} != {k: v for k, v in n.items() if k != "bodies"}:
            problems.append(f"{name}: results, messages or flags differ")
            continue
        ob, nb = o.get("bodies") or [], n.get("bodies") or []
        if [(b.get("id"), b.get("name"), b.get("solid")) for b in ob] != [(b.get("id"), b.get("name"), b.get("solid")) for b in nb]:
            problems.append(f"{name}: bodies differ")
            continue
        for b1, b2 in zip(ob, nb):
            s1, s2 = b1.get("stats") or {}, b2.get("stats") or {}
            if sorted(s1) != sorted(s2) or s1.get("watertight") != s2.get("watertight"):
                problems.append(f"{name}/{b2.get('id')}: stats fields or watertight differ")
                continue
            for k in ("volume", "area"):
                if k in s1 and not _close(s1[k], s2[k]):
                    problems.append(f"{name}/{b2.get('id')}: {k} {s1[k]} vs {s2[k]}")
            for k in ("bbox_min", "bbox_max"):
                if k in s1 and not all(_close(x, y) for x, y in zip(s1[k], s2[k])):
                    problems.append(f"{name}/{b2.get('id')}: {k} differs")
            for k in ("triangles", "vertices"):
                if k in s1 and s1[k] != s2[k]:
                    if abs(s1[k] - s2[k]) <= _COUNT * max(s1[k], s2[k]):
                        machine.append(f"{name}/{b2.get('id')}: {k} {s1[k]} vs {s2[k]}")
                    else:
                        problems.append(f"{name}/{b2.get('id')}: {k} {s1[k]} vs {s2[k]}")
    return problems, machine


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="exit 1 if any golden differs")
    args = ap.parse_args()

    golden = build_all()
    mismatch = False
    for type_name, data in golden.items():
        text = _dumps(data)
        dest = GOLDEN_DIR / f"{type_name}.json"
        if args.check:
            existing = dest.read_text(encoding="utf-8") if dest.exists() else ""
            if _norm_newlines(existing) != _norm_newlines(text):
                problems, machine = _compare_records(json.loads(existing) if existing else {}, data)
                if not problems:
                    # 1.x is not bit-identical across Windows builds (the C runtime's sin/cos feed
                    # Manifold's round shapes), so a CI runner can tessellate connector pins a little
                    # differently. The golden files from Nathan's PC stay the reference; here only
                    # geometry within the spec's tolerances may differ.
                    for line in machine:
                        print(f"make_golden: machine difference (allowed) {dest.relative_to(ROOT)}: {line}")
                    continue
                for line in problems[:20]:
                    print(f"  {line}", file=sys.stderr)
                print(f"make_golden: MISMATCH {dest.relative_to(ROOT)}", file=sys.stderr)
                import difflib
                diff = difflib.unified_diff(_norm_newlines(existing).splitlines(), _norm_newlines(text).splitlines(),
                                            "committed", "generated", n=2, lineterm="")
                for line in list(diff)[:60]:
                    print("  " + line, file=sys.stderr)
                mismatch = True
            continue
        GOLDEN_DIR.mkdir(parents=True, exist_ok=True)
        dest.write_text(text, encoding="utf-8", newline="\n")
        print(f"wrote {dest.relative_to(ROOT)} ({len(data)} cases)")

    if args.check:
        if mismatch:
            return 1
        print(f"make_golden: ok ({len(ALL_TYPES)} types)")
        return 0
    return 0


if __name__ == "__main__":
    sys.exit(main())
