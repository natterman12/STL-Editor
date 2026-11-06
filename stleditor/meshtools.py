"""v1.3 tools: project outlines into sketches, measure / resize round holes and pins,
offset (grow / shrink) bodies, and text outlines for sketches."""
from __future__ import annotations

import glob
import math
import os
from functools import lru_cache

import numpy as np
import manifold3d as m3

from . import geometry as G
from .geometry import GeometryError


# ------------------------------------------------------------------------------------------
# Project outline into a sketch
# ------------------------------------------------------------------------------------------
def _to_local(solid: m3.Manifold, frame) -> m3.Manifold:
    """Express a solid in sketch coordinates (u, v along the plane, w along its normal)."""
    o, x, y, n = (np.asarray(a, float) for a in frame)
    R = np.stack([x, y, n])            # rows: world -> local rotation
    t = -R @ o
    return solid.transform(np.column_stack([R, t]))


def outline(solids, frame, mode="section", tolerance=0.02, offset=0.0):
    """Closed 2D contours (sketch coordinates) of where `solids` meet the sketch plane
    ("section") or of their shadow on it ("silhouette")."""
    cs = None
    locs = [_to_local(s, frame) for s in solids]
    if mode == "section":
        # a sketch drawn on a face lies exactly on the surface: look just inside it then
        for h in (0.0, -0.01, 0.01):
            cs = None
            for loc in locs:
                c = loc.slice(h)
                cs = c if cs is None else cs + c
            if cs is not None and not cs.is_empty():
                break
    else:
        for loc in locs:
            c = loc.project()
            cs = c if cs is None else cs + c
    if cs is None or cs.is_empty():
        raise GeometryError("The sketch plane doesn't cut through the body here. "
                            "Try Silhouette, or sketch on a plane that crosses the part."
                            if mode == "section" else "Nothing to project.")
    if abs(offset) > 1e-9:
        cs = cs.offset(float(offset), m3.JoinType.Round, 2.0, 64)
    if tolerance > 0:
        cs = cs.simplify(float(tolerance))
    polys = [np.asarray(p, float) for p in cs.to_polygons() if len(p) >= 3]
    if not polys:
        raise GeometryError("The outline came out empty.")
    return polys


def ref_geometry(solids, frame, max_segs=3000):
    """Part edges a sketch can measure from: the outline of the faces lying on the sketch plane;
    otherwise where the plane cuts the parts; otherwise their shadow on it. Round loops come back
    as circles, everything else as straight segments (sketch coordinates)."""
    tris = []
    total = 0
    for s in solids:
        mm = _to_local(s, frame).to_mesh()
        vp = np.asarray(mm.vert_properties, np.float64)[:, :3]
        tv = np.asarray(mm.tri_verts, np.int64)
        total += len(tv)
        if not len(tv):
            continue
        t = vp[tv]                                    # (k, 3, 3)
        keep = np.all(np.abs(t[:, :, 2]) <= 1e-3, axis=1)
        uv = t[keep][:, :, :2]
        if not len(uv):
            continue
        a = uv[:, 1] - uv[:, 0]
        b = uv[:, 2] - uv[:, 0]
        ar = a[:, 0] * b[:, 1] - a[:, 1] * b[:, 0]
        uv = uv[np.abs(ar) >= 1e-12]
        ar = ar[np.abs(ar) >= 1e-12]
        uv[ar < 0] = uv[ar < 0][:, ::-1]
        tris.extend(np.ascontiguousarray(x) for x in uv)
    kind, polys = "face", []
    if tris:
        polys = [np.asarray(p, float) for p in m3.CrossSection(tris, m3.FillRule.NonZero).to_polygons()]
    if not polys:
        kind = "section"
        try:
            polys = outline(solids, frame, "section", 0.02)
        except GeometryError:
            polys = []
    if not polys and total <= 300000:     # a shadow of a huge scan would take too long
        kind = "silhouette"
        try:
            polys = outline(solids, frame, "silhouette", 0.02)
        except GeometryError:
            polys = []

    def ents(tol):
        ps = polys
        if tol:
            cs = m3.CrossSection([np.ascontiguousarray(p) for p in polys], m3.FillRule.EvenOdd).simplify(tol)
            ps = [np.asarray(p, float) for p in cs.to_polygons()]
        out = []
        for p in ps:
            if len(p) < 3:
                continue
            c = _as_circle(p)
            if c is not None:
                out.append({"type": "circle", "cx": c[0], "cy": c[1], "r": c[2]})
                continue
            for i in range(len(p)):
                q0, q1 = p[i], p[(i + 1) % len(p)]
                if math.hypot(q1[0] - q0[0], q1[1] - q0[1]) > 1e-6:
                    out.append({"type": "line", "p1": [round(float(q0[0]), 5), round(float(q0[1]), 5)],
                                "p2": [round(float(q1[0]), 5), round(float(q1[1]), 5)]})
        return out

    out = ents(0.005)
    for tol in (0.05, 0.2, 1.0):
        if len(out) <= max_segs:
            break
        out = ents(tol)
    return {"kind": kind if polys else None, "refs": out[:max_segs]}


