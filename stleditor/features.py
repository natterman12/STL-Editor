"""Feature evaluators. Each feature reads and updates an evaluation State."""
from __future__ import annotations

import math
from collections import OrderedDict

import numpy as np
import manifold3d as m3

from . import geometry as G
from .geometry import Body, GeometryError
from . import sketch as S
from . import printing as PR
from .feature_modules.registry import EVALUATORS, LABELS

OPS = ("new", "join", "cut", "intersect")


class State:
    def __init__(self):
        self.bodies: "OrderedDict[str, Body]" = OrderedDict()
        self.planes: dict[str, dict] = {}
        self.sketches: dict[str, dict] = {}
        # tool solid each feature used (extrude, revolve, hole, shapes), so mirror and pattern
        # can repeat the feature itself rather than a whole body
        self.tools: dict[str, dict] = {}

    def clone(self) -> "State":
        s = State()
        s.bodies = OrderedDict(self.bodies)
        s.planes = dict(self.planes)
        s.sketches = dict(self.sketches)
        s.tools = dict(self.tools)
        return s

    # -- lookups ----------------------------------------------------------------------------
    def body(self, bid) -> Body:
        if bid not in self.bodies:
            raise GeometryError(f"Body {bid!r} no longer exists.")
        return self.bodies[bid]

    def frame(self, ref):
        """Resolve a plane reference into an (o, x, y, n) frame."""
        if ref is None:
            ref = "XY"
        if isinstance(ref, str):
            if ref in G.BASE_PLANES:
                o, n, x = G.BASE_PLANES[ref]
                return G.plane_frame(o, n, x)
            if ref in self.planes:
                return self.planes[ref]["frame"]
            raise GeometryError(f"Plane {ref!r} no longer exists.")
        if isinstance(ref, dict):
            if "plane" in ref:
                return self.frame(ref["plane"])
            if "origin" in ref and "normal" in ref:
                fr = G.plane_frame(ref["origin"], ref["normal"], ref.get("xdir"))
                if ref.get("flip"):
                    o, x, y, n = fr
                    fr = (o, x, -y, -n)
                return fr
        raise GeometryError("Invalid plane reference.")


# ------------------------------------------------------------------------------------------
# Shared helpers
# ------------------------------------------------------------------------------------------
def _bbox(man: m3.Manifold):
    b = man.bounding_box()
    return np.array(b[:3]), np.array(b[3:])


def _overlaps(a, b, tol=1e-6):
    alo, ahi = a
    blo, bhi = b
    return bool(np.all(alo <= bhi + tol) and np.all(blo <= ahi + tol))


def apply_tool(state: State, fid: str, tool: m3.Manifold, operation: str, targets, name: str,
               result: dict, new_name: str | None = None):
    """Combine a tool solid with the design: new body / join / cut / intersect."""
    if tool.is_empty():
        raise GeometryError("The feature produced no geometry.")
    op = operation or "new"
    if op not in OPS:
        raise GeometryError(f"Unknown operation {op}.")
    state.tools[fid] = {"tool": tool, "op": op, "targets": list(targets or []), "name": name}
    if op == "new":
        bid = fid
        state.bodies[bid] = Body(new_name or name, solid=tool)
        result["created"].append(bid)
        return
    tb = _bbox(tool)
    if targets:
        tids = [t for t in targets if t in state.bodies]
        if not tids:
            raise GeometryError("The target body no longer exists.")
    else:
        tids = [k for k, b in state.bodies.items()
                if b.is_solid and _overlaps(_bbox(b.solid), tb)]
    if not tids:
        if op == "join":
            state.bodies[fid] = Body(new_name or name, solid=tool)
            result["created"].append(fid)
            result["warning"] = "Nothing to join to: made a new body."
            return
        raise GeometryError(f"Nothing to {op}: the tool doesn't touch any body.")
    if op == "join":
        first = tids[0]
        solids = [state.body(first).require_solid(), tool] + \
                 [state.body(t).require_solid() for t in tids[1:]]
        res = m3.Manifold.batch_boolean(solids, m3.OpType.Add)
        state.bodies[first] = state.body(first).copy(solid=res)
        for t in tids[1:]:
            del state.bodies[t]
        result["modified"].append(first)
        return
    changed = 0
    for t in tids:
        b = state.body(t)
        s = b.require_solid()
        if not _overlaps(_bbox(s), tb):
            if op == "intersect":
                del state.bodies[t]
            continue
        res = (s - tool) if op == "cut" else (s ^ tool)
        if res.is_empty():
            del state.bodies[t]
            result.setdefault("removed", []).append(t)
        else:
            state.bodies[t] = b.copy(solid=res)
            result["modified"].append(t)
        changed += 1
    if not changed:
        raise GeometryError(f"Nothing to {op}: the tool doesn't touch the target body.")


def _affine(R, t):
    return np.column_stack([R, t]).astype(np.float64)


def _place(man: m3.Manifold, frame, offset=(0, 0, 0)):
    o, x, y, n = frame
    off = np.asarray(offset, float)
    o2 = o + off[0] * x + off[1] * y + off[2] * n
    return man.transform(G.frame_matrix(o2, x, y, n))


def _rotation(rx, ry, rz):
    rx, ry, rz = (math.radians(float(a)) for a in (rx, ry, rz))
    Rx = np.array([[1, 0, 0], [0, math.cos(rx), -math.sin(rx)], [0, math.sin(rx), math.cos(rx)]])
    Ry = np.array([[math.cos(ry), 0, math.sin(ry)], [0, 1, 0], [-math.sin(ry), 0, math.cos(ry)]])
    Rz = np.array([[math.cos(rz), -math.sin(rz), 0], [math.sin(rz), math.cos(rz), 0], [0, 0, 1]])
    return Rz @ Ry @ Rx


def _transform_body(b: Body, M: np.ndarray) -> Body:
    if b.is_solid:
        return b.copy(solid=b.solid.transform(M))
    v, f = b.mesh()
    v2 = (np.asarray(v, np.float64) @ M[:, :3].T) + M[:, 3]
    if np.linalg.det(M[:, :3]) < 0:
        f = f[:, ::-1]
    return Body(b.name, verts=v2, faces=f)


def _float(p, k, default=None):
    v = p.get(k, default)
    if v is None or v == "":
        if default is None:
            raise GeometryError(f"Missing value: {k}.")
        return float(default)
    try:
        return float(v)
    except (TypeError, ValueError):
        raise GeometryError(f"{k} must be a number.")


# ------------------------------------------------------------------------------------------
# Features
# ------------------------------------------------------------------------------------------
def f_import(state, fid, p, res, ctx):
    parts = ctx["load_source"](p["source"])
    repair = p.get("repair", True)
    # "center": put the part on the print bed at the origin (bottom on Z=0, centred in X/Y).
    # "keep": the coordinates stored in the file. Older projects without the key keep them.
    place = p.get("place", "keep")
    shift = np.zeros(3)
    if place == "center":
        lo = np.min([v.min(0) for _, v, _ in parts], axis=0)
        hi = np.max([v.max(0) for _, v, _ in parts], axis=0)
        shift = np.array([-(lo[0] + hi[0]) / 2, -(lo[1] + hi[1]) / 2, -lo[2]])
        others = [b.mesh()[0] for b in state.bodies.values()]
        others = [o for o in others if len(o)]
        if others:
            # already something on the bed: put the new part next to it instead of inside it
            ohi = max(float(o[:, 0].max()) for o in others)
            shift[0] = ohi + 10.0 - lo[0]
        if np.linalg.norm(shift) > 1e-9:
            res["moved"] = shift.round(4).tolist()
    broken = 0
    for i, (nm, v, f) in enumerate(parts):
        v = v + shift
        bid = fid if len(parts) == 1 else f"{fid}.{i + 1}"
        name = p.get("name") or nm
        if len(parts) > 1:
            name = f"{name} {i + 1}" if p.get("name") else nm
        man = G.manifold_from_arrays(v, f)
        if man is None and repair:
            try:
                man = G.local_repair(v, f)
                if man is None:
                    rv, rf = G.quick_repair(v, f)
                    man = G.manifold_from_arrays(rv, rf)
                if man is not None:
                    res["warning"] = "The mesh had small defects (open or doubled edges); they were fixed automatically."
            except Exception:
                man = None
        if man is not None:
            man, merged = G.merge_overlapping_shells(man)
            if merged:
                res["warning"] = (f"{merged} overlapping shells were merged into one solid." +
                                  (" " + res["warning"] if res.get("warning") else ""))
            state.bodies[bid] = Body(name, solid=man)
        else:
            state.bodies[bid] = Body(name, verts=v, faces=f)
            broken += 1
        res["created"].append(bid)
    if broken:
        res["warning"] = (f"{broken} mesh(es) are not watertight. They display and export, but "
                          "editing needs Repair (Rebuild) first.")


