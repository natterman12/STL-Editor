"""2D sketch entities and their conversion to Manifold cross-sections."""
from __future__ import annotations

import math

import numpy as np
import manifold3d as m3

from .geometry import GeometryError


def _segs(r: float) -> int:
    return int(min(256, max(48, r * 10)))


def entity_contours(e: dict) -> list[np.ndarray]:
    """Closed contours (CCW, Nx2) for a closed entity; [] for open/construction entities."""
    t = e.get("type")
    if e.get("construction"):
        return []
    if t == "rect":
        x, y, w, h = (float(e[k]) for k in ("x", "y", "w", "h"))
        if abs(w) < 1e-9 or abs(h) < 1e-9:
            return []
        x0, x1 = sorted((x, x + w))
        y0, y1 = sorted((y, y + h))
        r = float(e.get("fillet") or 0)
        r = min(r, (x1 - x0) / 2 - 1e-6, (y1 - y0) / 2 - 1e-6)
        if r > 1e-6:
            n = max(8, _segs(r) // 4)
            pts = []
            for cx, cy, a0 in ((x1 - r, y0 + r, -90), (x1 - r, y1 - r, 0), (x0 + r, y1 - r, 90),
                               (x0 + r, y0 + r, 180)):
                for i in range(n + 1):
                    a = math.radians(a0 + 90 * i / n)
                    pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
            c = np.array(pts)
        else:
            c = np.array([[x0, y0], [x1, y0], [x1, y1], [x0, y1]], float)
        return [_rotate(c, e)]
    if t == "circle":
        r = float(e["r"])
        if r <= 0:
            return []
        n = _segs(r)
        a = np.linspace(0, 2 * math.pi, n, endpoint=False)
        return [np.column_stack([float(e["cx"]) + r * np.cos(a), float(e["cy"]) + r * np.sin(a)])]
    if t == "polygon":
        r, n = float(e["r"]), int(e.get("n", 6))
        if r <= 0 or n < 3:
            return []
        a0 = math.radians(float(e.get("angle", 0)))
        a = a0 + np.arange(n) * 2 * math.pi / n
        return [np.column_stack([float(e["cx"]) + r * np.cos(a), float(e["cy"]) + r * np.sin(a)])]
    if t == "slot":
        x1, y1, x2, y2, r = (float(e[k]) for k in ("x1", "y1", "x2", "y2", "r"))
        if r <= 0:
            return []
        d = np.array([x2 - x1, y2 - y1])
        L = float(np.linalg.norm(d))
        if L < 1e-9:
            return entity_contours({"type": "circle", "cx": x1, "cy": y1, "r": r})
        ang = math.atan2(d[1], d[0])
        n = max(12, _segs(r) // 2)
        pts = []
        for i in range(n + 1):
            a = ang - math.pi / 2 + math.pi * i / n
            pts.append((x2 + r * math.cos(a), y2 + r * math.sin(a)))
        for i in range(n + 1):
            a = ang + math.pi / 2 + math.pi * i / n
            pts.append((x1 + r * math.cos(a), y1 + r * math.sin(a)))
        return [np.array(pts)]
    if t == "polyline":
        pts = np.asarray(e.get("pts", []), float)
        if not e.get("closed") or len(pts) < 3:
            return []
        return [pts]
    return []


def _rotate(c, e):
    ang = float(e.get("angle") or 0)
    if not ang:
        return c
    cx = float(e["x"]) + float(e["w"]) / 2
    cy = float(e["y"]) + float(e["h"]) / 2
    a = math.radians(ang)
    R = np.array([[math.cos(a), -math.sin(a)], [math.sin(a), math.cos(a)]])
    return (c - [cx, cy]) @ R.T + [cx, cy]


def _arc_points(e: dict, n=None) -> np.ndarray:
    cx, cy, r = float(e["cx"]), float(e["cy"]), float(e["r"])
    a0, a1 = math.radians(float(e["a0"])), math.radians(float(e["a1"]))
    sweep = (a1 - a0) % (2 * math.pi)
    if sweep < 1e-9:
        sweep = 2 * math.pi
    n = n or max(8, int(_segs(r) * sweep / (2 * math.pi)))
    a = a0 + np.linspace(0, sweep, n + 1)
    return np.column_stack([cx + r * np.cos(a), cy + r * np.sin(a)])


def entity_path(e: dict):
    """(points, closed) outline of any entity, used for linework, picking and trimming."""
    t = e.get("type")
    if t == "line":
        return np.array([e["p1"], e["p2"]], float), False
    if t == "arc":
        return _arc_points(e), False
    if t == "polyline":
        return np.asarray(e.get("pts", []), float), bool(e.get("closed"))
    c = entity_contours({**e, "construction": False})
    return (c[0], True) if c else (np.zeros((0, 2)), False)


def _q(p, tol=1e-5):
    return (round(p[0] / tol) * tol, round(p[1] / tol) * tol)


def _linework(entities, only=None):
    from shapely.geometry import LineString
    lines = []
    for e in entities:
        if e.get("construction") or (only and e.get("id") not in only):
            continue
        pts, closed = entity_path(e)
        if len(pts) < 2:
            continue
        pts = [_q(p) for p in pts]
        if closed:
            pts = pts + [pts[0]]
        lines.append(LineString(pts))
    return lines


def faces(entities, only=None):
    """All minimal enclosed regions of the sketch (shapely Polygons), lines and shapes alike."""
    from shapely.ops import unary_union, polygonize
    lw = _linework(entities, only)
    if not lw:
        return []
    return [f for f in polygonize(unary_union(lw)) if f.area > 1e-10]


def _cs_from_polys(polys) -> m3.CrossSection:
    contours = []
    for g in polys:
        for poly in getattr(g, "geoms", [g]):
            contours.append(np.asarray(poly.exterior.coords)[:-1])
            for hole in poly.interiors:
                contours.append(np.asarray(hole.coords)[:-1])
    if not contours:
        raise GeometryError("No closed region there.")
    return m3.CrossSection([np.ascontiguousarray(c, dtype=np.float64) for c in contours], m3.FillRule.EvenOdd)


def profile(entities: list[dict], only: list | None = None, regions: list | None = None) -> m3.CrossSection:
    """The area to extrude or revolve.

    Without `regions`: shapes inside other shapes become holes (even-odd), and closed loops made
    of separate lines and arcs count as shapes too. With `regions` (points in sketch coordinates):
    exactly the enclosed regions under those points, e.g. a single island or one half of a box
    split by a line."""
    if regions:
        from shapely.geometry import Point
        from shapely.ops import unary_union
        fs = faces(entities, only)
        picked = [f for f in fs if any(f.contains(Point(float(u), float(v))) for u, v in regions)]
        if not picked:
            raise GeometryError("None of the picked points is inside a closed region of the sketch.")
        cs = _cs_from_polys([unary_union(picked)])
    else:
        contours = []
        for e in entities:
            if only and e.get("id") not in only:
                continue
            contours.extend(entity_contours(e))
        # closed loops formed by lines / arcs / open polylines
        loose = [e for e in entities if e.get("type") in ("line", "arc") or
                 (e.get("type") == "polyline" and not e.get("closed"))]
        if loose:
            for f in faces(loose, only):
                contours.append(np.asarray(f.exterior.coords)[:-1])
        if not contours:
            raise GeometryError("The sketch has no closed profile to use. Draw a rectangle, circle, "
                                "polygon, slot, or a closed loop of lines and arcs.")
        cs = m3.CrossSection([np.ascontiguousarray(c, dtype=np.float64) for c in contours],
                             m3.FillRule.EvenOdd)
    if cs.is_empty() or cs.area() <= 1e-12:
        raise GeometryError("The sketch profile has no area.")
    return cs


# ------------------------------------------------------------------------------------------
# Sketch editing operations (trim / offset / mirror)
# ------------------------------------------------------------------------------------------
def _nid(base, k):
    return f"{base}_{k}"


def explode(e: dict) -> list[dict]:
    """Break a compound shape into lines and arcs (keeps construction flag)."""
    t = e.get("type")
    c = {"construction": e.get("construction", False)}
    if t in ("line", "arc"):
        return [e]
    if t == "circle":
        return [e]
    if t == "rect" and not float(e.get("fillet") or 0) and not float(e.get("angle") or 0):
        x, y, w, h = (float(e[k]) for k in ("x", "y", "w", "h"))
        P = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
        return [dict(c, id=_nid(e["id"], i), type="line", p1=list(P[i]), p2=list(P[(i + 1) % 4])) for i in range(4)]
    if t == "slot":
        x1, y1, x2, y2, r = (float(e[k]) for k in ("x1", "y1", "x2", "y2", "r"))
        ang = math.degrees(math.atan2(y2 - y1, x2 - x1))
        nx, ny = -math.sin(math.radians(ang)) * r, math.cos(math.radians(ang)) * r
        return [dict(c, id=_nid(e["id"], 0), type="line", p1=[x1 + nx, y1 + ny], p2=[x2 + nx, y2 + ny]),
                dict(c, id=_nid(e["id"], 1), type="line", p1=[x2 - nx, y2 - ny], p2=[x1 - nx, y1 - ny]),
                dict(c, id=_nid(e["id"], 2), type="arc", cx=x2, cy=y2, r=r, a0=ang - 90, a1=ang + 90),
                dict(c, id=_nid(e["id"], 3), type="arc", cx=x1, cy=y1, r=r, a0=ang + 90, a1=ang + 270)]
    pts, closed = entity_path(e)
    n = len(pts)
    segs = [(pts[i], pts[(i + 1) % n]) for i in range(n if closed else n - 1)]
    return [dict(c, id=_nid(e["id"], i), type="line", p1=list(map(float, a)), p2=list(map(float, b)))
            for i, (a, b) in enumerate(segs)]


def _geom(e):
    from shapely.geometry import LineString
    pts, closed = entity_path(e)
    if len(pts) < 2:
        return None
    pts = list(map(tuple, pts)) + ([tuple(pts[0])] if closed else [])
    return LineString(pts)


def _param(prim, pt):
    """Parameter of a point on a line (0..1) or arc/circle (degrees)."""
    if prim["type"] == "line":
        a, b = np.array(prim["p1"], float), np.array(prim["p2"], float)
        d = b - a
        return float(np.dot(np.asarray(pt) - a, d) / max(np.dot(d, d), 1e-18))
    return math.degrees(math.atan2(pt[1] - float(prim["cy"]), pt[0] - float(prim["cx"])))


def trim(entities: list[dict], eid: str, point) -> list[dict]:
    """Remove the piece of entity `eid` under `point`, between its nearest intersections."""
    from shapely.geometry import Point
    target = next((e for e in entities if e.get("id") == eid), None)
    if target is None:
        raise GeometryError("Nothing to trim there.")
    others = [e for e in entities if e.get("id") != eid]
    prims = explode(target)
    pt = Point(float(point[0]), float(point[1]))
    gi = min(range(len(prims)), key=lambda i: (_geom(prims[i]) or pt).distance(pt))
    prim = prims[gi]
    rest = prims[:gi] + prims[gi + 1:]
    g = _geom(prim)
    cuts = []
    for o in others + rest:
        go = _geom(o)
        if go is None or not g.intersects(go):
            continue
        inter = g.intersection(go)
        for q in getattr(inter, "geoms", [inter]):
            if q.geom_type == "Point":
                cuts.append(_param(prim, (q.x, q.y)))
            elif hasattr(q, "coords"):
                for c in q.coords:
                    cuts.append(_param(prim, c))
    tp = _param(prim, (pt.x, pt.y))
    out = []
    if prim["type"] == "line":
        cs = sorted({round(c, 9) for c in cuts if 1e-6 < c < 1 - 1e-6})
        lo = max([c for c in cs if c <= tp], default=0.0)
        hi = min([c for c in cs if c >= tp], default=1.0)
        a, b = np.array(prim["p1"], float), np.array(prim["p2"], float)
        if lo > 1e-6:
            out.append(dict(prim, id=prim["id"] + "a", p1=a.tolist(), p2=(a + (b - a) * lo).tolist()))
        if hi < 1 - 1e-6:
            out.append(dict(prim, id=prim["id"] + "b", p1=(a + (b - a) * hi).tolist(), p2=b.tolist()))
    else:
        full = prim["type"] == "circle"
        a0 = 0.0 if full else float(prim["a0"])
        sweep = 360.0 if full else (float(prim["a1"]) - a0) % 360 or 360.0
        rel = lambda ang: (ang - a0) % 360
        if full:
            cs = sorted({round(rel(c), 7) % 360 for c in cuts})
        else:
            cs = sorted({round(rel(c), 7) for c in cuts if 1e-5 < rel(c) < sweep - 1e-5})
        t = rel(tp)
        base = {k: prim[k] for k in ("cx", "cy", "r")}
        base["construction"] = prim.get("construction", False)
        if full:
            if len(cs) < 2:
                cs = []  # a circle cut once (or not at all) is removed entirely
            else:
                lo = max([c for c in cs if c <= t], default=cs[-1] - 360)
                hi = min([c for c in cs if c >= t], default=cs[0] + 360)
                out.append(dict(base, id=prim["id"] + "a", type="arc", a0=a0 + hi, a1=a0 + lo + 360))
        else:
            lo = max([c for c in cs if c <= t], default=0.0)
            hi = min([c for c in cs if c >= t], default=sweep)
            if lo > 1e-5:
                out.append(dict(base, id=prim["id"] + "a", type="arc", a0=a0, a1=a0 + lo))
            if hi < sweep - 1e-5:
                out.append(dict(base, id=prim["id"] + "b", type="arc", a0=a0 + hi, a1=a0 + sweep))
    res = []
    for e in entities:
        if e.get("id") == eid:
            res.extend(rest + out)
        else:
            res.append(e)
    return res


def offset(entities: list[dict], ids: list, d: float, new_id) -> list[dict]:
    """Offset copies of the chosen shapes (positive = outward / to the left of lines)."""
    from shapely.geometry import Polygon
    from shapely.ops import linemerge, unary_union
    out = []
    loose = []
    for e in entities:
        if e.get("id") not in ids:
            continue
        t = e["type"]
        c = {"id": new_id(), "construction": e.get("construction", False)}
        if t == "rect":
            f = float(e.get("fillet") or 0)
            x0, x1 = sorted((float(e["x"]), float(e["x"]) + float(e["w"])))
            y0, y1 = sorted((float(e["y"]), float(e["y"]) + float(e["h"])))
            if x1 - x0 + 2 * d <= 0 or y1 - y0 + 2 * d <= 0:
                raise GeometryError("Offset is larger than the shape.")
            out.append(dict(e, **c, x=x0 - d, y=y0 - d, w=x1 - x0 + 2 * d, h=y1 - y0 + 2 * d,
                            fillet=max(f + d, 0) if f else 0))
        elif t == "circle":
            if e["r"] + d <= 0:
                raise GeometryError("Offset is larger than the circle.")
            out.append(dict(e, **c, r=float(e["r"]) + d))
        elif t == "arc":
            out.append(dict(e, **c, r=max(float(e["r"]) + d, 1e-6)))
        elif t == "polygon":
            out.append(dict(e, **c, r=float(e["r"]) + d / math.cos(math.pi / int(e.get("n", 6)))))
        elif t == "slot":
            out.append(dict(e, **c, r=float(e["r"]) + d))
        elif t == "polyline" and e.get("closed"):
            poly = Polygon(e["pts"]).buffer(d, join_style="mitre")
            for g in getattr(poly, "geoms", [poly]):
                out.append({**c, "type": "polyline", "pts": [list(p) for p in g.exterior.coords[:-1]], "closed": True})
        else:
            loose.append(e)
    if loose:
        merged = linemerge(unary_union([_geom(e) for e in loose if _geom(e) is not None]))
        for g in getattr(merged, "geoms", [merged]):
            if g.is_ring:
                poly = Polygon(g.coords)
                res = poly.buffer(d, join_style="mitre")
                for gg in getattr(res, "geoms", [res]):
                    out.append({"id": new_id(), "type": "polyline", "pts": [list(p) for p in gg.exterior.coords[:-1]], "closed": True})
            else:
                oc = g.offset_curve(d, join_style="mitre")
                for gg in getattr(oc, "geoms", [oc]):
                    pts = [list(p) for p in gg.coords]
                    if len(pts) >= 2:
                        out.append({"id": new_id(), "type": "polyline", "pts": pts, "closed": False})
    if not out:
        raise GeometryError("Select the shapes to offset first.")
    return entities + out


def mirror(entities: list[dict], ids: list, axis_id: str, new_id) -> list[dict]:
    """Mirrored copies of the chosen shapes across a sketch line."""
    p0, d = find_axis(entities, axis_id)
    d = d / np.linalg.norm(d)
    th = math.degrees(math.atan2(d[1], d[0]))

    def m(p):
        p = np.asarray(p, float) - p0
        return (p0 + 2 * np.dot(p, d) * d - p).tolist()

    out = []
    for e in entities:
        if e.get("id") not in ids or e.get("id") == axis_id:
            continue
        t, c = e["type"], {"id": new_id()}
        if t == "line":
            out.append(dict(e, **c, p1=m(e["p1"]), p2=m(e["p2"])))
        elif t == "polyline":
            out.append(dict(e, **c, pts=[m(p) for p in e["pts"]][::-1]))
        elif t in ("circle", "polygon"):
            cx, cy = m([e["cx"], e["cy"]])
            ne = dict(e, **c, cx=cx, cy=cy)
            if t == "polygon":
                ne["angle"] = 2 * th - float(e.get("angle", 0))
            out.append(ne)
        elif t == "arc":
            cx, cy = m([e["cx"], e["cy"]])
            out.append(dict(e, **c, cx=cx, cy=cy, a0=2 * th - float(e["a1"]), a1=2 * th - float(e["a0"])))
        elif t == "slot":
            (x1, y1), (x2, y2) = m([e["x1"], e["y1"]]), m([e["x2"], e["y2"]])
            out.append(dict(e, **c, x1=x1, y1=y1, x2=x2, y2=y2))
        elif t == "rect":
            cxy = m([float(e["x"]) + float(e["w"]) / 2, float(e["y"]) + float(e["h"]) / 2])
            w, h = abs(float(e["w"])), abs(float(e["h"]))
            out.append(dict(e, **c, x=cxy[0] - w / 2, y=cxy[1] - h / 2, w=w, h=h,
                            angle=2 * th - float(e.get("angle", 0))))
    if not out:
        raise GeometryError("Select the shapes to mirror (not the mirror line itself).")
    return entities + out


def find_axis(entities: list[dict], axis_id) -> tuple[np.ndarray, np.ndarray]:
    for e in entities:
        if e.get("id") == axis_id:
            if e["type"] == "polyline" and len(e["pts"]) >= 2:
                p0, p1 = np.asarray(e["pts"][0], float), np.asarray(e["pts"][1], float)
                return p0, p1 - p0
            if e["type"] == "line":
                p0, p1 = np.asarray(e["p1"], float), np.asarray(e["p2"], float)
                return p0, p1 - p0
    raise GeometryError("Pick a straight sketch line (or the sketch X/Y axis) as the revolve axis.")


def snap_points(entities: list[dict]) -> list[list[float]]:
    """Key points used for snapping in the sketcher."""
    pts = []
    for e in entities:
        t = e.get("type")
        if t == "rect":
            x, y, w, h = (float(e[k]) for k in ("x", "y", "w", "h"))
            pts += [[x, y], [x + w, y], [x + w, y + h], [x, y + h], [x + w / 2, y + h / 2]]
        elif t in ("circle", "polygon"):
            pts.append([float(e["cx"]), float(e["cy"])])
        elif t == "slot":
            pts += [[float(e["x1"]), float(e["y1"])], [float(e["x2"]), float(e["y2"])]]
        elif t == "polyline":
            pts += [list(map(float, p)) for p in e.get("pts", [])]
        elif t == "line":
            pts += [list(map(float, e["p1"])), list(map(float, e["p2"]))]
        elif t == "arc":
            a = _arc_points(e, 2)
            pts += [[float(e["cx"]), float(e["cy"])], a[0].tolist(), a[-1].tolist()]
    return pts
