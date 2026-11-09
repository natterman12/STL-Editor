"""Print-oriented geometry: split with alignment connectors, split-to-bed, sculpt brushes,
remeshing and push/pull of flat faces."""
from __future__ import annotations

import math

import numpy as np
import manifold3d as m3

from . import geometry as G
from .geometry import GeometryError

PRINTERS = {
    "bambu_x1": ("Bambu X1C / X1E / P1S / P1P / A1", (256, 256, 256)),
    "bambu_a1mini": ("Bambu A1 mini", (180, 180, 180)),
    "bambu_h2d": ("Bambu H2D", (325, 320, 325)),
    "prusa_mk4": ("Prusa MK4 / MK3S", (250, 210, 220)),
    "prusa_xl": ("Prusa XL", (360, 360, 360)),
    "voron_350": ("Voron 2.4 (350)", (350, 350, 340)),
    "voron_300": ("Voron 2.4 (300)", (300, 300, 280)),
    "ender3": ("Ender 3 / V2", (220, 220, 250)),
}


# ------------------------------------------------------------------------------------------
# Connectors
# ------------------------------------------------------------------------------------------
CONNECTORS = {
    # mode: (pin diameter, total pin length, hole clearance per side)
    "pins": (5.0, 10.0, 0.15),          # printed pin on one piece, matching hole in the other
    "dowels": (5.0, 12.0, 0.15),        # holes in both pieces + separate printable pins
    "filament": (1.75, 10.0, 0.15),     # holes in both pieces for pieces of 1.75 mm filament
    "magnets": (6.0, 3.0, 0.15),        # 6x3 magnet pockets on both faces (glue magnets in)
}


def _plane_basis(o, n):
    o, x, y, n = G.plane_frame(o, n)
    return o, x, y, n


def section_shape(solid: m3.Manifold, o, n):
    """Cross-section of a solid with the plane (o, n), in plane (u, v) coordinates."""
    o, x, y, n = _plane_basis(o, n)
    # world -> plane frame: rows x, y, n; translation -R o
    R = np.vstack([x, y, n])
    M = np.column_stack([R, -R @ o])
    return solid.transform(M).slice(0.0), (o, x, y, n)


def connector_points(cs: m3.CrossSection, radius: float, wall: float, count: int | None = None,
                     max_per_part: int = 3):
    """Spread-out pin centres inside a cross-section, keeping `wall` of material round each."""
    from shapely.geometry import Polygon
    from shapely import contains_xy
    inner = cs.offset(-(radius + wall), m3.JoinType.Round)
    if inner.is_empty() or inner.area() <= 1e-9:
        return []
    pts = []
    for comp in sorted(inner.decompose(), key=lambda c: -c.area()):
        polys = comp.to_polygons()
        geom = None
        for pl in polys:
            g = Polygon(pl)
            if not g.is_valid:
                g = g.buffer(0)
            geom = g if geom is None else geom.symmetric_difference(g)
        if geom is None or geom.is_empty:
            continue
        lo = np.array(comp.bounds()[:2])
        hi = np.array(comp.bounds()[2:])
        ext = float(max(hi - lo))
        step = max(ext / 60.0, radius * 0.25)
        gx, gy = np.meshgrid(np.arange(lo[0], hi[0] + step, step), np.arange(lo[1], hi[1] + step, step))
        cand = np.column_stack([gx.ravel(), gy.ravel()])
        cand = cand[contains_xy(geom, cand[:, 0], cand[:, 1])]
        if not len(cand):
            c = geom.representative_point()
            cand = np.array([[c.x, c.y]])
        # how many pins for this piece of the cut face
        n_here = count or int(np.clip(round(ext / (radius * 10)), 1, max_per_part))
        boundary = geom.boundary
        from shapely import distance as sdist, points as spoints
        dist_edge = sdist(spoints(cand), boundary)
        chosen = [cand[int(np.argmax(dist_edge))]]
        for _ in range(n_here - 1):
            dmin = np.min(np.linalg.norm(cand[:, None, :] - np.array(chosen)[None], axis=2), axis=1)
            if dmin.max() < radius * 4:
                break
            chosen.append(cand[int(np.argmax(dmin))])
        pts.extend(chosen)
        if len(pts) >= 8:
            break
    return pts


