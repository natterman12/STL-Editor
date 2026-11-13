# M4a shell/viewport screenshots

Window size: 1280×800. 1.x from `node dev/m4a_shots/capture_m4a_shots.mjs`; 2.0 from `cargo test -p stl-app --test m4a_shots -- --ignored`.

| State / theme | 1.x | 2.0 |
| --- | --- | --- |
| start / dark | [start_dark_1x.png](start_dark_1x.png) | [start_dark_2x.png](start_dark_2x.png) |
| start / light | [start_light_1x.png](start_light_1x.png) | [start_light_2x.png](start_light_2x.png) |
| project / dark | [project_dark_1x.png](project_dark_1x.png) | [project_dark_2x.png](project_dark_2x.png) |
| project / light | [project_light_1x.png](project_light_1x.png) | [project_light_2x.png](project_light_2x.png) |
| rename / dark | [rename_dark_1x.png](rename_dark_1x.png) | [rename_dark_2x.png](rename_dark_2x.png) |
| rename / light | [rename_light_1x.png](rename_light_1x.png) | [rename_light_2x.png](rename_light_2x.png) |
| timeline / dark | [timeline_dark_1x.png](timeline_dark_1x.png) | [timeline_dark_2x.png](timeline_dark_2x.png) |
| timeline / light | [timeline_light_1x.png](timeline_light_1x.png) | [timeline_light_2x.png](timeline_light_2x.png) |
| parameters / dark | [parameters_dark_1x.png](parameters_dark_1x.png) | [parameters_dark_2x.png](parameters_dark_2x.png) |
| parameters / light | [parameters_light_1x.png](parameters_light_1x.png) | [parameters_light_2x.png](parameters_light_2x.png) |
| section / dark | [section_dark_1x.png](section_dark_1x.png) | [section_dark_2x.png](section_dark_2x.png) |
| section / light | [section_light_1x.png](section_light_1x.png) | [section_light_2x.png](section_light_2x.png) |

## Remaining visible differences (1.x vs 2.0)

Dark and light match on these points unless a theme is named.

### start
- 1.x shows the web-only “Web edition · runs in your browser…” toast above the History bar (with a Desktop app link); 2.0 has no toast there because these 2.0 shots are the native app.
- The start modal copy and the two buttons match; ribbon groups, view buttons, Browser (Bodies 0), and History line up the same way with only egui vs HTML spacing differences.

### project
- Top-bar title and Browser root are both `basic.stlx`; bodies (`holed_block` 276, `two_blobs` 1k), swatches, and `dx = 5` / `spare = 1` match.
- 1.x keeps the web-only Web edition toast; 2.0 does not.
- Viewport content matches (holed block, two blobs, RGB axes, floor grid); Browser row spacing and count placement differ slightly (HTML vs egui).

### rename
- Both have finished the rename: Browser shows `Renamed body` (276) and `two_blobs` (1k) with the rename dialog closed.
- Same title / Browser-root match and web-only toast gap as project.
- Viewport otherwise matches project.

### timeline
- Both rolled back so only `holed_block` remains (Bodies 1); parameters still list `dx` and `spare`.
- Same web-only toast gap as project.
- Mesh and grid look close; the blue Z axis is a bit clearer in 2.0, and the green edge line sits slightly differently relative to the holes.

### parameters
- Both open a Parameters dialog over the loaded project with the same Name / Value / Note layout (`dx` / `5` / `X shift`, `spare` / `1` / `unused`), `= 5 · used` / `= 1`, Add parameter, and Cancel plus blue Apply on the bottom right.
- Behind the dialog: same web-only toast gap; small egui vs HTML differences in field borders and text weight only.

### section
- 4b: 1.x shows the Section View tool panel on the right (Axis X/Y/Z with Z on, position 2.5 mm, Flip, Remove cut, Cancel, Done) and the “Drag the slider to look inside…” help strip; 2.0 has Section on in the view bar but is missing that panel and help strip.
- Both paint a salmon cut face on the block and the clipped blob with the floor grid visible on the cut (matching 1.x).
- Same web-only toast gap as project.
