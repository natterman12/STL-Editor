"""Open each .stlx in a directory with 1.x Document and compare feature errors.

For every .stlx in <dir>, open and evaluate with Document.open. Fail if any
feature result has an error that the original fixture of the same name in
rust/tests/projects/ did not have. Also fail if <dir> has fewer than 3 .stlx
files.

Usage:
  python dev/parity/open_check.py rust/tests/projects
  python dev/parity/open_check.py rust/target/parity-out
"""
from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))

from stleditor.document import Document  # noqa: E402

ORIGINALS = ROOT / "rust" / "tests" / "projects"


def _errors_by_id(path: Path) -> dict[str, str | None]:
    doc = Document.open(str(path))
    results = doc.evaluate()
    return {r["id"]: r.get("error") for r in results}


def main(argv: list[str] | None = None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    if len(argv) != 1:
        print("usage: open_check.py <dir>", file=sys.stderr)
        return 2
    target = Path(argv[0])
    if not target.is_dir():
        print(f"open_check: not a directory: {target}", file=sys.stderr)
        return 1

    files = sorted(p for p in target.iterdir() if p.suffix.lower() == ".stlx")
    if len(files) < 3:
        print(f"open_check: expected at least 3 .stlx files, found {len(files)}")
        return 1

    problems: list[str] = []
    for path in files:
        original = ORIGINALS / path.name
        if not original.is_file():
            problems.append(f"{path.name}: no original fixture at {original}")
            continue
        try:
            got = _errors_by_id(path)
        except Exception as e:
            problems.append(f"{path.name}: open failed: {type(e).__name__}: {e}")
            continue
        try:
            baseline = _errors_by_id(original)
        except Exception as e:
            problems.append(
                f"{path.name}: original open failed: {type(e).__name__}: {e}"
            )
            continue
        for fid, err in got.items():
            if not err:
                continue
            base_err = baseline.get(fid)
            if err != base_err:
                problems.append(
                    f"{path.name}: feature {fid}: error {err!r} "
                    f"(original {base_err!r})"
                )
        for fid, err in baseline.items():
            if err and fid not in got:
                problems.append(
                    f"{path.name}: feature {fid}: missing result "
                    f"(original error {err!r})"
                )

    if problems:
        for line in problems:
            print(line)
        return 1
    print(f"open_check: {len(files)} ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
