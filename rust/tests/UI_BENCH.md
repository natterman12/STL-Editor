# UI bench (`xtask uibench`)

Results from `cargo run --release -p xtask -- uibench` on Nathan's PC. Each run appends a dated table. No results yet.

## Rows

| Scenario | What it drives |
| --- | --- |
| orbit 10 s | Left-drag orbit on the 504k-triangle bench mesh for ten seconds |
| pan 5 s | Middle/right-drag pan for five seconds |
| zoom 5 s | Cursor-centred zoom for five seconds |
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
