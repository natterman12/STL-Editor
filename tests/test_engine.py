"""Engine tests: every main tool on simple solids with known volumes.

Run with:  python -m pytest -q tests
"""
import json
import math
import re
import os
import sys
import urllib.request

import manifold3d as m3
import numpy as np
import pytest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from stleditor import geometry as G  # noqa: E402
from stleditor import printing as PR  # noqa: E402
from stleditor.document import Document  # noqa: E402
from stleditor.features import GeometryError  # noqa: E402


def stl_file(tmp_path, man, name):
    mm = man.to_mesh()
    p = tmp_path / name
    p.write_bytes(G.stl_bytes(np.asarray(mm.vert_properties)[:, :3], mm.tri_verts))
    return str(p)


def vol(doc, bid):
    return doc.state.body(bid).info()["volume"]


def bbox(doc, bid):
    v, _ = doc.state.body(bid).mesh()
    return v.min(0), v.max(0)


@pytest.fixture
def block(tmp_path):
    """A 40 x 30 x 20 block imported at its own position (corner at the origin)."""
    doc = Document()
    f, _ = doc.import_file(stl_file(tmp_path, m3.Manifold.cube((40, 30, 20)), "block.stl"), place="keep")
    return doc, f["id"]


def test_import_centres_on_bed(tmp_path):
    doc = Document()
    f, _ = doc.import_file(stl_file(tmp_path, m3.Manifold.cube((10, 10, 10)).translate((50, 60, 70)), "c.stl"))
    lo, hi = bbox(doc, f["id"])
    assert abs(lo[2]) < 1e-6, "sits on the bed"
    assert np.allclose((lo[:2] + hi[:2]) / 2, 0, atol=1e-6), "centred on the origin"


def test_sketch_cut_and_join(block):
    doc, B = block
    v0 = vol(doc, B)
    sk, _ = doc.add_feature("sketch", {"plane": {"origin": [0, 0, 20], "normal": [0, 0, 1]},
                                       "entities": [{"id": "r", "type": "rect", "x": 5, "y": 5, "w": 30, "h": 20},
                                                    {"id": "c", "type": "circle", "cx": 20, "cy": 15, "r": 5}]})
    doc.add_feature("extrude", {"sketch": sk["id"], "distance": 8, "operation": "cut"})
    # the circle inside the rectangle is an island, so it stays
    assert vol(doc, B) == pytest.approx(v0 - (30 * 20 - math.pi * 25) * 8, rel=2e-3)
    doc.add_feature("extrude", {"sketch": sk["id"], "distance": 5, "operation": "join", "entities": ["c"]})
    assert bbox(doc, B)[1][2] == pytest.approx(25, abs=1e-6)


def test_holes(block):
    doc, B = block
    v0 = vol(doc, B)
    doc.add_feature("hole", {"points": [[10, 15, 20]], "normal": [0, 0, 1], "diameter": 5, "through": True})
    v1 = vol(doc, B)
    assert v0 - v1 == pytest.approx(math.pi * 2.5 ** 2 * 20, rel=0.01)
    doc.add_feature("hole", {"points": [[30, 15, 20]], "normal": [0, 0, 1], "diameter": 6.2, "through": False,
                             "depth": 3.2, "tip": False})
    assert v1 - vol(doc, B) == pytest.approx(math.pi * 3.1 ** 2 * 3.2, rel=0.01)
    for kind in ("counterbore", "countersink", "hex"):
        doc.add_feature("hole", {"points": [[20, 8, 20]], "normal": [0, 0, 1], "diameter": 3.4, "kind": kind})
    assert doc.state.body(B).is_solid


def test_failing_feature_is_refused(block):
    doc, _ = block
    n = len(doc.features)
    with pytest.raises(GeometryError):
        doc.add_feature("hole", {"points": [], "normal": [0, 0, 1]})
    assert len(doc.features) == n


def test_thread_bolt_and_nut():
    doc = Document()
    doc.add_feature("thread", {"kind": "external", "diameter": 8, "pitch": 1.25, "length": 12, "tolerance": 0.2,
                               "plane": "XY", "offset": [0, 0, 0]})
    bolt = doc.state.bodies[doc.features[-1]["id"]].require_solid()
    nut_f, _ = doc.add_feature("primitive", {"shape": "cylinder", "radius": 7.5, "height": 6, "segments": 6,
                                             "plane": "XY", "offset": [0, 0, 4], "operation": "new"})
    N = nut_f["id"]
    doc.add_feature("thread", {"kind": "internal", "diameter": 8, "pitch": 1.25, "length": 6, "tolerance": 0.2,
                               "plane": "XY", "offset": [0, 0, 10], "operation": "cut", "targets": [N]})
    nut = doc.state.body(N).require_solid()
    hex_prism = 3 * math.sqrt(3) / 2 * 7.5 ** 2 * 6
    assert nut.volume() < 0.85 * hex_prism, "the nut has a threaded hole"
    # screwed onto the bolt (turned to where the threads mesh) they must not overlap:
    # 0.2 mm clearance on each part
    assert min((bolt ^ nut.rotate((0, 0, a))).volume() for a in range(0, 360, 15)) < 0.05


def _iso_thread_radius(u, D, P):
    """Ideal ISO metric profile radius at axial phase u (same profile and phase as printing.py)."""
    depth = 5 * 0.8660254 * P / 8
    R = D / 2
    a, b, c, d = P / 8, 7 * P / 16, 9 * P / 16, 7 * P / 8
    u = np.mod(u, P)
    return np.select([u < a, u < b, u < c, u < d],
                     [R - depth, R - depth + depth * (u - a) / (b - a), R, R - depth * (u - c) / (d - c)],
                     R - depth)


def test_thread_flanks_smooth():
    """The thread surface follows the ISO profile along the helix (no jagged flanks)."""
    D, P, L = 8.0, 1.25, 12.0
    uu = np.linspace(0, P, 200001)
    ideal_vol = L * math.pi / P * np.trapezoid(_iso_thread_radius(uu, D, P) ** 2, uu)
    for hand, s in (("right", 1), ("left", -1)):
        rod = PR.thread_tool("external", D, P, L, tolerance=0, hand=hand, chamfer=False)
        mm = rod.to_mesh()
        v = np.asarray(mm.vert_properties)[:, :3]
        cen = v[np.asarray(mm.tri_verts)].mean(1)
        cen = cen[(cen[:, 2] > 1.5) & (cen[:, 2] < L - 1.5)]
        th = np.arctan2(cen[:, 1], cen[:, 0])
        u = cen[:, 2] - s * th / (2 * math.pi) * P
        dev = np.abs(np.hypot(cen[:, 0], cen[:, 1]) - _iso_thread_radius(u, D, P)).max()
        assert dev < 0.01, f"{hand}-hand thread surface is jagged: {dev:.4f} mm off the ISO profile"
        assert rod.volume() == pytest.approx(ideal_vol, rel=0.01), hand


def test_lay_flat_auto(block):
    doc, B = block
    doc.add_feature("move", {"bodies": [B], "rotate": [30, 20, 0], "translate": [0, 0, 0]})
    doc.add_feature("layflat", {"bodies": [B], "auto": True})
    lo, hi = bbox(doc, B)
    assert lo[2] == pytest.approx(0, abs=1e-6)
    assert hi[2] - lo[2] == pytest.approx(20, abs=0.05), "rests on its largest face"


def test_split_with_pins(block):
    doc, B = block
    doc.add_feature("split", {"body": B, "plane": {"origin": [20, 0, 0], "normal": [1, 0, 0]},
                              "connector": "pins", "pin_d": 5, "pin_len": 10, "clearance": 0.15})
    assert len(doc.state.bodies) == 2
    # printed pins stick out of one half across the cut plane
    los, his = zip(*(bbox(doc, b) for b in doc.state.bodies))
    assert min(h[0] for h in his) > 20 + 3 or max(lo[0] for lo in los) < 20 - 3


def test_fit_to_bed():
    doc = Document()
    f, _ = doc.add_feature("primitive", {"shape": "box", "length": 400, "width": 60, "height": 40, "plane": "XY"})
    doc.add_feature("bedsplit", {"body": f["id"], "printer": "bambu_a1mini", "connector": "pins"})
    bed = PR.PRINTERS["bambu_a1mini"][1]
    pieces = [b for b in doc.state.bodies if not doc.body_name(b).startswith("Pin")]
    assert len(pieces) >= 3
    for b in pieces:
        lo, hi = bbox(doc, b)
        assert max(hi - lo) <= max(bed) + 1e-6


def test_mirror_and_pattern_features(block):
    doc, B = block
    h, _ = doc.add_feature("hole", {"points": [[5, 15, 20]], "normal": [0, 0, 1], "diameter": 4, "through": True})
    v1 = vol(doc, B)
    doc.add_feature("mirror", {"kind": "features", "features": [h["id"]], "plane": {"origin": [20, 0, 0], "normal": [1, 0, 0]}})
    v2 = vol(doc, B)
    assert v1 - v2 == pytest.approx(math.pi * 4 * 20, rel=0.02)
    doc.add_feature("pattern", {"source": "features", "features": [h["id"]], "kind": "linear",
                                "direction": [0, 1, 0], "count": 3, "spacing": 5})
    assert v2 - vol(doc, B) == pytest.approx(2 * math.pi * 4 * 20, rel=0.03)


