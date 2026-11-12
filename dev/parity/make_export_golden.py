"""Generate rust/tests/parity/export/ from 1.x Document.export + trimesh 5.2.0.

For each case in cases.json, builds a 1.x Document (same import path as the app:
import_file(repair=False, place=\"keep\"), set body names, hide bodies).

Writer-level goldens (out.stl / out.obj / 3MF XML entries) come from
geometry.export_bodies on the raw source-file meshes so float bits (e.g. -0.0)
match what the 2.0 writers see when they load the same models. 3MF XML entries
are stored with every UUID=\"...\" value replaced by UUID. Files over 1 MB are
stored as {sha256, size} in golden.json instead of on disk.

Document-layer goldens: for each case with bodies, doc.export to a temp dir for
.stl, .obj and .3mf; golden.json records under \"document\":
  {\"stl\": {sha256, size}, \"obj\": {sha256, size},
   \"3mf_model_xml\": {sha256, size}}
where 3mf_model_xml is 3D/3dmodel.model after UUID normalisation and CRLF→LF.
count / base / error come from that Document (export return / export_base_name /
GeometryError).

STEP bytes and step_error are owned by dev/parity/js/step_golden.mjs; this script
preserves an existing step_error when rewriting golden.json and ignores that field
under --check.

Usage:
  python dev/parity/make_export_golden.py          # write goldens
  python dev/parity/make_export_golden.py --check  # exit 1 if golden differs
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
import tempfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "dev" / "parity"))

from stleditor.document import Document  # noqa: E402
from stleditor.geometry import GeometryError  # noqa: E402
from stleditor import geometry as G  # noqa: E402
import stats as parity_stats  # noqa: E402

MODELS = ROOT / "rust" / "tests" / "models"
OUT = ROOT / "rust" / "tests" / "parity" / "export"
CASES = OUT / "cases.json"
ONE_MB = 1 << 20
UUID_RE = re.compile(r'(UUID=")[^"]*"')
ARTIFACTS = ("out.stl", "out.obj", "3dmodel.model", ".rels", "[Content_Types].xml")


def _dumps(obj) -> str:
    return json.dumps(obj, indent=1, sort_keys=True, ensure_ascii=False) + "\n"


def _norm_newlines(s: str) -> str:
    return s.replace("\r\n", "\n").replace("\r", "\n")


def _norm_bytes_text(data: bytes) -> bytes:
    if b"\0" in data[:1024]:
        return data
    try:
        return _norm_newlines(data.decode("utf-8")).encode("utf-8")
    except UnicodeDecodeError:
        return data


def find_model(file: str) -> Path:
    for folder in (MODELS, MODELS / "export", MODELS / "readers"):
        path = folder / file
        if path.is_file():
            return path
    raise FileNotFoundError(f"model not found: {file}")


def export_base_name(doc: Document, ids: list[str], project_path: str | None, title: str | None) -> str:
    if len(ids) == 1:
        return doc.body_name(ids[0])
    if project_path:
        base = os.path.splitext(os.path.basename(project_path))[0]
    elif title:
        base = title
    else:
        base = "model"
    if base.lower().endswith(".stlx"):
        base = base[:-5]
    return base


def _resolve_ids(doc: Document, case: dict) -> list[str] | None:
    raw = case.get("body_ids")
    if raw is None:
        return None
    by_name = {doc.body_name(b): b for b in doc.state.bodies}
    return [by_name[n] for n in raw]


def _exported_ids(doc: Document, body_ids: list[str] | None) -> list[str]:
    return list(
        body_ids
        or [b for b in doc.state.bodies if b not in doc.ui["hidden"]]
        or list(doc.state.bodies)
    )


def _build_doc(case: dict) -> Document:
    doc = Document()
    if case.get("path"):
        doc.path = case["path"]
    if case.get("title") is not None:
        doc.title = case["title"]
    for m in case.get("models") or []:
        path = find_model(m["file"])
        feat, _ = doc.import_file(m["file"], path.read_bytes(), repair=False, place="keep")
        doc.set_body_props(feat["id"], name=m["name"])
    hidden_names = set(case.get("hidden") or [])
    for bid in list(doc.state.bodies):
        if doc.body_name(bid) in hidden_names:
            doc.set_body_props(bid, hidden=True)
    return doc


def _readback(path: Path) -> list[dict]:
    parts = G.load_mesh_file(path.read_bytes(), path.name)
    return [{"name": name, "stats": parity_stats.mesh_stats(verts, faces)} for name, verts, faces in parts]


def _uuid_norm(text: str) -> str:
    return UUID_RE.sub(r'\1UUID"', text)


def _pack_artifact(name: str, data: bytes) -> tuple[dict | None, bytes | None]:
    """Return (meta_entry, file_bytes). One of the two is set."""
    if len(data) > ONE_MB:
        return {"sha256": hashlib.sha256(data).hexdigest(), "size": len(data)}, None
    return None, data


def _raw_bodies(case: dict, doc: Document, exported_ids: list[str]) -> list:
    """Bodies for export_bodies from the source STL bytes (keeps -0.0); names from the doc."""
    from stleditor.geometry import Body

    name_to_file = {m["name"]: m["file"] for m in (case.get("models") or [])}
    bodies = []
    for bid in exported_ids:
        name = doc.body_name(bid)
        path = find_model(name_to_file[name])
        parts = G.load_mesh_file(path.read_bytes(), path.name)
        # One STL → one part; match Document's single-body import.
        _, verts, faces = parts[0]
        bodies.append(Body(name, verts=verts, faces=faces))
    return bodies


def _file_digest(data: bytes) -> dict:
    return {"sha256": hashlib.sha256(data).hexdigest(), "size": len(data)}


def _document_exports(doc: Document, body_ids: list[str] | None, tmp_path: Path) -> tuple[int, dict]:
    """Call doc.export for STL/OBJ/3MF; return (count, document digest block)."""
    document: dict = {}
    count = 0
    for ext, key in ((".stl", "stl"), (".obj", "obj"), (".3mf", "3mf_model_xml")):
        out_path = tmp_path / f"doc{ext}"
        count = doc.export(str(out_path), body_ids)
        if key == "3mf_model_xml":
            with zipfile.ZipFile(out_path) as z:
                text = _norm_newlines(_uuid_norm(z.read("3D/3dmodel.model").decode("utf-8")))
            document[key] = _file_digest(text.encode("utf-8"))
        else:
            document[key] = _file_digest(out_path.read_bytes())
    return count, document


def generate_case(case: dict) -> tuple[dict, dict[str, bytes]]:
    """Return (golden_dict without step_error, artifact_name -> bytes).

    Document decides which bodies, count, base name and error; its export bytes
    are recorded under golden[\"document\"]. Writer-level artifact bytes come from
    geometry.export_bodies on the source-file meshes so float bits (e.g. -0.0)
    match what the 2.0 writers see when they load the same models.
    """
    doc = _build_doc(case)
    body_ids = _resolve_ids(doc, case)
    meta: dict = {}
    files: dict[str, bytes] = {}
    readback = {"3mf": [], "obj": [], "stl": []}
    error = None
    count = 0
    base = "model"
    document: dict | None = None

    with tempfile.TemporaryDirectory(prefix="export-golden-") as tmp:
        tmp_path = Path(tmp)
        try:
            exported = _exported_ids(doc, body_ids)
            if not exported:
                raise GeometryError("There is nothing to export.")
            base = export_base_name(doc, exported, case.get("path"), case.get("title"))

            doc_dir = tmp_path / "document"
            doc_dir.mkdir(parents=True, exist_ok=True)
            count, document = _document_exports(doc, body_ids, doc_dir)

            bodies = _raw_bodies(case, doc, exported)
            for ext, key in ((".stl", "stl"), (".obj", "obj"), (".3mf", "3mf")):
                out_path = tmp_path / f"out{ext}"
                G.export_bodies(bodies, str(out_path))
                if ext == ".3mf":
                    with zipfile.ZipFile(out_path) as z:
                        mapping = {
                            "3D/3dmodel.model": "3dmodel.model",
                            "_rels/.rels": ".rels",
                            "[Content_Types].xml": "[Content_Types].xml",
                        }
                        for entry, out_name in mapping.items():
                            raw = _uuid_norm(z.read(entry).decode("utf-8")).encode("utf-8")
                            entry_meta, entry_bytes = _pack_artifact(out_name, raw)
                            if entry_meta is not None:
                                meta[out_name] = entry_meta
                            else:
                                files[out_name] = entry_bytes  # type: ignore[assignment]
                else:
                    data = out_path.read_bytes()
                    entry_meta, entry_bytes = _pack_artifact(f"out{ext}", data)
                    if entry_meta is not None:
                        meta[f"out{ext}"] = entry_meta
                    else:
                        files[f"out{ext}"] = entry_bytes  # type: ignore[assignment]
                try:
                    readback[key] = _readback(out_path)
                except GeometryError:
                    readback[key] = []
        except GeometryError as e:
            error = str(e)
            count = 0
            exported = _exported_ids(doc, body_ids) if doc.state.bodies else []
            base = export_base_name(doc, exported, case.get("path"), case.get("title"))
            document = None
            files.clear()
            meta.clear()
            readback = {"3mf": [], "obj": [], "stl": []}

    golden = {
        "base": base,
        "count": count,
        "error": error,
        "readback": readback,
        **meta,
    }
    if document is not None:
        golden["document"] = document
    return golden, files


def load_cases() -> dict:
    if not CASES.is_file():
        raise FileNotFoundError(f"missing {CASES.relative_to(ROOT)}")
    return json.loads(CASES.read_text(encoding="utf-8"))


def _strip_step(g: dict) -> dict:
    out = {k: v for k, v in g.items() if k != "step_error"}
    return out


def write_case(name: str, case: dict) -> None:
    golden, files = generate_case(case)
    case_dir = OUT / name
    case_dir.mkdir(parents=True, exist_ok=True)

    prev_step = None
    gp = case_dir / "golden.json"
    if gp.exists():
        try:
            prev_step = json.loads(gp.read_text(encoding="utf-8")).get("step_error")
        except json.JSONDecodeError:
            prev_step = None

    # Writer-level artifacts are pinned for stl-io; keep existing files, only
    # create missing ones. Document digests live in golden.json alone.
    for fname in ARTIFACTS:
        p = case_dir / fname
        if fname not in files:
            if p.exists() and fname not in golden:
                p.unlink()
            continue
        if not p.exists():
            p.write_bytes(files[fname])

    to_write = {**golden, "step_error": prev_step}
    gp.write_text(_dumps(to_write), encoding="utf-8", newline="\n")
    print(f"wrote {case_dir.relative_to(ROOT)}/")


def check_case(name: str, case: dict) -> list[str]:
    mismatches: list[str] = []
    case_dir = OUT / name
    gp = case_dir / "golden.json"
    if not gp.exists():
        return [f"{name}/golden.json (missing)"]

    existing = json.loads(gp.read_text(encoding="utf-8"))
    golden, files = generate_case(case)

    if _norm_newlines(_dumps(_strip_step(existing))) != _norm_newlines(_dumps(golden)):
        mismatches.append(f"{name}/golden.json")

    for fname, data in files.items():
        path = case_dir / fname
        if not path.exists():
            mismatches.append(f"{name}/{fname} (missing)")
            continue
        got = path.read_bytes()
        # Text artifacts: normalise CRLF. Binary STL/OBJ: raw compare (OBJ is text).
        if fname.endswith((".model", ".xml")) or fname == ".rels" or fname.endswith(".obj"):
            if _norm_bytes_text(got) != _norm_bytes_text(data):
                mismatches.append(f"{name}/{fname}")
        elif got != data:
            mismatches.append(f"{name}/{fname}")

    for fname in ARTIFACTS:
        if fname in files:
            continue
        if fname in golden:
            # large-file placeholder lives in golden only
            continue
        if (case_dir / fname).exists():
            mismatches.append(f"{name}/{fname} (unexpected)")

    return mismatches


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="exit 1 if golden differs")
    args = ap.parse_args()

    try:
        cases = load_cases()
    except FileNotFoundError as e:
        print(f"make_export_golden: {e}", file=sys.stderr)
        return 1

    if args.check:
        mismatches: list[str] = []
        for name, case in cases.items():
            mismatches.extend(check_case(name, case))
        if mismatches:
            for m in mismatches:
                print(f"make_export_golden: MISMATCH {m}", file=sys.stderr)
            return 1
        print(f"make_export_golden: ok ({len(cases)} cases)")
        return 0

    for name, case in cases.items():
        write_case(name, case)
    print(f"make_export_golden: wrote {len(cases)} cases")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
