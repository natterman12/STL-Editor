"""Draws the STL Editor 2.0 icon and writes every file made from it, next to this script.

    py -3.14 make_icon.py        (needs Pillow)

The icon: a dark navy tile, a pale blue cube with one top corner cut away, and an orange line
along the edge of the cut. Small sizes get a heavier orange line, or it would vanish on a
taskbar. Run this again after changing the drawing, and commit what it writes:

    icon.svg          the drawing, for people (not used by the build)
    icon.ico          Windows exe icon (build.rs)
    icon.icns         Mac app icon (the bundle step in .github/workflows/rust.yml)
    icon-256.rgba     the window icon: 256 x 256 raw RGBA (main.rs; no PNG reader needed)
    icon-512.png      Linux AppImage
    ../../web/icon-32.png, icon-192.png, icon-512.png    the web page's tab, home-screen and
                      installed-app icons
    ../../web/icon-maskable.png    the same for phones that cut icons to their own shape: the
                      tile fills the square and the cube is smaller, so nothing is cut off
"""

from pathlib import Path

from PIL import Image, ImageDraw

HERE = Path(__file__).resolve().parent
WEB = HERE.parent.parent / "web"

# The drawing, on a 240 x 240 tile.
TILE = "#1b2a4a"
RADIUS = 52
FACES = [
    ("#a9c6f5", [(46.4, 162.5), (120, 205), (120, 162.5), (83.2, 141.25), (83.2, 98.75), (46.4, 77.5)]),
    ("#6f9be0", [(193.6, 162.5), (120, 205), (120, 162.5), (156.8, 141.25), (156.8, 98.75), (193.6, 77.5)]),
    ("#e9f1ff", [(120, 35), (193.6, 77.5), (156.8, 98.75), (120, 77.5), (83.2, 98.75), (46.4, 77.5)]),
    ("#5c88cf", [(120, 120), (83.2, 141.25), (83.2, 98.75), (120, 77.5)]),
    ("#93b4ea", [(120, 120), (156.8, 141.25), (156.8, 98.75), (120, 77.5)]),
    ("#cfe0fb", [(120, 120), (156.8, 141.25), (120, 162.5), (83.2, 141.25)]),
]
ORANGE = "#f07a3c"
LINE = [(156.8, 98.75), (120, 77.5), (83.2, 98.75)]


def line_width(size: int) -> float:
    """The orange line's width on the 240 tile, for an icon `size` pixels across."""
    if size >= 128:
        return 6.0
    if size >= 64:
        return 6.5
    if size >= 40:
        return 8.0
    if size >= 32:
        return 9.5
    if size >= 24:
        return 11.0
    return 13.0


def draw(size: int, maskable: bool = False) -> Image.Image:
    """The icon at `size` pixels, drawn larger and scaled down for smooth edges. `maskable`:
    a square tile with the cube at 72%, for launchers that cut icons to their own shape."""
    over = 8 if size <= 256 else 4
    n = size * over
    k = n / 240
    s = 0.72 if maskable else 1.0

    def at(p):
        return ((120 + (p[0] - 120) * s) * k, (120 + (p[1] - 120) * s) * k)

    img = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    if maskable:
        d.rectangle([0, 0, n, n], fill=TILE)
    else:
        d.rounded_rectangle([0, 0, n - 1, n - 1], radius=RADIUS * k, fill=TILE)
    for colour, points in FACES:
        d.polygon([at(p) for p in points], fill=colour)
    w = line_width(size) * k * s
    pts = [at(p) for p in LINE]
    d.line(pts, fill=ORANGE, width=round(w), joint="curve")
    for x, y in pts:  # round ends and a round corner
        d.ellipse([x - w / 2, y - w / 2, x + w / 2, y + w / 2], fill=ORANGE)
    return img.resize((size, size), Image.LANCZOS)


def svg() -> str:
    out = ['<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 240 240">']
    out.append(f'  <rect width="240" height="240" rx="{RADIUS}" fill="{TILE}"/>')
    for colour, points in FACES:
        pts = " ".join(f"{x:g},{y:g}" for x, y in points)
        out.append(f'  <polygon points="{pts}" fill="{colour}"/>')
    pts = " ".join(f"{x:g},{y:g}" for x, y in LINE)
    out.append(
        f'  <polyline points="{pts}" fill="none" stroke="{ORANGE}" stroke-width="{line_width(256):g}" '
        'stroke-linecap="round" stroke-linejoin="round"/>'
    )
    out.append("</svg>")
    return "\n".join(out) + "\n"


def main() -> None:
    (HERE / "icon.svg").write_text(svg(), encoding="utf-8", newline="\n")

    ico_sizes = [16, 20, 24, 32, 40, 48, 64, 256]
    ico = [draw(s) for s in ico_sizes]
    ico[-1].save(HERE / "icon.ico", format="ICO", sizes=[(s, s) for s in ico_sizes], append_images=ico[:-1])

    icns_sizes = [16, 32, 64, 128, 256, 512, 1024]
    icns = [draw(s) for s in icns_sizes]
    icns[-1].save(HERE / "icon.icns", format="ICNS", append_images=icns[:-1])

    (HERE / "icon-256.rgba").write_bytes(draw(256).tobytes())
    draw(512).save(HERE / "icon-512.png", optimize=True)
    draw(32).save(WEB / "icon-32.png", optimize=True)
    draw(192).save(WEB / "icon-192.png", optimize=True)
    draw(512).save(WEB / "icon-512.png", optimize=True)
    draw(512, maskable=True).save(WEB / "icon-maskable.png", optimize=True)

    for p in sorted(list(HERE.glob("icon*")) + list(WEB.glob("icon-*.png"))):
        print(f"{p.stat().st_size:>8}  {p.relative_to(HERE.parent.parent)}")


if __name__ == "__main__":
    main()
