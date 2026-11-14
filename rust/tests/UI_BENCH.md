# UI bench (`xtask uibench`)

Results from `cargo run --release -p xtask -- uibench` on Nathan's PC. Each run appends a dated table. No results yet.

## Rows

| Scenario | What it drives |
| --- | --- |
| orbit 10 s | Left-drag orbit on the 504k-triangle bench mesh for ten seconds |
| pan 5 s | Middle/right-drag pan for five seconds |
| zoom 5 s | Cursor-centred zoom for five seconds |
| gizmo drag | Move-tool handle drag on the bench mesh (`scenarios.gizmo_drag` in `src/bench.rs`) |
| open tool | Open each tool once on the bench mesh and wait for its first preview (`scenarios.open_tool` in `src/bench.rs`). A tool that never asks for a preview is cancelled after 0.8 s, and one that has not answered after 20 s is given up on; a quick run does the first six tools. The frames counted are the ones in between; the report lists every tool with its outcome (`open_tool_tools`) |
| enter sketch | Enter a sketch on the bench mesh (look-at + grid); frames until settled (`scenarios.enter_sketch`) |
| sketch drag | Drag a free point in a 200-entity / 100-constraint sketch; every frame target under 16.7 ms (`scenarios.sketch_drag`) |
| idle 5 s | Five seconds with no input |
| Fillet rebuild | One Fillet rebuild on the bench mesh |
| launch to first frame | Time from process start until the first painted frame |

## Columns

| Column | Meaning |
| --- | --- |
| frames | Frame count during the scenario (idle should be 0) |
| median ms | Median frame time in milliseconds |
| p99 ms | 99th-percentile frame time in milliseconds |
| longest ms | Longest single frame in milliseconds |
| idle frames | Frames drawn while idle (target: 0) |
| uploads | GPU mesh uploads (only bodies whose mesh changed) |

## Spec 2.3 targets

- Orbit, pan and zoom run at the display refresh rate, at least 60 frames a second; 99th-percentile frame time under 16.7 ms on the bench mesh.
- Idle: no frames drawn; CPU use near zero while nothing changes.
- The UI thread never waits on the engine: a rebuild never stalls a frame over 16.7 ms.
- Uploads: only bodies whose mesh changed are uploaded again.
- Start-up: the window shows within 1 second of launch on Windows.

## Results

### 2026-11-12 (quick)

_stl-engine has no Fillet yet; used move 0.1 mm so the mesh revises_

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 235 | 4.14 | 10.08 | 22.73 | 0 | 1 |
| pan 5 s | 240 | 4.15 | 5.00 | 5.33 | 0 | 1 |
| zoom 5 s | 241 | 4.14 | 4.99 | 5.94 | 0 | 1 |
| idle 5 s | 3 | 4.34 | 992.84 | 992.84 | 3 | 1 |
| Fillet rebuild | 21 | 4.11 | 90303.48 | 90303.48 | 0 | 1 |
| launch to first frame | — | 734.85 | — | — | — | — |

### 2026-11-12 (quick)

_stl-engine has no Fillet yet; used move 0.1 mm so the mesh revises_

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 241 | 4.16 | 4.80 | 5.02 | 0 | 1 |
| pan 5 s | 240 | 4.17 | 5.04 | 5.30 | 0 | 1 |
| zoom 5 s | 241 | 4.18 | 4.77 | 6.29 | 0 | 1 |
| idle 5 s | 6 | 250.32 | 250.52 | 250.52 | 6 | 1 |
| Fillet rebuild | 18 | 3.99 | 26080.98 | 26080.98 | 0 | 1 |
| launch to first frame | — | 0.28 | — | — | — | — |


### 2026-11-12 (quick)

_stl-engine has no Fillet yet; used move 0.1 mm so the mesh revises_

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 241 | 4.15 | 4.73 | 7.82 | 0 | 1 |
| pan 5 s | 240 | 4.16 | 4.68 | 9.80 | 0 | 1 |
| zoom 5 s | 241 | 4.15 | 4.70 | 8.45 | 0 | 1 |
| idle 5 s | 3 | 4.31 | 992.13 | 992.13 | 3 | 1 |
| Fillet rebuild | 16 | 4.13 | 313947.85 | 313947.85 | 0 | 1 |
| launch to first frame | — | 542.60 | — | — | — | — |

### 2026-11-12 (quick)

_stl-engine has no Fillet yet; used offset 0.2 mm grow for a real slow evaluation_

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 241 | 4.16 | 4.66 | 7.74 | 0 | 1 |
| pan 5 s | 241 | 4.17 | 4.84 | 6.79 | 0 | 1 |
| zoom 5 s | 240 | 4.17 | 4.71 | 10.35 | 0 | 1 |
| idle 5 s | 3 | 4.02 | 992.66 | 992.66 | 3 | 1 |
| Fillet rebuild | 1471 | 4.17 | 5.61 | 364386.27 | 0 | 1 |
| launch to first frame | — | 510.34 | — | — | — | — |

### 2026-11-12

_stl-engine has no Fillet yet; used offset 0.2 mm grow for a real slow evaluation_

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 2401 | 4.17 | 4.78 | 14.89 | 0 | 1 |
| pan 5 s | 1201 | 4.17 | 4.78 | 12.47 | 0 | 1 |
| zoom 5 s | 1200 | 4.17 | 4.92 | 11.42 | 0 | 1 |
| idle 5 s | 3 | 4.13 | 4992.25 | 4992.25 | 3 | 1 |
| Fillet rebuild | 1706 | 4.15 | 6.83 | 301880.31 | 0 | 1 |
| launch to first frame | — | 570.14 | — | — | — | — |

### 2026-11-12

_stl-engine has no Fillet yet; used offset 0.2 mm grow for a real slow evaluation_

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 2401 | 4.16 | 4.91 | 14.24 | 0 | 1 |
| pan 5 s | 1200 | 4.15 | 5.25 | 12.45 | 0 | 1 |
| zoom 5 s | 1200 | 4.17 | 4.86 | 13.11 | 0 | 1 |
| idle 5 s | 6 | 4.48 | 4593.07 | 4593.07 | 6 | 1 |
| Fillet rebuild | 1601 | 4.16 | 5.93 | 2001.42 | 0 | 2 |
| launch to first frame | — | 542.02 | — | — | — | — |

### 2026-11-12

_stl-engine has no Fillet yet; used offset 0.2 mm grow for a real slow evaluation_

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 2401 | 4.16 | 4.79 | 13.13 | 0 | 1 |
| pan 5 s | 1200 | 4.16 | 4.73 | 11.21 | 0 | 1 |
| zoom 5 s | 1201 | 4.17 | 4.72 | 11.87 | 0 | 1 |
| idle 5 s | 3 | 4.29 | 4992.19 | 4992.19 | 3 | 1 |
| Fillet rebuild | 1600 | 4.16 | 6.52 | 199075.80 | 0 | 1 |
| launch to first frame | — | 530.11 | — | — | — | — |

