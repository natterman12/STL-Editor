"""The website's demo parts: a phone stand, a long desk tray and a leaning arm.

Drawn here from scratch (no downloaded models, so they are ours to show and to give away as the
"Try it now" sample). The clips in rust/crates/stl-app/tests/site_videos.rs import these STLs the
way someone imports a download. Run: python dev/site-models/make_models.py (needs manifold3d and
numpy); it writes the .stl files next to this script and docs/samples/phone-stand.stl.
"""
import math
import os
import struct

import manifold3d as mf
import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
mf.set_circular_segments(48)


def rounded_2d(cs, r):
    """Round every corner of a 2D shape by r (outer and inner corners)."""
    return cs.offset(-r, mf.JoinType.Round).offset(2 * r, mf.JoinType.Round).offset(-r, mf.JoinType.Round)


def rect(x0, y0, x1, y1):
    return mf.CrossSection.square((x1 - x0, y1 - y0)).translate((x0, y0))


def bar(p, q, t):
    """A strip of thickness t from p to q (2D)."""
    dx, dy = q[0] - p[0], q[1] - p[1]
    L = math.hypot(dx, dy)
    a = math.degrees(math.atan2(dy, dx))
    return mf.CrossSection.square((L, t)).translate((0, -t / 2)).rotate(a).translate(p)


def soften(solid, r=0.8):
    """Break every sharp edge a little, the way a moulded or printed part looks."""
    return solid.minkowski_sum(mf.Manifold.sphere(r, 12)).translate((0, 0, 0))


def write_stl(solid, path):
    mesh = solid.to_mesh()
    v = np.asarray(mesh.vert_properties)[:, :3]
    t = np.asarray(mesh.tri_verts)
    with open(path, "wb") as f:
        f.write(b"STL Editor demo part".ljust(80, b" "))
        f.write(struct.pack("<I", len(t)))
        for a, b, c in t:
            p0, p1, p2 = v[a], v[b], v[c]
            n = np.cross(p1 - p0, p2 - p0)
            ln = np.linalg.norm(n)
            n = n / ln if ln > 0 else n
            f.write(struct.pack("<12fH", *n, *p0, *p1, *p2, 0))
    lo, hi = solid.bounding_box()[:3], solid.bounding_box()[3:]
    print(f"{os.path.relpath(path)}: {len(t)} triangles, {hi[0]-lo[0]:.0f} x {hi[1]-lo[1]:.0f} x {hi[2]-lo[2]:.0f} mm")


def on_bed(solid):
    b = solid.bounding_box()
    return solid.translate((-(b[0] + b[3]) / 2, -(b[1] + b[4]) / 2, -b[2]))


def phone_stand():
    """Side profile in (y, z): the phone leans on the back plate and sits against the front lip.
    Extruded 80 mm along x. The front lip faces -y (toward the iso camera)."""
    t = 6.0
    base = rect(0, 0, 96, t)
    # a low lip, and the plate set well back, so the iso view sees the base between them
    lip = rect(0, 0, t, 15)
    # the back plate, leaning back at 65 degrees from the base
    a = math.radians(65)
    p = (42.0, t / 2)
    q = (p[0] + 100 * math.cos(a), p[1] + 100 * math.sin(a))
    plate = bar(p, q, t)
    # a rear brace from the back of the base up under the plate
    brace = bar((94.0, t / 2), (p[0] + 58 * math.cos(a), p[1] + 58 * math.sin(a)), 5.0)
    prof = rounded_2d(base + lip + plate + brace, 1.5)
    # the profile is in the XY plane: x = depth (y in the part), y = height
    body = prof.extrude(80.0)  # along +z
    # depth -> y, height -> z, width -> x
    body = body.transform(np.array([[0, 0, 1, 0], [1, 0, 0, 0], [0, 1, 0, 0]], dtype=float))
    # a rounded window in the back plate, cut straight through along the plate's normal:
    # local x = width, y = up the plate, z = the plate's normal
    win = rounded_2d(rect(-20, -22, 20, 22), 8).extrude(30).translate((0, 0, -15))
    c, s_ = math.cos(a), math.sin(a)
    mid = (40.0, p[0] + 62 * c, p[1] + 62 * s_)
    win = win.transform(np.array([[1, 0, 0, mid[0]], [0, c, -s_, mid[1]], [0, s_, c, mid[2]]], dtype=float))
    body = body - win
    return on_bed(soften(body, 0.6))


def desk_tray():
    """A long tray with rounded corners and two dividers: longer than most beds (300 mm)."""
    L, W, H, wall, floor = 300.0, 64.0, 32.0, 3.2, 2.4
    outer = rounded_2d(rect(0, 0, L, W), 10).extrude(H)
    inner = rounded_2d(rect(wall, wall, L - wall, W - wall), 8).extrude(H).translate((0, 0, floor))
    tray = outer - inner
    for x in (100.0, 200.0):
        tray = tray + mf.Manifold.cube((wall, W - 2 * wall, H - 8)).translate((x - wall / 2, wall, floor))
    return on_bed(soften(tray, 0.6))


def leaning_arm():
    """A rounded arm leaning 50 degrees off vertical, standing on its flat-cut end: its whole
    underside hangs out over the bed, the plain case Support Fins is for."""
    lean = math.radians(50)
    # the arm: 30 x 20 mm, 100 mm long, tipped about the y axis so it leans toward +x
    arm = rounded_2d(rect(-15, -10, 15, 10), 5).extrude(100)
    c, s_ = math.cos(lean), math.sin(lean)
    arm = arm.transform(np.array([[c, 0, s_, -12], [0, 1, 0, 0], [-s_, 0, c, 2]], dtype=float))
    arm = arm - mf.Manifold.cube((400, 400, 50)).translate((-200, -200, -50))  # flat on the bed
    return on_bed(soften(arm, 0.6))


if __name__ == "__main__":
    stand = phone_stand()
    write_stl(stand, os.path.join(HERE, "phone-stand.stl"))
    samples = os.path.join(HERE, "..", "..", "docs", "samples")
    os.makedirs(samples, exist_ok=True)
    write_stl(stand, os.path.join(samples, "phone-stand.stl"))
    write_stl(desk_tray(), os.path.join(HERE, "desk-tray.stl"))
    write_stl(leaning_arm(), os.path.join(HERE, "leaning-arm.stl"))
