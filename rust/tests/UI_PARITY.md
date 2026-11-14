# UI parity map (1.x → 2.0)

One row per file in `tests/ui/*.js`, except `common.js`, `run.js` and `serve.js`.

Batch rules: **4a** = shell, viewport, files, browser tree, timeline or parameters only; **4b** = ribbon tool panels, gizmo, Support Fins panel; **4c** = sketch mode. A file that mixes several goes to the latest batch it needs; the note says which parts 4a covers.

| 1.x file | what it tests | batch (4a/4b/4c) | 2.0 test | status |
| --- | --- | --- | --- | --- |
| desktop_flow.js | Full edit path: import a mesh, sketch and cut on a face, hole, then export | 4c | rust/crates/stl-app/tests/ui/desktop_flow.rs | pass (import, viewport dblclick / face plane at centred top [0,0,20] like 1.x's face centre, rect+circle, Finish & Extrude cut 5, volume drop, export) |
| desktop_mirror.js | Mirror and pattern tool panels (preview, apply, timeline) | 4b | rust/crates/stl-app/tests/ui/desktop_mirror.rs | panel open + Pattern defaults + Mirror OK timeline + the mirrored-cut volume (the cuts are primitives, so no sketch mode); nothing parked |
| desktop_new.js | Hole presets, push/pull, sketch trim/offset/mirror/regions, split, fit to bed, sculpt | 4c | rust/crates/stl-app/tests/ui/desktop_new.rs | pass (hole preset, push/pull, sketch trim/mirror tools, Finish & Extrude path, split/fittest panels) |
| desktop_place.js | Import place mode (keep), undo after editing the import feature | 4b | rust/crates/stl-app/tests/ui/desktop_place.rs | place=keep edit+OK and Ctrl+Z after it (found and fixed: the undo step recorded the edit session's rolled-back marker); nothing parked |
| desktop_sketch2.js | Sketch on the XZ plane, revolve, edit the sketch from the timeline, undo, save | 4c | rust/crates/stl-app/tests/ui/desktop_sketch2.rs | pass (XZ plane, revolve, timeline edit, Ctrl+Z, Save) |
| desktop_smoke.js | Empty project / start screen loads without console errors | 4a | tests/ui.rs (app harness opens basic.stlx); tests/ui_shell.rs `start_page_shows_until_the_project_has_steps` and start snapshots | pass |
| desktop_tools.js | Hole, primitives, planes, split, mirror, move, hollow, scale, section, info, timeline edit, undo/redo, File menu | 4b/4c | rust/crates/stl-app/tests/ui/desktop_tools.rs | pass (open each named tool, Move/Scale, File menu, undo/redo, hole countersink; sketch on body face + Extrude join, timeline edit distance 8→12 changes volume) |
| desktop_v11.js | Thread tool, lay flat, sketch relations and dimensions, inch mode, dark theme, update banner | 4c | rust/crates/stl-app/tests/ui/desktop_v11.rs | pass (thread, layflat, sketch dims, Cancel edit, inch toast, dark) |
| gizmo.js | Move / Rotate / Scale handles on the part, and Land on bed | 4b | rust/crates/stl-app/tests/ui/gizmo.rs | rotate rings, move drag translate, scale Escape, Land on bed after a ring drag (the grab point comes from the camera and the ring's size); nothing parked |
| kofi.js | Ko-fi links on the start screen, Help menu and About dialog | 4a | tests/ui_shell.rs `help_menu_has_kofi_and_about`, `help_shortcuts_and_about_use_the_1x_text`, `start_page_kofi_link_text_and_url` | pass |
| params.js | Parameters dialog: names, expressions, live values, undo, removing a used parameter | 4a | tests/ui_panels.rs `params_edit_expr`, `params_resolved_shows_used_marker`, `params_cancel_closes`, `params_show_the_1x_hint_and_three_decimals`, `params_undo_and_removing_a_used_parameter_with_the_engine`, `params_dialog_matches_1x_layout`, `params_dialog_snapshots_dark_light` | pass |
| ribbon.js | Ribbon tabs and groups once each, Inspect pinned, narrow window folds groups into » | 4a | tests/ui_shell.rs `ribbon_tools_match_ribbon_def`, `ribbon_group_names_and_tabs_match_ribbon_def`, `each_tool_is_on_one_tab_only`, `ribbon_tabs_show_their_tools_and_inspect_is_pinned`, `narrow_window_folds_groups_into_more` | pass |
| sf11.js | Support Fins: suggest orientations, strength, support plastic, gussets, anchors | 4b | rust/crates/stl-app/tests/ui/sf11.rs | panel/pose/plastic/about, Suggest orientations click, Use, and the strength load vector; nothing parked |
| sf9.js | Support Fins modes: Rotate, Add fins, Remove fins; Move applies on release | 4b | rust/crates/stl-app/tests/ui/sf9.rs | mode switching + Move tool + a click on a fin in Remove mode; nothing parked |
| sfcov.js | Support Fins coverage: bare spots, Full coverage, Bare beyond | 4b | rust/crates/stl-app/tests/ui/sfcov.rs | Held card/reach, Full coverage, then a stricter Bare beyond (the bare-spot numbers against 1.x's goldens are `supports_coverage`); nothing parked |
| sfpanel.js | Support Fins panel: mode buttons, Held card, folding sections remembered, footer credit | 4b | rust/crates/stl-app/tests/ui/sfpanel.rs | modes/credit/Held/order and folding sections remembered across reopen and across runs (platform setting `stle_sections`, as 1.x's localStorage); nothing parked |
| skdim.js | Smart Dimension, part edges, reference dims, Use Edge, parameters, edges found again | 4c | rust/crates/stl-app/tests/ui/skdim.rs | pass (dims, over-defined→reference when dialog present, Use Edge tool, Save step JSON) |
| skdim2.js | Typed sizes become dimensions, double-click to edit, silhouette edges, dark theme | 4c | rust/crates/stl-app/tests/ui/skdim2.rs | pass (typed-size outcomes 30/15/6/12, edit to 35, dark) |
| tap.js | Phone/tablet layout: import, tap to select, double-tap into sketch, draw by taps | 4c | rust/crates/stl-app/tests/ui/tap.rs | pass (iPad size, select, enter sketch, rect by taps) |
| touch.js | Tablet and phone layout, browser drawer, one-finger orbit; opens a tool to check the panel layout | 4b | rust/crates/stl-app/tests/ui/touch.rs | hollow panel + orbit on tablet sizes; the browser drawer is un-parked: `touch_browser_drawer_toggle` (both phone orientations get the Bodies / Close button, tablets keep the floating panel; milestone 5, below) |
| v110.js | SUPPORTS tab, Export and Send to Slicer menus, Support Fins edited not stacked | 4b | rust/crates/stl-app/tests/ui/v110.rs | SUPPORTS tools, edit-not-stack, Draw Walls reopen, Export smoke, and rotate after fins (the turn goes before the fins step, which is rebuilt); nothing parked |
| web2.js | Web tools: import, hole, sketch cut, threads, export, save and reopen round trip | 4c | rust/crates/stl-app/tests/ui/web2.rs | pass (import, hole, sketch+Project dialog, thread, undo/redo, Save/Export) |
| web4.js | Web files: save in place, recent files, autosave and recovery | 4a | tests/files_keys.rs `save_reuses_path`, `save_as_always_asks`; tests/ui_shell.rs `recent_files_open_again_from_the_file_menu`; tests/web_worker.rs | save and session recent files pass; autosave, recovery and lasting recent files are in the milestone 5 section below (desktop; the web edition's copy in IndexedDB is not browser-tested here) |
| web6.js | Published web app: installable, works offline, update bar | 4a | none yet | deferred: installable app, offline and update bar come with publishing /app2/ (needs Nathan's yes); the desktop update banner is in the milestone 5 section below |
| web7.js | Support Fins panel from the engine schema, preview, edit, Draw Wall | 4b | rust/crates/stl-app/tests/ui/web7.rs | pass |
| web8.js | Instant red overhangs, fins-only preview, Draw Walls lines / multi-line / curve | 4b | rust/crates/stl-app/tests/ui/web8.rs | overhang session, Draw Walls line clicks on the part, wall curve values, fins preview then OK adds plastic; nothing parked |

## Batch notes (4a coverage in later batches)

- **desktop_flow.js (4c):** 4a covers import and export.
- **desktop_new.js (4c):** 4a covers nothing shell-only; hole, push/pull, split, fit to bed and sculpt are 4b, sketch parts are 4c.
- **desktop_place.js (4b):** 4a covers undo after the edit.
- **desktop_tools.js (4b):** 4a covers section view, undo/redo and the File menu.
- **desktop_v11.js (4c):** 4a covers inch mode, dark theme and the update banner.
- **tap.js (4c):** 4a covers import and tap-to-select.
- **touch.js (4b):** 4a covers tablet/phone layout and one-finger orbit. The browser drawer came with milestone 5 (see its section).
- **desktop_tools.js (4b/4c):** sketch + Extrude timeline edit passes (sketch on body face → join; 8→12 changes volume). 1.x's same-named step edits a hole diameter from the timeline.
- **v110.js (4b):** 4a covers ribbon tabs and the Export / Send to Slicer menus.
- **desktop_flow.js (4c):** passes with centred top-face origin [0,0,20] (1.x clicked [20,15,20] before import centring).
- **web2.js (4c):** 4a covers import, export, and save/reopen; hole and thread panels are 4b.

## Milestone 4a fixes (2026-11-13)

Checks added for what the 4a screenshots showed. None of them maps to a single 1.x test file.

- **Title, swatches, Parameters list, About credits:** `tests/m4a_fix_t1.rs` (`title_after_each_open_path`, `failed_open_keeps_title`, `swatch_matches_viewport_colour`, `params_listed_expanded`, `about_lists_rust_crates_only`). Pass.
- **Start view, grid and axes, section cut face:** `tests/m4a_fix_t5.rs` (`start_page_uses_iso_view`, `start_grid_and_axes`, `section_cap_shows_grid`, `grid_through_body`, dark and light). Pass.
- **WebGL2:** `cargo run -p xtask -- web --check --webgl2` (both bodies in the tree, a pick at the block's centre hits it, section view changes the picture). Pass.
- **Screenshots:** `dev/m4a_shots/capture_m4a_shots.mjs` and `tests/m4a_shots.rs` reach every state and fail when two states give the same picture (`states_differ`). Pass.

## Milestone 5 (platform and a local test build, 2026-11-14)

Features that 1.x has in `web/js/app.js` and `stleditor/*.py` but no browser test of its own (1.x's `tests/ui/*.js` reach them only in `web4.js`, `web6.js`, `desktop_v11.js` and `touch.js`, in part). Same columns; the 1.x source stands in for the file. Texts are 1.x's word for word; where 2.0 differs the row says so and `EXCEPTIONS.md` has it pending Nathan's OK. All run against a throwaway `STLEDITOR2_CONFIG_DIR` and the local fake release server, never Nathan's folders or GitHub.

| 1.x source | what it tests | 2.0 test | status |
| --- | --- | --- | --- |
| app.js `checkRecovery`, server.py `recovery/*` | Recover Unsaved Work: the "Recover unsaved work?" dialog lists a crashed session ("N steps · autosaved <date>", Discard, Recover, Later), Recover rebuilds the project and toasts "Recovered. Save it to keep it.", Help › Recover Unsaved Work… on demand, "There is no unsaved work to recover." | rust/crates/stl-app/tests/ui/recover.rs (`recover_dialog_lists_sessions`, `recover_later_discard_and_help_menu`, `a_clean_start_shows_no_dialog`); engine side and the dialog's own details in tests/recover.rs | pass (a real crashed window's copy, the whole app; a running window's session is never offered: stl-platform `tests/autosave.rs` `live_session_not_offered`) |
| server.py `_remember`, `recent` setting; app.js File menu and start screen | Open Recent from disk: recent files lasting across runs in `settings.json`, the first six in the File menu and on the start screen by file name, ten kept, a click opens the file from disk and moves it to the top | rust/crates/stl-app/tests/ui/open_recent.rs (`open_recent_from_disk`) | pass |
| server.py `open` (a missing file's error) | A recent file that was moved or deleted: 1.x's own error text (`FileNotFoundError: [Errno 2] No such file or directory: '<path>'`), and the file is dropped from the list and from `settings.json` | rust/crates/stl-app/tests/ui/open_recent.rs (`missing_recent_dropped`) | pass (1.x shows the error and keeps the entry; dropping it is 2.0's rule, spec 2.2) |
| app.js `checkUpdates`, `showUpdateBanner`, `updateSettings`; server.py `update/*` | Check for Updates and Update Settings: the Help items, the banner ("STL Editor <v> is available", "You have <v>." and the install line, Skip this version / Later / What’s new / Update now), "You have the latest version (<v>).", a failed download shows why and keeps the exe, Download instead for a copy that can't swap itself, the settings box and "Update checks are off." | rust/crates/stl-app/tests/ui/updates.rs (all of `ui::updates`); the platform side in tests/updates_app.rs | pass against the fake release server. Not run through `StlApp` itself: `enable_autosave` builds its updater against GitHub with no way to point it elsewhere. The settings dialog's wording is 2.0's (on/off; 1.x asked for a repository), pending Nathan's OK |
| touch.js `setupDrawer` | The phone drawer: on narrow windows the browser is a drawer opened by a "Bodies" button over the model ("Close" while open), as 1.x | rust/crates/stl-app/tests/ui/touch.rs (`touch_browser_drawer_toggle`, `touch_drawer_breakpoint`) | pass |
| app.js `sketchText` font list | Sketch text fonts: the desktop font list, "More: fonts on this computer…" and "Add a font file…", an added font remembered in settings | rust/crates/stl-app/tests/sketch_fonts.rs | pass |
| (2.0 rule, review focus 6) | 1.x's folders are never touched: settings, recent, autosave, Recover and an update check, then `xtask install` / `uninstall --purge`, leave every 1.x file byte for byte as it was and add none | rust/crates/stl-app/tests/leaves_1x_alone.rs and rust/xtask/tests/leaves_1x_alone.rs | pass |
| (2.0 rule, smoothness) | No frame over 16.7 ms while autosave writes the bench project during an orbit (`xtask uibench`, row "autosave"), and the dialogs above draw on the bundled fonts alone | rust/crates/stl-app/src/bench.rs, rust/xtask/src/uibench.rs; rust/crates/stl-app/tests/fonts_fallback.rs | pass (the row is in the results table; the fonts test fails on a glyph no bundled font has) |

## Counts

- **4a:** 6
- **4b:** 12
- **4c:** 8
- **Total:** 26 (the milestone 5 rows above are extra and not counted)