### 2026-11-12 (quick)

_stl-engine has no Fillet. Release engine times on this 504k-triangle mesh: simplify 50% 376 ms; hollow draft 636 ms; hollow normal 1.43 s; hollow fine 4.16 s; remesh edge 2 mm 3.09 s (down to 5k triangles); remesh edge 1 mm 2.35 s. Offset 0.2 mm took about 200–360 s in earlier full runs, and offset 5 mm was still running at 45 s: a 0.2 mm grow only simplifies within 0.08 mm, so the Minkowski sum still sees most of the triangles. This row is Hollow, wall 2 mm, fine quality. Frame times are the frames that kept orbiting until the new mesh was uploaded and drawn._

_Idle frames are paints something other than this bench asked for. Not counted: the frame already queued by the last zoom, egui's extra settle frame after an immediate repaint, and the wake this bench schedules so it can leave the idle window. Toasts are dismissed and any camera flight is finished as idle starts._

_This run: hollow took 4171 ms from the command until the new mesh was uploaded and drawn. Frame times on that row are the orbiting frames in between._

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 241 | 4.17 | 4.53 | 7.78 | 0 | 1 |
| pan 5 s | 241 | 4.17 | 4.50 | 9.94 | 0 | 1 |
| zoom 5 s | 240 | 4.16 | 4.63 | 10.60 | 0 | 1 |
| idle 5 s | 0 | 0.00 | 0.00 | 0.00 | 0 | 1 |
| hollow rebuild | 901 | 4.14 | 24.15 | 34.39 | 0 | 2 |
| launch to first frame | — | 505.84 | — | — | — | — |

### 2026-11-12 (quick)

_stl-engine has no Fillet. Release engine times on this 504k-triangle mesh: simplify 50% 376 ms; hollow draft 636 ms; hollow normal 1.43 s; hollow fine 4.16 s; remesh edge 2 mm 3.09 s (down to 5k triangles); remesh edge 1 mm 2.35 s. Offset 0.2 mm took about 200–360 s in earlier full runs, and offset 5 mm was still running at 45 s: a 0.2 mm grow only simplifies within 0.08 mm, so the Minkowski sum still sees most of the triangles. This row is Hollow, wall 2 mm, fine quality. Frame times are the frames that kept orbiting until the new mesh was uploaded and drawn._

_Idle frames are paints something other than this bench asked for. Not counted: the frame already queued by the last zoom, egui's extra settle frame after an immediate repaint, and the wake this bench schedules so it can leave the idle window. Toasts are dismissed and any camera flight is finished as idle starts._

_This run: hollow took 4146 ms from the command until the new mesh was uploaded and drawn. Frame times on that row are the orbiting frames in between._

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 241 | 4.17 | 4.81 | 7.89 | 0 | 1 |
| pan 5 s | 241 | 4.17 | 4.44 | 7.35 | 0 | 1 |
| zoom 5 s | 241 | 4.17 | 4.53 | 9.79 | 0 | 1 |
| idle 5 s | 0 | 0.00 | 0.00 | 0.00 | 0 | 1 |
| hollow rebuild | 869 | 4.14 | 29.47 | 35.08 | 0 | 2 |
| launch to first frame | — | 506.74 | — | — | — | — |

### 2026-11-12 (quick)

_stl-engine has no Fillet. Release engine times on this 504k-triangle mesh: simplify 50% 376 ms; hollow draft 636 ms; hollow normal 1.43 s; hollow fine 4.16 s; remesh edge 2 mm 3.09 s (down to 5k triangles); remesh edge 1 mm 2.35 s. Offset 0.2 mm took about 200–360 s in earlier full runs, and offset 5 mm was still running at 45 s: a 0.2 mm grow only simplifies within 0.08 mm, so the Minkowski sum still sees most of the triangles. This row is Hollow, wall 2 mm, fine quality. Frame times are the frames that kept orbiting until the new mesh was uploaded and drawn._

_Idle frames are paints something other than this bench asked for. Not counted: the frame already queued by the last zoom, egui's extra settle frame after an immediate repaint, and the wake this bench schedules so it can leave the idle window. Toasts are dismissed and any camera flight is finished as idle starts._

_This run: hollow took 4247 ms from the command until the new mesh was uploaded and drawn. Frame times on that row are the orbiting frames in between._

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 241 | 4.17 | 4.46 | 8.92 | 0 | 1 |
| pan 5 s | 241 | 4.17 | 4.62 | 9.46 | 0 | 1 |
| zoom 5 s | 240 | 4.16 | 4.55 | 7.41 | 0 | 1 |
| idle 5 s | 0 | 0.00 | 0.00 | 0.00 | 0 | 1 |
| hollow rebuild | 1002 | 4.17 | 7.39 | 34.36 | 0 | 2 |
| launch to first frame | — | 502.95 | — | — | — | — |

### 2026-11-12 (quick)

_stl-engine has no Fillet. Release engine times on this 504k-triangle mesh: simplify 50% 376 ms; hollow draft 636 ms; hollow normal 1.43 s; hollow fine 4.16 s; remesh edge 2 mm 3.09 s (down to 5k triangles); remesh edge 1 mm 2.35 s. Offset 0.2 mm took about 200–360 s in earlier full runs, and offset 5 mm was still running at 45 s: a 0.2 mm grow only simplifies within 0.08 mm, so the Minkowski sum still sees most of the triangles. This row is Hollow, wall 2 mm, fine quality. Frame times are the frames that kept orbiting until the new mesh was uploaded and drawn._

_Idle frames are paints something other than this bench asked for. Not counted: the frame already queued by the last zoom, egui's extra settle frame after an immediate repaint, and the wake this bench schedules so it can leave the idle window. Toasts are dismissed and any camera flight is finished as idle starts._

_This run: hollow took 4201 ms from the command until the new mesh was uploaded and drawn. Frame times on that row are the orbiting frames in between._

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 241 | 4.17 | 4.55 | 9.26 | 0 | 1 |
| pan 5 s | 240 | 4.17 | 4.64 | 7.02 | 0 | 1 |
| zoom 5 s | 241 | 4.16 | 4.58 | 10.39 | 0 | 1 |
| idle 5 s | 0 | 0.00 | 0.00 | 0.00 | 0 | 1 |
| hollow rebuild | 997 | 4.17 | 6.87 | 29.66 | 0 | 2 |
| launch to first frame | — | 503.38 | — | — | — | — |

### 2026-11-12

