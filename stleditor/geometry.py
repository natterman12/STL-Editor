"""Mesh geometry helpers: conversion between numpy meshes and Manifold solids, import/export,
repair, voxel rebuild, hollowing, face-region detection and plane math."""
from __future__ import annotations

import io
import itertools
import math
import os
import struct

import numpy as np
import manifold3d as m3

EPS = 1e-9
_SERIAL = itertools.count(1)


class GeometryError(Exception):
    """An operation that cannot be done on the given geometry (shown to the user)."""


# --------------------------------------------------------------------------------------------
# Bodies
# --------------------------------------------------------------------------------------------
class Body:
    """A body is either a watertight solid (Manifold) or a raw, broken mesh that only displays
    and exports until it is repaired."""

    def __init__(self, name: str, solid: m3.Manifold | None = None,
                 verts: np.ndarray | None = None, faces: np.ndarray | None = None):
        self.name = name
        self.serial = next(_SERIAL)
        self.solid = solid
        self._verts = verts
        self._faces = faces
        self._cache = None

    @property
    def is_solid(self) -> bool:
        return self.solid is not None

    def mesh(self) -> tuple[np.ndarray, np.ndarray]:
        if self._cache is None:
            if self.solid is not None:
                mm = self.solid.to_mesh()
                v = np.asarray(mm.vert_properties, dtype=np.float32)[:, :3]
                f = np.asarray(mm.tri_verts, dtype=np.uint32)
                self._cache = (np.ascontiguousarray(v), np.ascontiguousarray(f))
            else:
                self._cache = (np.asarray(self._verts, np.float32), np.asarray(self._faces, np.uint32))
        return self._cache

    def require_solid(self) -> m3.Manifold:
        if self.solid is None:
            raise GeometryError(f'"{self.name}" is not a watertight solid. Run Repair on it first.')
        return self.solid

    def copy(self, name=None, solid=None):
        if solid is not None:
            return Body(name or self.name, solid=solid)
        b = Body(name or self.name, self.solid, self._verts, self._faces)
        b._cache = self._cache
        return b

    def info(self) -> dict:
        v, f = self.mesh()
        lo, hi = (v.min(0), v.max(0)) if len(v) else (np.zeros(3), np.zeros(3))
        d = {"name": self.name, "solid": self.is_solid, "triangles": int(len(f)),
             "vertices": int(len(v)), "bbox": [lo.tolist(), hi.tolist()],
             "size": (hi - lo).tolist()}
        if self.solid is not None:
            d["volume"] = float(self.solid.volume())
            d["area"] = float(self.solid.surface_area())
            d["genus"] = int(self.solid.genus())
        else:
            d["area"] = float(_area(v, f))
        return d


def _area(v, f):
    if len(f) == 0:
        return 0.0
    t = v[f].astype(np.float64)
    return float(np.linalg.norm(np.cross(t[:, 1] - t[:, 0], t[:, 2] - t[:, 0]), axis=1).sum() / 2)


def manifold_from_arrays(verts, faces) -> m3.Manifold | None:
    """Return a Manifold, or None if the mesh is not a closed 2-manifold."""
    v = np.ascontiguousarray(np.asarray(verts, dtype=np.float32)[:, :3])
    f = np.ascontiguousarray(np.asarray(faces, dtype=np.uint32))
    if len(f) == 0:
        return None
    try:
        man = m3.Manifold(m3.Mesh(vert_properties=v, tri_verts=f))
    except Exception:
        return None
    if man.status() != m3.Error.NoError or man.is_empty():
        return None
    return man


def weld(verts, faces, tol=1e-5):
    """Merge coincident vertices and drop degenerate triangles."""
    v = np.asarray(verts, dtype=np.float64)
    f = np.asarray(faces, dtype=np.int64)
    q = np.round(v / tol).astype(np.int64)
    _, idx, inv = np.unique(q, axis=0, return_index=True, return_inverse=True)
    inv = inv.reshape(-1)
    nv = v[idx]
    nf = inv[f]
    ok = (nf[:, 0] != nf[:, 1]) & (nf[:, 1] != nf[:, 2]) & (nf[:, 0] != nf[:, 2])
    nf = nf[ok]
    # drop exact duplicate triangles (same vertex set)
    if len(nf):
        key = np.sort(nf, axis=1)
        _, first = np.unique(key, axis=0, return_index=True)
        nf = nf[np.sort(first)]
    return nv, nf


