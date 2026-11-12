# STL Editor 2.0 Milestone 3: file export (design)

Status: approved in chat on 2026-11-11. This is a sub-spec of `dev/specs/2026-11-10-stl-editor-2-rust-design.md`. Main spec milestone 3 is "Files: STL, 3MF, OBJ; SVG, DXF and fonts; STEP". Reading every mesh format came in batch 1, and SVG, DXF and fonts came in batch 2. What is left is writing files.

## 1. Scope

What Nathan decided:

- **3MF and OBJ** match desktop 1.x, which writes them through trimesh 5.2.0. Web 1.x's own writers (`web/engine/io.js`) are not the reference.
- **Faceted STEP export** (every triangle becomes one flat face, so CAD opens the file as a solid) is offered on desktop and web. In 1.x only the web edition has it. Desktop also keeps SolidCraft's "To STEP" hand-off, which belongs to milestone 5. Offering faceted STEP on desktop is listed in `EXCEPTIONS.md` as a 2.0 addition.
- **Leftovers from batches 4 and 5**, folded into this run:
  - a kernel speed pass for Pattern (1.12)
  - sending the mesh to the Support Fins webview once per body for Draw Walls (1.38)
  - replacing the "dense tines" option set in `fins_golden.json` with one that changes the result
  - recording two items as accepted with Nathan's OK on 2026-11-11: the shorter Support Fins error text (`Support Fins: boom`), and the Resize Hole bench row on `hole_and_pin.stl`
- **Run rules as in batch 4+5:**
  - the lead doesn't stop for decisions; it parks them for the end
  - read-only reviews by a second worker are for code ports only
  - volume and area within 0.1 percent everywhere

In scope:

**Writers in `stl-io`:**

- **STL:** a port of `geometry.stl_bytes`. It is binary only, as 1.x exports, and byte-identical to 1.x:
  - the header `b"STL Editor binary STL"` padded with spaces to 80 bytes
  - the triangle count, then float32 normals from the cross product, with zero-length normals written as 0
  - float32 vertices and a zero attribute word
  - bodies concatenated in order, as `export_bodies` does
- **3MF:** a port of `trimesh/exchange/threemf.py` `export_3MF`, as `export_bodies` calls it. Each body is added to a `trimesh.Scene` with `node_name` and `geom_name` set to the body name.
  - The same zip entries, XML elements and attributes, object and build-item order, and the unit and number formatting that trimesh writes.
  - The zip container need not be byte-identical, because compression settings may differ. The XML text inside must be, apart from the `p:UUID` values: trimesh writes a random UUID on every object, on `<build>` and on each item, so 1.x's own output changes with each export. 2.0 writes UUIDs in the same places, and the comparison replaces them with a placeholder first.
- **OBJ:** a port of `trimesh/exchange/obj.py` `export_obj`, applied to `trimesh.util.concatenate` of the bodies' meshes, with the defaults `export_bodies` uses.
  - The same header comment, vertex lines, any normal lines trimesh includes, face lines, and digits.
  - The output is byte-identical to 1.x.
- **Faceted STEP:** a port of `web/engine/step.js` `stepBytes(meshes, {title, date})`:
  - the same entity order and numbering
  - the `real()` and `str()` formatting
  - the shared edges and vertices
  - the `FILE_NAME` line
  - the error when a body isn't a closed solid, word for word

  Byte-identical to `step.js` when given the same `title` and `date`.

**`Document.export` in `stl-engine`:** a port of `document.export(path, body_ids)` and the engine side of `r_export` and the web `stepResult`:

- Choosing bodies: the given ids. With none given, the visible bodies. With none visible, every body.
- Names come from `body_name`.
- With no bodies at all, it gives `"There is nothing to export."`
- The format comes from the path's extension. A path without one gets `.{format}` added.
- `export_base_name(ids)`:
  - one body: its name
  - otherwise the project path's stem, or the title, or `"model"`, with `.stlx` removed
- It returns the number of bodies written.
- `export_bytes(format, ids)` returns the bytes for the web edition.
- The file dialog, the slicer hand-off and SolidCraft stay with milestones 4 and 5.

Out of scope:

- The Export dialog and menus (milestone 4).
- Slicer and SolidCraft hand-offs, and the updater (milestone 5).
- ASCII STL export. 1.x has none; ASCII is only read.

## 2. Structure

| Crate | Added |
|---|---|
| `stl-io` | `write/{mod.rs,stl.rs,threemf.rs,obj.rs,step.rs}`, where `mod.rs` holds `export_bodies(bodies, format) -> Vec<u8>` |
| `stl-engine` | `document/export.rs`: `export`, `export_bytes`, `export_base_name` |
| `third_party/manifold-rust` | The Pattern speed pass, listed in `VENDORED.md` |
| `stl-platform` | The Draw Walls change in `fins/webview.rs`: the mesh is kept in the page once per body |

`stl-io` stays pure Rust and builds for wasm32. Zip and XML use the crates the 3MF reader already uses.

## 3. Reference behaviour and checks

- **STL and OBJ:** compared byte for byte with 1.x desktop `export_bodies` on the test models and on multi-body documents.
- **3MF:** checked three ways.
  - The XML inside the zip is compared byte for byte with 1.x's, with UUID values replaced by a placeholder.
  - The file is read back by 1.x `load_mesh_file` and by 2.0's reader: the same parts, names and `MeshStats`.
  - The zip must open with a standard zip reader.
- **STEP:** compared byte for byte with `web/engine/step.js` run in Node, with a fixed `date`, on the same bodies.
- **Manual check, once:** Nathan or the lead opens one 3MF, one OBJ, one STL and one STEP file:
  - the first three in Bambu Studio and PrusaSlicer
  - the STEP file in SolidCraft or FreeCAD, if installed

  This is noted in the final summary and isn't a CI step.
- **The Pattern speed pass:** results identical to before (kernel corpus, parity and every golden check unchanged), and the Pattern bench row at or below 1.0.
- **The Draw Walls change:** results identical, and the Draw Walls row at or below 1.0.

## 4. Golden additions

- **`dev/parity/make_export_golden.py` → `rust/tests/parity/export/`:**
  - for each case, the bytes 1.x writes for STL and OBJ, and the 3MF XML entries
  - the 1.x read-back stats
  - cases: one body; three bodies with names that need escaping (spaces, `&`, non-ASCII); a hidden body; and no bodies (the error)
- **`dev/parity/js/step_golden.mjs` → `rust/tests/parity/export/*.step`:** `step.js` output with `date` fixed to `2026-01-01T00:00:00`, plus the open-body error case.
- **Size:** golden files over 1 MB are stored as a SHA-256 plus the size. The bench mesh is never used as a golden input.
- **CI:** the golden job runs both `--check` scripts.

## 5. Done when

1. Every export golden matches under section 3's rules. Parity still shows 0 failed and 0 pending.
2. **Bench** on the bench mesh, against 1.x desktop. Faceted STEP is compared with `step.js` in Node instead.
   - New rows: Export STL, Export 3MF, Export OBJ, Export STEP.
   - Every row at or below 1.0, including Pattern and Draw Walls. Any row that can't reach 1.0 goes in `PERF_GAPS.md` for Nathan's OK.
3. The `fins_golden.json` dense-tines set gives a result different from the default on at least one test model.
4. `EXCEPTIONS.md` records:
   - the faceted STEP addition on desktop
   - the shorter engine error text
   - the Resize Hole bench model
5. The pure-Rust check, the wasm32 builds, `tests/test_engine.py` and CI all pass.

## 6. Team

| Who | Jobs |
|---|---|
| Opus 5.5 (lead) | Leads; checks golden scripts, cases, docs, CI and bench rows itself |
| Sonnet A | 3MF and OBJ writers |
| Sonnet B | STL and STEP writers; `Document.export` |
| Worker 6 (Grok 4.7) | Pattern speed pass in `third_party/manifold-rust` |
| Opus (Claude) | The Draw Walls webview change |
| Cursor | Both golden scripts, case files, the dense-tines option set, bench rows, CI |
| Worker 7 (Composer 2.5), Haiku (Claude), Hermes (Inkling 2 Small) | Read-only reviews |

- Worker 1 and Fable are not used.
- Gemini (Cursor, limited) may take one second-opinion review.