_stl-engine has no Fillet. Release engine times on this 504k-triangle mesh: simplify 50% 376 ms; hollow draft 636 ms; hollow normal 1.43 s; hollow fine 4.16 s; remesh edge 2 mm 3.09 s (down to 5k triangles); remesh edge 1 mm 2.35 s. Offset 0.2 mm took about 200–360 s in earlier full runs, and offset 5 mm was still running at 45 s: a 0.2 mm grow only simplifies within 0.08 mm, so the Minkowski sum still sees most of the triangles. This row is Hollow, wall 2 mm, fine quality. Frame times are the frames that kept orbiting until the new mesh was uploaded and drawn. The bench raises the window thread above the engine's rayon pool. Without that, dozens of frames sit at 20–35 ms while Hollow fills every core. Even then, one frame still reaches about 30 ms partway through Hollow, with nothing uploading._

_Idle frames are paints something other than this bench asked for. Not counted: the frame already queued by the last zoom, egui's extra settle frame after an immediate repaint, and the wake this bench schedules so it can leave the idle window. Toasts are dismissed and any camera flight is finished as idle starts._

_This run: hollow took 4205 ms from the command until the new mesh was uploaded and drawn. Frame times on that row are the orbiting frames in between._

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 2401 | 4.17 | 4.54 | 10.77 | 0 | 1 |
| pan 5 s | 1200 | 4.17 | 4.55 | 10.77 | 0 | 1 |
| zoom 5 s | 1201 | 4.17 | 4.69 | 12.87 | 0 | 1 |
| idle 5 s | 0 | 0.00 | 0.00 | 0.00 | 0 | 1 |
| hollow rebuild | 999 | 4.17 | 5.64 | 32.25 | 0 | 2 |
| launch to first frame | — | 498.83 | — | — | — | — |

### 2026-11-12

_stl-engine has no Fillet. Release engine times on this 504k-triangle mesh: simplify 50% 376 ms; hollow draft 636 ms; hollow normal 1.43 s; hollow fine 4.16 s; remesh edge 2 mm 3.09 s (down to 5k triangles); remesh edge 1 mm 2.35 s. Offset 0.2 mm took about 200–360 s in earlier full runs, and offset 5 mm was still running at 45 s: a 0.2 mm grow only simplifies within 0.08 mm, so the Minkowski sum still sees most of the triangles. This row is Hollow, wall 2 mm, fine quality. Frame times are the frames that kept orbiting until the new mesh was uploaded and drawn. The window keeps one physical core: the main thread is pinned to it, Rayon's pool is sized to the other logical processors, and the engine thread and threads started after it run at a lower priority on those processors. The bench does not raise the window thread; this is the scheduling the app uses._

_Idle frames are paints something other than this bench asked for. Not counted: the frame already queued by the last zoom, egui's extra settle frame after an immediate repaint, and the wake this bench schedules so it can leave the idle window. Toasts are dismissed and any camera flight is finished as idle starts._

_This run: hollow took 5465 ms from the command until the new mesh was uploaded and drawn. Frame times on that row are the orbiting frames in between._

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 2400 | 4.16 | 5.34 | 18.19 | 0 | 1 |
| pan 5 s | 1201 | 4.17 | 4.77 | 11.74 | 0 | 1 |
| zoom 5 s | 1201 | 4.17 | 4.77 | 10.90 | 0 | 1 |
| idle 5 s | 0 | 0.00 | 0.00 | 0.00 | 0 | 1 |
| hollow rebuild | 1230 | 4.17 | 23.93 | 36.99 | 0 | 2 |
| launch to first frame | — | 637.19 | — | — | — | — |

### 2026-11-12

_stl-engine has no Fillet. Release engine times on this 504k-triangle mesh: simplify 50% 376 ms; hollow draft 636 ms; hollow normal 1.43 s; hollow fine 4.16 s; remesh edge 2 mm 3.09 s (down to 5k triangles); remesh edge 1 mm 2.35 s. Offset 0.2 mm took about 200–360 s in earlier full runs, and offset 5 mm was still running at 45 s: a 0.2 mm grow only simplifies within 0.08 mm, so the Minkowski sum still sees most of the triangles. This row is Hollow, wall 2 mm, fine quality. Frame times are the frames that kept orbiting until the new mesh was uploaded and drawn. Rayon's pool is one thread per physical core when the chip has hyperthreads (otherwise one processor is kept back), and the engine thread and threads started after it run below the window's priority. The window thread is not pinned or raised._

_Idle frames are paints something other than this bench asked for. Not counted: the frame already queued by the last zoom, egui's extra settle frame after an immediate repaint, and the wake this bench schedules so it can leave the idle window. Toasts are dismissed and any camera flight is finished as idle starts._

_This run: hollow took 6294 ms from the command until the new mesh was uploaded and drawn. Frame times on that row are the orbiting frames in between._

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 2401 | 4.16 | 5.04 | 11.98 | 0 | 1 |
| pan 5 s | 1200 | 4.17 | 4.89 | 10.61 | 0 | 1 |
| zoom 5 s | 1201 | 4.17 | 4.86 | 11.73 | 0 | 1 |
| idle 5 s | 0 | 0.00 | 0.00 | 0.00 | 0 | 1 |
| hollow rebuild | 1511 | 4.16 | 5.61 | 19.02 | 0 | 2 |
| launch to first frame | — | 526.14 | — | — | — | — |

### 2026-11-12

_stl-engine has no Fillet. Release engine times on this 504k-triangle mesh: simplify 50% 376 ms; hollow draft 636 ms; hollow normal 1.43 s; hollow fine 4.16 s; remesh edge 2 mm 3.09 s (down to 5k triangles); remesh edge 1 mm 2.35 s. Offset 0.2 mm took about 200–360 s in earlier full runs, and offset 5 mm was still running at 45 s: a 0.2 mm grow only simplifies within 0.08 mm, so the Minkowski sum still sees most of the triangles. This row is Hollow, wall 2 mm, fine quality. Frame times are the frames that kept orbiting until the new mesh was uploaded and drawn. Rayon's pool keeps two physical cores free when the chip has hyperthreads (otherwise one processor is kept back). On Windows the window thread runs above normal priority and is not pinned. The engine thread and threads started after it run below the window._

_Idle frames are paints something other than this bench asked for. Not counted: the frame already queued by the last zoom, egui's extra settle frame after an immediate repaint, and the wake this bench schedules so it can leave the idle window. Toasts are dismissed and any camera flight is finished as idle starts._

