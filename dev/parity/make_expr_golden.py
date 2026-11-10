"""Generate rust/tests/parity/expr_golden.json from 1.x stleditor.params.

Reads rust/tests/parity/expr_cases.json, runs each case through the desktop
expression engine, and writes a deterministic golden file keyed by case name.

Usage:
  python dev/parity/make_expr_golden.py          # write golden
  python dev/parity/make_expr_golden.py --check  # exit 1 if golden differs
"""
from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))

from stleditor import params  # noqa: E402

CASES = ROOT / "rust" / "tests" / "parity" / "expr_cases.json"
GOLDEN = ROOT / "rust" / "tests" / "parity" / "expr_golden.json"


def _finite_value(x: float) -> dict:
    if math.isnan(x):
        return {"value_nonfinite": "nan"}
    if math.isinf(x):
        return {"value_nonfinite": "inf" if x > 0 else "-inf"}
    return {"value": x}


def _run_evaluate(case: dict) -> dict:
    try:
        v = params.evaluate(case["text"], case.get("vars") or {})
        return _finite_value(float(v))
    except params.ExprError as e:
        return {"error": str(e)}
    except Exception as e:
        return {"python_exception": type(e).__name__}


def _run_resolve(case: dict) -> dict:
    try:
        r = params.resolve_params(case.get("params") or [])
        values = {k: float(v) for k, v in (r.get("values") or {}).items()}
        errors = {k: str(v) for k, v in (r.get("errors") or {}).items()}
        # Non-finite values should not appear (evaluate rejects them), but keep the rule.
        out_values = {}
        for k, v in values.items():
            if not math.isfinite(v):
                tag = "nan" if math.isnan(v) else ("inf" if v > 0 else "-inf")
                out_values[k] = {"value_nonfinite": tag}
            else:
                out_values[k] = v
        return {"values": out_values, "errors": errors}
    except params.ExprError as e:
        return {"error": str(e)}
    except Exception as e:
        return {"python_exception": type(e).__name__}


def _run_names_in(case: dict) -> dict:
    try:
        names = sorted(params.names_in(case["text"]))
        return {"names": names}
    except params.ExprError as e:
        return {"error": str(e)}
    except Exception as e:
        return {"python_exception": type(e).__name__}


def _run_is_valid_name(case: dict) -> dict:
    text = case.get("text", "")
    # params.py has NAME_RE but no is_valid_name helper; match resolve_params.
    try:
        valid = bool(params.NAME_RE.match(text or ""))
        return {"valid": valid}
    except Exception as e:
        return {"python_exception": type(e).__name__}


def _run_resolve_deep(case: dict) -> dict:
    try:
        return {"value": params.resolve_deep(case.get("obj"), case.get("values") or {})}
    except params.ExprError as e:
        return {"error": str(e)}
    except Exception as e:
        return {"python_exception": type(e).__name__}


def _run_uses_expr(case: dict) -> dict:
    try:
        return {"uses": bool(params.uses_expr(case.get("obj")))}
    except Exception as e:
        return {"python_exception": type(e).__name__}


def run_case(case: dict) -> dict:
    kind = case["kind"]
    if kind == "evaluate":
        return _run_evaluate(case)
    if kind == "resolve":
        return _run_resolve(case)
    if kind == "names_in":
        return _run_names_in(case)
    if kind == "is_valid_name":
        return _run_is_valid_name(case)
    if kind == "resolve_deep":
        return _run_resolve_deep(case)
    if kind == "uses_expr":
        return _run_uses_expr(case)
    raise SystemExit(f"unknown case kind: {kind!r} ({case.get('name')})")


def build_golden(cases: list) -> dict:
    out = {}
    for case in cases:
        name = case["name"]
        if name in out:
            raise SystemExit(f"duplicate case name: {name}")
        out[name] = run_case(case)
    return out


def dumps(obj: dict) -> str:
    return json.dumps(obj, indent=1, sort_keys=True, ensure_ascii=False) + "\n"


def normalize_newlines(s: str) -> str:
    return s.replace("\r\n", "\n").replace("\r", "\n")


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--check", action="store_true", help="exit 1 if golden differs")
    args = p.parse_args(argv)

    cases = json.loads(CASES.read_text(encoding="utf-8"))
    if not isinstance(cases, list):
        raise SystemExit("expr_cases.json must be a JSON array")
    golden = build_golden(cases)
    text = dumps(golden)

    if args.check:
        if not GOLDEN.is_file():
            print(f"--check: missing {GOLDEN}", file=sys.stderr)
            return 1
        existing = normalize_newlines(GOLDEN.read_text(encoding="utf-8"))
        if existing == normalize_newlines(text):
            print(f"ok: {GOLDEN.relative_to(ROOT).as_posix()} matches ({len(golden)} cases)")
            return 0
        # Short diff summary: keys only present on one side, then first differing records.
        old = json.loads(existing) if existing.strip() else {}
        new_keys = set(golden)
        old_keys = set(old)
        lines = ["--check: expr_golden.json differs"]
        only_new = sorted(new_keys - old_keys)
        only_old = sorted(old_keys - new_keys)
        if only_new:
            lines.append(f"  only in regenerated: {only_new[:20]}")
        if only_old:
            lines.append(f"  only in file: {only_old[:20]}")
        changed = sorted(k for k in (new_keys & old_keys) if old[k] != golden[k])
        for k in changed[:20]:
            lines.append(f"  changed {k}: file={old[k]!r} regen={golden[k]!r}")
        if len(changed) > 20:
            lines.append(f"  ... and {len(changed) - 20} more changed")
        print("\n".join(lines), file=sys.stderr)
        return 1

    GOLDEN.parent.mkdir(parents=True, exist_ok=True)
    GOLDEN.write_text(text, encoding="utf-8", newline="\n")
    print(f"wrote {GOLDEN.relative_to(ROOT).as_posix()} ({len(golden)} cases)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
