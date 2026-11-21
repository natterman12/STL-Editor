"""Bring the 2.0 golden files up to a newly bundled Support Fins engine, and say whether the fins changed.

Run after packaging/update_support_fins.py has bundled the new engine (the daily "Support Fins update"
workflow does this on v2-main). The golden files that depend on the engine are rebuilt with it:

  rust/tests/parity/features_golden/supportfins.json, supportwall.json   (make_golden.py, BATCH5 only)
  rust/tests/parity/fins_golden.json                                     (make_fins_golden.py)
  rust/tests/parity/fins_replay.json                                     (record_fins_replay.py)

Then each is compared with what was committed, with the old version number put in place of the new:
  - the same (within make_golden's tolerances; replayed answers equal as values): the committed file is
    kept with only the version changed, so a CI runner's last-digit differences don't churn it;
  - different: the rebuilt file is kept and the file is listed under "changed", for a person to look at.

  python dev/parity/fins_bump.py [--summary summary.json]

Exit 0 either way (1 on an error). The summary: {"old", "new", "changed": [paths], "notes": [lines]}.
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "dev" / "parity"))

PARITY = ROOT / "rust" / "tests" / "parity"
REPLAY = PARITY / "fins_replay.json"
FINS_GOLDEN = PARITY / "fins_golden.json"
VERSION_JSON = ROOT / "web" / "vendor" / "support-fins" / "VERSION.json"


def rel(p: Path) -> str:
    return p.relative_to(ROOT).as_posix()


def read(p: Path) -> str:
    return p.read_text(encoding="utf-8").replace("\r\n", "\n") if p.exists() else ""


def write(p: Path, text: str) -> None:
    p.write_text(text, encoding="utf-8", newline="\n")


def value(v):
    """A replayed answer as a value: JSON text parsed, so 1 and 1.0 or key order don't count."""
    if isinstance(v, str):
        try:
            return json.loads(v)
        except ValueError:
            return v
    return v


def same_replay(old: dict, new: dict) -> bool:
    oc = {(c["n"], c["x"]): c for c in old.get("calls", [])}
    nc = {(c["n"], c["x"]): c for c in new.get("calls", [])}
    if oc.keys() != nc.keys():
        return False
    return all(value(oc[k]["v"]) == value(nc[k]["v"]) for k in oc)


def bump(summary_path: Path | None) -> int:
    import make_golden as MG

    info = json.loads(read(VERSION_JSON))
    new = info["version"]
    old_replay_text = read(REPLAY)
    old_replay = json.loads(old_replay_text) if old_replay_text else {}
    old = (old_replay.get("info") or {}).get("version") or ""
    out = {"old": old, "new": new, "changed": [], "notes": []}
    if old == new:
        out["notes"].append(f"The golden files already have Support Fins {new}.")
        return finish(out, summary_path)

    def swap(text: str) -> str:
        return text.replace(f"Support Fins {old}:", f"Support Fins {new}:")

    # the user-level results: supportfins / supportwall cases, with 1.x's info line naming the version
    MG._force_bundled_fins()
    remesh = MG._load_remesh_golden()
    for t in MG.BATCH5:
        dest = MG.GOLDEN_DIR / f"{t}.json"
        before = swap(read(dest))
        data = MG.build_type(t, remesh)
        problems, machine = MG._compare_records(json.loads(before) if before else {}, data)
        if problems:
            write(dest, MG._dumps(data))
            out["changed"].append(rel(dest))
            out["notes"].extend(f"{rel(dest)}: {p}" for p in problems[:20])
        else:
            write(dest, before)
            out["notes"].extend(f"{rel(dest)}: machine difference kept as committed: {m}" for m in machine)

    # the engine's own answers for the test models
    before = read(FINS_GOLDEN)
    r = subprocess.run([sys.executable, str(ROOT / "dev/parity/make_fins_golden.py")], cwd=ROOT)
    if r.returncode:
        return r.returncode
    if read(FINS_GOLDEN) != before:
        out["changed"].append(rel(FINS_GOLDEN))
        out["notes"].append(f"{rel(FINS_GOLDEN)}: the engine's walls, tines or report for the test models changed")

    # every engine call the parity cases make, which the Rust tests replay
    r = subprocess.run([sys.executable, str(ROOT / "dev/parity/record_fins_replay.py")], cwd=ROOT)
    if r.returncode:
        return r.returncode
    new_replay = json.loads(read(REPLAY))
    if old_replay and same_replay(old_replay, new_replay):
        kept = dict(old_replay)
        kept["info"] = new_replay["info"]
        kept["has"] = new_replay["has"]
        if kept["has"] != old_replay.get("has"):
            out["notes"].append(f"{rel(REPLAY)}: the engine's list of calls changed: {new_replay['has']}")
        write(REPLAY, json.dumps(kept, separators=(",", ":")) + "\n")
    else:
        out["changed"].append(rel(REPLAY))
        out["notes"].append(f"{rel(REPLAY)}: some replayed engine answers changed")
    return finish(out, summary_path)


def finish(out: dict, summary_path: Path | None) -> int:
    text = json.dumps(out, indent=1)
    print(text)
    if summary_path:
        summary_path.write_text(text + "\n", encoding="utf-8")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--summary", type=Path, help="also write the summary JSON here")
    args = ap.parse_args()
    return bump(args.summary)


if __name__ == "__main__":
    sys.exit(main())