_This run: hollow took 7051 ms from the command until the new mesh was uploaded and drawn. Frame times on that row are the orbiting frames in between._

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 2401 | 4.17 | 4.82 | 12.31 | 0 | 1 |
| pan 5 s | 1200 | 4.17 | 4.86 | 11.88 | 0 | 1 |
| zoom 5 s | 1201 | 4.16 | 4.77 | 11.06 | 0 | 1 |
| idle 5 s | 0 | 0.00 | 0.00 | 0.00 | 0 | 1 |
| hollow rebuild | 1695 | 4.16 | 5.49 | 16.09 | 0 | 2 |
| launch to first frame | — | 539.07 | — | — | — | — |

### 2026-11-12

_stl-engine has no Fillet. Release engine times on this 504k-triangle mesh: simplify 50% 376 ms; hollow draft 636 ms; hollow normal 1.43 s; hollow fine 4.16 s; remesh edge 2 mm 3.09 s (down to 5k triangles); remesh edge 1 mm 2.35 s. Offset 0.2 mm took about 200–360 s in earlier full runs, and offset 5 mm was still running at 45 s: a 0.2 mm grow only simplifies within 0.08 mm, so the Minkowski sum still sees most of the triangles. This row is Hollow, wall 2 mm, fine quality. Frame times are the frames that kept orbiting until the new mesh was uploaded and drawn. Rayon's pool keeps two physical cores free when the chip has hyperthreads (otherwise one processor is kept back). On Windows the window thread runs above normal priority and is not pinned. The engine thread and its Rayon workers run below the window. Other threads stay at normal priority._

_Idle frames are paints something other than this bench asked for. Not counted: the frame already queued by the last zoom, egui's extra settle frame after an immediate repaint, and the wake this bench schedules so it can leave the idle window. Toasts are dismissed and any camera flight is finished as idle starts._

_This run: hollow took 6951 ms from the command until the new mesh was uploaded and drawn. Frame times on that row are the orbiting frames in between._

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 2401 | 4.16 | 4.79 | 12.99 | 0 | 1 |
| pan 5 s | 1200 | 4.15 | 4.83 | 9.96 | 0 | 1 |
| zoom 5 s | 1200 | 4.16 | 4.80 | 10.86 | 0 | 1 |
| idle 5 s | 0 | 0.00 | 0.00 | 0.00 | 0 | 1 |
| hollow rebuild | 1670 | 4.16 | 4.72 | 15.72 | 0 | 2 |
| launch to first frame | — | 530.85 | — | — | — | — |

### 2026-11-12

_stl-engine has no Fillet. Release engine times on this 504k-triangle mesh: simplify 50% 376 ms; hollow draft 636 ms; hollow normal 1.43 s; hollow fine 4.16 s; remesh edge 2 mm 3.09 s (down to 5k triangles); remesh edge 1 mm 2.35 s. Offset 0.2 mm took about 200–360 s in earlier full runs, and offset 5 mm was still running at 45 s: a 0.2 mm grow only simplifies within 0.08 mm, so the Minkowski sum still sees most of the triangles. This row is Hollow, wall 2 mm, fine quality. Frame times are the frames that kept orbiting until the new mesh was uploaded and drawn. Rayon's pool keeps two physical cores free when the chip has hyperthreads (otherwise one processor is kept back). On Windows the window thread runs above normal priority and is not pinned. The engine thread and its Rayon workers run below the window. Other threads stay at normal priority._

_Idle frames are paints something other than this bench asked for. Not counted: the frame already queued by the last zoom, egui's extra settle frame after an immediate repaint, and the wake this bench schedules so it can leave the idle window. Toasts are dismissed and any camera flight is finished as idle starts._

_This run: hollow took 7143 ms from the command until the new mesh was uploaded and drawn. Frame times on that row are the orbiting frames in between._

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 2401 | 4.16 | 4.76 | 11.46 | 0 | 1 |
| pan 5 s | 1201 | 4.16 | 4.57 | 11.38 | 0 | 1 |
| zoom 5 s | 1200 | 4.16 | 4.62 | 14.00 | 0 | 1 |
| idle 5 s | 0 | 0.00 | 0.00 | 0.00 | 0 | 1 |
| hollow rebuild | 1715 | 4.16 | 4.93 | 23.33 | 0 | 2 |
| launch to first frame | — | 540.69 | — | — | — | — |

### 2026-11-12

_stl-engine has no Fillet. Release engine times on this 504k-triangle mesh: simplify 50% 376 ms; hollow draft 636 ms; hollow normal 1.43 s; hollow fine 4.16 s; remesh edge 2 mm 3.09 s (down to 5k triangles); remesh edge 1 mm 2.35 s. Offset 0.2 mm took about 200–360 s in earlier full runs, and offset 5 mm was still running at 45 s: a 0.2 mm grow only simplifies within 0.08 mm, so the Minkowski sum still sees most of the triangles. This row is Hollow, wall 2 mm, fine quality. Frame times are the frames that kept orbiting until the new mesh was uploaded and drawn. Rayon's pool keeps two physical cores free when the chip has hyperthreads (otherwise one processor is kept back). On Windows the window thread runs above normal priority and is not pinned. The engine thread and its Rayon workers run below the window. Other threads stay at normal priority._

_Idle frames are paints something other than this bench asked for. Not counted: the frame already queued by the last zoom, egui's extra settle frame after an immediate repaint, and the wake this bench schedules so it can leave the idle window. Toasts are dismissed and any camera flight is finished as idle starts._

_This run: hollow took 6805 ms from the command until the new mesh was uploaded and drawn. Frame times on that row are the orbiting frames in between._

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 2401 | 4.17 | 4.86 | 11.71 | 0 | 1 |
| pan 5 s | 1200 | 4.17 | 4.74 | 10.50 | 0 | 1 |
| zoom 5 s | 1201 | 4.17 | 4.75 | 11.30 | 0 | 1 |
| idle 5 s | 0 | 0.00 | 0.00 | 0.00 | 0 | 1 |
| hollow rebuild | 1636 | 4.16 | 4.87 | 15.81 | 0 | 2 |
| launch to first frame | — | 530.71 | — | — | — | — |

### 2026-11-12

_stl-engine has no Fillet. Release engine times on this 504k-triangle mesh: simplify 50% 376 ms; hollow draft 636 ms; hollow normal 1.43 s; hollow fine 4.16 s; remesh edge 2 mm 3.09 s (down to 5k triangles); remesh edge 1 mm 2.35 s. Offset 0.2 mm took about 200–360 s in earlier full runs, and offset 5 mm was still running at 45 s: a 0.2 mm grow only simplifies within 0.08 mm, so the Minkowski sum still sees most of the triangles. This row is Hollow, wall 2 mm, fine quality. Frame times are the frames that kept orbiting until the new mesh was uploaded and drawn. Rayon's pool keeps two physical cores free when the chip has hyperthreads (otherwise one processor is kept back). On Windows the window thread runs above normal priority and is not pinned. The engine thread and its Rayon workers run below the window. Other threads stay at normal priority._

