"""Write rust/tests/models/readers/* for mesh-reader parity.

Deterministic bytes on re-run: hand-written files (and fixed ZIP dates for 3MF).
"""
from __future__ import annotations

import io
import json
import struct
import sys
import zipfile
from pathlib import Path
from zipfile import ZipInfo

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "rust" / "tests" / "models" / "readers"

# Fixed DOS date for every 3MF zip entry (1980-01-01 00:00:00).
_ZIP_DATE = (1980, 1, 1, 0, 0, 0)


def _write(name: str, data: bytes) -> Path:
    OUT.mkdir(parents=True, exist_ok=True)
    path = OUT / name
    path.write_bytes(data)
    return path


def _stl_binary(verts: np.ndarray, faces: np.ndarray) -> bytes:
    v = np.asarray(verts, np.float32)
    f = np.asarray(faces, np.int64)
    tri = v[f]
    n = np.cross(tri[:, 1] - tri[:, 0], tri[:, 2] - tri[:, 0])
    ln = np.linalg.norm(n, axis=1, keepdims=True)
    n = np.divide(n, ln, out=np.zeros_like(n), where=ln > 0)
    rec = np.zeros(len(f), dtype=np.dtype([("n", "<3f4"), ("v", "<9f4"), ("a", "<u2")]))
    rec["n"] = n
    rec["v"] = tri.reshape(-1, 9)
    header = b"STL Editor binary STL".ljust(80, b" ")
    return header + struct.pack("<I", len(f)) + rec.tobytes()


def _unit_cube_vf(origin=(0.0, 0.0, 0.0), size=10.0):
    o = np.asarray(origin, np.float64)
    s = float(size)
    v = np.array(
        [
            o + [0, 0, 0],
            o + [s, 0, 0],
            o + [s, s, 0],
            o + [0, s, 0],
            o + [0, 0, s],
            o + [s, 0, s],
            o + [s, s, s],
            o + [0, s, s],
        ],
        dtype=np.float32,
    )
    f = np.array(
        [
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ],
        dtype=np.uint32,
    )
    return v, f


def make_empty_obj() -> Path:
    return _write("empty.obj", b"# empty file with no triangles\n")


def make_multi_obj() -> Path:
    # Several objects, groups, a quad, an n-gon, and negative face indices.
    text = """# multi-object OBJ for reader parity
o BoxA
v 0 0 0
v 10 0 0
v 10 10 0
v 0 10 0
v 0 0 10
v 10 0 10
v 10 10 10
v 0 10 10
g Bottom
f 1 2 3 4
g Top
f 5 8 7 6
g Sides
f 1 5 6 2
f 2 6 7 3
f 3 7 8 4
f 4 8 5 1
o TriNeg
v 20 0 0
v 30 0 0
v 25 8 0
f -3 -2 -1
o Pentagon
v 40 0 0
v 48 0 0
v 50 6 0
v 45 12 0
v 38 8 0
f 12 13 14 15 16
"""
    return _write("multi.obj", text.encode("ascii"))


def make_materials_obj() -> Path:
    # usemtl grouping: faces before first usemtl, then a/b/a; 1.x dumps reverse
    # first-appearance order as separate parts.
    text = """# materials OBJ for reader parity
v 0 0 0
v 1 0 0
v 0 1 0
v 1 1 0
v 0.5 0.5 1
f 1 2 3
usemtl a
f 1 2 3
usemtl b
f 1 3 4
usemtl a
f 1 4 5
"""
    return _write("materials.obj", text.encode("ascii"))


def make_ascii_stl() -> Path:
    v, f = _unit_cube_vf()
    lines = ["solid ascii_cube"]
    for tri in f:
        a, b, c = v[tri]
        n = np.cross(b - a, c - a)
        ln = np.linalg.norm(n)
        if ln > 0:
            n = n / ln
        lines.append(f"facet normal {n[0]:.6g} {n[1]:.6g} {n[2]:.6g}")
        lines.append(" outer loop")
        for p in (a, b, c):
            lines.append(f"  vertex {p[0]:.6g} {p[1]:.6g} {p[2]:.6g}")
        lines.append(" endloop")
        lines.append("endfacet")
    lines.append("endsolid ascii_cube")
    lines.append("")
    return _write("ascii_cube.stl", "\n".join(lines).encode("ascii"))


def make_binary_stl() -> Path:
    return _write("binary_cube.stl", _stl_binary(*_unit_cube_vf(origin=(1.0, 2.0, 3.0), size=8.0)))


def make_ply_ascii() -> Path:
    text = """ply
format ascii 1.0
comment STL Editor reader parity
element vertex 4
property float x
property float y
property float z
property uchar red
property uchar green
property uchar blue
property float intensity
element face 2
property list uchar int vertex_indices
end_header
0 0 0 255 0 0 0.1
10 0 0 0 255 0 0.2
10 10 0 0 0 255 0.3
0 10 0 255 255 0 0.4
3 0 1 2
3 0 2 3
"""
    return _write("extra_props_ascii.ply", text.encode("ascii"))