# --------------------------------------------------------------------------------------------
# File IO
# --------------------------------------------------------------------------------------------
MESH_EXTS = (".stl", ".obj", ".3mf", ".ply", ".off", ".glb", ".gltf")


def read_stl_bytes(b: bytes):
    if len(b) >= 84:
        n = struct.unpack_from("<I", b, 80)[0]
        if 84 + 50 * n == len(b):
            rec = np.frombuffer(b, dtype=np.dtype([("n", "<3f4"), ("v", "<9f4"), ("a", "<u2")]),
                                count=n, offset=84)
            tri = rec["v"].reshape(-1, 3).astype(np.float64)
            return tri, np.arange(len(tri), dtype=np.int64).reshape(-1, 3)
    # ASCII
    txt = b.decode("ascii", "ignore")
    pts = []
    for line in txt.splitlines():
        s = line.strip()
        if s.startswith("vertex"):
            p = s.split()
            pts.append((float(p[1]), float(p[2]), float(p[3])))
    if not pts or len(pts) % 3:
        raise GeometryError("Could not read this STL file.")
    tri = np.asarray(pts, dtype=np.float64)
    return tri, np.arange(len(tri), dtype=np.int64).reshape(-1, 3)


def load_mesh_file(data: bytes, filename: str) -> list[tuple[str, np.ndarray, np.ndarray]]:
    """Read a mesh file into one or more (name, verts, faces) parts, vertices welded."""
    ext = os.path.splitext(filename)[1].lower()
    base = os.path.splitext(os.path.basename(filename))[0]
    parts = []
    if ext == ".stl":
        v, f = read_stl_bytes(data)
        parts.append((base, *weld(v, f)))
    else:
        import trimesh
        loaded = trimesh.load(io.BytesIO(data), file_type=ext[1:], force=None, process=False)
        if isinstance(loaded, trimesh.Scene):
            geoms = list(loaded.dump()) if hasattr(loaded, "dump") else list(loaded.geometry.values())
            for i, g in enumerate(geoms):
                if hasattr(g, "faces") and len(g.faces):
                    nm = base if len(geoms) == 1 else f"{base} {i + 1}"
                    parts.append((nm, *weld(g.vertices, g.faces)))
        else:
            parts.append((base, *weld(loaded.vertices, loaded.faces)))
    if not parts:
        raise GeometryError("No triangles found in this file.")
    return parts


def stl_bytes(verts, faces) -> bytes:
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


def export_bodies(bodies: list[Body], path: str):
    ext = os.path.splitext(path)[1].lower()
    if ext == ".stl":
        vs, fs, off = [], [], 0
        for b in bodies:
            v, f = b.mesh()
            vs.append(v)
            fs.append(f.astype(np.int64) + off)
            off += len(v)
        v = np.concatenate(vs) if vs else np.zeros((0, 3), np.float32)
        f = np.concatenate(fs) if fs else np.zeros((0, 3), np.int64)
        with open(path, "wb") as fh:
            fh.write(stl_bytes(v, f))
        return
    import trimesh
    meshes = []
    for b in bodies:
        v, f = b.mesh()
        meshes.append(trimesh.Trimesh(vertices=v, faces=f, process=False))
    if ext == ".3mf":
        scene = trimesh.Scene()
        for b, mm in zip(bodies, meshes):
            scene.add_geometry(mm, node_name=b.name, geom_name=b.name)
        data = scene.export(file_type="3mf")
    else:
        data = trimesh.util.concatenate(meshes).export(file_type=ext[1:])
    with open(path, "wb") as fh:
        fh.write(data if isinstance(data, bytes) else data.encode())


