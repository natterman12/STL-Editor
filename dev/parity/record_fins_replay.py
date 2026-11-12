"""Record the Support Fins engine calls 1.x makes for every supportfins / supportwall parity case.

Writes rust/tests/parity/fins_replay.json: {"version": "...", "has": [names], "calls": {key: result}},
key = "<name>:<sha256 of the arguments joined by NUL>". tests/fins_replay.rs answers FinsHost calls
from it, so the Rust engine can be tested without a JavaScript engine.

  python dev/parity/record_fins_replay.py [--check]
"""
from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "dev" / "parity"))

import make_golden as MG  # noqa: E402
from stleditor import supportfins as SF  # noqa: E402

OUT = ROOT / "rust" / "tests" / "parity" / "fins_replay.json"


import struct

NO_SOUP = ("optionsFromDialogJson", "reportLine", "optionsSchemaJson")


def _canon_value(v):
    """JSON with every number as its float64 bits, so Python and Rust agree on the text."""
    if isinstance(v, bool) or v is None:
        return json.dumps(v)
    if isinstance(v, (int, float)):
        return "f" + struct.pack(">d", float(v)).hex()
    if isinstance(v, str):
        return json.dumps(v)
    if isinstance(v, list):
        return "[" + ",".join(_canon_value(x) for x in v) + "]"
    return "{" + ",".join(json.dumps(k) + ":" + _canon_value(x) for k, x in v.items()) + "}"


def canon(a):
    s = str(a)
    if s[:1] in "[{" and len(s) < 100000:
        try:
            return _canon_value(json.loads(s))
        except ValueError:
            pass
    return s


def sha(s):
    return hashlib.sha256(s.encode("utf-8")).hexdigest()


def entry(name, args, result):
    c = [canon(a) for a in args]
    soup = len(c[0]) if name not in NO_SOUP and c else 0
    rest = c[1:] if soup else c
    return {"n": name, "x": sha("\0".join(c)), "r": sha("\0".join(rest)), "l": soup, "v": result}


def uniq(calls):
    seen = {}
    for c in calls:
        seen[(c["n"], c["x"])] = c
    return sorted(seen.values(), key=lambda c: (c["n"], c["x"]))


def main():
    MG._force_bundled_fins()
    calls, has = [], set()
    real_call, real_has = SF._call, SF._has

    def call(name, *args):
        r = real_call(name, *args)
        # the host takes strings: an object or a number goes over as its JSON
        calls.append(entry(name, [a if isinstance(a, str) else json.dumps(a) for a in args], r))
        return r

    def hasf(name):
        r = real_has(name)
        if r:
            has.add(name)
        return r

    SF._call, SF._has = call, hasf
    for t in MG.BATCH5:
        for case in json.loads((MG.FEATURES_DIR / f"{t}.json").read_text(encoding="utf-8")):
            MG._run_case(case)
    for n in ("computeFinsB64", "reportLine", "optionsFromDialogJson", "drawWallB64", "drawWallThroughB64",
              "gussetWallB64", "anchorFinB64", "suggestPosesJson", "strengthOfJson", "strongestPoseJson"):
        if real_has(n):
            has.add(n)
    SF._engine()
    info = {k: SF._active.get(k) for k in ("version", "tag", "commit", "source")}
    data = {"info": info, "has": sorted(has), "calls": uniq(calls)}
    text = json.dumps(data, separators=(",", ":")) + "\n"
    if "--check" in sys.argv:
        old = json.loads(OUT.read_text(encoding="utf-8")) if OUT.exists() else {}
        if old != data:
            print("record_fins_replay: out of date", file=sys.stderr)
            return 1
        print("record_fins_replay: ok")
        return 0
    OUT.write_text(text, encoding="utf-8", newline="\n")
    print(f"wrote {OUT} ({len(calls)} calls, {len(text)} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