def split_with_connectors(solid: m3.Manifold, o, n, mode="none", pin_d=None, pin_len=None,
                          clearance=None):
    """Split a solid by a plane. Returns (above, below, extra_bodies, info)."""
    o = np.asarray(o, float)
    n = G.unit(n)
    above, below = solid.split_by_plane(tuple(n), float(np.dot(n, o)))
    info = {"connectors": 0, "skipped": False}
    extras = []
    if mode in (None, "", "none") or above.is_empty() or below.is_empty():
        return above, below, extras, info
    d0, L0, c0 = CONNECTORS[mode]
    d = float(pin_d or d0)
    L = float(pin_len or L0)
    c = float(clearance if clearance is not None else c0)
    r = d / 2
    # filament pins are meant for thin shells, so they may sit closer to the surface
    wall = 0.45 if mode == "filament" else max(1.2, 0.6 * r)
    cs, (o, x, y, n) = section_shape(solid, o, n)
    pts = connector_points(cs, r + c, wall, max_per_part=4)
    if not pts:
        info["skipped"] = True
        return above, below, extras, info
    hole_r = r + c
    for u, v in pts:
        P = o + u * x + v * y
        if mode == "pins":
            pin = G.make_cylinder(P, -n, L / 2 - 0.2, r, r, 48)
            tip = G.make_cylinder(P - n * (L / 2 - 0.2), -n, 0.5, r, max(r - 0.5, 0.2), 48)
            above = above + pin + tip
            below = below - G.make_cylinder(P + n * 0.01, -n, L / 2 + 0.4, hole_r, hole_r, 48)
        elif mode == "magnets":
            depth = L + c + 0.1
            above = above - G.make_cylinder(P - n * 0.01, n, depth, hole_r, hole_r, 48)
            below = below - G.make_cylinder(P + n * 0.01, -n, depth, hole_r, hole_r, 48)
        else:  # holes on both sides (dowels / filament)
            half = L / 2 + 0.4
            above = above - G.make_cylinder(P - n * 0.01, n, half, hole_r, hole_r, 48)
            below = below - G.make_cylinder(P + n * 0.01, -n, half, hole_r, hole_r, 48)
            if mode == "dowels":
                dowel = G.make_cylinder(P - n * (L / 2 - 0.2), n, L - 0.4, r - 0.05, r - 0.05, 48)
                extras.append(dowel)
        info["connectors"] += 1
    return above, below, extras, info


# ------------------------------------------------------------------------------------------
# Split to fit the print bed
# ------------------------------------------------------------------------------------------
def _axis_section_area(solid, axis, pos):
    n = np.zeros(3)
    n[axis] = 1.0
    o = np.zeros(3)
    o[axis] = pos
    cs, _ = section_shape(solid, o, n)
    return cs.area()


def plan_cuts(solid: m3.Manifold, usable, optimize=True):
    """Cut positions per axis so that every piece fits `usable` (x, y, z), placing each cut
    where the cross-section is smallest within the allowed range."""
    b = solid.bounding_box()
    lo, hi = np.array(b[:3]), np.array(b[3:])
    cuts = {}
    for ax in range(3):
        S = hi[ax] - lo[ax]
        U = float(usable[ax])
        npieces = int(math.ceil(S / U - 1e-9))
        if npieces <= 1:
            continue
        pos = []
        prev = lo[ax]
        for k in range(1, npieces):
            remaining = npieces - k
            even = lo[ax] + S * k / npieces
            lo_ok = max(prev + 1.0, hi[ax] - remaining * U)
            hi_ok = min(prev + U, hi[ax] - 1.0)
            if lo_ok > hi_ok:
                lo_ok = hi_ok = min(max(even, lo_ok), hi_ok)
            if optimize and hi_ok - lo_ok > 1.0:
                cand = np.linspace(lo_ok, hi_ok, 13)
                # prefer small cross-sections, but stay near the even split
                areas = np.array([_axis_section_area(solid, ax, c) for c in cand])
                score = areas / (areas.max() + 1e-9) + 0.15 * np.abs(cand - even) / (hi_ok - lo_ok + 1e-9)
                p = float(cand[int(np.argmin(score))])
            else:
                p = float(min(max(even, lo_ok), hi_ok))
            pos.append(p)
            prev = p
        cuts[ax] = pos
    return cuts