# --------------------------------------------------------------------------------------------
# Repair, rebuild, simplify, hollow
# --------------------------------------------------------------------------------------------
def quick_repair(verts, faces):
    """Weld, remove degenerate/duplicate faces, unify winding, fill small holes."""
    import trimesh
    v, f = weld(verts, faces)
    tm = trimesh.Trimesh(vertices=v, faces=f, process=True, validate=True)
    tm.update_faces(tm.nondegenerate_faces())
    tm.update_faces(tm.unique_faces())
    tm.remove_unreferenced_vertices()
    trimesh.repair.fix_winding(tm)
    trimesh.repair.fix_inversion(tm, multibody=True)
    trimesh.repair.fill_holes(tm)
    trimesh.repair.fix_normals(tm, multibody=True)
    return np.asarray(tm.vertices), np.asarray(tm.faces)


def _cap_holes(v, f):
    """Close every boundary loop with a fan around its centroid."""
    e = np.concatenate([f[:, [0, 1]], f[:, [1, 2]], f[:, [2, 0]]])
    key = e[:, 0] * (len(v) + 1) + e[:, 1]
    rkey = e[:, 1] * (len(v) + 1) + e[:, 0]
    boundary = e[~np.isin(key, rkey)]
    if not len(boundary):
        return v, f, 0
    nxt = {}
    for a, b in boundary:
        nxt.setdefault(int(a), []).append(int(b))
    new_v, new_f = [v], [f]
    nv = len(v)
    loops = 0
    used = set()
    for a0, b0 in boundary:
        a0, b0 = int(a0), int(b0)
        if (a0, b0) in used:
            continue
        loop = [a0]
        cur, prev = b0, a0
        used.add((a0, b0))
        ok = True
        while cur != a0:
            loop.append(cur)
            outs = [x for x in nxt.get(cur, []) if (cur, x) not in used]
            if not outs or len(loop) > 200000:
                ok = False
                break
            nx = outs[0]
            used.add((cur, nx))
            prev, cur = cur, nx
        if not ok or len(loop) < 3:
            continue
        c = v[loop].mean(axis=0)
        new_v.append(c[None, :])
        ci = nv
        nv += 1
        L = np.array(loop)
        # boundary edge (u -> w) belongs to an existing face, so the cap uses (w -> u)
        new_f.append(np.column_stack([np.roll(L, -1), L, np.full(len(L), ci)]))
        loops += 1
    return np.concatenate(new_v), np.concatenate(new_f), loops


def merge_overlapping_shells(man: m3.Manifold) -> tuple[m3.Manifold, int]:
    """Many STLs are several closed shells pushed into each other. Union them into one clean
    solid (shells that are cavities, i.e. inside-out, are left alone)."""
    parts = man.decompose()
    if len(parts) < 2 or any(p.volume() <= 0 for p in parts):
        return man, 0
    total = sum(p.volume() for p in parts)
    merged = m3.Manifold.batch_boolean(parts, m3.OpType.Add)
    if merged.volume() < total * (1 - 1e-6):
        return merged, len(parts)
    return man, 0


def local_repair(verts, faces):
    """Fix small defects without remeshing: drop faces on non-manifold edges, unify winding,
    cap the holes, and keep each shell's original triangles. Returns a Manifold or None."""
    import trimesh
    v, f = weld(verts, faces)
    for _ in range(3):
        e = np.sort(np.concatenate([f[:, [0, 1]], f[:, [1, 2]], f[:, [2, 0]]]), axis=1)
        _, inv, cnt = np.unique(e, axis=0, return_inverse=True, return_counts=True)
        bad_edge = cnt[inv.reshape(-1)] > 2
        bad_face = bad_edge.reshape(3, -1).any(axis=0)
        if bad_face.any():
            f = f[~bad_face]
        tm = trimesh.Trimesh(vertices=v, faces=f, process=False)
        trimesh.repair.fix_winding(tm)
        v, f = np.asarray(tm.vertices), np.asarray(tm.faces)
        v, f, _ = _cap_holes(v, f)
        man = manifold_from_arrays(v, f)
        if man is not None:
            if man.volume() < 0:
                man = manifold_from_arrays(v, f[:, ::-1])
            return merge_overlapping_shells(man)[0] if man is not None else None
    # last try: shell by shell, union of the shells that are (or became) closed
    tm = trimesh.Trimesh(vertices=v, faces=f, process=False)
    solids = []
    for sh in tm.split(only_watertight=False):
        m = manifold_from_arrays(sh.vertices, sh.faces)
        if m is None:
            sv, sf, _ = _cap_holes(np.asarray(sh.vertices), np.asarray(sh.faces))
            m = manifold_from_arrays(sv, sf)
        if m is None:
            return None
        if m.volume() < 0:
            m = manifold_from_arrays(sh.vertices, np.asarray(sh.faces)[:, ::-1])
        solids.append(m)
    if not solids:
        return None
    return m3.Manifold.batch_boolean(solids, m3.OpType.Add) if len(solids) > 1 else solids[0]


