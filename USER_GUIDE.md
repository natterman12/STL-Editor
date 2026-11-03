# STL Editor: user guide

CAD-style editing for STL, 3MF and OBJ meshes: sketch on a face and extrude or cut, drill
holes, add or subtract shapes, split, mirror, hollow, scale, repair and simplify. Everything
goes into an editable history (the timeline), so you can change an earlier step later.

## Start

- Portable version: double-click **STL Editor.exe** in the unzipped folder.
- Installed from source with `install.ps1`: double-click **STL Editor** on the Desktop (or the Start Menu).
- Right-click any `.stl`, `.3mf` or `.obj` file → **Edit in STL Editor** (after `install.ps1`).
- Drag files onto the window to insert them.
- `STL Editor (debug).bat` starts it with a console window if something goes wrong.
  The log is in `%LOCALAPPDATA%\STL Editor\log.txt`.
- Web edition: open https://natterman12.github.io/STL-Editor/app/ in Chrome, Edge, Firefox or Safari.
  Nothing is installed. See *Web edition* below for what's different.

## Mouse

| Action | How |
|---|---|
| Orbit | Left-drag |
| Pan | Right-drag or middle-drag |
| Zoom | Wheel (zooms toward the cursor) |
| Select a body | Click (Shift-click adds) |
| Sketch on a flat face | Double-click the face |
| Body menu | Right-click a body |

In a sketch, the left button draws; orbit with right-drag and pan with middle-drag.

## Typical workflow: cut a pocket into an STL

1. Open the STL (File › Open or drag it in).
2. Double-click the flat face you want to work on. You're now sketching on it.
3. Pick **Rectangle**, **Circle**, **Slot** etc. Click to draw. To get exact sizes, type
   numbers while drawing (Tab for the next value, Enter to place). Shapes inside shapes
   become holes in the profile.
4. Click **Finish & Extrude**, choose **Cut**, set the depth, and press **OK**.
   Cuts automatically go into the material.

Double-click any item on the timeline to change it later; the steps after it update.

## Tools

The toolbar has four tabs: **Solid** (modelling), **Print** (getting parts ready to print),
**Construct** (planes and inspection) and **Mesh** (repair, remesh, sculpt). Some tools are on
more than one tab. Buttons with a ▾ open a short list (Primitive, Plane, Rectangle). Press
`Ctrl+K` or `/` and type to find any tool by name. In a narrow window the toolbar shrinks to
icons, and anything that still doesn't fit goes under **»**.

**Create**: Sketch, Extrude (new / join / cut / intersect; one side, flipped, symmetric or
two sides; through all; taper), Revolve, Box, Cylinder, Sphere, Cone, Torus (click a face
to place them on it).