def test_hollow_sphere(tmp_path):
    doc = Document()
    f, _ = doc.import_file(stl_file(tmp_path, m3.Manifold.sphere(15, 128), "ball.stl"), place="keep")
    doc.add_feature("hollow", {"body": f["id"], "thickness": 2})
    expected = 4 / 3 * math.pi * (15 ** 3 - 13 ** 3)
    assert vol(doc, f["id"]) == pytest.approx(expected, rel=0.08)


def test_shell_opens_picked_faces(tmp_path):
    # 20 mm cube, 2 mm wall: closed = 20^3 - 16^3; top open = 20^3 - 16*16*18; top+bottom = 20^3 - 16*16*20;
    # top + a side (no bar left along their shared edge) = 20^3 - 18*16*18
    sq = [[[-10, -10], [10, -10], [10, 10], [-10, 10]]]
    top = {"origin": [10, 10, 20], "normal": [0, 0, 1], "xdir": [1, 0, 0], "contours": sq}
    bottom = {"origin": [10, 10, 0], "normal": [0, 0, -1], "xdir": [1, 0, 0], "contours": sq}
    side = {"origin": [20, 10, 10], "normal": [1, 0, 0], "contours": sq}   # +x side, shares an edge with the top
    path = stl_file(tmp_path, m3.Manifold.cube((20, 20, 20)), "cube.stl")
    for faces, expected, genus in (([], 8000 - 16 ** 3, -1), ([top], 8000 - 16 * 16 * 18, 0),
                                   ([top, bottom], 8000 - 16 * 16 * 20, 1), ([top, side], 8000 - 18 * 16 * 18, 0)):
        doc = Document()
        f, _ = doc.import_file(path, place="keep")
        doc.add_feature("shell", {"body": f["id"], "faces": faces, "thickness": 2})
        assert vol(doc, f["id"]) == pytest.approx(expected, rel=3e-3)
        assert doc.state.body(f["id"]).solid.genus() == genus
    doc = Document()
    f, _ = doc.import_file(path, place="keep")
    with pytest.raises(GeometryError, match="too large"):
        doc.add_feature("shell", {"body": f["id"], "faces": [top], "thickness": 12})
    with pytest.raises(GeometryError, match="flat"):
        doc.add_feature("shell", {"body": f["id"], "faces": [{"origin": [10, 10, 20], "normal": [0, 0, 1],
                                                             "contours": []}], "thickness": 2})


def test_lattice_fills_hollow_with_struts(tmp_path):
    # 20 mm cube, 2 mm wall: cavity 16^3, cell 8 -> struts at 4 and 12 in each direction (12 struts
    # 16 mm long). Skin 8000-4096 = 3904, plus about 12*16*0.7654 (16-sided 1 mm strut) minus the
    # crossings = ~4038. The cavity is voxel-made, so this is a range check, not exact.
    path = stl_file(tmp_path, m3.Manifold.cube((20, 20, 20)), "cube.stl")
    doc = Document()
    f, _ = doc.import_file(path, place="keep")
    doc.add_feature("lattice", {"body": f["id"], "cell": 8, "strut": 1, "thickness": 2})
    assert 3904 < vol(doc, f["id"]) < 8000
    assert vol(doc, f["id"]) == pytest.approx(4038, abs=45)
    for bad, msg in (({"cell": 1, "strut": 2}, "thinner"), ({"cell": 8, "strut": 1, "thickness": 12}, "too large"),
                     ({"cell": 0}, "above zero")):
        doc = Document()
        f, _ = doc.import_file(path, place="keep")
        with pytest.raises(GeometryError, match=msg):
            doc.add_feature("lattice", {"body": f["id"], "cell": 8, "strut": 1, "thickness": 2, **bad})


def _shoelace(contours):
    return abs(sum(0.5 * sum(p[0] * q[1] - q[0] * p[1] for p, q in zip(c, c[1:] + c[:1])) for c in contours))


def test_face_pick_rejects_curved_faces():
    # a facet of a cylinder's side is two coplanar triangles: it isn't a flat face
    from stleditor import server
    app = server.App()
    for p in ({"shape": "cylinder", "radius": 10, "height": 20}, {"shape": "box", "length": 20, "width": 20, "height": 20,
                                                                   "offset": [40, 0, 0]}):
        app.doc.add_feature("primitive", {**p, "plane": "XY"})
    cyl, box = list(app.doc.state.bodies)

    def pick(bid, want):
        v, f = app.doc.state.body(bid).mesh()
        t = v[f]
        n = np.cross(t[:, 1] - t[:, 0], t[:, 2] - t[:, 0])
        n /= np.linalg.norm(n, axis=1)[:, None]
        tri = int(np.nonzero(want(n))[0][0])
        return server.r_face(app, {"body": bid, "tri": tri, "outline": True})

    side = pick(cyl, lambda n: np.abs(n[:, 2]) < 0.1)
    assert not side["planar"]
    top = pick(cyl, lambda n: n[:, 2] > 0.99)
    assert top["planar"]
    assert _shoelace(top["contours"]) == pytest.approx(0.5 * 96 * 100 * math.sin(2 * math.pi / 96), rel=1e-6)
    assert _shoelace(top["contours"]) == pytest.approx(math.pi * 100, rel=5e-3)
    btop = pick(box, lambda n: n[:, 2] > 0.99)
    assert btop["planar"] and _shoelace(btop["contours"]) == pytest.approx(400, rel=1e-9)
    assert pick(box, lambda n: n[:, 0] > 0.99)["planar"]