def _grid_for(lo, hi, pitch, pad=2):
    n = np.ceil((hi - lo) / pitch).astype(int) + 1 + 2 * pad
    origin = lo - pad * pitch
    return origin, n


def _check_grid(n):
    if np.prod(n.astype(np.float64)) > 300e6:
        raise GeometryError("Resolution too fine for this model size. Use a coarser setting.")


def _raster_evenodd(polys, ox, oy, pitch, nx, ny):
    """Even-odd scanline fill of 2D polygons onto an (nx, ny) grid of sample points
    at (ox + i*pitch, oy + j*pitch)."""
    out = np.zeros((nx, ny), dtype=bool)
    if not polys:
        return out
    P0 = np.concatenate([np.asarray(p, np.float64) for p in polys])
    P1 = np.concatenate([np.roll(np.asarray(p, np.float64), -1, axis=0) for p in polys])
    gx0, gy0 = (P0[:, 0] - ox) / pitch, (P0[:, 1] - oy) / pitch
    gx1, gy1 = (P1[:, 0] - ox) / pitch, (P1[:, 1] - oy) / pitch
    lo = np.ceil(np.minimum(gx0, gx1)).astype(np.int64)
    hi = np.ceil(np.maximum(gx0, gx1)).astype(np.int64) - 1
    lo = np.maximum(lo, 0)
    hi = np.minimum(hi, nx - 1)
    cnt = np.clip(hi - lo + 1, 0, None)
    tot = int(cnt.sum())
    if tot == 0:
        return out
    e = np.repeat(np.arange(len(cnt)), cnt)
    start = np.repeat(np.cumsum(cnt) - cnt, cnt)
    rows = lo[e] + (np.arange(tot) - start)
    dx = gx1[e] - gx0[e]
    t = (rows - gx0[e]) / np.where(dx == 0, 1, dx)
    yc = gy0[e] + t * (gy1[e] - gy0[e])
    order = np.lexsort((yc, rows))
    rows, yc = rows[order], yc[order]
    # each row has an even number of crossings for closed polygons; pair them up
    _, first, counts = np.unique(rows, return_index=True, return_counts=True)
    ok = (counts % 2) == 0
    keep = np.repeat(ok, counts)
    rows, yc = rows[keep], yc[keep]
    ra, ya, yb = rows[0::2], yc[0::2], yc[1::2]
    ja = np.clip(np.ceil(ya).astype(np.int64), 0, ny)
    jb = np.clip(np.ceil(yb).astype(np.int64), 0, ny)
    diff = np.zeros((nx, ny + 1), dtype=np.int32)
    np.add.at(diff, (ra, ja), 1)
    np.add.at(diff, (ra, jb), -1)
    out = np.cumsum(diff[:, :ny], axis=1) > 0
    return out


def solid_occupancy(solid: m3.Manifold, pitch):
    """Exact inside/outside grid for a watertight solid, from Manifold slices.
    Voxel (i,j,k) has its centre at origin + (i,j,k) * pitch."""
    b = solid.bounding_box()
    lo, hi = np.array(b[:3]), np.array(b[3:])
    origin, n = _grid_for(lo, hi, pitch)
    _check_grid(n)
    occ = np.zeros(n, dtype=bool)
    for k in range(n[2]):
        z = origin[2] + k * pitch
        if z <= lo[2] or z >= hi[2]:
            continue
        polys = solid.slice(float(z)).to_polygons()
        occ[:, :, k] = _raster_evenodd(polys, origin[0], origin[1], pitch, n[0], n[1])
    return occ, origin