def split_to_bed(solid: m3.Manifold, bed, margin=5.0, mode="pins", pin_d=None, pin_len=None,
                 clearance=None, optimize=True):
    usable = np.asarray(bed, float) - 2 * float(margin)
    if np.any(usable <= 5):
        raise GeometryError("The margin is too large for this bed.")
    b = solid.bounding_box()
    size = np.array(b[3:]) - np.array(b[:3])
    # use the bed's longest horizontal side along the part's longest horizontal side
    swap = (size[0] < size[1]) != (usable[0] < usable[1])
    if swap:
        usable = usable[[1, 0, 2]]
    cuts = plan_cuts(solid, usable, optimize)
    pieces = [solid]
    extras = []
    total_conn, skipped = 0, 0
    for ax, positions in cuts.items():
        nvec = np.zeros(3)
        nvec[ax] = 1.0
        for pos in positions:
            new = []
            for pc in pieces:
                pb = pc.bounding_box()
                if not (pb[ax] < pos < pb[ax + 3]):
                    new.append(pc)
                    continue
                o = np.zeros(3)
                o[ax] = pos
                a, bl, ex, info = split_with_connectors(pc, o, nvec, mode, pin_d, pin_len, clearance)
                total_conn += info["connectors"]
                skipped += 1 if info["skipped"] else 0
                extras += ex
                new += [m for m in (bl, a) if not m.is_empty()]
            pieces = new
    return pieces, extras, {"cuts": {"xyz"[k]: [round(p, 2) for p in v] for k, v in cuts.items()},
                            "connectors": total_conn, "faces_without_connectors": skipped,
                            "swapped": bool(swap)}


# ------------------------------------------------------------------------------------------
# Sculpt brushes and remeshing
# ------------------------------------------------------------------------------------------
def _adjacency(f, n):
    from scipy.sparse import coo_matrix
    e = np.concatenate([f[:, [0, 1]], f[:, [1, 2]], f[:, [2, 0]]])
    e = np.concatenate([e, e[:, ::-1]])
    A = coo_matrix((np.ones(len(e)), (e[:, 0], e[:, 1])), shape=(n, n)).tocsr()
    A.data[:] = 1.0
    deg = np.asarray(A.sum(axis=1)).ravel()
    return A, np.maximum(deg, 1)


def _vertex_normals(v, f):
    t = v[f]
    fn = np.cross(t[:, 1] - t[:, 0], t[:, 2] - t[:, 0])
    vn = np.zeros_like(v)
    for k in range(3):
        np.add.at(vn, f[:, k], fn)
    ln = np.linalg.norm(vn, axis=1, keepdims=True)
    return np.divide(vn, ln, out=np.zeros_like(vn), where=ln > 0)