def f_primitive(state, fid, p, res, ctx):
    shape = p.get("shape", "box")
    seg = int(p.get("segments", 96))
    anchor = p.get("anchor", "base")
    if shape == "box":
        L, W, H = _float(p, "length", 10), _float(p, "width", 10), _float(p, "height", 10)
        man = m3.Manifold.cube((L, W, H), center=False)
        shift = {"corner": (0, 0, 0), "base": (-L / 2, -W / 2, 0), "center": (-L / 2, -W / 2, -H / 2)}
        man = man.translate(shift.get(anchor, shift["base"]))
    elif shape == "cylinder":
        r, h = _float(p, "radius", 5), _float(p, "height", 10)
        man = m3.Manifold.cylinder(h, r, r, seg)
        if anchor == "center":
            man = man.translate((0, 0, -h / 2))
    elif shape == "cone":
        r1, r2, h = _float(p, "radius", 5), _float(p, "radius2", 0), _float(p, "height", 10)
        man = m3.Manifold.cylinder(h, r1, max(r2, 0.0), seg)
        if anchor == "center":
            man = man.translate((0, 0, -h / 2))
    elif shape == "sphere":
        r = _float(p, "radius", 5)
        man = m3.Manifold.sphere(r, seg)
        if anchor == "base":
            man = man.translate((0, 0, r))
    elif shape == "torus":
        R, r = _float(p, "radius", 10), _float(p, "radius2", 2)
        if r >= R:
            raise GeometryError("Tube radius must be smaller than the ring radius.")
        circ = m3.CrossSection.circle(r, max(24, seg // 2)).translate((R, 0))
        man = m3.Manifold.revolve(circ, seg)
        if anchor == "base":
            man = man.translate((0, 0, r))
    else:
        raise GeometryError(f"Unknown shape {shape}.")
    R = _rotation(*p.get("rotate", (0, 0, 0)))
    man = man.transform(_affine(R, np.zeros(3)))
    man = _place(man, state.frame(p.get("plane")), p.get("offset", (0, 0, 0)))
    apply_tool(state, fid, man, p.get("operation", "new"), p.get("targets"),
               p.get("body_name") or shape.capitalize(), res)


def f_plane(state, fid, p, res, ctx):
    mode = p.get("mode", "offset")
    if mode == "offset":
        o, x, y, n = state.frame(p.get("base"))
        d = _float(p, "distance", 0)
        fr = (o + n * d, x, y, n)
    elif mode == "angle":
        o, x, y, n = state.frame(p.get("base"))
        a = math.radians(_float(p, "angle", 45))
        axis = p.get("axis", "x")
        if axis == "x":  # rotate about the base plane's x axis
            n2 = n * math.cos(a) - y * math.sin(a)
            y2 = np.cross(n2, x)
            fr = (o, x, y2, n2)
        else:
            n2 = n * math.cos(a) + x * math.sin(a)
            x2 = np.cross(y, n2)
            fr = (o, x2, y, n2)
        if p.get("distance"):
            o2, x2, y2, n2 = fr
            fr = (o2 + n2 * float(p["distance"]), x2, y2, n2)
    elif mode == "three_points":
        pts = np.asarray(p.get("points", []), float)
        if pts.shape != (3, 3):
            raise GeometryError("Pick three points.")
        n = np.cross(pts[1] - pts[0], pts[2] - pts[0])
        if np.linalg.norm(n) < 1e-9:
            raise GeometryError("The three points are in a line.")
        fr = G.plane_frame(pts[0], n, pts[1] - pts[0])
    elif mode == "midplane":
        a = state.frame(p.get("base"))
        b = state.frame(p.get("base2"))
        mid_n = G.unit(a[3] + (b[3] if np.dot(a[3], b[3]) >= 0 else -b[3]))
        o = (a[0] + b[0]) / 2
        fr = G.plane_frame(o, mid_n, a[1])
    elif mode == "face":
        fr = state.frame(p.get("base"))
        if p.get("distance"):
            o, x, y, n = fr
            fr = (o + n * float(p["distance"]), x, y, n)
    else:
        raise GeometryError(f"Unknown plane mode {mode}.")
    if p.get("flip"):
        o, x, y, n = fr
        fr = (o, x, -y, -n)
    state.planes[fid] = {"name": p.get("name") or "Plane", "frame": tuple(np.asarray(a, float) for a in fr),
                         "size": float(p.get("size", 0) or 0)}
    res["created_plane"] = fid


def f_sketch(state, fid, p, res, ctx):
    fr = state.frame(p.get("plane"))
    state.sketches[fid] = {"name": p.get("name") or "Sketch", "frame": fr,
                           "entities": p.get("entities", []), "refs": p.get("refs", [])}
    res["created_sketch"] = fid


def _sketch(state, sid):
    if sid not in state.sketches:
        raise GeometryError("The sketch for this feature no longer exists.")
    return state.sketches[sid]


def f_extrude(state, fid, p, res, ctx):
    sk = _sketch(state, p.get("sketch"))
    cs = S.profile(sk["entities"], p.get("entities"), p.get("regions"))
    op = p.get("operation", "new")
    if p.get("through_all"):
        d1 = G.big_extent(list(state.bodies.values()))
    else:
        d1 = _float(p, "distance", 10)
    direction = p.get("direction", "one")
    d2 = _float(p, "distance2", 0) if direction == "two" else 0.0
    if direction == "two" and p.get("through_all"):
        d2 = d1
    if abs(d1) < 1e-9 and abs(d2) < 1e-9:
        raise GeometryError("Distance can't be zero.")
    if d1 < 0:
        direction = {"one": "reverse", "reverse": "one"}.get(direction, direction)
        d1 = -d1
    start = _float(p, "start_offset", 0)
    if direction == "one":
        z0, h = 0.0, d1
    elif direction == "reverse":
        z0, h = -d1, d1
    elif direction == "symmetric":
        z0, h = -d1 / 2, d1
    elif direction == "two":
        z0, h = -d2, d1 + d2
    else:
        raise GeometryError(f"Unknown direction {direction}.")
    taper = float(p.get("taper", 0) or 0)
    scale_top = (1.0, 1.0)
    if taper:
        # approximate taper as a uniform scale of the profile about its centroid
        lo = np.array(cs.bounds()[:2])
        hi = np.array(cs.bounds()[2:])
        c = (lo + hi) / 2
        half = float(max(hi - lo)) / 2
        k = max(0.0, (half - h * math.tan(math.radians(taper))) / half) if half > 0 else 1.0
        cs = cs.translate((-c[0], -c[1]))
        scale_top = (k, k)
        man = m3.Manifold.extrude(cs, h, 0, 0.0, scale_top).translate((c[0], c[1], z0 + start))
    else:
        man = m3.Manifold.extrude(cs, h).translate((0, 0, z0 + start))
    o, x, y, n = sk["frame"]
    M = G.frame_matrix(o, x, y, n)
    tool = man.transform(M)
    if op in ("cut", "intersect") and direction in ("one", "reverse") and p.get("auto_flip", True):
        # sketching on a face and cutting: go into the material, whichever side that is
        eps = max(tool.volume() * 1e-5, 1e-9)
        if _material_volume(state, tool, p.get("targets")) <= eps:
            flipped = man.mirror((0, 0, 1)).transform(M)
            if _material_volume(state, flipped, p.get("targets")) > eps:
                tool = flipped
                res["flipped"] = True
    apply_tool(state, fid, tool, op, p.get("targets"), p.get("body_name") or "Extrude", res)


def _material_volume(state, tool, targets):
    tb = _bbox(tool)
    vol = 0.0
    for k, b in state.bodies.items():
        if targets and k not in targets:
            continue
        if b.is_solid and _overlaps(_bbox(b.solid), tb):
            vol += (b.solid ^ tool).volume()
    return vol


def f_revolve(state, fid, p, res, ctx):
    sk = _sketch(state, p.get("sketch"))
    cs = S.profile(sk["entities"], p.get("entities"), p.get("regions"))
    axis = p.get("axis", "y")
    if axis == "x":
        p0, d = np.zeros(2), np.array([1.0, 0])
    elif axis == "y":
        p0, d = np.zeros(2), np.array([0, 1.0])
    else:
        p0, d = S.find_axis(sk["entities"], axis)
    e = d / np.linalg.norm(d)
    f = np.array([e[1], -e[0]])
    # 2D map: sketch (u,v) -> local (a,b): a along f (radial), b along e (axis)
    M2 = np.array([[f[0], f[1], -np.dot(f, p0)], [e[0], e[1], -np.dot(e, p0)]])
    local = cs.transform(M2)
    ang = _float(p, "angle", 360)
    seg = int(p.get("segments", 128))
    parts = []
    pos = local.translate((0, 0))
    # keep each side of the axis; mirror the negative side so it revolves too
    lo = np.array(local.bounds()[:2])
    hi = np.array(local.bounds()[2:])
    big = float(np.abs(np.concatenate([lo, hi])).max()) * 4 + 10
    right = m3.CrossSection.square((big, big * 2)).translate((0, -big))
    rp = pos ^ right
    lp = (pos - right).mirror((1, 0))
    for part in (rp, lp):
        if not part.is_empty() and part.area() > 1e-12:
            parts.append(m3.Manifold.revolve(part, seg, ang))
    if not parts:
        raise GeometryError("The profile is empty.")
    man = m3.Manifold.batch_boolean(parts, m3.OpType.Add) if len(parts) > 1 else parts[0]
    o, x, y, n = sk["frame"]
    F = f[0] * x + f[1] * y
    E = e[0] * x + e[1] * y
    Gd = np.cross(E, F)
    origin = o + p0[0] * x + p0[1] * y
    man = man.transform(np.column_stack([F, Gd, E, origin]))
    apply_tool(state, fid, man, p.get("operation", "new"), p.get("targets"),
               p.get("body_name") or "Revolve", res)


# ------------------------------------------------------------------------------------------
# Loft and sweep (version 1: one closed outline per profile, no holes; open paths only)
# ------------------------------------------------------------------------------------------
def _dedupe(pts, closed=False, tol=1e-9):
    out = []
    for p in pts:
        if not out or np.linalg.norm(p - out[-1]) > tol:
            out.append(p)
    if closed:
        while len(out) > 1 and np.linalg.norm(out[0] - out[-1]) <= tol:
            out.pop()
    return np.array(out, float)


def _ccw(pts):
    x, y = pts[:, 0], pts[:, 1]
    if 0.5 * np.sum(x * np.roll(y, -1) - np.roll(x, -1) * y) < 0:
        pts = pts[::-1]
    return np.ascontiguousarray(pts, dtype=np.float64)


def _profile_loop(sk, idx):
    """The one closed outline of a sketch, CCW in sketch coordinates (Nx2)."""
    try:
        cs = S.profile(sk["entities"])
    except GeometryError:
        raise GeometryError(f"Profile {idx} isn't a closed shape with an area. Close its outline first.")
    polys = cs.to_polygons()
    if len(polys) != 1:
        raise GeometryError(f"Profile {idx} has a hole or more than one shape. Loft and sweep profiles "
                            "must be one closed outline in this version.")
    return _ccw(_dedupe(np.asarray(polys[0], float), closed=True))


def _resample(pts, n):
    """n points spaced evenly by length around a closed loop, starting at pts[0]."""
    closed = np.vstack([pts, pts[:1]])
    seg = np.linalg.norm(np.diff(closed, axis=0), axis=1)
    cum = np.concatenate([[0.0], np.cumsum(seg)])
    out = np.empty((n, 2))
    for k in range(n):
        s = k * cum[-1] / n
        i = min(int(np.searchsorted(cum, s, side="right")) - 1, len(seg) - 1)
        f = (s - cum[i]) / seg[i]
        out[k] = closed[i] + f * (closed[i + 1] - closed[i])
    return out


def _align(prev, cur):
    """Rotate the start of loop `cur` so its points line up best with loop `prev`."""
    n = len(cur)
    best, bs = None, 0
    for s in range(n):
        d = float(np.sum((prev - np.roll(cur, -s, axis=0)) ** 2))
        if best is None or d < best - 1e-12:
            best, bs = d, s
    return np.roll(cur, -bs, axis=0)


def _mesh_solid(verts, tris, what):
    try:
        man = m3.Manifold(m3.Mesh(vert_properties=np.asarray(verts, np.float32),
                                  tri_verts=np.asarray(tris, np.uint32)))
    except Exception:
        man = None
    if man is None or man.is_empty() or man.volume() <= 1e-9:
        raise GeometryError(f"The {what} couldn't be closed into a solid. Check that the profiles or path "
                            "don't cross or fold back on themselves.")
    return man


def f_loft(state, fid, p, res, ctx):
    ids = [s for s in (p.get("profiles") or []) if s]
    if len(ids) < 2:
        raise GeometryError("A loft needs at least two sketch profiles, at different heights.")
    sks = [_sketch(state, sid) for sid in ids]
    lines = [_profile_loop(sk, i + 1) for i, sk in enumerate(sks)]
    N = max(len(l) for l in lines)
    n_ref = np.asarray(sks[0]["frame"][3], float)
    loops, heights = [], []
    for i, (sk, lp) in enumerate(zip(sks, lines)):
        o, x, y, n = (np.asarray(v, float) for v in sk["frame"])
        if abs(float(n @ n_ref)) < 1 - 1e-6:
            raise GeometryError("The loft profiles must lie on parallel planes (sketch them on the same "
                                "plane or on parallel planes).")
        pts = _resample(lp, N)
        if float(n @ n_ref) < 0:
            pts = pts[::-1]          # keep every loop turning the same way round the loft direction
        P3 = o + pts[:, :1] * x + pts[:, 1:2] * y
        if loops:
            P3 = _align(loops[-1], P3)
        loops.append(P3)
        heights.append(float(o @ n_ref))
    d = np.diff(heights)
    if np.any(np.abs(d) < 1e-6) or not (np.all(d > 0) or np.all(d < 0)):
        raise GeometryError("The loft profiles must be at different heights, one after another in a single "
                            "direction (in the order you picked them).")
    s = 1.0 if d[0] > 0 else -1.0
    J = len(loops)
    verts = np.vstack(loops)
    tris = []
    for j in range(J - 1):
        lo, up = (j, j + 1) if s > 0 else (j + 1, j)
        for k in range(N):
            k2 = (k + 1) % N
            a, b, c, e = lo * N + k, lo * N + k2, up * N + k2, up * N + k
            tris.append((a, b, c))
            tris.append((a, c, e))
    e1 = np.cross(n_ref, [0.0, 0.0, 1.0])
    if np.linalg.norm(e1) < 1e-6:
        e1 = np.cross(n_ref, [1.0, 0.0, 0.0])
    e1 = e1 / np.linalg.norm(e1)
    e2 = np.cross(n_ref, e1)
    for j, keep in ((0, s < 0), (J - 1, s > 0)):
        # the end caps: triangles wound to face away from the solid
        p2 = np.column_stack([loops[j] @ e1, loops[j] @ e2])
        base = j * N
        for a, b, c in m3.triangulate([p2]):
            tris.append((base + a, base + b, base + c) if keep else (base + a, base + c, base + b))
    man = _mesh_solid(verts, tris, "loft")
    apply_tool(state, fid, man, p.get("operation", "new"), p.get("targets"), p.get("body_name") or "Loft", res)


def _path_points(sk):
    """The sketch's open path (lines, arcs, open polylines joined end to end) in sketch coordinates."""
    items = []
    for e in sk["entities"]:
        if e.get("construction") or e.get("type") not in ("line", "arc", "polyline"):
            continue
        pts, closed = S.entity_path(e)
        if not closed and len(pts) >= 2:
            items.append(np.asarray(pts, float))
    if not items:
        raise GeometryError("The sweep path needs one open path of lines and arcs in its sketch.")
    chain = items.pop(0)
    tol = 1e-6
    while items:
        for idx, it in enumerate(items):
            if np.linalg.norm(it[0] - chain[-1]) <= tol:
                chain = np.vstack([chain, it[1:]])
            elif np.linalg.norm(it[-1] - chain[-1]) <= tol:
                chain = np.vstack([chain, it[::-1][1:]])
            elif np.linalg.norm(it[-1] - chain[0]) <= tol:
                chain = np.vstack([it, chain[1:]])
            elif np.linalg.norm(it[0] - chain[0]) <= tol:
                chain = np.vstack([it[::-1], chain[1:]])
            else:
                continue
            items.pop(idx)
            break
        else:
            raise GeometryError("The sweep path must be one connected chain of lines and arcs.")
    chain = _dedupe(chain)
    if len(chain) < 2:
        raise GeometryError("The sweep path is too short.")
    if np.linalg.norm(chain[0] - chain[-1]) <= tol:
        raise GeometryError("The sweep path must be open in this version (its two ends can't meet).")
    return chain


def _cross2(u, v):
    return u[..., 0] * v[..., 1] - u[..., 1] * v[..., 0]


def _path_self_crosses(P):
    A, B = P[:-1], P[1:]
    eps = 1e-9 * max(1.0, float(np.ptp(P, axis=0).max()))

    def sg(x):
        return np.where(np.abs(x) <= eps, 0.0, np.sign(x))
    for i in range(len(A)):
        j = np.arange(i + 2, len(A))
        if j.size == 0:
            break
        C, D = A[j], B[j]
        d1 = sg(_cross2(B[i] - A[i], C - A[i]))
        d2 = sg(_cross2(B[i] - A[i], D - A[i]))
        d3 = sg(_cross2(D - C, A[i] - C))
        d4 = sg(_cross2(D - C, B[i] - C))
        lo1, hi1 = np.minimum(A[i], B[i]), np.maximum(A[i], B[i])
        lo2, hi2 = np.minimum(C, D), np.maximum(C, D)
        boxes = np.all(np.maximum(lo1, lo2) <= np.minimum(hi1, hi2) + eps, axis=1)
        hit = np.where((d1 == 0) & (d2 == 0), boxes, (d1 * d2 <= 0) & (d3 * d4 <= 0))
        if np.any(hit):
            return True
    return False


def _rot_between(a, b):
    """Smallest rotation taking unit vector a onto unit vector b (3x3)."""
    c = float(a @ b)
    if c < -1 + 1e-12:
        w = np.cross(a, [1.0, 0.0, 0.0])
        if np.linalg.norm(w) < 1e-6:
            w = np.cross(a, [0.0, 1.0, 0.0])
        w = w / np.linalg.norm(w)
        return 2 * np.outer(w, w) - np.eye(3)
    v = np.cross(a, b)
    K = np.array([[0, -v[2], v[1]], [v[2], 0, -v[0]], [-v[1], v[0], 0]])
    return np.eye(3) + K + K @ K / (1 + c)


def f_sweep(state, fid, p, res, ctx):
    psk = _sketch(state, p.get("profile"))
    lp = _profile_loop(psk, 1)
    K = len(lp)
    po, px, py, _ = (np.asarray(v, float) for v in _sketch(state, p.get("path"))["frame"])
    P2 = _path_points(_sketch(state, p.get("path")))
    if _path_self_crosses(P2):
        raise GeometryError("The sweep path crosses itself. Move the path so it doesn't cross.")
    P3 = po + P2[:, :1] * px + P2[:, 1:2] * py
    m = len(P3)
    dirs = np.diff(P3, axis=0)
    dirs = dirs / np.linalg.norm(dirs, axis=1)[:, None]
    T = np.empty((m, 3))
    T[0], T[-1] = dirs[0], dirs[-1]
    for i in range(1, m - 1):
        sm = dirs[i - 1] + dirs[i]
        T[i] = sm / np.linalg.norm(sm) if np.linalg.norm(sm) > 1e-9 else dirs[i]
    # the profile starts at the path's first point, turned to face along the path
    o_s, x_s, y_s, n_s = (np.asarray(v, float) for v in psk["frame"])
    # the path's first point lands on the profile point under it, so a profile drawn around the
    # path start stays where it was drawn relative to the path
    shift = np.array([float((P3[0] - o_s) @ x_s), float((P3[0] - o_s) @ y_s)])
    lp = lp - shift
    A = _rot_between(n_s, T[0])
    R = np.zeros((m, 3))
    R[0] = A @ x_s
    # rotation-minimising frames along the path (double reflection)
    for i in range(m - 1):
        v1 = P3[i + 1] - P3[i]
        c1 = float(v1 @ v1)
        rL = R[i] - 2 / c1 * (v1 @ R[i]) * v1
        tL = T[i] - 2 / c1 * (v1 @ T[i]) * v1
        v2 = T[i + 1] - tL
        c2 = float(v2 @ v2)
        rn = rL - 2 / c2 * (v2 @ rL) * v2 if c2 > 1e-18 else rL
        rn = rn - (rn @ T[i + 1]) * T[i + 1]
        R[i + 1] = rn / np.linalg.norm(rn)
    Sv = np.cross(T, R)
    verts = np.empty((m * K, 3))
    for i in range(m):
        Q = lp[:, :1] * R[i] + lp[:, 1:2] * Sv[i]
        if 0 < i < m - 1:
            # at a bend the profile is stretched across the bend so the corner is mitred, not cut
            c = float(dirs[i - 1] @ dirs[i])
            if c < -0.99:
                raise GeometryError("The sweep path turns back on itself too sharply. Open up the bend.")
            wv = dirs[i] - dirs[i - 1]
            if np.linalg.norm(wv) > 1e-9:
                wv = wv / np.linalg.norm(wv)
                Q = Q + (1 / math.sqrt((1 + c) / 2) - 1) * (Q @ wv)[:, None] * wv
        verts[i * K:(i + 1) * K] = P3[i] + Q
    tris = []
    for i in range(m - 1):
        for k in range(K):
            k2 = (k + 1) % K
            a, b, c, e = i * K + k, i * K + k2, (i + 1) * K + k2, (i + 1) * K + k
            tris.append((a, b, c))
            tris.append((a, c, e))
    for i, keep in ((0, False), (m - 1, True)):
        base = i * K
        for a, b, c in m3.triangulate([lp]):
            tris.append((base + a, base + b, base + c) if keep else (base + a, base + c, base + b))
    man = _mesh_solid(verts, tris, "sweep")
    apply_tool(state, fid, man, p.get("operation", "new"), p.get("targets"), p.get("body_name") or "Sweep", res)


def f_hole(state, fid, p, res, ctx):
    pts = p.get("points") or ([p["point"]] if p.get("point") else [])
    if not pts:
        raise GeometryError("Click on a face to place the hole.")
    n = G.unit(p.get("normal", (0, 0, 1)))
    d = _float(p, "diameter", 5)
    r = d / 2
    targets = p.get("targets")
    tbodies = [state.bodies[t] for t in targets if t in state.bodies] if targets else \
        [b for b in state.bodies.values() if b.is_solid]
    if p.get("through", True) or not p.get("depth"):
        depth = G.big_extent(tbodies)
    else:
        depth = _float(p, "depth", 10)
    kind = p.get("kind", "simple")
    eps = 0.01
    tools = []
    for pt in pts:
        pt = np.asarray(pt, float)
        top = pt + n * eps
        parts = [G.make_cylinder(top, -n, depth + eps, r)]
        if not p.get("through", True) and p.get("depth") and p.get("tip", True):
            tip_h = r / math.tan(math.radians(118 / 2))
            parts.append(G.make_cylinder(pt - n * depth, -n, tip_h, r, 0.001))
        if kind == "counterbore":
            cbd = _float(p, "cb_diameter", d * 1.8)
            cbh = _float(p, "cb_depth", d * 0.6)
            parts.append(G.make_cylinder(pt + n * 1.0, -n, cbh + 1.0, cbd / 2))
        elif kind == "hex":
            af = _float(p, "hex_af", d * 1.8)          # across flats, tolerance included
            hd = _float(p, "hex_depth", 3)
            rr = af / math.sqrt(3)                       # circumradius of the hexagon
            parts.append(G.make_cylinder(pt + n * 1.0, -n, hd + 1.0, rr, rr, 6))
        elif kind == "countersink":
            csd = _float(p, "cs_diameter", d * 2)
            ang = _float(p, "cs_angle", 90)
            h = (csd - d) / 2 / math.tan(math.radians(ang / 2))
            # cone from csd at the surface (plus a little above) to d at depth h
            extra = 1.0
            r_top = csd / 2 + extra * math.tan(math.radians(ang / 2))
            parts.append(G.make_cylinder(pt + n * extra, -n, h + extra, r_top, r))
        tools.extend(parts)
    tool = m3.Manifold.batch_boolean(tools, m3.OpType.Add) if len(tools) > 1 else tools[0]
    apply_tool(state, fid, tool, "cut", targets, "Hole", res)


def f_combine(state, fid, p, res, ctx):
    tgt = p.get("target")
    tools = [t for t in p.get("tools", []) if t != tgt]
    if not tgt or not tools:
        raise GeometryError("Choose a target body and at least one tool body.")
    b = state.body(tgt)
    s = b.require_solid()
    ts = [state.body(t).require_solid() for t in tools]
    op = p.get("operation", "join")
    if op == "join":
        out = m3.Manifold.batch_boolean([s] + ts, m3.OpType.Add)
    elif op == "cut":
        clearance = float(p.get("clearance") or 0)
        if clearance > 0:
            from . import meshtools as MT
            ts = [MT.offset_solid(t, clearance) for t in ts]
        out = s - m3.Manifold.batch_boolean(ts, m3.OpType.Add) if len(ts) > 1 else s - ts[0]
    elif op == "intersect":
        out = s
        for t in ts:
            out = out ^ t
    else:
        raise GeometryError(f"Unknown operation {op}.")
    if out.is_empty():
        raise GeometryError("The result is empty.")
    state.bodies[tgt] = b.copy(solid=out)
    res["modified"].append(tgt)
    if not p.get("keep_tools"):
        for t in tools:
            state.bodies.pop(t, None)


def f_split(state, fid, p, res, ctx):
    bid = p.get("body")
    b = state.body(bid)
    s = b.require_solid()
    o, x, y, n = state.frame(p.get("plane"))
    conn = p.get("connector", "none") if p.get("keep", "both") == "both" else "none"
    above, below, extras, info = PR.split_with_connectors(
        s, o, n, conn, p.get("pin_d"), p.get("pin_len"), p.get("clearance"))
    if conn != "none" and info["skipped"]:
        res["warning"] = "The cut face is too thin for connectors here; split without them."
    elif conn != "none":
        res["warning"] = None
        res["connectors"] = info["connectors"]
    for i, ex in enumerate(extras, 1):
        state.bodies[f"{fid}.pin{i}"] = Body(f"Pin {i}", solid=ex)
        res["created"].append(f"{fid}.pin{i}")
    keep = p.get("keep", "both")
    if keep == "above":
        below = m3.Manifold()
    elif keep == "below":
        above = m3.Manifold()
    if above.is_empty() and below.is_empty():
        raise GeometryError("Nothing left after the split.")
    if (above.is_empty() or below.is_empty()) and keep == "both":
        raise GeometryError("The plane doesn't cut through this body.")
    first, second = (above, below) if not above.is_empty() else (below, m3.Manifold())
    state.bodies[bid] = b.copy(name=b.name if second.is_empty() else f"{b.name} (1)", solid=first)
    res["modified"].append(bid)
    if not second.is_empty():
        nid = f"{fid}.2"
        state.bodies[nid] = Body(f"{b.name} (2)", solid=second)
        res["created"].append(nid)


def _mirror_matrix(frame):
    o, x, y, n = frame
    n = G.unit(n)
    R = np.eye(3) - 2 * np.outer(n, n)
    t = 2 * np.dot(o, n) * n
    return _affine(R, t)


def _repeat_features(state, fid, feature_ids, transforms, res, verb):
    """Apply copies of earlier features' tools, each moved by one of `transforms`."""
    if not feature_ids:
        raise GeometryError(f"Choose the feature(s) to {verb}.")
    done, failed = 0, []
    k = 0
    for src in feature_ids:
        rec = state.tools.get(src)
        if rec is None:
            raise GeometryError(f"A chosen feature can't be {verb}ed (it isn't an extrude, revolve, "
                                "hole or shape that comes earlier in the timeline, or it failed).")
        for M in transforms:
            k += 1
            sub = {"created": [], "modified": []}
            try:
                apply_tool(state, f"{fid}.{k}", rec["tool"].transform(M), rec["op"],
                           [t for t in rec["targets"] if t in state.bodies] or None,
                           rec["name"], sub)
                res["created"] += sub["created"]
                res["modified"] += [m for m in sub["modified"] if m not in res["modified"]]
                res.setdefault("removed", []).extend(sub.get("removed", []))
                done += 1
            except GeometryError as e:
                failed.append(str(e))
            state.tools.pop(f"{fid}.{k}", None)   # a copy's tool isn't a feature to repeat again
    if not done:
        raise GeometryError(failed[0] if failed else "Nothing changed.")
    if failed:
        res["warning"] = f"{len(failed)} cop{'y' if len(failed) == 1 else 'ies'} didn't touch the part and were skipped."


def f_mirror(state, fid, p, res, ctx):
    M = _mirror_matrix(state.frame(p.get("plane")))
    if p.get("kind") == "features":
        return _repeat_features(state, fid, p.get("features") or [], [M], res, "mirror")
    bids = p.get("bodies") or []
    if not bids:
        raise GeometryError("Choose a body to mirror.")
    mode = p.get("mode", "join")
    for i, bid in enumerate(bids):
        b = state.body(bid)
        mb = _transform_body(b, M)
        if mode == "replace":
            state.bodies[bid] = mb
            res["modified"].append(bid)
        elif mode == "join":
            s = b.require_solid()
            state.bodies[bid] = b.copy(solid=s + mb.require_solid())
            res["modified"].append(bid)
        else:
            nid = fid if len(bids) == 1 else f"{fid}.{i + 1}"
            mb.name = f"{b.name} (mirror)"
            state.bodies[nid] = mb
            res["created"].append(nid)


def _pivot(b: Body, pivot):
    if pivot == "origin":
        return np.zeros(3)
    v, _ = b.mesh()
    if pivot == "base":
        lo, hi = v.min(0), v.max(0)
        return np.array([(lo[0] + hi[0]) / 2, (lo[1] + hi[1]) / 2, lo[2]])
    return (v.min(0) + v.max(0)) / 2.0


def f_move(state, fid, p, res, ctx):
    bids = p.get("bodies") or []
    if not bids:
        raise GeometryError("Choose a body to move.")
    R = _rotation(*p.get("rotate", (0, 0, 0)))
    t = np.asarray(p.get("translate", (0, 0, 0)), float)
    for i, bid in enumerate(bids):
        b = state.body(bid)
        c = _pivot(b, p.get("pivot", "center"))
        M = _affine(R, c - R @ c + t)
        nb = _transform_body(b, M)
        # Land on bed: a turned part rests on the bed (Move Z then lifts it off), so it never
        # hangs through the bed after a rotation
        if p.get("bed") and any(abs(float(a)) > 1e-9 for a in p.get("rotate", (0, 0, 0))):
            dz = t[2] - float(nb.mesh()[0][:, 2].min())
            if abs(dz) > 1e-9:
                nb = _transform_body(nb, _affine(np.eye(3), np.array([0.0, 0.0, dz])))
        if p.get("copy"):
            nid = fid if len(bids) == 1 else f"{fid}.{i + 1}"
            nb.name = f"{b.name} (copy)"
            state.bodies[nid] = nb
            res["created"].append(nid)
        else:
            state.bodies[bid] = nb
            res["modified"].append(bid)


def f_scale(state, fid, p, res, ctx):
    bids = p.get("bodies") or []
    if not bids:
        raise GeometryError("Choose a body to scale.")
    for bid in bids:
        b = state.body(bid)
        v, _ = b.mesh()
        size = v.max(0) - v.min(0)
        mode = p.get("mode", "factor")
        if mode == "factor":
            k = p.get("factor", 1.0)
            k = np.asarray(k if isinstance(k, (list, tuple)) else [k, k, k], float)
        elif mode == "fit":
            target = np.asarray(p.get("size", size.tolist()), float)
            k = np.divide(target, size, out=np.ones(3), where=size > 1e-9)
            if p.get("uniform", True):
                axis = int(p.get("axis", int(np.argmax(np.abs(target - size)))))
                k = np.full(3, k[axis])
        elif mode == "units":
            k = np.full(3, {"in_to_mm": 25.4, "mm_to_in": 1 / 25.4, "cm_to_mm": 10.0,
                            "m_to_mm": 1000.0}.get(p.get("units", "in_to_mm"), 1.0))
        else:
            raise GeometryError(f"Unknown scale mode {mode}.")
        if np.any(k <= 0):
            raise GeometryError("Scale factors must be positive.")
        c = _pivot(b, p.get("pivot", "base"))
        R = np.diag(k)
        state.bodies[bid] = _transform_body(b, _affine(R, c - R @ c))
        res["modified"].append(bid)


def _hollow_pitch(b, t, q):
    v, _ = b.mesh()
    diag = float(np.linalg.norm(v.max(0) - v.min(0)))
    div = {"draft": 80, "normal": 150, "fine": 260}.get(q or "normal", 150)
    return max(min(t / 2.5, diag / div), diag / (div * 2.5))


def f_hollow(state, fid, p, res, ctx):
    bid = p.get("body")
    b = state.body(bid)
    s = b.require_solid()
    t = _float(p, "thickness", 2)
    out = G.hollow(s, t, _hollow_pitch(b, t, p.get("resolution")))
    state.bodies[bid] = b.copy(solid=out)
    res["modified"].append(bid)


def f_shell(state, fid, p, res, ctx):
    """Hollow the body, then open each picked flat face: the hollow space just under the wall
    (kept inside the face) is pushed out through it. Two passes, so faces that meet at an edge
    also take the wall strip along that edge. No faces: a closed hollow, like Hollow."""
    bid = p.get("body")
    if not bid:
        raise GeometryError("Choose a body to shell.")
    b = state.body(bid)
    s = b.require_solid()
    t = _float(p, "thickness", 2)
    if t <= 0:
        raise GeometryError("Wall thickness must be positive.")
    pitch = _hollow_pitch(b, t, p.get("resolution"))
    cavity = G.hollow(s, t, pitch, cavity_only=True)
    depth = t + 2 * pitch
    picked = []
    for i, face in enumerate([f for f in (p.get("faces") or []) if f], 1):
        if not face.get("contours"):
            raise GeometryError(f"Face {i} isn't flat. Shell can only open flat faces.")
        o, x, y, n = G.plane_frame(face["origin"], face["normal"], face.get("xdir"))
        R = np.vstack([x, y, n])
        outline = m3.CrossSection([np.ascontiguousarray(np.asarray(c, np.float64)) for c in face["contours"]],
                                  m3.FillRule.EvenOdd)
        picked.append((i, (o, x, y, n), np.column_stack([R, -R @ o]), outline))

    def openings(space):
        tools = []
        for i, fr, to_local, outline in picked:
            hole = space.transform(to_local).slice(-depth) ^ outline
            if hole.is_empty() or hole.area() < 1e-6:
                raise GeometryError(f"There's no hollow space behind face {i} to open. Try a thinner wall.")
            tool = m3.Manifold.extrude(hole, depth + pitch).translate((0, 0, -depth))
            tools.append(tool.transform(G.frame_matrix(*fr)))
        return tools

    cut = cavity
    if picked:
        first = openings(cavity)
        cut = m3.Manifold.batch_boolean([cavity] + openings(m3.Manifold.batch_boolean([cavity] + first, m3.OpType.Add)),
                                        m3.OpType.Add)
    state.bodies[bid] = b.copy(solid=s - cut)
    res["modified"].append(bid)


def f_repair(state, fid, p, res, ctx):
    bid = p.get("body")
    b = state.body(bid)
    v, f = b.mesh()
    mode = p.get("mode", "auto")
    man = None
    if mode in ("auto", "quick"):
        man = G.local_repair(v, f)
        if man is None:
            rv, rf = G.quick_repair(v, f)
            man = G.manifold_from_arrays(rv, rf)
        if man is None and mode == "quick":
            raise GeometryError("Quick repair wasn't enough. Use Rebuild.")
    if man is None:
        pitch = p.get("voxel")
        pitch = float(pitch) if pitch else G.auto_pitch(v, {"draft": 120, "normal": 220,
                                                             "fine": 360}.get(p.get("resolution"), 220))
        man = G.voxel_rebuild(v, f, pitch)
        res["warning"] = f"Rebuilt as a watertight solid on a {pitch:.3g} mm grid."
    state.bodies[bid] = Body(b.name, solid=man)
    res["modified"].append(bid)


def f_simplify(state, fid, p, res, ctx):
    bid = p.get("body")
    b = state.body(bid)
    ratio = p.get("ratio")
    out = G.simplify(b, target_ratio=float(ratio) if ratio else None,
                     tolerance=p.get("tolerance"))
    if isinstance(out, tuple):
        state.bodies[bid] = Body(b.name, verts=out[0], faces=out[1])
        res["warning"] = "Simplified mesh isn't watertight; run Repair to edit it."
    else:
        state.bodies[bid] = b.copy(solid=out)
    res["modified"].append(bid)


def _pattern_transforms(p):
    count = int(p.get("count", 3))
    if count < 2:
        raise GeometryError("Count must be at least 2.")
    out = []
    for i in range(1, count):
        if p.get("kind", "linear") == "linear":
            d = G.unit(p.get("direction", (1, 0, 0))) * _float(p, "spacing", 10) * i
            out.append(_affine(np.eye(3), d))
        else:
            axis = G.unit(p.get("axis", (0, 0, 1)))
            c = np.asarray(p.get("center", (0, 0, 0)), float)
            total = _float(p, "angle", 360)
            step = total / count if abs(total) >= 360 else total / (count - 1)
            a = math.radians(step * i)
            K = np.array([[0, -axis[2], axis[1]], [axis[2], 0, -axis[0]], [-axis[1], axis[0], 0]])
            R = np.eye(3) + math.sin(a) * K + (1 - math.cos(a)) * K @ K
            out.append(_affine(R, c - R @ c))
    return out


def f_pattern(state, fid, p, res, ctx):
    if p.get("source") == "features":
        return _repeat_features(state, fid, p.get("features") or [], _pattern_transforms(p), res,
                                "pattern")
    bids = p.get("bodies") or []
    if not bids:
        raise GeometryError("Choose a body to pattern.")
    count = int(p.get("count", 3))
    if count < 2:
        raise GeometryError("Count must be at least 2.")
    kind = p.get("kind", "linear")
    for bid in bids:
        b = state.body(bid)
        s = b.require_solid()
        copies = [s]
        for i in range(1, count):
            if kind == "linear":
                d = G.unit(p.get("direction", (1, 0, 0))) * _float(p, "spacing", 10) * i
                copies.append(s.translate(tuple(d)))
            else:
                axis = G.unit(p.get("axis", (0, 0, 1)))
                c = np.asarray(p.get("center", (0, 0, 0)), float)
                total = _float(p, "angle", 360)
                step = total / count if abs(total) >= 360 else total / (count - 1)
                a = math.radians(step * i)
                K = np.array([[0, -axis[2], axis[1]], [axis[2], 0, -axis[0]], [-axis[1], axis[0], 0]])
                R = np.eye(3) + math.sin(a) * K + (1 - math.cos(a)) * K @ K
                copies.append(s.transform(_affine(R, c - R @ c)))
        if p.get("join", True):
            state.bodies[bid] = b.copy(solid=m3.Manifold.batch_boolean(copies, m3.OpType.Add))
            res["modified"].append(bid)
        else:
            for i, cpy in enumerate(copies[1:], 2):
                nid = f"{fid}.{bid}.{i}"
                state.bodies[nid] = Body(f"{b.name} ({i})", solid=cpy)
                res["created"].append(nid)


def f_delete(state, fid, p, res, ctx):
    for bid in p.get("bodies", []):
        if bid in state.bodies:
            del state.bodies[bid]
            res.setdefault("removed", []).append(bid)


def f_separate(state, fid, p, res, ctx):
    """Split a body made of several disconnected pieces into one body per piece."""
    bid = p.get("body")
    b = state.body(bid)
    pieces = b.require_solid().decompose()
    if len(pieces) < 2:
        raise GeometryError("This body is already one piece.")
    pieces = sorted(pieces, key=lambda m: -m.volume())
    state.bodies[bid] = b.copy(solid=pieces[0])
    res["modified"].append(bid)
    for i, pc in enumerate(pieces[1:], 2):
        nid = f"{fid}.{i}"
        state.bodies[nid] = Body(f"{b.name} ({i})", solid=pc)
        res["created"].append(nid)


def f_bedsplit(state, fid, p, res, ctx):
    bid = p.get("body")
    b = state.body(bid)
    s = b.require_solid()
    bed = p.get("bed") or PR.PRINTERS.get(p.get("printer", "bambu_x1"), PR.PRINTERS["bambu_x1"])[1]
    pieces, extras, info = PR.split_to_bed(s, bed, float(p.get("margin", 5)), p.get("connector", "pins"),
                                           p.get("pin_d"), p.get("pin_len"), p.get("clearance"),
                                           p.get("optimize", True))
    if len(pieces) == 1:
        res["warning"] = "This body already fits the bed; nothing was split."
    res["info"] = info
    state.bodies[bid] = b.copy(name=f"{b.name} 1" if len(pieces) > 1 else b.name, solid=pieces[0])
    res["modified"].append(bid)
    for i, pc in enumerate(pieces[1:], 2):
        nid = f"{fid}.{i}"
        state.bodies[nid] = Body(f"{b.name} {i}", solid=pc)
        res["created"].append(nid)
    for i, ex in enumerate(extras, 1):
        nid = f"{fid}.pin{i}"
        state.bodies[nid] = Body(f"Pin {i}", solid=ex)
        res["created"].append(nid)
    if info["faces_without_connectors"] and p.get("connector", "pins") != "none":
        res["warning"] = (f"{info['faces_without_connectors']} cut face(s) were too thin for connectors. "
                          "Try Filament pins (1.75 mm), or glue those seams.")


def f_pushpull(state, fid, p, res, ctx):
    bid = p.get("body")
    b = state.body(bid)
    b.require_solid()
    face = p.get("face") or {}
    if not face.get("contours"):
        raise GeometryError("Click a flat face to push or pull.")
    fr = G.plane_frame(face["origin"], face["normal"], face.get("xdir"))
    tool = PR.pushpull_tool(face["contours"], fr, _float(p, "distance", 5))
    op = "join" if float(p.get("distance", 5)) > 0 else "cut"
    apply_tool(state, fid, tool, op, [bid], "Push/Pull", res)


def f_sculpt(state, fid, p, res, ctx):
    bid = p.get("body")
    b = state.body(bid)
    strokes = p.get("strokes") or []
    if not strokes:
        raise GeometryError("Drag over the model to sculpt.")
    state.bodies[bid] = b.copy(solid=PR.sculpt(b.require_solid(), strokes))
    res["modified"].append(bid)


def f_remesh(state, fid, p, res, ctx):
    bid = p.get("body")
    b = state.body(bid)
    state.bodies[bid] = b.copy(solid=PR.remesh(b.require_solid(), _float(p, "edge", 1.0)))
    res["modified"].append(bid)


def f_layflat(state, fid, p, res, ctx):
    bids = p.get("bodies") or ([p["body"]] if p.get("body") else [])
    if not bids:
        raise GeometryError("Choose a body to lay flat.")
    for bid in bids:
        b = state.body(bid)
        s = b.require_solid()
        normal = PR.best_rest_normal(s) if p.get("auto") or not p.get("normal") else p["normal"]
        M = PR.lay_flat_matrix(s, normal)
        state.bodies[bid] = b.copy(solid=s.transform(M))
        res["modified"].append(bid)


def _mm(x):
    return f"{round(float(x), 1):g}"


def f_plate(state, fid, p, res, ctx):
    """Pack the chosen bodies onto the print bed: rows of footprints a gap apart, the block
    centred on the origin, every body resting on the bed. Deterministic: footprints are placed
    tallest first (ties: wider first, then the order you chose them), each row fills left to
    right, and a new row starts above the tallest part in the row before."""
    bids = list(p.get("bodies") or [])
    if not bids:
        raise GeometryError("Choose the bodies to lay out on the bed.")
    gap = _float(p, "gap", 5)
    if gap < 0:
        raise GeometryError("The gap can't be negative.")
    bed = p.get("bed") or PR.PRINTERS.get(p.get("printer", "bambu_x1"), PR.PRINTERS["bambu_x1"])[1]
    W, H, Hz = (float(a) for a in bed)
    rotate = bool(p.get("rotate", False))
    items = []
    for i, bid in enumerate(bids):
        b = state.body(bid)
        v, _ = b.mesh()
        v = np.asarray(v, float)
        if not len(v):
            raise GeometryError(f"\"{b.name}\" has no geometry.")
        lo, hi = v.min(0), v.max(0)
        size = hi - lo
        if size[2] > Hz + 1e-9:
            raise GeometryError(f"\"{b.name}\" is {_mm(size[2])} mm tall, taller than the bed "
                                f"({_mm(Hz)} mm). Pick a taller printer, or cut it with Fit to Bed.")
        options = [(False, size[0], size[1])]
        if rotate:
            options.append((True, size[1], size[0]))
        fits = [o for o in options if o[1] <= W + 1e-9 and o[2] <= H + 1e-9]
        if not fits:
            turn = "" if rotate else "turn on Rotate, "
            raise GeometryError(f"\"{b.name}\" is {_mm(size[0])} x {_mm(size[1])} mm, too big for the bed "
                                f"({_mm(W)} x {_mm(H)} mm). Try: {turn}pick a bigger printer, or cut it "
                                "with Fit to Bed.")
        rot, dx, dy = min(fits, key=lambda o: o[2])      # the shortest footprint; ties keep the original turn
        items.append({"i": i, "bid": bid, "name": b.name, "lo": lo, "hi": hi, "rot": rot, "dx": dx, "dy": dy})
    order = sorted(items, key=lambda it: (-it["dy"], -it["dx"], it["i"]))
    x = y = shelf = 0.0
    for n, it in enumerate(order):
        if n and x + it["dx"] > W + 1e-9:          # start a new row above the tallest part so far
            y += shelf + gap
            x = shelf = 0.0
        if y + it["dy"] > H + 1e-9:
            raise GeometryError(f"Not everything fits on the bed: \"{it['name']}\" has no room left. "
                                "Try a smaller gap, turn on Rotate, pick a bigger printer, or choose fewer bodies.")
        it["x"], it["y"] = x, y
        x += it["dx"] + gap
        shelf = max(shelf, it["dy"])
    width = max(it["x"] + it["dx"] for it in items)
    height = max(it["y"] + it["dy"] for it in items)
    ox, oy = -width / 2, -height / 2
    for it in order:
        b = state.body(it["bid"])
        R = _rotation(0, 0, 90) if it["rot"] else np.eye(3)
        mx, my = (-it["hi"][1], it["lo"][0]) if it["rot"] else (it["lo"][0], it["lo"][1])
        t = np.array([ox + it["x"] - mx, oy + it["y"] - my, -it["lo"][2]])
        state.bodies[it["bid"]] = _transform_body(b, _affine(R, t))
        res["modified"].append(it["bid"])
    res["info"] = {"placed": len(items), "rotated": sum(1 for it in items if it["rot"])}


def f_thread(state, fid, p, res, ctx):
    kind = p.get("kind", "internal")
    D = _float(p, "diameter", 8)
    P = _float(p, "pitch", 1.25)
    L = _float(p, "length", 10)
    tool = PR.thread_tool(kind, D, P, L, float(p.get("tolerance", 0.2) or 0), p.get("hand", "right"),
                          p.get("chamfer", True))
    o, x, y, n = state.frame(p.get("plane"))
    off = np.asarray(p.get("offset", (0, 0, 0)), float)
    o2 = o + off[0] * x + off[1] * y + off[2] * n
    tool = tool.transform(G.frame_matrix(o2, x, y, n))
    op = p.get("operation") or ("cut" if kind == "internal" else "new")
    apply_tool(state, fid, tool, op, p.get("targets"),
               p.get("body_name") or (f"M{D:g} screw" if kind == "external" else "Thread"), res)


def f_offset(state, fid, p, res, ctx):
    from . import meshtools as MT
    bids = p.get("bodies") or []
    if not bids:
        raise GeometryError("Choose a body to offset.")
    d = _float(p, "distance", 0.2)
    if p.get("direction", "grow") == "shrink":
        d = -abs(d)
    else:
        d = abs(d)
    if abs(d) < 1e-6:
        raise GeometryError("Distance can't be zero.")
    for bid in bids:
        b = state.body(bid)
        state.bodies[bid] = b.copy(solid=MT.offset_solid(b.require_solid(), d))
        res["modified"].append(bid)


def f_resizehole(state, fid, p, res, ctx):
    from . import meshtools as MT
    bid = p.get("body")
    if not bid or not p.get("point"):
        raise GeometryError("Click the wall of a round hole or pin.")
    b = state.body(bid)
    s = b.require_solid()
    v, f = b.mesh()
    tri = MT.nearest_triangle(v, f, p["point"])
    cyl = MT.fit_cylinder(v, f, tri)
    res["info"] = {"kind": cyl["kind"], "diameter": round(2 * cyl["radius"], 3),
                   "length": round(cyl["z1"] - cyl["z0"], 3)}
    fill = p.get("mode") == "fill"
    out = MT.resize_round(s, cyl, None if fill else _float(p, "diameter", 2 * cyl["radius"]), fill=fill)
    if out.is_empty():
        raise GeometryError("The result is empty.")
    state.bodies[bid] = b.copy(solid=out)
    res["modified"].append(bid)


# ------------------------------------------------------------------------------------------
# Support Fins (github.com/gittrahan/support-fins): breakaway fins for the part as posed now
# ------------------------------------------------------------------------------------------
SUPPORT_COLOR = "#59d98e"   # Support Fins' own fin green


def _add_supports(state, fid, res, bid, solids, output, suffix="supports", ctx=None):
    b = state.body(bid)
    fins = solids[0] if len(solids) == 1 else m3.Manifold.batch_boolean(solids, m3.OpType.Add)
    preview = bool((ctx or {}).get("preview"))
    if preview and output != "separate" and b.is_solid:
        # the live preview shows the supports next to the untouched part (no union to wait
        # for, and only the supports are sent to the screen); OK joins them for real
        nid = f"{fid}~sf"
        state.bodies[nid] = Body(f"{b.name} {suffix}", solid=fins)
        res["created"].append(nid)
        res["previewOnly"] = True
        res.setdefault("colors", {})[nid] = SUPPORT_COLOR
        return
    if output == "separate" or not b.is_solid:
        nid, n = fid, 1
        while nid in state.bodies:
            n += 1
            nid = f"{fid}.{n}"
        state.bodies[nid] = Body(f"{b.name} {suffix}", solid=fins)
        res["created"].append(nid)
        if preview:
            res.setdefault("colors", {})[nid] = SUPPORT_COLOR
        if not b.is_solid and output != "separate":
            res["warning"] = f'"{b.name}" isn\'t a watertight solid, so the supports were added as a separate body.'
    else:
        state.bodies[bid] = b.copy(solid=b.solid + fins)
        res["modified"].append(bid)


def f_supportfins(state, fid, p, res, ctx):
    from . import supportfins as SF
    ids = p.get("bodies") or ([p["body"]] if p.get("body") else [])
    if not ids:
        raise GeometryError("Choose the body to support.")
    notes = []
    preview = bool((ctx or {}).get("preview"))
    for bid in ids:
        b = state.body(bid)
        v, f = b.mesh()
        r = SF.fins(v, f, p.get("options") or {})
        res["paint"] = {"body": bid, "over": r["over"], "small": r["small"]}
        # fins you clicked away, and walls you drew by hand (Support Fins' "+ Add walls by hand")
        gone, missing = SF.match_removed(r["pieces"], p.get("removed"))
        hand = [w for w in drawn_walls(p) if len(w.get("points") or []) >= (1 if w.get("shape") == "anchor" else 2)]
        wr = SF.walls(v, f, hand, p.get("options") or {}) if hand else None
        if wr:
            res["walls"] = [{"ok": w["ok"], "reason": w.get("reason")} for w in wr["walls"]]
        extra = ", ".join(x for x in (f"{len(gone)} removed" if gone else "",
                                      f"{sum(1 for w in wr['walls'] if w['ok'])} drawn by hand" if wr else "") if x)
        res["info"] = f"Support Fins {r['version']}: {r['report']}" + (f" ({extra})" if extra else "")
        if missing:
            notes.append(f"{missing} fin(s) you removed aren't there any more in this build, so nothing was removed for them.")
        if wr:
            for i, w in enumerate(wr["walls"]):
                if not w["ok"] or w["failed"]:
                    notes.append(f"Hand wall {i + 1}{' (part of it)' if w['ok'] else ''}: {w.get('reason')}.")
        kept = [sd for i, sd in enumerate(r["solids"]) if i not in gone]
        every = kept + (wr["solids"] if wr else [])
        # the plastic the supports cost (the site's grams receipt; the panel turns it into grams)
        res["support"] = {"volume": round(sum(m.volume() for m in every), 1), "pieces": len(every)}
        if preview:
            # every fin a body of its own beside the untouched part, so one can be clicked to
            # remove it; removed ones stay as see-through ghosts to click back
            res["pieces"], res["previewOnly"] = {}, True
            res.setdefault("colors", {})
            res.setdefault("ghosts", [])

            def put(nid, solid, info):
                state.bodies[nid] = Body(f"{b.name} supports", solid=solid)
                res["created"].append(nid)
                res["pieces"][nid] = info
                res["colors"][nid] = "#d64545" if info.get("removed") else SUPPORT_COLOR
                if info.get("removed"):
                    res["ghosts"].append(nid)
            for i, sd in enumerate(r["solids"]):
                put(f"{fid}~p{i}", sd, dict(r["pieces"][i], removed=i in gone))
            if wr:
                for i, lst in enumerate(wr["byWall"]):
                    if lst:
                        put(f"{fid}~w{i}", lst[0] if len(lst) == 1 else m3.Manifold.batch_boolean(lst, m3.OpType.Add),
                            {"kind": "hand", "wall": i})
            if not every and not r["stats"].get("overhangRegions") and not hand:
                notes.append(f'"{b.name}" has no overhangs in this position. Turn it the way you want to print it '
                             "(drag the rings), then add fins.")
            continue
        if not every:
            notes.append(f'"{b.name}": {r["report"]}. Nothing could be placed; try Full coverage, draw walls by hand, '
                         "or tip the part differently." if r["stats"].get("overhangRegions") or hand else
                         f'"{b.name}" has no overhangs in this position. Rotate it to the way you want to print it, '
                         "then add fins.")
            continue
        if r["skipped"]:
            notes.append(f"{r['skipped']} support piece(s) weren't closed solids and were left out.")
        _add_supports(state, fid, res, bid, every, p.get("output"), "supports", ctx)
    if notes:
        res["warning"] = " ".join(notes)


def drawn_walls(p):
    """The walls of a Draw Walls feature (older projects keep one wall in `points`)."""
    if isinstance(p.get("walls"), list):
        return p["walls"]
    pts = p.get("points") or []
    return [{"shape": "line", "points": pts}] if len(pts) == 2 else []


def f_supportwall(state, fid, p, res, ctx):
    from . import supportfins as SF
    bid = p.get("body")
    b = state.body(bid)
    walls = [w for w in drawn_walls(p) if len(w.get("points") or []) >= (1 if w.get("shape") == "anchor" else 2)]
    if not walls:
        raise GeometryError("Click on the part to draw a wall under an overhang.")
    v, f = b.mesh()
    r = SF.walls(v, f, walls, p.get("options") or {})
    res["walls"] = [{"ok": w["ok"], "reason": w.get("reason")} for w in r["walls"]]
    good = [w for w in r["walls"] if w["ok"]]
    bad = [(i, w) for i, w in enumerate(r["walls"]) if not w["ok"] or w["failed"]]
    if not good:
        raise GeometryError(f"Support Fins couldn’t place that wall: {r['walls'][0].get('reason')}." if len(walls) == 1 else
                            f"Support Fins couldn’t place any of the {len(walls)} walls ({r['walls'][0].get('reason')}).")
    res["support"] = {"volume": round(sum(m.volume() for m in r["solids"]), 1), "pieces": len(r["solids"])}
    length = sum(w["length"] for w in good)
    tines = sum(w["tines"] for w in good)
    tall = max(w["height"] for w in good)
    if len(good) == 1:
        res["info"] = (f"Support Fins {r['version']}: wall {length:.1f} mm long, {tall:.1f} mm tall, {tines} tines"
                       + (", standing on the part" if good[0]["partAttached"] else ""))
    else:
        res["info"] = f"Support Fins {r['version']}: {len(good)} walls, {length:.1f} mm in all, {tines} tines"
    if bad:
        res["warning"] = ". ".join(f"Wall {i + 1}{' (part of it)' if w['ok'] else ''}: {w.get('reason')}" for i, w in bad) + "."
    _add_supports(state, fid, res, bid, r["solids"], p.get("output"), "walls", ctx)


# Slice the solid across each axis; in every slice, what an opening of width w removes is
# narrower than w. Keep the web port (web/engine/feature_modules/thin_wall.js) in step.
EVALUATORS.update({
    "import": f_import, "primitive": f_primitive, "plane": f_plane, "sketch": f_sketch,
    "extrude": f_extrude, "revolve": f_revolve, "loft": f_loft, "sweep": f_sweep, "hole": f_hole, "combine": f_combine,
    "split": f_split, "mirror": f_mirror, "move": f_move, "scale": f_scale, "hollow": f_hollow, "shell": f_shell, "repair": f_repair, "simplify": f_simplify, "pattern": f_pattern, "delete": f_delete,
    "separate": f_separate, "bedsplit": f_bedsplit, "pushpull": f_pushpull, "sculpt": f_sculpt,
    "remesh": f_remesh, "layflat": f_layflat, "plate": f_plate, "thread": f_thread, "offset": f_offset,
    "resizehole": f_resizehole, "supportfins": f_supportfins, "supportwall": f_supportwall,
})

LABELS.update({
    "import": "Import", "primitive": "Primitive", "plane": "Plane", "sketch": "Sketch",
    "extrude": "Extrude", "revolve": "Revolve", "loft": "Loft", "sweep": "Sweep", "hole": "Hole", "combine": "Combine",
    "split": "Split", "mirror": "Mirror", "move": "Move", "scale": "Scale", "hollow": "Hollow", "shell": "Shell", "repair": "Repair", "simplify": "Simplify", "pattern": "Pattern", "delete": "Remove",
    "separate": "Separate", "bedsplit": "Fit to Bed", "pushpull": "Push/Pull", "sculpt": "Sculpt",
    "remesh": "Remesh", "layflat": "Lay Flat", "plate": "Plate Layout", "thread": "Thread", "offset": "Offset",
    "resizehole": "Resize Hole", "supportfins": "Support Fins", "supportwall": "Draw Walls",
})

from .feature_modules import thin_wall, fit_test, lattice, snap_fit, living_hinge  # noqa: E402,F401