def outline_entities(polys, new_id, construction=False):
    """Turn contours into sketch entities: circles where a contour is round, closed polylines otherwise."""
    out = []
    for p in polys:
        c = _as_circle(p)
        if c is not None:
            out.append({"id": new_id(), "type": "circle", "cx": c[0], "cy": c[1], "r": c[2]})
        else:
            out.append({"id": new_id(), "type": "polyline", "closed": True,
                        "pts": [[round(float(a), 4), round(float(b), 4)] for a, b in p]})
        if construction:
            out[-1]["construction"] = True
        out[-1]["projected"] = True
    return out


def _as_circle(p):
    if len(p) < 12:
        return None
    cx, cy, r = _fit_circle(p)
    if r <= 0:
        return None
    dev = np.abs(np.hypot(p[:, 0] - cx, p[:, 1] - cy) - r)
    if dev.max() < max(0.01, r * 0.01):
        return round(float(cx), 4), round(float(cy), 4), round(float(r), 4)
    return None


def _fit_circle(p):
    """Least-squares circle through 2D points (Kasa fit)."""
    x, y = p[:, 0], p[:, 1]
    A = np.column_stack([x, y, np.ones_like(x)])
    b = x * x + y * y
    sol, *_ = np.linalg.lstsq(A, b, rcond=None)
    cx, cy = sol[0] / 2, sol[1] / 2
    r2 = sol[2] + cx * cx + cy * cy
    return cx, cy, math.sqrt(r2) if r2 > 0 else -1.0


# ------------------------------------------------------------------------------------------
# Round holes and pins
# ------------------------------------------------------------------------------------------
def _tri_normals(v, f):
    t = v[f]
    n = np.cross(t[:, 1] - t[:, 0], t[:, 2] - t[:, 0])
    a = np.linalg.norm(n, axis=1)
    return np.divide(n, a[:, None], out=np.zeros_like(n), where=a[:, None] > 0), a / 2


def _adjacency(f):
    e = np.concatenate([f[:, [0, 1]], f[:, [1, 2]], f[:, [2, 0]]])
    e.sort(axis=1)
    owner = np.tile(np.arange(len(f)), 3)
    order = np.lexsort((e[:, 1], e[:, 0]))
    es, os_ = e[order], owner[order]
    same = np.all(es[1:] == es[:-1], axis=1)
    return os_[:-1][same], os_[1:][same]


def nearest_triangle(v, f, point):
    t = v[f]
    c = t.mean(axis=1)
    d = np.linalg.norm(c - np.asarray(point, float), axis=1)
    return int(np.argmin(d))


