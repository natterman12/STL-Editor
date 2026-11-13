#!/usr/bin/env python3
"""Rebuild rust/crates/stl-app/assets/fonts/symbols.ttf.

The bundled fallback is a tiny SIL OFL subset renamed "STL Editor Symbols":
  U+2192 →  rightwards arrow   (from Noto Sans Math)
  U+2033 ″  double prime       (from Noto Sans Math)
  U+2665 ♥  black heart        (from Noto Sans Symbols 2)
  U+26A0 ⚠  warning sign       (from Noto Sans Symbols 2)
  U+2713 ✓  check mark         (from Noto Sans Symbols 2)
  U+2714 ✔  heavy check mark   (from Noto Sans Symbols 2)

Coverage check (Noto Sans / Noto Sans Symbols / Noto Sans Math / Noto Emoji):
  - Noto Sans Math has → and ″ (and ♥ ✓, but not ⚠ ✔).
  - Noto Sans Symbols 2 has ♥ ⚠ ✓ ✔ (not → ″).
  - Together they cover all six; no other Noto face is required.

Download the two source Regular TTFs from Google Fonts (ofl/notosansmath and
ofl/notosanssymbols2) or the matching notofonts GitHub release assets into a
folder outside the repo, then:

  py -3.14 dev/fonts/rebuild_symbols.py --source-dir PATH

Requires fontTools (pip install fonttools).
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from fontTools import subset
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont

ARROW_PRIME = [0x2192, 0x2033]
MARKS = [0x2665, 0x26A0, 0x2713, 0x2714]
ALL = ARROW_PRIME + MARKS

MATH_FILE = "NotoSansMath-Regular.ttf"
SYMBOLS2_FILE = "NotoSansSymbols2-Regular.ttf"

DROP = [
    "GSUB",
    "GPOS",
    "GDEF",
    "BASE",
    "JSTF",
    "MATH",
    "STAT",
    "vhea",
    "vmtx",
    "VORG",
    "hdmx",
    "LTSH",
    "DSIG",
    "meta",
    "COLR",
    "CPAL",
    "CBDT",
    "CBLC",
    "sbix",
    "SVG ",
]

REPO_OUT = (
    Path(__file__).resolve().parents[2]
    / "rust"
    / "crates"
    / "stl-app"
    / "assets"
    / "fonts"
    / "symbols.ttf"
)


def subset_font(inp: Path, outp: Path, unicodes: list[int]) -> None:
    options = subset.Options()
    options.layout_features = []
    options.name_IDs = ["*"]
    options.name_legacy = True
    options.name_languages = ["*"]
    options.notdef_outline = True
    options.recalc_bounds = True
    options.recalc_timestamp = False
    options.drop_tables += DROP
    font = subset.load_font(str(inp), options)
    for tag in list(font.keys()):
        if tag in DROP:
            del font[tag]
    subsetter = subset.Subsetter(options)
    subsetter.populate(unicodes=unicodes)
    subsetter.subset(font)
    subset.save_font(font, str(outp), options)


def rename_family(font: TTFont) -> None:
    for rec in font["name"].names:
        if rec.nameID in (1, 4, 16):
            rec.string = "STL Editor Symbols"
        elif rec.nameID == 6:
            rec.string = "STLEditorSymbols"
        elif rec.nameID == 3:
            rec.string = (
                "STL Editor Symbols: subset of Noto Sans Math + Noto Sans Symbols 2"
            )


def build(math_path: Path, symbols2_path: Path, out_path: Path, work: Path) -> None:
    work.mkdir(parents=True, exist_ok=True)
    out_math = work / "math_part.ttf"
    out_sym = work / "sym_part.ttf"
    subset_font(math_path, out_math, ARROW_PRIME)
    subset_font(symbols2_path, out_sym, MARKS)

    base = TTFont(str(out_sym))
    donor = TTFont(str(out_math))
    for tag in DROP:
        if tag in base:
            del base[tag]
        if tag in donor:
            del donor[tag]

    donor_cmap = donor.getBestCmap()
    donor_gs = donor.getGlyphSet()
    unicode_cmaps = [t for t in base["cmap"].tables if hasattr(t, "cmap")]
    if not unicode_cmaps:
        raise SystemExit("symbols2 subset has no cmap table")

    added: list[str] = []
    for cp in ARROW_PRIME:
        src_name = donor_cmap[cp]
        # Keep the source glyph name (arrowright / second) — short and stable.
        dst_name = src_name
        pen = TTGlyphPen(None)
        donor_gs[src_name].draw(pen)
        base["glyf"][dst_name] = pen.glyph()
        base["hmtx"][dst_name] = donor["hmtx"][src_name]
        for t in unicode_cmaps:
            t.cmap[cp] = dst_name
        added.append(dst_name)

    order = [g for g in base.getGlyphOrder() if g != ".notdef"]
    for name in added:
        if name not in order:
            order.append(name)
    base.setGlyphOrder([".notdef"] + order)
    base["maxp"].numGlyphs = len(base.getGlyphOrder())
    rename_family(base)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    base.save(str(out_path))

    font = TTFont(str(out_path))
    cmap = font.getBestCmap() or {}
    missing = [f"U+{c:04X}" for c in ALL if c not in cmap]
    if missing:
        raise SystemExit(f"built font missing glyphs: {missing}")
    size = out_path.stat().st_size
    print(f"wrote {out_path} ({size} bytes)")
    print("glyphs:", ", ".join(f"U+{c:04X}->{cmap[c]}" for c in ALL))


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--source-dir",
        type=Path,
        required=True,
        help=f"Folder with {MATH_FILE} and {SYMBOLS2_FILE} (outside the repo)",
    )
    ap.add_argument(
        "--out",
        type=Path,
        default=REPO_OUT,
        help=f"Output TTF (default: {REPO_OUT})",
    )
    args = ap.parse_args()
    math_path = args.source_dir / MATH_FILE
    symbols2_path = args.source_dir / SYMBOLS2_FILE
    for p in (math_path, symbols2_path):
        if not p.is_file() or p.stat().st_size < 10_000:
            print(f"missing or too small: {p}", file=sys.stderr)
            return 1
    work = args.source_dir / "work"
    build(math_path, symbols2_path, args.out, work)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
