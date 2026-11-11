"""Generate rust/tests/parity/offset_corpus.json from 1.x sketch.offset.

Fixed-seed random offset inputs in five groups: three large groups (a few hundred
cases each), `closed_self_crossing` (~100 closed self-crossing paths), and
`open_curves` (~150 sampled arcs / sine paths, chained mixes, and loose-line
rings), with 1.x's results. The Rust test `offset_corpus` requires every group's
`baseline` to be 0 and a full match on every case.

Usage:
  py -3.14 dev/parity/make_offset_corpus.py          # write corpus
  py -3.14 dev/parity/make_offset_corpus.py --check  # exit 1 if corpus differs

Uses shapely from dev/parity/requirements-golden.txt (same as the other golden
scripts). On Nathan's PC run with: C:\\Program Files\\Python314\\python.exe
"""
from __future__ import annotations

import argparse
import json
import math
import random
import sys
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))

from stleditor import sketch as S  # noqa: E402
from stleditor.geometry import GeometryError  # noqa: E402

OUT = ROOT / "rust" / "tests" / "parity" / "offset_corpus.json"

SEED = 20261110
CASES_PER_GROUP = 300
CLOSED_SELF_CROSSING_CASES = 100
OPEN_CURVES_CASES = 150

# Written into the corpus as-is (not copied from a previous file).
# every group is at 0: Rust must match 1.x on all cases.
BASELINES = {
    "open_lines": 0,
    "messy_large": 0,
    "closed_polylines": 0,
    "closed_self_crossing": 0,
    "open_curves": 0,
}


def _dumps(obj) -> str:
    return json.dumps(obj, indent=1, sort_keys=True, ensure_ascii=False) + "\n"


def _norm_newlines(s: str) -> str:
    return s.replace("\r\n", "\n").replace("\r", "\n")


def _r(v, nd=9):
    if isinstance(v, (float, np.floating)):
        x = float(v)
        if not math.isfinite(x):
            return x
        return round(x, nd)
    if isinstance(v, (list, tuple)):
        return [_r(a, nd) for a in v]
    if isinstance(v, dict):
        return {k: _r(v[k], nd) for k in sorted(v)}
    if isinstance(v, np.ndarray):
        return _r(v.tolist(), nd)
    return v


def _new_id_factory():
    n = 0

    def nid():
        nonlocal n
        n += 1
        return f"n{n}"

    return nid


def _run_offset(ents: list, ids: list, d: float):
    try:
        return _r(S.offset(ents, list(ids), float(d), _new_id_factory()))
    except (GeometryError, ValueError) as e:
        return {"error": str(e)}


def _chain_ok(pts: list[list[float]]) -> bool:
    if len(pts) < 3:
        return False
    for a, b in zip(pts, pts[1:]):
        if a[0] == b[0] and a[1] == b[1]:
            return False
        if math.hypot(b[0] - a[0], b[1] - a[1]) < 1e-6:
            return False
    return True


def _lines_from_chain(pts: list[list[float]]) -> tuple[list[dict], list[str]]:
    ents: list[dict] = []
    ids: list[str] = []
    for j in range(len(pts) - 1):
        eid = f"l{j}"
        ents.append({"id": eid, "type": "line", "p1": list(pts[j]), "p2": list(pts[j + 1])})
        ids.append(eid)
    return ents, ids


def gen_open_lines(rng: random.Random, n: int) -> list[dict]:
    """Open chains as separate connected lines; |d| <= 1."""
    out: list[dict] = []
    attempts = 0
    while len(out) < n and attempts < n * 40:
        attempts += 1
        nseg = rng.randint(2, 6)
        pts = [[rng.uniform(-20.0, 20.0), rng.uniform(-20.0, 20.0)]]
        for _ in range(nseg):
            last = pts[-1]
            pts.append([last[0] + rng.uniform(-10.0, 10.0), last[1] + rng.uniform(-10.0, 10.0)])
        if not _chain_ok(pts):
            continue
        ents, ids = _lines_from_chain(pts)
        d = rng.choice((-1.0, 1.0)) * rng.uniform(0.05, 1.0)
        # Round inputs first so expected matches what Rust sees in the corpus file.
        ents_r, d_r = _r(ents), _r(d)
        expected = _run_offset(ents_r, ids, d_r)
        out.append({"entities": ents_r, "ids": ids, "d": d_r, "expected": expected})
    if len(out) < n:
        raise RuntimeError(f"open_lines: only produced {len(out)}/{n} cases")
    return out


