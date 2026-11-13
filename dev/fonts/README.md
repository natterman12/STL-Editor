# Bundled symbols font

`rust/crates/stl-app/assets/fonts/symbols.ttf` is a small SIL OFL subset renamed
**STL Editor Symbols**. It is the last proportional fallback after Selawik, for
marks Selawik (and egui's defaults) lack.

| Code | Glyph | Source face |
|------|-------|-------------|
| U+2192 | → | Noto Sans Math |
| U+2033 | ″ | Noto Sans Math |
| U+2665 | ♥ | Noto Sans Symbols 2 |
| U+26A0 | ⚠ | Noto Sans Symbols 2 |
| U+2713 | ✓ | Noto Sans Symbols 2 |
| U+2714 | ✔ | Noto Sans Symbols 2 |

## Rebuild

1. Download `NotoSansMath-Regular.ttf` and `NotoSansSymbols2-Regular.ttf` from
   [Google Fonts](https://github.com/google/fonts) (`ofl/notosansmath`,
   `ofl/notosanssymbols2`) or the matching [notofonts](https://github.com/notofonts)
   release into an empty folder **outside** this repo.
2. `pip install fonttools` (once).
3. From the repo root:

```powershell
py -3.14 dev/fonts/rebuild_symbols.py --source-dir C:\path\to\noto-fonts
```

That overwrites `rust/crates/stl-app/assets/fonts/symbols.ttf`. Keep
`Symbols-OFL.txt` next to it.