_Idle frames are paints something other than this bench asked for. Not counted: the frame already queued by the last zoom, egui's extra settle frame after an immediate repaint, and the wake this bench schedules so it can leave the idle window. Toasts are dismissed and any camera flight is finished as idle starts._

_This run: hollow took 6651 ms from the command until the new mesh was uploaded and drawn. Frame times on that row are the orbiting frames in between._

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 2401 | 4.17 | 4.87 | 11.28 | 0 | 1 |
| pan 5 s | 1200 | 4.17 | 4.80 | 10.98 | 0 | 1 |
| zoom 5 s | 1200 | 4.16 | 4.73 | 10.93 | 0 | 1 |
| idle 5 s | 0 | 0.00 | 0.00 | 0.00 | 0 | 1 |
| hollow rebuild | 1599 | 4.16 | 4.68 | 11.98 | 0 | 2 |
| launch to first frame | — | 526.56 | — | — | — | — |

### 2026-11-13

_stl-engine has no Fillet. Release engine times on this 504k-triangle mesh: simplify 50% 376 ms; hollow draft 636 ms; hollow normal 1.43 s; hollow fine 4.16 s; remesh edge 2 mm 3.09 s (down to 5k triangles); remesh edge 1 mm 2.35 s. Offset 0.2 mm took about 200–360 s in earlier full runs, and offset 5 mm was still running at 45 s: a 0.2 mm grow only simplifies within 0.08 mm, so the Minkowski sum still sees most of the triangles. This row is Hollow, wall 2 mm, fine quality. Frame times are the frames that kept orbiting until the new mesh was uploaded and drawn. Rayon's pool keeps two physical cores free when the chip has hyperthreads, one physical core free without them, and two processors free where the layout can't be read. On Windows the window thread runs above normal priority and is not pinned. The engine thread and its Rayon workers run below the window. Other threads stay at normal priority._

_Idle frames are paints something other than this bench asked for. Not counted: the frame already queued by the last zoom, egui's extra settle frame after an immediate repaint, and the wake this bench schedules so it can leave the idle window. Toasts are dismissed and any camera flight is finished as idle starts._

_This run: hollow took 6465 ms from the command until the new mesh was uploaded and drawn. Frame times on that row are the orbiting frames in between._

_Slowest rebuild frame: 4.89 ms. The frame before it spent 0.00 ms on engine updates, 0.29 ms on the rest of the layout, 0.04 ms in the viewport's paint callback (upload and render), 0.49 ms on egui's own paint, and 4.06 ms waiting (the swapchain, the submit, and until the window thread ran again)._

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 2401 | 4.16 | 4.32 | 4.60 | 0 | 1 |
| pan 5 s | 1201 | 4.16 | 4.29 | 4.47 | 0 | 1 |
| zoom 5 s | 1201 | 4.17 | 4.31 | 4.47 | 0 | 1 |
| idle 5 s | 0 | 0.00 | 0.00 | 0.00 | 0 | 1 |
| hollow rebuild | 1555 | 4.16 | 4.45 | 4.89 | 0 | 2 |
| launch to first frame | — | 600.45 | — | — | — | — |

### 2026-11-13

_stl-engine has no Fillet. Release engine times on this 504k-triangle mesh: simplify 50% 376 ms; hollow draft 636 ms; hollow normal 1.43 s; hollow fine 4.16 s; remesh edge 2 mm 3.09 s (down to 5k triangles); remesh edge 1 mm 2.35 s. Offset 0.2 mm took about 200–360 s in earlier full runs, and offset 5 mm was still running at 45 s: a 0.2 mm grow only simplifies within 0.08 mm, so the Minkowski sum still sees most of the triangles. This row is Hollow, wall 2 mm, fine quality. Frame times are the frames that kept orbiting until the new mesh was uploaded and drawn. Rayon's pool keeps two physical cores free when the chip has hyperthreads, one physical core free without them, and two processors free where the layout can't be read. On Windows the window thread runs above normal priority and is not pinned. The engine thread and its Rayon workers run below the window. Other threads stay at normal priority._

_Idle frames are paints something other than this bench asked for. Not counted: the frame already queued by the last zoom, egui's extra settle frame after an immediate repaint, and the wake this bench schedules so it can leave the idle window. Toasts are dismissed and any camera flight is finished as idle starts._

_This run: hollow took 6107 ms from the command until the new mesh was uploaded and drawn. Frame times on that row are the orbiting frames in between._

_Slowest rebuild frame: 5.21 ms. The frame before it spent 0.00 ms on engine updates, 0.32 ms on the rest of the layout, 0.10 ms in the viewport's paint callback (upload and render), 0.68 ms on egui's own paint, and 4.11 ms waiting (the swapchain, the submit, and until the window thread ran again)._

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 2401 | 4.16 | 4.38 | 5.24 | 0 | 1 |
| pan 5 s | 1201 | 4.17 | 4.31 | 4.59 | 0 | 1 |
| zoom 5 s | 1201 | 4.17 | 4.28 | 4.48 | 0 | 1 |
| idle 5 s | 0 | 0.00 | 0.00 | 0.00 | 0 | 1 |
| hollow rebuild | 1469 | 4.17 | 4.44 | 5.21 | 0 | 2 |
| launch to first frame | — | 511.76 | — | — | — | — |

### 2026-11-13

_stl-engine has no Fillet. Release engine times on this 504k-triangle mesh: simplify 50% 376 ms; hollow draft 636 ms; hollow normal 1.43 s; hollow fine 4.16 s; remesh edge 2 mm 3.09 s (down to 5k triangles); remesh edge 1 mm 2.35 s. Offset 0.2 mm took about 200–360 s in earlier full runs, and offset 5 mm was still running at 45 s: a 0.2 mm grow only simplifies within 0.08 mm, so the Minkowski sum still sees most of the triangles. This row is Hollow, wall 2 mm, fine quality. Frame times are the frames that kept orbiting until the new mesh was uploaded and drawn. Rayon's pool keeps two physical cores free when the chip has hyperthreads, one physical core free without them, and two processors free where the layout can't be read. On Windows the window thread runs above normal priority and is not pinned. The engine thread and its Rayon workers run below the window. Other threads stay at normal priority._

_Idle frames are paints something other than this bench asked for. Not counted: the frame already queued by the last zoom, egui's extra settle frame after an immediate repaint, and the wake this bench schedules so it can leave the idle window. Toasts are dismissed and any camera flight is finished as idle starts._

_This run: hollow took 6494 ms from the command until the new mesh was uploaded and drawn. Frame times on that row are the orbiting frames in between._

