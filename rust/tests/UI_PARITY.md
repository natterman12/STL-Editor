# UI parity map (1.x → 2.0)

One row per file in `tests/ui/*.js`, except `common.js`, `run.js` and `serve.js`.

Batch rules: **4a** = shell, viewport, files, browser tree, timeline or parameters only; **4b** = ribbon tool panels, gizmo, Support Fins panel; **4c** = sketch mode. A file that mixes several goes to the latest batch it needs; the note says which parts 4a covers.

| 1.x file | what it tests | batch (4a/4b/4c) | 2.0 test | status |
| --- | --- | --- | --- | --- |
| desktop_flow.js | Full edit path: import a mesh, sketch and cut on a face, hole, then export | 4c | rust/crates/stl-app/tests/ui/desktop_flow.rs | planned |
| desktop_mirror.js | Mirror and pattern tool panels (preview, apply, timeline) | 4b | rust/crates/stl-app/tests/ui/desktop_mirror.rs | panel open + Pattern defaults + Mirror OK timeline + the mirrored-cut volume (the cuts are primitives, so no sketch mode); nothing parked |
| desktop_new.js | Hole presets, push/pull, sketch trim/offset/mirror/regions, split, fit to bed, sculpt | 4c | rust/crates/stl-app/tests/ui/desktop_new.rs | planned |
| desktop_place.js | Import place mode (keep), undo after editing the import feature | 4b | rust/crates/stl-app/tests/ui/desktop_place.rs | place=keep edit+OK and Ctrl+Z after it (found and fixed: the undo step recorded the edit session's rolled-back marker); nothing parked |
| desktop_sketch2.js | Sketch on the XZ plane, revolve, edit the sketch from the timeline, undo, save | 4c | rust/crates/stl-app/tests/ui/desktop_sketch2.rs | planned |
| desktop_smoke.js | Empty project / start screen loads without console errors | 4a | tests/ui.rs (app harness opens basic.stlx); tests/ui_shell.rs `start_page_shows_until_the_project_has_steps` and start snapshots | pass |
| desktop_tools.js | Hole, primitives, planes, split, mirror, move, hollow, scale, section, info, timeline edit, undo/redo, File menu | 4b | rust/crates/stl-app/tests/ui/desktop_tools.rs | open each named tool, Move/Scale, File menu, undo/redo, hole countersink on a picked face; parked: the sketch + Extrude timeline edit (needs sketch mode: 4c) |
| desktop_v11.js | Thread tool, lay flat, sketch relations and dimensions, inch mode, dark theme, update banner | 4c | rust/crates/stl-app/tests/ui/desktop_v11.rs | planned |
| gizmo.js | Move / Rotate / Scale handles on the part, and Land on bed | 4b | rust/crates/stl-app/tests/ui/gizmo.rs | rotate rings, move drag translate, scale Escape, Land on bed after a ring drag (the grab point comes from the camera and the ring's size); nothing parked |
| kofi.js | Ko-fi links on the start screen, Help menu and About dialog | 4a | tests/ui_shell.rs `help_menu_has_kofi_and_about`, `help_shortcuts_and_about_use_the_1x_text`, `start_page_kofi_link_text_and_url` | pass |
| params.js | Parameters dialog: names, expressions, live values, undo, removing a used parameter | 4a | tests/ui_panels.rs `params_edit_expr`, `params_resolved_shows_used_marker`, `params_cancel_closes`, `params_show_the_1x_hint_and_three_decimals`, `params_undo_and_removing_a_used_parameter_with_the_engine`, `params_dialog_matches_1x_layout`, `params_dialog_snapshots_dark_light` | pass |
| ribbon.js | Ribbon tabs and groups once each, Inspect pinned, narrow window folds groups into » | 4a | tests/ui_shell.rs `ribbon_tools_match_ribbon_def`, `ribbon_group_names_and_tabs_match_ribbon_def`, `each_tool_is_on_one_tab_only`, `ribbon_tabs_show_their_tools_and_inspect_is_pinned`, `narrow_window_folds_groups_into_more` | pass |
| sf11.js | Support Fins: suggest orientations, strength, support plastic, gussets, anchors | 4b | rust/crates/stl-app/tests/ui/sf11.rs | panel/pose/plastic/about, Suggest orientations click, Use, and the strength load vector; nothing parked |
| sf9.js | Support Fins modes: Rotate, Add fins, Remove fins; Move applies on release | 4b | rust/crates/stl-app/tests/ui/sf9.rs | mode switching + Move tool + a click on a fin in Remove mode; nothing parked |
| sfcov.js | Support Fins coverage: bare spots, Full coverage, Bare beyond | 4b | rust/crates/stl-app/tests/ui/sfcov.rs | Held card/reach, Full coverage, then a stricter Bare beyond (the bare-spot numbers against 1.x's goldens are `supports_coverage`); nothing parked |
| sfpanel.js | Support Fins panel: mode buttons, Held card, folding sections remembered, footer credit | 4b | rust/crates/stl-app/tests/ui/sfpanel.rs | modes/credit/Held/order and folding sections remembered across reopen and across runs (platform setting `stle_sections`, as 1.x's localStorage); nothing parked |
| skdim.js | Smart Dimension, part edges, reference dims, Use Edge, parameters, edges found again | 4c | rust/crates/stl-app/tests/ui/skdim.rs | planned |
| skdim2.js | Typed sizes become dimensions, double-click to edit, silhouette edges, dark theme | 4c | rust/crates/stl-app/tests/ui/skdim2.rs | planned |
| tap.js | Phone/tablet layout: import, tap to select, double-tap into sketch, draw by taps | 4c | rust/crates/stl-app/tests/ui/tap.rs | planned |
| touch.js | Tablet and phone layout, browser drawer, one-finger orbit; opens a tool to check the panel layout | 4b | rust/crates/stl-app/tests/ui/touch.rs | hollow panel + orbit on tablet sizes; parked: the browser drawer (milestone 5: 2.0's browser panel floats and has no drawer toggle yet) |
| v110.js | SUPPORTS tab, Export and Send to Slicer menus, Support Fins edited not stacked | 4b | rust/crates/stl-app/tests/ui/v110.rs | SUPPORTS tools, edit-not-stack, Draw Walls reopen, Export smoke, and rotate after fins (the turn goes before the fins step, which is rebuilt); nothing parked |
| web2.js | Web tools: import, hole, sketch cut, threads, export, save and reopen round trip | 4c | rust/crates/stl-app/tests/ui/web2.rs | planned |
| web4.js | Web files: save in place, recent files, autosave and recovery | 4a | tests/files_keys.rs `save_reuses_path`, `save_as_always_asks`; tests/ui_shell.rs `recent_files_open_again_from_the_file_menu`; tests/web_worker.rs | partial: save and session recent files pass; autosave, recovery and lasting recent files are milestone 5 |
| web6.js | Published web app: installable, works offline, update bar | 4a | none yet | deferred: installable app, offline and update bar come with publishing /app2/ (needs Nathan's yes) and milestone 5 |
| web7.js | Support Fins panel from the engine schema, preview, edit, Draw Wall | 4b | rust/crates/stl-app/tests/ui/web7.rs | pass |
| web8.js | Instant red overhangs, fins-only preview, Draw Walls lines / multi-line / curve | 4b | rust/crates/stl-app/tests/ui/web8.rs | overhang session, Draw Walls line clicks on the part, wall curve values, fins preview then OK adds plastic; nothing parked |

## Batch notes (4a coverage in later batches)

- **desktop_flow.js (4c):** 4a covers import and export.
- **desktop_new.js (4c):** 4a covers nothing shell-only; hole, push/pull, split, fit to bed and sculpt are 4b, sketch parts are 4c.
- **desktop_place.js (4b):** 4a covers undo after the edit.
- **desktop_tools.js (4b):** 4a covers section view, undo/redo and the File menu.
- **desktop_v11.js (4c):** 4a covers inch mode, dark theme and the update banner.
- **tap.js (4c):** 4a covers import and tap-to-select.
- **touch.js (4b):** 4a covers tablet/phone layout and one-finger orbit. The browser drawer waits for milestone 5 (phone/tablet layout).
- **desktop_tools.js (4b):** the sketch + Extrude timeline edit stays parked until milestone 4c (sketch mode).
- **v110.js (4b):** 4a covers ribbon tabs and the Export / Send to Slicer menus.
- **web2.js (4c):** 4a covers import, export, and save/reopen; hole and thread panels are 4b.

## Milestone 4a fixes (2026-11-13)

Checks added for what the 4a screenshots showed. None of them maps to a single 1.x test file.

- **Title, swatches, Parameters list, About credits:** `tests/m4a_fix_t1.rs` (`title_after_each_open_path`, `failed_open_keeps_title`, `swatch_matches_viewport_colour`, `params_listed_expanded`, `about_lists_rust_crates_only`). Pass.
- **Start view, grid and axes, section cut face:** `tests/m4a_fix_t5.rs` (`start_page_uses_iso_view`, `start_grid_and_axes`, `section_cap_shows_grid`, `grid_through_body`, dark and light). Pass.
- **WebGL2:** `cargo run -p xtask -- web --check --webgl2` (both bodies in the tree, a pick at the block's centre hits it, section view changes the picture). Pass.
- **Screenshots:** `dev/m4a_shots/capture_m4a_shots.mjs` and `tests/m4a_shots.rs` reach every state and fail when two states give the same picture (`states_differ`). Pass.

## Counts

- **4a:** 6
- **4b:** 12
- **4c:** 8
- **Total:** 26