def make_ply_binary_le() -> Path:
    header = b"""ply
format binary_little_endian 1.0
comment STL Editor reader parity
element vertex 4
property float x
property float y
property float z
property uchar red
property uchar green
property uchar blue
property float intensity
element face 2
property list uchar int vertex_indices
end_header
"""
    verts = [
        (0.0, 0.0, 0.0, 255, 0, 0, 0.1),
        (10.0, 0.0, 0.0, 0, 255, 0, 0.2),
        (10.0, 10.0, 0.0, 0, 0, 255, 0.3),
        (0.0, 10.0, 0.0, 255, 255, 0, 0.4),
    ]
    body = bytearray()
    for x, y, z, r, g, b, inten in verts:
        body += struct.pack("<fffBBBf", x, y, z, r, g, b, inten)
    for face in ((0, 1, 2), (0, 2, 3)):
        body += struct.pack("<Biii", 3, *face)
    return _write("extra_props_binary.ply", header + bytes(body))


def make_off() -> Path:
    v, f = _unit_cube_vf(origin=(-5.0, -5.0, 0.0), size=10.0)
    lines = ["OFF", f"{len(v)} {len(f)} 0"]
    for p in v:
        lines.append(f"{p[0]:.6g} {p[1]:.6g} {p[2]:.6g}")
    for tri in f:
        lines.append(f"3 {int(tri[0])} {int(tri[1])} {int(tri[2])}")
    lines.append("")
    return _write("box.off", "\n".join(lines).encode("ascii"))


def _zip_bytes(entries: list[tuple[str, bytes]]) -> bytes:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", compression=zipfile.ZIP_STORED) as zf:
        for name, data in entries:
            info = ZipInfo(filename=name, date_time=_ZIP_DATE)
            info.compress_type = zipfile.ZIP_STORED
            info.create_system = 0
            zf.writestr(info, data)
    return buf.getvalue()


_CONTENT_TYPES = """<?xml version='1.0' encoding='utf-8'?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>
"""

_RELS = """<?xml version='1.0' encoding='utf-8'?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/3dmodel.model" Id="rel0"/></Relationships>
"""


def _cube_3mf_mesh(origin, size) -> tuple[str, str]:
    v, f = _unit_cube_vf(origin, size)
    verts = "".join(f'<vertex x="{p[0]:.6g}" y="{p[1]:.6g}" z="{p[2]:.6g}" />' for p in v)
    tris = "".join(f'<triangle v1="{int(a)}" v2="{int(b)}" v3="{int(c)}" />' for a, b, c in f)
    return verts, tris


def make_two_objects_3mf() -> Path:
    # Two objects; build items apply translations.
    v1, t1 = _cube_3mf_mesh((0.0, 0.0, 0.0), 10.0)
    v2, t2 = _cube_3mf_mesh((0.0, 0.0, 0.0), 6.0)
    model = f"""<?xml version='1.0' encoding='utf-8'?>
<model unit="millimeter" xml:lang="en-US" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
 <resources>
  <object id="1" name="BoxA" type="model"><mesh><vertices>{v1}</vertices><triangles>{t1}</triangles></mesh></object>
  <object id="2" name="BoxB" type="model"><mesh><vertices>{v2}</vertices><triangles>{t2}</triangles></mesh></object>
 </resources>
 <build>
  <item objectid="1" transform="1 0 0 0 1 0 0 0 1 2 3 4"/>
  <item objectid="2" transform="1 0 0 0 1 0 0 0 1 20 0 0"/>
 </build>
</model>
"""
    data = _zip_bytes(
        [
            ("[Content_Types].xml", _CONTENT_TYPES.encode("utf-8")),
            ("_rels/.rels", _RELS.encode("utf-8")),
            ("3D/3dmodel.model", model.encode("utf-8")),
        ]
    )
    return _write("two_objects.3mf", data)


def make_threemf_empty_object() -> Path:
    v1, t1 = _cube_3mf_mesh((0.0, 0.0, 0.0), 8.0)
    model = f"""<?xml version='1.0' encoding='utf-8'?>
<model unit="millimeter" xml:lang="en-US" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
 <resources>
  <object id="1" name="Empty" type="model"><mesh><vertices></vertices><triangles></triangles></mesh></object>
  <object id="2" name="Solid" type="model"><mesh><vertices>{v1}</vertices><triangles>{t1}</triangles></mesh></object>
  <object id="3" name="AlsoEmpty" type="model"><mesh><vertices></vertices><triangles></triangles></mesh></object>
 </resources>
 <build>
  <item objectid="1"/>
  <item objectid="2" transform="1 0 0 0 1 0 0 0 1 0 0 0"/>
  <item objectid="3"/>
 </build>
</model>
"""
    data = _zip_bytes(
        [
            ("[Content_Types].xml", _CONTENT_TYPES.encode("utf-8")),
            ("_rels/.rels", _RELS.encode("utf-8")),
            ("3D/3dmodel.model", model.encode("utf-8")),
        ]
    )
    return _write("threemf_empty_object.3mf", data)