def test_face_planar_keeps_flat_faces_of_meshes():
    def top_of(man):
        mm = man.to_mesh()
        v, f = np.asarray(mm.vert_properties)[:, :3], np.asarray(mm.tri_verts)
        t = v[f]
        n = np.cross(t[:, 1] - t[:, 0], t[:, 2] - t[:, 0])
        n /= np.maximum(np.linalg.norm(n, axis=1), 1e-30)[:, None]
        region, nrm, _ = G.face_region(v, f, int(np.nonzero(n[:, 2] > 0.9999)[0][0]))
        return v, f, region, nrm
    # finely tessellated flat face (as in a downloaded STL), a face with a hole,
    # and a flat top that blends into rounded edges at a shallow angle
    fine = m3.Manifold.cube((20, 20, 20)).refine(8)
    holed = m3.Manifold.cube((20, 20, 20)) - m3.Manifold.cylinder(30, 4, 4, 64).translate((10, 10, -5))
    rounded = m3.Manifold.batch_hull([m3.Manifold.sphere(3, 48).translate((x, y, z))
                                      for x in (0, 20) for y in (0, 20) for z in (0, 10)])
    for man in (fine, holed, rounded):
        v, f, region, nrm = top_of(man)
        assert G.face_is_planar(v, f, region, nrm)
    # a facet of a sphere isn't flat
    mm = m3.Manifold.sphere(10, 48).to_mesh()
    v, f = np.asarray(mm.vert_properties)[:, :3], np.asarray(mm.tri_verts)
    for tri in (0, len(f) // 2):
        region, nrm, _ = G.face_region(v, f, tri)
        assert not G.face_is_planar(v, f, region, nrm)


def test_repair_broken_mesh(tmp_path):
    mm = m3.Manifold.sphere(10, 48).to_mesh()
    v, t = np.asarray(mm.vert_properties)[:, :3], np.asarray(mm.tri_verts)
    p = tmp_path / "broken.stl"
    p.write_bytes(G.stl_bytes(v, t[10:]))  # punch a hole: drop 10 triangles
    doc = Document()
    f, _ = doc.import_file(str(p), place="keep", repair=False)
    assert not doc.state.body(f["id"]).is_solid
    doc.add_feature("repair", {"body": f["id"], "mode": "auto"})
    assert doc.state.body(f["id"]).is_solid
    assert vol(doc, f["id"]) == pytest.approx(4 / 3 * math.pi * 1000, rel=0.05)


def test_undo_redo_save_open_export(block, tmp_path):
    doc, B = block
    v0 = vol(doc, B)
    doc.add_feature("hole", {"points": [[20, 15, 20]], "normal": [0, 0, 1], "diameter": 6, "through": True})
    v1 = vol(doc, B)
    doc.undo()
    assert vol(doc, B) == pytest.approx(v0)
    doc.redo()
    assert vol(doc, B) == pytest.approx(v1)
    path = str(tmp_path / "part.stlx")
    doc.save(path)
    doc2 = Document.open(path)
    assert vol(doc2, B) == pytest.approx(v1)
    for ext in ("stl", "3mf", "obj"):
        out = tmp_path / f"part.{ext}"
        assert doc2.export(str(out)) == 1
        assert out.stat().st_size > 1000


def test_http_api():
    from stleditor import server
    app = server.App()
    httpd, port = server.serve(app, 0)
    try:
        def call(route, data=None):
            req = urllib.request.Request(f"http://127.0.0.1:{port}/api/{route}",
                                         data=None if data is None else json.dumps(data).encode(),
                                         method="GET" if data is None else "POST")
            with urllib.request.urlopen(req, timeout=30) as r:
                return json.load(r)
        assert call("state")["features"] == []
        r = call("feature/add", {"type": "primitive", "params": {"shape": "box", "length": 10, "width": 10, "height": 10}})
        assert r["ok"] and len(r["state"]["bodies"]) == 1
        with urllib.request.urlopen(f"http://127.0.0.1:{port}/", timeout=10) as r:
            assert b"STL Editor" in r.read()
    finally:
        httpd.shutdown()


# ------------------------------------------------------------------ v1.3 tools
def test_resize_hole_shrink_grow_fill(block):
    doc, B = block
    doc.add_feature("hole", {"points": [[20, 15, 20]], "normal": [0, 0, 1], "diameter": 5, "through": True})
    v0 = vol(doc, B)
    doc.add_feature("resizehole", {"body": B, "point": [22.5, 15, 10], "diameter": 3.4})
    v1 = vol(doc, B)
    assert v1 - v0 == pytest.approx(math.pi * (2.5 ** 2 - 1.7 ** 2) * 20, rel=0.02)
    doc.add_feature("resizehole", {"body": B, "point": [21.7, 15, 10], "diameter": 6})
    assert v1 - vol(doc, B) == pytest.approx(math.pi * (3 ** 2 - 1.7 ** 2) * 20, rel=0.02)
    doc.add_feature("resizehole", {"body": B, "point": [23, 15, 10], "mode": "fill"})
    assert vol(doc, B) == pytest.approx(40 * 30 * 20, rel=1e-3)


def test_resize_blind_hole_keeps_bottom(block):
    doc, B = block
    doc.add_feature("hole", {"points": [[20, 15, 20]], "normal": [0, 0, 1], "diameter": 5, "through": False,
                             "depth": 8, "tip": False})
    v0 = vol(doc, B)
    doc.add_feature("resizehole", {"body": B, "point": [22.5, 15, 16], "diameter": 6.2})
    assert v0 - vol(doc, B) == pytest.approx(math.pi * (3.1 ** 2 - 2.5 ** 2) * 8, rel=0.03)


def test_resize_pin():
    doc = Document()
    f, _ = doc.add_feature("primitive", {"shape": "box", "length": 40, "width": 30, "height": 5, "anchor": "corner"})
    P = f["id"]
    doc.add_feature("primitive", {"shape": "cylinder", "radius": 3, "height": 10, "offset": [20, 15, 5],
                                  "operation": "join", "targets": [P]})
    v0 = vol(doc, P)
    doc.add_feature("resizehole", {"body": P, "point": [23, 15, 10], "diameter": 5.6})
    assert v0 - vol(doc, P) == pytest.approx(math.pi * (9 - 2.8 ** 2) * 10, rel=0.03)


def test_resize_hole_rejects_flat_face(block):
    doc, B = block
    with pytest.raises(GeometryError):
        doc.add_feature("resizehole", {"body": B, "point": [20, 15, 20], "diameter": 4})


def test_offset_grow_and_shrink():
    doc = Document()
    f, _ = doc.add_feature("primitive", {"shape": "box", "length": 20, "width": 20, "height": 20})
    C = f["id"]
    doc.add_feature("offset", {"bodies": [C], "distance": 0.5, "direction": "grow"})
    lo, hi = bbox(doc, C)
    assert np.all(hi - lo >= 21 - 1e-6) and np.all(hi - lo <= 21.2)
    doc.add_feature("offset", {"bodies": [C], "distance": 0.5, "direction": "shrink"})
    assert vol(doc, C) == pytest.approx(8000, rel=0.01)


def test_cut_with_clearance():
    doc = Document()
    a, _ = doc.add_feature("primitive", {"shape": "box", "length": 30, "width": 30, "height": 10})
    t, _ = doc.add_feature("primitive", {"shape": "cylinder", "radius": 5, "height": 20, "offset": [0, 0, 2]})
    peg = doc.state.body(t["id"]).require_solid()
    doc.add_feature("combine", {"target": a["id"], "tools": [t["id"]], "operation": "cut", "clearance": 0.3,
                                "keep_tools": True})
    box = doc.state.body(a["id"]).require_solid()
    # the original peg fits with a gap: no overlap, and the pocket is wider than the peg
    assert (box ^ peg).volume() < 1e-6
    assert 9000 - box.volume() > math.pi * 5.3 ** 2 * 8


def test_project_outline():
    import itertools
    from stleditor import meshtools as MT
    doc = Document()
    f, _ = doc.add_feature("primitive", {"shape": "box", "length": 40, "width": 30, "height": 20})
    doc.add_feature("hole", {"points": [[0, 0, 20]], "normal": [0, 0, 1], "diameter": 8, "through": True})
    solid = doc.state.body(f["id"]).solid
    ids = itertools.count()
    ents = MT.outline_entities(MT.outline([solid], G.plane_frame([0, 0, 10], [0, 0, 1])), lambda: f"p{next(ids)}")
    kinds = sorted(e["type"] for e in ents)
    assert kinds == ["circle", "polyline"]
    circle = next(e for e in ents if e["type"] == "circle")
    assert circle["r"] == pytest.approx(4, abs=0.02)
    # an outline offset outward by 1 mm, extruded, makes a frame 2 mm wider than the part
    grown = MT.outline([solid], G.plane_frame([0, 0, 0], [0, 1, 0]), "silhouette", offset=1.0)
    pts = np.concatenate(grown)
    assert pts[:, 0].max() - pts[:, 0].min() == pytest.approx(42, abs=0.05)


def test_text_profile():
    from stleditor import meshtools as MT
    if MT.default_font() is None:
        pytest.skip("no fonts on this machine")
    cont = MT.text_contours("O", size=10)
    assert len(cont) == 2, "the O keeps its hole"
    lo, hi = MT.text_extent(cont)
    assert 4 < hi[1] - lo[1] < 10
    doc = Document()
    ents = [{"id": f"t{i}", "type": "polyline", "closed": True, "pts": c.tolist()} for i, c in enumerate(cont)]
    sk, _ = doc.add_feature("sketch", {"plane": "XY", "entities": ents})
    f, _ = doc.add_feature("extrude", {"sketch": sk["id"], "distance": 2, "operation": "new"})
    body = doc.state.bodies[f["id"]].require_solid()
    assert body.genus() == 1, "a ring, not a disc"


def test_project_outline_on_a_face():
    """Sketching on the top face and projecting gives that face's outline (looks just inside)."""
    from stleditor import meshtools as MT
    doc = Document()
    f, _ = doc.add_feature("primitive", {"shape": "box", "length": 40, "width": 30, "height": 20})
    polys = MT.outline([doc.state.body(f["id"]).solid], G.plane_frame([0, 0, 20], [0, 0, 1]))
    pts = np.concatenate(polys)
    assert pts[:, 0].max() - pts[:, 0].min() == pytest.approx(40, abs=0.01)


def test_sketch_reference_edges():
    """Part edges a sketch can dimension from: the face on the plane, else the cut, else the shadow."""
    from stleditor import meshtools as MT
    doc = Document()
    f, _ = doc.add_feature("primitive", {"shape": "box", "length": 40, "width": 30, "height": 20})
    doc.add_feature("hole", {"points": [[0, 0, 20]], "normal": [0, 0, 1], "diameter": 8, "through": True})
    solid = doc.state.body(f["id"]).solid
    for z, kind in ((20, "face"), (10, "section"), (-5, "silhouette")):
        r = MT.ref_geometry([solid], G.plane_frame([0, 0, z], [0, 0, 1]))
        assert r["kind"] == kind
        types = sorted(e["type"] for e in r["refs"])
        assert types == ["circle", "line", "line", "line", "line"], types
        circle = next(e for e in r["refs"] if e["type"] == "circle")
        assert circle["r"] == pytest.approx(4, abs=0.02)
        xs = [p[0] for e in r["refs"] if e["type"] == "line" for p in (e["p1"], e["p2"])]
        assert max(xs) - min(xs) == pytest.approx(40, abs=1e-3)


def _arrays(man):
    mm = man.to_mesh()
    return np.asarray(mm.vert_properties)[:, :3], np.asarray(mm.tri_verts)


def test_repair_touching_shells_and_fins():
    # two cubes sharing a wall: the wall cancels out and the result is one 20 x 10 x 10 block
    va, fa = _arrays(m3.Manifold.cube([10, 10, 10]))
    vb, fb = _arrays(m3.Manifold.cube([10, 10, 10]).translate([10, 0, 0]))
    man = G.local_repair(np.concatenate([va, vb]), np.concatenate([fa, fb + len(va)]))
    assert man is not None and abs(man.volume() - 2000) < 1e-6
    # a stray triangle hanging off a sphere's edge is dropped
    s = m3.Manifold.sphere(10, 32)
    v, f = _arrays(s)
    man = G.local_repair(np.concatenate([v, [[0, 0, 20]]]), np.concatenate([f, [[f[0][0], f[0][1], len(v)]]]))
    assert man is not None and abs(man.volume() - s.volume()) < 1e-3


def test_weld_keeps_one_duplicate_and_cancels_opposite_pairs():
    v = np.array([[0, 0, 0], [1, 0, 0], [0, 1, 0], [0, 0, 1]], float)
    f = np.array([[0, 1, 2], [2, 1, 0], [0, 1, 3], [0, 1, 3], [1, 3, 2]])
    _, nf = G.weld(v, f)
    assert len(nf) == 2


def _doc_with(man, name="part.stl"):
    v, f = _arrays(man)
    d = Document()
    fe, _ = d.import_file(name, G.stl_bytes(v, f), True, "keep")
    return d, fe["id"]


def test_support_fins():
    pytest.importorskip("py_mini_racer")
    from stleditor import supportfins as SF
    inf = SF.info()
    assert inf["schema"]["version"] == 1 and any(o["key"] == "tineDensity" for o in inf["schema"]["options"])
    d, bid = _doc_with(m3.Manifold.cube([30, 20, 10], True).rotate([0, 40, 0]))
    f, r = d.add_feature("supportfins", {"body": bid, "output": "join", "options": {}})
    assert "3 walls, 15 tines" in r["info"] and r["paint"]["over"]
    assert abs(d.state.body(bid).info()["volume"] - 6796) < 15   # the web edition gives the same
    d.edit_feature(f["id"], {"body": bid, "output": "separate", "options": {"tineDensity": 1}})
    assert len(d.state.bodies) == 2 and abs(d.state.body(bid).info()["volume"] - 6000) < 1e-3
    with pytest.raises(GeometryError, match="tineDensity"):
        d.add_feature("supportfins", {"body": bid, "options": {"tineDensity": 5}})
    # a hand-drawn wall under the arm of an upside-down L
    ell = m3.Manifold.batch_boolean([m3.Manifold.cube([10, 20, 35]), m3.Manifold.cube([30, 20, 8]).translate([0, 0, 27])],
                                    m3.OpType.Add)
    d2, eid = _doc_with(ell, "ell.stl")
    v0 = d2.state.body(eid).info()["volume"]
    _, rw = d2.add_feature("supportwall", {"body": eid, "points": [[22, 3, 27], [22, 17, 27]], "output": "join", "options": {}})
    assert "wall 14.0 mm long" in rw["info"] and d2.state.body(eid).info()["volume"] > v0 + 100


def test_support_fins_engine_update(tmp_path, monkeypatch):
    """The desktop app downloads a newer published engine, checks it, and ignores bad ones."""
    import functools
    import hashlib
    import http.server
    import shutil
    import threading
    from stleditor import supportfins as SF
    pub = tmp_path / "pub"
    pub.mkdir()
    shutil.copyfile(os.path.join(SF.BUNDLED, "engine.iife.js"), pub / "engine.iife.js")
    data = (pub / "engine.iife.js").read_bytes()

    def publish(**kw):
        info = {"tag": "v9.9.9", "version": "9.9.9", "api": SF.API, "sha256": {"engine.iife.js": hashlib.sha256(data).hexdigest()}}
        info.update(kw)
        (pub / "VERSION.json").write_text(json.dumps(info), encoding="utf-8")

    httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(pub)))
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    try:
        monkeypatch.setattr(SF, "published_url", lambda repo=None: f"http://127.0.0.1:{httpd.server_address[1]}/")
        monkeypatch.setattr(SF, "USER_DIR", str(tmp_path / "user"))
        publish(api=SF.API + 1)
        assert SF.check_update(force=True) is None, "a newer integration API waits for an app update"
        publish(sha256={"engine.iife.js": "0" * 64})
        assert SF.check_update(force=True) is None, "a bad checksum is refused"
        publish()
        assert SF.check_update(force=True) == "v9.9.9"
        assert SF._read_info(SF.USER_DIR)["tag"] == "v9.9.9"
        assert SF._choose()["tag"] == "v9.9.9", "the newer engine is used from the next start"
        assert SF.check_update(force=True) is None, "nothing newer the second time"
    finally:
        httpd.shutdown()


