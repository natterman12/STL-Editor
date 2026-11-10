from __future__ import annotations

import numpy as np
import manifold3d as m3

from .. import geometry as G
from ..geometry import GeometryError
from ..features import _float, _hollow_pitch


def f_lattice(state, fid, p, res, ctx):
    """Hollow the body (solid wall kept outside) and fill the hollow with a cubic grid of round
    struts. Grid lines are spread evenly across the hollow space, about one per cell size."""
    bid = p.get("body")
    if not bid:
        raise GeometryError("Choose a body to fill with a lattice.")
    b = state.body(bid)
    s = b.require_solid()
    cell = _float(p, "cell", 6)
    d = _float(p, "strut", 1)
    t = _float(p, "thickness", 2)
    if cell <= 0 or d <= 0 or t <= 0:
        raise GeometryError("Cell size, strut diameter and wall thickness must all be above zero.")
    if d >= cell:
        raise GeometryError("The struts are as wide as the cells, so there'd be no gaps. Use a thinner strut or a bigger cell.")
    cavity = G.hollow(s, t, _hollow_pitch(b, t, p.get("resolution")), cavity_only=True)
    bb = cavity.bounding_box()
    lo, hi = np.asarray(bb[:3], float), np.asarray(bb[3:], float)
    size = hi - lo
    n = [max(1, round(float(x) / cell)) for x in size]
    if n[1] * n[2] + n[0] * n[2] + n[0] * n[1] > 2500:
        raise GeometryError("That's too many struts. Use a bigger cell size.")
    pos = [lo[a] + (np.arange(n[a]) + 0.5) * size[a] / n[a] for a in range(3)]
    r = d / 2
    struts = []
    for a in range(3):
        o1, o2 = [k for k in range(3) if k != a]
        cyl = m3.Manifold.cylinder(float(size[a]) + 2, r, r, 16)
        if a == 0:
            cyl = cyl.rotate((0, 90, 0))
        elif a == 1:
            cyl = cyl.rotate((-90, 0, 0))
        for u in pos[o1]:
            for w in pos[o2]:
                off = [0.0, 0.0, 0.0]
                off[a], off[o1], off[o2] = lo[a] - 1, u, w
                struts.append(cyl.translate(tuple(off)))
    grid = m3.Manifold.batch_boolean(struts, m3.OpType.Add)
    state.bodies[bid] = b.copy(solid=s - (cavity - grid))
    res["modified"].append(bid)


from .registry import register

register('lattice', f_lattice, 'Lattice')