_Slowest rebuild frame: 6.49 ms. The frame before it spent 0.00 ms on engine updates, 0.33 ms on the rest of the layout, 0.04 ms in the viewport's paint callback (upload and render), 0.97 ms on egui's own paint, and 5.14 ms waiting (the swapchain, the submit, and until the window thread ran again)._

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 2401 | 4.16 | 4.34 | 4.62 | 0 | 1 |
| pan 5 s | 1201 | 4.16 | 4.33 | 4.79 | 0 | 1 |
| zoom 5 s | 1201 | 4.17 | 4.28 | 4.79 | 0 | 1 |
| idle 5 s | 0 | 0.00 | 0.00 | 0.00 | 0 | 1 |
| hollow rebuild | 1562 | 4.16 | 4.44 | 6.49 | 0 | 2 |
| launch to first frame | — | 498.83 | — | — | — | — |

### 2026-11-13 (4b close-out)

_stl-engine has no Fillet. Release engine times on this 504k-triangle mesh: simplify 50% 376 ms; hollow draft 636 ms; hollow normal 1.43 s; hollow fine 4.16 s; remesh edge 2 mm 3.09 s (down to 5k triangles); remesh edge 1 mm 2.35 s. Offset 0.2 mm took about 200–360 s in earlier full runs, and offset 5 mm was still running at 45 s: a 0.2 mm grow only simplifies within 0.08 mm, so the Minkowski sum still sees most of the triangles. This row is Hollow, wall 2 mm, fine quality. Frame times are the frames that kept orbiting until the new mesh was uploaded and drawn. Rayon's pool keeps two physical cores free when the chip has hyperthreads, one physical core free without them, and two processors free where the layout can't be read. On Windows the window thread runs above normal priority and is not pinned. The engine thread and its Rayon workers run below the window. Other threads stay at normal priority._

_Idle frames are paints something other than this bench asked for. Not counted: the frame already queued by the last zoom, egui's extra settle frame after an immediate repaint, and the wake this bench schedules so it can leave the idle window. Toasts are dismissed and any camera flight is finished as idle starts._

_This run: hollow took 6594 ms from the command until the new mesh was uploaded and drawn. Frame times on that row are the orbiting frames in between._

_Slowest rebuild frame: 5.65 ms. The frame before it spent 0.01 ms on engine updates, 0.37 ms on the rest of the layout, 0.10 ms in the viewport's paint callback (upload and render), 0.66 ms on egui's own paint, and 4.51 ms waiting (the swapchain, the submit, and until the window thread ran again)._

_Open tool: 41 tools opened, 21 answered a preview; slowest first previews: thincheck 917 ms, hinge 204 ms, thread 180 ms; longest frames: hinge 6.4 ms (frame 19), hole 6.3 ms (frame 182), thincheck 6.0 ms (frame 179)._

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 2401 | 4.16 | 4.72 | 5.94 | 0 | 1 |
| pan 5 s | 1200 | 4.16 | 4.69 | 7.24 | 0 | 1 |
| zoom 5 s | 1201 | 4.16 | 4.76 | 5.97 | 0 | 1 |
| gizmo drag | 1200 | 4.17 | 4.82 | 5.73 | 0 | 1 |
| open tool | 4322 | 4.17 | 4.95 | 6.35 | 0 | 1 |
| idle 5 s | 1 | 3.82 | 3.82 | 3.82 | 1 | 1 |
| hollow rebuild | 1585 | 4.16 | 4.68 | 5.65 | 0 | 2 |
| launch to first frame | — | 553.88 | — | — | — | — |

### 2026-11-13 (4b close-out)

_stl-engine has no Fillet. Release engine times on this 504k-triangle mesh: simplify 50% 376 ms; hollow draft 636 ms; hollow normal 1.43 s; hollow fine 4.16 s; remesh edge 2 mm 3.09 s (down to 5k triangles); remesh edge 1 mm 2.35 s. Offset 0.2 mm took about 200–360 s in earlier full runs, and offset 5 mm was still running at 45 s: a 0.2 mm grow only simplifies within 0.08 mm, so the Minkowski sum still sees most of the triangles. This row is Hollow, wall 2 mm, fine quality. Frame times are the frames that kept orbiting until the new mesh was uploaded and drawn. Rayon's pool keeps two physical cores free when the chip has hyperthreads, one physical core free without them, and two processors free where the layout can't be read. On Windows the window thread runs above normal priority and is not pinned. The engine thread and its Rayon workers run below the window. Other threads stay at normal priority._

_Idle frames are paints something other than this bench asked for. Not counted: the frame already queued by the last zoom, egui's extra settle frame after an immediate repaint, and the wake this bench schedules so it can leave the idle window. Toasts are dismissed and any camera flight is finished as idle starts._

_This run: hollow took 6580 ms from the command until the new mesh was uploaded and drawn. Frame times on that row are the orbiting frames in between._

_Slowest rebuild frame: 5.86 ms. The frame before it spent 0.01 ms on engine updates, 0.23 ms on the rest of the layout, 0.06 ms in the viewport's paint callback (upload and render), 0.37 ms on egui's own paint, and 5.20 ms waiting (the swapchain, the submit, and until the window thread ran again)._

_Open tool: 41 tools opened, 21 answered a preview; slowest first previews: thincheck 921 ms, hinge 204 ms, thread 175 ms; longest frames: plane_mid 6.5 ms (frame 76), hinge 6.4 ms (frame 19), extrude 6.2 ms (frame 64)._

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 2401 | 4.16 | 4.71 | 9.13 | 0 | 1 |
| pan 5 s | 1201 | 4.17 | 4.89 | 6.31 | 0 | 1 |
| zoom 5 s | 1200 | 4.16 | 4.65 | 5.71 | 0 | 1 |
| gizmo drag | 1201 | 4.16 | 4.69 | 7.35 | 0 | 1 |
| open tool | 4323 | 4.17 | 5.01 | 6.46 | 0 | 1 |
| idle 5 s | 1 | 3.87 | 3.87 | 3.87 | 1 | 1 |
| hollow rebuild | 1582 | 4.16 | 4.74 | 5.86 | 0 | 2 |
| launch to first frame | — | 527.19 | — | — | — | — |

### 2026-11-13 (4b close-out)

_stl-engine has no Fillet. Release engine times on this 504k-triangle mesh: simplify 50% 376 ms; hollow draft 636 ms; hollow normal 1.43 s; hollow fine 4.16 s; remesh edge 2 mm 3.09 s (down to 5k triangles); remesh edge 1 mm 2.35 s. Offset 0.2 mm took about 200–360 s in earlier full runs, and offset 5 mm was still running at 45 s: a 0.2 mm grow only simplifies within 0.08 mm, so the Minkowski sum still sees most of the triangles. This row is Hollow, wall 2 mm, fine quality. Frame times are the frames that kept orbiting until the new mesh was uploaded and drawn. Rayon's pool keeps two physical cores free when the chip has hyperthreads, one physical core free without them, and two processors free where the layout can't be read. On Windows the window thread runs above normal priority and is not pinned. The engine thread and its Rayon workers run below the window. Other threads stay at normal priority._

