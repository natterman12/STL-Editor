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

The toolbar has four tabs: **Solid** (modelling, planes and inspection), **Print** (getting
parts ready to print), **Supports** (Support Fins, with the orient and output tools you use
alongside it) and **Mesh** (repair, remesh, sculpt). Some tools are on
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
Move / Rotate and Scale (drag the arrows, rings or handles on the part and the change is
applied as soon as you let go, or type the numbers and press OK; a rotated part lands on the bed instead of hanging through it, and Move Z then lifts
it off; also "drop to bed" and "centre on origin"; Scale by factor, to size, or unit
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

**Support Fins** (Supports tab, built from [Support Fins](https://github.com/gittrahan/support-fins)
by gittrahan): printing a part tipped on an edge is often much stronger than printing it flat,
but it needs supports. Support Fins adds breakaway fins (thin walls with one-layer *tines*
that grip the part and snap off) and a bed pad, as part of the mesh, so it prints the same in
any slicer with supports turned off.

1. Click **Support Fins**. Faces that need support turn red straight away (amber ones are
   small enough to bridge), and the green fins appear next to the part a moment later with a
   one-line summary (walls, tines, anything it couldn't reach).
2. **Rotate** (the panel starts here): drag the rings on the part to turn it the way you want
   to print it. It stays on the bed, the turn is applied as soon as you let go (one *Rotate*
   step on the timeline), and the fins are rebuilt. You choose the orientation; Support Fins
   doesn't.
3. **Add fins**: draw walls by hand where the auto fins left a gap (line, multi-line or curve,
   as in Draw Walls below). **Remove fins**: click a fin to take it away (it turns see-through
   red); click it again to bring it back, or *Bring all back*.
4. Adjust the settings and press OK. *Add as: Part of the body* joins the fins to the part
   so it exports as one object; *Separate body* keeps them apart so you can hide them.

The settings are Support Fins' own: Auto or Full coverage placement, overhang angle, nozzle,
tines (grip, horizontal for FDM or vertical pins for resin, layer height), support gap, bed
pad, sway braces for tall parts, cutouts, wide-face coverage and plate-only. Hover the **?**
for each one.

Supports stay editable. Clicking **Support Fins** (or **Draw Walls**) again on a part that
already has them opens the same step to change it, rather than stacking a second set on top.
Double-clicking the step on the timeline does the same. If you move, turn, scale or lay flat
a part after its fins were added, the move goes in before the fins on the timeline, so the
fins are rebuilt for the new position instead of tipping over with the part. Walls you drew
by hand stay where you drew them, so check those afterwards.

**Draw Walls** is Support Fins' Draw mode: the red faces show where support is needed, and
you place breakaway walls under them by hand, as many as you like:

- *Line*: click the two ends of a wall; keep clicking pairs to add more.
- *Multi-line*: click each corner of a wall that changes direction; double-click the last
  point (or press Enter, or *Finish wall*) to end it.
- *Curve*: click the two ends, then a point on the curve between them.

A green band follows the pointer while you draw. Esc drops the wall in progress, Ctrl+Z (or
*Undo*) takes back the last point or wall. A wall Support Fins can't build is drawn red with
the reason. Both tools are timeline steps you can edit later.

New Support Fins releases are picked up automatically: the web edition gets them when the
site updates, and the desktop app downloads the new engine in the background and uses it
from the next start (if update checks are on). Settings a new release adds show up in the
panel by themselves, and settings you haven't changed follow the new defaults. Help › About
Support Fins shows the version in use.

**Planes** (Solid tab › Plane): Offset Plane, Plane at Angle, 3-Point Plane, Midplane. Any
flat face also works as a plane.

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
  Both editions open STL, OBJ, 3MF, PLY, OFF, GLB and glTF (a `.gltf` file must have its data
  embedded; use `.glb` otherwise).
- **Save** in Chrome and Edge asks where to save the first time, then saves straight back to
  that file. Other browsers download the project to your Downloads folder each time.
  **Export** always asks (or downloads).
- **File › Recent** lists the last ten projects and meshes. In Chrome and Edge it reopens the
  file itself (the browser may ask for permission again); other browsers keep a copy of files
  up to 25 MB in the browser so they can be reopened.
- **Autosave:** unsaved work is copied into the browser every minute and whenever you switch
  away from the tab. If the tab closes or crashes before you save, the next visit offers to
  recover it (also Help › Recover Unsaved Work). Clearing the site's data in your browser
  removes these copies.
- `.stlx` projects move freely between the web edition and the desktop app.
- **Text** comes with the DejaVu fonts. *Add a font file…* in the font list adds a `.ttf`,
  `.otf` or `.woff` font (pick the bold and italic files too if you want those); the browser
  keeps it for next time. In Chrome and Edge, *More: fonts on this computer…* uses your
  installed fonts (the browser asks first).
- **Offline and installing:** after the first visit the editor works without an internet
  connection. In Chrome and Edge, Help › Install STL Editor (or the install icon in the address
  bar) adds it to your apps; the installed app can open `.stlx`, STL and other mesh files from
  your file manager. When a new version is published, a bar offers to reload into it.
- **Tablets and phones:** one finger orbits, two fingers zoom and pan. While sketching or
  sculpting, one finger draws and two fingers move the view. Double-tap a face to sketch on
  it. On narrow screens the Browser panel opens with the *Bodies* button and tool settings
  slide up from the bottom.
- **Desktop only:** Send to Slicer and To STEP, which start programs on your computer. Their
  buttons are dimmed and point you to the download.
- **Hollow, Repair › Rebuild and Remesh** take a few seconds on big models. The browser has
  less memory than the desktop app, so *Fine* quality on a very large model may ask you to
  pick a coarser setting.
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

---

STL Editor is free and made in spare time. If it's useful to you, you can
[support it on Ko-fi](https://ko-fi.com/natterman12).