def test_support_fins_engine_crlf_checkout(tmp_path):
    """A Windows checkout with CRLF line endings still counts as a valid engine."""
    import shutil
    from stleditor import supportfins as SF
    for name in ("VERSION.json", "engine.iife.js"):
        shutil.copy(os.path.join(SF.BUNDLED, name), tmp_path / name)
    js = tmp_path / "engine.iife.js"
    js.write_bytes(js.read_bytes().replace(b"\r\n", b"\n").replace(b"\n", b"\r\n"))
    assert SF._read_info(str(tmp_path)) is not None
    js.write_bytes(js.read_bytes() + b"// tampered\r\n")
    assert SF._read_info(str(tmp_path)) is None


def test_draw_walls_lines_paths_curves():
    """Several walls, a multi-section line and a curve; the preview shows them beside the part."""
    pytest.importorskip("py_mini_racer")
    from stleditor import supportfins as SF
    assert SF.info()["curves"], "the curved-wall extra is built in"
    ell = m3.Manifold.cube([10, 20, 35]) + m3.Manifold.cube([30, 20, 8]).translate([0, 0, 27])
    d, e = _doc_with(ell, "ell.stl")
    v0 = d.state.body(e).info()["volume"]
    walls = [
        {"shape": "line", "points": [[17, 3, 27], [17, 17, 27]]},
        {"shape": "path", "points": [[21, 2, 27], [26, 6, 27], [26, 14, 27]]},
        {"shape": "curve", "points": [[28.5, 2, 27], [28.5, 18, 27], [24, 10, 27]]},
        {"shape": "line", "points": [[22, 3, 27], [22.5, 3, 27]]},
    ]
    pst, pr = d.preview("supportwall", {"body": e, "walls": walls, "output": "join", "options": {}})
    assert pr["previewOnly"] and len(pr["created"]) == 1
    assert abs(pst.body(e).info()["volume"] - v0) < 1e-6
    assert [w["ok"] for w in pr["walls"]] == [True, True, True, False]
    assert "Wall 4: wall too short" in pr["warning"]
    f, r = d.add_feature("supportwall", {"body": e, "walls": walls, "output": "join", "options": {}})
    assert "3 walls" in r["info"]
    v1 = d.state.body(e).info()["volume"]
    assert abs(v1 - 11550.85) < 2, v1     # the web engine gives the same
    d.edit_feature(f["id"], {"body": e, "walls": [walls[2]], "output": "separate", "options": {}})
    curved = list(d.state.bodies.values())[-1].info()["volume"]
    assert abs(curved - 576.5) < 1, curved


def _box(doc, length, width, height):
    f, _ = doc.add_feature("primitive", {"shape": "box", "length": length, "width": width, "height": height, "plane": "XY"})
    return f["id"]


def test_plate_layout_hand_checked():
    doc = Document()
    A, B, C = _box(doc, 40, 30, 10), _box(doc, 40, 30, 20), _box(doc, 50, 20, 5)
    doc.add_feature("move", {"bodies": [A], "translate": [0, 0, 7]})    # A starts 7 mm up in the air
    doc.add_feature("plate", {"bodies": [A, B, C], "bed": [100, 100, 50], "gap": 5})
    # A and B (40 x 30) share the first row with a 5 mm gap; C (50 x 20) starts a second row 5 mm
    # above the first. The 85 x 55 block is centred on the origin and every body sits on the bed.
    expect = {A: ((-42.5, -27.5, 0), (-2.5, 2.5, 10)),
              B: ((2.5, -27.5, 0), (42.5, 2.5, 20)),
              C: ((-42.5, 7.5, 0), (7.5, 27.5, 5))}
    for bid, (lo, hi) in expect.items():
        l, h = bbox(doc, bid)
        assert np.allclose(l, lo, atol=1e-3) and np.allclose(h, hi, atol=1e-3), bid


def test_plate_rotate_and_refusals():
    doc = Document()
    L = _box(doc, 80, 20, 5)
    with pytest.raises(GeometryError, match="Rotate"):      # 80 mm wide won't fit a 50 mm bed as it lies
        doc.add_feature("plate", {"bodies": [L], "bed": [50, 100, 50]})
    doc.add_feature("plate", {"bodies": [L], "bed": [50, 100, 50], "rotate": True})
    lo, hi = bbox(doc, L)                                     # turned to 20 x 80, centred
    assert np.allclose(lo[:2], (-10, -40), atol=1e-3) and np.allclose(hi[:2], (10, 40), atol=1e-3)
    tall = _box(doc, 10, 10, 60)
    with pytest.raises(GeometryError, match="taller"):
        doc.add_feature("plate", {"bodies": [tall], "bed": [100, 100, 50]})
    crowd = [_box(doc, 60, 60, 5), _box(doc, 60, 60, 5)]     # 65 + 60 > 100 and 65 + 60 > 100 again
    with pytest.raises(GeometryError, match="Not everything fits"):
        doc.add_feature("plate", {"bodies": crowd, "bed": [100, 100, 50]})


