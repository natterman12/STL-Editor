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

The toolbar has four tabs, and each tool is on one of them only:

- **Model**: Create (Sketch, Extrude, Revolve, Primitive), Modify (Push/Pull, Combine, Split,
  Mirror, Pattern), Holes (Hole, Thread, Resize Hole), Shape (Hollow, Offset Body) and
  Reference (Plane, Parameters).
- **Print**: Position (Lay Flat, Move, Rotate, Scale), Fit (Fit to Bed) and Output (Export,
  Send to Slicer).
- **Supports**: Support Fins and Draw Walls.
- **Mesh**: Repair, Simplify, Remesh, Separate, Remove Bodies, Sculpt and To STEP.

**Inspect** (Measure, Section, Properties) sits at the right end of every tab. Buttons with a
▾ open a short list (Primitive, Plane, Rectangle), and a group name with a ⌄ under it lists
that group's tools. Press `Ctrl+K` or `/` (or click the search box at the top right) and type
to find any tool by name. In a narrow window the toolbar shrinks to icons, and anything that
still doesn't fit goes under **»**.

The model fills the whole window. The **Browser** (your bodies, sketches and parameters)
floats at the top left, a tool's settings open in a panel at the top right, and the
**History** bar along the bottom holds the timeline. The view buttons (Top, Front, Iso, Fit…)
sit at the top of the model.

Tool panels keep the everyday settings at the top. Sections with a › fold away the rest:
click the heading to open or close it, and the panel remembers that next time. A closed
section shows "2 changed" when you've changed something inside it.

**Create**: Sketch, Extrude (new / join / cut / intersect; one side, flipped, symmetric or
two sides; through all; taper), Revolve, Box, Cylinder, Sphere, Cone, Torus (click a face
to place them on it).