def gen_messy_large(rng: random.Random, n: int) -> list[dict]:
    """Messy line networks (crossings, T-joins, short zigzags) with large |d|."""
    out: list[dict] = []
    attempts = 0
    while len(out) < n and attempts < n * 40:
        attempts += 1
        ents: list[dict] = []
        ids: list[str] = []
        j = 0

        def add_line(p1, p2):
            nonlocal j
            if math.hypot(p2[0] - p1[0], p2[1] - p1[1]) < 0.2:
                return
            eid = f"l{j}"
            j += 1
            ents.append({"id": eid, "type": "line", "p1": list(p1), "p2": list(p2)})
            ids.append(eid)

        # mostly loose crossing segments; occasionally a short zigzag chain
        for _ in range(rng.randint(6, 12)):
            add_line(
                [rng.uniform(-40.0, 40.0), rng.uniform(-40.0, 40.0)],
                [rng.uniform(-40.0, 40.0), rng.uniform(-40.0, 40.0)],
            )
        if rng.random() < 0.35:
            x, y = rng.uniform(-25.0, 25.0), rng.uniform(-25.0, 25.0)
            for _ in range(rng.randint(3, 5)):
                nx = x + rng.uniform(-8.0, 8.0)
                ny = y + rng.uniform(-8.0, 8.0)
                add_line([x, y], [nx, ny])
                x, y = nx, ny
        if len(ids) < 4:
            continue
        d = rng.choice((-1.0, 1.0)) * rng.uniform(2.0, 12.0)
        ents_r, d_r = _r(ents), _r(d)
        expected = _run_offset(ents_r, ids, d_r)
        out.append({"entities": ents_r, "ids": ids, "d": d_r, "expected": expected})
    if len(out) < n:
        raise RuntimeError(f"messy_large: only produced {len(out)}/{n} cases")
    return out


def _simple_ring(rng: random.Random) -> list[list[float]] | None:
    n = rng.randint(4, 10)
    cx, cy = rng.uniform(-10.0, 10.0), rng.uniform(-10.0, 10.0)
    r0 = rng.uniform(4.0, 18.0)
    angs = sorted(rng.uniform(0.0, 2.0 * math.pi) for _ in range(n))
    # keep angular gaps so the ring does not fold on itself
    for a, b in zip(angs, angs[1:] + [angs[0] + 2.0 * math.pi]):
        if b - a < 0.15:
            return None
    pts: list[list[float]] = []
    for a in angs:
        r = r0 * rng.uniform(0.75, 1.25)
        pts.append([cx + r * math.cos(a), cy + r * math.sin(a)])
    # reject near-zero area
    area = 0.0
    for i in range(n):
        x0, y0 = pts[i]
        x1, y1 = pts[(i + 1) % n]
        area += x0 * y1 - x1 * y0
    if abs(area) < 8.0:
        return None
    return pts


def gen_closed_polylines(rng: random.Random, n: int) -> list[dict]:
    """Closed polylines: grow and shrink."""
    out: list[dict] = []
    attempts = 0
    while len(out) < n and attempts < n * 80:
        attempts += 1
        pts = _simple_ring(rng)
        if pts is None:
            continue
        grow = len(out) % 2 == 0
        if grow:
            d = rng.uniform(0.2, 4.0)
        else:
            d = -rng.uniform(0.2, 3.0)
        ents = [{"id": "p0", "type": "polyline", "closed": True, "pts": pts}]
        ids = ["p0"]
        ents_r, d_r = _r(ents), _r(d)
        expected = _run_offset(ents_r, ids, d_r)
        out.append({"entities": ents_r, "ids": ids, "d": d_r, "expected": expected})
    if len(out) < n:
        raise RuntimeError(f"closed_polylines: only produced {len(out)}/{n} cases")
    return out