def test_move_lands_on_bed():
    low = lambda d, b: float(d.state.body(b).mesh()[0][:, 2].min())  # noqa: E731
    doc, B = _doc_with(m3.Manifold.cube((40, 30, 20)), "block.stl")
    doc.add_feature("move", {"bodies": [B], "translate": [0, 0, 0], "rotate": [30, 0, 0], "pivot": "center", "bed": True})
    assert abs(low(doc, B)) < 1e-6
    doc.add_feature("move", {"bodies": [B], "translate": [0, 0, 5], "rotate": [0, 20, 0], "pivot": "center", "bed": True})
    assert abs(low(doc, B) - 5) < 1e-6
    doc2, B2 = _doc_with(m3.Manifold.cube((40, 30, 20)), "block.stl")
    doc2.add_feature("move", {"bodies": [B2], "translate": [0, 0, 0], "rotate": [30, 0, 0], "pivot": "center"})
    assert low(doc2, B2) < -1


def test_support_fins_remove_and_hand_walls():
    pytest.importorskip("py_mini_racer")
    d, a = _doc_with(m3.Manifold.cube([30, 20, 10], True).rotate([0, 40, 0]), "tip.stl")
    _, pr = d.preview("supportfins", {"body": a, "output": "join", "options": {}})
    ids = list(pr["pieces"])
    assert [p["kind"] for p in pr["pieces"].values()] == ["prop", "prop", "prop", "pad"]
    rem = [{k: pr["pieces"][ids[1]][k] for k in ("kind", "c", "size")}]
    _, pr2 = d.preview("supportfins", {"body": a, "output": "join", "options": {}, "removed": rem})
    assert pr2["ghosts"] == [ids[1]] and "1 removed" in pr2["info"]
    d.add_feature("supportfins", {"body": a, "output": "join", "options": {}, "removed": rem})
    assert abs(d.state.body(a).info()["volume"] - 6543.19) < 7    # the web engine gives the same
    ell = (m3.Manifold.cube([10, 20, 35]) + m3.Manifold.cube([30, 20, 8]).translate([0, 0, 27])).translate([0, 100, 0])
    d2, e = _doc_with(ell, "ell.stl")
    _, ph = d2.preview("supportfins", {"body": e, "output": "join", "options": {},
                                       "walls": [{"shape": "line", "points": [[24, 104, 27], [24, 116, 27]]}]})
    assert "1 drawn by hand" in ph["info"]
    assert any(p["kind"] == "hand" for p in ph["pieces"].values())


def test_parameters():
    from stleditor.params import evaluate, resolve_params, ExprError
    assert evaluate("=2*(3+4)^2 - sqrt(16) + sin(30) + max(1,5,3) % 3") == 96.5
    assert evaluate("round(2.5) + floor(-1.5) + atan(1)") == 3 - 2 + 45
    r = resolve_params([{"name": "b", "expr": "a*2"}, {"name": "a", "expr": "10"}, {"name": "c", "expr": "c+1"}])
    assert r["values"] == {"a": 10, "b": 20} and "depends on itself" in r["errors"]["c"]
    with pytest.raises(ExprError):
        evaluate("(-8)^0.5")
    d = Document()
    d.set_params([{"name": "w", "expr": "40"}, {"name": "h", "expr": "w/4 + 5"}])
    d.add_feature("primitive", {"shape": "box", "length": "=w", "width": 20, "height": "=h*2", "plane": "XY",
                                "offset": [0, 0, "=h"], "rotate": [0, 0, 0], "anchor": "base", "operation": "new",
                                "targets": [], "body_name": "Box"})

    def size():
        v, _ = list(d.state.bodies.values())[0].mesh()
        return [round(float(x), 4) for x in (v.max(0) - v.min(0))] + [round(float(v[:, 2].min()), 4)]
    assert size() == [40, 20, 30, 15]
    d.set_params([{"name": "w", "expr": "60"}, {"name": "h", "expr": "w/4 + 5"}])
    assert size() == [60, 20, 40, 20]
    d.undo()
    assert size() == [40, 20, 30, 15]
    d.set_params([{"name": "w", "expr": "40"}])
    assert 'no parameter "h"' in d._results[-1]["error"]
    with pytest.raises(GeometryError):
        d.set_params([{"name": "sin", "expr": "1"}])


def test_parameters_saved(tmp_path):
    d = Document()
    d.set_params([{"name": "w", "expr": "12"}])
    d.add_feature("primitive", {"shape": "box", "length": "=w", "width": 5, "height": 5, "plane": "XY", "offset": [0, 0, 0],
                                "rotate": [0, 0, 0], "anchor": "base", "operation": "new", "targets": [], "body_name": "Box"})
    d.save(str(tmp_path / "p.stlx"))
    d2 = Document.open(str(tmp_path / "p.stlx"))
    assert d2.params[0]["name"] == "w"
    assert abs(list(d2.state.bodies.values())[0].info()["volume"] - 300) < 1e-6


def test_support_fins_extras():
    pytest.importorskip("py_mini_racer")
    from stleditor import supportfins as SF
    # a Support Fins release that broke an extra is vendored without it (the daily update still
    # goes through); what is there has to work
    have = json.load(open(os.path.join(SF.BUNDLED, "VERSION.json"))).get("features", [])
    feats = SF.features()
    assert sorted(k for k, v in feats.items() if v) == sorted(have)
    ell = m3.Manifold.cube([10, 20, 35]) + m3.Manifold.cube([30, 20, 8]).translate([0, 0, 27])
    d, e = _doc_with(ell, "ell.stl")
    v, f = d.state.body(e).mesh()
    if feats["orient"]:
        sg = SF.suggest(v, f, {})
        assert len(sg["candidates"]) == 3 and round(sg["candidates"][0]["overArea"]) == 0
        st = SF.strength(v, f, {}, [0, 0, 1])
        assert st["posture"] == "weak" and st["load"]["quality"] == "poor" and st["strongest"]["quality"] == "good"
    walls = ([{"shape": "gusset", "points": [[10, 10, 8], [18, 10, 27]], "normal": [1, 0, 0]}] if feats["gussets"] else []) + \
        ([{"shape": "anchor", "points": [[10, 10, 15]], "normal": [1, 0, 0]}] if feats["anchors"] else [])
    _, pr = d.preview("supportfins", {"body": e, "output": "join", "options": {}, "walls": walls})
    assert all(w["ok"] for w in pr.get("walls", []))
    if len(walls) == 2:
        assert "2 drawn by hand" in pr["info"]
        assert abs(pr["support"]["volume"] - 2014.4) < 4     # the web engine gives the same


PLATFORM_CASES = [
    # (sys.platform, machine, release asset, layout of the zip / tar.gz, the app's top-level item)
    ("win32", "AMD64", "STL-Editor-Windows-portable.zip",
     {"STL Editor/STL Editor.exe": b"new exe", "STL Editor/_internal/x.pyd": b"lib"}, "STL Editor"),
    ("darwin", "arm64", "STL-Editor-macOS-arm64.zip",
     {"STL Editor.app/Contents/MacOS/STL Editor": b"new exe", "STL Editor.app/Contents/Info.plist": b"plist"},
     "STL Editor.app"),
    ("linux", "x86_64", "STL-Editor-Linux-x86_64.tar.gz",
     {"STL Editor/STL Editor": b"new exe", "STL Editor/_internal/x.so": b"lib"}, "STL Editor"),
]


def _make_archive(path, files):
    import io
    import tarfile
    import zipfile as zf
    if str(path).endswith(".tar.gz"):
        with tarfile.open(path, "w:gz") as t:
            for name, data in files.items():
                info = tarfile.TarInfo(name)
                info.size = len(data)
                t.addfile(info, io.BytesIO(data))
    else:
        with zf.ZipFile(path, "w") as w:
            for name, data in files.items():
                w.writestr(name, data)


@pytest.mark.parametrize("plat,machine,asset,files,top", PLATFORM_CASES)
def test_one_click_update_download(tmp_path, monkeypatch, plat, machine, asset, files, top):
    """The app downloads its system's release archive, checks its SHA-256 and unpacks it."""
    import functools
    import hashlib
    import http.server
    import threading
    from stleditor import updates as U
    monkeypatch.setattr(sys, "platform", plat)
    monkeypatch.setattr(U.platform, "machine", lambda: machine)
    z = tmp_path / "serve" / asset
    z.parent.mkdir()
    _make_archive(z, files)
    digest = hashlib.sha256(z.read_bytes()).hexdigest()
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(http.server.SimpleHTTPRequestHandler,
                                                                                directory=str(z.parent)))
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    url = f"http://127.0.0.1:{srv.server_address[1]}/{asset}"
    app_dir = tmp_path / "app"
    app_dir.mkdir()
    monkeypatch.setattr(U, "install_mode", lambda: "portable")
    monkeypatch.setattr(U, "app_dir", lambda: str(app_dir))
    monkeypatch.setattr(U, "WORK", str(tmp_path / "work"))
    rel = {"tag_name": "v9.9.9", "assets": [{"name": asset, "browser_download_url": url, "size": z.stat().st_size,
                                             "digest": "sha256:" + digest}]}
    monkeypatch.setattr(U, "_release", lambda: rel)
    U._install()
    try:
        assert U.INSTALL["state"] == "ready", U.INSTALL
        assert U.INSTALL["src"] == os.path.join(str(tmp_path / "work" / "new"), top)
        assert os.path.isdir(U.INSTALL["src"])
        rel["assets"][0]["digest"] = "sha256:" + "0" * 64
        U._install()
        assert U.INSTALL["state"] == "error" and "checksum" in U.INSTALL["error"]
    finally:
        srv.shutdown()