def _surface_points(verts, faces, spacing):
    """Points covering every triangle no further than `spacing` apart (barycentric grid)."""
    v = np.asarray(verts, np.float64)
    t = v[np.asarray(faces, np.int64)]
    edge = np.max(np.stack([np.linalg.norm(t[:, 1] - t[:, 0], axis=1),
                            np.linalg.norm(t[:, 2] - t[:, 1], axis=1),
                            np.linalg.norm(t[:, 0] - t[:, 2], axis=1)]), axis=0)
    k = np.clip(np.ceil(edge / spacing).astype(int), 1, 400)
    out = [v]
    for kk in np.unique(k):
        sel = t[k == kk]
        if kk == 1:
            out.append(sel.mean(axis=1))
            continue
        i, j = np.meshgrid(np.arange(kk + 1), np.arange(kk + 1), indexing="ij")
        m = (i + j) <= kk
        a = (i[m] / kk)[:, None]
        bb = (j[m] / kk)[:, None]
        # (tris, samples, 3)
        pts = sel[:, None, 0] * (1 - a - bb) + sel[:, None, 1] * a + sel[:, None, 2] * bb
        out.append(pts.reshape(-1, 3))
    return np.concatenate(out)


def mesh_occupancy(verts, faces, pitch):
    """Occupancy for any (possibly broken) mesh: rasterise the surface, then fill the inside.
    Holes larger than a voxel are closed by growing the surface before filling."""
    from scipy import ndimage
    v = np.asarray(verts, np.float64)
    lo, hi = v.min(0), v.max(0)
    pad = 12
    origin, n = _grid_for(lo, hi, pitch, pad=pad)
    _check_grid(n)
    pts = _surface_points(v, faces, pitch * 0.5)
    idx = np.clip(np.rint((pts - origin) / pitch).astype(int), 0, n - 1)
    shell = np.zeros(n, dtype=bool)
    shell[idx[:, 0], idx[:, 1], idx[:, 2]] = True
    shell_count = shell.sum()
    ball = ndimage.generate_binary_structure(3, 1)
    for it in (0, 1, 2, 3, 5, 8, 11):
        grown = ndimage.binary_dilation(shell, ball, iterations=it) if it else shell
        filled = ndimage.binary_fill_holes(grown)
        inside = filled & ~grown
        if inside.sum() > 0.2 * shell_count:
            if it:
                filled = ndimage.binary_erosion(filled, ball, iterations=it, border_value=0)
                filled |= shell  # keep the original surface voxels
            return filled, origin
    return ndimage.binary_fill_holes(shell), origin


def _field_to_manifold(field, origin, pitch, level=0.0):
    """Marching cubes on a scalar field (positive = inside) → Manifold."""
    from skimage import measure
    pad = np.pad(field, 1, mode="constant", constant_values=min(field.min(), level) - 1.0)
    v, f, _, _ = measure.marching_cubes(pad, level=level, spacing=(pitch, pitch, pitch))
    v = v + origin - pitch
    f = f[:, ::-1]  # skimage winds faces inward for "positive inside" fields
    v, f = weld(v, f, tol=pitch * 1e-4)
    man = manifold_from_arrays(v, f)
    if man is None:
        # flip winding if needed
        man = manifold_from_arrays(v, f[:, ::-1])
    if man is None:
        raise GeometryError("Voxel rebuild produced an invalid mesh.")
    if man.volume() < 0:
        man = manifold_from_arrays(v, f[:, ::-1])
    return man


def voxel_rebuild(verts, faces, pitch, smooth=0.6):
    """Rebuild any mesh (open, self-intersecting, multi-shell) as one watertight solid."""
    from scipy import ndimage
    occ, origin = mesh_occupancy(verts, faces, pitch)
    field = ndimage.gaussian_filter(occ.astype(np.float32), sigma=smooth) if smooth > 0 \
        else occ.astype(np.float32)
    return _field_to_manifold(field, origin, pitch, level=0.5)


def auto_pitch(verts, target=180):
    v = np.asarray(verts)
    diag = float(np.linalg.norm(v.max(0) - v.min(0)))
    return max(diag / target, 0.05)


