from __future__ import annotations

import manifold3d as m3

from ..features import apply_tool, _float
from ..geometry import GeometryError


def f_fittest(state, fid, p, res, ctx):
    """One plate: pegs of a fixed diameter and matching holes at stepped clearances."""
    D = _float(p, "diameter", 8)
    start = _float(p, "start", 0.05)
    step = _float(p, "step", 0.05)
    count = int(p.get("count", 6))
    T = _float(p, "thickness", 3)
    gap = _float(p, "gap", 4)
    if not (D > 0):
        raise GeometryError("Peg diameter must be more than zero.")
    if start < 0:
        raise GeometryError("Clearance start can't be negative.")
    if not (step > 0):
        raise GeometryError("Clearance step must be more than zero.")
    if count < 1:
        raise GeometryError("Count must be at least 1.")
    if not (T > 0):
        raise GeometryError("Plate thickness must be more than zero.")
    if not (gap > 0):
        raise GeometryError("Gap must be more than zero.")
    clearances = [start + i * step for i in range(count)]
    max_hole = D + clearances[-1]
    # edge pad = gap; holes in one row, pegs in a parallel row on the same plate
    L = count * max_hole + (count + 1) * gap
    W = max_hole + D + 3 * gap
    seg = 64
    man = m3.Manifold.cube((L, W, T), center=False).translate((-L / 2, -W / 2, 0))
    for i, c in enumerate(clearances):
        x = -L / 2 + gap + max_hole / 2 + i * (max_hole + gap)
        y_hole = -W / 2 + gap + max_hole / 2
        y_peg = -W / 2 + gap + max_hole + gap + D / 2
        hr = (D + c) / 2
        man = man - m3.Manifold.cylinder(T + 2, hr, hr, seg).translate((x, y_hole, -1))
        man = man + m3.Manifold.cylinder(T, D / 2, D / 2, seg).translate((x, y_peg, T))
    apply_tool(state, fid, man, "new", None, p.get("body_name") or "Fit Test", res)
    res["info"] = (f"{count} pair{'s' if count != 1 else ''}, peg Ø{D:g} mm, "
                   f"clearances {', '.join(f'{c:g}' for c in clearances)} mm.")


from .registry import register
register('fittest', f_fittest, 'Fit Test')
