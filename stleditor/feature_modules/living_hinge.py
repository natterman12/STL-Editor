from __future__ import annotations

import numpy as np
import manifold3d as m3

from ..geometry import GeometryError
from ..features import apply_tool, _float, _place


def f_hinge(state, fid, p, res, ctx):
    """Living hinge: a slot across the whole block, leaving a thin web of `thickness` at the bottom."""
    slot = _float(p, "slot", 1.0)        # width of the slot along the plane's x
    web = _float(p, "thickness", 0.5)    # the hinge: material left under the slot
    if slot <= 0 or web <= 0:
        raise GeometryError("The slot width and the hinge thickness must be more than zero.")
    targets = p.get("targets")
    bodies = [state.body(t) for t in targets if t in state.bodies] if targets else \
        [b for b in state.bodies.values() if b.is_solid]
    if not bodies:
        raise GeometryError("Choose the block to put the living hinge in.")
    o, xa, ya, n = state.frame(p.get("plane"))
    lo, hi, reach = np.inf, -np.inf, 0.0
    for b in bodies:
        v, _ = b.mesh()
        rel = np.asarray(v, float) - o
        d = rel @ n
        lo, hi = min(lo, d.min()), max(hi, d.max())
        reach = max(reach, float(np.abs(rel @ xa).max()), float(np.abs(rel @ ya).max()))
    if web >= hi - lo:
        raise GeometryError("The hinge is as thick as the block, so there is nothing to cut.")
    B = reach + 10
    strip = m3.Manifold.cube((slot, 2 * B, hi - lo - web + 1), center=False) \
        .translate((_float(p, "x", 0), -B, lo + web))
    apply_tool(state, fid, _place(strip, state.frame(p.get("plane"))), "cut", targets,
               "Living hinge", res)


from .registry import register
register('hinge', f_hinge, 'Living hinge')