_Idle frames are paints something other than this bench asked for. Not counted: the frame already queued by the last zoom, egui's extra settle frame after an immediate repaint, and the wake this bench schedules so it can leave the idle window. Toasts are dismissed and any camera flight is finished as idle starts._

_This run: hollow took 6664 ms from the command until the new mesh was uploaded and drawn. Frame times on that row are the orbiting frames in between._

_Slowest rebuild frame: 5.95 ms. The frame before it spent 0.01 ms on engine updates, 0.31 ms on the rest of the layout, 0.07 ms in the viewport's paint callback (upload and render), 0.42 ms on egui's own paint, and 5.15 ms waiting (the swapchain, the submit, and until the window thread ran again)._

_Open tool: 41 tools opened, 21 answered a preview; slowest first previews: thincheck 900 ms, hinge 204 ms, thread 183 ms; longest frames: split 8.2 ms (frame 163), hinge 7.1 ms (frame 18), simplify 6.5 ms (frame 178)._

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 2401 | 4.16 | 4.69 | 5.60 | 0 | 1 |
| pan 5 s | 1201 | 4.17 | 4.68 | 5.77 | 0 | 1 |
| zoom 5 s | 1200 | 4.16 | 4.75 | 8.62 | 0 | 1 |
| gizmo drag | 1200 | 4.15 | 4.69 | 5.88 | 0 | 1 |
| open tool | 4317 | 4.16 | 5.01 | 8.23 | 0 | 1 |
| idle 5 s | 1 | 3.30 | 3.30 | 3.30 | 1 | 1 |
| hollow rebuild | 1602 | 4.16 | 4.83 | 5.95 | 0 | 2 |
| launch to first frame | — | 523.65 | — | — | — | — |

### 2026-11-14 (quick)

_stl-engine has no Fillet. Release engine times on this 504k-triangle mesh: simplify 50% 376 ms; hollow draft 636 ms; hollow normal 1.43 s; hollow fine 4.16 s; remesh edge 2 mm 3.09 s (down to 5k triangles); remesh edge 1 mm 2.35 s. Offset 0.2 mm took about 200–360 s in earlier full runs, and offset 5 mm was still running at 45 s: a 0.2 mm grow only simplifies within 0.08 mm, so the Minkowski sum still sees most of the triangles. This row is Hollow, wall 2 mm, fine quality. Frame times are the frames that kept orbiting until the new mesh was uploaded and drawn. Rayon's pool keeps two physical cores free when the chip has hyperthreads, one physical core free without them, and two processors free where the layout can't be read. On Windows the window thread runs above normal priority and is not pinned. The engine thread and its Rayon workers run below the window. Other threads stay at normal priority._

_Idle frames are paints something other than this bench asked for. Not counted: the frame already queued by the last zoom, egui's extra settle frame after an immediate repaint, and the wake this bench schedules so it can leave the idle window. Toasts are dismissed and any camera flight is finished as idle starts._

_This run: hollow took 6179 ms from the command until the new mesh was uploaded and drawn. Frame times on that row are the orbiting frames in between._

_Slowest rebuild frame: 5.81 ms. The frame before it spent 0.01 ms on engine updates, 0.20 ms on the rest of the layout, 0.48 ms in the viewport's paint callback (upload and render), 0.88 ms on egui's own paint, and 4.25 ms waiting (the swapchain, the submit, and until the window thread ran again)._

_Open tool: 6 tools opened, 2 answered a preview; slowest first previews: primitive_box 8 ms, primitive_cylinder 8 ms; longest frames: sweep 5.2 ms (frame 190), extrude 5.1 ms (frame 42), revolve 4.9 ms (frame 41)._

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 241 | 4.16 | 4.48 | 4.54 | 0 | 1 |
| pan 5 s | 240 | 4.17 | 4.40 | 5.25 | 0 | 1 |
| zoom 5 s | 240 | 4.17 | 4.36 | 4.80 | 0 | 1 |
| gizmo drag | 240 | 4.17 | 4.43 | 4.49 | 0 | 1 |
| open tool | 775 | 4.16 | 4.57 | 5.24 | 0 | 1 |
| enter sketch | 237 | 4.15 | 5.95 | 12.21 | 0 | 1 |
| sketch drag | 241 | 4.16 | 4.50 | 5.07 | 0 | 1 |
| idle 5 s | 1 | 0.82 | 0.82 | 0.82 | 1 | 1 |
| hollow rebuild | 1486 | 4.16 | 4.70 | 5.81 | 0 | 2 |
| launch to first frame | — | 755.57 | — | — | — | — |

### 2026-11-14

_stl-engine has no Fillet. Release engine times on this 504k-triangle mesh: simplify 50% 376 ms; hollow draft 636 ms; hollow normal 1.43 s; hollow fine 4.16 s; remesh edge 2 mm 3.09 s (down to 5k triangles); remesh edge 1 mm 2.35 s. Offset 0.2 mm took about 200–360 s in earlier full runs, and offset 5 mm was still running at 45 s: a 0.2 mm grow only simplifies within 0.08 mm, so the Minkowski sum still sees most of the triangles. This row is Hollow, wall 2 mm, fine quality. Frame times are the frames that kept orbiting until the new mesh was uploaded and drawn. Rayon's pool keeps two physical cores free when the chip has hyperthreads, one physical core free without them, and two processors free where the layout can't be read. On Windows the window thread runs above normal priority and is not pinned. The engine thread and its Rayon workers run below the window. Other threads stay at normal priority._

_Idle frames are paints something other than this bench asked for. Not counted: the frame already queued by the last zoom, egui's extra settle frame after an immediate repaint, and the wake this bench schedules so it can leave the idle window. Toasts are dismissed and any camera flight is finished as idle starts._

_This run: hollow took 6482 ms from the command until the new mesh was uploaded and drawn. Frame times on that row are the orbiting frames in between._

_Slowest rebuild frame: 5.15 ms. The frame before it spent 0.00 ms on engine updates, 0.20 ms on the rest of the layout, 0.04 ms in the viewport's paint callback (upload and render), 0.41 ms on egui's own paint, and 4.50 ms waiting (the swapchain, the submit, and until the window thread ran again)._