def hollow(solid: m3.Manifold, thickness: float, pitch: float | None = None) -> m3.Manifold:
    """Shell a solid inward with the given wall thickness. The outer surface is kept exactly;
    the inner cavity is built on a voxel grid from an exact-ish distance field."""
    from scipy import ndimage
    if thickness <= 0:
        raise GeometryError("Wall thickness must be positive.")
    mm = solid.to_mesh()
    v = np.asarray(mm.vert_properties)[:, :3]
    f = np.asarray(mm.tri_verts)
    size = v.max(0) - v.min(0)
    if thickness * 2 >= size.min():
        raise GeometryError("Wall thickness is too large for this body; nothing would be left hollow.")
    if pitch is None:
        pitch = min(thickness / 3.0, float(np.linalg.norm(size)) / 120.0)
        pitch = max(pitch, float(np.linalg.norm(size)) / 400.0)
    occ, origin = solid_occupancy(solid, pitch)
    # distance (mm) from each inside voxel centre to the nearest outside voxel centre
    dist = ndimage.distance_transform_edt(occ) * pitch - pitch * 0.5
    field = dist - thickness
    # refine near the cavity wall with exact distances to the surface
    from scipy.spatial import cKDTree
    band = occ & (np.abs(field) < 2.5 * pitch)
    if band.any():
        tree = cKDTree(_surface_points(v, f, pitch / 3.0))
        ii = np.nonzero(band)
        centers = origin + np.stack(ii, axis=1) * pitch
        dd, _ = tree.query(centers, workers=-1)
        field[ii] = dd - thickness
    field[~occ] = -pitch
    if field.max() <= 0:
        raise GeometryError("Wall thickness is too large for this body; nothing would be left hollow.")
    cavity = _field_to_manifold(field.astype(np.float32), origin, pitch, level=0.0)
    cavity = cavity.simplify(pitch * 0.1)   # more and the cavity drifts by up to 0.1 mm
    return solid - cavity


def simplify(body: Body, target_ratio: float | None = None, tolerance: float | None = None):
    """Reduce triangle count. target_ratio: keep this fraction of triangles (0-1)."""
    v, f = body.mesh()
    if tolerance is not None and body.is_solid and target_ratio is None:
        return body.solid.simplify(float(tolerance))
    ratio = float(target_ratio if target_ratio is not None else 0.5)
    ratio = min(max(ratio, 0.001), 1.0)
    import fast_simplification
    nv, nf = fast_simplification.simplify(np.asarray(v, np.float32), np.asarray(f, np.int64),
                                          target_reduction=1.0 - ratio)
    nv, nf = weld(nv, nf)
    man = manifold_from_arrays(nv, nf)
    if man is None:
        rv, rf = quick_repair(nv, nf)
        man = manifold_from_arrays(rv, rf)
    if man is None and body.is_solid:
        # fall back to Manifold's own (topology-safe) simplification
        diag = float(np.linalg.norm(v.max(0) - v.min(0)))
        tol = diag * 1e-3
        man = body.solid
        for _ in range(12):
            man = body.solid.simplify(tol)
            if man.num_tri() <= len(f) * ratio * 1.15:
                break
            tol *= 1.8
    return man if man is not None else (nv, nf)


# --------------------------------------------------------------------------------------------
# Planes and faces
# --------------------------------------------------------------------------------------------
def unit(v):
    v = np.asarray(v, dtype=np.float64)
    n = np.linalg.norm(v)
    if n < EPS:
        raise GeometryError("Zero-length direction.")
    return v / n


def plane_frame(origin, normal, xdir=None):
    """Return (origin, x, y, n) orthonormal right-handed frame for a plane."""
    n = unit(normal)
    if xdir is None or np.linalg.norm(xdir) < EPS or abs(np.dot(unit(xdir), n)) > 0.999:
        if abs(n[2]) < 0.9:
            # wall-like plane: keep world Z as the sketch's "up" (y) direction
            y = unit(np.array([0, 0, 1.0]) - n * n[2])
            x = np.cross(y, n)
            return np.asarray(origin, float), unit(x), y, n
        xdir = np.array([1.0, 0, 0])
    x = unit(np.asarray(xdir, float) - n * np.dot(xdir, n))
    y = np.cross(n, x)
    return np.asarray(origin, float), x, y, n


