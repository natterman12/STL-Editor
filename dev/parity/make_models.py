"""Write rust/tests/models/{holed_block,open_box,two_blobs}.stl for the kernel corpus.

Deterministic binary STLs built with manifold3d (fixed circular_segments).
"""
from __future__ import annotations

import struct
import sys
from pathlib import Path

import manifold3d as m3
import numpy as np

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "rust" / "tests" / "models"
SEG = 32


def _stl_bytes(verts: np.ndarray, faces: np.ndarray) -> bytes:
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


def _mesh_from_manifold(man: m3.Manifold) -> tuple[np.ndarray, np.ndarray]:
    mm = man.to_mesh()
    v = np.asarray(mm.vert_properties, dtype=np.float32)[:, :3].copy()
    f = np.asarray(mm.tri_verts, dtype=np.uint32).copy()
    return v, f


def _write(name: str, verts: np.ndarray, faces: np.ndarray) -> Path:
    OUT.mkdir(parents=True, exist_ok=True)
    path = OUT / name
    path.write_bytes(_stl_bytes(verts, faces))
    return path


def make_holed_block() -> Path:
    # 30x20x10 box minus two r=3 through-holes (along Z), SEG segments each.
    box = m3.Manifold.cube((30.0, 20.0, 10.0), False)
    h1 = m3.Manifold.cylinder(12.0, 3.0, 3.0, SEG, False).translate((10.0, 10.0, -1.0))
    h2 = m3.Manifold.cylinder(12.0, 3.0, 3.0, SEG, False).translate((20.0, 10.0, -1.0))
    solid = box - h1 - h2
    return _write("holed_block.stl", *_mesh_from_manifold(solid))


def make_open_box() -> Path:
    # 20 mm cube with the two top-face triangles removed (not a closed manifold).
    cube = m3.Manifold.cube((20.0, 20.0, 20.0), False)
    v, f = _mesh_from_manifold(cube)
    keep = []
    for tri in f:
        zs = v[tri][:, 2]
        if not bool(np.all(zs == np.float32(20.0))):
            keep.append(tri)
    faces = np.asarray(keep, dtype=np.uint32)
    return _write("open_box.stl", v, faces)


def make_two_blobs() -> Path:
    # Two disjoint spheres (r=5 and r=3), SEG segments.
    a = m3.Manifold.sphere(5.0, SEG).translate((-8.0, 0.0, 0.0))
    b = m3.Manifold.sphere(3.0, SEG).translate((8.0, 0.0, 0.0))
    return _write("two_blobs.stl", *_mesh_from_manifold(a + b))


def main() -> int:
    paths = [make_holed_block(), make_open_box(), make_two_blobs()]
    for p in paths:
        print(f"wrote {p.relative_to(ROOT)} ({p.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