def _xform(pts: list[list[float]], rng: random.Random) -> list[list[float]]:
    """Translate, rotate, and scale a polyline in-place-style copy."""
    ang = rng.uniform(0.0, 2.0 * math.pi)
    ca, sa = math.cos(ang), math.sin(ang)
    s = rng.uniform(0.5, 2.5)
    dx, dy = rng.uniform(-20.0, 20.0), rng.uniform(-20.0, 20.0)
    out: list[list[float]] = []
    for x, y in pts:
        xr, yr = s * (ca * x - sa * y), s * (sa * x + ca * y)
        out.append([xr + dx, yr + dy])
    return out


def _bow_tie(rng: random.Random) -> list[list[float]]:
    a = rng.uniform(2.0, 10.0)
    b = rng.uniform(2.0, 10.0)
    # Classic self-crossing quad; close by repeating the first point.
    return _xform([[-a, -b], [a, b], [-a, b], [a, -b], [-a, -b]], rng)


def _figure_eight(rng: random.Random) -> list[list[float]]:
    rx = rng.uniform(2.0, 9.0)
    ry = rng.uniform(2.0, 9.0)
    n = rng.randint(6, 10)
    pts: list[list[float]] = []
    # Two full loops through the origin (lemniscate-style): upper then lower.
    for i in range(n + 1):
        t = 2.0 * math.pi * i / n
        pts.append([rx * math.sin(t), ry * (1.0 - math.cos(t))])
    for i in range(1, n + 1):
        t = 2.0 * math.pi * i / n
        pts.append([-rx * math.sin(t), -ry * (1.0 - math.cos(t))])
    return _xform(pts, rng)