def _pad4(n: int) -> int:
    return (4 - (n % 4)) % 4


def _glb_bytes(gltf: dict, bin_blob: bytes) -> bytes:
    json_bytes = json.dumps(gltf, separators=(",", ":"), sort_keys=True).encode("utf-8")
    json_pad = b" " * _pad4(len(json_bytes))
    bin_pad = b"\x00" * _pad4(len(bin_blob))
    json_chunk = json_bytes + json_pad
    bin_chunk = bin_blob + bin_pad
    total = 12 + 8 + len(json_chunk) + 8 + len(bin_chunk)
    out = bytearray()
    out += struct.pack("<4sII", b"glTF", 2, total)
    out += struct.pack("<I4s", len(json_chunk), b"JSON")
    out += json_chunk
    out += struct.pack("<I4s", len(bin_chunk), b"BIN\x00")
    out += bin_chunk
    return bytes(out)


def make_nested_glb() -> Path:
    # Two unit cubes in BIN; nested nodes: root -> parent (T+10x) -> child (T+15y) + sibling mesh.
    v0, f0 = _unit_cube_vf(origin=(-2.0, -2.0, 0.0), size=4.0)
    v1, f1 = _unit_cube_vf(origin=(-1.5, -1.5, 0.0), size=3.0)
    # float32 positions then uint16 indices for each mesh, tightly packed then padded in chunk.
    pos0 = v0.astype("<f4").tobytes()
    idx0 = f0.astype("<u2").reshape(-1).tobytes()
    pos1 = v1.astype("<f4").tobytes()
    idx1 = f1.astype("<u2").reshape(-1).tobytes()
    # Align each buffer view to 4 bytes.
    parts = []
    offset = 0
    views = []

    def add(blob: bytes, target: int) -> int:
        nonlocal offset
        pad = _pad4(offset)
        if pad:
            parts.append(b"\x00" * pad)
            offset += pad
        start = offset
        parts.append(blob)
        views.append({"buffer": 0, "byteOffset": start, "byteLength": len(blob), "target": target})
        offset += len(blob)
        return len(views) - 1

    bv_pos0 = add(pos0, 34962)
    bv_idx0 = add(idx0, 34963)
    bv_pos1 = add(pos1, 34962)
    bv_idx1 = add(idx1, 34963)
    bin_blob = b"".join(parts)

    def accessor_pos(bv, verts):
        arr = verts.astype(np.float64)
        return {
            "bufferView": bv,
            "componentType": 5126,
            "count": len(verts),
            "type": "VEC3",
            "max": [float(x) for x in arr.max(axis=0)],
            "min": [float(x) for x in arr.min(axis=0)],
        }

    def accessor_idx(bv, faces):
        return {
            "bufferView": bv,
            "componentType": 5123,
            "count": int(faces.size),
            "type": "SCALAR",
        }

    gltf = {
        "asset": {"version": "2.0", "generator": "STL Editor make_reader_models"},
        "buffers": [{"byteLength": len(bin_blob)}],
        "bufferViews": views,
        "accessors": [
            accessor_pos(bv_pos0, v0),
            accessor_idx(bv_idx0, f0),
            accessor_pos(bv_pos1, v1),
            accessor_idx(bv_idx1, f1),
        ],
        "meshes": [
            {"name": "MeshA", "primitives": [{"attributes": {"POSITION": 0}, "indices": 1}]},
            {"name": "MeshB", "primitives": [{"attributes": {"POSITION": 2}, "indices": 3}]},
        ],
        "nodes": [
            {
                "name": "parent",
                "children": [1, 2],
                "translation": [10.0, 0.0, 0.0],
            },
            {"name": "meshA_node", "mesh": 0},
            {
                "name": "child",
                "mesh": 1,
                "translation": [0.0, 15.0, 0.0],
            },
        ],
        "scenes": [{"nodes": [0]}],
        "scene": 0,
    }
    return _write("nested_nodes.glb", _glb_bytes(gltf, bin_blob))


def main() -> int:
    paths = [
        make_empty_obj(),
        make_multi_obj(),
        make_materials_obj(),
        make_ascii_stl(),
        make_binary_stl(),
        make_ply_ascii(),
        make_ply_binary_le(),
        make_off(),
        make_two_objects_3mf(),
        make_threemf_empty_object(),
        make_nested_glb(),
    ]
    for p in paths:
        print(f"wrote {p.relative_to(ROOT)} ({p.stat().st_size} bytes)")
    # Determinism check within this run.
    again = [
        make_empty_obj(),
        make_multi_obj(),
        make_materials_obj(),
        make_ascii_stl(),
        make_binary_stl(),
        make_ply_ascii(),
        make_ply_binary_le(),
        make_off(),
        make_two_objects_3mf(),
        make_threemf_empty_object(),
        make_nested_glb(),
    ]
    for a, b in zip(paths, again):
        if a.read_bytes() != b.read_bytes():
            print(f"make_reader_models: non-deterministic {a.name}", file=sys.stderr)
            return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
