# Vendored egui-wgpu

A copy of [egui-wgpu](https://github.com/emilk/egui/tree/main/crates/egui-wgpu) 0.37.0 (MIT OR
Apache-2.0, by Emil Ernerfeldt and the egui contributors), as published on crates.io: upstream
commit `7c0b40402271e8d40cda69f87cf3c2617c2e4382`, path `crates/egui-wgpu`. It is used through
`[patch.crates-io]` in `rust/Cargo.toml`, so eframe and `stl-app` (the only crates that use
egui-wgpu) build this one copy (`cargo tree -i egui-wgpu` shows the path).

Only `src/`, `Cargo.toml` (as published) and `README.md` are copied. The published crate carries no
licence file (its `include` points at `../../LICENSE-*` of the egui repository); the licence is
the one its `Cargo.toml` states, the same as the crates.io dependency it replaces.

Milestone 7 (2026-11-15), spec `dev/specs/2026-11-15-stl-editor-2-m7-testing-fixes-design.md`
section 2.1. Drop this copy (remove the `[patch.crates-io]` line) once egui-wgpu stops panicking
on a refused staging write.

## Changes from upstream

One change, in two files. Every changed spot is marked `STL Editor patch`.

| File | Change | Why |
|---|---|---|
| `src/renderer.rs` | `Renderer::update_buffers`: when `queue.write_buffer_with` returns `None` for the index or the vertex data, log the same text with `log::error!` instead of `panic!`, clear both slice lists, set a new flag and return no command buffers (no paint callback is prepared). New `Renderer::upload_refused()` reads the flag; `update_buffers` clears it first, `Renderer::render` draws nothing while it is set. | wgpu returns `None` there quietly only for a device that is lost (an out-of-memory or validation error would have gone to wgpu's error handler first). Upstream then panics inside winit's window procedure, which Windows can't unwind through, so the process ended (0xc000041d) and STL Editor's restart after a lost device never ran. |
| `src/winit.rs` | `Painter::paint_and_update_textures`: after `update_buffers`, if `upload_refused()`, free the textures egui asked to free, ask for another frame of that viewport and return, without configuring or acquiring the surface, encoding a pass, submitting or presenting. | Nothing more is done with a lost device (anything but a device-lost error would reach wgpu's default error handler, which panics too), and an idle window still gets the next frame in which the app notices the loss. |

How STL Editor reads it: `Viewport::device_lost` (`crates/stl-app/src/viewport/mod.rs`) counts
`upload_refused()` as a lost device, so `StlApp::device_frame` logs the loss to `log.txt`, closes
the window and takes milestone 6's restart path (autosave, message, restart). The probe
`render_state_refuses_uploads` is unchanged and still used after a panic.

Tests: `crates/stl-app/tests/device_lost_window.rs` (`refused_staging_write_does_not_panic`
paints through `Renderer::update_buffers` and `Renderer::render` on a destroyed device with no
`catch_unwind` around it).

## Checking it against upstream

```
diff -ru ~/.cargo/registry/src/index.crates.io-*/egui-wgpu-0.37.0/src rust/third_party/egui-wgpu/src
```

shows only the two changes above.