**Modify**: Resize Hole (click the wall of any round hole or pin, even in a downloaded STL: it's measured, then type a new diameter, pick a common size like *M3 heat-set insert* or *6 mm magnet*, or fill the hole), Offset Body (grow or shrink a whole part by a set distance, e.g. 0.2 mm for a tighter or looser fit), Hole (simple, counterbore, countersink, nut trap; click several points; presets for
heat-set inserts (M1.6–M5 and #2-56–1/4"-20), screw clearance (M1.6–M8 and #2–3/8"),
socket-head counterbores, hex nut traps, holes for self-tapping screws (M1.6–M5) and magnet
pockets, metric and inch, with a print tolerance), Push/Pull (drag a flat face out or in), Combine (join /
cut / intersect bodies; Cut takes a *Clearance* that grows the cutting bodies first, so a part made to fit the cut-out isn't too tight), Split (by a plane or flat face),
Move / Rotate and Scale (drag the arrows, rings or handles on the part and the change is
applied as soon as you let go, or type the numbers and press OK; a rotated part lands on the bed instead of hanging through it, and Move Z then lifts
it off; also "drop to bed" and "centre on origin"; Scale by factor, to size, or unit
conversion), Pattern (linear or circular, of bodies or of features), Mirror (bodies or
features), Hollow (wall thickness). Split can add connectors at the cut.

**Threads**: Thread tool: tapped holes or screws/studs, ISO metric M1.6–M30, UNC #1-64 to
3/4"-10 and the common UNF sizes (#0-80, #10-32, 1/4"-28…) or any
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
   only suggests: **Suggest orientations** (in the Print check section at the bottom of the
   panel) tries the part every way down and lists the best
   few (how much needs support, how tall, how many fins, how much plastic), and *Use* turns
   the part that way.
   **Strength** says how the layers sit for this pose: prints are weakest across their
   layers, so a long part standing up is the weak way to print it. Set *Load in use* (how
   the part is pulled when it's used) and it says whether the load runs along the layers or
   across them, with *Turn it to the strongest pose* when a better one exists.
3. **Add fins**: draw walls by hand where the auto fins left a gap (line, multi-line, curve,
   gusset or anchor, as in Draw Walls below). **Remove fins**: click a fin to take it away (it turns see-through
   red); click it again to bring it back, or *Bring all back*.
4. Adjust the settings and press OK. **Add as**, right under the part's name, decides how the
   fins are added: *Part of the body* joins them to the part so it exports as one object;
   *Separate body* keeps them apart so you can hide them. The panel opens with Placement and
   Setup showing; the rest (Tines, Clearances, Sway braces, Walls) fold away until you need
   them.
   **Print check**, at the bottom, sums it up in two cards: **Held** (how much of the
   overhang the fins hold, see below) and **Support plastic** (what the supports add, in
   grams, from their exact volume). Under them are Suggest orientations and Strength (in
   Rotate), *Material* (your filament for the plastic estimate: PLA, PETG, ABS, ASA, TPU or
   Nylon) and *Bare beyond*.

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
- *Gusset*: a fin that stands off a side of the part instead of the bed, like the triangle
  under a shelf. Click the side (low enough for the fin to climb out at 50°), then the
  overhang it should reach.
- *Anchor*: a rib that holds a tall side, or a back that leans up, from the bed. Click the
  side where it should stand.

A green band follows the pointer while you draw. Esc drops the wall in progress, Ctrl+Z (or
*Undo*) takes back the last point or wall. A wall Support Fins can't build is drawn red with
the reason. Both tools are timeline steps you can edit later.

New Support Fins releases are picked up automatically: the web edition gets them when the
site updates, and the desktop app downloads the new engine in the background and uses it
from the next start (if update checks are on). Settings a new release adds show up in the
panel by themselves, and settings you haven't changed follow the new defaults. Help › About
Support Fins shows the version in use.

- **Held**: once the fins are built, the overhangs they hold turn pale and any left bare stay
  bright red. A bare spot is overhang further than *Bare beyond* (5 mm to start with) from
  the top of a fin or from a wall of the part it grows out of. The Held card says how much of
  the overhang is held and how many bare spots there are; the line under it says how far the
  worst is from a support. *Bare beyond* is in the Print check section.
  To fix them, switch Placement mode to Full coverage, raise Wide-face coverage, or draw fins
  there in *Add fins*; the check runs again each time. Bridging a few millimetres usually
  prints; much more than 5 mm sags, so lower *Bare beyond* for a stricter check.

**Planes** (Model tab › Plane): Offset Plane, Plane at Angle, 3-Point Plane, Midplane. Any
flat face also works as a plane.

**Sketch tools**: Line (connected segments), 3-Point Arc, Rectangle, Centre Rectangle,
Circle, Polygon, Slot, Text, Project, Use Edge, Trim, Offset, Mirror (across a sketch line), Construction.

- **Text**: click where it should start, then type. Any font installed on your computer works, with height, bold / italic, alignment, letter spacing and rotation. Letters with holes keep them. Extrude to emboss, or cut to engrave. Select the text and press *Edit…* to change it later.
- **Project**: adds the model's outline to the sketch as shapes, either where the sketch plane cuts through the model or the model's shadow on the plane. An offset makes a matching lid, collar or gasket, e.g. +0.3 mm to fit over the part. Round outlines come in as real circles.
- **Use Edge**: click one of the part's orange edges to copy it into the sketch as a line or circle that stays on the edge.

Closed loops of lines and arcs become profiles, and when extruding or revolving you can pick
single regions under **Profiles**.

**Dimensions** work like SOLIDWORKS. Pick **Smart Dimension** (`D`), click what to measure,
move the pointer to where the dimension should sit, click, and type the value in the box
(✓ or Enter applies it, ✗ or Esc leaves it). What you pick decides the dimension: a line gives
its length, a circle its diameter, an arc its radius; two points or a point and a line give the
distance between them, two lines their angle (or the distance between them when they're
parallel). Where you move the pointer decides horizontal, vertical or along the line, and for
an angle which side is measured. Each dimension is drawn on the sketch with extension lines and
arrows showing exactly what it measures; drag it to move it, double-click it to change it.
Sizes typed while drawing (a rectangle's width and height, a circle's diameter, a line's
length) become dimensions too.

- **Measure from the part**: the edges of the part are shown in orange: the outline of the
  face you sketch on, or where the sketch plane cuts the part, or the part's shadow on the
  plane. Click them (and the origin) like any sketch line or point to dimension from them or
  to snap and join to them; they don't move. When you edit the sketch after the part has
  changed, the edges are found again; one that's gone is shown red.
- **Colours**: black shapes are fully defined (dimensions and relations set their size and
  place), blue ones can still move, and red means relations that can't all be met. The panel
  and the status bar say *Fully defined* or how many more dimensions are needed. Tie a sketch
  to the origin or a part edge to fully define it.
- **Reference dimensions**: adding a dimension the sketch already sets would over-define it,
  so it's offered as a reference dimension instead: grey, in brackets, showing the size
  without driving it. Tick or untick *Reference* in the panel to switch.
- **Parameters**: type a parameter name or an expression (`width`, `=width/2 + 3`) as the
  value. The dimension turns purple, and changing the parameter in Edit › Parameters…
  updates the sketch and everything built from it.

**Sketch relations**: Coincident, Horizontal / Vertical, Parallel, Perpendicular, Equal,
Tangent, Concentric, Midpoint, Fix, Symmetric. Lines you draw are joined and kept
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

## Parameters

Name a value once and use it anywhere: Edit › Parameters… (or Model tab › Parameters, or the
*Parameters* folder in the browser). Give each one a name and a value; a value can use other
parameters (`height = width / 4 + 5`). Then type the name, or an expression like
`=width/2 + 3`, into any number in a tool: an extrude distance, a hole diameter, a box size, a
move. Change a parameter and every step that uses it is rebuilt. Expressions are in mm (degrees
for angles) and can use `+ - * / ^ ( )`, `sqrt`, `min`, `max`, `round`, `sin`, `cos`, `tan`
and `pi`. A field with an expression shows it in purple; hover it to see what it comes to.
Parameters are saved in the project, and Ctrl+Z undoes a change.

## Units and theme

View › Units switches between millimetres and inches (you can type fractions like `1 3/8`).
Models are always stored in millimetres. View › Theme: Dark (the start), Light or Match
Windows.

## Autosave, recovery and updates

Your work is saved in the background every minute. If STL Editor closes unexpectedly, the
next start offers to recover it (also Help › Recover Unsaved Work).

Help › Check for Updates asks the project's GitHub Releases page whether a newer version
exists (only that public page is contacted; nothing about you or your files is sent). The
repository is set in Help › Update Settings (default: `natterman12/STL-Editor`; leave it empty to turn checks off).

When there's a new version, **Update now** installs it in one click: the app downloads the
release, checks it against its SHA-256, swaps it in and starts again (unsaved work is kept for
Recover Unsaved Work). This needs the STL Editor folder somewhere you can write to, like
Documents or the Desktop. A copy run from a git checkout pulls the new version instead (only
when it has no changes of its own). When a newer Support Fins engine has been downloaded,
Help › About Support Fins has a *Restart now* link.

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
