"""Mesh statistics for 1.x / 2.0 parity golden files.

Mirrors the Rust `stl_parity::mesh_stats` formulas: float32 positions,
float64 accumulation, signed tetrahedron volume, triangle-area sum, and
undirected-edge watertightness.
"""

from __future__ import annotations

import numpy as np


def mesh_stats(verts, faces) -> dict:
    """Return MeshStats-compatible dict for ``verts`` (N×3) and ``faces`` (M×3).

    Keys: triangles, vertices, volume, area, bbox_min, bbox_max, watertight.
    Values are plain Python types (int / float / bool / list) for JSON.
    """
    verts = np.asarray(verts, dtype=np.float32)
    faces = np.asarray(faces)

    if verts.size == 0:
        verts = verts.reshape(0, 3)
    else:
        verts = verts.reshape(-1, 3)

    if faces.size == 0:
        faces = faces.reshape(0, 3)
    else:
        faces = faces.reshape(-1, 3).astype(np.int64, copy=False)

    n_tri = int(faces.shape[0])
    n_vert = int(verts.shape[0])

    if n_tri == 0:
        return {
            "triangles": 0,
            "vertices": n_vert,
            "volume": 0.0,
            "area": 0.0,
            "bbox_min": [0.0, 0.0, 0.0],
            "bbox_max": [0.0, 0.0, 0.0],
            "watertight": False,
        }

    v0 = verts[faces[:, 0]].astype(np.float64)
    v1 = verts[faces[:, 1]].astype(np.float64)
    v2 = verts[faces[:, 2]].astype(np.float64)

    # Signed tetrahedron sum relative to the origin: sum(dot(v0, cross(v1, v2))) / 6.
    cross_v1_v2 = np.cross(v1, v2)
    volume = float(np.sum(np.einsum("ij,ij->i", v0, cross_v1_v2)) / 6.0)

    # Triangle areas from edge cross products.
    e1 = v1 - v0
    e2 = v2 - v0
    area = float(0.5 * np.sum(np.linalg.norm(np.cross(e1, e2), axis=1)))

    verts64 = verts.astype(np.float64)
    bbox_min = [float(x) for x in verts64.min(axis=0)]
    bbox_max = [float(x) for x in verts64.max(axis=0)]

    # Undirected edges: watertight iff every edge is shared by exactly two triangles.
    i0 = faces[:, 0]
    i1 = faces[:, 1]
    i2 = faces[:, 2]
    a = np.concatenate([i0, i1, i2])
    b = np.concatenate([i1, i2, i0])
    lo = np.minimum(a, b)
    hi = np.maximum(a, b)
    # Pack edge endpoints into one int64 key for counting.
    keys = lo.astype(np.int64) << 32 | hi.astype(np.int64)
    _, counts = np.unique(keys, return_counts=True)
    watertight = bool(counts.size > 0 and np.all(counts == 2))

    return {
        "triangles": n_tri,
        "vertices": n_vert,
        "volume": volume,
        "area": area,
        "bbox_min": bbox_min,
        "bbox_max": bbox_max,
        "watertight": watertight,
    }