_Open tool: 41 tools opened, 21 answered a preview; slowest first previews: thincheck 854 ms, hinge 187 ms, thread 163 ms; longest frames: thincheck 6.3 ms (frame 141), thread 5.8 ms (frame 15), separate 5.7 ms (frame 14)._

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 2401 | 4.16 | 4.44 | 5.80 | 0 | 1 |
| pan 5 s | 1201 | 4.16 | 4.47 | 4.83 | 0 | 1 |
| zoom 5 s | 1201 | 4.17 | 4.49 | 5.00 | 0 | 1 |
| gizmo drag | 1201 | 4.17 | 4.44 | 5.28 | 0 | 1 |
| open tool | 4287 | 4.16 | 4.68 | 6.33 | 0 | 1 |
| enter sketch | 718 | 4.16 | 5.65 | 11.22 | 0 | 1 |
| sketch drag | 1201 | 4.16 | 4.98 | 7.63 | 0 | 1 |
| idle 5 s | 1 | 0.83 | 0.83 | 0.83 | 1 | 1 |
| hollow rebuild | 1559 | 4.16 | 4.65 | 5.15 | 0 | 2 |
| launch to first frame | — | 652.05 | — | — | — | — |

### 2026-11-14

_stl-engine has no Fillet. Release engine times on this 504k-triangle mesh: simplify 50% 376 ms; hollow draft 636 ms; hollow normal 1.43 s; hollow fine 4.16 s; remesh edge 2 mm 3.09 s (down to 5k triangles); remesh edge 1 mm 2.35 s. Offset 0.2 mm took about 200–360 s in earlier full runs, and offset 5 mm was still running at 45 s: a 0.2 mm grow only simplifies within 0.08 mm, so the Minkowski sum still sees most of the triangles. This row is Hollow, wall 2 mm, fine quality. Frame times are the frames that kept orbiting until the new mesh was uploaded and drawn. Rayon's pool keeps two physical cores free when the chip has hyperthreads, one physical core free without them, and two processors free where the layout can't be read. On Windows the window thread runs above normal priority and is not pinned. The engine thread and its Rayon workers run below the window. Other threads stay at normal priority._

_Idle frames are paints something other than this bench asked for. Not counted: the frame already queued by the last zoom, egui's extra settle frame after an immediate repaint, and the wake this bench schedules so it can leave the idle window. Toasts are dismissed and any camera flight is finished as idle starts._

_This run: hollow took 6540 ms from the command until the new mesh was uploaded and drawn. Frame times on that row are the orbiting frames in between._

_Slowest rebuild frame: 5.74 ms. The frame before it spent 0.01 ms on engine updates, 0.35 ms on the rest of the layout, 0.07 ms in the viewport's paint callback (upload and render), 0.86 ms on egui's own paint, and 4.45 ms waiting (the swapchain, the submit, and until the window thread ran again)._

_Open tool: 41 tools opened, 21 answered a preview; slowest first previews: thincheck 875 ms, hinge 187 ms, thread 158 ms; longest frames: hinge 6.1 ms (frame 21), thincheck 5.7 ms (frame 95), remesh 5.6 ms (frame 38)._

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 2401 | 4.16 | 4.43 | 5.33 | 0 | 1 |
| pan 5 s | 1201 | 4.17 | 4.43 | 5.44 | 0 | 1 |
| zoom 5 s | 1201 | 4.17 | 4.48 | 5.39 | 0 | 1 |
| gizmo drag | 1200 | 4.17 | 4.40 | 4.90 | 0 | 1 |
| open tool | 4292 | 4.16 | 4.74 | 6.13 | 0 | 1 |
| enter sketch | 718 | 4.16 | 5.28 | 11.58 | 0 | 1 |
| sketch drag | 1201 | 4.16 | 4.68 | 5.87 | 0 | 1 |
| idle 5 s | 1 | 0.77 | 0.77 | 0.77 | 1 | 1 |
| hollow rebuild | 1573 | 4.16 | 4.62 | 5.74 | 0 | 2 |
| launch to first frame | — | 596.88 | — | — | — | — |

### 2026-11-14

_stl-engine has no Fillet. Release engine times on this 504k-triangle mesh: simplify 50% 376 ms; hollow draft 636 ms; hollow normal 1.43 s; hollow fine 4.16 s; remesh edge 2 mm 3.09 s (down to 5k triangles); remesh edge 1 mm 2.35 s. Offset 0.2 mm took about 200–360 s in earlier full runs, and offset 5 mm was still running at 45 s: a 0.2 mm grow only simplifies within 0.08 mm, so the Minkowski sum still sees most of the triangles. This row is Hollow, wall 2 mm, fine quality. Frame times are the frames that kept orbiting until the new mesh was uploaded and drawn. Rayon's pool keeps two physical cores free when the chip has hyperthreads, one physical core free without them, and two processors free where the layout can't be read. On Windows the window thread runs above normal priority and is not pinned. The engine thread and its Rayon workers run below the window. Other threads stay at normal priority._

_Idle frames are paints something other than this bench asked for. Not counted: the frame already queued by the last zoom, egui's extra settle frame after an immediate repaint, and the wake this bench schedules so it can leave the idle window. Toasts are dismissed and any camera flight is finished as idle starts._

_This run: hollow took 6716 ms from the command until the new mesh was uploaded and drawn. Frame times on that row are the orbiting frames in between._

_Slowest rebuild frame: 5.73 ms. The frame before it spent 0.01 ms on engine updates, 0.22 ms on the rest of the layout, 0.04 ms in the viewport's paint callback (upload and render), 0.91 ms on egui's own paint, and 4.56 ms waiting (the swapchain, the submit, and until the window thread ran again)._

_Open tool: 41 tools opened, 21 answered a preview; slowest first previews: thincheck 846 ms, hinge 188 ms, thread 162 ms; longest frames: sculpt 5.8 ms (frame 1), thincheck 5.8 ms (frame 55), loft 5.4 ms (frame 3)._

| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| orbit 10 s | 2401 | 4.17 | 4.48 | 5.75 | 0 | 1 |
| pan 5 s | 1201 | 4.17 | 4.55 | 5.63 | 0 | 1 |
| zoom 5 s | 1201 | 4.17 | 4.36 | 4.89 | 0 | 1 |
| gizmo drag | 1201 | 4.16 | 4.43 | 4.72 | 0 | 1 |
| open tool | 4290 | 4.16 | 4.66 | 5.76 | 0 | 1 |
| enter sketch | 717 | 4.15 | 5.19 | 10.99 | 0 | 1 |
| sketch drag | 1201 | 4.15 | 4.76 | 6.29 | 0 | 1 |
| idle 5 s | 1 | 0.83 | 0.83 | 0.83 | 1 | 1 |
| hollow rebuild | 1615 | 4.16 | 4.63 | 5.73 | 0 | 2 |
| launch to first frame | — | 585.04 | — | — | — | — |
