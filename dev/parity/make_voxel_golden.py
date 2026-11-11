"""Generate rust/tests/parity/voxel_golden.json from 1.x voxel steps.

Covers solid_occupancy, mesh_occupancy, edt, gaussian, dilate, erode,
fill_holes, marching_cubes and field_to_manifold. Grids stay small; a few
larger ones use the size-limit form (sha256/count or sum/min/max/sample).

Usage:
  py -3.14 dev/parity/make_voxel_golden.py
  py -3.14 dev/parity/make_voxel_golden.py --check
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

import numpy as np
from scipy import ndimage
from skimage import measure

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "dev" / "parity"))

import manifold3d as m3  # noqa: E402

from stleditor import geometry as G  # noqa: E402
import stats as parity_stats  # noqa: E402

MODELS = ROOT / "rust" / "tests" / "models"
GOLDEN = ROOT / "rust" / "tests" / "parity" / "voxel_golden.json"
VOXEL_DIR = ROOT / "rust" / "tests" / "parity" / "voxel"

# Store full grids only when under this many cells; larger ones use the size-limit form.
FULL_GRID_CELLS = 8000
SAMPLE_STRIDE = 97


def _dumps(obj) -> str:
    return json.dumps(obj, indent=1, sort_keys=True, ensure_ascii=False) + "\n"


def _norm_newlines(s: str) -> str:
    return s.replace("\r\n", "\n").replace("\r", "\n")


def _origin_list(origin) -> list[float]:
    return [float(x) for x in np.asarray(origin, float).reshape(-1)]


def _pack_bool(grid: np.ndarray) -> dict:
    g = np.asarray(grid, dtype=bool)
    flat = np.ascontiguousarray(g.reshape(-1), dtype=np.uint8)
    n = int(flat.size)
    if n <= FULL_GRID_CELLS:
        return {
            "shape": list(g.shape),
            "data": [bool(x) for x in flat.tolist()],
        }
    return {
        "shape": list(g.shape),
        "sha256": hashlib.sha256(flat.tobytes()).hexdigest(),
        "count": int(flat.sum()),
    }


def _pack_float(field: np.ndarray) -> dict:
    f = np.asarray(field, dtype=np.float64)
    flat = np.ascontiguousarray(f.reshape(-1))
    n = int(flat.size)
    if n <= FULL_GRID_CELLS:
        return {
            "shape": list(f.shape),
            "dtype": "f64",
            "data": [float(x) for x in flat.tolist()],
        }
    sample = [float(x) for x in flat[::SAMPLE_STRIDE].tolist()]
    return {
        "shape": list(f.shape),
        "dtype": "f64",
        "sum": float(flat.sum()),
        "min": float(flat.min()) if n else 0.0,
        "max": float(flat.max()) if n else 0.0,
        "sample": sample,
    }


def _pack_f32_field(field: np.ndarray) -> dict:
    f = np.asarray(field, dtype=np.float32)
    flat = np.ascontiguousarray(f.reshape(-1))
    n = int(flat.size)
    if n <= FULL_GRID_CELLS:
        return {
            "shape": list(f.shape),
            "dtype": "f32",
            "data": [float(x) for x in flat.tolist()],
        }
    sample = [float(x) for x in flat[::SAMPLE_STRIDE].tolist()]
    return {
        "shape": list(f.shape),
        "dtype": "f32",
        "sum": float(flat.astype(np.float64).sum()),
        "min": float(flat.min()) if n else 0.0,
        "max": float(flat.max()) if n else 0.0,
        "sample": sample,
    }


def _hand_occ() -> np.ndarray:
    """Small 7×7×7 occupancy with a hollow cube shell and one interior hole."""
    n = 7
    occ = np.zeros((n, n, n), dtype=bool)
    occ[1:6, 1:6, 1:6] = True
    occ[2:5, 2:5, 2:5] = False
    occ[3, 3, 3] = True  # island inside the cavity
    return occ


def _hand_field() -> np.ndarray:
    """Small signed field for marching cubes / field_to_manifold (positive inside)."""
    n = 9
    field = np.zeros((n, n, n), dtype=np.float32)
    c = (n - 1) / 2.0
    for i in range(n):
        for j in range(n):
            for k in range(n):
                r = np.sqrt((i - c) ** 2 + (j - c) ** 2 + (k - c) ** 2)
                field[i, j, k] = np.float32(2.5 - r)
    return field


def build() -> dict:
    records: dict[str, dict] = {}

    cube = m3.Manifold.cube((10.0, 10.0, 10.0), center=False)
    occ, origin = G.solid_occupancy(cube, 2.0)
    records["solid_occ_cube_p2"] = {
        "op": "solid_occupancy",
        "input": {"model": "primitive:cube10", "pitch": 2.0},
        "output": {"origin": _origin_list(origin), "grid": _pack_bool(occ)},
    }

    # Larger solid occupancy stored in size-limit form.
    cube40 = m3.Manifold.cube((40.0, 40.0, 40.0), center=False)
    occ40, origin40 = G.solid_occupancy(cube40, 2.0)
    records["solid_occ_cube40_p2"] = {
        "op": "solid_occupancy",
        "input": {"model": "primitive:cube40", "pitch": 2.0},
        "output": {"origin": _origin_list(origin40), "grid": _pack_bool(occ40)},
    }

    v, f = G.read_stl_bytes((MODELS / "holed_block.stl").read_bytes())
    mocc, morigin = G.mesh_occupancy(v, f, 2.0)
    records["mesh_occ_holed_p2"] = {
        "op": "mesh_occupancy",
        "input": {"model": "holed_block.stl", "pitch": 2.0},
        "output": {"origin": _origin_list(morigin), "grid": _pack_bool(mocc)},
    }

    open_v, open_f = G.read_stl_bytes((MODELS / "open_box.stl").read_bytes())
    oocc, oorigin = G.mesh_occupancy(open_v, open_f, 1.5)
    records["mesh_occ_open_box_p1_5"] = {
        "op": "mesh_occupancy",
        "input": {"model": "open_box.stl", "pitch": 1.5},
        "output": {"origin": _origin_list(oorigin), "grid": _pack_bool(oocc)},
    }

    hand = _hand_occ()
    edt = ndimage.distance_transform_edt(hand)
    records["edt_hand"] = {
        "op": "edt",
        "input": {"grid": _pack_bool(hand)},
        "output": {"field": _pack_float(edt)},
    }

    # Larger EDT in size-limit form.
    big = np.zeros((24, 24, 24), dtype=bool)
    big[4:20, 4:20, 4:20] = True
    big[8:16, 8:16, 8:16] = False
    edt_big = ndimage.distance_transform_edt(big)
    records["edt_box24"] = {
        "op": "edt",
        "input": {"grid": _pack_bool(big)},
        "output": {"field": _pack_float(edt_big)},
    }

    src_f32 = hand.astype(np.float32)
    gauss = ndimage.gaussian_filter(src_f32, sigma=0.8)
    records["gaussian_hand_s0_8"] = {
        "op": "gaussian",
        "input": {"field": _pack_f32_field(src_f32), "sigma": 0.8},
        "output": {"field": _pack_f32_field(gauss)},
    }

    ball = ndimage.generate_binary_structure(3, 1)
    dil = ndimage.binary_dilation(hand, structure=ball, iterations=1)
    records["dilate_hand_1"] = {
        "op": "dilate",
        "input": {"grid": _pack_bool(hand), "iterations": 1},
        "output": {"grid": _pack_bool(dil)},
    }
    ero = ndimage.binary_erosion(hand, structure=ball, iterations=1, border_value=0)
    records["erode_hand_1"] = {
        "op": "erode",
        "input": {"grid": _pack_bool(hand), "iterations": 1},
        "output": {"grid": _pack_bool(ero)},
    }
    filled = ndimage.binary_fill_holes(hand)
    records["fill_holes_hand"] = {
        "op": "fill_holes",
        "input": {"grid": _pack_bool(hand)},
        "output": {"grid": _pack_bool(filled)},
    }

    field = _hand_field()
    pitch = 1.0
    verts, faces, *_ = measure.marching_cubes(field, level=0.0, spacing=(pitch, pitch, pitch))
    records["mc_sphere_field"] = {
        "op": "marching_cubes",
        "input": {"field": _pack_f32_field(field), "level": 0.0, "spacing": pitch},
        "output": {
            "verts": [[float(x) for x in row] for row in np.asarray(verts, float)],
            "faces": [[int(a), int(b), int(c)] for a, b, c in np.asarray(faces)],
        },
    }

    origin_f = np.array([0.0, 0.0, 0.0], float)
    man = G._field_to_manifold(field, origin_f, pitch, level=0.0)
    mv, mf = G.Body("x", solid=man).mesh()
    records["field_to_manifold_sphere"] = {
        "op": "field_to_manifold",
        "input": {
            "field": _pack_f32_field(field),
            "origin": _origin_list(origin_f),
            "pitch": pitch,
            "level": 0.0,
        },
        "output": {"stats": parity_stats.mesh_stats(mv, mf)},
    }

    return {k: records[k] for k in sorted(records)}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="exit 1 if golden differs")
    args = ap.parse_args()

    golden = build()
    text = _dumps(golden)

    if args.check:
        existing = GOLDEN.read_text(encoding="utf-8") if GOLDEN.exists() else ""
        if _norm_newlines(existing) != _norm_newlines(text):
            print(f"make_voxel_golden: MISMATCH {GOLDEN.relative_to(ROOT)}", file=sys.stderr)
            return 1
        print(f"make_voxel_golden: ok ({len(golden)} cases)")
        return 0

    GOLDEN.parent.mkdir(parents=True, exist_ok=True)
    VOXEL_DIR.mkdir(parents=True, exist_ok=True)
    GOLDEN.write_text(text, encoding="utf-8", newline="\n")
    print(f"wrote {GOLDEN.relative_to(ROOT)} ({len(golden)} cases)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
