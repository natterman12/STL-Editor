# STL Editor 2.0: sketch look, snapping and tool panels (design)

Approved by Nathan 2026-11-16 ("Yes, build it"), after a side-by-side test of SolidCraft 0.43
and 2.0 (new design, sketch on XY, rectangle, Extrude).

## Why

Sketching in 2.0 looks heavy and is hard to read: origin planes far too big, an X,Y box that
wraps next to the pointer, thick finished lines, orange lines while drawing that vanish on the
orange plane and grid, and a Rectangle button that opens a menu first. Typing a size with Tab
didn't work. Nathan wants it to feel like SolidCraft.

## Rules

- SolidCraft is a behaviour and layout reference. 2.0 writes its own code and keeps its own
  colours, icons and font (Nathan allowed copying without credit; we don't need to, and most
  licences require the notice).
- Both themes, mm and inch. Snapshot changes are deliberate and go in `rust/tests/EXCEPTIONS.md`.

## What changes

| Item | Now | New |
|---|---|---|
| Origin planes | Centred, side 0.6 × the next power of 10 above the part | One square from the origin in + directions, side ¼ of the part's longest side, at least 10 mm; 25 mm with no part |
| Cursor readout | X,Y box next to the pointer while drawing (setting on by default) | Gone while sketching; the status bar X Y stays. The View menu item and setting go too |
| Value boxes | Label over the value, narrow, wraps | One line, label + value + unit ("W 18.00 mm", "in" in inch mode), box sized to its text |
| Finished lines | ~4 px blue with a dark outline | ~1.5 px, no outline; small square points at corners and ends; light fill in closed shapes |
| Lines while drawing | Orange (plane/grid colour) | A bright green of 2.0's own, both themes |
| Rectangle button | Opens a menu | Starts the plain rectangle; the small arrow still opens the menu; the kinds also show as icons in the sketch panel |
| Snapping | Endpoints, midpoints, centres, silently | A marker at the snap point shows the kind (endpoint, midpoint, centre, tangent) with a short label; new tangent snap from a line to an arc/circle, tied as tangent |
| Tab to type a size | Didn't reach the sketch in the real app | Fixed; a test presses Tab through the real key path |
| Sketch panel | Shapes list, grid snap, long help text | Tool options (the drawing tool's kinds), line type, tick boxes (Constraints, Dimensions, Points, Snap, Construction), a big Finish Sketch, Look At; the help moves to a "?" tooltip |
| Extrude / Cut / feature panels | Every field flat, Sketch first | Main rows first (Profiles, Direction, Distance, Operation); start offset, taper and other rarely used fields under a folded "More options" |

## Build order (native, one session)

1. Tab fix + real-key-path test.
2. Origin planes.
3. Line widths, draft colour, value boxes with units, no cursor readout.
4. Snap markers + tangent snap (tests: each kind found and labelled; tangent line touches the
   circle and gets a tangent relation).
5. Rectangle button; sketch panel; feature panels with folded "More options".
6. Snapshots regenerated and listed in EXCEPTIONS.md; all tests; install; push.
