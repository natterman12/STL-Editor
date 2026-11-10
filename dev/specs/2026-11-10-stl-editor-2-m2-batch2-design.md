# STL Editor 2.0 Milestone 2: engine batch 2 (sketching) and the kernel speed fix (design)

Status: approved in chat on 2026-11-10. This is a sub-spec of `dev/specs/2026-11-10-stl-editor-2-rust-design.md`. The batch 1 sub-spec's structure rules (one file per feature, `features/common/`) still apply.

## 1. Scope

What Nathan decided:

- Font reading and SVG/DXF import move forward from milestone 3 into this batch, so Sketch is complete.
- The manifold-rust speed fix runs in the same run, in our vendored copy.

In scope:

**Features:**

- Sketch, Extrude, Revolve, Loft, Sweep, Hole, Push/Pull.
- The helpers they use:
  - `features.py`: `_sketch`, `_material_volume`, `_dedupe`, `_ccw`, `_profile_loop`, `_resample`, `_align`, `_mesh_solid`, `_path_points`, `_cross2`, `_path_self_crosses`, `_rot_between`, `_bbox`, `_overlaps`
  - `geometry.py`: `big_extent`, `frame_matrix`, `make_cylinder`
  - `printing.py`: `pushpull_tool`

**Sketch geometry** (`stleditor/sketch.py`, all of it):

- entity contours and paths, arcs, linework, faces, and `profile`
- the sketch operations `explode`, `trim`, `offset`, `mirror`, `find_axis` and `snap_points`
- 1.x uses shapely (`polygonize`, `unary_union`, and so on) here; 2.0 matches its results

**Part edges for sketches** (`meshtools.py`): `ref_geometry`, `outline`, `outline_entities`, `_as_circle` and `_fit_circle`, used by the sketch `refs` operation.

**Text:**

- `meshtools.py` `font_dirs`, `font_files`, `list_fonts`, `default_font`, `_font_path`, `_load_font`, `text_contours` and `text_extent`
- 1.x flattens glyphs with a fontTools pen; 2.0 flattens the same way
- the bundled DejaVu fonts in `web/fonts/` are the test fonts

**The sketch solver**, from `web/js/constraints.js`, all of it:

- `solve` (minimum-norm Gauss-Newton with the Levenberg-style fallback)
- `analyze` (rank and degrees of freedom), `isRedundant`, `unsatisfied` and `measure`
- `makeConstraint`, `inferDimension`, `angleTo`, `lineMeet`, `glyphSpots`, `refsExist`, `pointKeys` and `point`
- This runs in the interface in 1.x. In 2.0 it is engine code that the interface (milestone 4) will call.

**SVG and DXF import** (`web/js/import2d.js`):

- the same entities, units, transforms and sampling as 1.x
- SVG goes through a pure-Rust SVG library (usvg, with text and fonts off) instead of the browser

**The kernel speed fix:**

- vendored manifold-rust in `rust/third_party/manifold-rust`, used through `[patch.crates-io]`
- Combine join, Combine cut and Split on the bench mesh must be no slower than 1.x
- the kernel corpus, reader, repair and parity results must stay identical
- every change is listed in `VENDORED.md` and kept upstreamable

Out of scope:

- Hole thread options, if they need the Thread feature (batch 4). Such cases carry `"requires": ["thread"]`.
- Anything that needs batch 3's offset or voxel tools, also marked with `requires`.
- File export and STEP stay in milestone 3. The interface's drawing of dimensions (`dims.js`) belongs to milestone 4.

## 2. Structure

| Crate | Added |
|---|---|
| `stl-sketch` | `entities.rs` (entity types, `pointKeys`, `point`); `contours.rs`, `profile.rs`, `ops/{explode,trim,offset,mirror,axis,snap}.rs`, ports of `sketch.py`; `polygon.rs` (the shapely operations sketch.py uses, on our kernel's 2D shapes or a pure-Rust polygon crate); `text/{fonts,glyphs}.rs`; `solver/{constraints,solve,analyze,infer}.rs`, a port of `constraints.js`; `import2d/{svg,dxf}.rs` |
| `stl-mesh` | `refs.rs` (`ref_geometry`, `outline`, `outline_entities`, circle fitting) |
| `stl-engine` | `features/{sketch,extrude,revolve,loft,sweep,hole,pushpull}.rs`; `features/common/profiles.rs` for the shared profile and path helpers |
| `third_party/manifold-rust` | the speed fix |

## 3. Reference behaviour

- **The Python side** (sketch, features, text, refs): desktop 1.x on Nathan's PC, pinned by `dev/parity/requirements-golden.txt`.
- **The JS side** (solver, SVG/DXF): 1.x's own code.
  - `constraints.js` runs in Node.
  - `import2d.js` runs in headless Chrome through puppeteer-core (`CHROME_PATH`), because SVG needs a real browser.
  - Golden scripts live in `dev/parity/js/`.
- **SVG matching rule:** point-by-point within 0.01 mm, or 0.1% of the drawing size, because browser path flattening and usvg's differ. Entity count and kinds must match exactly. Where that can't be met, it's listed in `EXCEPTIONS.md` with Nathan's agreement.
- **Solver matching rule:** the same converged coordinates within 1e-6 mm, and identical `analyze` and `unsatisfied` results, on the golden scenes. Iteration counts may differ.

## 4. Golden additions

- `rust/tests/parity/features/` gets at least 4 cases each for sketch, extrude, revolve, loft, sweep, hole and pushpull. That includes every mode and operation and at least one 1.x error per feature.
- `rust/tests/parity/sketch/`: cases for `profile` and every sketch operation, including nested loops, touching shapes, slots and polylines.
- `rust/tests/parity/text/`: `text_contours` and `text_extent` for several strings, all bundled fonts, sizes, and characters outside basic Latin.
- `rust/tests/parity/solver/`: scenes covering every constraint and dimension kind, under-, fully- and over-defined sketches, redundant dimensions, and `inferDimension` picks.
- `rust/tests/parity/import2d/`:
  - SVG: paths with arcs, curves and transforms, units in mm/in/px and viewBox, the basic shapes
  - DXF: LINE, CIRCLE, ARC, and LWPOLYLINE with bulges
  - a broken file of each kind
- `rust/tests/parity/refs/`: `ref_geometry` on the test models, with sketch planes on a face, cutting the part, and off the part.

## 5. Done when

1. Parity:
   - 0 failed
   - all new case files pass, except those with `requires`
   - every batch 1 case still passes
2. Sketch, text, solver, import2d and refs golden tests all pass under their matching rules.
3. Bench:
   - every row at or below 1.0, including Combine join, Combine cut and Split
   - new rows for Extrude and Revolve of a 200-segment sketch, and a 40-step Sweep
   - `PERF_GAPS.md` emptied of the batch 1 gaps
4. With the vendored kernel, the kernel corpus, reader, repair and all earlier checks still pass. The pure-Rust check and wasm32 builds pass.

## 6. Team

| Who | Jobs |
|---|---|
| Opus 5.5 | Leads and reviews |
| Fable 5.2 | The kernel speed fix in `third_party/manifold-rust`; reviews the solver |
| Sonnet A | The solver (`solver/`); Sweep and Loft |
| Sonnet B | `sketch.py` port (`contours`, `profile`, `ops`, `polygon`); Extrude, Revolve, Sketch |
| Cursor | Text and fonts; SVG/DXF import; `refs.rs`; Hole and Push/Pull; all golden scripts and case files, including the Node and Chrome harnesses |

Every job gets a read-only review by a worker other than its author.