def _resample(points, spacing):
    P = np.asarray(points, float)
    if len(P) < 2:
        return P
    out = [P[0]]
    for a, b in zip(P[:-1], P[1:]):
        L = np.linalg.norm(b - a)
        k = int(L // spacing)
        for i in range(1, k + 1):
            out.append(a + (b - a) * (i * spacing / L))
        out.append(b)
    return np.array(out)


def sculpt(solid: m3.Manifold, strokes: list[dict]) -> m3.Manifold:
    from scipy.spatial import cKDTree
    mm = solid.to_mesh()
    v = np.asarray(mm.vert_properties, np.float64)[:, :3].copy()
    f = np.asarray(mm.tri_verts, np.int64)
    A, deg = _adjacency(f, len(v))
    for st in strokes:
        r = float(st.get("radius", 5))
        k = float(np.clip(st.get("strength", 0.5), 0.0, 1.0))
        mode = st.get("mode", "smooth")
        samples = _resample(st.get("points", []), r / 4)
        if not len(samples) or r <= 0:
            continue
        tree = cKDTree(v)
        idx = sorted(set(i for lst in tree.query_ball_point(samples, r) for i in lst))
        if not idx:
            continue
        idx = np.array(idx)
        dd, _ = cKDTree(samples).query(v[idx])
        w = (1 - np.clip(dd / r, 0, 1) ** 2) ** 2 * k
        if mode == "smooth":
            for _ in range(4):
                lap = (A[idx] @ v) / deg[idx, None] - v[idx]
                v[idx] += 0.6 * w[:, None] * lap
        elif mode == "flatten":
            c = (v[idx] * w[:, None]).sum(0) / max(w.sum(), 1e-12)
            X = (v[idx] - c) * np.sqrt(w)[:, None]
            nrm = np.linalg.svd(X, full_matrices=False)[2][-1]
            dist = (v[idx] - c) @ nrm
            v[idx] -= (w * dist)[:, None] * nrm
        elif mode in ("inflate", "deflate"):
            vn = _vertex_normals(v, f)
            sgn = 1.0 if mode == "inflate" else -1.0
            v[idx] += sgn * (w * r * 0.12)[:, None] * vn[idx]
            for _ in range(1):
                lap = (A[idx] @ v) / deg[idx, None] - v[idx]
                v[idx] += 0.3 * w[:, None] * lap
        else:
            raise GeometryError(f"Unknown brush {mode}.")
    out = G.manifold_from_arrays(v, f)
    if out is None:
        raise GeometryError("The brush strokes folded the surface over itself. Use a smaller "
                            "strength or radius.")
    return out


def remesh(solid: m3.Manifold, edge: float) -> m3.Manifold:
    """Even out the triangles: split long edges to `edge`, then merge tiny ones."""
    if edge <= 0:
        raise GeometryError("Edge length must be positive.")
    fine = solid.refine_to_length(float(edge))
    mm = fine.to_mesh()
    v = np.asarray(mm.vert_properties, np.float32)[:, :3]
    f = np.asarray(mm.tri_verts, np.int64)
    area = float(solid.surface_area())
    target = max(int(area / (0.433 * edge * edge)), 200)
    if len(f) > target * 1.2:
        import fast_simplification
        nv, nf = fast_simplification.simplify(v, f, target_reduction=1 - target / len(f))
        m = G.manifold_from_arrays(nv, nf)
        if m is not None:
            return m
    return fine


# ------------------------------------------------------------------------------------------
# Push / pull a flat face
# ------------------------------------------------------------------------------------------
def face_outline(verts, faces, region, frame):
    """Outline of a planar triangle region as contours in plane (u, v) coordinates."""
    o, x, y, n = frame
    t = np.asarray(verts, np.float64)[np.asarray(faces)[region]]
    uv = np.stack([(t - o) @ x, (t - o) @ y], axis=-1)  # (k, 3, 2)
    # orient every triangle counter-clockwise, then union them
    a = uv[:, 1] - uv[:, 0]
    b = uv[:, 2] - uv[:, 0]
    cw = (a[:, 0] * b[:, 1] - a[:, 1] * b[:, 0]) < 0
    uv[cw] = uv[cw][:, ::-1]
    cs = m3.CrossSection([np.ascontiguousarray(tri) for tri in uv], m3.FillRule.NonZero)
    return [np.asarray(c).tolist() for c in cs.to_polygons()]


def pushpull_tool(contours, frame, distance):
    o, x, y, n = (np.asarray(a, float) for a in frame)
    cs = m3.CrossSection([np.ascontiguousarray(np.asarray(c, np.float64)) for c in contours],
                         m3.FillRule.EvenOdd)
    if cs.is_empty():
        raise GeometryError("That face has no area.")
    d = float(distance)
    if abs(d) < 1e-9:
        raise GeometryError("Distance can't be zero.")
    man = m3.Manifold.extrude(cs, abs(d))
    if d < 0:
        man = man.translate((0, 0, -abs(d)))
    return man.transform(G.frame_matrix(o, x, y, n))


# ------------------------------------------------------------------------------------------
# Lay flat
# ------------------------------------------------------------------------------------------
def rotation_to(a, b):
    """Rotation matrix taking unit vector a onto unit vector b."""
    a, b = G.unit(a), G.unit(b)
    v = np.cross(a, b)
    c = float(np.dot(a, b))
    if c < -0.999999:
        # opposite: half turn about any axis perpendicular to a
        axis = G.unit(np.cross(a, [1.0, 0, 0]) if abs(a[0]) < 0.9 else np.cross(a, [0, 1.0, 0]))
        return 2 * np.outer(axis, axis) - np.eye(3)
    K = np.array([[0, -v[2], v[1]], [v[2], 0, -v[0]], [-v[1], v[0], 0]])
    return np.eye(3) + K + K @ K / (1 + c)


def best_rest_normal(solid: m3.Manifold):
    """Outward normal of the largest flat area of the convex hull: the most stable way to
    stand the part on the bed."""
    hull = solid.hull().to_mesh()
    v = np.asarray(hull.vert_properties, np.float64)[:, :3]
    f = np.asarray(hull.tri_verts, np.int64)
    t = v[f]
    n = np.cross(t[:, 1] - t[:, 0], t[:, 2] - t[:, 0])
    area = np.linalg.norm(n, axis=1) / 2
    n = np.divide(n, (2 * area)[:, None], out=np.zeros_like(n), where=area[:, None] > 0)
    key = np.round(n, 3)
    uniq, inv = np.unique(key, axis=0, return_inverse=True)
    tot = np.bincount(inv.ravel(), weights=area)
    best = int(np.argmax(tot))
    sel = inv.ravel() == best
    return G.unit((n[sel] * area[sel, None]).sum(0))


def lay_flat_matrix(solid: m3.Manifold, normal):
    """3x4 transform: turn `normal` straight down, keep the part's X/Y centre, rest it on Z=0."""
    R = rotation_to(normal, (0, 0, -1.0))
    b = solid.bounding_box()
    c = (np.array(b[:3]) + np.array(b[3:])) / 2
    M = np.column_stack([R, c - R @ c])
    moved = solid.transform(M)
    nb = moved.bounding_box()
    nc = (np.array(nb[:3]) + np.array(nb[3:])) / 2
    shift = np.array([c[0] - nc[0], c[1] - nc[1], -nb[2]])
    M[:, 3] += shift
    return M


# ------------------------------------------------------------------------------------------
# Threads (ISO metric 60° profile, built as a helical sweep so the flanks follow the helix)
# ------------------------------------------------------------------------------------------
METRIC_COARSE = {2: 0.4, 2.5: 0.45, 3: 0.5, 4: 0.7, 5: 0.8, 6: 1.0, 8: 1.25, 10: 1.5,
                 12: 1.75, 14: 2.0, 16: 2.0, 20: 2.5, 24: 3.0, 30: 3.5}


def _thread_section(D, P, n):
    H = 0.8660254 * P
    R = D / 2
    depth = 5 * H / 8
    a, b = P / 8, P / 8 + 5 * P / 16
    c, d = b + P / 8, b + P / 8 + 5 * P / 16

    def f(u):
        u = u % P
        if u < a or u >= d:
            return R - depth
        if u < b:
            return R - depth + depth * (u - a) / (b - a)
        if u < c:
            return R
        return R - depth * (u - c) / (d - c)
    th = np.linspace(0, 2 * math.pi, n, endpoint=False)
    r = np.array([f(t / (2 * math.pi) * P) for t in th])
    return np.column_stack([r * np.cos(th), r * np.sin(th)]), depth


def _wall(outer, axis, flip):
    """Triangles closing the flat cut between a column of profile corners and the axis
    (both lists of (index, z), z increasing, same first and last z)."""
    tris = []
    i = j = 0
    while i < len(outer) - 1 or j < len(axis) - 1:
        if j == len(axis) - 1 or (i < len(outer) - 1 and outer[i + 1][1] <= axis[j + 1][1]):
            t = (outer[i][0], outer[i + 1][0], axis[j][0])
            i += 1
        else:
            t = (outer[i][0], axis[j + 1][0], axis[j][0])
            j += 1
        tris.append(t[::-1] if flip else t)
    return tris


def _helical_thread(D, P, L, N):
    """Right-hand ISO thread as a closed mesh: the four profile corners (root end, crest
    start, crest end, root start) of every turn are helices sampled at N angles, stitched
    into one band, closed at both ends by a helicoid and a flat cut. Reaches past z=0 and
    z=L by about a pitch; the caller clips it. Surface radius: f(phi*P/2pi - z)."""
    H = 0.8660254 * P
    R = D / 2
    Rm = R - 5 * H / 8
    U = (14 * P / 16, 9 * P / 16, 7 * P / 16, 2 * P / 16)       # u of each corner, z order
    RR = (Rm, R, R, Rm)
    M = int(math.ceil(L / P - 1e-9)) + 3
    J = 4 * M
    zb = -P
    k = np.arange(N)
    phi = 2 * math.pi * k / N
    j = np.arange(J)
    r = np.array(RR)[j % 4]
    z = zb + (j // 4)[:, None] * P - np.array(U)[j % 4][:, None] + (k * P / N)[None, :]
    V = np.empty((J, N, 3))
    V[:, :, 0] = r[:, None] * np.cos(phi)[None, :]
    V[:, :, 1] = r[:, None] * np.sin(phi)[None, :]
    V[:, :, 2] = z
    V = V.reshape(-1, 3)
    idx = lambda jj, kk: jj * N + kk
    # band
    jj, kk = np.meshgrid(np.arange(J - 1), np.arange(N - 1), indexing="ij")
    a, b, c, d = idx(jj, kk), idx(jj, kk + 1), idx(jj + 1, kk + 1), idx(jj + 1, kk)
    js = np.arange(J - 5)
    sa, sb, sc, sd = idx(js, N - 1), idx(js + 4, 0), idx(js + 5, 0), idx(js + 1, N - 1)
    A = np.concatenate([a.ravel(), sa]); B = np.concatenate([b.ravel(), sb])
    C = np.concatenate([c.ravel(), sc]); Dd = np.concatenate([d.ravel(), sd])
    tris = [np.column_stack([A, B, C]), np.column_stack([A, C, Dd])]
    # bottom: helicoid under corner row 0, flat cut at phi=0
    base = J * N
    ab = base + np.arange(N + 1)
    zab = zb - U[0] + np.arange(N + 1) * P / N
    o = np.concatenate([idx(0, np.arange(N)), [idx(4, 0)]])
    tris.append(np.column_stack([ab[:-1], o[1:], o[:-1]]))
    tris.append(np.column_stack([ab[:-1], ab[1:], o[1:]]))
    col = [(idx(q, 0), V[idx(q, 0), 2]) for q in range(5)]
    tris.append(np.array(_wall(col, list(zip(ab.tolist(), zab.tolist())), False)))
    # top: helicoid over corner row J-1, flat cut at phi of column N-1
    at = base + N + 1 + np.arange(N + 1)
    zat = zb + (M - 1) * P - U[3] + (np.arange(N + 1) - 1) * P / N
    o = np.concatenate([[idx(J - 5, N - 1)], idx(J - 1, np.arange(N))])
    tris.append(np.column_stack([at[:-1], o[:-1], o[1:]]))
    tris.append(np.column_stack([at[:-1], o[1:], at[1:]]))
    col = [(idx(q, N - 1), V[idx(q, N - 1), 2]) for q in range(J - 5, J)]
    tris.append(np.array(_wall(col, list(zip(at.tolist(), zat.tolist())), True)))
    Vaxis = np.zeros((2 * (N + 1), 3))
    Vaxis[:N + 1, 2] = zab
    Vaxis[N + 1:, 2] = zat
    verts = np.vstack([V, Vaxis])
    man = G.manifold_from_arrays(verts, np.vstack(tris))
    if man is None:
        raise GeometryError("Could not build this thread. Try a different size or pitch.")
    return man


def threaded_rod(D, P, L, hand="right", chamfer=True):
    """Threaded rod along +Z from 0 to L (major diameter D, pitch P)."""
    if D <= 0 or P <= 0 or L <= 0:
        raise GeometryError("Diameter, pitch and length must be positive.")
    if P > D / 2:
        raise GeometryError("The pitch is too coarse for this diameter.")
    n = int(np.floor(min(240, max(90, D * 10)) + 0.5))
    depth = 5 * 0.8660254 * P / 8
    rod = _helical_thread(D, P, L, n)
    if hand != "right":
        rod = rod.mirror((0, 1, 0))
    R = D / 2
    c = min(depth * 1.1, L / 3) if chamfer else 0.0
    prof = [[0, 0], [R - c, 0], [R + 0.01, c], [R + 0.01, L - c], [R - c, L], [0, L]] if chamfer else [[0, 0], [R + 0.01, 0], [R + 0.01, L], [0, L]]
    clip = m3.Manifold.revolve(m3.CrossSection([np.array(prof, float)]), n).rotate((0, 0, 180.0 / n))
    return rod ^ clip


def thread_tool(kind, D, P, L, tolerance=0.2, hand="right", chamfer=True):
    """Solid in local coordinates (axis +Z from the surface at z=0).
    external: the screw itself, standing on z=0.  internal: what to cut for a tapped hole,
    going down into the material (z from -L to just above 0)."""
    tol = float(tolerance)
    if kind == "external":
        return threaded_rod(D - tol, P, L, hand, chamfer)
    over = 0.6
    rod = threaded_rod(D + tol, P, L + over, hand, chamfer=False).translate((0, 0, -L))
    if chamfer:
        _, depth = _thread_section(D + tol, P, 8)
        R = (D + tol) / 2
        cs = m3.Manifold.cylinder(depth + over, R - depth, R + over, 96).translate((0, 0, -depth))
        rod = rod + cs
    return rod
