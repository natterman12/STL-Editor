<div align="center">

<img src="web/icon.png" width="88" alt="">

# STL Editor

**CAD-style editing for STL files.**
Sketch on any face, extrude, cut, drill holes, add printable threads and split big parts to fit your printer. The meshes you download from Printables, Thingiverse or MakerWorld become editable solids.

[**Download for Windows**](https://github.com/natterman12/STL-Editor/releases/latest/download/STL-Editor-Windows-portable.zip) ·
[**Try it in your browser**](https://natterman12.github.io/STL-Editor/app/) ·
[Project website](https://natterman12.github.io/STL-Editor/) ·
[User guide](USER_GUIDE.md) ·
[Releases](https://github.com/natterman12/STL-Editor/releases)

![Windows 10/11](https://img.shields.io/badge/Windows-10%20%7C%2011-1f6fd1)
![Licence GPL-3.0](https://img.shields.io/badge/licence-GPL--3.0-2f855a)
![No install needed](https://img.shields.io/badge/install-not%20needed-66727f)
[![Web edition](https://img.shields.io/badge/web%20edition-open-e8763a)](https://natterman12.github.io/STL-Editor/app/)
![Latest release](https://img.shields.io/github/v/release/natterman12/STL-Editor?color=e8763a)
[![Support on Ko-fi](https://img.shields.io/badge/Ko--fi-support%20the%20project-ff5e5b?logo=kofi&logoColor=white)](https://ko-fi.com/natterman12)
[![Tests](https://github.com/natterman12/STL-Editor/actions/workflows/tests.yml/badge.svg)](https://github.com/natterman12/STL-Editor/actions/workflows/tests.yml)

<img src="docs/img/fit-to-bed.webp" alt="A tall vase cut into colour-coded pieces that fit a Bambu A1 mini, with the Fit to Bed panel open" width="100%">

</div>

---

## Why

Most CAD programs treat STL files as dead geometry. You can look at them but you can't really change them. Mesh tools such as Meshmixer let you sculpt and patch, but they don't let you sketch a rectangle on a face and cut a 4 mm pocket.

STL Editor sits in between. It loads a mesh, makes sure it's a clean watertight solid, and lets you work on it with real CAD tools:

- **sketches** with constraints and dimensions
- **extrude / revolve / cut**
- **holes** with presets for inserts, screws, nuts and magnets
- **threads** that actually print
- **booleans**, **mirror**, **pattern**, **split**, **hollow**

Every step goes into a **timeline**. Double-click an earlier step to change it and everything after it updates. Nothing is baked in until you export.

## What it does

| | |
|---|---|
| <img src="docs/img/planter-edit.webp" alt="A twisted vase STL turned into a planter"> | **Edit downloaded models.** This vase came in as a 200k-triangle STL. A sketched circle cut the planter pocket, then a drain hole and three magnet pockets went into the base, giving four timeline steps that all stay editable. |
| <img src="docs/img/project-outline.webp" alt="A ring traced from the vase's outline, with embossed text"> | **New in 1.3: trace, resize, fit.** *Project* drops a model's outline into a sketch: here, a holder ring that follows the twisted vase with 0.4 mm of clearance, plus a text label. *Resize Hole* measures any round hole or pin you click and changes its diameter (or fills it). *Combine › Cut* takes a clearance so printed parts fit, and *Offset Body* grows or shrinks a whole part. |
| <img src="docs/img/sketch-constraints.webp" alt="Sketch with dimensions and constraints"> | **Real sketches.** Lines, arcs, rectangles, circles, polygons and slots. Constraints include coincident, horizontal/vertical, parallel, perpendicular, equal, tangent, concentric, midpoint, symmetric and fix, plus driving dimensions. Change a number and the shape follows. Trim, offset and mirror are included. |
| <img src="docs/img/hole-presets.webp" alt="Hole tool with the heat-set insert preset"> | **Holes for real hardware.** Presets for heat-set inserts M2–M5, screw clearance, socket-head counterbores, hex nut traps and magnet pockets, all with a print tolerance. Click as many points as you like. |
| <img src="docs/img/threads.webp" alt="Printed M8 bolt and nut"> | **Printable threads.** Tapped holes or bolts/studs, ISO metric M3–M30 or any size, right or left hand, chamfered ends and a print clearance. An M8 bolt and nut made with the defaults screw together. |
| <img src="docs/img/section-view.webp" alt="Section view through the planter"> | **See inside.** Section view on X, Y or Z, plus measure and properties: size, volume, triangle count and print weight. |
| <img src="docs/img/tool-search.webp" alt="Toolbar tabs and the tool search"> | **A toolbar that stays out of the way.** Four tabs: Solid, Print, Supports and Mesh. Press `Ctrl+K` to find any tool by name. The toolbar shrinks to fit narrow windows instead of scrolling. |

### All tools

- **Create:** Sketch · Extrude (new / join / cut / intersect, one side / two sides / symmetric, through all, taper) · Revolve · Box · Cylinder · Sphere · Cone · Torus
- **Modify:** Hole · Resize Hole (measure and change, or fill, any round hole or pin) · Thread · Push/Pull · Combine (cut with clearance) · Offset Body · Split (with connectors) · Mirror (bodies *or* features) · Pattern (linear / circular, bodies or features) · Move/Rotate and Scale with on-part handles (rotated parts land on the bed) · Hollow
- **Print:**
  - Lay Flat: click a face, or let it pick the most stable side.
  - **Support Fins** ([Support Fins](https://github.com/gittrahan/support-fins) by gittrahan, built in): tip a part to its strong print orientation and add the breakaway fins and bed pad that hold up its overhangs, baked into the mesh. Every Support Fins setting is there (placement, tines, nozzle, sway braces for tall parts, cutouts...), plus *Draw Walls* to place walls by hand (straight, multi-section, curved, gussets off a side and anchor fins). *Suggest orientations* ranks the best ways to print the part, a strength check says whether the layers run along or across the load, and the support plastic is shown in grams. New Support Fins releases reach both editions automatically.
  - Fit to Bed: pick your printer and big parts are cut into pieces that fit, with pins, pin holes, filament-pin holes or magnet pockets at every seam. *Smart cut positions* put the seams where the part is thinnest.
  - Export STL / 3MF / OBJ and Send to Bambu Studio / OrcaSlicer.
- **Construct:** Offset Plane · Plane at Angle · 3-Point Plane · Midplane. Any flat face works as a plane too.
- **Parameters:** name values (`width = 40`, `height = width/4 + 5`) and use them in any number; change one and the whole timeline is rebuilt.
- **Mesh:** Repair (quick fix, or rebuild as one watertight solid) · Simplify · Remesh · Sculpt brushes (smooth / flatten / inflate / deflate) · Separate · To STEP (through SolidCraft)
- **Sketcher:** Line · Arc · Rectangle · Circle · Polygon · Slot · Text (any installed font) · Project (section or silhouette of the model, with offset) · Trim · Offset · Mirror · 11 constraints and driving dimensions
- **Everything else:** undo/redo · `.stlx` projects that keep the whole timeline · inch mode with fractions (`1 3/8`) · light and dark themes · autosave with crash recovery · update notice

## Install

1. Download **[STL-Editor-Windows-portable.zip](https://github.com/natterman12/STL-Editor/releases/latest/download/STL-Editor-Windows-portable.zip)** (about 65 MB).
2. Unzip it anywhere (Desktop, Documents, a USB stick) and run **STL Editor.exe**.
3. The app isn't code-signed yet, so Windows SmartScreen may say *"Windows protected your PC"*. Click **More info › Run anyway**.

Nothing gets installed and no admin rights are needed. Settings live in `%APPDATA%\STL Editor`. When a new release is posted here, the app shows a small notice with a download button.

### Web edition

**[Open STL Editor in your browser](https://natterman12.github.io/STL-Editor/app/)**: nothing to download, works on Windows, macOS, Linux, ChromeOS and tablets in a recent Chrome, Edge, Firefox or Safari. It's the same interface and the same `.stlx` project files. The geometry engine (Manifold compiled to WebAssembly) runs inside the page, so your models never leave your computer.

Every modelling tool works in the browser, including Hollow, Remesh, Sculpt and *Repair › Rebuild*. It keeps autosave copies for recovery and a recent-files list, saves straight back to the project you opened (Chrome and Edge), works offline after the first visit, and can be installed as an app that opens `.stlx` and mesh files. Only Send to Slicer and To STEP need the desktop app, because they start programs on your computer. The desktop app is also faster on very large meshes and allows finer voxel settings.

### Quick start

1. Drag an STL onto the window.
2. Double-click a flat face to sketch on it, and draw a rectangle or circle (type numbers while drawing for exact sizes).
3. Click **Finish & Extrude**, choose **Cut**, set the depth and press **OK**.
4. **File › Export STL / 3MF**, or **Send to Slicer**.

The full manual, with every tool and shortcut, is in [USER_GUIDE.md](USER_GUIDE.md).

## Run from source

You need Python 3.12 or newer on Windows (tested with 3.12, 3.13 and 3.14). The WebView2 runtime is already part of Windows 10/11.

```powershell
git clone https://github.com/natterman12/STL-Editor.git
cd STL-Editor
pip install -r requirements.txt
python main.py                      # desktop window
python main.py --browser            # or open it in your browser instead
```

`install.ps1` adds Start Menu and Desktop shortcuts, opens `.stlx` files with STL Editor, and adds *Edit in STL Editor* to the right-click menu of `.stl`, `.3mf` and `.obj` files (current user only).

The engine and the browser UI aren't tied to Windows: `python main.py --browser` should work on macOS and Linux too. Only Windows is tested so far. Reports and fixes are welcome.

Run the tests with `pip install pytest` and then `python -m pytest -q tests`.

### Build the portable zip

GitHub Actions builds it for every release tag. To build it yourself:

```powershell
pip install pyinstaller
powershell -ExecutionPolicy Bypass -File packaging\build_portable.ps1 -Repo natterman12/STL-Editor
```

The zip lands in `Documents\STL Editor - Share\`.

## How it works

```
 main.py ──► local engine (Python, 127.0.0.1 only) ◄──JSON/binary──► UI (three.js in a WebView2 window)
              │
              ├─ document.py   feature timeline, cached re-evaluation, undo/redo, .stlx projects
              ├─ features.py   every tool as a function of (state, parameters)
              ├─ geometry.py   mesh I/O, repair, voxel rebuild, hollow
              ├─ sketch.py     sketch entities → regions (Shapely)
              └─ printing.py   fit-to-bed, connectors, threads, lay flat, sculpt, remesh
```

- **Booleans:** [Manifold](https://github.com/elalish/manifold) gives exact, watertight results fast, even on meshes with hundreds of thousands of triangles.
- **Repair and voxel work:** trimesh, scikit-image and SciPy.
- **Sketch constraints:** solved in the browser with a small Gauss-Newton solver (`web/js/constraints.js`).
- **Web edition:** the same UI runs the JavaScript engine in `web/engine/` in a Web Worker instead of the Python one. It answers the same API routes, uses [manifold-3d](https://www.npmjs.com/package/manifold-3d) (WebAssembly) for booleans, [fflate](https://github.com/101arrowz/fflate) for `.stlx` and 3MF and [opentype.js](https://opentype.js.org) for text. `packaging/build_web.py` publishes it to `docs/app/` for GitHub Pages.
- **Privacy:** nothing leaves your computer. The only network request is an optional check of this repository's *latest release* page.

## Limits

- STL files have no real edges, so **fillet and chamfer on mesh edges aren't available**. For blocky mechanical parts, *To STEP* hands the mesh to SolidCraft.
- Bodies that aren't watertight show and export fine, but need **Repair** before you can edit them.
- Hollow and *Repair › Rebuild* use a voxel grid. Pick *Quality › Fine* for small details.

## Contributing

Bug reports, ideas and pull requests are welcome. [Open an issue](https://github.com/natterman12/STL-Editor/issues/new/choose): the forms ask for the model file and steps, which makes mesh bugs much faster to fix. [CONTRIBUTING.md](CONTRIBUTING.md) covers setup, the code layout, how to add a tool and how releases are made. Tests run on GitHub for every push and pull request, and pushing a `v*` tag builds and publishes the portable zip automatically. Ideas already on the list: SVG/DXF import, colour 3MF for the AMS, wall-thickness and overhang checks, and plate layout.

## Support the project

STL Editor is free, with no ads and no account, and it stays that way. I build it in my spare time. If it saved you a reprint or an evening in CAD, you can [buy me a coffee on Ko-fi](https://ko-fi.com/natterman12) ♥. Bug reports, ideas and telling a friend about it help just as much.

## Licence

STL Editor is free software under the **GNU General Public License v3.0**. See [LICENSE](LICENSE). You can use, study, change and share it. If you distribute a modified version, it must stay under the GPL with its source available.

It's built on open-source libraries, whose licence texts ship in the `LICENSES` folder of every release:
[manifold3d](https://github.com/elalish/manifold) (Apache-2.0) ·
[three.js](https://threejs.org) (MIT) ·
[three-mesh-bvh](https://github.com/gkjohnson/three-mesh-bvh) (MIT) ·
[trimesh](https://trimesh.org) (MIT) ·
[Shapely](https://shapely.readthedocs.io) (BSD-3) ·
[scikit-image](https://scikit-image.org) (BSD-3) ·
[SciPy](https://scipy.org) / [NumPy](https://numpy.org) (BSD-3) ·
[fast-simplification](https://github.com/pyvista/fast-simplification) (MIT) ·
[pywebview](https://pywebview.flowrl.com) (BSD-3) ·
[Support Fins](https://github.com/gittrahan/support-fins) (MIT) ·
[mini-racer](https://github.com/bpcreech/PyMiniRacer) (ISC)
