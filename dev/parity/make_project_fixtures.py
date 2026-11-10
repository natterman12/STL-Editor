"""Build rust/tests/projects/{basic,unicode,extra_keys}.stlx from 1.x Document.

Uses small STLs in rust/tests/models/. Output is written with fixed zip
timestamps so regenerating does not churn the bytes more than necessary.

Usage:
  python dev/parity/make_project_fixtures.py
"""
from __future__ import annotations

import io
import json
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))

from stleditor.document import Document  # noqa: E402

MODELS = ROOT / "rust" / "tests" / "models"
OUT = ROOT / "rust" / "tests" / "projects"
ZIP_DATE = (1980, 1, 1, 0, 0, 0)


def _read_model(name: str) -> bytes:
    return (MODELS / name).read_bytes()


def _rewrite_zip_deterministic(path: Path, document_json: bytes | None = None) -> None:
    """Rewrite path with fixed entry timestamps; optionally replace document.json."""
    with zipfile.ZipFile(path, "r") as zin:
        items = []
        for info in sorted(zin.infolist(), key=lambda i: i.filename):
            data = zin.read(info.filename)
            if document_json is not None and info.filename == "document.json":
                data = document_json
            items.append((info.filename, data))
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as zout:
        for name, data in items:
            zi = zipfile.ZipInfo(name, date_time=ZIP_DATE)
            zi.compress_type = zipfile.ZIP_DEFLATED
            zout.writestr(zi, data)
    path.write_bytes(buf.getvalue())


def _save(doc: Document, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    doc.save(str(path))
    _rewrite_zip_deterministic(path)


def make_basic(path: Path) -> None:
    doc = Document()
    s1 = doc.add_source("holed_block.stl", _read_model("holed_block.stl"))
    s2 = doc.add_source("two_blobs.stl", _read_model("two_blobs.stl"))
    imp1, _ = doc.add_feature(
        "import", {"source": s1, "repair": True, "place": "keep"}
    )
    doc.add_feature("import", {"source": s2, "repair": True, "place": "keep"})
    doc.set_params(
        [
            {"name": "dx", "expr": "5", "note": "X shift"},
            {"name": "spare", "expr": "1", "note": "unused"},
        ]
    )
    doc.add_feature(
        "move",
        {
            "bodies": [imp1["id"]],
            "translate": ["=dx", 0, 0],
            "rotate": [0, 0, 0],
            "pivot": "center",
        },
    )
    doc.set_rollback(2)
    _save(doc, path)


def make_unicode(path: Path) -> None:
    doc = Document()
    sid = doc.add_source("Teil ö.stl", _read_model("holed_block.stl"))
    imp, _ = doc.add_feature(
        "import",
        {"source": sid, "repair": True, "place": "keep"},
        name="Teil ö",
    )
    doc.set_body_props(imp["id"], name="Grün")
    doc.set_params([{"name": "w", "expr": "10", "note": "größe"}])
    _save(doc, path)


def make_extra_keys(path: Path) -> None:
    doc = Document()
    sid = doc.add_source("holed_block.stl", _read_model("holed_block.stl"))
    imp, _ = doc.add_feature(
        "import", {"source": sid, "repair": True, "place": "keep"}
    )
    moved, _ = doc.add_feature(
        "move",
        {
            "bodies": [imp["id"]],
            "translate": [1, 0, 0],
            "rotate": [0, 0, 0],
            "pivot": "center",
        },
    )
    for feat in doc.features:
        if feat["id"] == moved["id"]:
            feat["suppressed"] = True
            feat["memo"] = "x"
            break
    doc.ui["camera"] = {"pos": [1, 2, 3]}
    _save(doc, path)


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    make_basic(OUT / "basic.stlx")
    make_unicode(OUT / "unicode.stlx")
    make_extra_keys(OUT / "extra_keys.stlx")
    print(f"wrote 3 fixtures in {OUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