# ------------------------------------------------------------------ per-system folders
def _system(monkeypatch, plat, home, **env):
    """Pretend to be `plat` with `home` as the user's folder and the given environment variables."""
    from stleditor import paths as P
    monkeypatch.setattr(sys, "platform", plat)
    for k in ("APPDATA", "LOCALAPPDATA", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME"):
        monkeypatch.delenv(k, raising=False)
    monkeypatch.setenv("HOME", str(home))
    monkeypatch.setenv("USERPROFILE", str(home))
    for k, v in env.items():
        monkeypatch.setenv(k, v)
    return P


def test_windows_folders_unchanged(tmp_path, monkeypatch):
    roaming = r"C:\Users\ggnn1\AppData\Roaming"
    local = r"C:\Users\ggnn1\AppData\Local"
    P = _system(monkeypatch, "win32", tmp_path, APPDATA=roaming, LOCALAPPDATA=local)
    assert P.config_dir() == os.path.join(roaming, "STL Editor")
    assert P.data_dir() == os.path.join(local, "STL Editor")
    assert P.log_dir() == os.path.join(local, "STL Editor")
    if os.name == "nt":  # the exact folders the installed app has always used
        assert P.config_dir() == r"C:\Users\ggnn1\AppData\Roaming\STL Editor"
        assert P.data_dir() == r"C:\Users\ggnn1\AppData\Local\STL Editor"


def test_mac_folders(tmp_path, monkeypatch):
    P = _system(monkeypatch, "darwin", tmp_path)
    assert P.config_dir() == os.path.join(str(tmp_path), "Library", "Application Support", "STL Editor")
    assert P.data_dir() == os.path.join(str(tmp_path), "Library", "Application Support", "STL Editor")
    assert P.log_dir() == os.path.join(str(tmp_path), "Library", "Logs", "STL Editor")


def test_linux_folders_default_to_home(tmp_path, monkeypatch):
    P = _system(monkeypatch, "linux", tmp_path)
    assert P.config_dir() == os.path.join(str(tmp_path), ".config", "STL Editor")
    assert P.data_dir() == os.path.join(str(tmp_path), ".local", "share", "STL Editor")
    assert P.log_dir() == os.path.join(str(tmp_path), ".local", "state", "STL Editor")


def test_linux_folders_follow_xdg(tmp_path, monkeypatch):
    P = _system(monkeypatch, "linux", tmp_path, XDG_CONFIG_HOME=str(tmp_path / "c"),
                XDG_DATA_HOME=str(tmp_path / "d"), XDG_STATE_HOME=str(tmp_path / "s"))
    assert P.config_dir() == os.path.join(str(tmp_path / "c"), "STL Editor")
    assert P.data_dir() == os.path.join(str(tmp_path / "d"), "STL Editor")
    assert P.log_dir() == os.path.join(str(tmp_path / "s"), "STL Editor")
    _system(monkeypatch, "linux", tmp_path, XDG_CONFIG_HOME="")   # an empty variable counts as unset
    assert P.config_dir() == os.path.join(str(tmp_path), ".config", "STL Editor")


@pytest.mark.parametrize("plat,machine,expected", [
    ("win32", "AMD64", "STL-Editor-Windows-portable.zip"),
    ("darwin", "arm64", "STL-Editor-macOS-arm64.zip"),
    ("darwin", "x86_64", "STL-Editor-macOS-x86_64.zip"),
    ("linux", "x86_64", "STL-Editor-Linux-x86_64.tar.gz"),
    ("linux", "aarch64", None),       # no release yet: the update check points to the website
    ("darwin", "i386", None),
])
def test_release_asset_per_system(monkeypatch, plat, machine, expected):
    from stleditor import updates as U
    monkeypatch.setattr(sys, "platform", plat)
    monkeypatch.setattr(U.platform, "machine", lambda: machine)
    assert U.asset_for() == expected


def test_windows_asset_name_unchanged():
    from stleditor import updates as U
    assert U.ASSET == "STL-Editor-Windows-portable.zip"


@pytest.mark.parametrize("plat,rel", [
    ("win32", "STL Editor/STL Editor.exe"),
    ("darwin", "STL Editor.app/Contents/MacOS/STL Editor"),
    ("linux", "STL Editor/STL Editor"),
])
def test_payload_found_per_system(tmp_path, monkeypatch, plat, rel):
    from stleditor import updates as U
    monkeypatch.setattr(sys, "platform", plat)
    f = tmp_path / rel
    f.parent.mkdir(parents=True)
    f.write_bytes(b"x")
    assert U.find_payload(str(tmp_path)) == os.path.join(str(tmp_path), rel.split("/")[0])


def test_app_dir_is_the_bundle_on_mac(monkeypatch):
    from stleditor import updates as U
    monkeypatch.setattr(sys, "platform", "darwin")
    monkeypatch.setattr(sys, "frozen", True, raising=False)
    monkeypatch.setattr(sys, "executable", "/Applications/STL Editor.app/Contents/MacOS/STL Editor")
    assert U.app_dir() == "/Applications/STL Editor.app"


def test_app_dir_is_the_folder_on_linux(monkeypatch):
    from stleditor import updates as U
    monkeypatch.setattr(sys, "platform", "linux")
    monkeypatch.setattr(sys, "frozen", True, raising=False)
    monkeypatch.setattr(sys, "executable", "/opt/STL Editor/STL Editor")
    assert U.app_dir() == "/opt/STL Editor"


def test_no_release_asset_points_to_website(monkeypatch, tmp_path):
    from stleditor import updates as U
    monkeypatch.setattr(sys, "platform", "linux")
    monkeypatch.setattr(U.platform, "machine", lambda: "aarch64")
    monkeypatch.setattr(U, "install_mode", lambda: "portable")
    monkeypatch.setattr(U, "app_dir", lambda: str(tmp_path))
    monkeypatch.setattr(U, "_release", lambda: pytest.fail("must not fetch a release with no asset"))
    U._install()
    assert U.INSTALL["state"] == "error"
    assert "website" in U.INSTALL["error"]


def test_messages_do_not_say_windows_off_windows(monkeypatch, tmp_path):
    from stleditor import updates as U
    monkeypatch.setattr(sys, "platform", "darwin")
    monkeypatch.setattr(U, "install_mode", lambda: None)
    U._install()
    assert U.INSTALL["state"] == "error" and "Windows" not in U.INSTALL["error"]
    monkeypatch.setattr(U, "install_mode", lambda: "portable")
    monkeypatch.setattr(U, "app_dir", lambda: str(tmp_path))
    monkeypatch.setattr(U, "_writable", lambda folder: False)
    U._install()
    assert "Windows" not in U.INSTALL["error"]


def test_posix_restart_helper(tmp_path, monkeypatch):
    import subprocess
    import threading
    from stleditor import server
    from stleditor import updates as U
    monkeypatch.setattr(sys, "platform", "linux")
    monkeypatch.setattr(U, "WORK", str(tmp_path / "work"))
    monkeypatch.setattr(U, "app_dir", lambda: str(tmp_path / "app"))
    calls = []
    monkeypatch.setattr(subprocess, "Popen", lambda args, **kw: calls.append((args, kw)))
    monkeypatch.setattr(server, "shutdown", lambda app: None)
    monkeypatch.setattr(threading, "Timer", lambda *a, **k: type("T", (), {"start": lambda self: None})())

    class App:
        discard_on_exit = True

    U.restart(App(), copy_from=str(tmp_path / "new" / "STL Editor"))
    args, kw = calls[0]
    assert kw.get("start_new_session") is True
    assert args[0] == "sh"
    script = open(args[1], encoding="utf-8").read()
    assert "\r" not in script and "kill -0" in script and "rm -rf" in script
    assert str(tmp_path / "new" / "STL Editor") in args
    calls.clear()
    monkeypatch.setattr(sys, "frozen", False, raising=False)
    U.restart(App(), copy_from=None)           # a git checkout: relaunch main.py with this Python
    args, _ = calls[0]
    assert sys.executable in args and any(a.endswith("main.py") for a in args)


def test_font_folders_include_linux_user_fonts(tmp_path, monkeypatch):
    from stleditor import meshtools as MT
    monkeypatch.setattr(sys, "platform", "linux")
    monkeypatch.setenv("HOME", str(tmp_path))
    monkeypatch.setenv("XDG_DATA_HOME", str(tmp_path / "data"))
    (tmp_path / "data" / "fonts").mkdir(parents=True)
    (tmp_path / ".local" / "share" / "fonts").mkdir(parents=True)
    dirs = MT.font_dirs()
    assert str(tmp_path / "data" / "fonts") in dirs


def test_font_files_are_found_in_subfolders(tmp_path):
    from stleditor import meshtools as MT
    (tmp_path / "truetype" / "dejavu").mkdir(parents=True)
    (tmp_path / "truetype" / "dejavu" / "DejaVuSans.ttf").write_bytes(b"x")
    (tmp_path / "readme.txt").write_text("x")
    found = [os.path.basename(p) for p in MT.font_files([str(tmp_path)])]
    assert found == ["DejaVuSans.ttf"]


def test_webview_gui_per_system():
    import main as M
    assert M.gui_candidates("win32") == ["edgechromium"]
    assert M.gui_candidates("darwin") == ["cocoa"]
    assert M.gui_candidates("linux") == ["qt", "gtk"]


def test_browser_per_system():
    import main as M
    app = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
    assert M.browser_exe("darwin", exists=lambda p: p == app, which=lambda c: None) == app
    assert M.browser_exe("darwin", exists=lambda p: False, which=lambda c: None) is None
    assert M.browser_exe("linux", exists=lambda p: False, which=lambda c: "/usr/bin/chromium-browser" if c == "chromium-browser" else None) == "/usr/bin/chromium-browser"
    assert M.browser_exe("linux", exists=lambda p: False, which=lambda c: None) is None
    msedge = r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"
    assert M.browser_exe("win32", exists=lambda p: p == msedge, which=lambda c: None) == msedge


# ------------------------------------------------------------------ robustness


# ------------------------------------------------------------------ robustness
class _FakeAutosaver:
    last_time = None

    def __init__(self):
        self.cleared = 0

    def clear(self):
        self.cleared += 1


def test_failed_open_keeps_the_open_document(block, tmp_path):
    from stleditor import server
    doc, B = block
    app = server.App()
    app.doc, app.autosaver = doc, _FakeAutosaver()
    bad = tmp_path / "broken.stl"
    bad.write_bytes(b"\x01\x02\x03")
    for p in (str(bad), str(tmp_path / "notes.txt")):
        with pytest.raises(Exception):
            server.r_open(app, {"path": p, "force": True})
        assert app.doc is doc and len(doc.features) == 1
    assert app.autosaver.cleared == 0, "the autosave copy of the open work is kept"
    # a mesh that opens: nothing to undo or redo
    good = stl_file(tmp_path, m3.Manifold.cube((5, 5, 5)), "good.stl")
    server.r_open(app, {"path": good, "force": True})
    assert app.doc is not doc and not app.doc._undo and not app.doc._redo and not app.doc.dirty
    assert app.autosaver.cleared == 1


def test_autosave_keeps_parameters(tmp_path, monkeypatch):
    from stleditor import autosave, server
    monkeypatch.setattr(autosave, "ROOT", str(tmp_path / "autosave"))
    app = server.App()
    app.doc.set_params([{"name": "w", "expr": "12"}])
    app.doc.add_feature("primitive", {"shape": "box", "length": "=w", "width": 5, "height": 5, "plane": "XY",
                                      "offset": [0, 0, 0], "rotate": [0, 0, 0], "anchor": "base", "operation": "new",
                                      "targets": [], "body_name": "Box"})
    a = autosave.Autosaver(app, interval=3600)
    try:
        assert a.save_now()
        assert not a.save_now(), "nothing changed"
        app.doc.set_params([{"name": "w", "expr": "20"}])
        assert a.save_now(), "a parameter change is a change"
        back = autosave.load(a.session)
        assert back.params == [{"name": "w", "expr": "20", "note": ""}]
        assert abs(list(back.state.bodies.values())[0].info()["volume"] - 500) < 1e-6
    finally:
        a.shutdown()


def test_parameter_functions_take_the_right_number_of_numbers():
    from stleditor.params import evaluate, resolve_params, ExprError
    for text, msg in (("=round(2.5, 1)", "round() takes 1 number"), ("=sqrt(4, 2)", "sqrt() takes 1 number"),
                      ("=sin()", "sin() needs a number"), ("=constructor(1)", 'there\'s no function "constructor"')):
        with pytest.raises(ExprError, match=re.escape(msg)):
            evaluate(text)
    assert evaluate("=max(1, 7, 3) + min(4)") == 11
    r = resolve_params([{"name": "constructor", "expr": "3"}, {"name": "b", "expr": "constructor * 2"},
                        {"name": "r", "expr": "round(1, 2)"}])
    assert r["values"] == {"constructor": 3, "b": 6} and r["errors"]["r"] == "round() takes 1 number"
    d = Document()
    d.set_params([{"name": "r", "expr": "round(1, 2)"}])
    assert d.param_values()["errors"]["r"] == "round() takes 1 number"


def test_cache_keys_stay_the_same_size(block):
    doc, B = block
    for i in range(6):
        doc.add_feature("hole", {"points": [[5 + 5 * i, 15, 20]], "normal": [0, 0, 1], "diameter": 2, "through": True})
    keys = [h for h, _ in doc._cache]
    assert len(keys) == 7 and all(len(h) == 64 for h in keys) and len(set(keys)) == 7
    assert vol(doc, B) == pytest.approx(40 * 30 * 20 - 6 * math.pi * 20, rel=0.02)


def test_pattern_leaves_no_copy_tools(block):
    doc, B = block
    h, _ = doc.add_feature("hole", {"points": [[5, 15, 20]], "normal": [0, 0, 1], "diameter": 4, "through": True})
    p, _ = doc.add_feature("pattern", {"source": "features", "features": [h["id"]], "kind": "linear",
                                       "direction": [0, 1, 0], "count": 3, "spacing": 5})
    assert [k for k in doc.state.tools if k.startswith(p["id"] + ".")] == []


def _raw(port, method, path, headers=None, body=None):
    import http.client
    c = http.client.HTTPConnection("127.0.0.1", port, timeout=10)
    try:
        c.request(method, path, body=body, headers=headers or {})
        r = c.getresponse()
        return r.status, r.read()
    finally:
        c.close()


def test_http_api_only_answers_its_own_pages():
    from stleditor import server
    app = server.App()
    httpd, port = server.serve(app, 0)
    try:
        js = {"Content-Type": "application/json"}
        assert _raw(port, "GET", "/api/state")[0] == 200
        assert _raw(port, "GET", "/api/state", {"Host": f"localhost:{port}"})[0] == 200
        assert _raw(port, "GET", "/api/state", {"Host": f"evil.example:{port}"})[0] == 403, "DNS rebinding"
        assert _raw(port, "POST", "/api/new", {**js, "Host": "evil.example"}, b'{"force": true}')[0] == 403
        assert _raw(port, "POST", "/api/new", {**js, "Origin": "https://evil.example"}, b'{"force": true}')[0] == 403
        assert _raw(port, "POST", "/api/new", {"Content-Type": "text/plain", "Sec-Fetch-Site": "cross-site"},
                    b'{"force": true}')[0] == 403
        assert _raw(port, "POST", "/api/new", {**js, "Origin": f"http://127.0.0.1:{port}", "Sec-Fetch-Site": "same-origin"},
                    b'{"force": true}')[0] == 200
        # the UI's own files, and nothing outside them
        assert _raw(port, "GET", "/js/api.js")[0] == 200
        for p in ("/../main.py", "/..%2fmain.py", "/%2e%2e/main.py", "/../web-other/x.js"):
            assert _raw(port, "GET", p)[0] == 404, p
    finally:
        httpd.shutdown()


def test_status_answers_during_a_long_operation():
    import threading
    from stleditor import server
    app = server.App()
    httpd, port = server.serve(app, 0)
    held, release = threading.Event(), threading.Event()

    def busy():
        with app.lock:
            held.set()
            release.wait(30)
    t = threading.Thread(target=busy, daemon=True)
    t.start()
    try:
        held.wait(5)
        code, body = _raw(port, "POST", "/api/status", {"Content-Type": "application/json"}, b"{}")
        assert code == 200 and json.loads(body)["ok"]
    finally:
        release.set()
        t.join(5)
        httpd.shutdown()


def _lsk(doc, origin, entities, normal=(0, 0, 1)):
    return doc.add_feature("sketch", {"plane": {"origin": list(origin), "normal": list(normal)},
                                      "entities": entities})[0]["id"]


def _lrect(i, x, y, w, h):
    return {"id": i, "type": "rect", "x": x, "y": y, "w": w, "h": h}


def test_loft_frustum_offset_and_middle_profile():
    """Square frustum 10 -> 4 over 20 mm: (100 + 4*49 + 16) * 20 / 6 = 1040 mm³. An offset top gives the
    same volume; a 10 -> 4 -> 10 loft over two 10 mm steps is two frustums of 520 mm³ each."""
    doc = Document()
    a = _lsk(doc, (0, 0, 0), [_lrect("a", -5, -5, 10, 10)])
    b = _lsk(doc, (0, 0, 20), [_lrect("b", -2, -2, 4, 4)])
    f, _ = doc.add_feature("loft", {"profiles": [a, b]})
    assert doc.state.body(f["id"]).info()["volume"] == pytest.approx(1040, rel=1e-3)
    doc = Document()
    a = _lsk(doc, (0, 0, 0), [_lrect("a", -5, -5, 10, 10)])
    b = _lsk(doc, (0, 0, 20), [_lrect("b", 1, 1, 4, 4)])
    f, _ = doc.add_feature("loft", {"profiles": [a, b]})
    assert doc.state.body(f["id"]).info()["volume"] == pytest.approx(1040, rel=1e-3)
    doc = Document()
    a = _lsk(doc, (0, 0, 0), [_lrect("a", -5, -5, 10, 10)])
    m = _lsk(doc, (0, 0, 10), [_lrect("m", -2, -2, 4, 4)])
    c = _lsk(doc, (0, 0, 20), [_lrect("c", -5, -5, 10, 10)])
    f, _ = doc.add_feature("loft", {"profiles": [a, m, c]})
    assert doc.state.body(f["id"]).info()["volume"] == pytest.approx(1040, rel=1e-3)


def test_loft_and_sweep_refuse_bad_input():
    doc = Document()
    a = _lsk(doc, (0, 0, 0), [_lrect("a", -5, -5, 10, 10)])
    with pytest.raises(GeometryError, match="at least two"):
        doc.add_feature("loft", {"profiles": [a]})
    open_sk = _lsk(doc, (0, 0, 20), [{"id": "l", "type": "line", "p1": [0, 0], "p2": [5, 0]}])
    with pytest.raises(GeometryError, match="closed shape"):
        doc.add_feature("loft", {"profiles": [a, open_sk]})
    crossing = _lsk(doc, (0, 0, 0), [{"id": "pl", "type": "polyline", "pts": [[0, 0], [10, 10], [10, 0], [0, 10]],
                                      "closed": False}])
    prof = _lsk(doc, (0, 0, 0), [_lrect("p", -1, -1, 2, 2)])
    with pytest.raises(GeometryError, match="crosses itself"):
        doc.add_feature("sweep", {"profile": prof, "path": crossing})


def test_sweep_straight_and_bent_paths():
    """A 2 x 2 mm square swept 10 mm along X is 40 mm³; two collinear 10 mm legs make 80 mm³; an L of two
    10 mm legs is 80 mm³ too (the bend is mitred, so the outer corner is filled)."""
    doc = Document()
    prof = _lsk(doc, (0, 0, 0), [_lrect("p", -1, -1, 2, 2)])
    line = {"id": "l", "type": "line", "p1": [0, 0], "p2": [10, 0]}
    path = _lsk(doc, (0, 0, 0), [line])
    f, _ = doc.add_feature("sweep", {"profile": prof, "path": path})
    assert doc.state.body(f["id"]).info()["volume"] == pytest.approx(40, rel=1e-3)
    path = _lsk(doc, (0, 0, 0), [{"id": "l1", "type": "line", "p1": [0, 0], "p2": [10, 0]},
                                 {"id": "l2", "type": "line", "p1": [10, 0], "p2": [20, 0]}])
    f, _ = doc.add_feature("sweep", {"profile": prof, "path": path})
    assert doc.state.body(f["id"]).info()["volume"] == pytest.approx(80, rel=1e-3)
    path = _lsk(doc, (0, 0, 0), [{"id": "l1", "type": "line", "p1": [0, 0], "p2": [10, 0]},
                                 {"id": "l2", "type": "line", "p1": [10, 0], "p2": [10, 10]}])
    f, _ = doc.add_feature("sweep", {"profile": prof, "path": path})
    assert doc.state.body(f["id"]).info()["volume"] == pytest.approx(80, rel=1e-3)


def test_sweep_collinear_legs_and_profile_stays_put():
    """Three collinear 10 mm legs of a 2 x 2 mm square: 120 mm³. A profile drawn 20 mm out from its
    sketch origin, around the path start (20, 0), still sits centred on that path start."""
    doc = Document()
    prof = _lsk(doc, (0, 0, 0), [_lrect("p", -1, -1, 2, 2)])
    path = _lsk(doc, (0, 0, 0), [{"id": "a", "type": "line", "p1": [0, 0], "p2": [10, 0]},
                                 {"id": "b", "type": "line", "p1": [10, 0], "p2": [20, 0]},
                                 {"id": "c", "type": "line", "p1": [20, 0], "p2": [30, 0]}])
    f, _ = doc.add_feature("sweep", {"profile": prof, "path": path})
    assert doc.state.body(f["id"]).info()["volume"] == pytest.approx(120, rel=1e-3)
    prof = _lsk(doc, (0, 0, 0), [_lrect("q", 19, -1, 2, 2)])
    path = _lsk(doc, (0, 0, 0), [{"id": "d", "type": "line", "p1": [20, 0], "p2": [20, 10]}])
    f, _ = doc.add_feature("sweep", {"profile": prof, "path": path})
    lo, hi = bbox(doc, f["id"])
    assert (lo[0] + hi[0]) / 2 == pytest.approx(20, abs=1e-3)
    assert doc.state.body(f["id"]).info()["volume"] == pytest.approx(40, rel=1e-3)


def test_thin_wall_check(tmp_path):
    """Hand-checked: a 20 x 20 x 0.5 slab is thinner than 0.8 mm; a 2 mm slab and a 10 mm cube aren't."""
    def check(man, name, mn=None):
        doc = Document()
        f, _ = doc.import_file(stl_file(tmp_path, man, name), place="keep")
        p = {"body": f["id"]}
        if mn is not None:
            p["min"] = mn
        _, res = doc.add_feature("thincheck", p)
        assert vol(doc, f["id"]) == pytest.approx(man.volume(), rel=1e-6), "reports only, changes nothing"
        return res
    ok = check(m3.Manifold.cube((10, 10, 10)), "cube.stl")
    assert ok["thin"]["count"] == 0 and ok["thin"]["min"] is None and not ok["warning"]
    assert check(m3.Manifold.cube((20, 20, 2)), "wall2.stl")["thin"]["count"] == 0
    r = check(m3.Manifold.cube((20, 20, 0.5)), "slab.stl")
    assert r["thin"]["count"] == 1 and r["warning"]
    assert r["thin"]["min"] == pytest.approx(0.5, abs=0.02)
    assert r["thin"]["spots"][0][2] == pytest.approx(0.25, abs=0.05)
    # the same 2 mm wall is thin when the minimum is 2.5 mm
    assert check(m3.Manifold.cube((20, 20, 2)), "wall2b.stl", 2.5)["thin"]["min"] == pytest.approx(2.0, abs=0.02)
    # a 0.5 mm pin standing on a cube is found, up on the pin
    pin = m3.Manifold.cube((10, 10, 10)) + m3.Manifold.cylinder(5, 0.25, 0.25, 32).translate((5, 5, 10))
    r = check(pin, "pin.stl")["thin"]
    assert r["count"] == 1 and r["min"] == pytest.approx(0.5, abs=0.02)
    x, y, z = r["spots"][0]
    assert x == pytest.approx(5, abs=0.3) and y == pytest.approx(5, abs=0.3) and 10 < z < 15
    with pytest.raises(GeometryError):
        check(m3.Manifold.cube((10, 10, 10)), "bad.stl", 0)


def test_fit_test_plate_volume():
    """Hand-checked: plate − hole cylinders + peg cylinders on top (same height as thickness)."""
    D, start, step, count, T, gap = 8.0, 0.05, 0.05, 3, 3.0, 4.0
    clearances = [start + i * step for i in range(count)]
    max_hole = D + clearances[-1]
    L = count * max_hole + (count + 1) * gap
    W = max_hole + D + 3 * gap
    expected = L * W * T
    expected -= sum(math.pi * ((D + c) / 2) ** 2 * T for c in clearances)
    expected += count * math.pi * (D / 2) ** 2 * T
    doc = Document()
    f, res = doc.add_feature("fittest", {"diameter": D, "start": start, "step": step,
                                         "count": count, "thickness": T, "gap": gap})
    assert vol(doc, f["id"]) == pytest.approx(expected, rel=0.01)
    assert "0.05" in res["info"] and "0.15" in res["info"]
    with pytest.raises(GeometryError):
        doc.add_feature("fittest", {"diameter": 0, "start": start, "step": step,
                                    "count": count, "thickness": T, "gap": gap})


def test_snap_fit_and_living_hinge(block):
    doc, B = block  # 40 x 30 x 20 block, corner at the origin
    top = {"origin": [0, 0, 20], "normal": [0, 0, 1], "xdir": [1, 0, 0]}
    clip = {"plane": top, "x": 5, "y": 5, "length": 12, "width": 6, "thickness": 1.5, "hook_height": 1.5}
    # new body: arm 1.5 x 6 x (12 + 0.5 sunk) plus hook 1.5 x 6 x 1.5 = 112.5 + 13.5
    f, _ = doc.add_feature("snapfit", {**clip, "operation": "new"})
    assert vol(doc, f["id"]) == pytest.approx(126, rel=1e-6)
    # joined to the block: 0.5 mm of the foot is already inside it (1.5 x 6 x 0.5 = 4.5)
    v0 = vol(doc, B)
    doc.add_feature("snapfit", {**clip, "targets": [B]})
    assert vol(doc, B) - v0 == pytest.approx(126 - 4.5, rel=1e-6)
    with pytest.raises(GeometryError):
        doc.add_feature("snapfit", {**clip, "hook_height": 12})
    # living hinge: 1 mm slot across the 30 mm width, 19.5 mm deep (leaves a 0.5 mm web)
    v0 = vol(doc, B)
    doc.add_feature("hinge", {"x": 10, "slot": 1, "thickness": 0.5})
    assert v0 - vol(doc, B) == pytest.approx(1 * 30 * 19.5, rel=1e-6)
    assert len(doc.state.bodies) == 2  # the block stays one piece (joined by the web)