def fit_cylinder(v, f, tri, max_angle=35.0):
    """Recognise the round hole or pin around triangle `tri`.
    Returns dict(center, axis, radius, z0, z1, kind, tris) with the axis running z0..z1 from center."""
    v = np.asarray(v, float)
    f = np.asarray(f, np.int64)
    nrm, area = _tri_normals(v, f)
    a, b = _adjacency(f)
    cos_t = math.cos(math.radians(max_angle))
    ok = np.einsum("ij,ij->i", nrm[a], nrm[b]) >= cos_t
    from scipy.sparse import coo_matrix
    from scipy.sparse.csgraph import breadth_first_order
    g = coo_matrix((np.ones(int(ok.sum())), (a[ok], b[ok])), shape=(len(f), len(f))).tocsr()
    g = g + g.T
    region = breadth_first_order(g, int(tri), directed=False, return_predecessors=False)
    if len(region) < 6:
        raise GeometryError("Click the curved wall of a round hole or pin.")
    N = nrm[region]
    W = area[region]
    M = (N * W[:, None]).T @ N
    evals, evecs = np.linalg.eigh(M)
    axis = evecs[:, 0]
    if evals[0] > 0.08 * evals[1] or evals[1] < 0.25 * evals[2]:
        raise GeometryError("That surface isn't round like a hole or pin. Click the curved wall itself.")
    # an orthonormal frame around the axis
    ref = np.array([1.0, 0, 0]) if abs(axis[0]) < 0.9 else np.array([0, 1.0, 0])
    ex = G.unit(np.cross(axis, ref))
    ey = np.cross(axis, ex)
    pts = v[np.unique(f[region])]
    p2 = np.column_stack([pts @ ex, pts @ ey])
    cx, cy, r = _fit_circle(p2)
    if r <= 0:
        raise GeometryError("Couldn't measure that surface.")
    dev = np.abs(np.hypot(p2[:, 0] - cx, p2[:, 1] - cy) - r)
    if np.percentile(dev, 90) > max(0.05, 0.04 * r):
        raise GeometryError("That surface isn't a clean cylinder, so it can't be resized.")
    # the wall should go most of the way around (a hole) or be a clear arc
    ang = np.arctan2(p2[:, 1] - cy, p2[:, 0] - cx)
    cover = len(np.unique(np.floor((ang + math.pi) / (2 * math.pi) * 36))) / 36
    if cover < 0.5:
        raise GeometryError("Only part of a round surface was found. Click the wall of a full hole or pin.")
    zs = pts @ axis
    center = cx * ex + cy * ey
    # hole: normals point at the axis; pin: away from it
    tc = v[f[region]].mean(axis=1)
    radial = tc - np.outer(tc @ axis, axis) - center
    kind = "hole" if float(np.einsum("ij,ij->i", radial, N).mean()) < 0 else "pin"
    return {"center": center.tolist(), "axis": axis.tolist(), "radius": float(r),
            "z0": float(zs.min()), "z1": float(zs.max()), "kind": kind, "tris": region.tolist(),
            "cover": cover}


def _inside(solid: m3.Manifold, p, eps=0.05):
    probe = m3.Manifold.cube((eps, eps, eps), center=True).translate(tuple(map(float, p)))
    return (solid ^ probe).volume() > 0.5 * eps ** 3


def _tube(center, axis, z0, z1, r_in, r_out, segs=128):
    h = z1 - z0
    if h <= 1e-6 or r_out <= 1e-6:
        return m3.Manifold()
    t = m3.Manifold.cylinder(h, r_out, r_out, segs)
    if r_in > 1e-6:
        t = t - m3.Manifold.cylinder(h + 2, r_in, r_in, segs).translate((0, 0, -1))
    axis = G.unit(axis)
    ref = np.array([1.0, 0, 0]) if abs(axis[0]) < 0.9 else np.array([0, 1.0, 0])
    ex = G.unit(np.cross(axis, ref))
    ey = np.cross(axis, ex)
    o = np.asarray(center, float) + axis * z0
    return t.transform(G.frame_matrix(o, ex, ey, axis))


def resize_round(solid: m3.Manifold, cyl: dict, diameter: float | None, fill=False):
    """Change the diameter of a fitted hole or pin (or fill a hole). Keeps everything else."""
    c, ax = np.asarray(cyl["center"], float), np.asarray(cyl["axis"], float)
    r0 = cyl["radius"]
    z0, z1 = cyl["z0"], cyl["z1"]
    delta = max(0.02, r0 * 0.01)
    if fill:
        if cyl["kind"] != "hole":
            raise GeometryError("Fill works on holes. To remove a pin, cut it off with Split or Extrude › Cut.")
        return solid + _tube(c, ax, z0, z1, 0.0, r0 + delta)
    if not diameter or diameter <= 0:
        raise GeometryError("Enter the new diameter.")
    r1 = float(diameter) / 2
    if abs(r1 - r0) < 1e-4:
        return solid
    if cyl["kind"] == "hole":
        if r1 < r0:
            return solid + _tube(c, ax, z0, z1, r1, r0 + delta)
        # open ends get cut slightly past the surface; a blind hole keeps its bottom
        lo, hi = z0, z1
        if not _inside(solid, c + ax * (z0 - 0.3)):
            lo -= 0.5
        if not _inside(solid, c + ax * (z1 + 0.3)):
            hi += 0.5
        return solid - _tube(c, ax, lo, hi, max(r0 - delta, 0.0), r1)
    # pin
    if r1 < r0:
        hi = z1 + (0.5 if not _inside(solid, c + ax * (z1 + 0.3)) else 0)
        lo = z0 - (0.5 if not _inside(solid, c + ax * (z0 - 0.3)) else 0)
        return solid - _tube(c, ax, lo, hi, r1, r0 + delta + 0.5)
    return solid + _tube(c, ax, z0, z1, max(r0 - delta, 0.0), r1)


