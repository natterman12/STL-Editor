"""Engine tests: every main tool on simple solids with known volumes.

Run with:  python -m pytest -q tests
"""
import json
import math
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
