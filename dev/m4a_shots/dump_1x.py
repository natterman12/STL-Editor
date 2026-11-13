"""Dump a 1.x Document's bodies / planes / sketches / timeline / params as JSON.

Used by rust/crates/stl-app/tests/projects.rs to compare the 2.0 app's views
against the same file opened in 1.x.

Usage:
  py -3.14 dev/m4a_shots/dump_1x.py <path.stlx>
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))

from stleditor.document import Document  # noqa: E402

def dump(path: Path) -> dict:
    doc = Document.open(str(path))
    results = doc.evaluate()
    by_id = {r["id"]: r for r in results}

    bodies = []
    for bid, _body in doc.state.bodies.items():
        colors = doc.ui.get("colors") or {}
        hidden = set(doc.ui.get("hidden") or [])
        bodies.append(
            {
                "id": bid,
                "name": doc.body_name(bid),
                "color": colors.get(bid),
                "hidden": bid in hidden,
            }
        )
    bodies.sort(key=lambda b: b["id"])

    # User construction planes only (origin XY/XZ/YZ are UI chrome, not in Document.state).
    planes = []
    for pid, plane in (getattr(doc.state, "planes", None) or {}).items():
        name = plane.get("name") if isinstance(plane, dict) else getattr(plane, "name", None)
        if not name:
            feat = next((f for f in doc.features if f["id"] == pid), None)
            name = (feat or {}).get("name") or pid
        planes.append({"id": pid, "name": name})

    sketches = []
    for sid, sk in (getattr(doc.state, "sketches", None) or {}).items():
        feat = next((f for f in doc.features if f["id"] == sid), None)
        name = getattr(sk, "name", None) or (feat or {}).get("name") or sid
        plane = (feat or {}).get("params", {}).get("plane") if feat else ""
        if isinstance(sk, dict):
            name = sk.get("name") or name
            plane = sk.get("plane") or plane
        hidden = sid in set(doc.ui.get("hidden") or [])
        sketches.append(
            {"id": sid, "name": name, "plane": plane or "", "hidden": hidden}
        )
    sketches.sort(key=lambda s: s["id"])

    n = len(doc.features)
    active_count = n if doc.rollback is None else min(int(doc.rollback), n)
    items = []
    for i, f in enumerate(doc.features):
        rid = f["id"]
        r = by_id.get(rid) or {}
        label = f.get("name") or f.get("type") or rid
        items.append(
            {
                "id": rid,
                "kind": f.get("type"),
                "label": label,
                "suppressed": bool(f.get("suppressed")),
                "active": i < active_count,
                "error": r.get("error"),
            }
        )
    marker = None
    if doc.rollback is not None and int(doc.rollback) < n:
        marker = int(doc.rollback)

    params = []
    for p in doc.params or []:
        params.append(
            {
                "name": p.get("name"),
                "expr": p.get("expr"),
                "note": p.get("note") or "",
            }
        )

    return {
        "bodies": bodies,
        "planes": planes,
        "sketches": sketches,
        "timeline": {"marker": marker, "items": items},
        "params": params,
    }


def main(argv: list[str] | None = None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    if len(argv) != 1:
        print("usage: dump_1x.py <path.stlx>", file=sys.stderr)
        return 2
    path = Path(argv[0])
    if not path.is_file():
        print(f"dump_1x: not a file: {path}", file=sys.stderr)
        return 1
    # ensure_ascii=True (json.dumps default): escape non-ASCII so Windows
    # console code pages cannot mangle the bytes Rust parses as UTF-8 JSON.
    json.dump(dump(path), sys.stdout, ensure_ascii=True, indent=2)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