# ------------------------------------------------------------------------------------------
# Offset a whole body (grow / shrink), used by Offset and by Cut with clearance
# ------------------------------------------------------------------------------------------
def offset_solid(solid: m3.Manifold, distance: float, max_tris=12000) -> m3.Manifold:
    """Grow (distance > 0) or shrink (< 0) a solid by `distance` in every direction.
    Uses a Minkowski sum with a small sphere. Detailed meshes are simplified first, within at most
    40 % of the distance, and growing adds that back so the offset is never less than `distance`."""
    d = float(distance)
    if abs(d) < 1e-6:
        return solid
    work, slack = solid, 0.0
    if work.num_tri() > max_tris:
        for frac in (0.1, 0.2, 0.3, 0.4):
            slack = abs(d) * frac
            work = solid.simplify(slack)
            if work.num_tri() <= max_tris:
                break
    # a 32-triangle sphere sits inside the true sphere, so scale it up to keep the full distance
    segs = 8
    r = abs(d) / math.cos(math.pi / segs) + slack
    ball = m3.Manifold.sphere(r, segs)
    if d > 0:
        out = work.minkowski_sum(ball)
        return out + solid
    out = work.minkowski_difference(ball)
    if out.is_empty():
        raise GeometryError(f"Shrinking by {abs(d):g} mm leaves nothing of this body.")
    return out ^ solid


# ------------------------------------------------------------------------------------------
# Text
# ------------------------------------------------------------------------------------------
def font_dirs():
    dirs = []
    win = os.environ.get("WINDIR")
    if win:
        dirs.append(os.path.join(win, "Fonts"))
    la = os.environ.get("LOCALAPPDATA")
    if la:
        dirs.append(os.path.join(la, "Microsoft", "Windows", "Fonts"))
    dirs += ["/usr/share/fonts", "/usr/local/share/fonts", os.path.expanduser("~/.fonts"),
             "/System/Library/Fonts", "/Library/Fonts", os.path.expanduser("~/Library/Fonts")]
    return [d for d in dirs if os.path.isdir(d)]


@lru_cache(maxsize=1)
def list_fonts():
    """{family name: {style: path}} for TrueType / OpenType outline fonts on this computer."""
    from fontTools.ttLib import TTFont
    fams = {}
    for d in font_dirs():
        for path in glob.glob(os.path.join(d, "**", "*"), recursive=True):
            if not path.lower().endswith((".ttf", ".otf")):
                continue
            try:
                f = TTFont(path, lazy=True, fontNumber=0)
                name = f["name"]
                fam = name.getBestFamilyName() or os.path.splitext(os.path.basename(path))[0]
                sub = name.getBestSubFamilyName() or "Regular"
                if "glyf" not in f and "CFF " not in f:
                    continue
                fams.setdefault(fam, {}).setdefault(sub, path)
                f.close()
            except Exception:
                continue
    return dict(sorted(fams.items(), key=lambda kv: kv[0].lower()))


def default_font():
    fams = list_fonts()
    for want in ("Arial", "Segoe UI", "DejaVu Sans", "Liberation Sans", "Helvetica", "Noto Sans"):
        if want in fams:
            return want
    return next(iter(fams), None)


def _font_path(family, bold=False, italic=False):
    fams = list_fonts()
    if family not in fams:
        family = default_font()
        if family is None:
            raise GeometryError("No outline fonts were found on this computer.")
    styles = fams[family]
    want = ("Bold " if bold else "") + ("Italic" if italic else "")
    want = want.strip() or "Regular"
    for k in (want, want.replace("Italic", "Oblique"), "Regular", "Normal", "Book", "Roman"):
        if k in styles:
            return styles[k]
    return next(iter(styles.values()))


@lru_cache(maxsize=16)
def _load_font(path):
    from fontTools.ttLib import TTFont
    return TTFont(path, fontNumber=0)