BASE_PLANES = {
    "XY": (np.zeros(3), np.array([0, 0, 1.0]), np.array([1.0, 0, 0])),
    "XZ": (np.zeros(3), np.array([0, -1.0, 0]), np.array([1.0, 0, 0])),
    "YZ": (np.zeros(3), np.array([1.0, 0, 0]), np.array([0, 1.0, 0])),
}


def frame_matrix(o, x, y, n):
    """3x4 matrix mapping local (u, v, w) to world o + u x + v y + w n."""
    return np.column_stack([x, y, n, o]).astype(np.float64)


def face_region(verts, faces, tri, angle_tol_deg=1.0, dist_tol=None):
    """Grow the coplanar region around triangle `tri`. Returns (tri_ids, normal, centroid)."""
    v = np.asarray(verts, np.float64)
    f = np.asarray(faces, np.int64)
    if tri < 0 or tri >= len(f):
        raise GeometryError("No face there.")
    t = v[f]
    nrm = np.cross(t[:, 1] - t[:, 0], t[:, 2] - t[:, 0])
    area2 = np.linalg.norm(nrm, axis=1)
    nrm = np.divide(nrm, area2[:, None], out=np.zeros_like(nrm), where=area2[:, None] > 0)
    n0 = nrm[tri]
    d0 = np.dot(n0, t[tri, 0])
    if dist_tol is None:
        dist_tol = max(float(np.linalg.norm(v.max(0) - v.min(0))) * 1e-5, 1e-5)
    cos_tol = math.cos(math.radians(angle_tol_deg))
    cand = (nrm @ n0 >= cos_tol) & (np.abs(t @ n0 - d0).max(axis=1) < dist_tol)
    cand_ids = np.nonzero(cand)[0]
    # connected component of candidates containing tri (edge adjacency)
    sub = f[cand_ids]
    e = np.concatenate([sub[:, [0, 1]], sub[:, [1, 2]], sub[:, [2, 0]]])
    e.sort(axis=1)
    owner = np.tile(np.arange(len(sub)), 3)
    order = np.lexsort((e[:, 1], e[:, 0]))
    es, os_ = e[order], owner[order]
    same = np.all(es[1:] == es[:-1], axis=1)
    a, b = os_[:-1][same], os_[1:][same]
    from scipy.sparse import coo_matrix
    from scipy.sparse.csgraph import connected_components
    nsub = len(sub)
    g = coo_matrix((np.ones(len(a)), (a, b)), shape=(nsub, nsub))
    _, lab = connected_components(g, directed=False)
    start = int(np.nonzero(cand_ids == tri)[0][0])
    region = cand_ids[lab == lab[start]]
    w = area2[region]
    normal = unit((nrm[region] * w[:, None]).sum(0))
    centroid = (t[region].mean(axis=1) * w[:, None]).sum(0) / max(w.sum(), EPS)
    return region, normal, centroid


def region_boundary(faces, region):
    """Directed boundary edges of a triangle region, as vertex-index pairs."""
    f = np.asarray(faces, np.int64)[region]
    e = np.concatenate([f[:, [0, 1]], f[:, [1, 2]], f[:, [2, 0]]])
    key = np.sort(e, axis=1)
    _, inv, cnt = np.unique(key, axis=0, return_inverse=True, return_counts=True)
    return e[cnt[inv.reshape(-1)] == 1]


def make_cylinder(p0, direction, length, r0, r1=None, segments=96):
    """Cylinder/cone from point p0 along direction for `length`."""
    r1 = r0 if r1 is None else r1
    c = m3.Manifold.cylinder(float(length), float(r0), float(r1), segments)
    o, x, y, n = plane_frame(p0, direction)
    return c.transform(frame_matrix(o, x, y, n))


def big_extent(bodies) -> float:
    lo = np.full(3, np.inf)
    hi = np.full(3, -np.inf)
    for b in bodies:
        v, _ = b.mesh()
        if len(v):
            lo = np.minimum(lo, v.min(0))
            hi = np.maximum(hi, v.max(0))
    if not np.all(np.isfinite(lo)):
        return 1000.0
    return float(np.linalg.norm(hi - lo)) * 2 + 10.0 + float(np.abs(np.concatenate([lo, hi])).max())
