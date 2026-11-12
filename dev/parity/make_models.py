"""Write rust/tests/models/{holed_block,open_box,two_blobs,...}.stl for the kernel corpus.

Deterministic binary STLs built with manifold3d (fixed circular_segments), plus
broken meshes for Repair parity.
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


def make_doubled_faces() -> Path:
    # Closed cube with every triangle listed twice.
    v, f = _mesh_from_manifold(m3.Manifold.cube((10.0, 10.0, 10.0), False))
    return _write("doubled_faces.stl", v, np.vstack([f, f]))


def make_touching_cubes() -> Path:
    # Two closed cubes that share a wall (touch but do not overlap).
    a = m3.Manifold.cube((10.0, 10.0, 10.0), False)
    b = m3.Manifold.cube((10.0, 10.0, 10.0), False).translate((10.0, 0.0, 0.0))
    va, fa = _mesh_from_manifold(a)
    vb, fb = _mesh_from_manifold(b)
    return _write("touching_cubes.stl", np.vstack([va, vb]), np.vstack([fa, fb + len(va)]))


def make_inside_out() -> Path:
    # Closed cube with reversed winding (negative volume / inverted shell).
    v, f = _mesh_from_manifold(m3.Manifold.cube((10.0, 10.0, 10.0), False))
    return _write("inside_out.stl", v, f[:, [0, 2, 1]])


def make_nonmanifold_edge() -> Path:
    # Three triangles share one edge (non-manifold).
    v = np.array(
        [
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [5.0, 8.0, 0.0],
            [5.0, 4.0, 8.0],
            [5.0, 4.0, -8.0],
        ],
        dtype=np.float32,
    )
    f = np.array([[0, 1, 2], [0, 1, 3], [0, 1, 4]], dtype=np.uint32)
    return _write("nonmanifold_edge.stl", v, f)


def make_overlapping_shells() -> Path:
    # Two closed cubes pushed into each other; 1.x merge_overlapping_shells unions them.
    a = m3.Manifold.cube((10.0, 10.0, 10.0), False)
    b = m3.Manifold.cube((10.0, 10.0, 10.0), False).translate((5.0, 0.0, 0.0))
    va, fa = _mesh_from_manifold(a)
    vb, fb = _mesh_from_manifold(b)
    return _write("overlapping_shells.stl", np.vstack([va, vb]), np.vstack([fa, fb + len(va)]))


def make_stray_triangle() -> Path:
    # One triangle; every repair step fails until Rebuild.
    v = np.array([[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]], dtype=np.float32)
    f = np.array([[0, 1, 2]], dtype=np.uint32)
    return _write("stray_triangle.stl", v, f)


def make_t_bracket() -> Path:
    # Upright stem with a top flange; flange underside overhangs when Z is up.
    stem = m3.Manifold.cube((8.0, 40.0, 50.0), False).translate((21.0, 0.0, 0.0))
    flange = m3.Manifold.cube((50.0, 40.0, 8.0), False).translate((0.0, 0.0, 50.0))
    return _write("t_bracket.stl", *_mesh_from_manifold(stem + flange))


def make_lipped_shelf() -> Path:
    # Back wall, forward shelf, and a down-hanging front lip (overhangs when Z is up).
    back = m3.Manifold.cube((60.0, 6.0, 40.0), False)
    shelf = m3.Manifold.cube((60.0, 35.0, 5.0), False).translate((0.0, 6.0, 20.0))
    lip = m3.Manifold.cube((60.0, 3.0, 12.0), False).translate((0.0, 38.0, 8.0))
    return _write("lipped_shelf.stl", *_mesh_from_manifold(back + shelf + lip))


def make_bridge_ring() -> Path:
    # Two posts with a top bridge; bridge underside overhangs when Z is up.
    a = m3.Manifold.cube((10.0, 10.0, 30.0), False)
    b = m3.Manifold.cube((10.0, 10.0, 30.0), False).translate((40.0, 0.0, 0.0))
    bridge = m3.Manifold.cube((50.0, 10.0, 6.0), False).translate((0.0, 0.0, 30.0))
    return _write("bridge_ring.stl", *_mesh_from_manifold(a + b + bridge))


def make_hole_and_pin() -> Path:
    # 40x30x15 block with a through-hole and a standing pin (for Resize Hole).
    box = m3.Manifold.cube((40.0, 30.0, 15.0), False)
    hole = m3.Manifold.cylinder(17.0, 4.0, 4.0, SEG, False).translate((12.0, 15.0, -1.0))
    pin = m3.Manifold.cylinder(12.0, 3.0, 3.0, SEG, False).translate((28.0, 15.0, 15.0))
    return _write("hole_and_pin.stl", *_mesh_from_manifold(box - hole + pin))


def main() -> int:
    paths = [
        make_holed_block(),
        make_open_box(),
        make_two_blobs(),
        make_doubled_faces(),
        make_touching_cubes(),
        make_inside_out(),
        make_nonmanifold_edge(),
        make_overlapping_shells(),
        make_stray_triangle(),
        make_t_bracket(),
        make_lipped_shelf(),
        make_bridge_ring(),
        make_hole_and_pin(),
    ]
    for p in paths:
        print(f"wrote {p.relative_to(ROOT)} ({p.stat().st_size} bytes)")

    # overlapping must trigger merge_overlapping_shells in 1.x.
    sys.path.insert(0, str(ROOT))
    from stleditor import geometry as G  # noqa: E402

    data = (OUT / "overlapping_shells.stl").read_bytes()
    verts, faces = G.read_stl_bytes(data)
    man = G.manifold_from_arrays(*G.weld(verts, faces))
    if man is None:
        print("make_models: overlapping_shells is not a solid", file=sys.stderr)
        return 1
    _, merged = G.merge_overlapping_shells(man)
    if merged < 2:
        print(f"make_models: overlapping_shells did not merge (n={merged})", file=sys.stderr)
        return 1
    print(f"overlapping_shells: merge_overlapping_shells merged {merged} shells")
    return 0


if __name__ == "__main__":
    sys.exit(main())
