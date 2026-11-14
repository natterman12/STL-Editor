#!/usr/bin/env python3
"""Rebuild rust/crates/stl-app/assets/fonts/symbols.ttf.

The bundled fallback is a tiny SIL OFL subset renamed "STL Editor Symbols" (16 marks):
  Noto Sans Math:       U+2033 ″  U+2192 →  U+2212 −  U+2225 ∥  U+2299 ⊙  U+22A5 ⊥  U+2300 ⌀  U+25CE ◎
  Noto Sans Symbols 2:  U+2665 ♥  U+26A0 ⚠  U+2713 ✓  U+2714 ✔  U+2715 ✕  U+2717 ✗  U+1F512 🔒
  Noto Sans Symbols:    U+232F ⌯

Download the three source Regular TTFs (Google Fonts ofl/notosansmath, ofl/notosanssymbols2,
ofl/notosanssymbols, or the matching notofonts releases) into a folder outside the repo, then:

  py -3.14 dev/fonts/rebuild_symbols.py --source-dir PATH

Requires fontTools (pip install fonttools).
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

from fontTools import subset
from fontTools.pens.recordingPen import DecomposingRecordingPen
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont

# (source file, code points taken from it). The first is the base font the others merge into.
SOURCES = [
    ("NotoSansSymbols2-Regular.ttf", [0x2665, 0x26A0, 0x2713, 0x2714, 0x2715, 0x2717, 0x1F512]),
    ("NotoSansMath-Regular.ttf", [0x2033, 0x2192, 0x2212, 0x2225, 0x2299, 0x22A5, 0x2300, 0x25CE]),
    ("NotoSansSymbols-Regular.ttf", [0x232F]),
]
ALL = [cp for _, cps in SOURCES for cp in cps]

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
                "STL Editor Symbols: subset of Noto Sans Math + Noto Sans Symbols 2 + Noto Sans Symbols"
            )


def build(sources: list[tuple[Path, list[int]]], out_path: Path, work: Path) -> None:
    work.mkdir(parents=True, exist_ok=True)
    parts = []
    for i, (path, cps) in enumerate(sources):
        part = work / f"part{i}.ttf"
        subset_font(path, part, cps)
        parts.append((TTFont(str(part)), cps))
    base, _ = parts[0]
    for tag in DROP:
        if tag in base:
            del base[tag]
    unicode_cmaps = [t for t in base["cmap"].tables if hasattr(t, "cmap")]
    if not unicode_cmaps:
        raise SystemExit("base subset has no cmap table")

    added: list[str] = []
    for donor, cps in parts[1:]:
        donor_cmap = donor.getBestCmap()
        donor_gs = donor.getGlyphSet()
        for cp in cps:
            src_name = donor_cmap[cp]
            dst_name = src_name if src_name not in base.getGlyphOrder() else f"u{cp:04X}"
            # composites (Math's parallel is two copies of one bar) are drawn as plain outlines
            rec = DecomposingRecordingPen(donor_gs)
            donor_gs[src_name].draw(rec)
            pen = TTGlyphPen(None)
            rec.replay(pen)
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
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument(
        "--source-dir",
        type=Path,
        required=True,
        help="Folder with the three Noto source TTFs (outside the repo)",
    )
    ap.add_argument(
        "--out",
        type=Path,
        default=REPO_OUT,
        help=f"Output TTF (default: {REPO_OUT})",
    )
    args = ap.parse_args()
    sources = [(args.source_dir / name, cps) for name, cps in SOURCES]
    for p, _ in sources:
        if not p.is_file() or p.stat().st_size < 10_000:
            print(f"missing or too small: {p}", file=sys.stderr)
            return 1
    work = args.source_dir / "work"
    build(sources, args.out, work)
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
