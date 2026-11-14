# Bundled symbols font

`rust/crates/stl-app/assets/fonts/symbols.ttf` is a small SIL OFL subset renamed
**STL Editor Symbols**. It is the last proportional fallback after Selawik, for
marks Selawik (and egui's defaults) lack.

| Source face | Code points |
|-------------|-------------|
| Noto Sans Math | U+2033 ″, U+2192 →, U+2212 −, U+2225 ∥, U+2299 ⊙, U+22A5 ⊥, U+2300 ⌀, U+25CE ◎ |
| Noto Sans Symbols 2 | U+2665 ♥, U+26A0 ⚠, U+2713 ✓, U+2714 ✔, U+2715 ✕, U+2717 ✗, U+1F512 🔒 |
| Noto Sans Symbols | U+232F ⌯ |

## Rebuild

1. Download `NotoSansMath-Regular.ttf`, `NotoSansSymbols2-Regular.ttf` and
   `NotoSansSymbols-Regular.ttf` from [Google Fonts](https://github.com/google/fonts)
   (`ofl/notosansmath`, `ofl/notosanssymbols2`, `ofl/notosanssymbols`) or the matching
   [notofonts](https://github.com/notofonts) releases into an empty folder **outside**
   this repo (`dev/fonts/noto-src/` is git-ignored if you keep them there).
2. `pip install fonttools` (once).
3. From the repo root:

```powershell
py -3.14 dev/fonts/rebuild_symbols.py --source-dir C:\path\to\noto-fonts
```

That overwrites `rust/crates/stl-app/assets/fonts/symbols.ttf`. Keep
`Symbols-OFL.txt` next to it.