class _FlatPen:
    """fontTools pen that flattens curves into polylines."""

    def __init__(self, glyphset, steps=8):
        self.glyphSet = glyphset
        self.steps = steps
        self.contours = []
        self.cur = []

    def moveTo(self, p):
        self.cur = [p]

    def lineTo(self, p):
        self.cur.append(p)

    def qCurveTo(self, *pts):
        # TrueType quadratic spline with implied on-curve points
        start = self.cur[-1]
        ctrl = list(pts[:-1])
        end = pts[-1]
        if end is None:  # closed contour of off-curve points only
            end = ((ctrl[-1][0] + ctrl[0][0]) / 2, (ctrl[-1][1] + ctrl[0][1]) / 2)
        segs = []
        for i, c in enumerate(ctrl):
            nxt = end if i == len(ctrl) - 1 else ((c[0] + ctrl[i + 1][0]) / 2, (c[1] + ctrl[i + 1][1]) / 2)
            segs.append((c, nxt))
        p0 = start
        for c, p2 in segs:
            for k in range(1, self.steps + 1):
                t = k / self.steps
                self.cur.append(((1 - t) ** 2 * p0[0] + 2 * (1 - t) * t * c[0] + t * t * p2[0],
                                 (1 - t) ** 2 * p0[1] + 2 * (1 - t) * t * c[1] + t * t * p2[1]))
            p0 = p2

    def curveTo(self, *pts):
        p0 = self.cur[-1]
        if len(pts) != 3:  # rare: treat extra points as a polyline
            for p in pts:
                self.cur.append(p)
            return
        c1, c2, p3 = pts
        for k in range(1, self.steps + 1):
            t = k / self.steps
            mt = 1 - t
            self.cur.append((mt ** 3 * p0[0] + 3 * mt * mt * t * c1[0] + 3 * mt * t * t * c2[0] + t ** 3 * p3[0],
                             mt ** 3 * p0[1] + 3 * mt * mt * t * c1[1] + 3 * mt * t * t * c2[1] + t ** 3 * p3[1]))

    def closePath(self):
        if len(self.cur) >= 3:
            self.contours.append(self.cur)
        self.cur = []

    endPath = closePath

    def addComponent(self, name, transform):
        g = self.glyphSet[name]
        sub = _FlatPen(self.glyphSet, self.steps)
        g.draw(sub)
        a, b, c, d, e, f = transform
        for cont in sub.contours:
            self.contours.append([(a * x + c * y + e, b * x + d * y + f) for x, y in cont])


def text_contours(text, family=None, size=10.0, bold=False, italic=False, spacing=0.0,
                  align="left", line_gap=1.2):
    """Outline contours of `text` (several lines allowed) in mm; the baseline of the first line is y = 0.
    Returns a list of (N, 2) arrays (even-odd fill)."""
    text = str(text or "")
    if not text.strip():
        return []
    font = _load_font(_font_path(family or default_font(), bold, italic))
    upm = font["head"].unitsPerEm
    cmap = font.getBestCmap() or {}
    gs = font.getGlyphSet()
    hmtx = font["hmtx"]
    scale = float(size) / upm   # size = em height in mm
    lines = text.split("\n")
    out = []
    widths = []
    per_line = []
    for line in lines:
        x = 0.0
        conts = []
        for ch in line:
            gname = cmap.get(ord(ch))
            if gname is None:
                gname = ".notdef" if ".notdef" in gs else None
            if gname is None:
                x += upm * 0.5
                continue
            pen = _FlatPen(gs)
            gs[gname].draw(pen)
            for c in pen.contours:
                conts.append(np.array([(px + x, py) for px, py in c], float))
            x += hmtx[gname][0] + spacing / scale
        widths.append(x - (spacing / scale if line else 0))
        per_line.append(conts)
    maxw = max(widths) if widths else 0
    for i, conts in enumerate(per_line):
        dx = {"left": 0.0, "center": (maxw - widths[i]) / 2, "right": maxw - widths[i]}.get(align, 0.0)
        dy = -i * upm * line_gap
        for c in conts:
            out.append((c + [dx, dy]) * scale)
    return out


def text_extent(contours):
    if not contours:
        return np.zeros(2), np.zeros(2)
    allp = np.concatenate(contours)
    return allp.min(0), allp.max(0)