**Modify**: Resize Hole (click the wall of any round hole or pin, even in a downloaded STL: it's measured, then type a new diameter, pick a common size like *M3 heat-set insert* or *6 mm magnet*, or fill the hole), Offset Body (grow or shrink a whole part by a set distance, e.g. 0.2 mm for a tighter or looser fit), Hole (simple, counterbore, countersink, nut trap; click several points; presets for
heat-set inserts M2–M5, screw clearance, socket-head counterbores, hex nut traps and magnet
pockets, with a print tolerance), Push/Pull (drag a flat face out or in), Combine (join /
cut / intersect bodies; Cut takes a *Clearance* that grows the cutting bodies first, so a part made to fit the cut-out isn't too tight), Split (by a plane or flat face),
Move / Rotate (with "drop to bed" and "centre on origin"), Scale (factor, to size, or unit
conversion), Pattern (linear or circular, of bodies or of features), Mirror (bodies or
features), Hollow (wall thickness). Split can add connectors at the cut.

**Threads**: Thread tool: tapped holes or screws/studs, ISO metric M3–M30 presets or any
size, right or left hand, chamfered ends and a print clearance (0.2–0.4 mm works well on
FDM; pitches of 1 mm and up print most reliably).

**Print**: Lay Flat (click the face to put on the bed, or let it pick the most stable side).
Fit to Bed: pick your printer and big parts are cut into pieces that fit, with
connectors at every seam. Connector types: printed pins, holes plus loose printed pins,
holes for 1.75 mm filament (good for thin shells like helmets), 6×3 magnet pockets, or none.
"Smart cut positions" moves each seam to where the part is thinnest.

**Construct**: Offset Plane, Plane at Angle, 3-Point Plane, Midplane. Any flat face also
works as a plane.

**Sketch tools**: Line (connected segments), 3-Point Arc, Rectangle, Centre Rectangle,
Circle, Polygon, Slot, Text, Project, Trim, Offset, Mirror (across a sketch line), Construction.

- **Text**: click where it should start, then type. Any font installed on your computer works, with height, bold / italic, alignment, letter spacing and rotation. Letters with holes keep them. Extrude to emboss, or cut to engrave. Select the text and press *Edit…* to change it later.
- **Project**: adds the model's outline to the sketch as shapes, either where the sketch plane cuts through the model or the model's shadow on the plane. An offset makes a matching lid, collar or gasket, e.g. +0.3 mm to fit over the part. Round outlines come in as real circles. Click a
dimension label on the sketch to change it. Closed loops of lines and arcs become profiles,
and when extruding or revolving you can pick single regions under **Profiles**.

**Sketch constraints**: Coincident, Horizontal / Vertical, Parallel, Perpendicular, Equal,
Tangent, Concentric, Midpoint, Fix, Symmetric, and driving Dimensions (length, diameter,
radius, distance between points, angle between lines). Lines you draw are joined and kept
level / plumb automatically. Change a dimension and the shape follows; drag points or shapes
with the Select tool and the constraints hold. Small glyphs (H, V, ∥, ⊥, =, T, ◎…) show
each constraint: click one and press Delete to remove it, or use the list in the panel.

**Mesh**: Repair (quick fix, or rebuild as one watertight solid), Simplify, Remesh (even
triangles of a chosen size), Sculpt brushes (Smooth, Flatten, Inflate, Deflate), Separate
(one body per loose piece), Remove Bodies, To STEP (uses SolidCraft's Convert Mesh, for
blocky mechanical parts).

**Inspect**: Measure, Section view, Properties (size, volume, triangles, print weight).

**Output**: Export STL / 3MF / OBJ (selected bodies, or all visible bodies), Send to Bambu
Studio / OrcaSlicer / SolidCraft.

## Keyboard

`Ctrl+K` or `/` search tools · `S` sketch · `E` extrude · `H` hole · `M` move · `F` fit · `0`–`6` views · `Ctrl+Z` /
`Ctrl+Y` undo / redo · `Ctrl+S` save · `Ctrl+O` open · `Ctrl+I` insert mesh · `Esc` cancel ·
`Del` remove selected bodies. In a sketch: `L` line, `A` arc, `R` rectangle, `C` circle,
`P` polygon, `S` slot, `T` trim, `Ctrl+Z` undo the last shape.

## Units and theme

View › Units switches between millimetres and inches (you can type fractions like `1 3/8`).
Models are always stored in millimetres. View › Theme: Light, Dark or Match Windows.

## Autosave, recovery and updates

Your work is saved in the background every minute. If STL Editor closes unexpectedly, the
next start offers to recover it (also Help › Recover Unsaved Work).

Help › Check for Updates asks the project's GitHub Releases page whether a newer version
exists (only that public page is contacted; nothing about you or your files is sent). The
repository is set in Help › Update Settings (default: `natterman12/STL-Editor`; leave it empty to turn checks off).

## Web edition

The web edition is the same editor running in your browser. Your files are opened from and
saved to your computer; nothing is uploaded.

- **Open / Import** show your browser's file picker; you can also drag files onto the window.
- **Save** and **Export** download the file. Chrome and Edge ask where to save it; other
  browsers put it in your Downloads folder.
- `.stlx` projects move freely between the web edition and the desktop app.
- **Text** comes with the DejaVu fonts. In Chrome and Edge, *More: fonts on this computer…*
  in the font list lets you use your installed fonts (the browser asks first).
- **Desktop only:** Hollow, Remesh, Sculpt, Repair › Rebuild, Send to Slicer and To STEP. Their
  buttons are dimmed and point you to the download.
- There's no autosave or recovery yet: save your project before closing the tab.
- Settings (units, theme) are remembered by your browser.

## Projects

**Save** writes a `.stlx` project with the original meshes and the full timeline, so every
step stays editable. **Export** writes plain mesh files for slicers.

## Notes and limits

- Bodies that aren't watertight (shown "needs repair" in the Browser) display and export,
  but must be repaired before editing.
- Fillet and chamfer on mesh edges aren't available: STL files have no real edges. For blocky
  parts, use To STEP and finish them in SolidCraft.
- Hollow and Repair › Rebuild work on a voxel grid. Use Quality › Fine for small details.

## How it works

A local Python engine (Manifold for booleans, trimesh/scikit-image for repair and voxel
work) serves a three.js interface shown in a desktop window (pywebview / WebView2).
The web edition runs a JavaScript port of the engine (Manifold compiled to WebAssembly)
inside the page. Nothing leaves your computer in either.

Libraries: manifold3d (Apache-2.0), three.js (MIT), three-mesh-bvh (MIT), trimesh (MIT), Shapely (BSD-3),
scikit-image (BSD-3), SciPy (BSD-3), NumPy (BSD-3), fast-simplification (MIT),
pywebview (BSD-3); in the web edition also fflate (MIT), opentype.js (MIT) and the DejaVu fonts (free licence).
