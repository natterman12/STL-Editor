import math

import numpy as np
import manifold3d as m3

from ..geometry import GeometryError
from ..features import _affine, _float, _bbox


def _thin_residual(cs, w):
    """The parts of a slice narrower than w (a wall, pin or rib thinner than that)."""
    r = w / 2 - 0.005 * w
    inner = cs.offset(-r, m3.JoinType.Miter, 2.0, 0)
    if inner.is_empty():
        return cs
    return cs - inner.offset(r, m3.JoinType.Miter, 2.0, 0)


def _thin_parts(cs, w, min_area):
    res = _thin_residual(cs, w)
    if res.is_empty():
        return []
    return [c for c in res.decompose() if c.area() >= min_area]


def thin_scan(solid, t):
    """Thin spots of a solid: {"min": thinnest width found or None, "count": clusters,
    "spots": [[x, y, z], ...] (up to 10, biggest cluster first)}."""
    t = float(t)
    min_area = 0.05 * t * t
    lo, hi = _bbox(solid)
    found = []              # (x, y, z) of every thin piece found
    widths = []             # per axis: thinnest slice width found there
    link = 1.5 * t * 2
    for a in range(3):
        u, v = (a + 1) % 3, (a + 2) % 3
        R = np.zeros((3, 3))
        R[0, u] = R[1, v] = R[2, a] = 1.0
        m = solid.transform(_affine(R, np.zeros(3)))
        ext = float(hi[a] - lo[a])
        step = max(t / 2, ext / 60)
        n = max(1, int(math.ceil(ext / step - 1e-9)))
        link = max(link, 1.5 * ext / n)
        flagged = []
        for i in range(n):
            h = float(lo[a] + (i + 0.5) * ext / n)
            cs = m.slice(h)
            if cs.is_empty():
                continue
            parts = _thin_parts(cs, t, min_area)
            if not parts:
                continue
            flagged.append(cs)
            for c in parts:
                b = c.bounds()
                p = [0.0, 0.0, 0.0]
                p[a], p[u], p[v] = h, (b[0] + b[2]) / 2, (b[1] + b[3]) / 2
                found.append(tuple(p))
        if flagged:
            a_lo, a_hi = 0.0, t
            for _ in range(8):
                mid = (a_lo + a_hi) / 2
                if any(_thin_parts(cs, mid, min_area) for cs in flagged):
                    a_hi = mid
                else:
                    a_lo = mid
            widths.append(a_hi * 0.99)
    if not found:
        return {"min": None, "count": 0, "spots": []}
    # group pieces that sit close together into one spot
    cell = {}
    for i, p in enumerate(found):
        cell.setdefault(tuple(int(math.floor(c / link)) for c in p), []).append(i)
    parent = list(range(len(found)))

    def root(i):
        while parent[i] != i:
            parent[i] = parent[parent[i]]
            i = parent[i]
        return i

    for key, ids in cell.items():
        for dx in (-1, 0, 1):
            for dy in (-1, 0, 1):
                for dz in (-1, 0, 1):
                    other = cell.get((key[0] + dx, key[1] + dy, key[2] + dz))
                    if not other:
                        continue
                    for i in ids:
                        for j in other:
                            if i < j and math.dist(found[i], found[j]) <= link:
                                parent[root(j)] = root(i)
    groups = {}
    for i in range(len(found)):
        groups.setdefault(root(i), []).append(found[i])
    spots = []
    for pts in groups.values():
        arr = np.array(pts)
        spots.append((len(pts), [round(float((arr[:, k].min() + arr[:, k].max()) / 2), 2) for k in range(3)]))
    spots.sort(key=lambda s: (-s[0], s[1]))
    return {"min": round(max(widths), 3), "count": len(spots), "spots": [s[1] for s in spots[:10]]}


def f_thincheck(state, fid, p, res, ctx):
    """Reports walls, pins and ribs thinner than the minimum; changes nothing."""
    bid = p.get("body")
    if not bid:
        raise GeometryError("Choose a body to check.")
    t = _float(p, "min", 0.8)
    if t <= 0:
        raise GeometryError("The minimum thickness must be more than zero.")
    r = thin_scan(state.body(bid).require_solid(), t)
    res["thin"] = r
    tt = f"{t:g}"
    if not r["count"]:
        res["info"] = f"Nothing thinner than {tt} mm found."
        return
    where = "; ".join(f"X {s[0]:g}, Y {s[1]:g}, Z {s[2]:g}" for s in r["spots"][:3])
    res["warning"] = (f"{r['count']} thin spot{'s' if r['count'] != 1 else ''} under {tt} mm. "
                      f"Thinnest: about {r['min']:g} mm. Near {where} (mm).")


from .registry import register
register('thincheck', f_thincheck, 'Thin-wall Check')
