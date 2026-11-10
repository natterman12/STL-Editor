from __future__ import annotations

import manifold3d as m3

from ..features import apply_tool, _float, _place
from ..geometry import GeometryError
from .registry import register


def f_snapfit(state, fid, p, res, ctx):
    """Cantilever clip: an arm standing on the plate, with a hook at its tip pointing inwards."""
    L = _float(p, "length", 12)          # arm length, from the plate face up to the tip
    W = _float(p, "width", 6)            # arm width, across the arm
    t = _float(p, "thickness", 1.5)      # arm thickness; the hook sticks out by this much too
    h = _float(p, "hook_height", 1.5)    # hook height, measured down from the tip
    if min(L, W, t, h) <= 0:
        raise GeometryError("Arm length, width, thickness and hook height must all be more than zero.")
    if h >= L:
        raise GeometryError("The hook must be shorter than the arm.")
    sink = 0.5                           # the foot sinks into the plate so the join is solid
    arm = m3.Manifold.cube((t, W, L + sink), center=False).translate((0, 0, -sink))
    hook = m3.Manifold.cube((t, W, h), center=False).translate((t, 0, L - h))
    tool = m3.Manifold.batch_boolean([arm, hook], m3.OpType.Add)
    tool = _place(tool, state.frame(p.get("plane")), (_float(p, "x", 0), _float(p, "y", 0), 0))
    apply_tool(state, fid, tool, p.get("operation") or "join", p.get("targets"),
               p.get("body_name") or "Snap-fit", res)


register('snapfit', f_snapfit, 'Snap-fit')