def _star_crossing(rng: random.Random) -> list[list[float]]:
    # Star polygon {n/k}: closed self-crossing when gcd(n, k) == 1 and k > 1.
    n = rng.choice((5, 7, 8, 9))
    ks = [k for k in range(2, (n - 1) // 2 + 1) if math.gcd(n, k) == 1]
    if not ks:
        ks = [2]
    k = rng.choice(ks)
    r = rng.uniform(4.0, 16.0)
    a0 = rng.uniform(0.0, 2.0 * math.pi)
    verts = [
        [r * math.cos(a0 + 2.0 * math.pi * i / n), r * math.sin(a0 + 2.0 * math.pi * i / n)]
        for i in range(n)
    ]
    pts = [verts[(i * k) % n] for i in range(n)]
    pts.append(pts[0])
    return _xform([[p[0], p[1]] for p in pts], rng)


def _closed_self_crossing_ok(pts: list[list[float]]) -> bool:
    """True when the closed path is not a simple ring (shapely is_ring is false)."""
    from shapely.geometry import LineString

    if len(pts) < 4:
        return False
    if not _chain_ok(pts):
        return False
    g = LineString([(float(x), float(y)) for x, y in pts])
    return bool(g.is_closed) and (not bool(g.is_ring))


def _offset_distance(rng: random.Random) -> float:
    sign = rng.choice((-1.0, 1.0))
    if rng.random() < 0.5:
        return sign * rng.uniform(0.05, 1.0)
    return sign * rng.uniform(2.0, 12.0)


def gen_closed_self_crossing(rng: random.Random, n: int) -> list[dict]:
    """Closed self-crossing paths (figure eight, bow tie, star) as lines or a polyline.

    Raw shapely geometry has is_ring false, so 1.x's loose-line path uses offset_curve
    (after noding/merge, some shapes split into rings and buffer instead).
    """
    makers = (_bow_tie, _figure_eight, _star_crossing)
    out: list[dict] = []
    attempts = 0
    while len(out) < n and attempts < n * 80:
        attempts += 1
        pts = makers[len(out) % len(makers)](rng)
        if not _closed_self_crossing_ok(pts):
            continue
        d = _offset_distance(rng)
        as_lines = len(out) % 2 == 0
        if as_lines:
            ents, ids = _lines_from_chain(pts)
        else:
            # Open polyline with first==last so it stays on the loose / offset_curve path
            # (closed:True would take Polygon.buffer instead).
            ents = [{"id": "p0", "type": "polyline", "closed": False, "pts": pts}]
            ids = ["p0"]
        ents_r, d_r = _r(ents), _r(d)
        expected = _run_offset(ents_r, ids, d_r)
        out.append({"entities": ents_r, "ids": ids, "d": d_r, "expected": expected})
    if len(out) < n:
        raise RuntimeError(f"closed_self_crossing: only produced {len(out)}/{n} cases")
    return out


def _small_d(rng: random.Random) -> float:
    return rng.choice((-1.0, 1.0)) * rng.uniform(0.05, 1.0)


def _sampled_arc_pts(rng: random.Random) -> list[list[float]] | None:
    n = rng.randint(50, 500)
    cx, cy = rng.uniform(-20.0, 20.0), rng.uniform(-20.0, 20.0)
    r = rng.uniform(2.0, 16.0)
    a0 = rng.uniform(0.0, 2.0 * math.pi)
    sweep = rng.uniform(math.radians(40.0), math.radians(300.0))
    pts = [
        [cx + r * math.cos(a0 + sweep * i / (n - 1)), cy + r * math.sin(a0 + sweep * i / (n - 1))]
        for i in range(n)
    ]
    return pts if _chain_ok(pts) else None


def _sine_curve_pts(rng: random.Random) -> list[list[float]] | None:
    n = rng.randint(50, 500)
    amp = rng.uniform(1.0, 8.0)
    length = rng.uniform(20.0, 90.0)
    waves = rng.uniform(1.0, 4.0)
    pts0 = [
        [length * i / (n - 1), amp * math.sin(2.0 * math.pi * waves * i / (n - 1))]
        for i in range(n)
    ]
    pts = _xform(pts0, rng)
    return pts if _chain_ok(pts) else None


def _chained_mix_ents(rng: random.Random) -> tuple[list[dict], list[str]] | None:
    """Polyline + arc entity + line joined end to end."""
    npoly = rng.randint(3, 7)
    pts = [[rng.uniform(-15.0, 15.0), rng.uniform(-15.0, 15.0)]]
    for _ in range(npoly - 1):
        last = pts[-1]
        pts.append([last[0] + rng.uniform(-6.0, 6.0), last[1] + rng.uniform(-6.0, 6.0)])
    if not _chain_ok(pts):
        return None
    end = pts[-1]
    r = rng.uniform(3.0, 12.0)
    sweep_deg = rng.uniform(40.0, 160.0) * rng.choice((-1.0, 1.0))
    # Arc centre so the arc starts at `end` with a random tangent-ish start angle.
    a0_deg = rng.uniform(0.0, 360.0)
    a0 = math.radians(a0_deg)
    cx = end[0] - r * math.cos(a0)
    cy = end[1] - r * math.sin(a0)
    a1_deg = a0_deg + sweep_deg
    a1 = math.radians(a1_deg)
    arc_end = [cx + r * math.cos(a1), cy + r * math.sin(a1)]
    # Line continuing from the arc end.
    line_end = [
        arc_end[0] + rng.uniform(-10.0, 10.0),
        arc_end[1] + rng.uniform(-10.0, 10.0),
    ]
    if math.hypot(line_end[0] - arc_end[0], line_end[1] - arc_end[1]) < 0.5:
        return None
    ents = [
        {"id": "p0", "type": "polyline", "closed": False, "pts": pts},
        {"id": "a0", "type": "arc", "cx": cx, "cy": cy, "r": r, "a0": a0_deg, "a1": a1_deg},
        {"id": "l0", "type": "line", "p1": list(arc_end), "p2": line_end},
    ]
    return ents, ["p0", "a0", "l0"]


def _loose_line_ring(rng: random.Random) -> tuple[list[dict], list[str]] | None:
    """Closed ring as many short loose lines (first point repeated at the end)."""
    n = rng.randint(24, 120)
    cx, cy = rng.uniform(-10.0, 10.0), rng.uniform(-10.0, 10.0)
    r0 = rng.uniform(6.0, 20.0)
    pts: list[list[float]] = []
    for i in range(n):
        a = 2.0 * math.pi * i / n
        r = r0 * rng.uniform(0.92, 1.08)
        pts.append([cx + r * math.cos(a), cy + r * math.sin(a)])
    pts.append(list(pts[0]))
    if not _chain_ok(pts):
        return None
    area = 0.0
    for i in range(n):
        x0, y0 = pts[i]
        x1, y1 = pts[(i + 1) % n]
        area += x0 * y1 - x1 * y0
    if abs(area) < 20.0:
        return None
    return _lines_from_chain(pts)


def gen_open_curves(rng: random.Random, n: int) -> list[dict]:
    """Open sampled arcs/sines, chained polyline+arc+line mixes, and loose-line rings.

    Dense open paths use 50..500 points; all cases use small |d| of both signs.
    """
    out: list[dict] = []
    attempts = 0
    while len(out) < n and attempts < n * 80:
        attempts += 1
        kind = len(out) % 4
        if kind == 0:
            pts = _sampled_arc_pts(rng)
            if pts is None:
                continue
            ents = [{"id": "p0", "type": "polyline", "closed": False, "pts": pts}]
            ids = ["p0"]
        elif kind == 1:
            pts = _sine_curve_pts(rng)
            if pts is None:
                continue
            ents = [{"id": "p0", "type": "polyline", "closed": False, "pts": pts}]
            ids = ["p0"]
        elif kind == 2:
            mix = _chained_mix_ents(rng)
            if mix is None:
                continue
            ents, ids = mix
        else:
            ring = _loose_line_ring(rng)
            if ring is None:
                continue
            ents, ids = ring
        d = _small_d(rng)
        ents_r, d_r = _r(ents), _r(d)
        expected = _run_offset(ents_r, ids, d_r)
        out.append({"entities": ents_r, "ids": ids, "d": d_r, "expected": expected})
    if len(out) < n:
        raise RuntimeError(f"open_curves: only produced {len(out)}/{n} cases")
    return out


def build() -> dict:
    rng = random.Random(SEED)
    groups = {
        "open_lines": gen_open_lines(rng, CASES_PER_GROUP),
        "messy_large": gen_messy_large(rng, CASES_PER_GROUP),
        "closed_polylines": gen_closed_polylines(rng, CASES_PER_GROUP),
        # After the three large groups so their fixed-seed inputs stay unchanged.
        "closed_self_crossing": gen_closed_self_crossing(rng, CLOSED_SELF_CROSSING_CASES),
        # After the four existing groups so their seed stream stays unchanged.
        "open_curves": gen_open_curves(rng, OPEN_CURVES_CASES),
    }
    return {
        "baseline": dict(BASELINES),
        "cases_per_group": CASES_PER_GROUP,
        "groups": groups,
        "seed": SEED,
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="exit 1 if corpus differs")
    args = ap.parse_args()

    corpus = build()
    text = _dumps(corpus)
    sizes = {name: len(cases) for name, cases in corpus["groups"].items()}

    if args.check:
        existing = OUT.read_text(encoding="utf-8") if OUT.exists() else ""
        if _norm_newlines(existing) != _norm_newlines(text):
            print(f"make_offset_corpus: MISMATCH {OUT.relative_to(ROOT)}", file=sys.stderr)
            return 1
        print(f"make_offset_corpus: ok ({sizes})")
        return 0

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(text, encoding="utf-8", newline="\n")
    print(f"wrote {OUT.relative_to(ROOT)} ({sizes})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
